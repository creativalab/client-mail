//! Main window: top bar · folders · list · reader (Main.dc.html).

use super::*;
use crate::app::{App, Conn, ListFilter, SEARCH_ID, Section};
use crate::i18n::{self, SUPPORT_EMAIL, t, tf};
use crate::mail::{Folder, FolderKind};
use crate::theme;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::text::Wrapping;
use iced::widget::{Column, Row, column, mouse_area, opaque, row, scrollable, space, stack, text_editor};
use iced::{Alignment, Length::Fill, Padding};
use std::collections::BTreeMap;

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.palette();
    let body = dir_row(vec![
        sidebar(app, p),
        vline(p.line),
        message_list(app, p),
        vline(p.line),
        reader(app, p),
    ])
    .height(Fill);

    let base = container(column![top_bar(app, p), hline(p.line), body])
        .width(Fill)
        .height(Fill)
        .style(panel(p.bg));

    let mut layers: Vec<Element<'_, Message>> = vec![base.into()];
    if app.appearance_open {
        layers.push(appearance_popover(app, p));
    }
    if let Some(c) = &app.compose {
        layers.push(super::compose::view(app, c, p));
    }
    if let Some(toast) = toast_layer(app, p) {
        layers.push(toast);
    }
    stack(layers).width(Fill).height(Fill).into()
}

// ---------------------------------------------------------------- top bar

