//! CardDAV (RFC 6352): finds the account's address books and keeps a copy
//! of their contacts in the cache.
//!
//! Discovery follows RFC 6764: the address typed in the account settings,
//! else a known server for the mail domain, else
//! `https://<domain>/.well-known/carddav`; then current-user-principal ->
//! addressbook-home-set -> the address books in it.
//!
//! Sync compares ETags: one PROPFIND lists every card with its ETag, only
//! new or changed cards are downloaded (addressbook-multiget), cards gone
//! from the server are deleted. This works on every server, including those
//! without sync-collection (RFC 6578).
//!
//! The account's mail username and password are used (Basic auth), only
//! over HTTPS (except for an `http://` address the user typed, e.g. a server
//! on the local network), and only to hosts that belong to the mail
//! provider: the typed address, a known provider's server, or the domain of
//! the account's IMAP/SMTP servers. The website of a custom mail domain may
//! be hosted by someone else, so `.well-known` there is asked without them.

use crate::config::Account;
use crate::vcard;
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use std::time::Duration;
use ureq::http::{Request, StatusCode};

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 5;
/// Cards per addressbook-multiget request.
const BATCH: usize = 50;
/// Largest response read (a big address book with photos).
const MAX_BODY: u64 = 64 * 1024 * 1024;

const NS_DAV: &str = "DAV:";
const NS_CARD: &str = "urn:ietf:params:xml:ns:carddav";

/// Servers of providers whose domain does not answer `.well-known`.
const KNOWN: &[(&[&str], &str)] = &[
    (
        &["icloud.com", "me.com", "mac.com"],
        "https://contacts.icloud.com/",
    ),
    (
        &[
            "yahoo.com",
            "yahoo.it",
            "yahoo.fr",
            "yahoo.de",
            "yahoo.es",
            "yahoo.co.uk",
            "ymail.com",
        ],
        "https://carddav.address.yahoo.com/",
    ),
    (&["fastmail.com", "fastmail.fm"], "https://carddav.fastmail.com/"),
    (&["mailbox.org"], "https://dav.mailbox.org/"),
    (&["posteo.de", "posteo.net"], "https://posteo.de:8843/"),
    (&["gmx.net", "gmx.de", "gmx.com"], "https://carddav.gmx.net/"),
    (&["web.de"], "https://carddav.web.de/"),
];

/// One address book on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    /// Absolute URL of the collection.
    pub url: String,
    pub name: String,
}

/// A downloaded card.
#[derive(Debug, Clone)]
pub struct Card {
    /// Absolute URL of the card (its identity).
    pub url: String,
    pub etag: String,
    pub vcard: String,
    pub contact: vcard::Contact,
}

/// What a sync changed, for the caller to store.
#[derive(Debug, Default)]
pub struct Changes {
    pub books: Vec<Book>,
    /// New or changed cards.
    pub cards: Vec<(String, Card)>,
    /// URLs of cards no longer on the server.
    pub gone: Vec<String>,
}

/// The server refused the username or password.
#[derive(Debug)]
pub struct AuthRefused;

impl std::fmt::Display for AuthRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::i18n::t("err_dav_auth"))
    }
}

impl std::error::Error for AuthRefused {}

pub struct Client {
    agent: ureq::Agent,
    auth: String,
    /// Plain HTTP was typed by the user, so it is allowed.
    allow_http: bool,
    /// Domains the credentials may be sent to (see the module docs).
    trusted: Vec<String>,
}

impl Client {
    pub fn new(account: &Account) -> Client {
        let agent = ureq::Agent::config_builder()
            .allow_non_standard_methods(true)
            .http_status_as_error(false)
            // Followed by hand, to keep the method and body (see `send`).
            .max_redirects(0)
            .timeout_global(Some(TIMEOUT))
            .user_agent(concat!("Ferrowl/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let auth = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", account.username, account.password));
        Client {
            agent,
            auth: format!("Basic {auth}"),
            allow_http: account.carddav_url.trim().starts_with("http://"),
            trusted: trusted_domains(account),
        }
    }

    /// Whether the credentials may go to this URL's host.
    fn may_authenticate(&self, url: &str) -> bool {
        let host = host_of(url);
        self.trusted
            .iter()
            .any(|d| host == *d || host.ends_with(&format!(".{d}")))
    }

