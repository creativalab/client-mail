//! "New message" dialog (Compose.dc.html), drawn over the main window
//! and following the active theme.

use super::*;
use crate::app::{App, Compose, ComposeField, ComposeMode};
use crate::i18n::t;
use iced::keyboard::{Key, key::Named};
use iced::widget::{Column, column, opaque, row, scrollable, space, text_editor};
use iced::widget::text_editor::{Binding, KeyPress};
use iced::{Alignment, Length::Fill, Padding};

pub fn view<'a>(app: &'a App, c: &'a Compose, p: Palette) -> Element<'a, Message> {
    let title = match c.mode {
        ComposeMode::New => t("compose_new"),
        ComposeMode::Reply => t("compose_reply"),
        ComposeMode::Forward => t("compose_forward"),
    };

    // Dark title bar (ink), like the mockup.
    let head = container(
        dir_row(vec![
            text(title).size(14).font(semibold()).color(p.on_ink).width(Fill).into(),
            button(icon(icons::CLOSE, 16.0, p.on_ink))
                .width(36)
                .height(36)
                .padding(10)
                .style(move |_, st| button::Style {
                    background: matches!(st, button::Status::Hovered)
                        .then_some(Background::Color(mix(p.ink, p.on_ink, 0.15))),
                    text_color: p.on_ink,
                    border: Border { radius: radius(8.0), ..Default::default() },
                    ..Default::default()
                })
                .on_press(Message::ComposeClose)
                .into(),
        ])
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 10.0, right: 10.0, bottom: 10.0, left: 20.0 })
    .width(Fill)
    .style(panel(p.ink));

    let from = app
        .config
        .account
        .as_ref()
        .map(|a| {
            if a.display_name.is_empty() {
                a.email.clone()
            } else {
                format!("{} <{}>", a.display_name, a.email)
            }
        })
        .unwrap_or_default();

    let field_row = |label: &'static str, content: Element<'a, Message>| -> Element<'a, Message> {
        column![
            container(
                dir_row(vec![
                    text(label).size(13).color(p.muted).width(64).into(),
                    content,
                ])
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([0, 20])
            .height(48)
            .center_y(48),
            hline(p.line_soft),
        ]
        .into()
    };
    let input = |value: &'a str, field: ComposeField, bold: bool| -> Element<'a, Message> {
        text_input("", value)
            .on_input(move |v| Message::ComposeField(field, v))
            .on_submit(Message::Noop)
            .size(14)
            .padding([8, 0])
            .font(if bold { semibold() } else { INTER })
            .style(bare_input(p))
            .width(Fill)
            .into()
    };

    let cc_toggle = button(text(t("cc_bcc")).size(13).color(p.muted))
        .padding([4, 6])
        .style(ghost(p, p.muted))
        .on_press(Message::ComposeToggleCc);

    let mut fields = Column::new()
        .push(field_row(t("from"), text(from).size(14).color(p.text).width(Fill).into()))
        .push(field_row(
            t("to"),
            dir_row(vec![input(&c.to, ComposeField::To, false), cc_toggle.into()])
                .align_y(Alignment::Center)
                .into(),
        ));
    if c.show_cc {
        fields = fields
            .push(field_row(t("cc"), input(&c.cc, ComposeField::Cc, false)))
            .push(field_row(t("bcc"), input(&c.bcc, ComposeField::Bcc, false)));
    }
    fields = fields.push(field_row(t("subject"), input(&c.subject, ComposeField::Subject, true)));

    // Format bar: plain text only for now (no rich-text toolbar until HTML sending exists).
    let format_bar = column![
        container(
            dir_row(vec![
                space::horizontal().into(),
                container(text(t("plain_text")).size(12).color(p.muted))
                    .padding([4, 8])
                    .style(move |_| container::Style {
                        border: Border { color: p.line, width: 1.0, radius: radius(6.0) },
                        ..Default::default()
                    })
                    .into(),
            ])
            .align_y(Alignment::Center),
        )
        .padding([6, 14]),
        hline(p.line_soft),
    ];

    let editor = text_editor(&c.body)
        .placeholder(t("body_placeholder"))
        .on_action(Message::ComposeEdit)
        .key_binding(|kp: KeyPress| {
            // Ctrl/⌘ + Enter sends.
            if kp.modifiers.command() && matches!(kp.key.as_ref(), Key::Named(Named::Enter)) {
                return Some(Binding::Custom(Message::ComposeSend));
            }
            Binding::from_key_press(kp)
        })
        .size(15)
        .line_height(1.6)
        .height(Fill)
        .padding([18, 20])
        .style(move |_, _| text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            placeholder: p.muted,
            value: p.text2,
            selection: mix(p.accent, p.bg, 0.7),
        });

    let mut attachments = iced::widget::Row::new().spacing(10);
    for (i, path) in c.attachments.iter().enumerate() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        attachments = attachments.push(
            container(
                row![
                    container(text(extension(&name)).font(MONO).size(10).color(p.accent_text))
                        .center_x(32)
                        .center_y(32)
                        .style(chip_box(p.chip, p.accent_text, 6.0)),
                    text(trunc(&name, 28)).size(13).font(medium()).color(p.text),
                    text(human_size(size)).size(12).color(p.muted),
                    button(icon(icons::CLOSE, 12.0, p.muted))
                        .padding(6)
                        .style(ghost(p, p.muted))
                        .on_press(Message::ComposeRemove(i)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 8.0, right: 6.0, bottom: 8.0, left: 8.0 })
            .style(card(p, p.line, 10.0)),
        );
    }

    let send_label = if c.sending { t("sending") } else { t("send") };
    let send = button(
        row![
            icon(icons::SEND, 16.0, p.on_accent),
            text(send_label).size(15).font(semibold()),
            container(text("Ctrl ↵").font(MONO).size(11).color(p.on_accent))
                .padding([2, 6])
                .style(move |_| container::Style {
                    background: Some(Background::Color(Color { a: 0.18, ..p.on_accent })),
                    border: Border { radius: radius(5.0), ..Default::default() },
                    ..Default::default()
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .height(44)
    .padding([12, 18])
    .style(accent_btn(p))
    .on_press_maybe((!c.sending).then_some(Message::ComposeSend));

    let attach = tooltip(
        button(icon(icons::CLIP, 18.0, p.text2))
            .width(44)
            .height(44)
            .padding(13)
            .style(ghost(p, p.text2))
            .on_press(Message::ComposeAttach),
        t("attach"),
        p,
    );
    let discard = tooltip(
        button(icon(icons::TRASH, 18.0, p.muted))
            .width(44)
            .height(44)
            .padding(13)
            .style(ghost(p, p.muted))
            .on_press(Message::ComposeClose),
        t("discard"),
        p,
    );
    let error: Element<'a, Message> = match &c.error {
        Some(e) => text(trunc(e, 90)).size(13).color(p.danger).width(Fill).into(),
        None => space::horizontal().into(),
    };
    let footer = column![
        hline(p.line_soft),
        container(dir_row(vec![send.into(), attach, error, discard]).spacing(8).align_y(Alignment::Center))
            .padding([12, 16])
            .width(Fill)
            .style(panel(p.chrome)),
    ];

    let mut body = column![head, fields, format_bar, editor];
    if !c.attachments.is_empty() {
        body = body.push(
            container(scrollable(attachments.wrap().vertical_spacing(10)).height(iced::Length::Shrink))
                .padding(Padding { top: 0.0, right: 20.0, bottom: 12.0, left: 20.0 })
                .max_height(140),
        );
    }
    body = body.push(footer);

    let dialog = container(body)
        .max_width(760)
        .max_height(640)
        .width(Fill)
        .height(Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.bg)),
            text_color: Some(p.text),
            border: Border { radius: radius(14.0), ..Default::default() },
            shadow: Shadow {
                color: Color::from_rgba(0.16, 0.10, 0.07, 0.25),
                offset: Vector::new(0.0, 12.0),
                blur_radius: 40.0,
            },
            ..Default::default()
        });

    // Dimmed backdrop; clicking it does not close (avoids losing a draft by accident).
    opaque(container(opaque(dialog))
        .center(Fill)
        .padding(24)
        .style(move |_| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
            ..Default::default()
        }))
}