fn top_bar<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let brand = row![logo(&p, 32.0), text("Ferrowl").size(18).font(INTER_BOLD).color(p.text)]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(208);

    let search = container(
        row![
            icon(icons::SEARCH, 18.0, p.muted),
            text_input(t("search_placeholder"), &app.search)
                .id(SEARCH_ID)
                .on_input(Message::Search)
                .size(14)
                .padding(0)
                .style(bare_input(p)),
            kbd("Ctrl K", p),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .height(40)
    .padding([0, 14])
    .center_y(40)
    .width(Fill)
    .max_width(720)
    .style(card(p, p.field, 10.0));

    let syncing = app.loading_list || app.conn == Conn::Connecting;
    let sync = button(icon(icons::SYNC, 20.0, if syncing { p.accent } else { p.text2 }))
        .width(44)
        .height(44)
        .padding(12)
        .style(ghost(p, p.text2))
        .on_press(Message::Sync);
    let sync = tooltip(sync, t("sync"), p);

    let gear_on = app.appearance_open;
    let gear = button(icon(icons::GEAR, 20.0, if gear_on { p.active_text } else { p.text2 }))
        .width(44)
        .height(44)
        .padding(12)
        .style(move |th, st| {
            let mut s = ghost(p, p.text2)(th, st);
            if gear_on {
                s.background = Some(Background::Color(p.side_active));
            }
            s
        })
        .on_press(Message::ToggleAppearance);
    let gear = tooltip(gear, t("appearance"), p);

    let initials = app
        .config
        .account
        .as_ref()
        .map(|a| a.initials())
        .unwrap_or_else(|| "?".into());
    let me = button(avatar(initials, p.ink, p.on_ink, 36.0))
        .padding(0)
        .style(ghost(p, p.text))
        .on_press(Message::OpenSettings(Section::Accounts));

    container(
        dir_row(vec![
            brand.into(),
            search.into(),
            space::horizontal().into(),
            row![sync, gear, me].spacing(8).align_y(Alignment::Center).into(),
        ])
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .padding([12, 20])
    .width(Fill)
    .style(panel(p.chrome))
    .into()
}

// ---------------------------------------------------------------- sidebar

fn kind_icon(kind: FolderKind) -> &'static str {
    match kind {
        FolderKind::Inbox => icons::INBOX,
        FolderKind::Sent => icons::SEND,
        FolderKind::Drafts => icons::DRAFT,
        FolderKind::Archive => icons::ARCHIVE,
        FolderKind::Spam => icons::SPAM,
        FolderKind::Trash => icons::TRASH,
        FolderKind::Other => icons::FOLDER,
    }
}

fn sidebar<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let compose = button(
        container(
            row![
                icon(icons::PEN, 18.0, p.on_accent),
                text(t("compose")).size(15).font(semibold())
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill),
    )
    .width(Fill)
    .height(44)
    .style(accent_btn(p))
    .on_press_maybe(app.config.account.as_ref().map(|_| Message::ComposeNew));

    let mut specials = Column::new().spacing(2);
    for kind in FolderKind::SPECIAL {
        if let Some(f) = app.folder_by_kind(kind) {
            let count = app.unseen.get(&f.name).copied().unwrap_or(0);
            specials = specials.push(folder_item(app, p, f, kind.label().to_string(), count));
        }
    }

    let tree = folder_tree(app, p);

    let (dot, status_line) = match &app.conn {
        Conn::Online => (
            theme::hex("#3C9462"),
            app.last_sync
                .map(|d| tf("status_synced", &[("time", &d.format("%H:%M").to_string())]))
                .unwrap_or_else(|| t("account_connected").into()),
        ),
        Conn::Connecting => (p.field, t("status_connecting").into()),
        Conn::Offline => (p.field, t("status_offline").into()),
        Conn::Error(_) => (p.danger, t("status_error").into()),
    };
    let email = app
        .config
        .account
        .as_ref()
        .map(|a| a.email.clone())
        .unwrap_or_default();
    let account = column![
        section_label(t("section_account"), p),
        row![
            container(space().width(8).height(8)).style(chip_box(dot, dot, 4.0)),
            text(trunc(&email, 26)).size(14).color(p.text2)
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding([8, 12]),
    ]
    .spacing(6);

    let status = container(
        column![
            text(status_line).font(MONO).size(12).color(p.text2),
            text(tf("folder_count", &[("n", &app.headers.len().to_string())]))
                .font(MONO)
                .size(12)
                .color(p.text2),
        ]
        .spacing(4),
    )
    .padding(12)
    .width(Fill)
    .style(chip_box(p.chip, p.text2, 10.0));

    let mut col = column![compose, specials].spacing(20);
    if let Some(tree) = tree {
        col = col.push(
            column![
                section_label(t("section_folders"), p),
                scrollable(tree).height(Fill).style(scroll_style(p))
            ]
            .spacing(6)
            .height(Fill),
        );
    } else {
        col = col.push(space::vertical());
    }
    container(col.push(account).push(status))
        .width(240)
        .height(Fill)
        .padding([16, 12])
        .style(panel(p.side))
        .into()
}

fn section_label<'a>(s: &str, p: Palette) -> Element<'a, Message> {
    container(text(s.to_uppercase()).size(12).font(semibold()).color(p.muted))
        .padding([0, 12])
        .into()
}

fn folder_item<'a>(app: &App, p: Palette, f: &Folder, label: String, count: u32) -> Element<'a, Message> {
    let active = app.current_folder.as_deref() == Some(f.name.as_str());
    let fg = if active { p.active_text } else { p.text2 };
    let count = if count > 0 { count.to_string() } else { String::new() };
    button(container(
        row![
            container(icon(kind_icon(f.kind), 18.0, fg)).width(20).center_x(20),
            text(trunc(&label, 22))
                .size(14)
                .font(if active { semibold() } else { medium() })
                .width(Fill),
            text(count).size(12).font(MONO).color(fg),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ).center_y(Fill))
    .width(Fill)
    .height(40)
    .padding([0, 12])
    .style(nav_item(p, active))
    .on_press(Message::SelectFolder(f.name.clone()))
    .into()
}

/// Tree of the other folders (subfolders included), collapsed by default.
#[derive(Default)]
struct Node {
    label: String,
    folder: Option<String>,
    kind: Option<FolderKind>,
    children: BTreeMap<String, Node>,
}

fn folder_tree<'a>(app: &'a App, p: Palette) -> Option<Element<'a, Message>> {
    let mut root = Node::default();
    for f in &app.folders {
        if f.kind == FolderKind::Inbox {
            continue;
        }
        let mut segs: &[String] = &f.path;
        if segs.len() > 1 && segs[0].eq_ignore_ascii_case("INBOX") {
            segs = &segs[1..];
        }
        let mut node = &mut root;
        for seg in segs {
            node = node.children.entry(seg.to_lowercase()).or_insert_with(|| Node {
                label: seg.clone(),
                ..Default::default()
            });
        }
        if f.selectable {
            node.folder = Some(f.name.clone());
        }
        if f.kind != FolderKind::Other {
            node.kind = Some(f.kind);
            node.label = f.display();
        }
    }
    // Special folders without subfolders are already at the top.
    root.children
        .retain(|_, n| n.kind.is_none() || !n.children.is_empty());
    if root.children.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    for (key, node) in &root.children {
        push_node(app, p, &mut rows, node, key.clone(), 0);
    }
    Some(Column::with_children(rows).spacing(1).into())
}

fn push_node<'a>(app: &App, p: Palette, rows: &mut Vec<Element<'a, Message>>, node: &Node, id: String, depth: u16) {
    let expanded = app.expanded.contains(&id);
    let active = node.folder.is_some() && app.current_folder == node.folder;
    let fg = if active { p.active_text } else { p.text2 };
    let chevron: Element<'a, Message> = if node.children.is_empty() {
        space().width(22).into()
    } else {
        let chev = if expanded {
            icons::CHEVRON_DOWN
        } else if i18n::is_rtl() {
            icons::CHEVRON_LEFT
        } else {
            icons::CHEVRON_RIGHT
        };
        button(container(icon(chev, 14.0, p.muted)).center(Fill))
            .width(22)
            .height(32)
            .padding(0)
            .style(ghost(p, p.muted))
            .on_press(Message::ToggleNode(id.clone()))
            .into()
    };
    let label = button(container(
        row![
            icon(node.kind.map(kind_icon).unwrap_or(icons::FOLDER), 16.0, fg),
            text(trunc(&node.label, 22 - (depth as usize * 2).min(12)))
                .size(13)
                .font(if active { semibold() } else { INTER })
                .color(fg)
                .wrapping(Wrapping::None)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ).center_y(Fill).clip(true))
    .width(Fill)
    .height(32)
    .padding([0, 8])
    .style(nav_item(p, active))
    .on_press(match &node.folder {
        Some(f) => Message::SelectFolder(f.clone()),
        None => Message::ToggleNode(id.clone()),
    });
    rows.push(
        dir_row(vec![
            space().width(depth as f32 * 14.0).into(),
            chevron,
            label.into(),
        ])
        .align_y(Alignment::Center)
        .into(),
    );
    if expanded {
        for (key, child) in &node.children {
            push_node(app, p, rows, child, format!("{id}\u{1f}{key}"), depth + 1);
        }
    }
}

