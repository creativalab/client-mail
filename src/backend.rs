//! Bridge between the UI (async, iced) and IMAP/SMTP (blocking).
//! A single shared IMAP session, guarded by a mutex; if the connection
//! drops, the session is recreated and the operation retried once.

use crate::cache;
use crate::config::Account;
use crate::i18n::{generic_error, t};
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
    pub async fn test(account: Account) -> Res<usize> {
        tokio::task::spawn_blocking(move || {
            let mut s = mail::connect(&account).map_err(text)?;
            let n = mail::list_folders(&mut s).map_err(text)?.len();
            let _ = s.logout();
            Ok(n)
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
            let page = b.with_session_sync(|s| mail::fetch_headers(s, &folder))?;
            b.cache(|c, key| {
                if let Some(v) = page.uidvalidity {
                    c.check_uidvalidity(key, &folder, v)?;
                }
                c.store_headers(key, &folder, &page.headers)?;
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
            if before <= 1 {
                return Ok(None);
            }
            let page = b.with_session_sync(|s| mail::fetch_older(s, &folder, before, count))?;
            let mut done = page.first_seq <= 1;
            b.cache(|c, key| {
                c.store_headers(key, &folder, &page.headers)?;
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
        cache::get()
            .and_then(|c| {
                c.headers(&cache::account_key(account), folder, mail::PAGE_SIZE)
                    .ok()
            })
            .unwrap_or_default()
    }

    /// Full-text search in the cached messages of a folder.
    pub fn search(account: &Account, folder: &str, words: &[String]) -> Vec<Header> {
        cache::get()
            .and_then(|c| c.search(&cache::account_key(account), folder, words, 500).ok())
            .unwrap_or_default()
    }

    pub async fn unseen(&self, folders: Vec<String>) -> Res<HashMap<String, u32>> {
        self.blocking(move |b| b.with_session_sync(|s| Ok(mail::unseen_counts(s, &folders))))
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
            let msg = b.with_session_sync(|s| mail::fetch_message(s, &folder, uid, &keys))?;
            b.cache(|c, key| c.store_body(key, &folder, uid, &msg.body));
            Ok(msg)
        })
        .await
    }

    pub async fn set_seen(&self, folder: String, uid: u32, seen: bool) -> Res<()> {
        self.blocking(move |b| {
            b.with_session_sync(|s| mail::set_seen(s, &folder, uid, seen))?;
            b.cache(|c, key| c.set_seen(key, &folder, uid, seen));
            Ok(())
        })
        .await
    }

    pub async fn move_to(&self, folder: String, uid: u32, target: String) -> Res<()> {
        self.blocking(move |b| {
            b.with_session_sync(|s| mail::move_message(s, &folder, uid, &target))?;
            b.cache(|c, key| c.remove(key, &folder, uid));
            Ok(())
        })
        .await
    }

    /// Archive: moves to the Archive folder, creating it if missing.
    /// Returns the name of the folder used.
    pub async fn archive(&self, folder: String, uid: u32, folders: Vec<Folder>) -> Res<String> {
        self.blocking(move |b| {
            b.with_session_sync(|s| {
                let target = match folders.iter().find(|f| f.kind == FolderKind::Archive) {
                    Some(f) => f.name.clone(),
                    None => mail::create_folder(s, &folders, "Archive")?,
                };
                mail::move_message(s, &folder, uid, &target)?;
                Ok(target)
            })
            .inspect(|_| b.cache(|c, key| c.remove(key, &folder, uid)))
        })
        .await
    }

    pub async fn delete_forever(&self, folder: String, uid: u32) -> Res<()> {
        self.blocking(move |b| {
            b.with_session_sync(|s| mail::delete_permanently(s, &folder, uid))?;
            b.cache(|c, key| c.remove(key, &folder, uid));
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
