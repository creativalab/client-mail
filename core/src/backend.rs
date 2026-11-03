//! Bridge between the UI (async, iced) and IMAP/SMTP (blocking).
//! A single shared IMAP session, guarded by a mutex; if the connection
//! drops, the session is recreated and the operation retried once.

use crate::cache;
use crate::config::Account;
use crate::i18n::{generic_error, t};
use crate::local;
use crate::mail::{self, Folder, FolderKind, FullMessage, Header, ImapSession, Outgoing};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Default)]
struct Inner {
    account: Option<Account>,
    session: Option<ImapSession>,
}

#[derive(Clone, Default)]
pub struct Backend {
    inner: Arc<Mutex<Inner>>,
}

/// One occurrence of an event, with its calendar's name and color.
#[derive(Debug, Clone, PartialEq)]
pub struct AgendaEntry {
    pub occurrence: crate::ical::Occurrence,
    pub calendar: String,
    /// "#RRGGBB" or "#RRGGBBAA" (empty when the server sets none).
    pub color: String,
}

/// Error as text: iced messages must be `Clone`.
pub type Res<T> = Result<T, String>;

fn text(e: anyhow::Error) -> String {
    format!("{e:#}")
}

const MAX_THROTTLE_RETRIES: u32 = 3;

/// If the server refused because of rate limits, how long to wait.
///
/// Exchange Online: "Request is throttled. Suggested Backoff Time: N
/// milliseconds"; others use [UNAVAILABLE], "too many connections", etc.
fn throttle_delay(e: &anyhow::Error) -> Option<std::time::Duration> {
    use std::time::Duration;
    let msg = format!("{e:#}").to_lowercase();
    let throttled = [
        "throttl",
        "[unavailable]",
        "too many",
        "rate limit",
        "try again later",
    ]
    .iter()
    .any(|k| msg.contains(k));
    if !throttled {
        return None;
    }
    let suggested = msg
        .split("backoff time:")
        .nth(1)
        .and_then(|rest| {
            rest.split_whitespace()
                .next()?
                .trim_end_matches('.')
                .parse::<u64>()
                .ok()
        })
        .map(Duration::from_millis);
    Some(
        suggested
            .unwrap_or(Duration::from_secs(5))
            .clamp(Duration::from_millis(500), Duration::from_secs(30)),
    )
}

fn is_connection_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<imap::Error>()
            .is_some_and(|ie| matches!(ie, imap::Error::Io(_) | imap::Error::ConnectionLost))
            || c.downcast_ref::<std::io::Error>().is_some()
    })
}

impl Backend {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The connected account's address (empty if none).
    fn own_address(&self) -> String {
        self.lock()
            .account
            .as_ref()
            .map(|a| a.email.clone())
            .unwrap_or_default()
    }

    /// Address suggestions for To/Cc from the account's mail.
    pub fn suggest(account: &Account, query: &str) -> Vec<(String, String)> {
        cache::get()
            .and_then(|c| c.suggest(&cache::account_key(account), query, 8).ok())
            .unwrap_or_default()
    }

    /// Saved contacts matching `query` (all when empty), by name.
    pub fn contacts(account: &Account, query: &str) -> Vec<crate::vcard::Contact> {
        cache::get()
            .and_then(|c| c.contacts(&cache::account_key(account), query).ok())
            .unwrap_or_default()
    }

    /// Brings the cached contacts up to date with the account's CardDAV
    /// server. Returns how many contacts are saved.
    pub async fn sync_contacts(account: Account) -> Res<usize> {
        tokio::task::spawn_blocking(move || {
            let cache = cache::get().ok_or_else(generic_error)?;
            let key = cache::account_key(&account);
            let known = cache.contact_etags(&key).map_err(text)?;
            let changes = crate::carddav::sync(&account, &known).map_err(text)?;
            cache.apply_contacts(&key, &changes).map_err(text)?;
            Ok(cache.contacts(&key, "").map_err(text)?.len())
        })
        .await
        .map_err(|e| format!("{}\n({e})", generic_error()))?
    }

