//! Bridge between the UI (async, iced) and IMAP/SMTP (blocking).
//! A single shared IMAP session, guarded by a mutex; if the connection
//! drops, the session is recreated and the operation retried once.

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

fn is_connection_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<imap::Error>().is_some_and(|ie| {
            matches!(ie, imap::Error::Io(_) | imap::Error::ConnectionLost)
        }) || c.downcast_ref::<std::io::Error>().is_some()
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
        if let Some(s) = g.session.as_mut() {
            match op(s) {
                Ok(v) => return Ok(v),
                Err(e) if !is_connection_error(&e) => return Err(text(e)),
                Err(_) => g.session = None,
            }
        }
        let mut session = mail::connect(&account).map_err(text)?;
        let result = op(&mut session).map_err(text);
        g.session = Some(session);
        result
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

    pub async fn headers(&self, folder: String) -> Res<Vec<Header>> {
        self.blocking(move |b| b.with_session_sync(|s| mail::fetch_headers(s, &folder)))
            .await
    }

    pub async fn unseen(&self, folders: Vec<String>) -> Res<HashMap<String, u32>> {
        self.blocking(move |b| b.with_session_sync(|s| Ok(mail::unseen_counts(s, &folders))))
            .await
    }

    pub async fn message(&self, folder: String, uid: u32) -> Res<FullMessage> {
        self.blocking(move |b| b.with_session_sync(|s| mail::fetch_message(s, &folder, uid)))
            .await
    }

    pub async fn set_seen(&self, folder: String, uid: u32, seen: bool) -> Res<()> {
        self.blocking(move |b| b.with_session_sync(|s| mail::set_seen(s, &folder, uid, seen)))
            .await
    }

    pub async fn move_to(&self, folder: String, uid: u32, target: String) -> Res<()> {
        self.blocking(move |b| {
            b.with_session_sync(|s| mail::move_message(s, &folder, uid, &target))
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
        })
        .await
    }

    pub async fn delete_forever(&self, folder: String, uid: u32) -> Res<()> {
        self.blocking(move |b| b.with_session_sync(|s| mail::delete_permanently(s, &folder, uid)))
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
