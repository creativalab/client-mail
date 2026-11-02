//! HTML mail rendering with Blitz (Stylo CSS + Taffy layout + Vello CPU).
//!
//! A dedicated worker thread owns the parsed document (Blitz documents are
//! not `Send`): it renders the message into image tiles for the reader and
//! answers hit tests ("which link is under this point?") for clicks/hover.
//!
//! - Scripts never run (Blitz has no JS engine); forms do nothing.
//! - Remote resources (images, stylesheets, fonts) are fetched over
//!   http(s) only when allowed; `cid:` images come from the message parts.
//! - Long messages are split into tiles to stay below GPU texture limits.

use anyrender::render_to_buffer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use iced::widget::image;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Tile height in physical pixels (well below the usual 8192 texture limit).
const TILE: u32 = 2048;
/// Longest message rendered, in logical pixels.
const MAX_HEIGHT: u32 = 40_000;
const MAX_FETCHES: usize = 80;
const MAX_RESOURCE_BYTES: u64 = 8 << 20;
const FETCH_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct Rendered {
    pub uid: u32,
    /// Logical width the message was laid out for.
    pub width: u32,
    /// Logical height of each tile, in order.
    pub tiles: Vec<(image::Handle, f32)>,
    /// Remote content was present but not loaded.
    pub blocked_remote: bool,
}

pub struct Job {
    pub uid: u32,
    pub html: String,
    pub width: u32,
    pub scale: f32,
    pub allow_remote: bool,
    /// Inline parts by Content-ID (without angle brackets).
    pub inline: HashMap<String, Vec<u8>>,
}

enum Command {
    Render(Job, mpsc::Sender<Rendered>),
    /// Link at logical (x, y) of message `uid`.
    Hit(u32, f32, f32, mpsc::Sender<Option<String>>),
}

fn worker() -> &'static Mutex<mpsc::Sender<Command>> {
    static WORKER: OnceLock<Mutex<mpsc::Sender<Command>>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Command>();
        std::thread::Builder::new()
            .name("html-render".into())
            .spawn(move || run(rx))
            .expect("spawning the HTML renderer");
        Mutex::new(tx)
    })
}

fn send(cmd: Command) {
    let _ = worker().lock().unwrap_or_else(|p| p.into_inner()).send(cmd);
}

/// Renders a message (blocking: call from a background task).
pub fn render(job: Job) -> Option<Rendered> {
    let (tx, rx) = mpsc::channel();
    send(Command::Render(job, tx));
    rx.recv().ok()
}

/// The link under a point of the last rendered message (blocking).
pub fn link_at(uid: u32, x: f32, y: f32) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    send(Command::Hit(uid, x, y, tx));
    rx.recv().ok().flatten()
}

fn run(rx: mpsc::Receiver<Command>) {
    let mut current: Option<(u32, HtmlDocument)> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::Render(job, reply) => {
                // Only the most recent request matters: skip stale ones.
                let mut job = job;
                let mut reply = reply;
                while let Ok(next) = rx.try_recv() {
                    match next {
                        Command::Render(j, r) => {
                            job = j;
                            reply = r;
                        }
                        Command::Hit(_, _, _, r) => {
                            let _ = r.send(None);
                        }
                    }
                }
                let uid = job.uid;
                let (doc, rendered) = render_job(job);
                current = Some((uid, doc));
                let _ = reply.send(rendered);
            }
            Command::Hit(uid, x, y, reply) => {
                let link = match &current {
                    Some((u, doc)) if *u == uid => link_in(doc, x, y),
                    _ => None,
                };
                let _ = reply.send(link);
            }
        }
    }
}

fn link_in(doc: &HtmlDocument, x: f32, y: f32) -> Option<String> {
    let hit = doc.hit(x, y)?;
    let mut node = doc.get_node(hit.node_id);
    while let Some(n) = node {
        if let Some(el) = n.element_data()
            && el.name.local == blitz_dom::local_name!("a")
            && let Some(href) = el.attr(blitz_dom::local_name!("href"))
        {
            return Some(href.trim().to_string());
        }
        node = n.parent.and_then(|p| doc.get_node(p));
    }
    None
}

/// Fetches resources on background threads; counts what is in flight.
struct Net {
    allow_remote: bool,
    inline: HashMap<String, Bytes>,
    pending: Arc<AtomicUsize>,
    started: AtomicUsize,
    blocked: Arc<AtomicUsize>,
}

impl NetProvider for Net {
    fn fetch(&self, _doc: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.to_string();
        if let Some(cid) = url.strip_prefix("cid:") {
            let cid = percent_decode(cid);
            if let Some(data) = self.inline.get(cid.trim_matches(['<', '>'])) {
                handler.bytes(url, data.clone());
            }
            return;
        }
        let remote = url.starts_with("https://") || url.starts_with("http://");
        if !remote {
            return;
        }
        if !self.allow_remote {
            self.blocked.fetch_add(1, Ordering::SeqCst);
            return;
        }
        if self.started.fetch_add(1, Ordering::SeqCst) >= MAX_FETCHES {
            return;
        }
        let pending = self.pending.clone();
        pending.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            if let Some(data) = fetch(&url) {
                handler.bytes(url, Bytes::from(data));
            }
            pending.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_DEADLINE))
        .build()
        .into();
    let mut response = agent.get(url).call().ok()?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_RESOURCE_BYTES)
        .read_to_vec()
        .ok()
}

