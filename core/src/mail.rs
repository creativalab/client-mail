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

/// TLS over TCP. A newtype so it can implement imap's `SetReadTimeout`
/// (needed by IDLE, which waits with a timeout).
pub struct TlsStream(rustls::StreamOwned<rustls::ClientConnection, TcpStream>);

impl std::io::Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl std::io::Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl imap::extensions::idle::SetReadTimeout for TlsStream {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::Result<()> {
        self.0.sock.set_read_timeout(timeout).map_err(imap::Error::Io)
    }
}
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
    let top_level = segments.len() == 1 || (segments.len() == 2 && segments[0].eq_ignore_ascii_case("INBOX"));
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
    /// Start of the body as text (for the search index; empty from the cache).
    pub text: String,
}

/// A slice of a folder's message list, by sequence number.
pub struct HeaderPage {
    pub uidvalidity: Option<u32>,
    /// Messages in the folder.
    pub exists: u32,
    /// Sequence number of the oldest message in this page (1 = none older).
    pub first_seq: u32,
    pub headers: Vec<Header>,
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
    /// Bare addresses of To and Cc (for "Reply all").
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: String,
    pub timestamp: i64,
    pub message_id: Option<String>,
    pub references: Vec<String>,
    pub body: String,
    /// HTML body as sent (only when the message has an HTML part).
    pub html: Option<String>,
    /// Inline parts referenced as `cid:` from the HTML, by Content-ID.
    pub inline: HashMap<String, Vec<u8>>,
    pub attachments: Vec<Attachment>,
    /// DKIM result reported by the server (Authentication-Results header).
    pub dkim_pass: bool,
    /// OpenPGP status, if the message is encrypted or signed.
    pub pgp: Option<PgpStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigState {
    /// Signed with the key we hold for the sender.
    Valid,
    /// Signature does not match the content or the sender's key.
    Invalid,
    /// Signed, but we have no key for the sender.
    UnknownKey,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PgpStatus {
    pub encrypted: bool,
    pub signature: Option<SigState>,
    /// Decryption failed (reason).
    pub error: Option<String>,
}

/// Keys used to open a message: our secret key (+ passphrase) and a lookup
/// of correspondents' public keys.
pub struct PgpKeys<'a> {
    pub own: Option<(pgp::composed::SignedSecretKey, String)>,
    pub contact: &'a dyn Fn(&str) -> Option<pgp::composed::SignedPublicKey>,
}

/// Unwraps PGP/MIME (RFC 3156). Returns the entity to display instead of
/// the raw message (decrypted content, or the signed part without the
/// signature) and the status; `(None, None)` for ordinary mail.
pub fn unwrap_pgp(raw: &[u8], from: &str, keys: &PgpKeys) -> (Option<Vec<u8>>, Option<PgpStatus>) {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return (None, None);
    };
    let Some(ct) = msg.content_type() else {
        return (None, None);
    };
    let sub = ct.subtype().unwrap_or_default().to_ascii_lowercase();
    if !ct.ctype().eq_ignore_ascii_case("multipart") {
        return (None, None);
    }
    match sub.as_str() {
        "encrypted" => {
            let mut status = PgpStatus {
                encrypted: true,
                ..Default::default()
            };
            let data = msg.parts().find(|p| {
                p.content_type().is_some_and(|c| {
                    c.ctype().eq_ignore_ascii_case("application")
                        && c.subtype()
                            .is_some_and(|s| s.eq_ignore_ascii_case("octet-stream"))
                })
            });
            let Some(data) = data.map(|p| p.decoded().into_owned()) else {
                status.error = Some(t("pgp_malformed").into());
                return (None, Some(status));
            };
            let Some((key, pass)) = &keys.own else {
                status.error = Some(t("err_pgp_no_own_key").into());
                return (None, Some(status));
            };
            match crate::crypto::decrypt(&data, key, pass) {
                Ok(plain) => {
                    // Signed then encrypted: check the inner signature.
                    let (inner, inner_status) = unwrap_pgp(&plain, from, keys);
                    status.signature = inner_status.and_then(|s| s.signature);
                    (Some(inner.unwrap_or(plain)), Some(status))
                }
                Err(e) => {
                    status.error = Some(format!("{e:#}"));
                    (None, Some(status))
                }
            }
        }
        "signed" => {
            // First child: exactly the signed bytes (mail-parser's raw part
            // excludes the line break before the delimiter, as RFC 3156 wants).
            let mut parts = msg.parts().skip(1);
            let Some(signed) = parts.next().map(|p| p.raw().to_vec()) else {
                return (None, None);
            };
            let signature = msg.parts().find(|p| {
                p.content_type().is_some_and(|c| {
                    c.subtype()
                        .is_some_and(|s| s.eq_ignore_ascii_case("pgp-signature"))
                })
            });
            let Some(signature) = signature.map(|p| p.decoded().into_owned()) else {
                return (None, None);
            };
            let canonical = crlf(&signed);
            let state = match (keys.contact)(&from.to_lowercase()) {
                Some(key) if crate::crypto::verify(&canonical, &signature, &key) => SigState::Valid,
                Some(_) => SigState::Invalid,
                None => SigState::UnknownKey,
            };
            (
                Some(signed),
                Some(PgpStatus {
                    signature: Some(state),
                    ..Default::default()
                }),
            )
        }
        _ => (None, None),
    }
}

