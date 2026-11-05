//! Links in plain-text mail: finding them in a line, coloring them in the
//! read-only body editor and opening the one under a click.

use iced::Color;
use iced::advanced::text::highlighter;
use std::ops::Range;

/// Byte ranges of the links in a line, with the URL each opens
/// (`www.` gets `https://`).
pub fn find(line: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices().chain(std::iter::once((line.len(), ' '))) {
        let separator = c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | '«' | '»');
        match (separator, start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                if let Some(link) = link_at(line, s, i) {
                    out.push(link);
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// The link in the word `line[start..end]`, if it is one.
fn link_at(line: &str, start: usize, end: usize) -> Option<(Range<usize>, String)> {
    let word = &line[start..end];
    // A word may start with an opening bracket: "(https://…)".
    let lead = word.len() - word.trim_start_matches(['(', '[', '{']).len();
    let word = &word[lead..];
    let lower = word.to_ascii_lowercase();
    let scheme = ["https://", "http://", "mailto:", "www."]
        .into_iter()
        .find(|p| lower.starts_with(p))?;
    // Trailing punctuation belongs to the sentence, not to the link; a
    // closing bracket stays only when the link opened one ("…/Foo_(bar)").
    let mut url = word;
    loop {
        let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        let trimmed = match trimmed.chars().last() {
            Some(c @ (')' | ']' | '}')) => {
                let open = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if trimmed.matches(open).count() < trimmed.matches(c).count() {
                    &trimmed[..trimmed.len() - 1]
                } else {
                    trimmed
                }
            }
            _ => trimmed,
        };
        if trimmed.len() == url.len() {
            break;
        }
        url = trimmed;
    }
    if url.len() <= scheme.len() {
        return None;
    }
    let target = if scheme == "www." {
        format!("https://{url}")
    } else {
        url.to_string()
    };
    let from = start + lead;
    Some((from..from + url.len(), target))
}

/// The link at byte `column` of `line`, if the position falls on one.
pub fn at(line: &str, column: usize) -> Option<String> {
    find(line)
        .into_iter()
        .find(|(range, _)| range.contains(&column))
        .map(|(_, url)| url)
}

/// Colors the links of the read-only body editor (`Settings` = their color).
pub struct Highlighter {
    color: Color,
    line: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Settings(pub Color);

impl highlighter::Highlighter for Highlighter {
    type Settings = Settings;
    type Highlight = Color;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Color)>;

    fn new(settings: &Settings) -> Self {
        Highlighter {
            color: settings.0,
            line: 0,
        }
    }

    fn update(&mut self, settings: &Settings) {
        self.color = settings.0;
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line += 1;
        let color = self.color;
        find(line)
            .into_iter()
            .map(|(range, _)| (range, color))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// Highlight to text format: links take their color.
pub fn format(color: &Color, _theme: &iced::Theme) -> highlighter::Format<iced::Font> {
    highlighter::Format {
        color: Some(*color),
        font: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(line: &str) -> Vec<String> {
        find(line).into_iter().map(|(_, u)| u).collect()
    }

    #[test]
    fn finds_links_and_drops_punctuation() {
        assert_eq!(
            urls("Vedi https://biglietti.com/evento?id=3. Oppure www.ferrowl.com, grazie"),
            ["https://biglietti.com/evento?id=3", "https://www.ferrowl.com"]
        );
        assert_eq!(
            urls("(https://a.it/x) e <http://b.it>"),
            ["https://a.it/x", "http://b.it"]
        );
        assert_eq!(
            urls("https://en.wikipedia.org/wiki/Foo_(bar)"),
            ["https://en.wikipedia.org/wiki/Foo_(bar)"]
        );
        assert_eq!(
            urls("scrivi a mailto:info@biglietti.com!"),
            ["mailto:info@biglietti.com"]
        );
        assert!(urls("nessun link qui, solo http:// e www.").is_empty());
    }

    #[test]
    fn link_under_a_column() {
        let line = "Apri https://a.it/x ora";
        assert_eq!(at(line, 5).as_deref(), Some("https://a.it/x"));
        assert_eq!(at(line, 18).as_deref(), Some("https://a.it/x"));
        assert_eq!(at(line, 19), None);
        assert_eq!(at(line, 2), None);
    }
}
