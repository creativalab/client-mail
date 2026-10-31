use crate::config::{Account, Config};
use crate::mail::{FullMessage, Header, Outgoing};
use crate::worker::{Command, Event, Worker};
use eframe::egui::{self, Align, Layout, RichText};
use std::time::{Duration, Instant};

const AUTO_REFRESH: Duration = Duration::from_secs(120);

pub struct MailApp {
    config: Config,
    worker: Worker,
    busy: usize,
    status: String,
    error: Option<String>,

    settings_open: bool,
    settings_draft: Account,

    folders: Vec<String>,
    current_folder: Option<String>,
    headers: Vec<Header>,
    filter: String,
    selected_uid: Option<u32>,
    message: Option<FullMessage>,
    confirm_delete: bool,
    last_refresh: Instant,

    compose: Option<Outgoing>,
}

impl MailApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::load();
        let worker = Worker::spawn(cc.egui_ctx.clone());
        let mut app = Self {
            settings_open: config.account.is_none(),
            settings_draft: config.account.clone().unwrap_or_default(),
            config,
            worker,
            busy: 0,
            status: String::new(),
            error: None,
            folders: Vec::new(),
            current_folder: None,
            headers: Vec::new(),
            filter: String::new(),
            selected_uid: None,
            message: None,
            confirm_delete: false,
            last_refresh: Instant::now(),
            compose: None,
        };
        if let Some(acc) = app.config.account.clone() {
            app.command(Command::Connect(acc), "Connessione…");
        }
        app
    }

    fn command(&mut self, cmd: Command, status: &str) {
        self.busy += 1;
        self.status = status.to_string();
        self.error = None;
        self.worker.send(cmd);
    }

    fn load_folder(&mut self, folder: String) {
        self.last_refresh = Instant::now();
        self.command(Command::LoadFolder(folder), "Caricamento messaggi…");
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.worker.rx.try_recv() {
            self.busy = self.busy.saturating_sub(1);
            self.status.clear();
            match event {
                Event::Connected(folders) => {
                    let first = folders
                        .iter()
                        .find(|f| f.eq_ignore_ascii_case("INBOX"))
                        .or(folders.first())
                        .cloned();
                    self.folders = folders;
                    if let Some(f) = first {
                        self.current_folder = Some(f.clone());
                        self.load_folder(f);
                    }
                }
                Event::Headers { folder, headers } => {
                    if self.current_folder.as_deref() == Some(folder.as_str()) {
                        self.headers = headers;
                    }
                }
                Event::Message(msg) => {
                    if let Some(h) = self.headers.iter_mut().find(|h| h.uid == msg.uid) {
                        h.seen = true;
                    }
                    self.message = Some(msg);
                }
                Event::Deleted(uid) => {
                    self.headers.retain(|h| h.uid != uid);
                    if self.selected_uid == Some(uid) {
                        self.selected_uid = None;
                        self.message = None;
                    }
                    self.status = "Messaggio eliminato".into();
                }
                Event::Sent => {
                    self.compose = None;
                    self.status = "Messaggio inviato".into();
                }
                Event::Error(e) => self.error = Some(e),
            }
        }
    }

    fn reply(&mut self, msg: &FullMessage) {
        let subject = if msg.subject.to_lowercase().starts_with("re:") {
            msg.subject.clone()
        } else {
            format!("Re: {}", msg.subject)
        };
        let quoted: String = msg.body.lines().map(|l| format!("> {l}\n")).collect();
        self.compose = Some(Outgoing {
            to: msg.from_address.clone(),
            cc: String::new(),
            subject,
            body: format!("\n\nIl {}, {} ha scritto:\n{quoted}", msg.date, msg.from),
            in_reply_to: msg.message_id.clone(),
        });
    }

    fn forward(&mut self, msg: &FullMessage) {
        self.compose = Some(Outgoing {
            to: String::new(),
            cc: String::new(),
            subject: format!("Fwd: {}", msg.subject),
            body: format!(
                "\n\n---------- Messaggio inoltrato ----------\nDa: {}\nData: {}\nOggetto: {}\nA: {}\n\n{}",
                msg.from, msg.date, msg.subject, msg.to, msg.body
            ),
            in_reply_to: None,
        });
    }

    fn ui_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let connected = self.config.account.is_some();
                if ui.add_enabled(connected, egui::Button::new("✉ Scrivi")).clicked() {
                    self.compose = Some(Outgoing::default());
                }
                if ui.add_enabled(connected, egui::Button::new("⟳ Aggiorna")).clicked() {
                    if let Some(f) = self.current_folder.clone() {
                        self.load_folder(f);
                    }
                }
                if ui.button("⚙ Impostazioni").clicked() {
                    self.settings_draft = self.config.account.clone().unwrap_or_default();
                    self.settings_open = true;
                }
                ui.separator();
                if self.busy > 0 {
                    ui.spinner();
                }
                if let Some(err) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Errore: {err}"));
                } else if !self.status.is_empty() {
                    ui.label(&self.status);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(acc) = &self.config.account {
                        ui.label(RichText::new(&acc.email).weak());
                    }
                });
            });
        });
    }

    fn ui_folders(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("folders")
            .resizable(true)
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.heading("Cartelle");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                        let mut clicked = None;
                        for folder in &self.folders {
                            let selected = self.current_folder.as_deref() == Some(folder);
                            if ui.selectable_label(selected, folder).clicked() {
                                clicked = Some(folder.clone());
                            }
                        }
                        if let Some(f) = clicked {
                            self.current_folder = Some(f.clone());
                            self.headers.clear();
                            self.selected_uid = None;
                            self.message = None;
                            self.load_folder(f);
                        }
                    });
                });
            });
    }

    fn ui_message_list(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("messages")
            .resizable(true)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("Cerca mittente o oggetto")
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.separator();
                let filter = self.filter.to_lowercase();
                let mut open = None;
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                            for h in self.headers.iter().filter(|h| {
                                filter.is_empty()
                                    || h.subject.to_lowercase().contains(&filter)
                                    || h.from.to_lowercase().contains(&filter)
                            }) {
                                let text = format!("{}   ·   {}\n{}", h.from, h.date, h.subject);
                                let text = if h.seen {
                                    RichText::new(text)
                                } else {
                                    RichText::new(format!("● {text}")).strong()
                                };
                                let selected = self.selected_uid == Some(h.uid);
                                if ui.selectable_label(selected, text).clicked() {
                                    open = Some(h.uid);
                                }
                                ui.add_space(2.0);
                            }
                            if self.headers.is_empty() && self.busy == 0 {
                                ui.label(RichText::new("Nessun messaggio").weak());
                            }
                        });
                    });
                if let (Some(uid), Some(folder)) = (open, self.current_folder.clone()) {
                    self.selected_uid = Some(uid);
                    self.message = None;
                    self.confirm_delete = false;
                    self.command(Command::OpenMessage { folder, uid }, "Apertura messaggio…");
                }
            });
    }

    fn ui_reader(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(msg) = self.message.clone() else {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Seleziona un messaggio").weak());
                });
                return;
            };
            ui.heading(&msg.subject);
            egui::Grid::new("hdr").num_columns(2).show(ui, |ui| {
                ui.label(RichText::new("Da:").strong());
                ui.label(format!("{} <{}>", msg.from, msg.from_address));
                ui.end_row();
                ui.label(RichText::new("A:").strong());
                ui.label(&msg.to);
                ui.end_row();
                ui.label(RichText::new("Data:").strong());
                ui.label(&msg.date);
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button("↩ Rispondi").clicked() {
                    self.reply(&msg);
                }
                if ui.button("➡ Inoltra").clicked() {
                    self.forward(&msg);
                }
                if self.confirm_delete {
                    if ui
                        .button(RichText::new("Conferma eliminazione").color(ui.visuals().error_fg_color))
                        .clicked()
                    {
                        self.confirm_delete = false;
                        if let Some(folder) = self.current_folder.clone() {
                            self.command(Command::Delete { folder, uid: msg.uid }, "Eliminazione…");
                        }
                    }
                    if ui.button("Annulla").clicked() {
                        self.confirm_delete = false;
                    }
                } else if ui.button("🗑 Elimina").clicked() {
                    self.confirm_delete = true;
                }
            });
            if !msg.attachments.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Allegati:").strong());
                    for att in &msg.attachments {
                        let label = format!("📎 {} ({} KB)", att.name, att.data.len().div_ceil(1024));
                        if ui.button(label).on_hover_text("Salva in Scaricati").clicked() {
                            self.status = match save_attachment(&att.name, &att.data) {
                                Ok(path) => format!("Salvato in {}", path.display()),
                                Err(e) => format!("Salvataggio fallito: {e}"),
                            };
                        }
                    }
                });
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.add(egui::Label::new(&msg.body).selectable(true).wrap());
            });
        });
    }

    fn ui_settings(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        let mut save = false;
        egui::Window::new("Account")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let d = &mut self.settings_draft;
                egui::Grid::new("settings").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Nome visualizzato");
                    ui.text_edit_singleline(&mut d.display_name);
                    ui.end_row();
                    ui.label("Email");
                    ui.text_edit_singleline(&mut d.email);
                    ui.end_row();
                    ui.label("Username");
                    ui.text_edit_singleline(&mut d.username);
                    ui.end_row();
                    ui.label("Password");
                    ui.add(egui::TextEdit::singleline(&mut d.password).password(true));
                    ui.end_row();
                    ui.label("Server IMAP");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut d.imap_host);
                        ui.add(egui::DragValue::new(&mut d.imap_port).range(1..=65535));
                    });
                    ui.end_row();
                    ui.label("Server SMTP");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut d.smtp_host);
                        ui.add(egui::DragValue::new(&mut d.smtp_port).range(1..=65535));
                    });
                    ui.end_row();
                });
                ui.label(
                    RichText::new("IMAP: TLS su porta 993. SMTP: 465 (TLS) oppure 587 (STARTTLS).")
                        .weak()
                        .small(),
                );
                ui.label(
                    RichText::new("Per Gmail usa una «password per le app».")
                        .weak()
                        .small(),
                );
                if d.username.is_empty() && !d.email.is_empty() {
                    if ui.small_button("Usa l'email come username").clicked() {
                        d.username = d.email.clone();
                    }
                }
                ui.separator();
                let ready = d.is_complete();
                if ui.add_enabled(ready, egui::Button::new("Salva e connetti")).clicked() {
                    save = true;
                }
            });
        if save {
            let acc = self.settings_draft.clone();
            self.config.account = Some(acc.clone());
            if let Err(e) = self.config.save() {
                self.error = Some(format!("salvataggio configurazione: {e}"));
            }
            self.folders.clear();
            self.headers.clear();
            self.message = None;
            self.selected_uid = None;
            self.settings_open = false;
            self.command(Command::Connect(acc), "Connessione…");
        } else if !open {
            self.settings_open = false;
        }
    }

    fn ui_compose(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.compose.as_mut() else {
            return;
        };
        let mut open = true;
        let mut send = false;
        egui::Window::new("Nuovo messaggio")
            .open(&mut open)
            .default_size([620.0, 480.0])
            .show(ctx, |ui| {
                egui::Grid::new("compose").num_columns(2).show(ui, |ui| {
                    ui.label("A:");
                    ui.add(egui::TextEdit::singleline(&mut draft.to).desired_width(f32::INFINITY));
                    ui.end_row();
                    ui.label("Cc:");
                    ui.add(egui::TextEdit::singleline(&mut draft.cc).desired_width(f32::INFINITY));
                    ui.end_row();
                    ui.label("Oggetto:");
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.subject).desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.body)
                            .desired_rows(16)
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.separator();
                if ui.button("Invia").clicked() {
                    send = true;
                }
            });
        if send {
            let out = draft.clone();
            self.command(Command::Send(out), "Invio…");
        } else if !open {
            self.compose = None;
        }
    }
}

fn save_attachment(name: &str, data: &[u8]) -> anyhow::Result<std::path::PathBuf> {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!("cartella Scaricati non trovata"))?;
    // Evita percorsi pericolosi nel nome fornito dal mittente.
    let safe: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c })
        .collect();
    let safe = safe.trim_start_matches('.');
    let safe = if safe.is_empty() { "allegato" } else { safe };
    let mut path = dir.join(safe);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("({n}) {safe}"));
        n += 1;
    }
    std::fs::write(&path, data)?;
    Ok(path)
}

impl eframe::App for MailApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();

        if self.busy == 0 && self.last_refresh.elapsed() >= AUTO_REFRESH {
            if let Some(f) = self.current_folder.clone() {
                self.load_folder(f);
            }
        }
        ctx.request_repaint_after(AUTO_REFRESH);

        self.ui_top_bar(ctx);
        self.ui_folders(ctx);
        self.ui_message_list(ctx);
        self.ui_reader(ctx);
        self.ui_settings(ctx);
        self.ui_compose(ctx);
    }
}
