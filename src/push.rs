//! Push: a second IMAP connection kept in IDLE on the inbox. When the
//! server reports a change, the app checks for new mail right away instead
//! of waiting for the periodic refresh.
//!
//! Servers without IDLE are left to the periodic refresh.

use crate::app::Message;
use crate::config::Account;
use crate::mail;
use iced::futures::SinkExt;
use iced::{Subscription, futures::Stream};
use std::hash::{Hash, Hasher};
use std::time::Duration;

/// IDLE is re-issued periodically (RFC 2177 asks for less than 29 minutes;
/// shorter also bounds how long a dropped watch keeps its connection).
const IDLE_TIMEOUT: Duration = Duration::from_secs(9 * 60);

#[derive(Clone)]
pub struct Watch {
    pub account: Account,
    pub folder: String,
    pub generation: u64,
}

impl Hash for Watch {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.account.username.hash(state);
        self.account.imap_host.hash(state);
        self.account.imap_port.hash(state);
        self.folder.hash(state);
        self.generation.hash(state);
    }
}

pub fn subscription(watch: Watch) -> Subscription<Message> {
    Subscription::run_with(watch, stream)
}

fn stream(watch: &Watch) -> impl Stream<Item = Message> + use<> {
    let watch = watch.clone();
    iced::stream::channel(4, async move |mut out| {
        let mut backoff = 15;
        loop {
            let account = watch.account.clone();
            let connected = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<mail::ImapSession>> {
                let mut session = mail::connect(&account)?;
                if !mail::supports_idle(&mut session)? {
                    let _ = session.logout();
                    return Ok(None);
                }
                Ok(Some(session))
            })
            .await;
            let mut session = match connected {
                Ok(Ok(Some(s))) => s,
                // No IDLE on this server: nothing to do, the periodic refresh covers it.
                Ok(Ok(None)) => return,
                _ => {
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(300);
                    continue;
                }
            };
            backoff = 15;
            loop {
                let folder = watch.folder.clone();
                let waited = tokio::task::spawn_blocking(move || {
                    let r = mail::idle_wait(&mut session, &folder, IDLE_TIMEOUT);
                    (session, r)
                })
                .await;
                match waited {
                    Ok((s, Ok(changed))) => {
                        session = s;
                        if changed && out.send(Message::Push(watch.generation)).await.is_err() {
                            return;
                        }
                    }
                    // Connection lost: reconnect.
                    _ => break,
                }
            }
        }
    })
}

/// Shows a desktop notification (blocking: run off the UI thread).
pub fn notify(summary: String, body: String) {
    let _ = notify_rust::Notification::new()
        .appname("Ferrowl")
        .summary(&summary)
        .body(&body)
        .show();
}
