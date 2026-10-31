//! Settings screen (Settings.dc.html): General (language), Accounts, Appearance.

use super::*;
use crate::app::{App, Conn, Field, Section};
use crate::config::Density;
use crate::i18n::{self, LANGS, t, tf};
use iced::widget::{Column, column, pick_list, row, scrollable, space, toggler};
use iced::{Alignment, Length::Fill, Padding};

pub fn view(app: &App, section: Section) -> Element<'_, Message> {
    let p = app.palette();
    let first_run = app.config.account.is_none();

    let back: Element<'_, Message> = if first_run {
        row![logo(&p, 32.0), text("Ferrowl").size(18).font(INTER_BOLD).color(p.text)]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    } else {
        button(
            row![
                icon(if i18n::is_rtl() { icons::CHEVRON_RIGHT } else { icons::CHEVRON_LEFT }, 18.0, p.text2),
                text(t("back_to_mail")).size(14).font(medium())
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .height(44)
        .padding([12, 12])
        .style(ghost(p, p.text2))
        .on_press(Message::CloseSettings)
        .into()
    };
    let header = container(
        dir_row(vec![
            back,
            text(t("settings")).size(18).font(INTER_BOLD).color(p.text).into(),
            space::horizontal().into(),
        ])
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .padding([12, 20])
    .width(Fill)
    .style(panel(p.chrome));

    let nav_btn = |s: Section, label: &'static str| {
        button(
            container(text(label).size(14).font(if s == section { semibold() } else { medium() }))
                .center_y(Fill),
        )
        .width(Fill)
        .height(40)
        .padding([0, 12])
            .style(nav_item(p, s == section))
            .on_press(Message::OpenSettings(s))
    };
    let nav = container(
        column![
            nav_btn(Section::General, t("section_general")),
            nav_btn(Section::Accounts, t("section_accounts")),
            nav_btn(Section::Appearance, t("section_appearance")),
            space::vertical(),
            container(
                text(tf("version", &[("v", env!("CARGO_PKG_VERSION"))]))
                    .font(MONO)
                    .size(12)
                    .color(p.muted)
            )
            .padding(Padding { top: 16.0, right: 12.0, bottom: 4.0, left: 12.0 }),
        ]
        .spacing(2),
    )
    .width(240)
    .height(Fill)
    .padding([16, 12])
    .style(panel(p.side));

    let content = match section {
        Section::General => general(app, p),
        Section::Accounts => accounts(app, p),
        Section::Appearance => appearance(app, p),
    };
    let main = scrollable(
        container(container(content).max_width(760))
            .padding(Padding { top: 32.0, right: 40.0, bottom: 48.0, left: 40.0 })
            .width(Fill),
    )
    .height(Fill)
    .style(scroll_style(p));

    let body = dir_row(vec![nav.into(), vline(p.line), main.into()]).height(Fill);
    let page = container(column![header, hline(p.line), body])
        .width(Fill)
        .height(Fill)
        .style(panel(p.bg));

    // Error/info notifications are shown here too.
    match super::mail_view::toast_layer(app, p) {
        Some(toast) => iced::widget::stack![page, toast].into(),
        None => page.into(),
    }
}

fn title<'a>(title: &'a str, desc: &'a str, p: Palette) -> Element<'a, Message> {
    column![
        text(title).size(26).font(INTER_BOLD).color(p.text),
        text(desc).size(15).color(p.muted)
    ]
    .spacing(6)
    .into()
}

/// Settings row: title + description on one side, control on the other.
fn setting_row<'a>(name: &'a str, desc: &'a str, control: Element<'a, Message>, p: Palette, last: bool) -> Element<'a, Message> {
    let r = container(
        dir_row(vec![
            column![
                text(name).size(15).font(medium()).color(p.text),
                text(desc).size(13).color(p.muted)
            ]
            .spacing(2)
            .width(Fill)
            .into(),
            control,
        ])
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .padding([16, 0]);
    if last {
        r.into()
    } else {
        column![r, hline(p.line_soft)].into()
    }
}