    /// Brings the cached calendars up to date with the account's CalDAV
    /// server. Returns how many calendars there are.
    pub async fn sync_calendar(account: Account) -> Res<usize> {
        tokio::task::spawn_blocking(move || {
            let cache = cache::get().ok_or_else(generic_error)?;
            let key = cache::account_key(&account);
            let known = cache.event_etags(&key).map_err(text)?;
            let changes = crate::dav::Client::new(&account, crate::dav::Kind::Calendar)
                .sync(&account, &known)
                .map_err(text)?;
            cache.apply_events(&key, &changes).map_err(text)?;
            Ok(changes.collections.len())
        })
        .await
        .map_err(|e| format!("{}\n({e})", generic_error()))?
    }

    /// Event occurrences overlapping `[from, to)` (timestamps), sorted, from
    /// the cached calendars.
    pub fn agenda(account: &Account, from: i64, to: i64) -> Vec<AgendaEntry> {
        let Some(cache) = cache::get() else {
            return Vec::new();
        };
        let key = cache::account_key(account);
        let calendars = cache.calendars(&key).unwrap_or_default();
        cache
            .occurrences(&key, from, to)
            .unwrap_or_default()
            .into_iter()
            .map(|(url, occurrence)| {
                let cal = calendars.iter().find(|c| c.url == url);
                AgendaEntry {
                    calendar: cal.map(|c| c.name.clone()).unwrap_or_default(),
                    color: cal.map(|c| c.color.clone()).unwrap_or_default(),
                    occurrence,
                }
            })
            .collect()
    }

    /// Our earlier answer to an invitation (by event UID), if any.
    pub fn invitation_reply(account: &Account, uid: &str) -> Option<crate::invite::Reply> {
        cache::get()
            .and_then(|c| c.invite_reply(&cache::account_key(account), uid).ok().flatten())
            .and_then(|p| crate::invite::Reply::from_partstat(&p))
    }

    /// Whether the event is in the cached calendar (to offer removing it
    /// when cancelled).
    pub fn in_calendar(account: &Account, uid: &str) -> bool {
        cache::get()
            .and_then(|c| c.event_by_uid(&cache::account_key(account), uid).ok().flatten())
            .is_some()
    }

