//! Quick preview of links printed in a terminal (mockups, dev servers, local
//! HTML files) in a separate desk window. That window loads untrusted content:
//! - it can call none of desk's commands (build.rs app manifest; the
//!   capability in capabilities/default.json is granted only to "main"),
//! - it may only navigate to http(s), and to `deskpreview://` only when desk
//!   opened that URL or the page already is a `deskpreview://` page,
//! - local files are never loaded as `file://`. `deskpreview://` serves web
//!   asset types (no dotfiles) from the previewed file's folder under a random
//!   token, never the home folder or above, with a CSP that blocks fetch/XHR
//!   and form posts to other hosts.
//!
//! A page can still send what it may read (its own folder) out through a
//! link or an image URL: preview only files you would open in a browser.

use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::http::{Request, Response, header};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

const LABEL: &str = "preview";
pub const SCHEME: &str = "deskpreview";

/// Folders opened for preview, by token (the first path segment).
static ROOTS: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

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
    let url = if link.starts_with('/') {
        Url::from_file_path(link).map_err(|_| format!("zła ścieżka: {link}"))?
    } else {
        Url::parse(link).map_err(|e| format!("{link}: {e}"))?
    };
    match url.scheme() {
        "http" | "https" => Ok(Target::Web(url)),
        "file" => {
            let path = url.to_file_path().map_err(|_| "zła ścieżka".to_string())?;
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

/// `deskpreview://localhost/<token>/<file>`, registering the file's folder.
fn file_url(path: &Path) -> Result<Url, String> {
    let dir = path.parent().ok_or("brak folderu")?.to_path_buf();
    if crate::env::home().starts_with(&dir) {
        return Err(format!(
            "podgląd pliku z {} odsłoniłby cały folder; przenieś go do podfolderu",
            dir.display()
        ));
    }
    let name = path
        .file_name()
        .ok_or("brak nazwy pliku")?
        .to_string_lossy();
    let token = {
        let mut roots = ROOTS.lock().unwrap();
        let roots = roots.get_or_insert_with(HashMap::new);
        match roots.iter().find(|(_, d)| **d == dir) {
            Some((t, _)) => t.clone(),
            None => {
                // Random, so a previewed page cannot guess other folders' tokens.
                let t = format!("{:016x}", RandomState::new().hash_one(&dir));
                roots.insert(t.clone(), dir);
                t
            }
        }
    };
    let mut url = Url::parse(&format!("{SCHEME}://localhost/")).map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "zły adres")?
        .push(&token)
        .push(&name);
    Ok(url)
}

/// Protocol handler: a file inside a registered folder, nothing outside it.
pub fn serve(req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    match resolve(req.uri().path()) {
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
    let mut parts = uri_path.trim_start_matches('/').splitn(2, '/');
    let token = parts.next()?;
    let rel = percent_encoding::percent_decode_str(parts.next()?)
        .decode_utf8()
        .ok()?;
    let root = ROOTS.lock().unwrap().as_ref()?.get(token)?.clone();
    let path = resolve_in(&root, &rel)?;
    Some((path.clone(), mime(&path)?))
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

pub fn open(app: &AppHandle, link: &str) -> Result<(), String> {
    let url = match parse(link)? {
        Target::Web(u) => u,
        Target::File(p) => file_url(&p)?,
    };
    if url.scheme() == SCHEME {
        NAV.lock().unwrap().0 = Some(url.clone());
    }
    let title = format!("Podgląd · {link}");
    if let Some(w) = app.get_webview_window(LABEL) {
        w.navigate(url).map_err(|e| e.to_string())?;
        let _ = w.set_title(&title);
        let _ = w.set_focus();
        return Ok(());
    }
    let local = url.scheme() == SCHEME;
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
        .title(title)
        .inner_size(1100.0, 800.0)
        .on_navigation(allow_navigation)
        .build()
        .map_err(|e| e.to_string())?;
    // The first page may load without a navigation check.
    *NAV.lock().unwrap() = (None, local);
    Ok(())
}

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

/// Open in the system browser (or the default app for a local file).
pub fn open_external(link: &str) -> Result<(), String> {
    let arg = match parse(link)? {
        Target::Web(u) => u.to_string(),
        // Only documents a browser shows: the default app for a `.command`
        // or `.terminal` file would run it. The path is absolute and URLs
        // start with http, so neither can be taken for an option.
        Target::File(p) if mime(&p).is_some() => p.display().to_string(),
        Target::File(p) => return Err(format!("nie otwieram tego typu pliku: {}", p.display())),
    };
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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
        assert!(resolve_in(&root, "/etc/hosts").is_none());
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

    #[test]
    fn opens_only_document_types_externally() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("run.command");
        std::fs::write(&script, "echo hi").unwrap();
        assert!(open_external(script.to_str().unwrap()).is_err());
        assert!(open_external("javascript:alert(1)").is_err());
    }

    #[test]
    fn never_exposes_the_home_folder() {
        let home = crate::env::home();
        assert!(file_url(&home.join("mock.html")).is_err());
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