fn group<'a>(rows: Vec<Element<'a, Message>>, p: Palette) -> Element<'a, Message> {
    container(Column::with_children(rows))
        .padding([4, 20])
        .width(Fill)
        .style(card(p, p.line, 14.0))
        .into()
}

// ---------------------------------------------------------------- General

#[derive(Debug, Clone, PartialEq, Eq)]
struct LangChoice {
    code: &'static str,
    label: String,
}

impl std::fmt::Display for LangChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn general(app: &App, p: Palette) -> Element<'_, Message> {
    let mut choices = vec![LangChoice {
        code: "auto",
        label: tf("language_system", &[("lang", i18n::system_lang().native)]),
    }];
    choices.extend(LANGS.iter().map(|l| LangChoice { code: l.code, label: l.native.to_string() }));
    let selected = choices
        .iter()
        .find(|c| c.code == app.config.ui.language)
        .cloned()
        .or_else(|| choices.first().cloned());
    let picker = pick_list(choices, selected, |c: LangChoice| Message::SetLanguage(c.code.to_string()))
        .text_size(14)
        .padding([10, 12])
        .width(220);

    column![
        title(t("section_general"), t("general_desc"), p),
        group(vec![setting_row(t("language"), t("language_desc"), picker.into(), p, true)], p),
    ]
    .spacing(28)
    .into()
}

// ---------------------------------------------------------------- Accounts