/// Normalizes line endings to CRLF (signatures are over canonical text).
fn crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 40);
    let mut prev = 0u8;
    for &b in data {
        if b == b'\n' && prev != b'\r' {
            out.push(b'\r');
        }
        out.push(b);
        prev = b;
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub body: String,
    /// HTML alternative of `body` (sent as multipart/alternative).
    pub html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<PathBuf>,
    /// OpenPGP: detached signature (PGP/MIME).
    pub sign: bool,
    /// OpenPGP: encrypt to all recipients (and to ourselves, for Sent).
    pub encrypt: bool,
}

// ---------------------------------------------------------------- connection

pub(crate) fn tls_config() -> Arc<rustls::ClientConfig> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config =
        rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("supported TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
    Arc::new(config)
}

pub fn connect(account: &Account) -> Result<ImapSession> {
    let addr = (account.imap_host.as_str(), account.imap_port);
    let tcp = TcpStream::connect(addr).with_context(|| {
        tf(
            "err_connect",
            &[
                ("host", &account.imap_host),
                ("port", &account.imap_port.to_string()),
            ],
        )
    })?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(60)))?;

    let server_name = rustls::pki_types::ServerName::try_from(account.imap_host.clone())
        .context("invalid IMAP host name")?;
    let conn = rustls::ClientConnection::new(tls_config(), server_name)?;
    let tls = TlsStream(rustls::StreamOwned::new(conn, tcp));

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

/// Whether the server supports IMAP IDLE (push).
pub fn supports_idle(session: &mut ImapSession) -> Result<bool> {
    Ok(session.capabilities()?.has_str("IDLE"))
}