// ---------------------------------------------------------------- message list

fn message_list<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let title = app
        .current_folder
        .as_ref()
        .map(|f| app.folder_label(f))
        .unwrap_or_default();

    let seg = |label: &'static str, f: ListFilter| {
        button(text(label).size(13).font(if app.filter == f { semibold() } else { INTER }))
            .padding([6, 9])
            .style(segment(p, app.filter == f))
            .on_press(Message::SetFilter(f))
    };
    let filters = container(
        row![
            seg(t("filter_all"), ListFilter::All),
            seg(t("filter_unread"), ListFilter::Unread),
            seg(t("filter_attachments"), ListFilter::Attachments),
        ]
        .spacing(4),
    )
    .padding(3)
    .style(chip_box(p.chip, p.text2, 9.0));

    let header = dir_row(vec![
        container(text(trunc(&title, 22)).size(20).font(INTER_BOLD).wrapping(Wrapping::None))
            .width(Fill)
            .clip(true)
            .into(),
        filters.into(),
    ])
    .spacing(12)
    .align_y(Alignment::Center)
    .padding(Padding { top: 16.0, right: 20.0, bottom: 12.0, left: 20.0 });

    let visible = app.visible();
    let content: Element<'a, Message> = if visible.is_empty() {
        let msg = if app.loading_list || app.conn == Conn::Connecting {
            t("list_loading")
        } else {
            t("list_empty")
        };
        container(text(msg).size(14).color(p.muted))
            .center_x(Fill)
            .padding(40)
            .into()
    } else {
        let pad = app.density_padding();
        let preview = app.config.ui.show_preview;
        let mut col = Column::new();
        for h in visible {
            let selected = app.selected == Some(h.uid);
            let unread = !h.seen;
            let mut top = vec![
                container(space().width(8).height(8))
                    .style(chip_box(
                        if unread { p.accent } else { Color::TRANSPARENT },
                        p.accent,
                        4.0,
                    ))
                    .into(),
                text(trunc(&h.from, 32))
                    .wrapping(Wrapping::None)
                    .size(14)
                    .font(if unread { INTER_BOLD } else { medium() })
                    .color(p.text)
                    .width(Fill)
                    .into(),
            ];
            if h.has_attachment {
                top.push(icon(icons::CLIP, 14.0, p.muted));
            }
            top.push(text(short_date(h.timestamp)).font(MONO).size(12).color(p.muted).into());

            let mut lines = column![
                dir_row(top).spacing(8).align_y(Alignment::Center),
                container(
                    text(trunc(&h.subject, 44))
                        .size(14)
                        .font(if unread { semibold() } else { INTER })
                        .color(p.text)
                        .wrapping(Wrapping::None)
                )
                .padding(indent(16.0))
                .width(Fill)
                .clip(true),
            ]
            .spacing(4);
            if preview && !h.preview.is_empty() {
                lines = lines.push(
                    container(
                        text(trunc(&h.preview, 52))
                            .size(13)
                            .color(p.muted)
                            .wrapping(Wrapping::None),
                    )
                    .padding(indent(16.0))
                    .width(Fill)
                    .clip(true),
                );
            }
            let row_btn = button(lines)
                .width(Fill)
                .padding([pad, 20.0])
                .style(move |_, st| button::Style {
                    background: Some(Background::Color(if selected {
                        p.soft
                    } else if matches!(st, button::Status::Hovered) {
                        mix(p.bg, p.soft, 0.5)
                    } else {
                        p.bg
                    })),
                    text_color: p.text,
                    ..Default::default()
                })
                .on_press(Message::Open(h.uid));
            // 3 px accent bar on the selected row.
            let item: Element<'a, Message> = if selected {
                let bar = container(space().width(3).height(Fill)).style(panel(p.accent));
                let bar = if i18n::is_rtl() {
                    container(bar).align_x(Horizontal::Right).width(Fill).height(Fill)
                } else {
                    container(bar).height(Fill)
                };
                stack![row_btn, bar].into()
            } else {
                row_btn.into()
            };
            col = col.push(item).push(hline(p.line_soft));
        }
        scrollable(col).height(Fill).style(scroll_style(p)).into()
    };

    container(column![header, content])
        .width(410)
        .height(Fill)
        .style(panel(p.bg))
        .into()
}