fn render_job(job: Job) -> (HtmlDocument, Rendered) {
    let scale = job.scale.clamp(1.0, 3.0);
    let width = job.width.max(200);
    let pending = Arc::new(AtomicUsize::new(0));
    let blocked = Arc::new(AtomicUsize::new(0));
    let net = Net {
        allow_remote: job.allow_remote,
        inline: job.inline.into_iter().map(|(k, v)| (k, Bytes::from(v))).collect(),
        pending: pending.clone(),
        started: AtomicUsize::new(0),
        blocked: blocked.clone(),
    };
    let physical_w = (width as f32 * scale).round() as u32;
    let config = DocumentConfig {
        viewport: Some(Viewport::new(physical_w, (800.0 * scale) as u32, scale, ColorScheme::Light)),
        net_provider: Some(Arc::new(net)),
        ..Default::default()
    };
    let mut doc = HtmlDocument::from_html(&job.html, config);
    doc.resolve(0.0);
    // Wait for images/stylesheets (bounded), then lay out again.
    let deadline = Instant::now() + FETCH_DEADLINE;
    while pending.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(15));
    }
    doc.resolve(0.0);

    let height = (doc.root_element().final_layout().size.height.ceil() as u32).clamp(1, MAX_HEIGHT);
    let physical_h = (height as f32 * scale).ceil() as u32;
    // Paint tile by tile: a viewport as tall as the tile, scrolled to it.
    let mut tiles = Vec::new();
    let mut y = 0;
    while y < physical_h {
        let h = TILE.min(physical_h - y);
        doc.set_viewport(Viewport::new(physical_w, h, scale, ColorScheme::Light));
        doc.resolve(0.0);
        doc.set_viewport_scroll(blitz_dom::Point { x: 0.0, y: y as f64 / scale as f64 });
        let pixels = render_to_buffer::<VelloCpuImageRenderer, _>(
            |scene| blitz_paint::paint_scene(scene, &mut doc, scale as f64, physical_w, h, 0, 0),
            physical_w,
            h,
        );
        tiles.push((image::Handle::from_rgba(physical_w, h, pixels), h as f32 / scale));
        y += h;
    }
    // Full-height, unscrolled viewport so hit testing works anywhere.
    doc.set_viewport(Viewport::new(physical_w, physical_h, scale, ColorScheme::Light));
    doc.resolve(0.0);
    doc.set_viewport_scroll(blitz_dom::Point { x: 0.0, y: 0.0 });

    let rendered = Rendered {
        uid: job.uid,
        width,
        tiles,
        blocked_remote: blocked.load(Ordering::SeqCst) > 0,
    };
    (doc, rendered)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The renderer keeps only the latest request: tests take turns.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn renders_tiles_and_finds_links() {
        let _turn = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let html = r#"<html><body style="margin:0">
            <div style="height:3000px;background:#eee">
              <a href="https://ferrowl.com/x" style="display:block;height:100px">link</a>
            </div>
            <img src="cid:logo@x" width="10" height="10">
            <img src="https://tracker.invalid/p.gif">
        </body></html>"#;
        let mut inline = HashMap::new();
        inline.insert("logo@x".to_string(), vec![0u8; 4]);
        let r = render(Job { uid: 7, html: html.into(), width: 600, scale: 1.0, allow_remote: false, inline })
            .unwrap();
        assert_eq!(r.width, 600);
        assert!(r.tiles.len() >= 2, "3000px+ must be split into tiles");
        let total: f32 = r.tiles.iter().map(|(_, h)| h).sum();
        assert!(total >= 3000.0);
        assert!(r.blocked_remote, "remote image must be blocked");
        assert_eq!(link_at(7, 50.0, 50.0).as_deref(), Some("https://ferrowl.com/x"));
        assert_eq!(link_at(7, 50.0, 2000.0), None);
        assert_eq!(link_at(8, 50.0, 50.0), None, "other message");
    }

    /// Renders `$FERROWL_HTML` and writes its tiles next to it (manual check).
    #[test]
    #[ignore = "manual: needs FERROWL_HTML"]
    fn dump_tiles() {
        let _turn = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let path = std::path::PathBuf::from(std::env::var("FERROWL_HTML").unwrap());
        let html = std::fs::read_to_string(&path).unwrap();
        let r = render(Job { uid: 1, html, width: 700, scale: 1.0, allow_remote: true, inline: HashMap::new() }).unwrap();
        for (i, (handle, h)) in r.tiles.iter().enumerate() {
            let iced::advanced::image::Handle::Rgba { width, height, pixels, .. } = handle else { panic!() };
            let f = std::fs::File::create(path.with_extension(format!("tile{i}.png"))).unwrap();
            let mut e = png::Encoder::new(f, *width, *height);
            e.set_color(png::ColorType::Rgba);
            e.write_header().unwrap().write_image_data(pixels).unwrap();
            println!("tile {i}: {width}x{height} logical h {h}");
        }
    }
}
