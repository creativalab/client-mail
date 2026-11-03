//! Calendar pane: the month's events as an agenda (grouped by day) in
//! place of the message list, the open event in place of the reader.
//! Events come from the account's CalDAV server (see `ferrowl_core::dav`).

use super::*;
use crate::app::{App, SyncState};
use crate::i18n::t;
use chrono::{Datelike, NaiveDate};
use ferrowl_core::backend::AgendaEntry;
use ferrowl_core::ical::{self, day_label, month_label};
use iced::widget::{Column, column, scrollable, space};
use iced::{Alignment, Length::Fill, Padding};

/// Calendar color from the server ("#RRGGBB" or "#RRGGBBAA"), else the
/// theme's accent.
fn color_of(e: &AgendaEntry, p: Palette) -> Color {
    let hex = e.color.trim();
    if hex.len() >= 7 && hex.starts_with('#') {
        crate::theme::hex(&hex[..7])
    } else {
        p.accent
    }
}

/// "09:00 – 10:30", or "All day".
fn time_range(e: &AgendaEntry) -> String {
    let o = &e.occurrence;
    if o.all_day {
        return t("all_day").to_string();
    }
    let (s, end) = (ical::local(o.start), ical::local(o.end));
    if o.end <= o.start {
        s.format("%H:%M").to_string()
    } else if s.date_naive() == end.date_naive() {
        format!("{} – {}", s.format("%H:%M"), end.format("%H:%M"))
    } else {
        format!(
            "{} – {} {}",
            s.format("%H:%M"),
            day_label(end.date_naive()),
            end.format("%H:%M")
        )
    }
}

/// The days of the shown month an occurrence appears on (multi-day events
/// are listed on each day).
fn days_of(e: &AgendaEntry, month: NaiveDate) -> Vec<NaiveDate> {
    let o = &e.occurrence;
    let (first, last) = match (o.first_day, o.last_day) {
        (Some(f), Some(l)) => (f, l),
        // Timed: from the start day to the day of the last second.
        _ => (
            ical::local(o.start).date_naive(),
            ical::local((o.end - 1).max(o.start)).date_naive(),
        ),
    };
    first
        .iter_days()
        .take_while(|d| *d <= last)
        .filter(|d| d.year() == month.year() && d.month() == month.month())
        .take(31)
        .collect()
}

pub fn list<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let syncing = app.calendar_sync == SyncState::Running;
    let nav = |ic: &'static str, tip: &'static str, delta: i32| {
        tooltip(
            button(icon(ic, 18.0, p.text2))
                .padding(8)
                .style(ghost(p, p.text2))
                .on_press(Message::CalendarMonth(delta)),
            tip,
            p,
        )
    };
    // Arrows point the reading direction's way (right-to-left in Arabic).
    let (back, forward) = if crate::i18n::is_rtl() {
        (icons::CHEVRON_RIGHT, icons::CHEVRON_LEFT)
    } else {
        (icons::CHEVRON_LEFT, icons::CHEVRON_RIGHT)
    };
    let header = dir_row(vec![
        text(month_label(app.cal_month))
            .size(20)
            .font(INTER_BOLD)
            .width(Fill)
            .into(),
        nav(back, t("previous_month"), -1),
        button(text(t("today")).size(13).font(medium()))
            .padding([6, 10])
            .style(outline_btn(p, p.text))
            .on_press(Message::CalendarMonth(0))
            .into(),
        nav(forward, t("next_month"), 1),
        tooltip(
            button(icon(icons::SYNC, 18.0, p.text2))
                .padding(8)
                .style(ghost(p, p.text2))
                .on_press_maybe((!syncing).then_some(Message::SyncCalendar)),
            t("calendar_sync"),
            p,
        ),
    ])
    .spacing(4)
    .align_y(Alignment::Center)
    .padding(Padding {
        top: 16.0,
        right: 12.0,
        bottom: 8.0,
        left: 20.0,
    });

    let status: Option<Element<'a, Message>> = match &app.calendar_sync {
        SyncState::Running => Some(text(t("calendar_syncing")).size(12).color(p.muted).into()),
        SyncState::Failed(e) => Some(text(e.as_str()).size(12).color(p.danger).into()),
        _ => None,
    };

    // Day headings with that day's events under them.
    let today = ical::today();
    let mut days: std::collections::BTreeMap<NaiveDate, Vec<usize>> = Default::default();
    for (i, e) in app.agenda.iter().enumerate() {
        for d in days_of(e, app.cal_month) {
            days.entry(d).or_default().push(i);
        }
    }
    let content: Element<'a, Message> = if days.is_empty() {
        container(text(t("calendar_empty")).size(14).color(p.muted))
            .padding(20)
            .into()
    } else {
        let mut col = Column::new().padding(Padding {
            bottom: 16.0,
            ..Padding::ZERO
        });
        for (day, items) in days {
            let is_today = day == today;
            let mut heading = vec![
                text(day_label(day))
                    .size(13)
                    .font(semibold())
                    .color(if is_today { p.accent_text } else { p.text2 })
                    .into(),
            ];
            if is_today {
                heading.push(
                    container(text(t("today")).size(11).font(semibold()).color(p.on_accent))
                        .padding([1, 8])
                        .style(chip_box(p.accent, p.on_accent, 8.0))
                        .into(),
                );
            }
            col = col.push(
                container(dir_row(heading).spacing(8).align_y(Alignment::Center)).padding(Padding {
                    top: 14.0,
                    right: 20.0,
                    bottom: 4.0,
                    left: 20.0,
                }),
            );
            for i in items {
                col = col.push(event_row(app, i, p));
            }
        }
        scrollable(col).height(Fill).style(scroll_style(p)).into()
    };

    let mut top = Column::new().push(header);
    if let Some(s) = status {
        top = top.push(container(s).padding([0, 20]));
    }
    container(column![top, space().height(4), hline(p.line_soft), content])
        .width(410)
        .height(Fill)
        .style(panel(p.bg))
        .into()
}

