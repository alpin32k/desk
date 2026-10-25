//! Preview of links printed in a terminal (mockups, dev servers, local files,
//! Markdown, diagrams) in a side panel of the main window: a second webview
//! next to the terminals. That webview loads untrusted content:
//! - it can call none of desk's commands (build.rs app manifest; the
//!   capability in capabilities/default.json is granted only to the "main"
//!   webview, not to the window both webviews share),
//! - it may only navigate to http(s), and to `deskpreview://` only when desk
//!   opened that URL or the page already is a `deskpreview://` page,
//! - local files are never loaded as `file://`. `deskpreview://` serves web
//!   asset types and Markdown (no dotfiles) from the previewed file's folder
//!   under a random token, never the home folder or above, with a CSP that
//!   blocks fetch/XHR and form posts to other hosts.
//! - documents desk renders itself (Markdown, Mermaid, JSON, CSV, text and
//!   code) run no scripts of their own: only desk's bundled Mermaid.
//!
//! A page can still send what it may read (its own folder) out through a
//! link or an image URL: preview only files you would open in a browser.

use serde::Deserialize;
use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::http::{Request, Response, header};
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use url::Url;

pub const LABEL: &str = "preview";
pub const SCHEME: &str = "deskpreview";

/// Folders opened for preview, by token (the first path segment).
static ROOTS: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

/// Single files desk renders itself (`/doc/<token>/<name>`), by token.
static DOCS: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

/// Largest file rendered as a document.
const MAX_DOC: u64 = 5 * 1024 * 1024;

/// Mermaid, bundled so diagrams render offline and no CDN script runs.
const MERMAID_JS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../node_modules/mermaid/dist/mermaid.min.js"
));

/// The `deskpreview://` URL desk itself is opening, and whether the window is
/// showing a `deskpreview://` page now (pages may link to each other).
static NAV: Mutex<(Option<Url>, bool)> = Mutex::new((None, false));

/// Content Security Policy for local pages: CDN scripts, styles and images
/// load, but nothing can be posted or fetched from another host.
const CSP: &str = "default-src 'self' https: data: blob: 'unsafe-inline' 'unsafe-eval'; \
                   connect-src 'self'; form-action 'none'; frame-src 'self'";

#[derive(Debug, PartialEq)]
pub enum Target {
    Web(Url),
    File(PathBuf),
}

/// Only web pages and existing local files; nothing that runs a program.
pub fn parse(link: &str) -> Result<Target, String> {
    let expanded;
    let link = match link.strip_prefix("~/") {
        Some(rest) => {
            expanded = crate::env::home().join(rest).display().to_string();
            expanded.as_str()
        }
        None => link,
    };
    let url = if link.starts_with('/') || (cfg!(windows) && Path::new(link).is_absolute()) {
        Url::from_file_path(link).map_err(|_| format!("zła ścieżka: {link}"))?
    } else {
        Url::parse(link).map_err(|e| format!("{link}: {e}"))?
    };
    match url.scheme() {
        "http" | "https" => Ok(Target::Web(url)),
        "file" => {
            let path = url.to_file_path().map_err(|_| "zła ścieżka".to_string())?;
            if !local_path(&path) {
                return Err(format!("tylko pliki na tym komputerze: {}", path.display()));
            }
            let path = path
                .canonicalize()
                .map_err(|_| format!("nie ma pliku {}", path.display()))?;
            if path.is_file() {
                Ok(Target::File(path))
            } else {
                Err(format!("to nie plik: {}", path.display()))
            }
        }
        other => Err(format!("nieobsługiwany adres ({other}:)")),
    }
}

/// A path on this machine. On Windows only drive paths (`C:\...`): touching a
/// UNC path (`\\host\share`) would make Windows connect to that host and
/// send it the user's NTLM credentials.
pub fn local_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    if !cfg!(windows) {
        return !path.to_string_lossy().starts_with("//");
    }
    matches!(
        path.components().next(),
        Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
    )
}

