//! Local cache (SQLite) and full-text index (FTS5).
//!
//! Stores, per account and folder, the message list (headers, preview, flags)
//! and the start of each body, plus the full text of messages that were
//! opened. Used to show a folder instantly and to search beyond the messages
//! loaded from the server.
//!
//! The file lives in the user's data directory with owner-only permissions.
//! It is a cache: deleting it loses nothing that is not on the server.

use crate::mail::Header;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

pub struct Cache {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS folders (
    account     TEXT NOT NULL,
    folder      TEXT NOT NULL,
    uidvalidity INTEGER NOT NULL,
    PRIMARY KEY (account, folder)
);
CREATE TABLE IF NOT EXISTS messages (
    id       INTEGER PRIMARY KEY,
    account  TEXT NOT NULL,
    folder   TEXT NOT NULL,
    uid      INTEGER NOT NULL,
    sender   TEXT NOT NULL,
    subject  TEXT NOT NULL,
    preview  TEXT NOT NULL,
    ts       INTEGER NOT NULL,
    seen     INTEGER NOT NULL,
    attach   INTEGER NOT NULL,
    body     TEXT NOT NULL DEFAULT '',
    full     INTEGER NOT NULL DEFAULT 0,
    msgid    TEXT NOT NULL DEFAULT '',
    parents  TEXT NOT NULL DEFAULT '',
    counted  INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account, folder, uid)
);
CREATE INDEX IF NOT EXISTS messages_list ON messages (account, folder, ts DESC);
-- Address book built from mail: people who wrote to us, people we wrote to.
CREATE TABLE IF NOT EXISTS addresses (
    account  TEXT NOT NULL,
    address  TEXT NOT NULL,
    name     TEXT NOT NULL DEFAULT '',
    received INTEGER NOT NULL DEFAULT 0,
    sent     INTEGER NOT NULL DEFAULT 0,
    last_ts  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account, address)
);
-- Contacts synced from the account's CardDAV server.
CREATE TABLE IF NOT EXISTS contacts (
    account TEXT NOT NULL,
    url     TEXT NOT NULL,
    book    TEXT NOT NULL,
    etag    TEXT NOT NULL,
    uid     TEXT NOT NULL DEFAULT '',
    name    TEXT NOT NULL,
    emails  TEXT NOT NULL DEFAULT '',
    phones  TEXT NOT NULL DEFAULT '',
    org     TEXT NOT NULL DEFAULT '',
    note    TEXT NOT NULL DEFAULT '',
    vcard   TEXT NOT NULL,
    PRIMARY KEY (account, url)
);
-- Calendars and their events synced from the account's CalDAV server.
-- first_ts/last_ts bound the event's occurrences (last_ts NULL = series
-- without end), so a time range only parses the events it may contain.
CREATE TABLE IF NOT EXISTS calendars (
    account TEXT NOT NULL,
    url     TEXT NOT NULL,
    name    TEXT NOT NULL,
    color   TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (account, url)
);
CREATE TABLE IF NOT EXISTS events (
    account  TEXT NOT NULL,
    url      TEXT NOT NULL,
    calendar TEXT NOT NULL,
    etag     TEXT NOT NULL,
    first_ts INTEGER NOT NULL,
    last_ts  INTEGER,
    ics      TEXT NOT NULL,
    PRIMARY KEY (account, url)
);
CREATE INDEX IF NOT EXISTS events_span ON events (account, first_ts);
-- Our answers to invitations, by event UID (shown on the invitation even
-- without a calendar server).
CREATE TABLE IF NOT EXISTS invite_replies (
    account  TEXT NOT NULL,
    uid      TEXT NOT NULL,
    partstat TEXT NOT NULL,
    PRIMARY KEY (account, uid)
);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    sender, subject, body,
    content = 'messages', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2',
    prefix = '2 3'
);
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts (rowid, sender, subject, body)
    VALUES (new.id, new.sender, new.subject, new.body);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, sender, subject, body)
    VALUES ('delete', old.id, old.sender, old.subject, old.body);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF sender, subject, body ON messages BEGIN
    INSERT INTO messages_fts (messages_fts, rowid, sender, subject, body)
    VALUES ('delete', old.id, old.sender, old.subject, old.body);
    INSERT INTO messages_fts (rowid, sender, subject, body)
    VALUES (new.id, new.sender, new.subject, new.body);
