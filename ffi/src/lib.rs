//! Bindings of `ferrowl-core` for the mobile apps, generated with UniFFI.
//!
//! The apps see one object, [`Engine`], with `suspend` (Kotlin) / `async`
//! (Swift) methods, and plain data records. These records mirror the core
//! types on purpose: the core can change freely while this surface, which
//! the Kotlin and Swift code is written against, stays stable.
//!
//! The apps own account settings and secrets (Android Keystore / iOS
//! Keychain) and pass a complete [`Account`] to [`Engine::connect`]; the
//! engine never reads a config file on mobile.

use ferrowl_core::backend::Backend;
use ferrowl_core::{config, i18n, mail, paths, providers};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

uniffi::setup_scaffolding!();

// ---------------------------------------------------------------- errors

/// Every failure reaches the app as a message ready to show to the user
/// (already translated into the engine's language).
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum FerrowlError {
    Failed(String),
}

impl std::fmt::Display for FerrowlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FerrowlError::Failed(msg) => f.write_str(msg),
        }
    }
}

impl From<String> for FerrowlError {
    fn from(msg: String) -> Self {
        FerrowlError::Failed(msg)
    }
}

type Result<T> = std::result::Result<T, FerrowlError>;

// ---------------------------------------------------------------- records

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Protocol {
    Imap,
    Pop3,
}

/// An account as the app stores it. For POP3, `incoming_host`/`incoming_port`
/// are the POP3 server.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Account {
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub password: String,
    pub incoming_host: String,
    pub incoming_port: u16,
    pub smtp_host: String,
    /// 465 = TLS, 587 = STARTTLS.
    pub smtp_port: u16,
    pub protocol: Protocol,
    /// POP3 only: days a message stays on the server (`None` = always).
    pub pop3_keep_days: Option<u32>,
    /// Passphrase of the account's OpenPGP key (empty = none).
    pub pgp_passphrase: String,
    /// Contacts server (CardDAV); empty = found from the address's domain.
    #[uniffi(default = "")]
    pub carddav_url: String,
    /// Calendar server (CalDAV); empty = found from the address's domain.
    #[uniffi(default = "")]
    pub caldav_url: String,
}