/// `deskpreview://localhost/<token>/<file>`, registering the file's folder.
fn file_url(path: &Path) -> Result<Url, String> {
    let dir = path.parent().ok_or("brak folderu")?.to_path_buf();
    let home = crate::env::home();
    // Canonical on both sides (Windows canonical paths start with \\?\).
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if canon(&home).starts_with(canon(&dir)) {
        return Err(format!(
            "podgląd pliku z {} odsłoniłby cały folder; przenieś go do podfolderu",
            dir.display()
        ));
    }
    let name = path
        .file_name()
        .ok_or("brak nazwy pliku")?
        .to_string_lossy();
    let token = token_for(&ROOTS, dir);
    let mut url = Url::parse(&format!("{SCHEME}://localhost/")).map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "zły adres")?
        .push(&token)
        .push(&name);
    Ok(url)
}

/// `deskpreview://localhost/doc/<token>/<file>`: a file desk renders itself.
fn doc_url(path: &Path) -> Result<Url, String> {
    let name = path
        .file_name()
        .ok_or("brak nazwy pliku")?
        .to_string_lossy();
    let token = token_for(&DOCS, path.to_path_buf());
    let mut url = Url::parse(&format!("{SCHEME}://localhost/")).map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "zły adres")?
        .push("doc")
        .push(&token)
        .push(&name);
    Ok(url)
}

/// The token of `path` in `map`, registering it. Random, so a previewed page
/// cannot guess the tokens of other folders or files.
fn token_for(map: &Mutex<Option<HashMap<String, PathBuf>>>, path: PathBuf) -> String {
    let mut map = map.lock().unwrap();
    let map = map.get_or_insert_with(HashMap::new);
    if let Some((t, _)) = map.iter().find(|(_, p)| **p == path) {
        return t.clone();
    }
    let t = format!("{:016x}", RandomState::new().hash_one(&path));
    map.insert(t.clone(), path);
    t
}

/// The URL to show `path` at: a web page or asset as it is, anything else
/// readable as text rendered by desk.
fn local_url(path: &Path) -> Result<Url, String> {
    if Doc::of(path).is_some() {
        return doc_url(path);
    }
    if mime(path).is_some() {
        return file_url(path);
    }
    Err(format!("nie umiem pokazać tego pliku: {}", path.display()))
}

/// Protocol handler: a file inside a registered folder, nothing outside it.
pub fn serve(req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = req.uri().path();
    if path == "/_desk/mermaid.js" {
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
            .header("X-Content-Type-Options", "nosniff")
            .body(MERMAID_JS.to_vec())
            .unwrap_or_else(|_| not_found());
    }
    if let Some(rest) = path.strip_prefix("/doc/") {
        let token = rest.split('/').next().unwrap_or_default();
        let file = DOCS
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|d| d.get(token).cloned());
        return match file.and_then(|f| Some((Doc::of(&f)?, f))) {
            Some((kind, f)) => document(&f, kind),
            None => not_found(),
        };
    }
    // Markdown inside a previewed folder (links between docs) renders too.
    if let Some(full) = resolve_path(path)
        && let Some(kind @ (Doc::Markdown | Doc::Mermaid)) = Doc::of(&full)
    {
        return document(&full, kind);
    }
    match resolve(path) {
        Some((path, mime)) => match std::fs::read(&path) {
            Ok(body) => Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::CONTENT_SECURITY_POLICY, CSP)
                .header("X-Content-Type-Options", "nosniff")
                .body(body)
                .unwrap_or_else(|_| not_found()),
            Err(_) => not_found(),
        },
        None => not_found(),
    }
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(404)
        .body(b"not found".to_vec())
        .expect("static response")
}

/// `/<token>/<relative path>` -> a canonical path that stays inside the folder,
/// with the MIME type of a web asset (anything else, e.g. `.env`, is refused).
pub fn resolve(uri_path: &str) -> Option<(PathBuf, &'static str)> {
    let path = resolve_path(uri_path)?;
    Some((path.clone(), mime(&path)?))
}

/// `/<token>/<relative path>` -> a file inside that folder (any type).
fn resolve_path(uri_path: &str) -> Option<PathBuf> {
    let mut parts = uri_path.trim_start_matches('/').splitn(2, '/');
    let token = parts.next()?;
    let rel = percent_encoding::percent_decode_str(parts.next()?)
        .decode_utf8()
        .ok()?;
    let root = ROOTS.lock().unwrap().as_ref()?.get(token)?.clone();
    resolve_in(&root, &rel)
}

