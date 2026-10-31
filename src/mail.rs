//! Accesso IMAP (lettura) e SMTP (invio).

use crate::config::Account;
use anyhow::{Context, Result, anyhow};
use mail_parser::{AddressList, MessageParser};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

type TlsStream = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;
pub type ImapSession = imap::Session<TlsStream>;

/// Quanti messaggi (i più recenti) caricare per cartella.
pub const PAGE_SIZE: u32 = 200;

#[derive(Debug, Clone)]
pub struct Header {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub date: String,
    pub timestamp: i64,
    pub seen: bool,
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
    pub subject: String,
    pub date: String,
    pub message_id: Option<String>,
    pub body: String,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("versioni TLS supportate")
    .with_root_certificates(roots)
    .with_no_client_auth();
    Arc::new(config)
}

pub fn connect(account: &Account) -> Result<ImapSession> {
    let addr = (account.imap_host.as_str(), account.imap_port);
    let tcp = TcpStream::connect(addr)
        .with_context(|| format!("connessione a {}:{}", account.imap_host, account.imap_port))?;
    tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(60)))?;

    let server_name = rustls::pki_types::ServerName::try_from(account.imap_host.clone())
        .context("nome host IMAP non valido")?;
    let conn = rustls::ClientConnection::new(tls_config(), server_name)?;
    let tls = rustls::StreamOwned::new(conn, tcp);

    let mut client = imap::Client::new(tls);
    client.read_greeting().context("saluto IMAP")?;
    client
        .login(&account.username, &account.password)
        .map_err(|(e, _)| anyhow!("login IMAP fallito: {e}"))
}

pub fn list_folders(session: &mut ImapSession) -> Result<Vec<String>> {
    let names = session.list(Some(""), Some("*"))?;
    let mut folders: Vec<String> = names
        .iter()
        .filter(|n| !n.attributes().contains(&imap::types::NameAttribute::NoSelect))
        .map(|n| n.name().to_string())
        .collect();
    // INBOX sempre in cima, il resto in ordine alfabetico.
    folders.sort_by_key(|f| (!f.eq_ignore_ascii_case("INBOX"), f.to_lowercase()));
    Ok(folders)
}

fn format_addr(addr: Option<AddressList>) -> (String, String) {
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

fn format_addr_list(addr: Option<AddressList>) -> String {
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

fn format_date(ts: Option<i64>) -> String {
    ts.and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// Seleziona la cartella e scarica le intestazioni dei messaggi più recenti.
pub fn fetch_headers(session: &mut ImapSession, folder: &str) -> Result<Vec<Header>> {
    let mailbox = session.select(folder)?;
    if mailbox.exists == 0 {
        return Ok(Vec::new());
    }
    let start = mailbox.exists.saturating_sub(PAGE_SIZE - 1).max(1);
    let range = format!("{start}:{}", mailbox.exists);
    let fetches = session.fetch(range, "(UID FLAGS BODY.PEEK[HEADER])")?;

    let parser = MessageParser::default();
    let mut headers: Vec<Header> = fetches
        .iter()
        .filter_map(|f| {
            let uid = f.uid?;
            let raw = f.header().unwrap_or_default();
            let msg = parser.parse_headers(raw);
            let (from, _) = format_addr(msg.as_ref().and_then(|m| m.from()));
            let ts = msg.as_ref().and_then(|m| m.date()).map(|d| d.to_timestamp());
            Some(Header {
                uid,
                from,
                subject: msg
                    .as_ref()
                    .and_then(|m| m.subject())
                    .unwrap_or("(nessun oggetto)")
                    .to_string(),
                date: format_date(ts),
                timestamp: ts.unwrap_or(0),
                seen: f.flags().contains(&imap::types::Flag::Seen),
            })
        })
        .collect();
    headers.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.uid.cmp(&a.uid)));
    Ok(headers)
}

/// Scarica il messaggio completo (lo segna come letto sul server).
pub fn fetch_message(session: &mut ImapSession, folder: &str, uid: u32) -> Result<FullMessage> {
    session.select(folder)?;
    let fetches = session.uid_fetch(uid.to_string(), "BODY[]")?;
    let raw = fetches
        .iter()
        .find_map(|f| f.body())
        .context("messaggio non trovato sul server")?;
    let msg = MessageParser::default()
        .parse(raw)
        .context("messaggio non leggibile")?;

    let (from, from_address) = format_addr(msg.from());
    let body = msg
        .body_text(0)
        .map(|b| b.into_owned())
        .unwrap_or_else(|| "(nessun testo)".into());
    let attachments = msg
        .attachments()
        .enumerate()
        .map(|(i, part)| Attachment {
            name: part
                .attachment_name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("allegato-{}", i + 1)),
            data: part.decoded().into_owned(),
        })
        .collect();

    Ok(FullMessage {
        uid,
        from,
        from_address,
        to: format_addr_list(msg.to()),
        subject: msg.subject().unwrap_or("(nessun oggetto)").to_string(),
        date: format_date(msg.date().map(|d| d.to_timestamp())),
        message_id: msg.message_id().map(str::to_string),
        body,
        attachments,
    })
}

pub fn delete_message(session: &mut ImapSession, folder: &str, uid: u32) -> Result<()> {
    session.select(folder)?;
    session.uid_store(uid.to_string(), "+FLAGS (\\Deleted)")?;
    session.expunge()?;
    Ok(())
}

pub fn send(account: &Account, mail: &Outgoing) -> Result<()> {
    use lettre::message::{Mailbox, header::ContentType};
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let from: Mailbox = format!("{} <{}>", account.display_name, account.email)
        .parse()
        .or_else(|_| account.email.parse())
        .context("indirizzo mittente non valido")?;

    let mut builder = Message::builder().from(from).subject(&mail.subject);
    let mut has_recipient = false;
    for to in mail.to.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        builder = builder.to(to.parse().with_context(|| format!("destinatario non valido: {to}"))?);
        has_recipient = true;
    }
    for cc in mail.cc.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        builder = builder.cc(cc.parse().with_context(|| format!("Cc non valido: {cc}"))?);
    }
    if !has_recipient {
        return Err(anyhow!("inserisci almeno un destinatario"));
    }
    if let Some(id) = &mail.in_reply_to {
        let id = format!("<{}>", id.trim_matches(['<', '>']));
        builder = builder.in_reply_to(id.clone()).references(id);
    }
    let email = builder
        .header(ContentType::TEXT_PLAIN)
        .body(mail.body.clone())?;

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
    mailer.send(&email).context("invio SMTP fallito")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Richiede rete: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn tls_handshake_and_login_rejected() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let acc = Account {
            username: "utente-inesistente@gmail.com".into(),
            password: "sbagliata".into(),
            imap_host: "imap.gmail.com".into(),
            ..Default::default()
        };
        let err = connect(&acc).err().expect("il login deve fallire");
        assert!(format!("{err:#}").contains("login IMAP fallito"), "{err:#}");
    }
}
