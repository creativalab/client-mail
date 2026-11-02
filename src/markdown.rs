//! Markdown → HTML for outgoing mail.
//!
//! The composer is a plain-text editor; formatting is written as Markdown
//! (`**bold**`, `- list`, `> quote`, …). On send, the Markdown source is the
//! `text/plain` part and this module renders the `text/html` part.

use pulldown_cmark::{CowStr, Event, Options, Parser, html};

fn options() -> Options {
    Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS
}

/// Renders the body fragment (no `<html>` wrapper).
///
/// - Raw HTML typed by the user (or present in quoted text) is escaped,
///   never passed through: quoted mail is untrusted input.
/// - Single newlines become `<br>`: in email, a line break means a line break.
pub fn to_html_fragment(md: &str) -> String {
    let events = Parser::new_ext(md, options()).map(|ev| match ev {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        Event::SoftBreak => Event::HardBreak,
        // Only allow safe link schemes.
        Event::Start(pulldown_cmark::Tag::Link { link_type, dest_url, title, id }) => {
            let dest_url = if is_safe_url(&dest_url) { dest_url } else { CowStr::from("") };
            Event::Start(pulldown_cmark::Tag::Link { link_type, dest_url, title, id })
        }
        Event::Start(pulldown_cmark::Tag::Image { dest_url, title, .. }) => {
            // Images are not embedded: show the alt text and the URL as text.
            Event::Text(CowStr::from(format!("{title} {dest_url}")))
        }
        Event::End(pulldown_cmark::TagEnd::Image) => Event::Text(CowStr::from("")),
        other => other,
    });
    let mut out = String::with_capacity(md.len() * 2);
    html::push_html(&mut out, events);
    out
}

fn is_safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|p| lower.starts_with(p))
        || !lower.contains(':')
}

/// Complete HTML document with a neutral inline style (mail clients ignore
/// `<style>` blocks inconsistently, so only basic inline styles are used).
pub fn to_html_document(md: &str) -> String {
    let body = to_html_fragment(md)
        .replace(
            "<blockquote>",
            "<blockquote style=\"margin:0 0 0 .8ex;border-left:2px solid #ccc;padding-left:1ex;color:#555\">",
        )
        .replace(
            "<pre>",
            "<pre style=\"background:#f4f4f4;padding:8px;border-radius:4px;overflow:auto\">",
        );
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"></head>\
         <body style=\"font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;\
         font-size:14px;line-height:1.5;color:#222\">\n{body}</body></html>\n"
    )
}

/// True when the text uses any Markdown construct worth an HTML part.
pub fn has_formatting(md: &str) -> bool {
    Parser::new_ext(md, options()).any(|ev| {
        matches!(
            ev,
            Event::Start(
                pulldown_cmark::Tag::Emphasis
                    | pulldown_cmark::Tag::Strong
                    | pulldown_cmark::Tag::Strikethrough
                    | pulldown_cmark::Tag::Link { .. }
                    | pulldown_cmark::Tag::List(_)
                    | pulldown_cmark::Tag::BlockQuote(_)
                    | pulldown_cmark::Tag::CodeBlock(_)
                    | pulldown_cmark::Tag::Heading { .. }
                    | pulldown_cmark::Tag::Table(_)
            ) | Event::Code(_)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_formatting() {
        let h = to_html_fragment("Hi **Giulia**,\nsee _this_.");
        assert!(h.contains("<strong>Giulia</strong>"));
        assert!(h.contains("<em>this</em>"));
        assert!(h.contains("<br />"));
    }

    #[test]
    fn raw_html_is_escaped() {
        let h = to_html_fragment("<script>alert(1)</script> and <b>x</b>");
        assert!(!h.contains("<script>"));
        assert!(h.contains("&lt;script&gt;"));
    }

    #[test]
    fn unsafe_links_dropped() {
        let h = to_html_fragment("[x](javascript:alert(1)) [y](https://ferrowl.com)");
        assert!(!h.contains("javascript:"));
        assert!(h.contains("href=\"https://ferrowl.com\""));
    }

    #[test]
    fn detects_formatting() {
        assert!(!has_formatting("plain text\nsecond line"));
        assert!(has_formatting("a **b**"));
        assert!(has_formatting("> quoted"));
    }
}