fn indent(n: f32) -> Padding {
    if i18n::is_rtl() {
        Padding { right: n, ..Padding::ZERO }
    } else {
        Padding { left: n, ..Padding::ZERO }
    }
}

// ---------------------------------------------------------------- reader

fn tool_btn<'a>(p: Palette, ic: &'static str, label: Option<&'static str>, msg: Message) -> Element<'a, Message> {
    let content: Element<'a, Message> = match label {
        Some(l) => row![icon(ic, 18.0, p.text2), text(l).size(14)]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        None => icon(ic, 18.0, p.text2),
    };
    let b = button(container(content).center_y(Fill))
        .height(44)
        .padding(if label.is_some() { [0, 14] } else { [0, 13] })
        .style(ghost(p, p.text2))
        .on_press(msg);
    b.into()
}

fn reader<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let Some(msg) = &app.message else {
        let inner: Element<'a, Message> = if app.loading_message {
            text(t("loading_message")).size(14).color(p.muted).into()
        } else {
            column![icon(icons::MAIL, 48.0, p.field), text(t("select_message")).size(15).color(p.muted)]
                .spacing(12)
                .align_x(Alignment::Center)
                .into()
        };
        return container(inner)
            .center(Fill)
            .style(panel(p.bg))
            .into();
    };

    let kind = app.current_kind();
    let mut tools = vec![
        tool_btn(p, icons::REPLY, Some(t("reply")), Message::Reply),
        tool_btn(p, icons::FORWARD, Some(t("forward")), Message::Forward),
    ];
    if kind != FolderKind::Archive {
        tools.push(tool_btn(p, icons::ARCHIVE, Some(t("archive")), Message::Archive));
    }
    let delete_tip = if kind == FolderKind::Trash || app.folder_by_kind(FolderKind::Trash).is_none() {
        t("delete_forever")
    } else {
        t("delete")
    };
    tools.push(tooltip(tool_btn(p, icons::TRASH, None, Message::Delete), delete_tip, p));
    tools.push(tooltip(tool_btn(p, icons::MAIL, None, Message::ToggleRead), t("mark_unread"), p));
    tools.push(space::horizontal().into());
    if msg.dkim_pass {
        tools.push(
            container(
                row![icon(icons::LOCK, 14.0, p.sec_text), text(t("dkim_ok")).size(12).font(medium())]
                    .spacing(6)
                    .align_y(Alignment::Center),
            )
            .padding([6, 10])
            .style(chip_box(p.sec_bg, p.sec_text, 8.0))
            .into(),
        );
    }
    let toolbar = container(dir_row(tools).spacing(4).align_y(Alignment::Center))
        .padding([10, 16])
        .width(Fill);

    let confirm: Option<Element<'a, Message>> = app.confirm_delete.then(|| {
        container(
            dir_row(vec![
                text(t("confirm_delete_forever")).size(14).color(p.danger).width(Fill).into(),
                button(text(t("cancel")).size(14))
                    .padding([8, 14])
                    .style(outline_btn(p, p.text))
                    .on_press(Message::ConfirmDelete(false))
                    .into(),
                button(text(t("delete_forever")).size(14).font(semibold()))
                    .padding([8, 14])
                    .style(move |_, _| button::Style {
                        background: Some(Background::Color(p.danger)),
                        text_color: if p.dark { p.bg } else { Color::WHITE },
                        border: Border { radius: radius(8.0), ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Message::ConfirmDelete(true))
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([12, 20])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(mix(p.bg, p.danger, 0.08))),
            border: Border { color: p.danger_line, width: 1.0, radius: radius(0.0) },
            ..Default::default()
        })
        .into()
    });

    let mut recipients = msg.to.clone();
    if !msg.cc.is_empty() {
        recipients = format!("{recipients}, {}: {}", t("cc"), msg.cc);
    }
    let me_line = tf("to_line", &[("to", &trunc(&recipients, 70))]);
    let sender = dir_row(vec![
        avatar(initials(&msg.from), avatar_color(&msg.from_address), Color::WHITE, 44.0),
        column![
            dir_row(vec![
                text(trunc(&msg.from, 40)).size(15).font(semibold()).color(p.text).into(),
                text(format!("<{}>", msg.from_address)).size(15).color(p.muted).into(),
            ])
            .spacing(6),
            text(format!("{me_line} · {}", long_date(msg.timestamp))).size(13).color(p.muted),
        ]
        .spacing(2)
        .width(Fill)
        .into(),
    ])
    .spacing(14)
    .align_y(Alignment::Center);

    let body = text_editor(&app.body)
        .on_action(Message::BodyAction)
        .size(15)
        .line_height(1.65)
        .padding(0)
        .style(move |_, _| text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            placeholder: p.muted,
            value: p.text2,
            selection: mix(p.accent, p.bg, 0.7),
        });

    let mut content = column![
        text(&msg.subject).size(26).font(INTER_BOLD).color(p.text).line_height(1.25),
        sender,
        body,
    ]
    .spacing(24)
    .max_width(820);

    if !msg.attachments.is_empty() {
        let label = if msg.attachments.len() == 1 {
            t("attachments_one").to_string()
        } else {
            tf("attachments_many", &[("n", &msg.attachments.len().to_string())])
        };
        let mut cards = Row::new().spacing(12);
        for (i, a) in msg.attachments.iter().enumerate() {
            let c = button(
                row![
                    container(text(extension(&a.name)).font(MONO).size(11).color(p.accent_text))
                        .center_x(40)
                        .center_y(40)
                        .style(chip_box(p.chip, p.accent_text, 8.0)),
                    column![
                        text(trunc(&a.name, 30)).size(14).font(medium()).color(p.text),
                        text(human_size(a.data.len())).size(12).color(p.muted),
                    ]
                    .spacing(2),
                    icon(icons::DOWNLOAD, 16.0, p.muted),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 10.0, right: 14.0, bottom: 10.0, left: 10.0 })
            .style(move |_, st| button::Style {
                background: Some(Background::Color(if matches!(st, button::Status::Hovered) {
                    p.chip
                } else {
                    p.bg
                })),
                text_color: p.text,
                border: Border { color: p.line, width: 1.0, radius: radius(12.0) },
                ..Default::default()
            })
            .on_press(Message::SaveAttachment(i));
            cards = cards.push(tooltip(c, t("save_attachment_hint"), p));
        }
        content = content.push(
            column![
                text(label).size(13).font(semibold()).color(p.text2),
                cards.wrap().vertical_spacing(12)
            ]
            .spacing(10),
        );
    }

    let quick = button(
        dir_row(vec![
            icon(icons::REPLY, 18.0, p.muted),
            text(tf("quick_reply", &[("name", &msg.from)])).size(14).color(p.muted).width(Fill).into(),
            kbd("R", p),
        ])
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([14, 16])
    .style(move |_, st| button::Style {
        background: Some(Background::Color(if matches!(st, button::Status::Hovered) {
            p.chip
        } else {
            p.bg
        })),
        text_color: p.muted,
        border: Border { color: p.field, width: 1.0, radius: radius(12.0) },
        ..Default::default()
    })
    .on_press(Message::Reply);
    content = content.push(quick);

    let mut col = column![toolbar, hline(p.line)];
    if let Some(c) = confirm {
        col = col.push(c);
    }
    col = col.push(
        scrollable(container(content).padding([28, 40]).width(Fill))
            .height(Fill)
            .style(scroll_style(p)),
    );
    container(col).width(Fill).height(Fill).style(panel(p.bg)).into()
}

