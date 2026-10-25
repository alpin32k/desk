//! Quick preview of links printed in a terminal (mockups, dev servers, local
//! HTML files) in a separate desk window. That window loads untrusted content,
//! so it has no Tauri permissions: capabilities/default.json covers only "main".

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

const LABEL: &str = "preview";

/// Only web pages and existing local files; nothing that runs a program.
pub fn parse(link: &str) -> Result<Url, String> {
    let url = if link.starts_with('/') {
        Url::from_file_path(link).map_err(|_| format!("zła ścieżka: {link}"))?
    } else {
        Url::parse(link).map_err(|e| format!("{link}: {e}"))?
    };
    match url.scheme() {
        "http" | "https" => Ok(url),
        "file" => {
            let path = url.to_file_path().map_err(|_| "zła ścieżka".to_string())?;
            if path.is_file() {
                Ok(url)
            } else {
                Err(format!("nie ma pliku {}", path.display()))
            }
        }
        other => Err(format!("nieobsługiwany adres ({other}:)")),
    }
}

pub fn open(app: &AppHandle, link: &str) -> Result<(), String> {
    let url = parse(link)?;
    let title = format!("Podgląd · {url}");
    if let Some(w) = app.get_webview_window(LABEL) {
        w.navigate(url).map_err(|e| e.to_string())?;
        let _ = w.set_title(&title);
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url))
        .title(title)
        .inner_size(1100.0, 800.0)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Open in the system browser (or the default app for a local file).
pub fn open_external(link: &str) -> Result<(), String> {
    let url = parse(link)?;
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_web_and_existing_files_only() {
        assert!(parse("https://example.com/mock?x=1").is_ok());
        assert!(parse("http://localhost:5173/").is_ok());
        assert!(parse("javascript:alert(1)").is_err());
        assert!(parse("file:///etc/definitely-missing.html").is_err());
        assert!(parse("ssh://host").is_err());
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("mock up.html");
        std::fs::write(&f, "<p>hi</p>").unwrap();
        let u = parse(f.to_str().unwrap()).unwrap();
        assert_eq!(u.scheme(), "file");
        assert!(u.as_str().contains("mock%20up.html"));
    }
}