impl From<Account> for config::Account {
    fn from(a: Account) -> Self {
        config::Account {
            display_name: a.display_name,
            email: a.email,
            username: a.username,
            password: a.password,
            imap_host: a.incoming_host,
            imap_port: a.incoming_port,
            smtp_host: a.smtp_host,
            smtp_port: a.smtp_port,
            pgp_passphrase: a.pgp_passphrase,
            protocol: match a.protocol {
                Protocol::Imap => config::Protocol::Imap,
                Protocol::Pop3 => config::Protocol::Pop3,
            },
            pop3_keep_days: a.pop3_keep_days,
            keyring: false,
            carddav_url: a.carddav_url,
            caldav_url: a.caldav_url,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FolderKind {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Spam,
    Trash,
    Other,
}

impl From<mail::FolderKind> for FolderKind {
    fn from(k: mail::FolderKind) -> Self {
        match k {
            mail::FolderKind::Inbox => FolderKind::Inbox,
            mail::FolderKind::Sent => FolderKind::Sent,
            mail::FolderKind::Drafts => FolderKind::Drafts,
            mail::FolderKind::Archive => FolderKind::Archive,
            mail::FolderKind::Spam => FolderKind::Spam,
            mail::FolderKind::Trash => FolderKind::Trash,
            mail::FolderKind::Other => FolderKind::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Folder {
    /// Server name: pass it back to the engine as is.
    pub id: String,
    /// Name to show (translated for the standard folders).
    pub name: String,
    /// Path segments, for nesting.
    pub path: Vec<String>,
    pub kind: FolderKind,
    pub selectable: bool,
}

impl From<&mail::Folder> for Folder {
    fn from(f: &mail::Folder) -> Self {
        Folder {
            id: f.name.clone(),
            name: f.display(),
            path: f.path.clone(),
            kind: f.kind.into(),
            selectable: f.selectable,
        }
    }
}

/// A row of the message list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Header {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub preview: String,
    /// Unix time, seconds.
    pub timestamp: i64,
    pub seen: bool,
    pub has_attachment: bool,
    /// Message-ID without brackets (empty if none); used for conversations.
    pub message_id: String,
    /// Ids this message replies to.
    pub parents: Vec<String>,
}

impl From<mail::Header> for Header {
    fn from(h: mail::Header) -> Self {
        Header {
            uid: h.uid,
            from: h.from,
            subject: h.subject,
            preview: h.preview,
            timestamp: h.timestamp,
            seen: h.seen,
            has_attachment: h.has_attachment,
            message_id: h.message_id,
            parents: h.parents,
        }
    }
}

impl From<&Header> for mail::Header {
    fn from(h: &Header) -> Self {
        mail::Header {
            uid: h.uid,
            from: h.from.clone(),
            subject: h.subject.clone(),
            preview: h.preview.clone(),
            timestamp: h.timestamp,
            seen: h.seen,
            has_attachment: h.has_attachment,
            message_id: h.message_id.clone(),
            parents: h.parents.clone(),
            ..Default::default()
        }
    }
}

/// A conversation in a message list: its newest message plus its members.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Conversation {
    /// Newest message, shown as the list row.
    pub latest: Header,
    /// Members' uids, oldest first (reading order).
    pub uids: Vec<u32>,
    pub unread: u32,
}

/// Groups a message list into conversations (see `ferrowl_core::threads`).
#[uniffi::export]
pub fn group_conversations(headers: Vec<Header>) -> Vec<Conversation> {
    let core: Vec<mail::Header> = headers.iter().map(mail::Header::from).collect();
    ferrowl_core::threads::group(&core)
        .into_iter()
        .map(|t| Conversation {
            latest: headers[t.latest()].clone(),
            uids: t.members.iter().rev().map(|&i| headers[i].uid).collect(),
            unread: t.unread as u32,
        })
        .collect()
}

/// The newest messages of a folder. `more` is set when older messages are
/// still being indexed: call [`Engine::index_older`] with it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HeaderPage {
    pub headers: Vec<Header>,
    pub more: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Signature {
    Valid,
    Invalid,
    UnknownKey,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PgpInfo {
    pub encrypted: bool,
    pub signature: Option<Signature>,
    /// Why decryption failed, if it did.
    pub error: Option<String>,
}

/// An opened message.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Message {
    pub uid: u32,
    pub from: String,
    pub from_address: String,
    pub to: String,
    pub cc: String,
    pub to_addresses: Vec<String>,
    pub cc_addresses: Vec<String>,
    pub subject: String,
    pub timestamp: i64,
    pub message_id: Option<String>,
    pub references: Vec<String>,
    /// Plain text body (always present).
    pub text: String,
    /// HTML body as sent. Show it in a WebView with JavaScript off.
    pub html: Option<String>,
    /// Inline images referenced as `cid:` from the HTML, by Content-ID.
    pub inline: HashMap<String, Vec<u8>>,
    pub attachments: Vec<Attachment>,
    pub dkim_pass: bool,
    pub pgp: Option<PgpInfo>,
}

impl From<mail::FullMessage> for Message {
    fn from(m: mail::FullMessage) -> Self {
        Message {
            uid: m.uid,
            from: m.from,
            from_address: m.from_address,
            to: m.to,
            cc: m.cc,
            to_addresses: m.to_addrs,
            cc_addresses: m.cc_addrs,
            subject: m.subject,
            timestamp: m.timestamp,
            message_id: m.message_id,
            references: m.references,
            text: m.body,
            html: m.html,
            inline: m.inline,
            attachments: m
                .attachments
                .into_iter()
                .map(|a| Attachment {
                    name: a.name,
                    data: a.data,
                })
                .collect(),
            dkim_pass: m.dkim_pass,
            pgp: m.pgp.map(|p| PgpInfo {
                encrypted: p.encrypted,
                signature: p.signature.map(|s| match s {
                    mail::SigState::Valid => Signature::Valid,
                    mail::SigState::Invalid => Signature::Invalid,
                    mail::SigState::UnknownKey => Signature::UnknownKey,
                }),
                error: p.error,
            }),
        }
    }
}

/// A message to send.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Outgoing {
    /// Comma-separated addresses.
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    /// Plain text (or Markdown, see `html`).
    pub body: String,
    /// HTML alternative, e.g. from [`markdown_to_html`].
    pub html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// Paths of files to attach (inside the app's sandbox).
    pub attachments: Vec<String>,
    pub sign: bool,
    pub encrypt: bool,
}

impl From<Outgoing> for mail::Outgoing {
    fn from(o: Outgoing) -> Self {
        mail::Outgoing {
            to: o.to,
            cc: o.cc,
            bcc: o.bcc,
            subject: o.subject,
            body: o.body,
            html: o.html,
            in_reply_to: o.in_reply_to,
            references: o.references,
            attachments: o.attachments.into_iter().map(PathBuf::from).collect(),
            sign: o.sign,
            encrypt: o.encrypt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Login {
    /// Needs an app-specific password.
    AppPassword,
    /// Needs "Sign in with Microsoft".
    MicrosoftSignIn,
}

/// An address suggested while typing a recipient.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AddressSuggestion {
    pub name: String,
    pub address: String,
}

/// A contact from the account's contacts server.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Contact {
    pub name: String,
    /// Preferred address first.
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub organization: String,
    pub note: String,
}

/// One occurrence of a calendar event.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CalendarEvent {
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    /// Start and end, seconds since the epoch.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// All-day events: first and last day (inclusive), "YYYY-MM-DD".
    pub first_day: Option<String>,
    pub last_day: Option<String>,
    /// Part of a repeating series.
    pub recurring: bool,
    pub organizer: String,
    /// Name and color ("#RRGGBB[AA]", may be empty) of its calendar.
    pub calendar: String,
    pub color: String,
}

/// A known mail provider, for the "add account" screen.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Provider {
    pub name: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub login: Login,
    /// Setup guide on ferrowl.com, in the engine's language.
    pub guide_url: Option<String>,
}

// ---------------------------------------------------------------- free functions

/// Providers shown when adding an account, in display order.
#[uniffi::export]
pub fn known_providers() -> Vec<Provider> {
    let lang = i18n::current().code;
    providers::PROVIDERS
        .iter()
        .map(|p| Provider {
            name: p.name.into(),
            imap_host: p.imap.0.into(),
            imap_port: p.imap.1,
            smtp_host: p.smtp.0.into(),
            smtp_port: p.smtp.1,
            login: match p.login {
                providers::Login::AppPassword => Login::AppPassword,
                providers::Login::MicrosoftSignIn => Login::MicrosoftSignIn,
            },
            guide_url: p.guide.map(|g| providers::guide_url(g, lang)),
        })
        .collect()
}

/// Index into [`known_providers`] of the provider an address belongs to.
#[uniffi::export]
pub fn provider_for_email(email: String) -> Option<u32> {
    providers::for_email(&email).map(|i| i as u32)
}

/// HTML to send alongside the text when the user wrote Markdown formatting
/// (`None` for plain text, which is then sent as text only).
/// Initials for an avatar: "Giulia Ferri" -> "GF", "anna@x.it" -> "A".
#[uniffi::export]
pub fn initials(name: String) -> String {
    ferrowl_core::initials(&name)
}

#[uniffi::export]
pub fn markdown_to_html(markdown: String) -> Option<String> {
    ferrowl_core::markdown::has_formatting(&markdown)
        .then(|| ferrowl_core::markdown::to_html_document(&markdown))
}

/// Translated UI string from `core/locales`, for texts shared with desktop.
#[uniffi::export]
pub fn translate(key: String) -> String {
    i18n::t(&key).to_string()
}

// ---------------------------------------------------------------- drafts

/// Full date in the engine's language, e.g. "2 novembre 2026, 09:42".
fn full_date(ts: i64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let Some(d) = Local.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    i18n::tf(
        "date_other",
        &[
            ("day", &d.day().to_string()),
            ("month", i18n::t(&format!("month_{}", d.month()))),
            ("year", &d.year().to_string()),
            ("time", &d.format("%H:%M").to_string()),
        ],
    )
}

fn blank_draft() -> Outgoing {
    Outgoing {
        to: String::new(),
        cc: String::new(),
        bcc: String::new(),
        subject: String::new(),
        body: String::new(),
        html: None,
        in_reply_to: None,
        references: Vec::new(),
        attachments: Vec::new(),
        sign: false,
        encrypt: false,
    }
}

/// A reply, as the desktop writes it: "Re:" subject, quoted text with a
/// translated "On <date>, <name> wrote:" line, threading headers.
/// `reply_all` adds the other recipients (never `own_address`).
#[uniffi::export]
pub fn reply_draft(message: Message, reply_all: bool, own_address: String) -> Outgoing {
    let own = own_address.trim().to_lowercase();
    let not_me = |a: &&String| a.trim().to_lowercase() != own;
    let mut to = vec![message.from_address.clone()];
    let mut cc = Vec::new();
    if reply_all {
        to.extend(message.to_addresses.iter().filter(not_me).cloned());
        cc.extend(message.cc_addresses.iter().filter(not_me).cloned());
    }
    to.dedup();
    let subject = if message.subject.to_lowercase().starts_with("re:") {
        message.subject.clone()
    } else {
        format!("Re: {}", message.subject)
    };
    let quoted: String = message.text.lines().map(|l| format!("> {l}\n")).collect();
    let header = i18n::tf(
        "quote_header",
        &[("date", &full_date(message.timestamp)), ("name", &message.from)],
    );
    // The engine adds the replied-to id to References when sending.
    let references = message.references.clone();
    Outgoing {
        to: to.join(", "),
        cc: cc.join(", "),
        subject,
        body: format!("\n\n{header}\n{quoted}"),
        in_reply_to: message.message_id.clone(),
        references,
        encrypt: message.pgp.as_ref().is_some_and(|p| p.encrypted),
        ..blank_draft()
    }
}

/// A forward: "Fwd:" subject and the original headers and text. Attachments
/// are not carried over yet.
#[uniffi::export]
pub fn forward_draft(message: Message) -> Outgoing {
    let body = format!(
        "\n\n{}\n{}: {} <{}>\n{}: {}\n{}: {}\n{}: {}\n\n{}",
        i18n::t("forward_header"),
        i18n::t("from"),
        message.from,
        message.from_address,
        i18n::t("date"),
        full_date(message.timestamp),
        i18n::t("subject"),
        message.subject,
        i18n::t("to"),
        message.to,
        message.text
    );
    Outgoing {
        subject: format!("Fwd: {}", message.subject),
        body,
        ..blank_draft()
    }
}

// ---------------------------------------------------------------- engine

/// The engine: one per app process.
#[derive(uniffi::Object)]
pub struct Engine {
    backend: Backend,
    account: Mutex<Option<config::Account>>,
    folders: Mutex<Vec<mail::Folder>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl Engine {
    /// `data_dir`: app-private directory for the cache, the POP3 mailbox and
    /// OpenPGP keys. `language`: a code such as "it", or "auto".
    #[uniffi::constructor]
    pub fn new(data_dir: String, language: String) -> Arc<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let data = PathBuf::from(data_dir);
        paths::set(data.clone(), data.join("config"));
        i18n::set(Some(&language));
        Arc::new(Engine {
            backend: Backend::default(),
            account: Mutex::new(None),
            folders: Mutex::new(Vec::new()),
        })
    }

    /// Checks the settings without changing the connected account.
    /// Returns a short confirmation to show.
    pub async fn test(&self, account: Account) -> Result<String> {
        Ok(Backend::test(account.into()).await?)
    }

    /// Connects an account (replacing the previous one) and lists its folders.
    pub async fn connect(&self, account: Account) -> Result<Vec<Folder>> {
        let account: config::Account = account.into();
        let folders = self.backend.connect(account.clone()).await?;
        let out = folders.iter().map(Folder::from).collect();
        *lock(&self.account) = Some(account);
        *lock(&self.folders) = folders;
        Ok(out)
    }

    /// Messages already stored on the device, instantly (before `headers`).
    pub fn cached_headers(&self, folder_id: String) -> Vec<Header> {
        match lock(&self.account).as_ref() {
            Some(a) => Backend::cached_headers(a, &folder_id)
                .into_iter()
                .map(Header::from)
                .collect(),
            None => Vec::new(),
        }
    }

    /// Newest messages from the server (POP3: downloads new mail first).
    pub async fn headers(&self, folder_id: String) -> Result<HeaderPage> {
        let (headers, first_seq) = self.backend.headers(folder_id).await?;
        Ok(HeaderPage {
            headers: headers.into_iter().map(Header::from).collect(),
            more: (first_seq > 1).then_some(first_seq),
        })
    }

    /// Indexes older messages for search, in the background. Returns the
    /// value for the next call, or `None` when done.
    pub async fn index_older(&self, folder_id: String, more: u32) -> Result<Option<u32>> {
        Ok(self.backend.index_older(folder_id, more, 150).await?)
    }

    /// Search in the messages stored on the device.
    pub fn search(&self, folder_id: String, query: String) -> Vec<Header> {
        let words: Vec<String> = query.split_whitespace().map(str::to_owned).collect();
        match lock(&self.account).as_ref() {
            Some(a) if !words.is_empty() => Backend::search(a, &folder_id, &words)
                .into_iter()
                .map(Header::from)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Recipients matching what is being typed, from the account's mail
    /// (people we write to first).
    pub fn suggest_addresses(&self, query: String) -> Vec<AddressSuggestion> {
        match lock(&self.account).as_ref() {
            Some(a) => Backend::suggest(a, &query)
                .into_iter()
                .map(|(name, address)| AddressSuggestion { name, address })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Saved contacts whose name, address, organization or phone contains
    /// `query` (all when empty), by name. Reads the local copy only.
    pub fn contacts(&self, query: String) -> Vec<Contact> {
        match lock(&self.account).as_ref() {
            Some(a) => Backend::contacts(a, &query)
                .into_iter()
                .map(|c| Contact {
                    name: c.name,
                    emails: c.emails,
                    phones: c.phones,
                    organization: c.organization,
                    note: c.note,
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Brings the local copy of the contacts up to date with the account's
    /// CardDAV server. Returns how many contacts are saved.
    pub async fn sync_contacts(&self) -> Result<u32> {
        // No account yet (the app connects first): nothing to sync.
        let Some(account) = lock(&self.account).clone() else {
            return Ok(0);
        };
        Ok(Backend::sync_contacts(account).await? as u32)
    }

    /// Event occurrences overlapping `[from, to)` (seconds since the
    /// epoch), sorted by start. Reads the local copy only.
    pub fn agenda(&self, from: i64, to: i64) -> Vec<CalendarEvent> {
        let Some(a) = lock(&self.account).clone() else {
            return Vec::new();
        };
        Backend::agenda(&a, from, to)
            .into_iter()
            .map(|e| {
                let o = e.occurrence;
                CalendarEvent {
                    uid: o.uid,
                    summary: o.summary,
                    location: o.location,
                    description: o.description,
                    start: o.start,
                    end: o.end,
                    all_day: o.all_day,
                    first_day: o.first_day.map(|d| d.to_string()),
                    last_day: o.last_day.map(|d| d.to_string()),
                    recurring: o.recurring,
                    organizer: o.organizer,
                    calendar: e.calendar,
                    color: e.color,
                }
            })
            .collect()
    }

    /// Brings the local copy of the calendars up to date with the
    /// account's CalDAV server. Returns how many calendars there are.
    pub async fn sync_calendar(&self) -> Result<u32> {
        // No account yet (the app connects first): nothing to sync.
        let Some(account) = lock(&self.account).clone() else {
            return Ok(0);
        };
        Ok(Backend::sync_calendar(account).await? as u32)
    }

    /// Unread counts per folder id.
    pub async fn unread(&self, folder_ids: Vec<String>) -> Result<HashMap<String, u32>> {
        Ok(self.backend.unseen(folder_ids).await?)
    }

    /// Opens a message (marks it as read).
    pub async fn message(&self, folder_id: String, uid: u32) -> Result<Message> {
        Ok(self.backend.message(folder_id, uid).await?.into())
    }

    pub async fn set_seen(&self, folder_id: String, uid: u32, seen: bool) -> Result<()> {
        Ok(self.backend.set_seen(folder_id, uid, seen).await?)
    }

    pub async fn move_to(&self, folder_id: String, uid: u32, target_id: String) -> Result<()> {
        Ok(self.backend.move_to(folder_id, uid, target_id).await?)
    }

    /// Moves to the Archive folder (created if missing). Returns its id.
    pub async fn archive(&self, folder_id: String, uid: u32) -> Result<String> {
        let folders = lock(&self.folders).clone();
        Ok(self.backend.archive(folder_id, uid, folders).await?)
    }

    pub async fn delete_forever(&self, folder_id: String, uid: u32) -> Result<()> {
        Ok(self.backend.delete_forever(folder_id, uid).await?)
    }

    /// Marks several messages read or unread with one server command.
    pub async fn set_seen_many(&self, folder_id: String, uids: Vec<u32>, seen: bool) -> Result<()> {
        Ok(self.backend.set_seen_many(folder_id, uids, seen).await?)
    }

    pub async fn move_many(&self, folder_id: String, uids: Vec<u32>, target_id: String) -> Result<()> {
        Ok(self.backend.move_many(folder_id, uids, target_id).await?)
    }

    /// Moves several messages to the Archive (created if missing).
    pub async fn archive_many(&self, folder_id: String, uids: Vec<u32>) -> Result<String> {
        let folders = lock(&self.folders).clone();
        Ok(self.backend.archive_many(folder_id, uids, folders).await?)
    }

    pub async fn delete_forever_many(&self, folder_id: String, uids: Vec<u32>) -> Result<()> {
        Ok(self.backend.delete_forever_many(folder_id, uids).await?)
    }

    /// Sends and stores a copy in Sent. Returns a warning if the message
    /// went out but the copy could not be saved.
    pub async fn send(&self, message: Outgoing) -> Result<Option<String>> {
        let sent = lock(&self.folders)
            .iter()
            .find(|f| f.kind == mail::FolderKind::Sent)
            .map(|f| f.name.clone());
        Ok(self.backend.send(message.into(), sent).await?)
    }
}

/// Poisoned locks only mean another call panicked; the data is still usable.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_and_detection() {
        let list = known_providers();
        assert!(list.iter().any(|p| p.name == "Google"));
        let i = provider_for_email("someone@gmail.com".into()).expect("gmail") as usize;
        assert_eq!(list[i].imap_host, "imap.gmail.com");
    }

    fn sample() -> Message {
        Message {
            uid: 1,
            from: "Ada".into(),
            from_address: "ada@example.org".into(),
            to: "me, Bob".into(),
            cc: String::new(),
            to_addresses: vec!["me@example.org".into(), "bob@example.org".into()],
            cc_addresses: vec!["carl@example.org".into()],
            subject: "Engines".into(),
            timestamp: 1_790_000_000,
            message_id: Some("<1@x>".into()),
            references: vec!["<0@x>".into()],
            text: "line 1\nline 2".into(),
            html: None,
            inline: HashMap::new(),
            attachments: Vec::new(),
            dkim_pass: false,
            pgp: None,
        }
    }

    #[test]
    fn conversations_group_replies() {
        let h = |uid: u32, id: &str, parent: &[&str]| Header {
            uid,
            from: String::new(),
            subject: String::new(),
            preview: String::new(),
            timestamp: uid as i64,
            seen: uid != 2,
            has_attachment: false,
            message_id: id.into(),
            parents: parent.iter().map(|p| p.to_string()).collect(),
        };
        let c = group_conversations(vec![h(1, "a", &[]), h(2, "b", &["a"]), h(3, "x", &[])]);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].uids, vec![1, 2]);
        assert_eq!(c[1].latest.uid, 2);
        assert_eq!(c[1].unread, 1);
    }

    #[test]
    fn reply_and_forward_drafts() {
        let r = reply_draft(sample(), false, "me@example.org".into());
        assert_eq!(r.to, "ada@example.org");
        assert_eq!(r.subject, "Re: Engines");
        assert!(r.body.contains("> line 1\n> line 2"));
        assert_eq!(r.in_reply_to.as_deref(), Some("<1@x>"));
        assert_eq!(r.references, vec!["<0@x>".to_string()]);

        let all = reply_draft(sample(), true, "ME@example.org".into());
        assert_eq!(all.to, "ada@example.org, bob@example.org");
        assert_eq!(all.cc, "carl@example.org");

        let f = forward_draft(sample());
        assert_eq!(f.subject, "Fwd: Engines");
        assert!(f.to.is_empty() && f.body.contains("line 2"));
    }

    #[test]
    fn account_round_trip() {
        let a = Account {
            display_name: "Ada".into(),
            email: "ada@example.org".into(),
            username: "ada".into(),
            password: "pw".into(),
            incoming_host: "pop.example.org".into(),
            incoming_port: 995,
            smtp_host: "smtp.example.org".into(),
            smtp_port: 465,
            protocol: Protocol::Pop3,
            pop3_keep_days: Some(7),
            pgp_passphrase: String::new(),
            carddav_url: String::new(),
            caldav_url: String::new(),
        };
        let core: config::Account = a.into();
        assert!(core.is_pop3());
        assert_eq!(core.imap_host, "pop.example.org");
        assert_eq!(core.pop3_keep_days, Some(7));
    }
}