fn event_row<'a>(app: &'a App, i: usize, p: Palette) -> Element<'a, Message> {
    let e = &app.agenda[i];
    let active = app.agenda_selected == Some(i);
    let color = color_of(e, p);
    let mut lines = Column::new().spacing(2).width(Fill).push(
        text(trunc(&e.occurrence.summary, 40))
            .size(14)
            .font(semibold())
            .color(p.text),
    );
    let mut meta = time_range(e);
    if !e.occurrence.location.is_empty() {
        meta = format!("{meta} · {}", e.occurrence.location);
    }
    lines = lines.push(text(trunc(&meta, 52)).size(12).color(p.text2));
    button(
        dir_row(vec![
            container(space().width(4).height(34))
                .style(chip_box(color, color, 2.0))
                .into(),
            lines.into(),
        ])
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([8, 20])
    .style(nav_item(p, active))
    .on_press(Message::AgendaSelect(i))
    .into()
}

pub fn detail<'a>(app: &'a App, p: Palette) -> Element<'a, Message> {
    let Some(e) = app.agenda_selected.and_then(|i| app.agenda.get(i)) else {
        let inner = column![
            icon(icons::CALENDAR, 48.0, p.field),
            text(t("calendar")).size(15).color(p.muted)
        ]
        .spacing(12)
        .align_x(Alignment::Center);
        return container(inner).center(Fill).style(panel(p.bg)).into();
    };
    let o = &e.occurrence;
    let color = color_of(e, p);
    let line = |ic: &'static str, value: String| -> Element<'a, Message> {
        dir_row(vec![
            container(icon(ic, 16.0, p.muted)).padding([2, 0]).into(),
            text(value).size(14).color(p.text).width(Fill).into(),
        ])
        .spacing(12)
        .into()
    };

    let day = match o.first_day {
        Some(d) => d,
        None => ical::local(o.start).date_naive(),
    };
    let mut when = day_label(day);
    if let Some(last) = o.last_day.filter(|l| *l != day) {
        when = format!("{when} – {}", day_label(last));
    }
    when = format!("{when}\n{}", time_range(e));

    let mut body = Column::new().spacing(14).push(
        dir_row(vec![
            container(space().width(6).height(30))
                .style(chip_box(color, color, 3.0))
                .into(),
            text(o.summary.as_str())
                .size(22)
                .font(INTER_BOLD)
                .color(p.text)
                .width(Fill)
                .into(),
        ])
        .spacing(14)
        .align_y(Alignment::Center),
    );
    body = body.push(space().height(4)).push(line(icons::CLOCK, when));
    if o.recurring {
        body = body.push(line(icons::REPEAT, t("event_repeats").to_string()));
    }
    if !o.location.is_empty() {
        body = body.push(line(icons::PIN, o.location.clone()));
    }
    if !e.calendar.is_empty() {
        body = body.push(line(icons::CALENDAR, e.calendar.clone()));
    }
    if !o.organizer.is_empty() {
        body = body.push(line(icons::MAIL, format!("{}: {}", t("organizer"), o.organizer)));
    }
    if !o.description.trim().is_empty() {
        body = body.push(space().height(4)).push(
            container(text(o.description.as_str()).size(14).color(p.text2))
                .padding(14)
                .width(Fill)
                .style(chip_box(p.chip, p.text2, 10.0)),
        );
    }

    container(scrollable(container(body).padding(32).max_width(720)).style(scroll_style(p)))
        .width(Fill)
        .height(Fill)
        .style(panel(p.bg))
        .into()
}
