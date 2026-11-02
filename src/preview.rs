//! Attachment previews: images, PDF (rendered with hayro, pure Rust) and
//! text files. Everything runs off the UI thread; results are image handles
//! or text ready to display.

use iced::widget::image;

/// What kind of preview an attachment can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Svg,
    Pdf,
    Text,
    None,
}

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff"];
const TEXT_EXT: &[&str] = &[
    "txt", "text", "csv", "tsv", "log", "md", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf",
    "ics", "vcf", "html", "htm", "css", "js", "sql", "sh", "rs", "py", "eml",
];
/// Text previews are cut here.
const MAX_TEXT: usize = 256 * 1024;
const MAX_PDF_PAGES: usize = 50;

pub fn kind(name: &str, data: &[u8]) -> Kind {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if data.starts_with(b"%PDF") || ext == "pdf" {
        Kind::Pdf
    } else if ext == "svg" {
        Kind::Svg
    } else if IMAGE_EXT.contains(&ext.as_str())
        || data.starts_with(b"\x89PNG")
        || data.starts_with(b"\xff\xd8\xff")
        || data.starts_with(b"GIF8")
    {
        Kind::Image
    } else if TEXT_EXT.contains(&ext.as_str()) {
        Kind::Text
    } else {
        Kind::None
    }
}

/// A rendered PDF page: handle, width and height in pixels.
pub type Page = (image::Handle, u32, u32);

#[derive(Debug, Clone)]
pub enum Content {
    Image(image::Handle),
    Svg(iced::widget::svg::Handle),
    /// PDF pages and the total page count.
    Pages(Vec<Page>, usize),
    Text(String, bool),
    Error(String),
}

/// Full preview (blocking: call from a background task).
pub fn build(name: &str, data: &[u8]) -> Content {
    match kind(name, data) {
        Kind::Image => Content::Image(image::Handle::from_bytes(data.to_vec())),
        Kind::Svg => Content::Svg(iced::widget::svg::Handle::from_memory(data.to_vec())),
        Kind::Pdf => match pdf_pages(data, 1100.0, MAX_PDF_PAGES) {
            Ok((pages, total)) => Content::Pages(pages, total),
            Err(e) => Content::Error(e),
        },
        Kind::Text => {
            let cut = data.len() > MAX_TEXT;
            let text = String::from_utf8_lossy(&data[..data.len().min(MAX_TEXT)]).into_owned();
            Content::Text(text, cut)
        }
        Kind::None => Content::Error(String::new()),
    }
}

/// Small image for the attachment card, if the type has one.
pub fn thumbnail(name: &str, data: &[u8]) -> Option<image::Handle> {
    match kind(name, data) {
        Kind::Image => Some(image::Handle::from_bytes(data.to_vec())),
        Kind::Pdf => pdf_pages(data, 160.0, 1)
            .ok()?
            .0
            .into_iter()
            .next()
            .map(|(h, _, _)| h),
        _ => None,
    }
}

/// Renders up to `max` pages, each `width` pixels wide. Returns the pages
/// and the total page count.
fn pdf_pages(data: &[u8], width: f32, max: usize) -> Result<(Vec<Page>, usize), String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::{PixmapSettings, RenderCache, RenderSettings};

    let pdf = Pdf::new(data.to_vec()).map_err(|e| format!("{e:?}"))?;
    let pages = pdf.pages();
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let mut out = Vec::new();
    for page in pages.iter().take(max) {
        let (w, _) = page.render_dimensions();
        let scale = (width / w.max(1.0)).clamp(0.05, 4.0);
        let pixmap = hayro::render(
            page,
            &cache,
            &settings,
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: hayro::vello_cpu::color::palette::css::WHITE,
            },
        );
        let (pw, ph) = (pixmap.width() as u32, pixmap.height() as u32);
        // Opaque white background: premultiplied equals straight RGBA.
        let rgba = pixmap.data_as_u8_slice().to_vec();
        out.push((image::Handle::from_rgba(pw, ph, rgba), pw, ph));
    }
    Ok((out, pages.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind("a.PNG", b""), Kind::Image);
        assert_eq!(kind("scan", b"\xff\xd8\xff\xe0"), Kind::Image);
        assert_eq!(kind("doc.pdf", b""), Kind::Pdf);
        assert_eq!(kind("x.bin", b"%PDF-1.7"), Kind::Pdf);
        assert_eq!(kind("notes.TXT", b""), Kind::Text);
        assert_eq!(kind("logo.svg", b""), Kind::Svg);
        assert_eq!(kind("archive.zip", b"PK"), Kind::None);
    }

    #[test]
    fn text_is_cut() {
        let big = vec![b'a'; MAX_TEXT + 10];
        let Content::Text(t, cut) = build("big.txt", &big) else {
            panic!()
        };
        assert!(cut && t.len() == MAX_TEXT);
    }

    #[test]
    fn renders_a_minimal_pdf() {
        // One empty A4-ish page.
        let pdf = minimal_pdf();
        let Content::Pages(pages, total) = build("p.pdf", &pdf) else {
            panic!("not rendered")
        };
        assert_eq!(total, 1);
        assert_eq!(pages[0].1, 1100);
        assert!(thumbnail("p.pdf", &pdf).is_some());
    }

    fn minimal_pdf() -> Vec<u8> {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R >>",
            "<< /Length 35 >>\nstream\n0 0 1 rg 100 100 200 200 re f\nendstream",
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objs.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}
