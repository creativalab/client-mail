//! Application state and logic (Elm architecture: update / view / subscription).

use crate::backend::{Backend, Res};
use crate::config::{Account, Config, Density};
use crate::i18n::{self, t, tf};
use crate::mail::{Folder, FolderKind, FullMessage, Header, Outgoing};
use crate::theme::{self, Palette};
use crate::ui;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{operation, text_editor};
use iced::{Element, Subscription, Task, Theme};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

pub const SEARCH_ID: &str = "search";
const AUTO_REFRESH: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------- state

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Mail,
    Settings(Section),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Accounts,
    Appearance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFilter {
    All,
    Unread,
    Attachments,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conn {
    Offline,
    Connecting,
    Online,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeMode {
    New,
    Reply,
    Forward,
}

pub struct Compose {
    pub mode: ComposeMode,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub show_cc: bool,
    pub subject: String,
    pub body: text_editor::Content,
    pub attachments: Vec<PathBuf>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub sending: bool,
    pub error: Option<String>,
}

impl Compose {
    fn new(mode: ComposeMode) -> Self {
        Self {
            mode,
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            show_cc: false,
            subject: String::new(),
            body: text_editor::Content::new(),
            attachments: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            sending: false,
            error: None,
        }
    }
}

/// Draft of the account fields in settings (ports as text).
#[derive(Debug, Clone, Default)]
pub struct AccountDraft {
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: String,
    pub smtp_host: String,
    pub smtp_port: String,
}

impl AccountDraft {
    fn from(a: &Account) -> Self {
        Self {
            display_name: a.display_name.clone(),
            email: a.email.clone(),
            username: a.username.clone(),
            password: a.password.clone(),
            imap_host: a.imap_host.clone(),
            imap_port: a.imap_port.to_string(),
            smtp_host: a.smtp_host.clone(),
            smtp_port: a.smtp_port.to_string(),
        }
    }

    pub fn to_account(&self) -> Option<Account> {
        let a = Account {
            display_name: self.display_name.trim().to_string(),
            email: self.email.trim().to_string(),
            username: self.username.trim().to_string(),
            password: self.password.clone(),
            imap_host: self.imap_host.trim().to_string(),
            imap_port: self.imap_port.trim().parse().ok()?,
            smtp_host: self.smtp_host.trim().to_string(),
            smtp_port: self.smtp_port.trim().parse().ok()?,
        };
        a.is_complete().then_some(a)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Field {
    DisplayName,
    Email,
    Username,
    Password,
    ImapHost,
    ImapPort,
    SmtpHost,
    SmtpPort,
}

#[derive(Debug, Clone, Copy)]
pub enum ComposeField {
    To,
    Cc,
    Bcc,
    Subject,
}

pub struct Toast {
    pub id: u64,
    pub text: String,
    pub error: bool,
}

pub struct App {
    pub config: Config,
    pub backend: Backend,
    pub screen: Screen,
    pub conn: Conn,
    pub folders: Vec<Folder>,
    pub unseen: HashMap<String, u32>,
    pub expanded: HashSet<String>,
    pub current_folder: Option<String>,
    pub headers: Vec<Header>,
    pub loading_list: bool,
    pub filter: ListFilter,
    pub search: String,
    pub selected: Option<u32>,
    pub message: Option<FullMessage>,
    pub body: text_editor::Content,
    pub loading_message: bool,
    pub confirm_delete: bool,
    pub appearance_open: bool,
    pub compose: Option<Compose>,
    pub draft: AccountDraft,
    pub testing: bool,
    pub test_result: Option<Res<String>>,
    pub last_sync: Option<chrono::DateTime<chrono::Local>>,
    pub toast: Option<Toast>,
    toast_seq: u64,
    dev_shots: u32,
}

// ---------------------------------------------------------------- messages

#[derive(Debug, Clone)]
pub enum Message {
    // connection and data
    Connected(Res<Vec<Folder>>),
    Unseen(Res<HashMap<String, u32>>),
    SelectFolder(String),
    ToggleNode(String),
    HeadersLoaded { folder: String, silent: bool, result: Res<Vec<Header>> },
    Sync,
    Tick,
    // list
    Search(String),
    SetFilter(ListFilter),
    Open(u32),
    Loaded(Res<FullMessage>),
    BodyAction(text_editor::Action),
    // message actions
    Reply,
    Forward,
    Archive,
    Delete,
    ConfirmDelete(bool),
    ToggleRead,
    Done { uid: u32, note: &'static str, result: Res<()> },
    Archived { uid: u32, result: Res<String> },
    SeenChanged(Res<()>),
    SaveAttachment(usize),
    // appearance
    ToggleAppearance,
    CloseAppearance,
    SetTheme(&'static str),
    SetDensity(Density),
    TogglePreview(bool),
    SetLanguage(String),
    // settings
    OpenSettings(Section),
    CloseSettings,
    Draft(Field, String),
    UseEmailAsUsername,
    TestConnection,
    Tested(Res<usize>),
    SaveAccount,
    // compose
    ComposeNew,
    ComposeField(ComposeField, String),
    ComposeToggleCc,
    ComposeEdit(text_editor::Action),
    ComposeAttach,
    ComposeAttached(Vec<PathBuf>),
    ComposeRemove(usize),
    ComposeSend,
    ComposeSent(Res<Option<String>>),
    ComposeClose,
    // misc
    Key(keyboard::Event),
    DismissToast(u64),
    CopyDetails(String),
    // development: FERROWL_SCREENSHOT=<file.png> captures the window and quits
    DevScreenshot,
    DevShot(iced::window::Screenshot),
    Noop,
}

// ---------------------------------------------------------------- startup

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let mut config = Config::load();
        // Development overrides (not saved unless the user changes a setting).
        if let Ok(lang) = std::env::var("FERROWL_LANG") {
            config.ui.language = lang;
        }
        if let Ok(theme) = std::env::var("FERROWL_THEME") {
            config.ui.theme = theme;
        }
        i18n::set(Some(&config.ui.language));
        let draft = config
            .account
            .as_ref()
            .map(AccountDraft::from)
            .unwrap_or_else(|| AccountDraft {
                imap_port: "993".into(),
                smtp_port: "465".into(),
                ..Default::default()
            });
        let mut app = App {
            screen: if config.account.is_some() {
                Screen::Mail
            } else {
                Screen::Settings(Section::Accounts)
            },
            config,
            backend: Backend::default(),
            conn: Conn::Offline,
            folders: Vec::new(),
            unseen: HashMap::new(),
            expanded: HashSet::new(),
            current_folder: None,
            headers: Vec::new(),
            loading_list: false,
            filter: ListFilter::All,
            search: String::new(),
            selected: None,
            message: None,
            body: text_editor::Content::new(),
            loading_message: false,
            confirm_delete: false,
            appearance_open: false,
            compose: None,
            draft,
            testing: false,
            test_result: None,
            last_sync: None,
            toast: None,
            toast_seq: 0,
            dev_shots: 0,
        };
        // Development: open a given view directly (used for screenshots).
        match std::env::var("FERROWL_DEV_VIEW").as_deref() {
            Ok("settings") => app.screen = Screen::Settings(Section::Accounts),
            Ok("appearance") => app.screen = Screen::Settings(Section::Appearance),
            Ok("general") => app.screen = Screen::Settings(Section::General),
            Ok("popover") => app.appearance_open = true,
            Ok("compose") => {
                let mut c = Compose::new(ComposeMode::New);
                c.to = "giulia.ferri@example.com".into();
                c.subject = "Customer portal quote".into();
                c.body = text_editor::Content::with_text("Hi Giulia,\n\nsounds good to me.\n\nFabio");
                app.compose = Some(c);
            }
            _ => {}
        }
        let task = app.connect();
        (app, task)
    }

    pub fn title(&self) -> String {
        match (&self.current_folder, self.screen) {
            (_, Screen::Settings(_)) => format!("{} — Ferrowl", t("settings")),
            (Some(f), _) => format!("{} — Ferrowl", self.folder_label(f)),
            _ => "Ferrowl".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        theme::iced_theme(theme::get(&self.config.ui.theme))
    }

    pub fn palette(&self) -> Palette {
        theme::get(&self.config.ui.theme).palette
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            keyboard::listen().map(Message::Key),
            iced::time::every(AUTO_REFRESH).map(|_| Message::Tick),
        ];
        if std::env::var_os("FERROWL_SCREENSHOT").is_some() {
            let delay = std::env::var("FERROWL_SCREENSHOT_DELAY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8);
            subs.push(iced::time::every(Duration::from_secs(delay)).map(|_| Message::DevScreenshot));
        }
        Subscription::batch(subs)
    }

    // ------------------------------------------------------------ helper

    fn connect(&mut self) -> Task<Message> {
        let Some(acc) = self.config.account.clone() else {
            return Task::none();
        };
        self.conn = Conn::Connecting;
        let b = self.backend.clone();
        Task::perform(async move { b.connect(acc).await }, Message::Connected)
    }

    fn load_folder(&mut self, folder: String, silent: bool) -> Task<Message> {
        if !silent {
            self.loading_list = true;
        }
        let b = self.backend.clone();
        let f = folder.clone();
        Task::perform(async move { b.headers(f).await }, move |result| {
            Message::HeadersLoaded { folder: folder.clone(), silent, result }
        })
    }

    fn refresh_unseen(&self) -> Task<Message> {
        let mut names: Vec<String> = self
            .folders
            .iter()
            .filter(|f| matches!(f.kind, FolderKind::Inbox | FolderKind::Spam | FolderKind::Drafts))
            .map(|f| f.name.clone())
            .collect();
        if let Some(c) = &self.current_folder {
            if !names.contains(c) {
                names.push(c.clone());
            }
        }
        let b = self.backend.clone();
        Task::perform(async move { b.unseen(names).await }, Message::Unseen)
    }

    pub fn folder_by_kind(&self, kind: FolderKind) -> Option<&Folder> {
        self.folders.iter().find(|f| f.kind == kind)
    }

    pub fn folder_label(&self, name: &str) -> String {
        self.folders
            .iter()
            .find(|f| f.name == name)
            .map(Folder::display)
            .unwrap_or_else(|| name.to_string())
    }

    pub fn current_kind(&self) -> FolderKind {
        self.current_folder
            .as_ref()
            .and_then(|c| self.folders.iter().find(|f| &f.name == c))
            .map(|f| f.kind)
            .unwrap_or(FolderKind::Other)
    }

    /// Messages visible after filter and search (`from:`, `has:attachment`, `is:unread`).
    pub fn visible(&self) -> Vec<&Header> {
        let tokens: Vec<String> = self.search.split_whitespace().map(str::to_lowercase).collect();
        self.headers
            .iter()
            .filter(|h| match self.filter {
                ListFilter::All => true,
                ListFilter::Unread => !h.seen,
                ListFilter::Attachments => h.has_attachment,
            })
            .filter(|h| {
                tokens.iter().all(|tok| {
                    if let Some(v) = tok.strip_prefix("from:") {
                        h.from.to_lowercase().contains(v)
                    } else if tok.starts_with("has:attach") {
                        h.has_attachment
                    } else if tok == "is:unread" {
                        !h.seen
                    } else if tok == "is:read" {
                        h.seen
                    } else {
                        h.subject.to_lowercase().contains(tok)
                            || h.from.to_lowercase().contains(tok)
                            || h.preview.to_lowercase().contains(tok)
                    }
                })
            })
            .collect()
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) -> Task<Message> {
        self.toast_seq += 1;
        let id = self.toast_seq;
        self.toast = Some(Toast { id, text: text.into(), error });
        let secs = if error { 12 } else { 5 };
        Task::perform(tokio::time::sleep(Duration::from_secs(secs)), move |_| {
            Message::DismissToast(id)
        })
    }

    fn save_config(&mut self) -> Task<Message> {
        match self.config.save() {
            Ok(()) => Task::none(),
            Err(e) => self.toast(e.to_string(), true),
        }
    }

    fn open(&mut self, uid: u32) -> Task<Message> {
        let Some(folder) = self.current_folder.clone() else {
            return Task::none();
        };
        self.selected = Some(uid);
        self.message = None;
        self.loading_message = true;
        self.confirm_delete = false;
        let b = self.backend.clone();
        Task::perform(async move { b.message(folder, uid).await }, Message::Loaded)
    }

    /// After removing a message from the list, opens the next one.
    fn remove_and_advance(&mut self, uid: u32) -> Task<Message> {
        let visible: Vec<u32> = self.visible().iter().map(|h| h.uid).collect();
        let pos = visible.iter().position(|&u| u == uid);
        self.headers.retain(|h| h.uid != uid);
        if self.selected != Some(uid) {
            return Task::none();
        }
        self.selected = None;
        self.message = None;
        let next = pos.and_then(|p| {
            visible
                .get(p + 1)
                .or_else(|| p.checked_sub(1).and_then(|q| visible.get(q)))
                .copied()
        });
        match next {
            Some(n) => self.open(n),
            None => Task::none(),
        }
    }

    fn step(&mut self, delta: isize) -> Task<Message> {
        let visible: Vec<u32> = self.visible().iter().map(|h| h.uid).collect();
        if visible.is_empty() {
            return Task::none();
        }
        let idx = self
            .selected
            .and_then(|s| visible.iter().position(|&u| u == s))
            .map(|i| (i as isize + delta).clamp(0, visible.len() as isize - 1) as usize)
            .unwrap_or(0);
        if Some(visible[idx]) == self.selected {
            return Task::none();
        }
        self.open(visible[idx])
    }

    fn reply_draft(&self, msg: &FullMessage) -> Compose {
        let mut c = Compose::new(ComposeMode::Reply);
        c.to = msg.from_address.clone();
        c.subject = if msg.subject.to_lowercase().starts_with("re:") {
            msg.subject.clone()
        } else {
            format!("Re: {}", msg.subject)
        };
        let quoted: String = msg.body.lines().map(|l| format!("> {l}\n")).collect();
        let header = tf(
            "quote_header",
            &[("date", &ui::long_date(msg.timestamp)), ("name", &msg.from)],
        );
        c.body = text_editor::Content::with_text(&format!("\n\n{header}\n{quoted}"));
        c.in_reply_to = msg.message_id.clone();
        c.references = msg.references.clone();
        c
    }

    fn forward_draft(&self, msg: &FullMessage) -> Compose {
        let mut c = Compose::new(ComposeMode::Forward);
        c.subject = format!("Fwd: {}", msg.subject);
        c.body = text_editor::Content::with_text(&format!(
            "\n\n{}\n{}: {} <{}>\n{}: {}\n{}: {}\n{}: {}\n\n{}",
            t("forward_header"),
            t("from"),
            msg.from,
            msg.from_address,
            t("date"),
            ui::long_date(msg.timestamp),
            t("subject"),
            msg.subject,
            t("to"),
            msg.to,
            msg.body
        ));
        // The original attachments are saved to a temporary folder and re-attached.
        let dir = std::env::temp_dir().join(format!("ferrowl-fwd-{}", msg.uid));
        if !msg.attachments.is_empty() && std::fs::create_dir_all(&dir).is_ok() {
            for a in &msg.attachments {
                let path = dir.join(safe_name(&a.name));
                if std::fs::write(&path, &a.data).is_ok() {
                    c.attachments.push(path);
                }
            }
        }
        c
    }

    // ------------------------------------------------------------ update

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Connected(Ok(folders)) => {
                self.conn = Conn::Online;
                self.folders = folders;
                let start = self
                    .folder_by_kind(FolderKind::Inbox)
                    .or(self.folders.iter().find(|f| f.selectable))
                    .map(|f| f.name.clone());
                let still_there = self
                    .current_folder
                    .as_ref()
                    .is_some_and(|c| self.folders.iter().any(|f| &f.name == c));
                if !still_there {
                    self.current_folder = start;
                    self.headers.clear();
                    self.selected = None;
                    self.message = None;
                }
                let load = match self.current_folder.clone() {
                    Some(f) => self.load_folder(f, false),
                    None => Task::none(),
                };
                Task::batch([load, self.refresh_unseen()])
            }
            Message::Connected(Err(e)) => {
                self.conn = Conn::Error(e.clone());
                self.toast(e, true)
            }
            Message::Unseen(Ok(map)) => {
                self.unseen.extend(map);
                Task::none()
            }
            Message::Unseen(Err(_)) => Task::none(),
            Message::SelectFolder(name) => {
                self.appearance_open = false;
                self.screen = Screen::Mail;
                if self.current_folder.as_deref() == Some(&name) {
                    return Task::none();
                }
                self.current_folder = Some(name.clone());
                self.headers.clear();
                self.selected = None;
                self.message = None;
                self.confirm_delete = false;
                self.load_folder(name, false)
            }
            Message::ToggleNode(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
                Task::none()
            }
            Message::HeadersLoaded { folder, silent, result } => {
                if self.current_folder.as_deref() != Some(&folder) {
                    return Task::none();
                }
                self.loading_list = false;
                match result {
                    Ok(headers) => {
                        self.conn = Conn::Online;
                        self.headers = headers;
                        self.last_sync = Some(chrono::Local::now());
                        self.refresh_unseen()
                    }
                    Err(e) => {
                        if !silent {
                            self.conn = Conn::Error(e.clone());
                        }
                        self.toast(e, true)
                    }
                }
            }
            Message::Sync => match (&self.conn, self.current_folder.clone()) {
                (Conn::Online, Some(f)) => self.load_folder(f, false),
                (Conn::Connecting, _) => Task::none(),
                _ => self.connect(),
            },
            Message::Tick => match (&self.conn, self.current_folder.clone()) {
                (Conn::Online, Some(f)) if !self.loading_list => self.load_folder(f, true),
                _ => Task::none(),
            },
            Message::Search(s) => {
                self.search = s;
                Task::none()
            }
            Message::SetFilter(f) => {
                self.filter = f;
                Task::none()
            }
            Message::Open(uid) => self.open(uid),
            Message::Loaded(Ok(msg)) => {
                if Some(msg.uid) != self.selected {
                    return Task::none();
                }
                self.loading_message = false;
                if let Some(h) = self.headers.iter_mut().find(|h| h.uid == msg.uid) {
                    h.seen = true;
                }
                if let Some(c) = &self.current_folder {
                    if let Some(n) = self.unseen.get_mut(c) {
                        *n = n.saturating_sub(1);
                    }
                }
                self.body = text_editor::Content::with_text(&msg.body);
                self.message = Some(msg);
                Task::none()
            }
            Message::Loaded(Err(e)) => {
                self.loading_message = false;
                self.toast(e, true)
            }
            Message::BodyAction(action) => {
                // Read-only text: selection and copy allowed, edits not.
                if !action.is_edit() {
                    self.body.perform(action);
                }
                Task::none()
            }
            Message::Reply => {
                if let Some(m) = &self.message {
                    self.compose = Some(self.reply_draft(m));
                }
                Task::none()
            }
            Message::Forward => {
                if let Some(m) = &self.message {
                    self.compose = Some(self.forward_draft(m));
                }
                Task::none()
            }
            Message::Archive => {
                let (Some(msg), Some(folder)) = (&self.message, self.current_folder.clone()) else {
                    return Task::none();
                };
                if self.current_kind() == FolderKind::Archive {
                    return Task::none();
                }
                let uid = msg.uid;
                let b = self.backend.clone();
                let folders = self.folders.clone();
                Task::perform(async move { b.archive(folder, uid, folders).await }, move |result| {
                    Message::Archived { uid, result }
                })
            }
            Message::Archived { uid, result } => match result {
                Ok(target) => {
                    if self.folder_by_kind(FolderKind::Archive).is_none() {
                        // Folder just created: reload the folder list in the background.
                        let b = self.backend.clone();
                        let acc = self.config.account.clone();
                        let _ = target;
                        let reload = match acc {
                            Some(a) => Task::perform(async move { b.connect(a).await }, Message::Connected),
                            None => Task::none(),
                        };
                        let next = self.remove_and_advance(uid);
                        let toast = self.toast(t("archived"), false);
                        return Task::batch([reload, next, toast]);
                    }
                    let next = self.remove_and_advance(uid);
                    Task::batch([next, self.toast(t("archived"), false)])
                }
                Err(e) => self.toast(e, true),
            },
            Message::Delete => {
                let (Some(msg), Some(folder)) = (&self.message, self.current_folder.clone()) else {
                    return Task::none();
                };
                let uid = msg.uid;
                let trash = self.folder_by_kind(FolderKind::Trash).map(|f| f.name.clone());
                match trash {
                    // Outside Trash: move to Trash (recoverable).
                    Some(trash) if trash != folder => {
                        let b = self.backend.clone();
                        Task::perform(async move { b.move_to(folder, uid, trash).await }, move |result| {
                            Message::Done { uid, note: "moved_to_trash", result }
                        })
                    }
                    // In Trash (or no Trash folder): ask for confirmation.
                    _ => {
                        self.confirm_delete = true;
                        Task::none()
                    }
                }
            }
            Message::ConfirmDelete(yes) => {
                self.confirm_delete = false;
                if !yes {
                    return Task::none();
                }
                let (Some(msg), Some(folder)) = (&self.message, self.current_folder.clone()) else {
                    return Task::none();
                };
                let uid = msg.uid;
                let b = self.backend.clone();
                Task::perform(async move { b.delete_forever(folder, uid).await }, move |result| {
                    Message::Done { uid, note: "deleted_forever", result }
                })
            }
            Message::Done { uid, note, result } => match result {
                Ok(()) => {
                    let next = self.remove_and_advance(uid);
                    Task::batch([next, self.toast(t(note), false)])
                }
                Err(e) => self.toast(e, true),
            },
            Message::ToggleRead => {
                let (Some(uid), Some(folder)) = (self.selected, self.current_folder.clone()) else {
                    return Task::none();
                };
                let Some(h) = self.headers.iter_mut().find(|h| h.uid == uid) else {
                    return Task::none();
                };
                h.seen = !h.seen;
                let seen = h.seen;
                if let Some(n) = self.unseen.get_mut(&folder) {
                    *n = if seen { n.saturating_sub(1) } else { *n + 1 };
                }
                let b = self.backend.clone();
                Task::perform(async move { b.set_seen(folder, uid, seen).await }, Message::SeenChanged)
            }
            Message::SeenChanged(Ok(())) => Task::none(),
            Message::SeenChanged(Err(e)) => self.toast(e, true),
            Message::SaveAttachment(i) => {
                let Some(att) = self.message.as_ref().and_then(|m| m.attachments.get(i)) else {
                    return Task::none();
                };
                match save_attachment(&att.name, &att.data) {
                    Ok(path) => {
                        let p = path.display().to_string();
                        self.toast(tf("saved_to", &[("path", &p)]), false)
                    }
                    Err(e) => self.toast(tf("save_failed", &[("err", &e.to_string())]), true),
                }
            }
            Message::ToggleAppearance => {
                self.appearance_open = !self.appearance_open;
                Task::none()
            }
            Message::CloseAppearance => {
                self.appearance_open = false;
                Task::none()
            }
            Message::SetTheme(key) => {
                self.config.ui.theme = key.to_string();
                self.save_config()
            }
            Message::SetDensity(d) => {
                self.config.ui.density = d;
                self.save_config()
            }
            Message::TogglePreview(on) => {
                self.config.ui.show_preview = on;
                self.save_config()
            }
            Message::SetLanguage(code) => {
                i18n::set(Some(&code));
                self.config.ui.language = code;
                // Folder labels depend on the language: already computed on the fly.
                self.save_config()
            }
            Message::OpenSettings(section) => {
                self.appearance_open = false;
                if let Some(a) = &self.config.account {
                    if self.screen == Screen::Mail {
                        self.draft = AccountDraft::from(a);
                    }
                }
                self.test_result = None;
                self.screen = Screen::Settings(section);
                Task::none()
            }
            Message::CloseSettings => {
                if self.config.account.is_some() {
                    self.screen = Screen::Mail;
                }
                Task::none()
            }
            Message::Draft(field, value) => {
                let d = &mut self.draft;
                let target = match field {
                    Field::DisplayName => &mut d.display_name,
                    Field::Email => &mut d.email,
                    Field::Username => &mut d.username,
                    Field::Password => &mut d.password,
                    Field::ImapHost => &mut d.imap_host,
                    Field::ImapPort => &mut d.imap_port,
                    Field::SmtpHost => &mut d.smtp_host,
                    Field::SmtpPort => &mut d.smtp_port,
                };
                *target = value;
                self.test_result = None;
                Task::none()
            }
            Message::UseEmailAsUsername => {
                self.draft.username = self.draft.email.trim().to_string();
                Task::none()
            }
            Message::TestConnection => {
                let Some(acc) = self.draft.to_account() else {
                    return Task::none();
                };
                self.testing = true;
                self.test_result = None;
                Task::perform(Backend::test(acc), Message::Tested)
            }
            Message::Tested(result) => {
                self.testing = false;
                self.test_result =
                    Some(result.map(|n| tf("test_ok", &[("n", &n.to_string())])));
                Task::none()
            }
            Message::SaveAccount => {
                let Some(acc) = self.draft.to_account() else {
                    return Task::none();
                };
                let changed = self.config.account.as_ref() != Some(&acc);
                self.config.account = Some(acc);
                let save = self.save_config();
                self.screen = Screen::Mail;
                if !changed && self.conn == Conn::Online {
                    return save;
                }
                self.folders.clear();
                self.headers.clear();
                self.current_folder = None;
                self.selected = None;
                self.message = None;
                self.unseen.clear();
                Task::batch([save, self.connect()])
            }
            Message::ComposeNew => {
                if self.compose.is_none() && self.config.account.is_some() {
                    self.compose = Some(Compose::new(ComposeMode::New));
                }
                Task::none()
            }
            Message::ComposeField(field, value) => {
                if let Some(c) = self.compose.as_mut() {
                    match field {
                        ComposeField::To => c.to = value,
                        ComposeField::Cc => c.cc = value,
                        ComposeField::Bcc => c.bcc = value,
                        ComposeField::Subject => c.subject = value,
                    }
                    c.error = None;
                }
                Task::none()
            }
            Message::ComposeToggleCc => {
                if let Some(c) = self.compose.as_mut() {
                    c.show_cc = !c.show_cc;
                }
                Task::none()
            }
            Message::ComposeEdit(action) => {
                if let Some(c) = self.compose.as_mut() {
                    c.body.perform(action);
                }
                Task::none()
            }
            Message::ComposeAttach => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title(t("attach"))
                        .pick_files()
                        .await
                        .map(|files| files.iter().map(|f| f.path().to_path_buf()).collect())
                        .unwrap_or_default()
                },
                Message::ComposeAttached,
            ),
            Message::ComposeAttached(paths) => {
                if let Some(c) = self.compose.as_mut() {
                    for p in paths {
                        if !c.attachments.contains(&p) {
                            c.attachments.push(p);
                        }
                    }
                }
                Task::none()
            }
            Message::ComposeRemove(i) => {
                if let Some(c) = self.compose.as_mut() {
                    if i < c.attachments.len() {
                        c.attachments.remove(i);
                    }
                }
                Task::none()
            }
            Message::ComposeSend => {
                let sent = self.folder_by_kind(FolderKind::Sent).map(|f| f.name.clone());
                let Some(c) = self.compose.as_mut() else {
                    return Task::none();
                };
                if c.sending {
                    return Task::none();
                }
                if c.to.trim().is_empty() {
                    c.error = Some(t("err_no_recipient").into());
                    return Task::none();
                }
                c.sending = true;
                c.error = None;
                let out = Outgoing {
                    to: c.to.clone(),
                    cc: c.cc.clone(),
                    bcc: c.bcc.clone(),
                    subject: c.subject.clone(),
                    body: c.body.text(),
                    in_reply_to: c.in_reply_to.clone(),
                    references: c.references.clone(),
                    attachments: c.attachments.clone(),
                };
                let b = self.backend.clone();
                Task::perform(async move { b.send(out, sent).await }, Message::ComposeSent)
            }
            Message::ComposeSent(result) => match result {
                Ok(note) => {
                    self.compose = None;
                    let toast = match note {
                        None => self.toast(t("sent_ok"), false),
                        Some(e) => self.toast(tf("sent_no_copy", &[("err", &e)]), true),
                    };
                    // If we are in Sent, reload to show the message.
                    let reload = match (self.current_kind(), self.current_folder.clone()) {
                        (FolderKind::Sent, Some(f)) => self.load_folder(f, true),
                        _ => Task::none(),
                    };
                    Task::batch([toast, reload])
                }
                Err(e) => {
                    if let Some(c) = self.compose.as_mut() {
                        c.sending = false;
                        c.error = Some(e);
                    }
                    Task::none()
                }
            },
            Message::ComposeClose => {
                self.compose = None;
                Task::none()
            }
            Message::Key(event) => self.shortcut(event),
            Message::DismissToast(id) => {
                if self.toast.as_ref().is_some_and(|t| t.id == id) {
                    self.toast = None;
                }
                Task::none()
            }
            Message::CopyDetails(details) => {
                let copy = iced::clipboard::write(details);
                Task::batch([copy, self.toast(t("copied"), false)])
            }
            Message::DevScreenshot => {
                // Open the first message so the reader pane is populated.
                if std::env::var_os("FERROWL_SCREENSHOT_OPEN").is_some() && self.selected.is_none() {
                    if let Some(uid) = self.visible().first().map(|h| h.uid) {
                        return self.open(uid);
                    }
                }
                iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(Message::DevShot)
            }
            Message::DevShot(shot) => {
                if let Some(path) = std::env::var_os("FERROWL_SCREENSHOT") {
                    let mut path = PathBuf::from(path);
                    self.dev_shots += 1;
                    if self.dev_shots > 1 {
                        path.set_extension(format!("{}.png", self.dev_shots));
                    }
                    if let Err(e) = save_png(&path, &shot) {
                        eprintln!("screenshot failed: {e}");
                    }
                }
                let wanted: u32 = std::env::var("FERROWL_SCREENSHOT_COUNT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                if self.dev_shots >= wanted { iced::exit() } else { Task::none() }
            }
            Message::Noop => Task::none(),
        }
    }

    /// Keyboard shortcuts (only when no text field has focus).
    fn shortcut(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        if let Key::Named(Named::Escape) = key {
            if self.appearance_open {
                self.appearance_open = false;
            } else if self.confirm_delete {
                self.confirm_delete = false;
            }
            return Task::none();
        }
        if self.compose.is_some() || self.screen != Screen::Mail {
            return Task::none();
        }
        if modifiers.command() {
            return match key.as_ref() {
                Key::Character("k") => operation::focus(SEARCH_ID),
                _ => Task::none(),
            };
        }
        if modifiers.alt() {
            return Task::none();
        }
        match key.as_ref() {
            Key::Character("c") => self.update(Message::ComposeNew),
            Key::Character("r") => self.update(Message::Reply),
            Key::Character("f") => self.update(Message::Forward),
            Key::Character("e") => self.update(Message::Archive),
            Key::Character("u") => self.update(Message::ToggleRead),
            Key::Character("j") => self.step(1),
            Key::Character("k") => self.step(-1),
            Key::Character("/") => operation::focus(SEARCH_ID),
            Key::Named(Named::ArrowDown) => self.step(1),
            Key::Named(Named::ArrowUp) => self.step(-1),
            Key::Named(Named::Delete) => self.update(Message::Delete),
            _ => Task::none(),
        }
    }

    // ------------------------------------------------------------ view

    pub fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::Mail => ui::mail_view::view(self),
            Screen::Settings(section) => ui::settings::view(self, section),
        }
    }

    pub fn density_padding(&self) -> f32 {
        match self.config.ui.density {
            Density::Compact => 8.0,
            Density::Comfortable => 14.0,
            Density::Spacious => 20.0,
        }
    }
}

fn save_png(path: &std::path::Path, shot: &iced::window::Screenshot) -> anyhow::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, shot.size.width, shot.size.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&shot.rgba)?;
    Ok(())
}

fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | '\0' | ':') { '_' } else { c })
        .collect();
    let s = s.trim_start_matches('.').to_string();
    if s.is_empty() { "attachment".into() } else { s }
}

fn save_attachment(name: &str, data: &[u8]) -> anyhow::Result<PathBuf> {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!(t("err_no_downloads")))?;
    let safe = safe_name(name);
    let mut path = dir.join(&safe);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("({n}) {safe}"));
        n += 1;
    }
    std::fs::write(&path, data)?;
    Ok(path)
}