pub fn resolve_in(root: &Path, rel: &str) -> Option<PathBuf> {
    let full = root.join(rel).canonicalize().ok()?;
    let inside = full.strip_prefix(root).ok()?;
    let hidden = inside
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
    (!hidden && full.is_file()).then_some(full)
}

fn mime(path: &Path) -> Option<&'static str> {
    Some(
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("html" | "htm") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js" | "mjs") => "text/javascript; charset=utf-8",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("pdf") => "application/pdf",
            Some("woff2") => "font/woff2",
            Some("woff") => "font/woff",
            Some("ico") => "image/x-icon",
            Some("mp4") => "video/mp4",
            Some("webm") => "video/webm",
            _ => return None,
        },
    )
}

/// Where the panel sits in the main window (CSS pixels).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Show `link` in the side panel, creating it at `bounds`. Returns what the
/// panel shows, for its address bar.
pub fn open(app: &AppHandle, link: &str, bounds: Bounds) -> Result<String, String> {
    let url = match parse(link)? {
        Target::Web(u) => u,
        Target::File(p) => local_url(&p)?,
    };
    if url.scheme() == SCHEME {
        NAV.lock().unwrap().0 = Some(url.clone());
    }
    if let Some(w) = app.get_webview(LABEL) {
        place(&w, bounds);
        w.navigate(url.clone()).map_err(|e| e.to_string())?;
        let _ = w.show();
        return Ok(display(&url));
    }
    let window = app.get_window("main").ok_or("brak głównego okna")?;
    let local = url.scheme() == SCHEME;
    let handle = app.clone();
    let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(url.clone()))
        .on_navigation(allow_navigation)
        .on_page_load(move |_, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = handle.emit_to("main", "preview-url", display(payload.url()));
            }
        });
    window
        .add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)),
        )
        .map_err(|e| e.to_string())?;
    // The first page may load without a navigation check.
    *NAV.lock().unwrap() = (None, local);
    Ok(display(&url))
}

fn place<R: tauri::Runtime>(w: &tauri::Webview<R>, b: Bounds) {
    let _ = w.set_position(LogicalPosition::new(b.x, b.y));
    let _ = w.set_size(LogicalSize::new(b.width.max(1.0), b.height.max(1.0)));
}

/// The panel follows its place in the window (resize, splitter).
pub fn set_bounds(app: &AppHandle, bounds: Bounds) {
    if let Some(w) = app.get_webview(LABEL) {
        place(&w, bounds);
    }
}

/// Hidden while desk shows a dialog over it (a native view would cover it).
pub fn set_visible(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview(LABEL) {
        let _ = if visible { w.show() } else { w.hide() };
    }
}

pub fn close(app: &AppHandle) {
    if let Some(w) = app.get_webview(LABEL) {
        let _ = w.close();
    }
}

pub fn navigate(app: &AppHandle, action: &str) -> Result<(), String> {
    let w = app.get_webview(LABEL).ok_or("podgląd jest zamknięty")?;
    let js = match action {
        "back" => "history.back()",
        "forward" => "history.forward()",
        "reload" => "location.reload()",
        _ => return Err(format!("nieznana akcja {action}")),
    };
    w.eval(js).map_err(|e| e.to_string())
}

/// What the address bar shows: the file for desk's own URLs, else the URL.
pub fn display(url: &Url) -> String {
    if url.scheme() != SCHEME {
        return url.to_string();
    }
    let path = url.path();
    let file = if let Some(rest) = path.strip_prefix("/doc/") {
        let token = rest.split('/').next().unwrap_or_default();
        DOCS.lock()
            .unwrap()
            .as_ref()
            .and_then(|d| d.get(token).cloned())
    } else {
        resolve_path(path)
    };
    file.map_or_else(|| url.to_string(), |f| f.display().to_string())
}

/// Text documents whose default app shows or edits them (never runs them).
fn document_type(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "md" | "markdown"
                | "mdx"
                | "mmd"
                | "txt"
                | "log"
                | "json"
                | "csv"
                | "tsv"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "rtf"
        )
    )
}

/// A file shown in the panel that may be edited there: one desk opened, or
/// Markdown inside a previewed folder.
fn editable(path: &Path) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|_| format!("nie ma pliku {}", path.display()))?;
    let opened = DOCS
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|d| d.values().any(|p| *p == path));
    let in_folder = matches!(Doc::of(&path), Some(Doc::Markdown | Doc::Mermaid))
        && ROOTS
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|r| r.values().any(|d| path.starts_with(d)));
    if (opened || in_folder) && Doc::of(&path).is_some() {
        Ok(path)
    } else {
        Err(format!("tego pliku nie edytuję w desk: {}", path.display()))
    }
}

