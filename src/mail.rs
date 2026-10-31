//! IMAP access (reading) and SMTP (sending). All functions are blocking:
//! the UI calls them from `backend`, inside `spawn_blocking`.

use crate::config::Account;
use crate::i18n::{t, tf};
use anyhow::{Context, Result, anyhow};
use mail_parser::{AddressList, MessageParser};
use std::collections::HashMap;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

type TlsStream = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;
pub type ImapSession = imap::Session<TlsStream>;

/// How many messages (the most recent) to load per folder.
pub const PAGE_SIZE: u32 = 200;
/// Bytes of text downloaded for the list preview.
const PREVIEW_BYTES: u32 = 2048;

// ---------------------------------------------------------------- folders

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderKind {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Spam,
    Trash,
    Other,
}

impl FolderKind {
    pub fn label(self) -> &'static str {
        match self {
            FolderKind::Inbox => t("folder_inbox"),
            FolderKind::Sent => t("folder_sent"),
            FolderKind::Drafts => t("folder_drafts"),
            FolderKind::Archive => t("folder_archive"),
            FolderKind::Spam => t("folder_spam"),
            FolderKind::Trash => t("folder_trash"),
            FolderKind::Other => "",
        }
    }

    /// Order in the sidebar.
    pub const SPECIAL: [FolderKind; 6] = [
        FolderKind::Inbox,
        FolderKind::Sent,
        FolderKind::Drafts,
        FolderKind::Archive,
        FolderKind::Spam,
        FolderKind::Trash,
    ];
}

#[derive(Debug, Clone)]
pub struct Folder {
    /// Raw IMAP name (to use in commands).
    pub name: String,
    /// Path segments, decoded for display.
    pub path: Vec<String>,
    pub delimiter: Option<String>,
    pub kind: FolderKind,
    pub selectable: bool,
}

impl Folder {
    pub fn display(&self) -> String {
        match self.kind {
            FolderKind::Other => self.path.last().cloned().unwrap_or_else(|| self.name.clone()),
            k => k.label().to_string(),
        }
    }
}

/// Decodes the "modified UTF-7" IMAP uses for folder names (RFC 3501 §5.1.3).
pub fn decode_utf7_imap(s: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";
    let mut out = String::new();
    let mut chars = s.split('&');
    if let Some(first) = chars.next() {
        out.push_str(first);
    }
    for chunk in chars {
        let Some(end) = chunk.find('-') else {
            out.push('&');
            out.push_str(chunk);
            continue;
        };
        let (encoded, rest) = chunk.split_at(end);
        let rest = &rest[1..];
        if encoded.is_empty() {
            out.push('&');
        } else {
            let mut bits: u32 = 0;
            let mut nbits = 0;
            let mut units = Vec::new();
            for b in encoded.bytes() {
                let Some(v) = ALPHABET.iter().position(|&c| c == b) else {
                    continue;
                };
                bits = (bits << 6) | v as u32;
                nbits += 6;
                if nbits >= 16 {
                    nbits -= 16;
                    units.push(((bits >> nbits) & 0xFFFF) as u16);
                }
            }
            out.push_str(&String::from_utf16_lossy(&units));
        }
        out.push_str(rest);
    }
    out
}

fn classify(name: &str, segments: &[String], attrs: &[imap::types::NameAttribute]) -> FolderKind {
    if name.eq_ignore_ascii_case("INBOX") {
        return FolderKind::Inbox;
    }
    // RFC 6154 (SPECIAL-USE), when the server advertises it.
    for a in attrs {
        if let imap::types::NameAttribute::Custom(c) = a {
            match c.to_ascii_lowercase().as_str() {
                "\\sent" => return FolderKind::Sent,
                "\\drafts" => return FolderKind::Drafts,
                "\\trash" => return FolderKind::Trash,
                "\\junk" => return FolderKind::Spam,
                "\\archive" => return FolderKind::Archive,
                _ => {}
            }
        }
    }
    // Otherwise by name, top level only (or under INBOX).
    let top_level = segments.len() == 1
        || (segments.len() == 2 && segments[0].eq_ignore_ascii_case("INBOX"));
    if !top_level {
        return FolderKind::Other;
    }
    match segments.last().map(|s| s.to_lowercase()).as_deref() {
        Some("sent" | "sent items" | "sent messages" | "inviati" | "inviata" | "posta inviata") => {
            FolderKind::Sent
        }
        Some("drafts" | "bozze") => FolderKind::Drafts,
        Some("trash" | "cestino" | "deleted items" | "deleted messages" | "posta eliminata") => {
            FolderKind::Trash
        }
        Some("spam" | "junk" | "junk e-mail" | "posta indesiderata") => FolderKind::Spam,
        Some("archive" | "archivio" | "archives") => FolderKind::Archive,
        _ => FolderKind::Other,
    }
}