// ---------------------------------------------------------------- Appearance panel

fn appearance_popover<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let header = dir_row(vec![
        column![
            text(t("settings")).size(12).color(p.muted),
            text(t("appearance")).size(17).font(INTER_BOLD).color(p.text)
        ]
        .spacing(2)
        .width(Fill)
        .into(),
        button(icon(icons::CLOSE, 18.0, p.text2))
            .width(44)
            .height(44)
            .padding(13)
            .style(ghost(p, p.text2))
            .on_press(Message::CloseAppearance)
            .into(),
    ])
    .align_y(Alignment::Center)
    .padding(Padding { top: 14.0, right: 12.0, bottom: 14.0, left: 20.0 });

    let all_settings = button(
        container(
            row![
                text(t("all_settings")).size(14).font(semibold()),
                icon(if i18n::is_rtl() { icons::CHEVRON_LEFT } else { icons::CHEVRON_RIGHT }, 16.0, p.text)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill),
    )
    .width(Fill)
    .height(44)
    .style(move |th, st| {
        let mut s = outline_btn(p, p.text)(th, st);
        s.border.radius = radius(10.0);
        s
    })
    .on_press(Message::OpenSettings(Section::Appearance));

    let body = column![
        text(t("color_theme")).size(13).font(semibold()).color(p.text2),
        theme_grid(&app.config.ui.theme, p, 58.0, 10.0),
        container(text(t("theme_note")).size(13).color(p.text2))
            .padding([12, 14])
            .width(Fill)
            .style(chip_box(p.chip, p.text2, 10.0)),
        all_settings,
    ]
    .spacing(14)
    .padding(Padding { top: 18.0, right: 20.0, bottom: 20.0, left: 20.0 });

    let card_box = container(column![header, hline(p.line), body])
        .width(440)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.bg)),
            text_color: Some(p.text),
            border: Border { color: p.line, width: 1.0, radius: radius(16.0) },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.22),
                offset: Vector::new(0.0, 20.0),
                blur_radius: 50.0,
            },
            ..Default::default()
        });

    // Click outside the panel = close.
    mouse_area(
        container(opaque(card_box))
            .width(Fill)
            .height(Fill)
            .align_x(if i18n::is_rtl() { Horizontal::Left } else { Horizontal::Right })
            .align_y(Vertical::Top)
            .padding(Padding { top: 64.0, right: 20.0, bottom: 0.0, left: 20.0 }),
    )
    .on_press(Message::CloseAppearance)
    .into()
}