    /// Sends a WebDAV request, following redirects. Returns the final URL,
    /// status and body.
    fn send(
        &self,
        method: &str,
        url: &str,
        depth: Option<&str>,
        body: &str,
    ) -> Result<(String, StatusCode, String)> {
        let mut url = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            if !url.starts_with("https://") && !(self.allow_http && url.starts_with("http://")) {
                bail!("{}", crate::i18n::t("err_dav_insecure"));
            }
            let authenticated = self.may_authenticate(&url);
            let mut req = Request::builder()
                .method(method)
                .uri(&url)
                .header("Content-Type", "application/xml; charset=utf-8");
            if authenticated {
                req = req.header("Authorization", &self.auth);
            }
            if let Some(d) = depth {
                req = req.header("Depth", d);
            }
            let req = req.body(body.to_owned())?;
            let mut resp = self
                .agent
                .run(req)
                .map_err(|e| anyhow!(crate::i18n::tf("err_dav_connect", &[("msg", &e.to_string())])))?;
            let status = resp.status();
            if status.is_redirection() {
                let to = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| anyhow!("redirect without Location"))?;
                url = join(&url, to);
                continue;
            }
            // A refusal of our credentials. Elsewhere a 401 just means this
            // is not the contacts server (discovery goes on).
            if authenticated && (status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN) {
                return Err(AuthRefused.into());
            }
            let text = resp
                .body_mut()
                .with_config()
                .limit(MAX_BODY)
                .read_to_string()
                .unwrap_or_default();
            return Ok((url, status, text));
        }
        bail!("too many redirects")
    }

    /// PROPFIND returning the multistatus responses (empty if the URL is
    /// not a WebDAV resource).
    fn propfind(&self, url: &str, depth: &str, props: &str) -> Result<(String, Vec<Response>)> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:cs="http://calendarserver.org/ns/"><d:prop>{props}</d:prop></d:propfind>"#
        );
        let (url, status, text) = self.send("PROPFIND", url, Some(depth), &body)?;
        if status.as_u16() != 207 {
            return Ok((url, Vec::new()));
        }
        Ok((url.clone(), parse_multistatus(&text, &url)?))
    }

    /// Finds the account's address books.
    pub fn discover(&self, account: &Account) -> Result<Vec<Book>> {
        for start in start_urls(account) {
            match self.books_from(&start) {
                Ok(books) if !books.is_empty() => return Ok(books),
                // A wrong password ends the search: trying other servers
                // with it would not help.
                Err(e) if e.is::<AuthRefused>() => return Err(e),
                _ => {}
            }
        }
        bail!("{}", crate::i18n::t("err_dav_none"))
    }

    fn books_from(&self, start: &str) -> Result<Vec<Book>> {
        // The start URL may be a context path, the principal, the home
        // set or an address book itself: ask for everything at once.
        let (url, found) = self.propfind(
            start,
            "0",
            "<d:current-user-principal/><card:addressbook-home-set/><d:resourcetype/><d:displayname/>",
        )?;
        let Some(here) = found.first() else {
            return Ok(Vec::new());
        };
        if here.is_book {
            return Ok(vec![here.book(&url)]);
        }
        let mut home = here.home.clone();
        if home.is_none()
            && let Some(principal) = &here.principal
        {
            let (_, p) = self.propfind(principal, "0", "<card:addressbook-home-set/>")?;
            home = p.first().and_then(|r| r.home.clone());
        }
        let home = home.unwrap_or(url);
        let (_, list) = self.propfind(&home, "1", "<d:resourcetype/><d:displayname/>")?;
        Ok(list
            .iter()
            .filter(|r| r.is_book)
            .map(|r| r.book(&r.href))
            .collect())
    }

    /// Lists a book's cards as (url, etag).
    fn etags(&self, book: &str) -> Result<Vec<(String, String)>> {
        let (_, list) = self.propfind(book, "1", "<d:getetag/><d:resourcetype/>")?;
        Ok(list
            .into_iter()
            .filter(|r| !r.is_collection && !r.etag.is_empty() && !same_url(&r.href, book))
            .map(|r| (r.href, r.etag))
            .collect())
    }

    /// Downloads cards by URL (addressbook-multiget).
    fn multiget(&self, book: &str, urls: &[String]) -> Result<Vec<Card>> {
        let mut out = Vec::new();
        for chunk in urls.chunks(BATCH) {
            let hrefs: String = chunk
                .iter()
                .map(|u| format!("<d:href>{}</d:href>", xml_escape(&path_of(u))))
                .collect();
            let body = format!(
                r#"<?xml version="1.0" encoding="utf-8"?><card:addressbook-multiget xmlns:d="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav"><d:prop><d:getetag/><card:address-data/></d:prop>{hrefs}</card:addressbook-multiget>"#
            );
            let (url, status, text) = self.send("REPORT", book, Some("1"), &body)?;
            if status.as_u16() != 207 {
                bail!("addressbook-multiget: HTTP {status}");
            }
            for r in parse_multistatus(&text, &url)? {
                let Some(data) = r.data else { continue };
                if let Some(contact) = vcard::parse(&data) {
                    out.push(Card {
                        url: r.href,
                        etag: r.etag,
                        vcard: data,
                        contact,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Compares the server with `known` (card URL -> ETag, from the cache)
    /// and downloads what changed.
    pub fn sync(&self, account: &Account, known: &[(String, String)]) -> Result<Changes> {
        let books = self.discover(account)?;
        let mut changes = Changes::default();
        let mut seen = std::collections::HashSet::new();
        for book in &books {
            let list = self.etags(&book.url).with_context(|| book.name.clone())?;
            let wanted: Vec<String> = list
                .iter()
                .filter(|(url, etag)| !known.iter().any(|(u, e)| u == url && e == etag))
                .map(|(url, _)| url.clone())
                .collect();
            seen.extend(list.into_iter().map(|(u, _)| u));
            for card in self.multiget(&book.url, &wanted)? {
                changes.cards.push((book.url.clone(), card));
            }
        }
        changes.gone = known
            .iter()
            .filter(|(u, _)| !seen.contains(u))
            .map(|(u, _)| u.clone())
            .collect();
        changes.books = books;
        Ok(changes)
    }
}

/// Where discovery starts, in order.
fn start_urls(account: &Account) -> Vec<String> {
    let typed = account.carddav_url.trim();
    if !typed.is_empty() {
        let url = if typed.contains("://") {
            typed.to_owned()
        } else {
            format!("https://{typed}")
        };
        return vec![url];
    }
    let domain = account
        .email
        .trim()
        .rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase())
        .unwrap_or_default();
    let mut out = Vec::new();
    if let Some((_, url)) = KNOWN.iter().find(|(d, _)| d.contains(&domain.as_str())) {
        out.push(url.to_string());
    }
    if !domain.is_empty() {
        out.push(format!("https://{domain}/.well-known/carddav"));
    }
    out
}

/// Domains of the account's provider: the typed contacts server, a known
/// provider's server, and the IMAP and SMTP servers' domains.
fn trusted_domains(account: &Account) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    let typed = account.carddav_url.trim();
    if !typed.is_empty() {
        // The exact host typed by the user (not its parent domain).
        let url = if typed.contains("://") {
            typed.to_owned()
        } else {
            format!("https://{typed}")
        };
        hosts.push(host_of(&url));
    }
    let mut bases = vec![account.imap_host.clone(), account.smtp_host.clone()];
    bases.extend(
        start_urls(account)
            .iter()
            .filter(|u| !u.contains("/.well-known/"))
            .map(|u| host_of(u)),
    );
    hosts.extend(bases.iter().filter_map(|h| base_domain(h)));
    hosts.retain(|h| !h.is_empty());
    hosts.dedup();
    hosts
}

/// Lowercase host of a URL, without port or credentials.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or_default();
    let host = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .map(|h| format!("{h}]"))
            .unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default().to_owned()
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// The registrable domain of a host: "imap.mail.example.co.uk" ->
/// "example.co.uk". Second-level labels like "co" or "com" under a country
/// code are kept, so a whole public suffix is never trusted. IP addresses
/// and single labels are returned as they are.
fn base_domain(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if host.parse::<std::net::IpAddr>().is_ok() || host.starts_with('[') {
        return Some(host);
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 {
        return Some(host);
    }
    let n = labels.len();
    let second = labels[n - 2];
    let generic = ["co", "com", "net", "org", "gov", "ac", "edu", "or", "ne", "go"];
    let keep = if labels[n - 1].len() == 2 && generic.contains(&second) {
        3
    } else {
        2
    };
    Some(labels[n.saturating_sub(keep)..].join("."))
}

/// One `<d:response>` of a multistatus, with the properties we read.
#[derive(Debug, Default)]
struct Response {
    href: String,
    etag: String,
    name: String,
    is_collection: bool,
    is_book: bool,
    principal: Option<String>,
    home: Option<String>,
    data: Option<String>,
}

impl Response {
    fn book(&self, url: &str) -> Book {
        let name = if self.name.trim().is_empty() {
            last_segment(url)
        } else {
            self.name.trim().to_owned()
        };
        Book {
            url: url.to_owned(),
            name,
        }
    }
}

/// Reads a 207 Multi-Status body. Hrefs are made absolute against `base`.
/// Properties in a non-200 propstat (not found) are ignored.
fn parse_multistatus(xml: &str, base: &str) -> Result<Vec<Response>> {
    let doc = roxmltree::Document::parse(xml).context("reading the server's reply")?;
    let href_in = |n: roxmltree::Node| {
        child(n, NS_DAV, "href")
            .and_then(|h| h.text())
            .map(|t| join(base, t.trim()))
    };
    let mut out = Vec::new();
    for resp in doc.descendants().filter(|n| is(n, NS_DAV, "response")) {
        let Some(href) = href_in(resp) else { continue };
        let mut r = Response {
            href,
            ..Default::default()
        };
        for ps in resp.children().filter(|n| is(n, NS_DAV, "propstat")) {
            let ok = child(ps, NS_DAV, "status")
                .and_then(|s| s.text())
                .is_none_or(|s| s.contains(" 200"));
            let Some(prop) = child(ps, NS_DAV, "prop") else {
                continue;
            };
            if !ok {
                continue;
            }
            for p in prop.children().filter(roxmltree::Node::is_element) {
                let text = || p.text().unwrap_or_default().to_owned();
                match (p.tag_name().namespace(), p.tag_name().name()) {
                    (Some(NS_DAV), "getetag") => r.etag = text().trim().to_owned(),
                    (Some(NS_DAV), "displayname") => r.name = text(),
                    (Some(NS_DAV), "resourcetype") => {
                        r.is_collection = child(p, NS_DAV, "collection").is_some();
                        r.is_book = child(p, NS_CARD, "addressbook").is_some();
                    }
                    (Some(NS_DAV), "current-user-principal") => r.principal = href_in(p),
                    (Some(NS_CARD), "addressbook-home-set") => r.home = href_in(p),
                    (Some(NS_CARD), "address-data") => {
                        r.data = Some(p.children().filter_map(|c| c.text()).collect())
                    }
                    _ => {}
                }
            }
        }
        out.push(r);
    }
    Ok(out)
}

fn is(n: &roxmltree::Node, ns: &str, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name && n.tag_name().namespace() == Some(ns)
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, ns: &str, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| is(c, ns, name))
}

/// Resolves `href` (absolute URL, absolute path or relative path) against
/// `base`.
fn join(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_owned();
    }
    let origin_end = base
        .find("://")
        .map(|i| i + 3)
        .and_then(|start| base[start..].find('/').map(|j| start + j))
        .unwrap_or(base.len());
    if href.starts_with('/') {
        return format!("{}{href}", &base[..origin_end]);
    }
    let dir_end = base
        .rfind('/')
        .filter(|&i| i >= origin_end)
        .map_or(base.len(), |i| i + 1);
    let mut dir = base[..dir_end].to_owned();
    if !dir.ends_with('/') {
        dir.push('/');
    }
    format!("{dir}{href}")
}

/// The path part of an absolute URL (what servers expect in `<d:href>`).
fn path_of(url: &str) -> String {
    url.find("://")
        .and_then(|i| url[i + 3..].find('/').map(|j| url[i + 3 + j..].to_owned()))
        .unwrap_or_else(|| url.to_owned())
}

fn same_url(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

fn last_segment(url: &str) -> String {
    let s = url.trim_end_matches('/');
    let s = s.rsplit('/').next().unwrap_or(s);
    percent_decode(s)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let (b'%', Some(b)) = (bytes[i], hex) {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[test]
    fn joins_hrefs() {
        let base = "https://dav.example.com/dav/books/";
        assert_eq!(join(base, "/p/x.vcf"), "https://dav.example.com/p/x.vcf");
        assert_eq!(join(base, "x.vcf"), "https://dav.example.com/dav/books/x.vcf");
        assert_eq!(join("https://h.it", "/a"), "https://h.it/a");
        assert_eq!(join(base, "https://o.it/a"), "https://o.it/a");
        assert_eq!(path_of("https://h.it:8443/a/b.vcf"), "/a/b.vcf");
        assert_eq!(last_segment("https://h.it/a/Lavoro%20e%20casa/"), "Lavoro e casa");
    }

    #[test]
    fn credentials_only_go_to_the_provider() {
        let account = Account {
            email: "me@shop.example".into(),
            imap_host: "imap.mailhost.it".into(),
            smtp_host: "smtp.mailhost.it".into(),
            ..Default::default()
        };
        let c = Client::new(&account);
        assert!(c.may_authenticate("https://dav.mailhost.it/x"));
        assert!(c.may_authenticate("https://mailhost.it/.well-known/carddav"));
        // The custom domain's website may be hosted by a third party.
        assert!(!c.may_authenticate("https://shop.example/.well-known/carddav"));
        assert!(!c.may_authenticate("https://evil-mailhost.it/"));

        let icloud = Account {
            email: "me@icloud.com".into(),
            imap_host: "imap.mail.me.com".into(),
            ..Default::default()
        };
        let c = Client::new(&icloud);
        assert!(c.may_authenticate("https://p42-contacts.icloud.com/123/carddavhome/"));

        assert_eq!(
            base_domain("imap.mail.example.co.uk").as_deref(),
            Some("example.co.uk")
        );
        assert_eq!(base_domain("mail.example.it").as_deref(), Some("example.it"));
        assert_eq!(host_of("https://u@Dav.Example.org:8443/p"), "dav.example.org");
    }

    #[test]
    fn start_urls_by_domain() {
        let a = |email: &str, url: &str| Account {
            email: email.into(),
            carddav_url: url.into(),
            ..Default::default()
        };
        assert_eq!(
            start_urls(&a("x@icloud.com", "")),
            [
                "https://contacts.icloud.com/",
                "https://icloud.com/.well-known/carddav"
            ]
        );
        assert_eq!(
            start_urls(&a("x@example.org", "")),
            ["https://example.org/.well-known/carddav"]
        );
        assert_eq!(
            start_urls(&a("x@example.org", "dav.example.org/c")),
            ["https://dav.example.org/c"]
        );
    }

    #[test]
    fn insecure_urls_are_refused() {
        crate::i18n::set(Some("en"));
        let mut account = Account {
            email: "x@example.org".into(),
            ..Default::default()
        };
        let c = Client::new(&account);
        assert!(c.send("PROPFIND", "http://example.org/", None, "").is_err());
        account.carddav_url = "http://127.0.0.1:1/".into();
        assert!(Client::new(&account).allow_http);
    }

    /// (file name, ETag, vCard) of each card on the fake server.
    type Cards = Arc<Mutex<Vec<(&'static str, &'static str, String)>>>;

    /// A minimal CardDAV server: the well-known path redirects to a
    /// principal, which points to a home with one address book of two
    /// cards. Records every request line.
    fn fake_server(cards: Cards) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut len = 0usize;
                let mut auth = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = line.trim().to_owned();
                    }
                }
                let mut body = vec![0; len];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                let mut it = first.split_whitespace();
                let (method, path) = (it.next().unwrap().to_owned(), it.next().unwrap().to_owned());
                log2.lock().unwrap().push(format!("{method} {path}"));
                let ms = |inner: String| {
                    (
                        207,
                        format!(
                            r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav">{inner}</d:multistatus>"#
                        ),
                        String::new(),
                    )
                };
                // "user:secret" in base64.
                let (status, text, location) = if !auth.ends_with("dXNlcjpzZWNyZXQ=") {
                    (401, String::new(), String::new())
                } else {
                    match (method.as_str(), path.as_str()) {
                        ("PROPFIND", "/.well-known/carddav") => (301, String::new(), "/dav/".to_owned()),
                        ("PROPFIND", "/dav/") => ms(r#"<d:response><d:href>/dav/</d:href><d:propstat><d:prop><d:current-user-principal><d:href>/dav/principals/user/</d:href></d:current-user-principal><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat><d:propstat><d:prop><c:addressbook-home-set/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat></d:response>"#.into()),
                        ("PROPFIND", "/dav/principals/user/") => ms(r#"<d:response><d:href>/dav/principals/user/</d:href><d:propstat><d:prop><c:addressbook-home-set><d:href>/dav/books/user/</d:href></c:addressbook-home-set></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"#.into()),
                        ("PROPFIND", "/dav/books/user/") => ms(r#"<d:response><d:href>/dav/books/user/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response><d:response><d:href>/dav/books/user/main/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><c:addressbook/></d:resourcetype><d:displayname>Personal</d:displayname></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"#.into()),
                        ("PROPFIND", "/dav/books/user/main/") => {
                            let mut inner = String::from(r#"<d:response><d:href>/dav/books/user/main/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><c:addressbook/></d:resourcetype><d:getetag/></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"#);
                            for (name, etag, _) in cards.lock().unwrap().iter() {
                                inner += &format!(r#"<d:response><d:href>/dav/books/user/main/{name}</d:href><d:propstat><d:prop><d:getetag>{etag}</d:getetag><d:resourcetype/></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"#);
                            }
                            ms(inner)
                        }
                        ("REPORT", "/dav/books/user/main/") => {
                            let mut inner = String::new();
                            for (name, etag, data) in cards.lock().unwrap().iter() {
                                if body.contains(&format!("/main/{name}<")) {
                                    inner += &format!(r#"<d:response><d:href>/dav/books/user/main/{name}</d:href><d:propstat><d:prop><d:getetag>{etag}</d:getetag><c:address-data>{data}</c:address-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"#);
                                }
                            }
                            ms(inner)
                        }
                        _ => (404, String::new(), String::new()),
                    }
                };
                let reason = match status {
                    207 => "Multi-Status",
                    301 => "Moved",
                    401 => "Unauthorized",
                    _ => "Not Found",
                };
                let loc = if location.is_empty() {
                    String::new()
                } else {
                    format!("Location: {location}\r\n")
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\n{loc}Content-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
            }
        });
        (base, log)
    }

    #[test]
    fn discovers_and_syncs_with_etags() {
        crate::i18n::set(Some("en"));
        let card = |fname: &str, email: &str| {
            format!("BEGIN:VCARD&#13;\nVERSION:4.0\nFN:{fname}\nEMAIL:{email}\nEND:VCARD\n")
        };
        let cards = Arc::new(Mutex::new(vec![
            ("a.vcf", "\"1\"", card("Anna Bianchi", "anna@example.org")),
            ("b.vcf", "\"1\"", card("Bruno Verdi", "bruno@example.org")),
        ]));
        let (base, log) = fake_server(cards.clone());
        let mut account = Account {
            email: "user@example.org".into(),
            username: "user".into(),
            password: "secret".into(),
            carddav_url: format!("{base}/.well-known/carddav"),
            ..Default::default()
        };
        let client = Client::new(&account);

        // First sync: discovery through the redirect, both cards fetched.
        let first = client.sync(&account, &[]).unwrap();
        assert_eq!(
            first.books,
            [Book {
                url: format!("{base}/dav/books/user/main/"),
                name: "Personal".into()
            }]
        );
        let mut names: Vec<_> = first.cards.iter().map(|(_, c)| c.contact.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["Anna Bianchi", "Bruno Verdi"]);
        assert!(first.gone.is_empty());

        // Second sync: one card changed, one deleted, one unknown in cache.
        let known: Vec<(String, String)> = first
            .cards
            .iter()
            .map(|(_, c)| (c.url.clone(), c.etag.clone()))
            .collect();
        {
            let mut c = cards.lock().unwrap();
            c[0].1 = "\"2\"";
            c[0].2 = card("Anna Bianchi Rossi", "anna@example.org");
            c.remove(1);
        }
        log.lock().unwrap().clear();
        let second = client.sync(&account, &known).unwrap();
        assert_eq!(second.cards.len(), 1);
        assert_eq!(second.cards[0].1.contact.name, "Anna Bianchi Rossi");
        assert_eq!(second.gone, [format!("{base}/dav/books/user/main/b.vcf")]);
        let reports = log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.starts_with("REPORT"))
            .count();
        assert_eq!(reports, 1, "only changed cards are downloaded");

        // Wrong password: a clear error, not "no address book".
        account.password = "wrong".into();
        let err = Client::new(&account).sync(&account, &[]).unwrap_err();
        assert!(err.is::<AuthRefused>(), "{err:#}");
    }
}