    /// Answers an invitation: mails the reply to the organizer (copy in
    /// Sent), remembers it, and stores the event in the calendar (removes
    /// it when declined) if the account has a calendar server. Returns the
    /// text to show.
    pub async fn respond_invitation(
        &self,
        ics: String,
        reply: crate::invite::Reply,
        sent_folder: Option<String>,
    ) -> Res<String> {
        let account = self
            .lock()
            .account
            .clone()
            .ok_or_else(|| t("err_no_account").to_string())?;
        let mail = crate::invite::reply_mail(&account, &ics, reply).map_err(text)?;
        let warning = self.send(mail, sent_folder).await?;
        let uid = crate::invite::read(&ics).map(|i| i.uid).unwrap_or_default();
        self.cache(|c, key| c.note_invite_reply(key, &uid, reply.partstat()));
        let saved = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let cache = cache::get().ok_or_else(|| anyhow::anyhow!(generic_error()))?;
            let key = cache::account_key(&account);
            let existing = cache.event_by_uid(&key, &uid)?;
            crate::invite::save_to_calendar(&account, &ics, reply, existing)?;
            // Refresh the local copy so the calendar shows the change.
            let known = cache.event_etags(&key)?;
            let changes =
                crate::dav::Client::new(&account, crate::dav::Kind::Calendar).sync(&account, &known)?;
            cache.apply_events(&key, &changes)
        })
        .await
        .map_err(|e| format!("{}\n({e})", generic_error()))?;
        let mut notice = match saved {
            Ok(()) if reply != crate::invite::Reply::Declined => t("invite_saved").to_string(),
            Ok(()) => t("invite_sent").to_string(),
            // No calendar server: the reply is all there is to do.
            Err(e) if e.to_string() == t("err_caldav_none") => t("invite_sent").to_string(),
            Err(e) => crate::i18n::tf("invite_not_saved", &[("err", &text(e))]),
        };
        if let Some(w) = warning {
            notice = format!("{notice}\n{}", crate::i18n::tf("sent_no_copy", &[("err", &w)]));
        }
        Ok(notice)
    }

    /// Removes a cancelled event from the calendar.
    pub async fn remove_cancelled(account: Account, uid: String) -> Res<String> {
        tokio::task::spawn_blocking(move || {
            let cache = cache::get().ok_or_else(generic_error)?;
            let key = cache::account_key(&account);
            if let Some((url, etag)) = cache.event_by_uid(&key, &uid).map_err(text)? {
                crate::invite::remove_from_calendar(&account, &url, &etag).map_err(text)?;
                let changes = crate::dav::Changes {
                    collections: cache.calendars(&key).map_err(text)?,
                    gone: vec![url],
                    ..Default::default()
                };
                cache.apply_events(&key, &changes).map_err(text)?;
            }
            Ok(t("invite_removed").to_string())
        })
        .await
        .map_err(|e| format!("{}\n({e})", generic_error()))?
    }

    /// The connected account, if it reads mail with POP3.
    fn pop3_account(&self) -> Option<Account> {
        self.lock().account.clone().filter(Account::is_pop3)
    }

    async fn blocking<T, F>(&self, f: F) -> Res<T>
    where
        T: Send + 'static,
        F: FnOnce(&Backend) -> Res<T> + Send + 'static,
    {
        let me = self.clone();
        // An error here means the thread panicked: internal error.
        tokio::task::spawn_blocking(move || f(&me))
            .await
            .map_err(|e| format!("{}\n({e})", generic_error()))?
    }

    fn with_session_sync<T>(&self, op: impl Fn(&mut ImapSession) -> anyhow::Result<T>) -> Res<T> {
        let mut g = self.lock();
        let account = g.account.clone().ok_or_else(|| t("err_no_account").to_string())?;
        let mut attempt = 0u32;
        loop {
            let result: anyhow::Result<T> = (|| {
                if let Some(s) = g.session.as_mut() {
                    match op(s) {
                        Ok(v) => return Ok(v),
                        Err(e) if !is_connection_error(&e) => return Err(e),
                        Err(_) => g.session = None,
                    }
                }
                let mut session = mail::connect(&account)?;
                let result = op(&mut session);
                g.session = Some(session);
                result
            })();
            match result {
                Ok(v) => return Ok(v),
                // The server asked to slow down: wait and try again a few times.
                Err(e) => match throttle_delay(&e) {
                    Some(delay) if attempt < MAX_THROTTLE_RETRIES => {
                        attempt += 1;
                        std::thread::sleep(delay * attempt);
                    }
                    _ => return Err(text(e)),
                },
            }
        }
    }

    /// Connects an account (replaces any previous session).
    pub async fn connect(&self, account: Account) -> Res<Vec<Folder>> {
        self.blocking(move |b| {
            if account.is_pop3() {
                // Checks the login; the mailbox itself is local.
                crate::pop3::Pop3::connect(&account)
                    .and_then(|p| p.quit())
                    .map_err(text)?;
                let mut g = b.lock();
                if let Some(mut old) = g.session.take() {
                    let _ = old.logout();
                }
                g.account = Some(account);
                return Ok(local::folders());
            }
            let mut session = mail::connect(&account).map_err(text)?;
            let folders = mail::list_folders(&mut session).map_err(text)?;
            let mut g = b.lock();
            if let Some(mut old) = g.session.take() {
                let _ = old.logout();
            }
            g.account = Some(account);
            g.session = Some(session);
            Ok(folders)
        })
        .await
    }

    /// Tests the credentials without touching the current session.
    /// Returns the message to show.
    pub async fn test(account: Account) -> Res<String> {
        tokio::task::spawn_blocking(move || {
            if account.is_pop3() {
                let mut p = crate::pop3::Pop3::connect(&account).map_err(text)?;
                let n = p.uidl().map_err(text)?.len();
                let _ = p.quit();
                return Ok(crate::i18n::tf("test_ok_pop3", &[("n", &n.to_string())]));
            }
            let mut s = mail::connect(&account).map_err(text)?;
            let n = mail::list_folders(&mut s).map_err(text)?.len();
            let _ = s.logout();
            Ok(crate::i18n::tf("test_ok", &[("n", &n.to_string())]))
        })
        .await
        .map_err(|e| format!("{}\n({e})", generic_error()))?
    }

    /// Cache key of the connected account.
    fn key(&self) -> Option<String> {
        self.lock().account.as_ref().map(cache::account_key)
    }

    /// Runs a cache update; failures only disable that update (the cache is
    /// an optimization, the server is the source of truth).
    fn cache(&self, f: impl FnOnce(&cache::Cache, &str) -> anyhow::Result<()>) {
        if let (Some(c), Some(key)) = (cache::get(), self.key())
            && let Err(e) = f(c, &key)
        {
            eprintln!("cache: {e:#}");
        }
    }

    /// Newest page of a folder; also refreshes the cache. Returns the
    /// sequence number to continue indexing from (see `index_older`).
    pub async fn headers(&self, folder: String) -> Res<(Vec<Header>, u32)> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                // Newest messages first; the rest continues in `index_older`.
                let left = pop3_sync(&account, POP3_FIRST_BATCH).map_err(text)?;
                return Ok((local_headers(&account, &folder), if left > 0 { 2 } else { 0 }));
            }
            let page = b.with_session_sync(|s| mail::fetch_headers(s, &folder))?;
            let own = b.own_address();
            b.cache(|c, key| {
                if let Some(v) = page.uidvalidity {
                    c.check_uidvalidity(key, &folder, v)?;
                }
                c.store_headers(key, &folder, &page.headers, &own)?;
                let uids: Vec<u32> = page.headers.iter().map(|h| h.uid).collect();
                if let Some(min) = uids.iter().min() {
                    c.prune(key, &folder, *min, &uids)?;
                }
                Ok(())
            });
            Ok((page.headers, page.first_seq))
        })
        .await
    }

    /// Indexes up to `count` messages older than sequence `before` into the
    /// cache. Returns where to continue, or `None` when the folder is done
    /// (or the cache already holds as many messages as the server).
    pub async fn index_older(&self, folder: String, before: u32, count: u32) -> Res<Option<u32>> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                let left = pop3_sync(&account, count as usize).map_err(text)?;
                return Ok((left > 0).then_some(2));
            }
            if before <= 1 {
                return Ok(None);
            }
            let page = b.with_session_sync(|s| mail::fetch_older(s, &folder, before, count))?;
            let own = b.own_address();
            let mut done = page.first_seq <= 1;
            b.cache(|c, key| {
                c.store_headers(key, &folder, &page.headers, &own)?;
                let (cached, _) = c.folder_stats(key, &folder)?;
                done |= cached >= page.exists;
                Ok(())
            });
            Ok((!done).then_some(page.first_seq))
        })
        .await
    }

    /// Cached list of a folder (instant, before the server answers).
    pub fn cached_headers(account: &Account, folder: &str) -> Vec<Header> {
        if account.is_pop3() {
            return local_headers(account, folder);
        }
        cache::get()
            .and_then(|c| {
                c.headers(&cache::account_key(account), folder, mail::PAGE_SIZE)
                    .ok()
            })
            .unwrap_or_default()
    }

    /// Full-text search in the cached messages of a folder.
    pub fn search(account: &Account, folder: &str, words: &[String]) -> Vec<Header> {
        if account.is_pop3() {
            return local::get()
                .and_then(|s| s.search(&cache::account_key(account), folder, words, 500).ok())
                .unwrap_or_default();
        }
        cache::get()
            .and_then(|c| c.search(&cache::account_key(account), folder, words, 500).ok())
            .unwrap_or_default()
    }

    pub async fn unseen(&self, folders: Vec<String>) -> Res<HashMap<String, u32>> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                let key = cache::account_key(&account);
                let store = local::get().ok_or_else(generic_error)?;
                return Ok(folders
                    .into_iter()
                    .filter_map(|f| Some((f.clone(), store.unseen(&key, &f).ok()?)))
                    .collect());
            }
            b.with_session_sync(|s| Ok(mail::unseen_counts(s, &folders)))
        })
        .await
    }

    pub async fn message(&self, folder: String, uid: u32) -> Res<FullMessage> {
        self.blocking(move |b| {
            let account = b.lock().account.clone();
            let own = account
                .as_ref()
                .and_then(|a| crate::crypto::own_key(&a.email).map(|k| (k, a.pgp_passphrase.clone())));
            let keys = mail::PgpKeys {
                own,
                contact: &crate::crypto::contact_key,
            };
            if let Some(account) = b.pop3_account() {
                let raw = local_op(&account, |s, key| s.raw(key, uid))?
                    .ok_or_else(|| t("err_not_found").to_string())?;
                let msg = mail::parse_full(uid, &raw, &keys).map_err(text)?;
                // Opening reads it, as an IMAP fetch of the body does.
                local_op(&account, |s, key| s.set_seen(key, uid, true))?;
                return Ok(msg);
            }
            let msg = b.with_session_sync(|s| mail::fetch_message(s, &folder, uid, &keys))?;
            b.cache(|c, key| c.store_body(key, &folder, uid, &msg.body));
            Ok(msg)
        })
        .await
    }

    pub async fn set_seen(&self, folder: String, uid: u32, seen: bool) -> Res<()> {
        self.set_seen_many(folder, vec![uid], seen).await
    }

    /// Marks several messages read or unread with one server command.
    pub async fn set_seen_many(&self, folder: String, uids: Vec<u32>, seen: bool) -> Res<()> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                return local_op(&account, |s, key| {
                    uids.iter().try_for_each(|&u| s.set_seen(key, u, seen))
                });
            }
            b.with_session_sync(|s| mail::set_seen(s, &folder, &uids, seen))?;
            b.cache(|c, key| uids.iter().try_for_each(|&u| c.set_seen(key, &folder, u, seen)));
            Ok(())
        })
        .await
    }

    pub async fn move_to(&self, folder: String, uid: u32, target: String) -> Res<()> {
        self.move_many(folder, vec![uid], target).await
    }

    /// Moves several messages with one server command.
    pub async fn move_many(&self, folder: String, uids: Vec<u32>, target: String) -> Res<()> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                return local_op(&account, |s, key| {
                    uids.iter().try_for_each(|&u| s.move_to(key, u, &target))
                });
            }
            b.with_session_sync(|s| mail::move_message(s, &folder, &uids, &target))?;
            b.cache(|c, key| uids.iter().try_for_each(|&u| c.remove(key, &folder, u)));
            Ok(())
        })
        .await
    }

    /// Archive: moves to the Archive folder, creating it if missing.
    /// Returns the name of the folder used.
    pub async fn archive(&self, folder: String, uid: u32, folders: Vec<Folder>) -> Res<String> {
        self.archive_many(folder, vec![uid], folders).await
    }

    pub async fn archive_many(&self, folder: String, uids: Vec<u32>, folders: Vec<Folder>) -> Res<String> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                return local_op(&account, |s, key| {
                    uids.iter().try_for_each(|&u| s.move_to(key, u, local::ARCHIVE))
                })
                .map(|_| local::ARCHIVE.to_string());
            }
            b.with_session_sync(|s| {
                let target = match folders.iter().find(|f| f.kind == FolderKind::Archive) {
                    Some(f) => f.name.clone(),
                    None => mail::create_folder(s, &folders, "Archive")?,
                };
                mail::move_message(s, &folder, &uids, &target)?;
                Ok(target)
            })
            .inspect(|_| b.cache(|c, key| uids.iter().try_for_each(|&u| c.remove(key, &folder, u))))
        })
        .await
    }

    pub async fn delete_forever(&self, folder: String, uid: u32) -> Res<()> {
        self.delete_forever_many(folder, vec![uid]).await
    }

    pub async fn delete_forever_many(&self, folder: String, uids: Vec<u32>) -> Res<()> {
        self.blocking(move |b| {
            if let Some(account) = b.pop3_account() {
                return local_op(&account, |s, key| uids.iter().try_for_each(|&u| s.delete(key, u)));
            }
            b.with_session_sync(|s| mail::delete_permanently(s, &folder, &uids))?;
            b.cache(|c, key| uids.iter().try_for_each(|&u| c.remove(key, &folder, u)));
            Ok(())
        })
        .await
    }

    /// Sends and saves a copy to Sent. `Ok(Some(err))` = sent, copy not saved.
    pub async fn send(&self, mail: Outgoing, sent_folder: Option<String>) -> Res<Option<String>> {
        self.blocking(move |b| {
            let account = b
                .lock()
                .account
                .clone()
                .ok_or_else(|| t("err_no_account").to_string())?;
            let raw = mail::send(&account, &mail).map_err(text)?;
            let to: Vec<(String, String)> = [&mail.to, &mail.cc, &mail.bcc]
                .iter()
                .flat_map(|f| mail::parse_recipients(f))
                .collect();
            b.cache(|c, key| c.note_sent(key, &to, chrono::Utc::now().timestamp()));
            if account.is_pop3() {
                // Sent copy in the local mailbox.
                return Ok(local_op(&account, |s, key| s.insert(key, local::SENT, &raw, true)).err());
            }
            let Some(folder) = sent_folder else {
                return Ok(None);
            };
            match b.with_session_sync(|s| mail::append_sent(s, &folder, &raw)) {
                Ok(()) => Ok(None),
                Err(e) => Ok(Some(e)),
            }
        })
        .await
    }
}

