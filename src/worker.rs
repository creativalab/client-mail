//! Thread in background che parla con il server, così la GUI non si blocca.

use crate::config::Account;
use crate::mail::{self, FullMessage, Header, ImapSession, Outgoing};
use anyhow::Result;
use std::sync::mpsc::{Receiver, Sender, channel};

pub enum Command {
    Connect(Account),
    LoadFolder(String),
    OpenMessage { folder: String, uid: u32 },
    Delete { folder: String, uid: u32 },
    Send(Outgoing),
}

pub enum Event {
    Connected(Vec<String>),
    Headers { folder: String, headers: Vec<Header> },
    Message(FullMessage),
    Deleted(u32),
    Sent,
    Error(String),
}

pub struct Worker {
    tx: Sender<Command>,
    pub rx: Receiver<Event>,
}

impl Worker {
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = channel::<Command>();
        let (evt_tx, evt_rx) = channel::<Event>();
        std::thread::spawn(move || run(cmd_rx, evt_tx, ctx));
        Self { tx: cmd_tx, rx: evt_rx }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

struct State {
    account: Option<Account>,
    session: Option<ImapSession>,
}

impl State {
    /// Esegue un'operazione IMAP; se la connessione è caduta riconnette e riprova una volta.
    fn with_session<T>(&mut self, op: impl Fn(&mut ImapSession) -> Result<T>) -> Result<T> {
        let account = self
            .account
            .clone()
            .ok_or_else(|| anyhow::anyhow!("nessun account configurato"))?;
        if let Some(session) = self.session.as_mut() {
            match op(session) {
                Ok(v) => return Ok(v),
                Err(_) => self.session = None,
            }
        }
        let mut session = mail::connect(&account)?;
        let result = op(&mut session);
        self.session = Some(session);
        result
    }
}

fn run(rx: Receiver<Command>, tx: Sender<Event>, ctx: eframe::egui::Context) {
    let mut state = State {
        account: None,
        session: None,
    };
    for cmd in rx {
        let event = handle(&mut state, cmd).unwrap_or_else(|e| Event::Error(format!("{e:#}")));
        if tx.send(event).is_err() {
            break;
        }
        ctx.request_repaint();
    }
    if let Some(mut s) = state.session {
        let _ = s.logout();
    }
}

fn handle(state: &mut State, cmd: Command) -> Result<Event> {
    match cmd {
        Command::Connect(account) => {
            if let Some(mut old) = state.session.take() {
                let _ = old.logout();
            }
            let mut session = mail::connect(&account)?;
            let folders = mail::list_folders(&mut session)?;
            state.account = Some(account);
            state.session = Some(session);
            Ok(Event::Connected(folders))
        }
        Command::LoadFolder(folder) => {
            let headers = state.with_session(|s| mail::fetch_headers(s, &folder))?;
            Ok(Event::Headers { folder, headers })
        }
        Command::OpenMessage { folder, uid } => {
            let msg = state.with_session(|s| mail::fetch_message(s, &folder, uid))?;
            Ok(Event::Message(msg))
        }
        Command::Delete { folder, uid } => {
            state.with_session(|s| mail::delete_message(s, &folder, uid))?;
            Ok(Event::Deleted(uid))
        }
        Command::Send(out) => {
            let account = state
                .account
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("nessun account configurato"))?;
            mail::send(account, &out)?;
            Ok(Event::Sent)
        }
    }
}