/// Waits (IDLE) until `folder` changes or `timeout` passes.
/// Returns `true` if the server reported a change.
pub fn idle_wait(session: &mut ImapSession, folder: &str, timeout: Duration) -> Result<bool> {
    session.select(folder)?;
    let outcome = session.idle()?.wait_with_timeout(timeout)?;
    Ok(matches!(
        outcome,
        imap::extensions::idle::WaitOutcome::MailboxChanged
    ))
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

fn bare_list(addr: Option<AddressList>) -> Vec<String> {
    addr.map(|a| {
        a.mailboxes()
            .filter_map(|m| m.address().map(str::to_string))
            .collect()
    })
    .unwrap_or_default()
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
/// The most recent `PAGE_SIZE` messages of a folder.
pub fn fetch_headers(session: &mut ImapSession, folder: &str) -> Result<HeaderPage> {
    let mailbox = session.select(folder)?;
    let last = mailbox.exists;
    fetch_range(session, mailbox, last.saturating_sub(PAGE_SIZE - 1).max(1), last)
}

/// Up to `count` messages older than sequence number `before` (for indexing).
pub fn fetch_older(session: &mut ImapSession, folder: &str, before: u32, count: u32) -> Result<HeaderPage> {
    let mailbox = session.select(folder)?;
    let last = before.saturating_sub(1).min(mailbox.exists);
    fetch_range(session, mailbox, last.saturating_sub(count - 1).max(1), last)
}

fn fetch_range(
    session: &mut ImapSession,
    mailbox: imap::types::Mailbox,
    first: u32,
    last: u32,
) -> Result<HeaderPage> {
    let mut page = HeaderPage {
        uidvalidity: mailbox.uid_validity,
        exists: mailbox.exists,
        first_seq: first,
        headers: Vec::new(),
    };
    if mailbox.exists == 0 || last == 0 {
        page.first_seq = 1;
        return Ok(page);
    }
    let range = format!("{first}:{last}");
    let query = format!("(UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.{PREVIEW_BYTES}>)");
    let fetches = session.fetch(range, query)?;

    let mut headers: Vec<Header> = fetches
        .iter()
        .filter_map(|f| {
            let uid = f.uid?;
            // Headers + start of the body: enough to build the preview.
            let mut raw = f.header().unwrap_or_default().to_vec();
            raw.extend_from_slice(f.text().unwrap_or_default());
            Some(header_from_raw(
                uid,
                &raw,
                f.flags().contains(&imap::types::Flag::Seen),
            ))
        })
        .collect();
    headers.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.uid.cmp(&a.uid)));
    page.headers = headers;
    Ok(page)
}

/// List entry (sender, subject, preview, attachments) from a raw message,
/// or from its headers plus the start of the body.
pub fn header_from_raw(uid: u32, raw: &[u8], seen: bool) -> Header {
    let parsed = MessageParser::default().parse(raw);
    let msg = parsed.as_ref();
    let (from, _) = first_addr(msg.and_then(|m| m.from()));
    let content_type = msg
        .and_then(|m| m.content_type())
        .map(|ct| format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or_default()))
        .unwrap_or_default()
        .to_lowercase();
    let has_attachment =
        content_type == "multipart/mixed" || msg.map(|m| m.attachments().len() > 0).unwrap_or(false);
    let text = msg
        .and_then(|m| m.body_text(0))
        .map(|t| t.into_owned())
        .unwrap_or_default();
    Header {
        uid,
        from,
        subject: msg
            .and_then(|m| m.subject())
            .unwrap_or(t("no_subject"))
            .to_string(),
        preview: one_line(&text, 140),
        timestamp: msg.and_then(|m| m.date()).map(|d| d.to_timestamp()).unwrap_or(0),
        seen,
        has_attachment,
        text: text.chars().take(4000).collect(),
    }
}

/// Downloads the full message (marks it as read on the server).
pub fn fetch_message(
    session: &mut ImapSession,
    folder: &str,
    uid: u32,
    keys: &PgpKeys,
) -> Result<FullMessage> {
    session.select(folder)?;
    let fetches = session.uid_fetch(uid.to_string(), "BODY[]")?;
    let raw = fetches
        .iter()
        .find_map(|f| f.body())
        .context(t("err_not_found"))?;
    parse_full(uid, raw, keys)
}

