//! Received HTML mail → Markdown, rendered with iced's markdown widget.
//!
//! There is no web engine: the HTML is reduced to text structure (paragraphs,
//! emphasis, links, lists, quotes, headings, tables with headers). Scripts,
//! styles, forms and embedded objects are dropped. Images are never loaded
//! (no tracking pixels): only their alt text is kept.

use htmd::{Element, HtmlToMarkdown, element_handler::Handlers};

pub fn to_markdown(html: &str) -> String {
    let converter = HtmlToMarkdown::builder()
        .skip_tags(vec![
            "script", "style", "head", "title", "meta", "link", "noscript", "template",
            "iframe", "object", "embed", "svg", "form", "input", "button", "select",
            "textarea",
        ])
        .add_handler(vec!["img"], |_: &dyn Handlers, el: Element| {
            let alt = el
                .attrs
                .iter()
                .find(|a| &*a.name.local == "alt")
                .map(|a| a.value.trim().replace(['[', ']', '*', '_', '`'], ""))
                .unwrap_or_default();
            // Marked so a link that also has text can drop it (see below).
            Some(if alt.is_empty() { String::new() } else { format!("{ALT_START}{alt}{ALT_END}") }.into())
        })
        .add_handler(vec!["a"], |h: &dyn Handlers, el: Element| {
            let inner = h.walk_children(el.node).content;
            let block = inner.contains("\n\n");
            // Prefer real text over image alt text inside the same link.
            let text_only = strip_alt(&inner);
            let inner = if text_only.trim().is_empty() { inner } else { text_only };
            // Link text on one line: newsletters wrap whole blocks in <a>,
            // and a line break inside [...] would break the Markdown.
            let label = inner
                .split_whitespace()
                .filter(|w| !w.chars().all(|c| c == '#' || c == '*' || c == '>'))
                .collect::<Vec<_>>()
                .join(" ")
                .replace(['[', ']', ALT_START, ALT_END], "");
            let label = dedupe_halves(&label);
            let href = el
                .attrs
                .iter()
                .find(|a| &*a.name.local == "href")
                .map(|a| a.value.trim().to_string())
                .unwrap_or_default();
            let out = if label.is_empty() {
                String::new()
            } else if is_openable(&href) {
                format!("[{label}](<{}>)", href.replace(['<', '>', ' '], ""))
            } else {
                label
            };
            Some(if block { format!("\n\n{out}\n\n") } else { out }.into())
        })
        .build();
    let md = converter.convert(html).unwrap_or_default();
    clean(&md.replace([ALT_START, ALT_END], ""))
}

// Private-use markers around image alt text during conversion.
const ALT_START: char = '\u{e000}';
const ALT_END: char = '\u{e001}';

/// "Title Title" → "Title" (cards often repeat the same text twice in one link).
fn dedupe_halves(s: &str) -> String {
    let words: Vec<&str> = s.split(' ').collect();
    let norm: Vec<&str> = words.iter().map(|w| w.trim_matches(['*', '_'])).collect();
    let n = words.len();
    if n >= 2 && n % 2 == 0 && norm[..n / 2] == norm[n / 2..] {
        words[..n / 2].join(" ")
    } else {
        s.to_string()
    }
}

fn strip_alt(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            ALT_START => inside = true,
            ALT_END => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

/// Removes the invisible filler common in newsletters (preheader padding,
/// zero-width characters) and collapses runs of blank lines.
fn clean(md: &str) -> String {
    const INVISIBLE: &[char] = &[
        '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}', '\u{034f}', '\u{00ad}',
    ];
    let mut out = String::with_capacity(md.len());
    let mut blank = 0;
    for line in md.lines() {
        let line: String = line.chars().filter(|c| !INVISIBLE.contains(c)).collect();
        let line = line.replace('\u{a0}', " ");
        if line.trim().is_empty() {
            blank += 1;
            if blank == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank = 0;
        out.push_str(line.trim_end_matches(' ').trim_end());
        // Keep Markdown hard breaks ("two spaces") as an explicit backslash.
        if line.ends_with("  ") {
            out.push('\\');
        }
        out.push('\n');
    }
    out.trim().to_string()
}

/// Links that may be opened from a message.
pub fn is_openable(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_structure_and_drops_unsafe_parts() {
        let html = r#"<html><head><style>p{color:red}</style><title>x</title></head>
            <body><script>alert(1)</script>
            <p>Hello <b>Fabio</b>,</p>
            <p>see <a href="https://ferrowl.com">the site</a>.</p>
            <img src="https://tracker.example/p.gif" width="1" height="1">
            <ul><li>one</li><li>two</li></ul></body></html>"#;
        let md = to_markdown(html);
        assert!(md.contains("Hello **Fabio**,"));
        assert!(md.contains("[the site](<https://ferrowl.com>)"));
        assert!(!md.contains("alert"));
        assert!(!md.contains("color:red"));
        assert!(!md.contains("tracker"));
        assert!(md.contains("one") && md.contains("two"));
    }

    #[test]
    fn block_links_stay_on_one_line() {
        let md = to_markdown(
            r#"<a href="https://x.it/a"><h2>Big title</h2><p>sub text</p></a><a href="javascript:x">bad</a>"#,
        );
        assert!(md.contains("[Big title sub text](<https://x.it/a>)"), "{md}");
        assert!(!md.contains("javascript"));
    }

    #[test]
    fn link_text_preferred_over_alt() {
        let md = to_markdown(r#"<a href="https://x.it"><img alt="Logo image" src="x.png"> Shop now</a>"#);
        assert!(md.contains("[Shop now](<https://x.it>)"), "{md}");
        let md = to_markdown(r#"<a href="https://x.it"><img alt="Play" src="x.png"></a>"#);
        assert!(md.contains("[Play](<https://x.it>)"), "{md}");
        let md = to_markdown(r#"<a href="https://x.it"><b>Deal 27</b><br>Deal 27</a>"#);
        assert!(md.contains("Deal 27**](<https://x.it>)") && md.matches("Deal").count() == 1, "{md}");
    }

    #[test]
    fn strips_invisible_padding() {
        let md = to_markdown("<p>Hi</p><div>\u{200c}&nbsp;\u{200c}&nbsp;</div><p></p><p>Bye</p>");
        assert_eq!(md, "Hi\n\nBye");
    }

    #[test]
    fn openable_links() {
        assert!(is_openable("https://x.it"));
        assert!(is_openable("mailto:a@b.it"));
        assert!(!is_openable("javascript:alert(1)"));
        assert!(!is_openable("file:///etc/passwd"));
    }
}