/// Text of a file shown in the panel, for the editor.
pub fn read_text(path: &Path) -> Result<String, String> {
    let path = editable(path)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Save the editor's text to a file shown in the panel.
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    let path = editable(path)?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Files desk renders as a page itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Doc {
    Markdown,
    Mermaid,
    Json,
    Table(char),
    Text,
}

impl Doc {
    pub fn of(path: &Path) -> Option<Doc> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let doc = match ext.as_deref() {
            Some("md" | "markdown" | "mdx") => Doc::Markdown,
            Some("mmd" | "mermaid") => Doc::Mermaid,
            Some("json" | "jsonc") => Doc::Json,
            Some("csv") => Doc::Table(','),
            Some("tsv") => Doc::Table('\t'),
            // Web pages, images, media and fonts are shown as they are.
            _ if mime(path).is_some() && !matches!(ext.as_deref(), Some("js" | "mjs" | "css")) => {
                return None;
            }
            _ => Doc::Text,
        };
        // Text only: no binaries, nothing huge.
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() || meta.len() > MAX_DOC {
            return None;
        }
        if doc == Doc::Text {
            let mut head = vec![0; 8192];
            let n = std::io::Read::read(&mut std::fs::File::open(path).ok()?, &mut head).ok()?;
            if head[..n].contains(&0) {
                return None;
            }
        }
        Some(doc)
    }
}

/// A document rendered to HTML. Its own scripts never run: the CSP allows
/// only desk's Mermaid, by nonce.
fn document(path: &Path, kind: Doc) -> Response<Vec<u8>> {
    let Ok(bytes) = std::fs::read(path) else {
        return not_found();
    };
    let text = String::from_utf8_lossy(&bytes);
    let nonce = format!("{:016x}", RandomState::new().hash_one(path));
    let html = render(path, kind, &text, &nonce);
    let csp = format!(
        "default-src 'none'; img-src 'self' data: https: http:; media-src 'self'; \
         style-src 'unsafe-inline'; font-src 'self' data:; script-src 'nonce-{nonce}'; \
         form-action 'none'; base-uri 'self'"
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_SECURITY_POLICY, csp)
        .header("X-Content-Type-Options", "nosniff")
        .body(html.into_bytes())
        .unwrap_or_else(|_| not_found())
}