fn accounts(app: &App, p: Palette) -> Element<'_, Message> {
    let first_run = app.config.account.is_none();
    let heading = if first_run {
        title(t("setup_title"), t("setup_desc"), p)
    } else {
        title(t("section_accounts"), t("accounts_desc"), p)
    };

    let mut col = column![heading].spacing(28);

    if let Some(acc) = &app.config.account {
        let (badge_bg, badge_fg, badge) = if app.conn == Conn::Online {
            (p.sec_bg, p.sec_text, t("account_connected"))
        } else {
            (p.chip, p.text2, t("account_offline"))
        };
        let name = if acc.display_name.is_empty() { acc.email.as_str() } else { acc.display_name.as_str() };
        col = col.push(
            container(
                dir_row(vec![
                    avatar(acc.initials(), p.ink, p.on_ink, 40.0),
                    column![
                        text(name).size(15).font(semibold()).color(p.text),
                        text(format!("{} · IMAP / SMTP", acc.email)).size(13).color(p.muted)
                    ]
                    .spacing(2)
                    .width(Fill)
                    .into(),
                    container(text(badge).size(12).font(semibold()))
                        .padding([4, 10])
                        .style(chip_box(badge_bg, badge_fg, 999.0))
                        .into(),
                ])
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .padding([14, 16])
            .style(move |_| container::Style {
                background: Some(Background::Color(p.soft)),
                border: Border { color: p.accent, width: 2.0, radius: radius(14.0) },
                ..Default::default()
            }),
        );
    }

    let d = &app.draft;
    let field = |label: &'static str, value: &str, f: Field, secure: bool, mono: bool| -> Element<'_, Message> {
        column![
            text(label).size(13).font(medium()).color(p.text2),
            text_input("", value)
                .on_input(move |v| Message::Draft(f, v))
                .secure(secure)
                .size(14)
                .padding([10, 12])
                .font(if mono { MONO } else { INTER })
                .style(input(p, 8.0))
        ]
        .spacing(6)
        .width(Fill)
        .into()
    };
    let port = |label: &'static str, value: &str, f: Field| -> Element<'_, Message> {
        column![
            text(label).size(13).font(medium()).color(p.text2),
            text_input("", value)
                .on_input(move |v| Message::Draft(f, v.chars().filter(char::is_ascii_digit).collect()))
                .size(14)
                .padding([10, 12])
                .font(MONO)
                .width(96)
                .style(input(p, 8.0))
        ]
        .spacing(6)
        .into()
    };
    let pair = |a: Element<'static, Message>, b: Element<'static, Message>| dir_row(vec![a, b]).spacing(16);

    let username_hint: Element<'_, Message> = if d.username.is_empty() && !d.email.is_empty() {
        button(text(t("use_email_as_username")).size(13).color(p.accent_text))
            .padding([2, 0])
            .style(ghost(p, p.accent_text))
            .on_press(Message::UseEmailAsUsername)
            .into()
    } else {
        space().into()
    };

    let can_test = d.to_account().is_some() && !app.testing;
    let test_label = if app.testing { t("testing") } else { t("test_connection") };
    let status: Element<'_, Message> = match &app.test_result {
        Some(Ok(msg)) => text(msg.clone()).size(13).color(p.sec_text).width(Fill).into(),
        Some(Err(e)) => text(trunc(e, 120)).size(13).color(p.danger).width(Fill).into(),
        None => space::horizontal().into(),
    };

    let details = container(
        column![
            text(t("account_details")).size(16).font(INTER_BOLD).color(p.text),
            pair(
                field(t("display_name"), &d.display_name, Field::DisplayName, false, false),
                field(t("email"), &d.email, Field::Email, false, false)
            ),
            pair(
                column![field(t("username"), &d.username, Field::Username, false, false), username_hint]
                    .spacing(4)
                    .width(Fill)
                    .into(),
                field(t("password"), &d.password, Field::Password, true, false)
            ),
            dir_row(vec![
                field(t("imap_server"), &d.imap_host, Field::ImapHost, false, true),
                port(t("port"), &d.imap_port, Field::ImapPort),
            ])
            .spacing(16),
            dir_row(vec![
                field(t("smtp_server"), &d.smtp_host, Field::SmtpHost, false, true),
                port(t("port"), &d.smtp_port, Field::SmtpPort),
            ])
            .spacing(16),
            text(t("server_hint")).size(12).color(p.muted),
            dir_row(vec![
                button(text(test_label).size(14).font(medium()))
                    .height(40)
                    .padding([10, 16])
                    .style(outline_btn(p, p.text))
                    .on_press_maybe(can_test.then_some(Message::TestConnection))
                    .into(),
                status,
                button(text(t("save")).size(14).font(semibold()))
                    .height(40)
                    .padding([10, 16])
                    .style(accent_btn(p))
                    .on_press_maybe(d.to_account().map(|_| Message::SaveAccount))
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(18),
    )
    .padding(20)
    .width(Fill)
    .style(card(p, p.line, 14.0));

    col.push(details).into()
}

// ---------------------------------------------------------------- Appearance

fn appearance(app: &App, p: Palette) -> Element<'_, Message> {
    let current = app.config.ui.density;
    let seg = |d: Density, label: &'static str| {
        button(text(label).size(13).font(if d == current { semibold() } else { INTER }))
            .padding([6, 12])
            .style(segment(p, d == current))
            .on_press(Message::SetDensity(d))
    };
    let density = container(
        row![
            seg(Density::Compact, t("density_compact")),
            seg(Density::Comfortable, t("density_comfortable")),
            seg(Density::Spacious, t("density_spacious")),
        ]
        .spacing(4),
    )
    .padding(3)
    .style(chip_box(p.chip, p.text2, 9.0));

    let preview = toggler(app.config.ui.show_preview)
        .on_toggle(Message::TogglePreview)
        .size(24)
        .style(move |_, st| {
            let on = matches!(st, iced::widget::toggler::Status::Active { is_toggled: true }
                | iced::widget::toggler::Status::Hovered { is_toggled: true });
            iced::widget::toggler::Style {
                background: Background::Color(if on { p.accent } else { p.knob_off }),
                background_border_width: 0.0,
                background_border_color: Color::TRANSPARENT,
                foreground: Background::Color(Color::WHITE),
                foreground_border_width: 0.0,
                foreground_border_color: Color::TRANSPARENT,
                text_color: None,
                border_radius: None,
                padding_ratio: 0.12,
            }
        });

    column![
        title(t("section_appearance"), t("appearance_desc"), p),
        column![
            text(t("color_theme")).size(14).font(semibold()).color(p.text2),
            theme_grid(&app.config.ui.theme, p, 84.0, 12.0)
        ]
        .spacing(12),
        group(
            vec![
                setting_row(t("density"), t("density_desc"), density.into(), p, false),
                setting_row(t("preview"), t("preview_desc"), preview.into(), p, true),
            ],
            p
        ),
    ]
    .spacing(28)
    .into()
}