/// Builds the reader's view of a raw message (OpenPGP unwrapped).
pub fn parse_full(uid: u32, raw: &[u8], keys: &PgpKeys) -> Result<FullMessage> {
    let outer = MessageParser::default().parse(raw).context(t("err_unreadable"))?;
    let (from, from_address) = first_addr(outer.from());

    // Autocrypt: remember the sender's key (before checking signatures).
    for value in raw_header_values(raw, "autocrypt") {
        crate::crypto::learn_autocrypt(&from_address, &value);
    }
    let (inner, mut pgp) = unwrap_pgp(raw, &from_address, keys);
    let inner_msg = inner
        .as_deref()
        .and_then(|bytes| MessageParser::default().parse(bytes));
    // Body, HTML and attachments come from the unwrapped content if any;
    // headers always from the outer message.
    let msg = inner_msg.as_ref().unwrap_or(&outer);

    let mut body = msg
        .body_text(0)
        .map(|b| b.into_owned())
        .unwrap_or_else(|| t("no_text").into());
    if let Some(status) = pgp.as_ref().and_then(|p| p.error.clone()) {
        body = format!("{}\n\n{status}", t("pgp_cannot_decrypt"));
    }
    // Inline PGP (armored block in a plain-text body).
    if pgp.is_none()
        && body.contains("-----BEGIN PGP MESSAGE-----")
        && let Some((key, pass)) = &keys.own
    {
        let mut status = PgpStatus {
            encrypted: true,
            ..Default::default()
        };
        match crate::crypto::decrypt(body.as_bytes(), key, pass) {
            Ok(plain) => body = String::from_utf8_lossy(&plain).into_owned(),
            Err(e) => status.error = Some(format!("{e:#}")),
        }
        pgp = Some(status);
    }
    let html: Vec<String> = msg
        .html_body()
        .filter(|part| matches!(part.kind(), mail_parser::PartKind::Html))
        .filter_map(|part| part.text().map(|t| t.into_owned()))
        .collect();
    let html = (!html.is_empty()).then(|| html.join("\n"));
    let inline: HashMap<String, Vec<u8>> = msg
        .parts()
        .filter_map(|part| {
            let id = part.content_id()?.trim().trim_matches(['<', '>']).to_string();
            (!id.is_empty()).then(|| (id, part.decoded().into_owned()))
        })
        .collect();
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
        to: addr_list(outer.to()),
        cc: addr_list(outer.cc()),
        to_addrs: bare_list(outer.to()),
        cc_addrs: bare_list(outer.cc()),
        subject: outer.subject().unwrap_or(t("no_subject")).to_string(),
        timestamp: outer.date().map(|d| d.to_timestamp()).unwrap_or(0),
        message_id: outer.message_id().map(str::to_string),
        references,
        body,
        html,
        inline,
        attachments,
        dkim_pass,
        pgp,
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
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case(name)
        {
            current = Some(v.trim().to_string());
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
    let has_move = session.capabilities().map(|c| c.has_str("MOVE")).unwrap_or(false);
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
    let op = if seen {
        "+FLAGS (\\Seen)"
    } else {
        "-FLAGS (\\Seen)"
    };
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
    use lettre::message::Mailbox;
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
        builder = builder.to(a
            .parse()
            .with_context(|| tf("err_bad_address", &[("addr", &a)]))?);
    }
    for a in split(&mail.cc) {
        builder = builder.cc(a
            .parse()
            .with_context(|| tf("err_bad_address", &[("addr", &a)]))?);
    }
    for a in split(&mail.bcc) {
        builder = builder.bcc(
            a.parse()
                .with_context(|| tf("err_bad_address", &[("addr", &a)]))?,
        );
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

    let content = build_content(mail)?;
    let content = protect(account, mail, content)?;
    if let Some(value) = crate::crypto::autocrypt_header(&account.email) {
        builder = builder.header(Autocrypt(value));
    }
    let email = match content {
        Part::Single(p) => builder.singlepart(p)?,
        Part::Multi(m) => builder.multipart(m)?,
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

/// A MIME body: one part or a multipart tree.
enum Part {
    Single(lettre::message::SinglePart),
    Multi(lettre::message::MultiPart),
}

impl Part {
    fn formatted(&self) -> Vec<u8> {
        match self {
            Part::Single(p) => p.formatted(),
            Part::Multi(m) => m.formatted(),
        }
    }

    fn into_multi(self, outer: lettre::message::MultiPartBuilder) -> lettre::message::MultiPart {
        match self {
            Part::Single(p) => outer.singlepart(p),
            Part::Multi(m) => outer.multipart(m),
        }
    }
}

/// Text (+ HTML alternative) (+ attachments).
fn build_content(mail: &Outgoing) -> Result<Part> {
    use lettre::message::{Attachment as LAttachment, MultiPart, SinglePart, header};
    let text = SinglePart::builder()
        .header(header::ContentType::TEXT_PLAIN)
        .body(mail.body.clone());
    let alternative = mail
        .html
        .as_ref()
        .map(|html| MultiPart::alternative_plain_html(mail.body.clone(), html.clone()));
    if mail.attachments.is_empty() {
        return Ok(match alternative {
            Some(alt) => Part::Multi(alt),
            None => Part::Single(text),
        });
    }
    let mut multi = match alternative {
        Some(alt) => MultiPart::mixed().multipart(alt),
        None => MultiPart::mixed().singlepart(text),
    };
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
    Ok(Part::Multi(multi))
}

/// Applies OpenPGP (RFC 3156): sign → multipart/signed; encrypt →
/// multipart/encrypted (signed first when both).
fn protect(account: &Account, mail: &Outgoing, content: Part) -> Result<Part> {
    if !mail.sign && !mail.encrypt {
        return Ok(content);
    }
    let own = crate::crypto::own_key(&account.email).ok_or_else(|| anyhow!(t("err_pgp_no_own_key")))?;
    protect_with(
        mail,
        content,
        &own,
        &account.pgp_passphrase,
        &crate::crypto::contact_key,
    )
}

fn protect_with(
    mail: &Outgoing,
    content: Part,
    own: &pgp::composed::SignedSecretKey,
    passphrase: &str,
    contact: &dyn Fn(&str) -> Option<pgp::composed::SignedPublicKey>,
) -> Result<Part> {
    use lettre::message::{MultiPart, SinglePart, header::ContentType};
    let mut content = content;
    if mail.sign {
        // RFC 3156: the line break before the next delimiter belongs to the
        // delimiter, not to the signed part.
        let mut signed_bytes = content.formatted();
        if signed_bytes.ends_with(b"\r\n") {
            signed_bytes.truncate(signed_bytes.len() - 2);
        }
        let signature = crate::crypto::sign(&signed_bytes, own, passphrase)?;
        let sig_part = SinglePart::builder()
            .header(ContentType::parse("application/pgp-signature; name=\"signature.asc\"").unwrap())
            .body(signature);
        let outer = MultiPart::signed("application/pgp-signature".into(), "pgp-sha256".into());
        content = Part::Multi(content.into_multi(outer).singlepart(sig_part));
    }
    if mail.encrypt {
        let mut keys = vec![own.to_public_key()];
        let all = [&mail.to, &mail.cc, &mail.bcc]
            .iter()
            .flat_map(|s| s.split([',', ';']))
            .map(bare_address)
            .filter(|a| !a.is_empty())
            .collect::<Vec<_>>();
        for address in all {
            let key =
                contact(&address).ok_or_else(|| anyhow!(tf("err_pgp_no_key", &[("addr", &address)])))?;
            keys.push(key);
        }
        let armored = crate::crypto::encrypt(&content.formatted(), &keys)?;
        let version = SinglePart::builder()
            .header(ContentType::parse("application/pgp-encrypted").unwrap())
            .body(String::from("Version: 1\r\n"));
        let data = SinglePart::builder()
            .header(ContentType::parse("application/octet-stream; name=\"encrypted.asc\"").unwrap())
            .body(armored);
        content = Part::Multi(
            MultiPart::encrypted("application/pgp-encrypted".into())
                .singlepart(version)
                .singlepart(data),
        );
    }
    Ok(content)
}

/// "Name <a@b.it>" → "a@b.it".
pub fn bare_address(s: &str) -> String {
    let s = s.trim();
    match (s.rfind('<'), s.rfind('>')) {
        (Some(a), Some(b)) if a < b => s[a + 1..b].trim().to_lowercase(),
        _ => s.to_lowercase(),
    }
}

/// `Autocrypt:` header (lettre has no type for it).
#[derive(Clone)]
struct Autocrypt(String);

impl lettre::message::header::Header for Autocrypt {
    fn name() -> lettre::message::header::HeaderName {
        lettre::message::header::HeaderName::new_from_ascii_str("Autocrypt")
    }
    fn parse(s: &str) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Autocrypt(s.to_string()))
    }
    fn display(&self) -> lettre::message::header::HeaderValue {
        lettre::message::header::HeaderValue::new(Self::name(), self.0.clone())
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
        assert_eq!(
            classify("INBOX.Trash", &seg("INBOX.Trash"), &[]),
            FolderKind::Trash
        );
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
        let acc = crate::config::Config::load()
            .account()
            .cloned()
            .expect("no account");
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
        let h = fetch_headers(&mut s, "INBOX").unwrap().headers;
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

    #[test]
    #[ignore = "uses the configured account"]
    fn live_idle() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let acc = crate::config::Config::load()
            .account()
            .cloned()
            .expect("no account");
        let mut s = connect(&acc).unwrap();
        assert!(supports_idle(&mut s).unwrap(), "server without IDLE");
        let t = std::time::Instant::now();
        let changed = idle_wait(&mut s, "INBOX", Duration::from_secs(5)).unwrap();
        println!("idle: changed={changed} after {:?}", t.elapsed());
        // The session is still usable after IDLE.
        assert!(s.select("INBOX").unwrap().exists > 0);
        let _ = s.logout();
    }

    #[test]
    fn pgp_mime_roundtrip() {
        use crate::crypto;
        let alice = crypto::generate("Alice", "alice@example.com", "a").unwrap();
        let bob = crypto::generate("Bob", "bob@example.com", "").unwrap();
        let (alice_pub, bob_pub) = (alice.to_public_key(), bob.to_public_key());
        let mut out = Outgoing {
            to: "Bob <bob@example.com>".into(),
            subject: "s".into(),
            body: "Ciao Bob, **prova** con àccénti.".into(),
            html: Some("<p>Ciao Bob, <b>prova</b> con àccénti.</p>".into()),
            sign: true,
            ..Default::default()
        };
        let dir = std::env::temp_dir().join("ferrowl-pgp-test");
        std::fs::create_dir_all(&dir).unwrap();
        let att = dir.join("note.txt");
        std::fs::write(&att, b"allegato").unwrap();
        out.attachments = vec![att];

        let build = |out: &Outgoing| -> Vec<u8> {
            let content = build_content(out).unwrap();
            let contact = |a: &str| (a == "bob@example.com").then(|| bob_pub.clone());
            let content = protect_with(out, content, &alice, "a", &contact).unwrap();
            let b = lettre::Message::builder()
                .from("Alice <alice@example.com>".parse().unwrap())
                .to("Bob <bob@example.com>".parse().unwrap())
                .subject("s");
            match content {
                Part::Single(p) => b.singlepart(p).unwrap(),
                Part::Multi(m) => b.multipart(m).unwrap(),
            }
            .formatted()
        };
        let knows_alice = |a: &str| (a == "alice@example.com").then(|| alice_pub.clone());
        let knows_nobody = |_: &str| None;
        let bob_keys = PgpKeys {
            own: Some((bob.clone(), String::new())),
            contact: &knows_alice,
        };

        // Signed only.
        let raw = build(&out);
        let m = parse_full(1, &raw, &bob_keys).unwrap();
        let pgp = m.pgp.clone().unwrap();
        assert_eq!(pgp.signature, Some(SigState::Valid), "signed: {pgp:?}");
        assert!(!pgp.encrypted);
        assert!(m.body.contains("àccénti"));
        assert_eq!(m.attachments.len(), 1, "signature.asc must not be listed");
        let unknown = PgpKeys {
            own: None,
            contact: &knows_nobody,
        };
        assert_eq!(
            parse_full(1, &raw, &unknown).unwrap().pgp.unwrap().signature,
            Some(SigState::UnknownKey)
        );
        // Tampered content → invalid.
        let tampered = String::from_utf8_lossy(&raw)
            .replace("prova", "PROVA")
            .into_bytes();
        assert_eq!(
            parse_full(1, &tampered, &bob_keys)
                .unwrap()
                .pgp
                .unwrap()
                .signature,
            Some(SigState::Invalid)
        );

        // Signed and encrypted.
        out.encrypt = true;
        let raw = build(&out);
        assert!(
            !String::from_utf8_lossy(&raw).contains("prova"),
            "plaintext leaked"
        );
        let m = parse_full(1, &raw, &bob_keys).unwrap();
        let pgp = m.pgp.clone().unwrap();
        assert!(pgp.encrypted && pgp.error.is_none(), "{pgp:?}");
        assert_eq!(pgp.signature, Some(SigState::Valid));
        assert!(m.body.contains("àccénti") && m.html.is_some());
        assert_eq!(m.attachments.len(), 1);
        // Someone without the key cannot read it.
        let eve = crypto::generate("Eve", "eve@example.com", "").unwrap();
        let eve_keys = PgpKeys {
            own: Some((eve, String::new())),
            contact: &knows_nobody,
        };
        let m = parse_full(1, &raw, &eve_keys).unwrap();
        assert!(m.pgp.unwrap().error.is_some());
        assert!(!m.body.contains("prova"));
        // Encrypting to someone without a key fails before sending.
        out.to = "carol@example.com".into();
        let content = build_content(&out).unwrap();
        let contact = |_: &str| None;
        assert!(protect_with(&out, content, &alice, "a", &contact).is_err());
    }

    /// Sends a test message from the configured account to itself and saves
    /// the copy in Sent, exactly like the app does.
    #[test]
    #[ignore = "sends a real email from the configured account"]
    fn live_send_to_self() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let acc = crate::config::Config::load()
            .account()
            .cloned()
            .expect("no account");
        let dir = std::env::temp_dir().join("ferrowl-live-send");
        std::fs::create_dir_all(&dir).unwrap();
        let att = dir.join("ferrowl-test.txt");
        std::fs::write(&att, "Allegato di prova inviato da Ferrowl.\n").unwrap();
        let body = "Ciao,\n\nquesta è una **mail di prova** inviata da _Ferrowl_.\n\n- testo formattato (HTML + testo)\n- un allegato\n- copia salvata in Inviati\n\nFerrowl";
        let out = Outgoing {
            to: acc.email.clone(),
            subject: format!("Ferrowl: mail di prova {}", chrono::Local::now().format("%H:%M")),
            body: body.into(),
            html: Some(crate::markdown::to_html_document(body)),
            attachments: vec![att],
            ..Default::default()
        };
        let raw = send(&acc, &out).expect("SMTP send");
        println!("sent: {} bytes", raw.len());
        let mut s = connect(&acc).unwrap();
        let sent = list_folders(&mut s)
            .unwrap()
            .into_iter()
            .find(|f| f.kind == FolderKind::Sent)
            .expect("no Sent folder");
        append_sent(&mut s, &sent.name, &raw).expect("append to Sent");
        println!("copy saved in {}", sent.name);
        let _ = s.logout();
    }
}