// ---------------------------------------------------------------- message types

#[derive(Debug, Clone)]
pub struct Header {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub preview: String,
    pub timestamp: i64,
    pub seen: bool,
    pub has_attachment: bool,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FullMessage {
    pub uid: u32,
    pub from: String,
    pub from_address: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub timestamp: i64,
    pub message_id: Option<String>,
    pub references: Vec<String>,
    pub body: String,
    pub attachments: Vec<Attachment>,
    /// DKIM result reported by the server (Authentication-Results header).
    pub dkim_pass: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<PathBuf>,
}

// ---------------------------------------------------------------- connection

fn tls_config() -> Arc<rustls::ClientConfig> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("supported TLS versions")
    .with_root_certificates(roots)
    .with_no_client_auth();
    Arc::new(config)
}

pub fn connect(account: &Account) -> Result<ImapSession> {
    let addr = (account.imap_host.as_str(), account.imap_port);
    let tcp = TcpStream::connect(addr)
        .with_context(|| {
            tf(
                "err_connect",
                &[("host", &account.imap_host), ("port", &account.imap_port.to_string())],
            )
        })?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(60)))?;

    let server_name = rustls::pki_types::ServerName::try_from(account.imap_host.clone())
        .context("invalid IMAP host name")?;
    let conn = rustls::ClientConnection::new(tls_config(), server_name)?;
    let tls = rustls::StreamOwned::new(conn, tcp);

    let mut client = imap::Client::new(tls);
    client.read_greeting().context("IMAP greeting")?;
    client
        .login(&account.username, &account.password)
        .map_err(|(e, _)| {
            let msg = e.to_string();
            if msg.to_uppercase().contains("AUTHENTICATIONFAILED") {
                anyhow!(tf("err_auth", &[("msg", &msg)]))
            } else {
                anyhow!(tf("err_login", &[("msg", &msg)]))
            }
        })
}

pub fn list_folders(session: &mut ImapSession) -> Result<Vec<Folder>> {
    let names = session.list(Some(""), Some("*"))?;
    let mut folders: Vec<Folder> = names
        .iter()
        .map(|n| {
            let delimiter = n.delimiter().map(str::to_string);
            let decoded = decode_utf7_imap(n.name());
            let path: Vec<String> = match &delimiter {
                Some(d) if !d.is_empty() => decoded.split(d.as_str()).map(str::to_string).collect(),
                _ => vec![decoded],
            };
            let kind = classify(n.name(), &path, n.attributes());
            Folder {
                name: n.name().to_string(),
                selectable: !n.attributes().contains(&imap::types::NameAttribute::NoSelect),
                path,
                delimiter,
                kind,
            }
        })
        .collect();
    // One folder per special kind: the others go back to "normal".
    let mut seen = std::collections::HashSet::new();
    for f in folders.iter_mut() {
        if f.kind != FolderKind::Other && !seen.insert(f.kind) {
            f.kind = FolderKind::Other;
        }
    }
    folders.sort_by_key(|f| f.path.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>());
    Ok(folders)
}