// ---------------------------------------------------------------- notifications

pub fn toast_layer<'a>(app: &'a App, p: Palette) -> Option<Element<'a, Message>> {
    let toast = app.toast.as_ref()?;
    let (bg, fg) = (p.ink, p.on_ink);
    let mut col = column![text(&toast.text).size(14).color(fg)].spacing(6);
    if toast.error {
        let details = format!(
            "{}\n\nFerrowl {} · {} {}\n{}",
            toast.text,
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            SUPPORT_EMAIL
        );
        col = col.push(
            row![
                text(tf("err_support_hint", &[("email", SUPPORT_EMAIL)]))
                    .size(12)
                    .color(mix(fg, bg, 0.3))
                    .width(Fill),
                button(text(t("copy_details")).size(12).font(semibold()))
                    .padding([4, 10])
                    .style(move |_, st| button::Style {
                        background: Some(Background::Color(if matches!(st, button::Status::Hovered) {
                            mix(bg, fg, 0.25)
                        } else {
                            mix(bg, fg, 0.12)
                        })),
                        text_color: fg,
                        border: Border { radius: radius(6.0), ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Message::CopyDetails(details)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }
    let close = button(icon(icons::CLOSE, 14.0, fg))
        .padding(6)
        .style(move |_, _| button::Style { text_color: fg, ..Default::default() })
        .on_press(Message::DismissToast(toast.id));
    let accent = if toast.error { p.danger } else { p.accent };
    let boxed = container(
        row![
            container(space().width(4).height(Fill)).style(chip_box(accent, accent, 2.0)),
            col.width(Fill),
            close
        ]
        .spacing(12)
        .height(iced::Length::Shrink),
    )
    .padding([12, 14])
    .max_width(620)
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        border: Border { radius: radius(12.0), ..Default::default() },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    });
    Some(
        container(opaque(boxed))
            .width(Fill)
            .height(Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Bottom)
            .padding(24)
            .into(),
    )
}
