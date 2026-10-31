//! Ferrowl's 6 themes, read from `design/themes.json` (embedded in the binary).

use iced::Color;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Display order in the Appearance panel.
pub const ORDER: [&str; 6] = ["ruggine", "ardesia", "bosco", "lavanda", "sabbia", "notte"];

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub accent: Color,
    pub on_accent: Color,
    pub accent_text: Color,
    pub active_text: Color,
    pub soft: Color,
    pub bg: Color,
    pub chrome: Color,
    pub side: Color,
    pub side_active: Color,
    pub line: Color,
    pub line_soft: Color,
    pub field: Color,
    pub text: Color,
    pub text2: Color,
    pub muted: Color,
    pub chip: Color,
    pub ink: Color,
    pub on_ink: Color,
    pub sec_bg: Color,
    pub sec_text: Color,
    pub knob_off: Color,
    pub logo: Color,
    pub danger: Color,
    pub danger_line: Color,
}

#[derive(Debug, Clone)]
pub struct ThemeDef {
    pub key: &'static str,
    pub name: String,
    pub palette: Palette,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTheme {
    name: String,
    dark: bool,
    accent: String,
    on_accent: String,
    accent_text: String,
    active_text: String,
    soft: String,
    bg: String,
    chrome: String,
    side: String,
    side_active: String,
    line: String,
    line_soft: String,
    field: String,
    text: String,
    text2: String,
    muted: String,
    chip: String,
    ink: String,
    on_ink: String,
    sec_bg: String,
    sec_text: String,
    knob_off: String,
    logo: String,
    danger: String,
    danger_line: String,
}

#[derive(Deserialize)]
struct RawFile {
    themes: HashMap<String, RawTheme>,
}

pub fn hex(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).unwrap_or(0);
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn all() -> &'static Vec<ThemeDef> {
    static THEMES: OnceLock<Vec<ThemeDef>> = OnceLock::new();
    THEMES.get_or_init(|| {
        let file: RawFile = serde_json::from_str(include_str!("../design/themes.json"))
            .expect("design/themes.json invalid");
        ORDER
            .iter()
            .filter_map(|key| {
                let r = file.themes.get(*key)?;
                Some(ThemeDef {
                    key,
                    name: r.name.clone(),
                    palette: Palette {
                        dark: r.dark,
                        accent: hex(&r.accent),
                        on_accent: hex(&r.on_accent),
                        accent_text: hex(&r.accent_text),
                        active_text: hex(&r.active_text),
                        soft: hex(&r.soft),
                        bg: hex(&r.bg),
                        chrome: hex(&r.chrome),
                        side: hex(&r.side),
                        side_active: hex(&r.side_active),
                        line: hex(&r.line),
                        line_soft: hex(&r.line_soft),
                        field: hex(&r.field),
                        text: hex(&r.text),
                        text2: hex(&r.text2),
                        muted: hex(&r.muted),
                        chip: hex(&r.chip),
                        ink: hex(&r.ink),
                        on_ink: hex(&r.on_ink),
                        sec_bg: hex(&r.sec_bg),
                        sec_text: hex(&r.sec_text),
                        knob_off: hex(&r.knob_off),
                        logo: hex(&r.logo),
                        danger: hex(&r.danger),
                        danger_line: hex(&r.danger_line),
                    },
                })
            })
            .collect()
    })
}

pub fn themes() -> &'static [ThemeDef] {
    all()
}

pub fn get(key: &str) -> &'static ThemeDef {
    let all = all();
    all.iter()
        .find(|t| t.key == key)
        .unwrap_or_else(|| &all[0])
}

/// Base iced theme: colors widgets we don't style explicitly.
pub fn iced_theme(def: &ThemeDef) -> iced::Theme {
    let p = &def.palette;
    iced::Theme::custom(
        def.name.clone(),
        iced::theme::Palette {
            background: p.bg,
            text: p.text,
            primary: p.accent,
            success: p.sec_text,
            warning: hex("#C77C02"),
            danger: p.danger,
        },
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_six_themes_load() {
        assert_eq!(super::themes().len(), 6);
        assert!(super::get("notte").palette.dark);
        assert_eq!(super::get("inesistente").key, "ruggine");
    }
}