// ---------------------------------------------------------------- POP3

/// Runs an operation on the local mailbox of a POP3 account.
fn local_op<T>(account: &Account, f: impl FnOnce(&local::Store, &str) -> anyhow::Result<T>) -> Res<T> {
    let store = local::get().ok_or_else(generic_error)?;
    f(store, &cache::account_key(account)).map_err(text)
}

fn local_headers(account: &Account, folder: &str) -> Vec<Header> {
    local_op(account, |s, key| s.headers(key, folder, 2000)).unwrap_or_default()
}

/// New POP3 messages downloaded before the list is shown.
const POP3_FIRST_BATCH: usize = 40;

/// Downloads up to `max` new messages (newest first) into the local inbox
/// and deletes from the server those kept longer than the account allows.
/// Returns how many new messages are still to download.
fn pop3_sync(account: &Account, max: usize) -> anyhow::Result<usize> {
    // POP3 servers lock the mailbox: one session at a time.
    static RUNNING: Mutex<()> = Mutex::new(());
    let _one = RUNNING.lock().unwrap_or_else(|p| p.into_inner());

    let store = local::get().ok_or_else(|| anyhow::anyhow!(generic_error()))?;
    let key = cache::account_key(account);
    let mut pop = crate::pop3::Pop3::connect(account)?;
    let listing = pop.uidl()?;
    let mut known = store.downloaded(&key)?;

    // Higher numbers are newer messages.
    let pending: Vec<&crate::pop3::Listing> = listing
        .iter()
        .rev()
        .filter(|l| !known.contains_key(&l.uidl))
        .collect();
    for item in pending.iter().take(max) {
        let raw = pop.retr(item.number)?;
        store.add_downloaded(&key, &item.uidl, &raw)?;
        if let Some(c) = cache::get() {
            let h = mail::header_from_raw(0, &raw, false);
            let _ = c.store_address_book(&key, &account.email, &h);
        }
        known.insert(item.uidl.clone(), local::now());
    }
    let left = pending.len().saturating_sub(max);

    if let Some(days) = account.pop3_keep_days {
        let limit = i64::from(days) * 86_400;
        let now = local::now();
        for item in &listing {
            if known.get(&item.uidl).is_some_and(|at| now - at >= limit) {
                pop.dele(item.number)?;
            }
        }
    }
    pop.quit()?;

    // Forget what is no longer on the server (deleted here or elsewhere).
    let on_server: std::collections::HashSet<&str> = listing.iter().map(|l| l.uidl.as_str()).collect();
    let gone: Vec<String> = known
        .into_keys()
        .filter(|u| !on_server.contains(u.as_str()))
        .collect();
    store.forget_downloaded(&key, &gone)?;
    Ok(left)
}

#[cfg(test)]
mod tests {
    use super::throttle_delay;
    use std::time::Duration;

    #[test]
    fn recognizes_throttling() {
        let e =
            anyhow::anyhow!("Bad Response: Request is throttled. Suggested Backoff Time: 2453 milliseconds");
        assert_eq!(throttle_delay(&e), Some(Duration::from_millis(2453)));
        let e = anyhow::anyhow!("NO [UNAVAILABLE] Server busy, try again later");
        assert_eq!(throttle_delay(&e), Some(Duration::from_secs(5)));
        let e = anyhow::anyhow!("Request is throttled. Suggested Backoff Time: 999999 milliseconds");
        assert_eq!(throttle_delay(&e), Some(Duration::from_secs(30)));
        assert_eq!(
            throttle_delay(&anyhow::anyhow!("NO [AUTHENTICATIONFAILED] Invalid credentials")),
            None
        );
    }
}