/// Unread messages for each requested folder (STATUS command).
pub fn unseen_counts(session: &mut ImapSession, folders: &[String]) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for f in folders {
        if session.status(f, "(UNSEEN)").is_err() {
            continue;
        }
        // imap 2.4 delivers the STATUS response among the unsolicited responses.
        while let Ok(resp) = session.unsolicited_responses.try_recv() {
            if let imap::types::UnsolicitedResponse::Status { attributes, .. } = resp {
                for a in attributes {
                    if let imap::types::StatusAttribute::Unseen(n) = a {
                        out.insert(f.clone(), n);
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- reading

fn first_addr(addr: Option<AddressList>) -> (String, String) {
    let Some(first) = addr.and_then(|a| a.first()) else {
        return (String::new(), String::new());
    };
    let email = first.address().unwrap_or_default().to_string();
    let shown = match first.name() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => email.clone(),
    };
    (shown, email)
}

fn addr_list(addr: Option<AddressList>) -> String {
    let Some(addr) = addr else {
        return String::new();
    };
    addr.mailboxes()
        .map(|a| match (a.name(), a.address()) {
            (Some(n), Some(e)) if !n.is_empty() => format!("{n} <{e}>"),
            (_, Some(e)) => e.to_string(),
            (Some(n), None) => n.to_string(),
            _ => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn one_line(s: &str, max: usize) -> String {
    let mut out = String::new();
    for word in s.split_whitespace() {
        if word.starts_with('>') {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() >= max {
            break;
        }
    }
    out.chars().take(max).collect()
}

/// Selects the folder and downloads headers and previews of the most recent messages.
pub fn fetch_headers(session: &mut ImapSession, folder: &str) -> Result<Vec<Header>> {
    let mailbox = session.select(folder)?;
    if mailbox.exists == 0 {
        return Ok(Vec::new());
    }
    let start = mailbox.exists.saturating_sub(PAGE_SIZE - 1).max(1);
    let range = format!("{start}:{}", mailbox.exists);
    let query = format!("(UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.{PREVIEW_BYTES}>)");
    let fetches = session.fetch(range, query)?;

    let parser = MessageParser::default();
    let mut headers: Vec<Header> = fetches
        .iter()
        .filter_map(|f| {
            let uid = f.uid?;
            let head = f.header().unwrap_or_default();
            // Headers + start of the body: enough for mail-parser to build the preview.
            let mut raw = head.to_vec();
            raw.extend_from_slice(f.text().unwrap_or_default());
            let msg = parser.parse(&raw[..]);
            let msg = msg.as_ref();
            let (from, _) = first_addr(msg.and_then(|m| m.from()));
            let content_type = msg
                .and_then(|m| m.content_type())
                .map(|ct| format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or_default()))
                .unwrap_or_default()
                .to_lowercase();
            let has_attachment = content_type == "multipart/mixed"
                || msg.map(|m| m.attachments().len() > 0).unwrap_or(false);
            Some(Header {
                uid,
                from,
                subject: msg
                    .and_then(|m| m.subject())
                    .unwrap_or(t("no_subject"))
                    .to_string(),
                preview: msg
                    .and_then(|m| m.body_text(0))
                    .map(|t| one_line(&t, 140))
                    .unwrap_or_default(),
                timestamp: msg.and_then(|m| m.date()).map(|d| d.to_timestamp()).unwrap_or(0),
                seen: f.flags().contains(&imap::types::Flag::Seen),
                has_attachment,
            })
        })
        .collect();
    headers.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.uid.cmp(&a.uid)));
    Ok(headers)
}

/// Downloads the full message (marks it as read on the server).
pub fn fetch_message(session: &mut ImapSession, folder: &str, uid: u32) -> Result<FullMessage> {
    session.select(folder)?;
    let fetches = session.uid_fetch(uid.to_string(), "BODY[]")?;
    let raw = fetches
        .iter()
        .find_map(|f| f.body())
        .context(t("err_not_found"))?;
    let msg = MessageParser::default()
        .parse(raw)
        .context(t("err_unreadable"))?;

    let (from, from_address) = first_addr(msg.from());
    let body = msg
        .body_text(0)
        .map(|b| b.into_owned())
        .unwrap_or_else(|| t("no_text").into());
    let attachments = msg
        .attachments()
        .enumerate()
        .map(|(i, part)| Attachment {
            name: part
                .attachment_name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("attachment-{}", i + 1)),
            data: part.decoded().into_owned(),
        })
        .collect();
    let dkim_pass = raw_header_values(raw, "authentication-results")
        .iter()
        .any(|v| v.to_lowercase().contains("dkim=pass"));
    let references = raw_header_values(raw, "references")
        .iter()
        .flat_map(|v| v.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .collect();

    Ok(FullMessage {
        uid,
        from,
        from_address,
        to: addr_list(msg.to()),
        cc: addr_list(msg.cc()),
        subject: msg.subject().unwrap_or(t("no_subject")).to_string(),
        timestamp: msg.date().map(|d| d.to_timestamp()).unwrap_or(0),
        message_id: msg.message_id().map(str::to_string),
        references,
        body,
        attachments,
        dkim_pass,
    })
}

/// Raw values of a header (continuation lines joined).
fn raw_header_values(raw: &[u8], name: &str) -> Vec<String> {
    let text = String::from_utf8_lossy(raw);
    let head = text.split("\r\n\r\n").next().unwrap_or_default();
    let head = head.split("\n\n").next().unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in head.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(c) = current.as_mut() {
                c.push(' ');
                c.push_str(line.trim());
            }
            continue;
        }
        if let Some(c) = current.take() {
            out.push(c);
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case(name) {
                current = Some(v.trim().to_string());
            }
        }
    }
    if let Some(c) = current {
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------- changes

/// Moves a message to another folder (MOVE if available, otherwise COPY + delete).
pub fn move_message(session: &mut ImapSession, from: &str, uid: u32, to: &str) -> Result<()> {
    session.select(from)?;
    let has_move = session
        .capabilities()
        .map(|c| c.has_str("MOVE"))
        .unwrap_or(false);
    if has_move {
        session.uid_mv(uid.to_string(), to)?;
    } else {
        session.uid_copy(uid.to_string(), to)?;
        session.uid_store(uid.to_string(), "+FLAGS (\\Deleted)")?;
        session.expunge()?;
    }
    Ok(())
}

/// Deletes permanently (used only inside Trash).
pub fn delete_permanently(session: &mut ImapSession, folder: &str, uid: u32) -> Result<()> {
    session.select(folder)?;
    session.uid_store(uid.to_string(), "+FLAGS (\\Deleted)")?;
    session.expunge()?;
    Ok(())
}

pub fn set_seen(session: &mut ImapSession, folder: &str, uid: u32, seen: bool) -> Result<()> {
    session.select(folder)?;
    let op = if seen { "+FLAGS (\\Seen)" } else { "-FLAGS (\\Seen)" };
    session.uid_store(uid.to_string(), op)?;
    Ok(())
}

/// Creates a special folder (e.g. Archive) next to the inbox.
pub fn create_folder(session: &mut ImapSession, like: &[Folder], leaf: &str) -> Result<String> {
    // If folders live under INBOX (e.g. Aruba: INBOX.Sent), create INBOX.<leaf>.
    let nested = like
        .iter()
        .find(|f| f.kind != FolderKind::Inbox && f.kind != FolderKind::Other)
        .filter(|f| f.path.len() == 2 && f.path[0].eq_ignore_ascii_case("INBOX"));
    let name = match nested.and_then(|f| f.delimiter.clone()) {
        Some(d) => format!("INBOX{d}{leaf}"),
        None => leaf.to_string(),
    };
    session.create(&name)?;
    Ok(name)
}

// ---------------------------------------------------------------- sending

/// Sends via SMTP and returns the formatted message (to save it in Sent).
pub fn send(account: &Account, mail: &Outgoing) -> Result<Vec<u8>> {
    use lettre::message::{Attachment as LAttachment, Mailbox, MultiPart, SinglePart, header};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let from: Mailbox = format!("{} <{}>", account.display_name, account.email)
        .parse()
        .or_else(|_| account.email.parse())
        .context(t("err_bad_sender"))?;

    let mut builder = Message::builder().from(from).subject(&mail.subject);
    let split = |s: &str| {
        s.split([',', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let to = split(&mail.to);
    if to.is_empty() {
        return Err(anyhow!(t("err_no_recipient")));
    }
    for a in to {
        builder = builder.to(a.parse().with_context(|| tf("err_bad_address", &[("addr", &a)]))?);
    }
    for a in split(&mail.cc) {
        builder = builder.cc(a.parse().with_context(|| tf("err_bad_address", &[("addr", &a)]))?);
    }
    for a in split(&mail.bcc) {
        builder = builder.bcc(a.parse().with_context(|| tf("err_bad_address", &[("addr", &a)]))?);
    }
    if let Some(id) = &mail.in_reply_to {
        let id = format!("<{}>", id.trim_matches(['<', '>']));
        builder = builder.in_reply_to(id.clone());
        let mut refs = mail.references.clone();
        if !refs.contains(&id) {
            refs.push(id);
        }
        builder = builder.references(refs.join(" "));
    }

    let text = SinglePart::builder()
        .header(header::ContentType::TEXT_PLAIN)
        .body(mail.body.clone());
    let email = if mail.attachments.is_empty() {
        builder.singlepart(text)?
    } else {
        let mut multi = MultiPart::mixed().singlepart(text);
        for path in &mail.attachments {
            let data = std::fs::read(path)
                .with_context(|| tf("err_attach_read", &[("path", &path.display().to_string())]))?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "attachment".into());
            let ct = header::ContentType::parse(guess_mime(&name))
                .unwrap_or(header::ContentType::parse("application/octet-stream").unwrap());
            multi = multi.singlepart(LAttachment::new(name).body(data, ct));
        }
        builder.multipart(multi)?
    };

    let relay = if account.smtp_port == 465 {
        SmtpTransport::relay(&account.smtp_host)?
    } else {
        SmtpTransport::starttls_relay(&account.smtp_host)?
    };
    let mailer = relay
        .port(account.smtp_port)
        .credentials(Credentials::new(
            account.username.clone(),
            account.password.clone(),
        ))
        .timeout(Some(Duration::from_secs(60)))
        .build();
    mailer.send(&email).context(t("err_smtp"))?;
    Ok(email.formatted())
}

/// Saves a copy of the sent message to the Sent folder.
pub fn append_sent(session: &mut ImapSession, folder: &str, raw: &[u8]) -> Result<()> {
    session.append_with_flags(folder, raw, &[imap::types::Flag::Seen])?;
    Ok(())
}

fn guess_mime(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xml" => "application/xml",
        "ics" => "text/calendar",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf7_imap() {
        assert_eq!(decode_utf7_imap("INBOX"), "INBOX");
        assert_eq!(decode_utf7_imap("Posta &AOg- qui"), "Posta è qui");
        assert_eq!(decode_utf7_imap("A&-B"), "A&B");
        assert_eq!(decode_utf7_imap("&ZeVnLIqe-"), "日本語");
    }

    #[test]
    fn classify_aruba_style() {
        let seg = |s: &str| s.split('.').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(classify("INBOX.Sent", &seg("INBOX.Sent"), &[]), FolderKind::Sent);
        assert_eq!(classify("INBOX.Trash", &seg("INBOX.Trash"), &[]), FolderKind::Trash);
        assert_eq!(
            classify("INBOX.Sent.2004.04-apr", &seg("INBOX.Sent.2004.04-apr"), &[]),
            FolderKind::Other
        );
        assert_eq!(classify("Junk", &seg("Junk"), &[]), FolderKind::Spam);
    }

    #[test]
    fn header_values() {
        let raw = b"Subject: x\r\nAuthentication-Results: mx.aruba.it;\r\n dkim=pass header.d=x\r\n\r\nbody";
        let v = raw_header_values(raw, "authentication-results");
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("dkim=pass"));
    }

    /// Requires network: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn tls_handshake_and_login_rejected() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        crate::i18n::set(Some("en"));
        let acc = Account {
            username: "nonexistent-user@gmail.com".into(),
            password: "wrong".into(),
            imap_host: "imap.gmail.com".into(),
            ..Default::default()
        };
        let err = connect(&acc).err().expect("login must fail");
        assert!(format!("{err:#}").contains("IMAP login failed"), "{err:#}");
    }

    /// Uses the configured account: `cargo test live_account -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_account() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let acc = crate::config::Config::load().account.expect("no account");
        let mut s = connect(&acc).unwrap_or_else(|e| panic!("IMAP ERROR: {e:#}"));
        let folders = list_folders(&mut s).unwrap();
        for f in folders.iter().filter(|f| f.kind != FolderKind::Other) {
            println!("special: {:?} -> {}", f.kind, f.name);
        }
        let names: Vec<String> = folders
            .iter()
            .filter(|f| f.kind != FolderKind::Other)
            .map(|f| f.name.clone())
            .collect();
        println!("unread: {:?}", unseen_counts(&mut s, &names));
        let h = fetch_headers(&mut s, "INBOX").unwrap();
        println!("INBOX: {} messages", h.len());
        for m in h.iter().take(3) {
            println!(
                "- [{}{}] {} | {} | {}",
                if m.seen { " " } else { "●" },
                if m.has_attachment { "📎" } else { " " },
                m.from,
                m.subject,
                m.preview
            );
        }
    }
}