END;
";

fn path() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("cache.sqlite"))
}

/// The shared cache, or `None` if it cannot be opened (the app then works
/// without it).
pub fn get() -> Option<&'static Cache> {
    static CACHE: OnceLock<Option<Cache>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let path = path()?;
            match Cache::open(&path) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("cache disabled: {e:#}");
                    None
                }
            }
        })
        .as_ref()
}

impl Cache {
    pub fn open(path: &std::path::Path) -> Result<Cache> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).context("opening cache")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Self::with_connection(conn)
    }

    fn with_connection(conn: Connection) -> Result<Cache> {
        conn.execute_batch(SCHEMA).context("creating cache schema")?;
        // Columns added after the first release (conversations).
        for (column, ddl) in [
            (
                "msgid",
                "ALTER TABLE messages ADD COLUMN msgid TEXT NOT NULL DEFAULT ''",
            ),
            (
                "parents",
                "ALTER TABLE messages ADD COLUMN parents TEXT NOT NULL DEFAULT ''",
            ),
            (
                "counted",
                "ALTER TABLE messages ADD COLUMN counted INTEGER NOT NULL DEFAULT 0",
            ),
        ] {
            if !has_column(&conn, "messages", column)? {
                conn.execute_batch(ddl)?;
            }
        }
        Ok(Cache {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> Cache {
        Self::with_connection(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Records the folder's UIDVALIDITY; if it changed, the cached UIDs are
    /// meaningless and the folder is emptied.
    pub fn check_uidvalidity(&self, account: &str, folder: &str, uidvalidity: u32) -> Result<()> {
        let conn = self.lock();
        let known: Option<u32> = conn
            .query_row(
                "SELECT uidvalidity FROM folders WHERE account = ?1 AND folder = ?2",
                params![account, folder],
                |r| r.get(0),
            )
            .optional()?;
        if known != Some(uidvalidity) {
            conn.execute(
                "DELETE FROM messages WHERE account = ?1 AND folder = ?2",
                params![account, folder],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO folders (account, folder, uidvalidity) VALUES (?1, ?2, ?3)",
                params![account, folder, uidvalidity],
            )?;
        }
        Ok(())
    }

    /// Inserts or updates headers. Bodies already stored in full are kept.
    /// Stores list entries. Messages seen for the first time also feed the
    /// address book (`own` = the account's address, to tell sent from received).
    pub fn store_headers(&self, account: &str, folder: &str, headers: &[Header], own: &str) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            // Each message feeds the address book once: new ones, and those
            // cached before the address book existed (counted = 0).
            let mut counted = tx.prepare_cached(
                "SELECT 1 FROM messages WHERE account = ?1 AND folder = ?2 AND uid = ?3 AND counted = 1",
            )?;
            for h in headers {
                if !counted.exists(params![account, folder, h.uid])? {
                    harvest(&tx, account, own, h)?;
                }
            }
        }
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO messages (account, folder, uid, sender, subject, preview, ts, seen, attach, body, msgid, parents, counted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)
                 ON CONFLICT (account, folder, uid) DO UPDATE SET
                    counted = 1,
                    seen = excluded.seen,
                    msgid = excluded.msgid,
                    parents = excluded.parents,
                    sender = excluded.sender,
                    subject = excluded.subject,
                    preview = excluded.preview,
                    attach = excluded.attach,
                    body = CASE WHEN messages.full THEN messages.body ELSE excluded.body END",
            )?;
            for h in headers {
                stmt.execute(params![
                    account,
                    folder,
                    h.uid,
                    h.from,
                    h.subject,
                    h.preview,
                    h.timestamp,
                    h.seen,
                    h.has_attachment,
                    h.text,
                    h.message_id,
                    h.parents.join(" ")
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Removes cached messages with UID ≥ `from_uid` that the server no longer
    /// lists (deleted or moved elsewhere).
    pub fn prune(&self, account: &str, folder: &str, from_uid: u32, present: &[u32]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let cached: Vec<u32> = {
            let mut stmt =
                tx.prepare("SELECT uid FROM messages WHERE account = ?1 AND folder = ?2 AND uid >= ?3")?;
            stmt.query_map(params![account, folder, from_uid], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let present: std::collections::HashSet<u32> = present.iter().copied().collect();
        for uid in cached.into_iter().filter(|u| !present.contains(u)) {
            tx.execute(
                "DELETE FROM messages WHERE account = ?1 AND folder = ?2 AND uid = ?3",
                params![account, folder, uid],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn store_body(&self, account: &str, folder: &str, uid: u32, body: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE messages SET body = ?4, full = 1, seen = 1
             WHERE account = ?1 AND folder = ?2 AND uid = ?3",
            params![account, folder, uid, body],
        )?;
        Ok(())
    }

    pub fn set_seen(&self, account: &str, folder: &str, uid: u32, seen: bool) -> Result<()> {
        self.lock().execute(
            "UPDATE messages SET seen = ?4 WHERE account = ?1 AND folder = ?2 AND uid = ?3",
            params![account, folder, uid, seen],
        )?;
        Ok(())
    }

    pub fn remove(&self, account: &str, folder: &str, uid: u32) -> Result<()> {
        self.lock().execute(
            "DELETE FROM messages WHERE account = ?1 AND folder = ?2 AND uid = ?3",
            params![account, folder, uid],
        )?;
        Ok(())
    }

    /// The most recent `limit` messages of a folder.
    pub fn headers(&self, account: &str, folder: &str, limit: u32) -> Result<Vec<Header>> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT uid, sender, subject, preview, ts, seen, attach, msgid, parents FROM messages
             WHERE account = ?1 AND folder = ?2 ORDER BY ts DESC, uid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![account, folder, limit], row_to_header)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Number of cached messages and lowest cached UID of a folder.
    pub fn folder_stats(&self, account: &str, folder: &str) -> Result<(u32, Option<u32>)> {
        Ok(self.lock().query_row(
            "SELECT COUNT(*), MIN(uid) FROM messages WHERE account = ?1 AND folder = ?2",
            params![account, folder],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }

    /// Feeds the address book with one message (POP3 downloads).
    pub fn store_address_book(&self, account: &str, own: &str, h: &Header) -> Result<()> {
        harvest(&self.lock(), account, own, h)
    }

    /// Recorded that we sent to these addresses (right after sending).
    pub fn note_sent(&self, account: &str, recipients: &[(String, String)], ts: i64) -> Result<()> {
        let conn = self.lock();
        let h = Header {
            from_address: "\u{0}me".into(),
            recipients: recipients.to_vec(),
            timestamp: ts,
            ..Default::default()
        };
        harvest(&conn, account, "\u{0}me", &h)
    }

    /// Address suggestions for a To/Cc field: addresses starting with, or
    /// names containing, `query`; people we write to first, then the most
    /// frequent and recent senders.
    pub fn suggest(&self, account: &str, query: &str, limit: u32) -> Result<Vec<(String, String)>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let like = like_escape;
        // Saved contacts come first.
        let mut out: Vec<(String, String)> = Vec::new();
        for c in self.contacts(account, &q)? {
            let name_hit = c.name.to_lowercase().contains(&q);
            for addr in &c.emails {
                if (name_hit || addr.to_lowercase().contains(&q)) && out.len() < limit as usize {
                    out.push((c.name.clone(), addr.to_lowercase()));
                }
            }
        }
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT name, address FROM addresses
             WHERE account = ?1
               AND (address LIKE ?2 ESCAPE '\\' OR lower(name) LIKE ?3 ESCAPE '\\')
             ORDER BY sent * 5 + received DESC, last_ts DESC LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![
                account,
                format!("{}%", like(&q)),
                format!("%{}%", like(&q)),
                limit * 2
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        for row in rows {
            let (name, addr) = row?;
            if !NO_REPLY.iter().any(|n| addr.contains(n)) && !out.iter().any(|(_, a)| *a == addr) {
                out.push((name, addr));
            }
        }
        out.truncate(limit as usize);
        Ok(out)
    }

    /// Card URL and ETag of every cached contact (to compare with the
    /// server).
    pub fn contact_etags(&self, account: &str) -> Result<Vec<(String, String)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached("SELECT url, etag FROM contacts WHERE account = ?1")?;
        let rows = stmt.query_map(params![account], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Stores what a CardDAV sync changed. Contacts of address books that
    /// no longer exist are removed too.
    pub fn apply_contacts(&self, account: &str, changes: &crate::carddav::Changes) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut put = tx.prepare_cached(
                "INSERT OR REPLACE INTO contacts
                 (account, url, book, etag, uid, name, emails, phones, org, note, vcard)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for (book, card) in &changes.cards {
                let c = &card.contact;
                put.execute(params![
                    account,
                    card.url,
                    book,
                    card.etag,
                    c.uid,
                    c.name,
                    c.emails.join("\n"),
                    c.phones.join("\n"),
                    c.organization,
                    c.note,
                    card.vcard
                ])?;
            }
            let mut del = tx.prepare_cached("DELETE FROM contacts WHERE account = ?1 AND url = ?2")?;
            for url in &changes.gone {
                del.execute(params![account, url])?;
            }
            let books: Vec<&str> = changes.books.iter().map(|b| b.url.as_str()).collect();
            let mut stmt = tx.prepare_cached("SELECT DISTINCT book FROM contacts WHERE account = ?1")?;
            let stored: Vec<String> = stmt
                .query_map(params![account], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for book in stored.iter().filter(|b| !books.contains(&b.as_str())) {
                tx.execute(
                    "DELETE FROM contacts WHERE account = ?1 AND book = ?2",
                    params![account, book],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Contacts whose name, address, organization or phone contains
    /// `query` (all of them when empty), by name.
    pub fn contacts(&self, account: &str, query: &str) -> Result<Vec<crate::vcard::Contact>> {
        let q = format!("%{}%", like_escape(&query.trim().to_lowercase()));
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT uid, name, emails, phones, org, note FROM contacts
             WHERE account = ?1
               AND (lower(name) LIKE ?2 ESCAPE '\\' OR lower(emails) LIKE ?2 ESCAPE '\\'
                    OR lower(org) LIKE ?2 ESCAPE '\\' OR phones LIKE ?2 ESCAPE '\\')
             ORDER BY name COLLATE NOCASE",
        )?;
        let lines =
            |s: String| -> Vec<String> { s.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect() };
        let rows = stmt.query_map(params![account, q], |r| {
            Ok(crate::vcard::Contact {
                uid: r.get(0)?,
                name: r.get(1)?,
                emails: lines(r.get(2)?),
                phones: lines(r.get(3)?),
                organization: r.get(4)?,
                note: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Item URL and ETag of every cached event (to compare with the server).
    pub fn event_etags(&self, account: &str) -> Result<Vec<(String, String)>> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached("SELECT url, etag FROM events WHERE account = ?1")?;
        let rows = stmt.query_map(params![account], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Stores what a CalDAV sync changed. The calendars list is replaced;
    /// events of calendars that no longer exist are removed.
    pub fn apply_events(&self, account: &str, changes: &crate::dav::Changes) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM calendars WHERE account = ?1", params![account])?;
        for c in &changes.collections {
            tx.execute(
                "INSERT OR REPLACE INTO calendars (account, url, name, color) VALUES (?1, ?2, ?3, ?4)",
                params![account, c.url, c.name, c.color],
            )?;
        }
        {
            let mut put = tx.prepare_cached(
                "INSERT OR REPLACE INTO events (account, url, calendar, etag, first_ts, last_ts, ics)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (calendar, item) in &changes.items {
                // Objects without events (tasks, journals) are not kept.
                let Some((first, last)) = crate::ical::span(&crate::ical::parse(&item.data)) else {
                    continue;
                };
                put.execute(params![
                    account, item.url, calendar, item.etag, first, last, item.data
                ])?;
            }
            let mut del = tx.prepare_cached("DELETE FROM events WHERE account = ?1 AND url = ?2")?;
            for url in &changes.gone {
                del.execute(params![account, url])?;
            }
        }
        tx.execute(
            "DELETE FROM events WHERE account = ?1
             AND calendar NOT IN (SELECT url FROM calendars WHERE account = ?1)",
            params![account],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// URL and ETag of the stored event with this UID, if any.
    pub fn event_by_uid(&self, account: &str, uid: &str) -> Result<Option<(String, String)>> {
        if uid.is_empty() {
            return Ok(None);
        }
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached("SELECT url, etag, ics FROM events WHERE account = ?1 AND instr(ics, ?2) > 0")?;
        let rows = stmt.query_map(params![account, uid], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (url, etag, ics) = row?;
            // The text matched somewhere: make sure it is the UID.
            if crate::ical::parse(&ics).events.iter().any(|e| e.uid == uid) {
                return Ok(Some((url, etag)));
            }
        }
        Ok(None)
    }

    pub fn note_invite_reply(&self, account: &str, uid: &str, partstat: &str) -> Result<()> {
        self.lock().execute(
            "INSERT OR REPLACE INTO invite_replies (account, uid, partstat) VALUES (?1, ?2, ?3)",
            params![account, uid, partstat],
        )?;
        Ok(())
    }

    /// Our answer to the invitation with this UID, if we gave one.
    pub fn invite_reply(&self, account: &str, uid: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT partstat FROM invite_replies WHERE account = ?1 AND uid = ?2",
                params![account, uid],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The account's calendars.
    pub fn calendars(&self, account: &str) -> Result<Vec<crate::dav::Collection>> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT url, name, color FROM calendars WHERE account = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![account], |r| {
            Ok(crate::dav::Collection {
                url: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Occurrences overlapping `[from, to)` (timestamps), with the URL of
    /// their calendar, sorted by start.
    pub fn occurrences(
        &self,
        account: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<(String, crate::ical::Occurrence)>> {
        let stored: Vec<(String, String)> = {
            let conn = self.lock();
            let mut stmt = conn.prepare_cached(
                "SELECT calendar, ics FROM events
                 WHERE account = ?1 AND first_ts < ?3 AND (last_ts IS NULL OR last_ts >= ?2)",
            )?;
            let rows = stmt.query_map(params![account, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut out: Vec<(String, crate::ical::Occurrence)> = stored
            .into_iter()
            .flat_map(|(calendar, ics)| {
                crate::ical::occurrences(&crate::ical::parse(&ics), from, to)
                    .into_iter()
                    .map(move |o| (calendar.clone(), o))
            })
            .collect();
        out.sort_by_key(|(_, o)| (o.start, !o.all_day, o.end));
        Ok(out)
    }

    /// Full-text search in one folder. Each word matches as a prefix,
    /// accents and case are ignored; all words must match.
    pub fn search(&self, account: &str, folder: &str, words: &[String], limit: u32) -> Result<Vec<Header>> {
        let Some(query) = fts_query(words) else {
            return Ok(Vec::new());
        };
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT m.uid, m.sender, m.subject, m.preview, m.ts, m.seen, m.attach, m.msgid, m.parents
             FROM messages_fts f JOIN messages m ON m.id = f.rowid
             WHERE messages_fts MATCH ?3 AND m.account = ?1 AND m.folder = ?2
             ORDER BY m.ts DESC LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![account, folder, query, limit], row_to_header)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

fn row_to_header(r: &rusqlite::Row<'_>) -> rusqlite::Result<Header> {
    Ok(Header {
        uid: r.get(0)?,
        from: r.get(1)?,
        subject: r.get(2)?,
        preview: r.get(3)?,
        timestamp: r.get(4)?,
        seen: r.get(5)?,
        has_attachment: r.get(6)?,
        text: String::new(),
        message_id: r.get(7)?,
        parents: r
            .get::<_, String>(8)?
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        ..Default::default()
    })
}

/// Counts a message in the address book: its sender as "received", or, for
/// messages we sent, its recipients as "sent".
fn harvest(conn: &Connection, account: &str, own: &str, h: &Header) -> Result<()> {
    let own = own.trim().to_lowercase();
    let from = h.from_address.trim().to_lowercase();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO addresses (account, address, name, received, sent, last_ts) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account, address) DO UPDATE SET
            received = received + excluded.received,
            sent = sent + excluded.sent,
            name = CASE WHEN excluded.name != '' THEN excluded.name ELSE addresses.name END,
            last_ts = MAX(last_ts, excluded.last_ts)",
    )?;
    if !own.is_empty() && from == own {
        for (name, addr) in &h.recipients {
            let addr = addr.trim().to_lowercase();
            if addr != own && addr.contains('@') {
                stmt.execute(params![account, addr, name, 0, 1, h.timestamp])?;
            }
        }
    } else if from.contains('@') {
        let name = if h.from.contains('@') { "" } else { h.from.as_str() };
        stmt.execute(params![account, from, name, 1, 0, h.timestamp])?;
    }
    Ok(())
}

/// Escapes LIKE wildcards (used with `ESCAPE '\\'`).
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Automated senders are never suggested as recipients.
const NO_REPLY: [&str; 5] = [
    "noreply",
    "no-reply",
    "donotreply",
    "do-not-reply",
    "nonrispondere",
];

/// Whether a table already has a column (for adding columns to old files).
pub(crate) fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Builds an FTS5 query from user words: each word quoted (so operators and
/// punctuation are literal) and used as a prefix.
fn fts_query(words: &[String]) -> Option<String> {
    let terms: Vec<String> = words
        .iter()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.trim().is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Cache key of an account (same address on two servers stays separate).
pub fn account_key(account: &crate::config::Account) -> String {
    format!("{}@{}", account.username, account.imap_host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(uid: u32, from: &str, subject: &str, text: &str) -> Header {
        Header {
            uid,
            from: from.into(),
            subject: subject.into(),
            preview: text.chars().take(20).collect(),
            timestamp: uid as i64,
            seen: false,
            has_attachment: false,
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn address_book_from_mail() {
        let c = Cache::in_memory();
        let mail = |uid: u32, from: &str, addr: &str, to: &[(&str, &str)]| Header {
            uid,
            from: from.into(),
            from_address: addr.into(),
            recipients: to.iter().map(|(n, a)| (n.to_string(), a.to_string())).collect(),
            timestamp: uid as i64,
            ..Default::default()
        };
        let list = [
            mail(1, "Ada Lovelace", "ada@x.org", &[]),
            mail(2, "Ada Lovelace", "ada@x.org", &[]),
            mail(3, "News", "noreply@shop.it", &[]),
            mail(4, "Me", "me@y.org", &[("Bob Smith", "bob@z.org")]),
        ];
        c.store_headers("a", "INBOX", &list, "me@y.org").unwrap();
        // Stored again (refresh): not counted twice.
        c.store_headers("a", "INBOX", &list, "me@y.org").unwrap();

        let names = |q: &str| c.suggest("a", q, 5).unwrap();
        assert_eq!(names("ada"), vec![("Ada Lovelace".into(), "ada@x.org".into())]);
        assert_eq!(names("smith"), vec![("Bob Smith".into(), "bob@z.org".into())]);
        assert!(names("noreply").is_empty(), "automated senders are skipped");
        assert!(names("me@").is_empty(), "our own address is not stored");

        // People we write to come before frequent senders.
        c.note_sent("a", &[("".into(), "ada2@x.org".into())], 9).unwrap();
        c.note_sent("a", &[("".into(), "ada2@x.org".into())], 10).unwrap();
        assert_eq!(names("ada")[0].1, "ada2@x.org");
    }

    #[test]
    fn store_search_prune() {
        let c = Cache::in_memory();
        c.check_uidvalidity("a", "INBOX", 1).unwrap();
        c.store_headers(
            "a",
            "INBOX",
            &[
                h(
                    1,
                    "Giulia Ferri",
                    "Preventivo portale",
                    "Ciao Fabio, ecco il preventivo aggiornato",
                ),
                h(2, "Banca", "Estratto conto", "Il tuo estratto conto di ottobre"),
                h(3, "Città di Roma", "Avviso", "Comunicazione importante"),
            ],
            "",
        )
        .unwrap();
        let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(c.search("a", "INBOX", &words("preven"), 10).unwrap().len(), 1);
        assert_eq!(
            c.search("a", "INBOX", &words("estratto ottobre"), 10).unwrap()[0].uid,
            2
        );
        // Accents ignored.
        assert_eq!(c.search("a", "INBOX", &words("citta"), 10).unwrap()[0].uid, 3);
        // Operators are literal.
        assert!(
            c.search("a", "INBOX", &words("\"OR NEAR("), 10)
                .unwrap()
                .is_empty()
        );
        // Full body replaces the snippet and stays on later header refreshes.
        c.store_body("a", "INBOX", 2, "testo completo con parola rara zibaldone")
            .unwrap();
        c.store_headers("a", "INBOX", &[h(2, "Banca", "Estratto conto", "snippet")], "")
            .unwrap();
        assert_eq!(c.search("a", "INBOX", &words("zibaldone"), 10).unwrap().len(), 1);
        // Prune removes messages gone from the server.
        c.prune("a", "INBOX", 1, &[1, 3]).unwrap();
        assert_eq!(c.headers("a", "INBOX", 10).unwrap().len(), 2);
        assert!(
            c.search("a", "INBOX", &words("zibaldone"), 10)
                .unwrap()
                .is_empty()
        );
        // UIDVALIDITY change empties the folder.
        c.check_uidvalidity("a", "INBOX", 2).unwrap();
        assert!(c.headers("a", "INBOX", 10).unwrap().is_empty());
    }

    #[test]
    fn contacts_are_stored_searched_and_suggested_first() {
        use crate::carddav::{Book, Card, Changes};
        let c = Cache::in_memory();
        let card = |url: &str, name: &str, email: &str| {
            (
                "https://d/book/".to_owned(),
                Card {
                    url: url.into(),
                    etag: "1".into(),
                    vcard: String::new(),
                    contact: crate::vcard::Contact {
                        name: name.into(),
                        emails: vec![email.into()],
                        ..Default::default()
                    },
                },
            )
        };
        let books = vec![Book {
            url: "https://d/book/".into(),
            name: "B".into(),
            ..Default::default()
        }];
        c.apply_contacts(
            "acc",
            &Changes {
                books: books.clone(),
                cards: vec![
                    card("u1", "Marta Neri", "marta@x.it"),
                    card("u2", "Luca Blu", "luca@y.it"),
                ],
                gone: vec![],
            },
        )
        .unwrap();
        // A frequent correspondent with the same prefix comes after the contact.
        let h = Header {
            from: "Mario".into(),
            from_address: "mario@z.it".into(),
            timestamp: 5,
            ..Default::default()
        };
        c.store_address_book("acc", "me@x.it", &h).unwrap();
        let s = c.suggest("acc", "mar", 8).unwrap();
        assert_eq!(s[0], ("Marta Neri".into(), "marta@x.it".into()));
        assert_eq!(s[1].1, "mario@z.it");
        assert_eq!(c.contacts("acc", "y.it").unwrap()[0].name, "Luca Blu");
        assert_eq!(c.contact_etags("acc").unwrap().len(), 2);

        // Deleted card, then the whole book gone.
        c.apply_contacts(
            "acc",
            &Changes {
                books,
                cards: vec![],
                gone: vec!["u2".into()],
            },
        )
        .unwrap();
        assert_eq!(c.contacts("acc", "").unwrap().len(), 1);
        c.apply_contacts("acc", &Changes::default()).unwrap();
        assert!(c.contacts("acc", "").unwrap().is_empty());
    }

    #[test]
    fn events_are_stored_and_expanded_in_a_range() {
        use crate::dav::{Changes, Collection, Item};
        let c = Cache::in_memory();
        let ics = |uid: &str, rule: &str| {
            format!(
                "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:{uid}\nSUMMARY:{uid}\nDTSTART:20261102T090000Z\nDTEND:20261102T100000Z\n{rule}END:VEVENT\nEND:VCALENDAR\n"
            )
        };
        let item = |url: &str, data: String| {
            (
                "https://d/cal/".to_owned(),
                Item {
                    url: url.into(),
                    etag: "1".into(),
                    data,
                },
            )
        };
        let cals = vec![Collection {
            url: "https://d/cal/".into(),
            name: "Lavoro".into(),
            color: "#A63A0CFF".into(),
        }];
        c.apply_events(
            "acc",
            &Changes {
                collections: cals.clone(),
                items: vec![
                    item("u1", ics("once", "")),
                    item("u2", ics("daily", "RRULE:FREQ=DAILY\n")),
                    item(
                        "u3",
                        "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:t\nEND:VTODO\nEND:VCALENDAR\n".into(),
                    ),
                ],
                gone: vec![],
            },
        )
        .unwrap();
        assert_eq!(c.event_etags("acc").unwrap().len(), 2, "tasks are not kept");
        // 2-4 Nov: the one-off on the 2nd plus three daily occurrences.
        let day = 86_400;
        let from = 1_793_577_600; // 2026-11-02T00:00:00Z
        let list = c.occurrences("acc", from, from + 3 * day).unwrap();
        assert_eq!(list.len(), 4);
        // A year later only the endless series remains.
        let later = c.occurrences("acc", from + 365 * day, from + 366 * day).unwrap();
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].1.summary, "daily");
        assert_eq!(c.calendars("acc").unwrap(), cals);

        c.apply_events("acc", &Changes::default()).unwrap();
        assert!(c.event_etags("acc").unwrap().is_empty(), "calendar removed");
    }
}