pub fn render(path: &Path, kind: Doc, text: &str, nonce: &str) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let body = match kind {
        Doc::Markdown => {
            use pulldown_cmark::{Options, Parser, html};
            let mut out = String::new();
            html::push_html(&mut out, Parser::new_ext(text, Options::all()));
            format!(r#"<article class="md">{out}</article>"#)
        }
        Doc::Mermaid => format!(r#"<div class="mermaid">{}</div>"#, esc(text)),
        Doc::Json => {
            let pretty = serde_json::from_str::<serde_json::Value>(text)
                .and_then(|v| serde_json::to_string_pretty(&v))
                .unwrap_or_else(|_| text.to_string());
            format!(r#"<pre class="code">{}</pre>"#, esc(&pretty))
        }
        Doc::Table(sep) => table(text, sep),
        Doc::Text => format!(r#"<pre class="code">{}</pre>"#, esc(text)),
    };
    // Relative links and images resolve inside the file's folder, when it may be served.
    let base = file_url(path)
        .ok()
        .map(|u| format!(r#"<base href="{}">"#, esc(u.as_str())))
        .unwrap_or_default();
    let mermaid = if body.contains("language-mermaid") || kind == Doc::Mermaid {
        format!(
            r#"<script nonce="{nonce}" src="/_desk/mermaid.js"></script>
<script nonce="{nonce}">
for (const c of document.querySelectorAll("pre > code.language-mermaid")) {{
  const d = document.createElement("div");
  d.className = "mermaid";
  d.textContent = c.textContent;
  c.parentElement.replaceWith(d);
}}
mermaid.initialize({{ startOnLoad: false, securityLevel: "strict",
  theme: matchMedia("(prefers-color-scheme: light)").matches ? "default" : "dark" }});
mermaid.run();
</script>"#
        )
    } else {
        String::new()
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8">{base}<title>{title}</title><style>{DOC_CSS}</style></head>
<body>{body}{mermaid}</body></html>"#,
        title = esc(&name)
    )
}

fn table(text: &str, sep: char) -> String {
    let mut out = String::from(r#"<table class="grid">"#);
    for (i, line) in text.lines().take(5000).enumerate() {
        let cell = if i == 0 { "th" } else { "td" };
        out.push_str("<tr>");
        for c in split_row(line, sep) {
            out.push_str(&format!("<{cell}>{}</{cell}>", esc(&c)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

/// One CSV row; quotes keep separators, `""` is a quote.
pub fn split_row(line: &str, sep: char) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cells.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => cells.push(String::new()),
            c => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const DOC_CSS: &str = r#"
:root { color-scheme: dark light; --fg: #d6d9de; --dim: #8b929c; --bg: #15171b; --line: #2a2e35; --code: #1d2026; --link: #8ab4f8; }
@media (prefers-color-scheme: light) { :root { --fg: #1f2328; --dim: #59636e; --bg: #ffffff; --line: #d1d9e0; --code: #f6f8fa; --link: #0969da; } }
html { background: var(--bg); color: var(--fg); }
body { margin: 0; padding: 20px 28px 48px; font: 14px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", Ubuntu, sans-serif; }
a { color: var(--link); }
.md { max-width: 860px; }
.md h1, .md h2 { border-bottom: 1px solid var(--line); padding-bottom: .3em; }
.md img { max-width: 100%; }
.md table, table.grid { border-collapse: collapse; }
.md th, .md td, table.grid th, table.grid td { border: 1px solid var(--line); padding: 4px 10px; text-align: left; vertical-align: top; }
table.grid { font: 12px/1.4 ui-monospace, "SF Mono", Menlo, monospace; }
table.grid th { position: sticky; top: 0; background: var(--code); }
code, pre { font: 12.5px/1.5 ui-monospace, "SF Mono", Menlo, Consolas, monospace; }
:not(pre) > code { background: var(--code); padding: .1em .35em; border-radius: 4px; }
pre { background: var(--code); padding: 12px 14px; border-radius: 6px; overflow: auto; }
pre.code { margin: 0; white-space: pre-wrap; word-break: break-word; }
blockquote { margin: 0; padding: 0 1em; color: var(--dim); border-left: 3px solid var(--line); }
.mermaid { display: flex; justify-content: center; margin: 16px 0; }
hr { border: 0; border-top: 1px solid var(--line); }
"#;

/// A web page may not open `deskpreview://` URLs (other previewed folders):
/// only desk itself, or a page that already is local.
fn allow_navigation(u: &Url) -> bool {
    let mut nav = NAV.lock().unwrap();
    match u.scheme() {
        "http" | "https" => {
            nav.1 = false;
            true
        }
        SCHEME if nav.1 || nav.0.as_ref() == Some(u) => {
            nav.0 = None;
            nav.1 = true;
            true
        }
        _ => false,
    }
}

#[derive(Debug, PartialEq)]
enum External {
    /// A URL or a document for the system opener.
    Open(String),
    /// Anything a default app might run (a `.command` script): an editor.
    Editor(PathBuf),
}

fn external(link: &str) -> Result<External, String> {
    Ok(match parse(link)? {
        Target::Web(u) => External::Open(u.to_string()),
        // Documents only: the path is absolute and URLs start with http, so
        // neither can be taken for an option.
        Target::File(p) if mime(&p).is_some() || document_type(&p) => {
            External::Open(p.display().to_string())
        }
        Target::File(p) => External::Editor(p),
    })
}

/// Open in the system browser, a local file in its default app, or (for
/// anything a default app might run, like a `.command` script) in an editor.
pub fn open_external(link: &str) -> Result<(), String> {
    let arg = match external(link)? {
        External::Open(arg) => arg,
        External::Editor(p) => return crate::project::open_path_in_editor(&p),
    };
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        // The shell's own opener; no cmd.exe parsing of the URL.
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(arg).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_web_and_existing_files_only() {
        assert!(matches!(
            parse("https://example.com/mock?x=1"),
            Ok(Target::Web(_))
        ));
        assert!(matches!(
            parse("http://localhost:5173/"),
            Ok(Target::Web(_))
        ));
        assert!(parse("javascript:alert(1)").is_err());
        assert!(parse("file:///etc/definitely-missing.html").is_err());
        assert!(parse("ssh://host").is_err());
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("mock up.html");
        std::fs::write(&f, "<p>hi</p>").unwrap();
        assert!(matches!(parse(f.to_str().unwrap()), Ok(Target::File(_))));
    }

    #[test]
    fn serves_only_inside_the_previewed_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let site = tmp.path().join("site");
        std::fs::create_dir_all(site.join("img")).unwrap();
        std::fs::write(site.join("index.html"), "x").unwrap();
        std::fs::write(site.join("img/a.png"), "x").unwrap();
        std::fs::write(tmp.path().join("secret.txt"), "x").unwrap();
        std::fs::write(site.join(".env"), "x").unwrap();
        std::fs::write(site.join("notes.txt"), "x").unwrap();
        std::fs::create_dir_all(site.join(".git")).unwrap();
        std::fs::write(site.join(".git/config.html"), "x").unwrap();
        let root = site.canonicalize().unwrap();
        assert!(resolve_in(&root, "index.html").is_some());
        assert!(resolve_in(&root, "img/a.png").is_some());
        assert!(resolve_in(&root, "../secret.txt").is_none());
        assert!(
            resolve_in(
                &root,
                if cfg!(windows) {
                    r"C:\Windows\win.ini"
                } else {
                    "/etc/hosts"
                }
            )
            .is_none()
        );
        assert!(resolve_in(&root, "img").is_none());
        assert!(resolve_in(&root, ".env").is_none());
        assert!(resolve_in(&root, ".git/config.html").is_none());

        let url = file_url(&root.join("index.html")).unwrap();
        assert_eq!(url.scheme(), SCHEME);
        let path = url.path().to_string();
        assert_eq!(
            resolve(&path),
            Some((root.join("index.html"), "text/html; charset=utf-8"))
        );
        assert_eq!(resolve(&path.replace("index.html", "notes.txt")), None);
        let escape = path.replace("index.html", "..%2Fsecret.txt");
        assert_eq!(resolve(&escape), None);
        assert_eq!(resolve("/unknown-token/index.html"), None);
    }

    fn get(path: &str) -> Response<Vec<u8>> {
        serve(
            &Request::builder()
                .uri(format!("{SCHEME}://localhost{path}"))
                .body(Vec::new())
                .unwrap(),
        )
    }

    #[test]
    fn renders_documents_without_their_own_scripts() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().canonicalize().unwrap();
        let md = d.join("README.md");
        std::fs::write(
            &md,
            "# Tytuł\n\n<script>alert(1)</script>\n\n```mermaid\ngraph TD; A-->B\n```\n\n[docs](docs/a.md)\n",
        )
        .unwrap();
        std::fs::create_dir_all(d.join("docs")).unwrap();
        std::fs::write(d.join("docs/a.md"), "# A").unwrap();
        std::fs::write(d.join("data.csv"), "a,b\n\"x, y\",2\n").unwrap();
        std::fs::write(d.join("main.rs"), "fn main() { let a = 1 < 2; }").unwrap();
        std::fs::write(d.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        std::fs::write(d.join("page.html"), "<p>x</p>").unwrap();

        assert_eq!(Doc::of(&md), Some(Doc::Markdown));
        assert_eq!(Doc::of(&d.join("data.csv")), Some(Doc::Table(',')));
        assert_eq!(Doc::of(&d.join("main.rs")), Some(Doc::Text));
        assert_eq!(Doc::of(&d.join("blob.bin")), None, "binary");
        assert_eq!(
            Doc::of(&d.join("page.html")),
            None,
            "a page is shown as it is"
        );

        let url = local_url(&md).unwrap();
        assert!(url.path().starts_with("/doc/"), "{url}");
        assert_eq!(display(&url), md.display().to_string());
        let res = get(url.path());
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("script-src 'nonce-"), "{csp}");
        assert!(!csp.contains("script-src 'unsafe"), "{csp}");
        let html = String::from_utf8(res.body().clone()).unwrap();
        assert!(html.contains("<h1>Tytuł</h1>"), "{html}");
        assert!(html.contains("/_desk/mermaid.js"));
        assert!(html.contains("language-mermaid"));
        // The page's own script stays in the HTML but has no nonce, so the CSP blocks it.
        assert!(html.contains("<script>alert(1)</script>"));

        let csv = render(&d.join("data.csv"), Doc::Table(','), "a,b\n\"x, y\",2", "n");
        assert!(csv.contains("<td>x, y</td>"), "{csv}");
        let code = render(&d.join("main.rs"), Doc::Text, "1 < 2", "n");
        assert!(code.contains("1 &lt; 2") && !code.contains("mermaid.js"));

        // Linked Markdown in the previewed folder renders; other text files there do not leak.
        let base = file_url(&md).unwrap();
        let linked = base.path().replace("README.md", "docs/a.md");
        assert!(
            String::from_utf8(get(&linked).body().clone())
                .unwrap()
                .contains("<h1>A</h1>")
        );
        assert_eq!(
            get(&base.path().replace("README.md", "main.rs")).status(),
            404
        );
        assert_eq!(get("/doc/0000000000000000/x.md").status(), 404);
        assert_eq!(get("/_desk/mermaid.js").status(), 200);
    }

    #[test]
    fn splits_csv_rows() {
        assert_eq!(
            split_row(r#"a,"b,c","d""e",,"#, ','),
            ["a", "b,c", "d\"e", "", ""]
        );
        assert_eq!(split_row("a\tb", '\t'), ["a", "b"]);
    }

    #[test]
    fn opens_only_document_types_externally() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("run.command");
        std::fs::write(&script, "echo hi").unwrap();
        // A script goes to an editor, never to its default app (Terminal would run it).
        let script = script.canonicalize().unwrap();
        assert_eq!(
            external(script.to_str().unwrap()),
            Ok(External::Editor(script))
        );
        let notes = tmp.path().join("notes.md");
        std::fs::write(&notes, "# x").unwrap();
        assert!(matches!(
            external(notes.to_str().unwrap()),
            Ok(External::Open(_))
        ));
        assert!(open_external("javascript:alert(1)").is_err());
    }

    #[test]
    fn edits_only_files_shown_in_the_panel() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().canonicalize().unwrap();
        let shown = d.join("shown.md");
        let other = d.join("other.json");
        std::fs::write(&shown, "# a").unwrap();
        std::fs::write(&other, "{}").unwrap();
        assert!(read_text(&shown).is_err(), "not opened yet");
        local_url(&shown).unwrap();
        assert_eq!(read_text(&shown).unwrap(), "# a");
        write_text(&shown, "# b").unwrap();
        assert_eq!(std::fs::read_to_string(&shown).unwrap(), "# b");
        assert!(write_text(&other, "x").is_err());
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "{}");
    }

    #[test]
    fn refuses_network_paths() {
        assert!(parse("file://attacker.example/share/x.html").is_err());
        #[cfg(windows)]
        {
            assert!(parse(r"\\attacker\share\x.html").is_err());
            assert!(parse("//attacker/share/x.html").is_err());
            assert!(!local_path(Path::new(r"\\?\UNC\attacker\share\x.html")));
            assert!(local_path(Path::new(r"C:\Users\x.html")));
        }
        #[cfg(unix)]
        assert!(!local_path(Path::new("//attacker/share/x.html")));
    }

    #[test]
    fn never_exposes_the_home_folder() {
        let home = crate::env::home();
        assert!(file_url(&home.join("mock.html")).is_err());
        #[cfg(unix)]
        assert!(file_url(Path::new("/mock.html")).is_err());
    }

    #[test]
    fn web_pages_cannot_open_preview_urls() {
        let local: Url = "deskpreview://localhost/abc/index.html".parse().unwrap();
        let web: Url = "https://example.com/".parse().unwrap();
        *NAV.lock().unwrap() = (None, false);
        assert!(allow_navigation(&web));
        assert!(!allow_navigation(&local), "a web page guessed a token");
        NAV.lock().unwrap().0 = Some(local.clone());
        assert!(allow_navigation(&local), "desk opened it");
        let other: Url = "deskpreview://localhost/abc/b.html".parse().unwrap();
        assert!(allow_navigation(&other), "local pages link to each other");
        assert!(!allow_navigation(&"javascript:alert(1)".parse().unwrap()));
    }
}
