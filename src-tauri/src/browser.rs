//! Web pages leave the side panel for a real browser: Brave or Chrome when
//! installed (Options picks which; "auto" tries Brave first), else the
//! system's default browser.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A browser found on this computer.
#[derive(Debug, Clone, PartialEq)]
pub struct Browser {
    pub name: &'static str,
    /// The `.app` bundle (macOS) or the executable.
    pub path: PathBuf,
}

/// Brave and Chrome, in the order `pref` asks for ("auto", "brave",
/// "chrome"; "system" = neither).
fn wanted(pref: &str) -> &'static [&'static str] {
    match pref {
        "system" => &[],
        "brave" => &["Brave"],
        "chrome" => &["Chrome"],
        _ => &["Brave", "Chrome"],
    }
}

/// Where `name` may be installed.
fn candidates(name: &str, home: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        let app = if name == "Brave" { "Brave Browser.app" } else { "Google Chrome.app" };
        return vec![Path::new("/Applications").join(app), home.join("Applications").join(app)];
    }
    if cfg!(windows) {
        let rel = if name == "Brave" {
            r"BraveSoftware\Brave-Browser\Application\brave.exe"
        } else {
            r"Google\Chrome\Application\chrome.exe"
        };
        return ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|base| PathBuf::from(base).join(rel))
            .collect();
    }
    let bins: &[&str] = if name == "Brave" {
        &["brave-browser", "brave", "brave-browser-stable"]
    } else {
        &["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .flat_map(|dir| bins.iter().map(move |b| dir.join(b)))
        .collect()
}

/// The browser `pref` asks for, if it is installed.
pub fn find(pref: &str) -> Option<Browser> {
    let home = dirs_home();
    wanted(pref).iter().find_map(|&name| {
        candidates(name, &home).into_iter().find(|p| p.exists()).map(|path| Browser { name, path })
    })
}

fn dirs_home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Open an http(s) `url` (already checked by the caller) in `browser`.
pub fn open(browser: &Browser, url: &str) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg("-a").arg(&browser.path);
        c
    } else {
        Command::new(&browser.path)
    };
    cmd.arg(url).spawn().map(|_| ()).map_err(|e| format!("{}: {e}", browser.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_brave_then_chrome() {
        assert_eq!(wanted("auto"), ["Brave", "Chrome"]);
        assert_eq!(wanted("chrome"), ["Chrome"]);
        assert!(wanted("system").is_empty());
        assert!(find("system").is_none());
    }

    #[test]
    fn looks_where_each_system_installs_them() {
        let home = Path::new("/home/me");
        let brave = candidates("Brave", home);
        if cfg!(target_os = "macos") {
            assert_eq!(brave[0], Path::new("/Applications/Brave Browser.app"));
            assert_eq!(candidates("Chrome", home)[1], Path::new("/home/me/Applications/Google Chrome.app"));
        } else if cfg!(windows) {
            assert!(brave.iter().all(|p| p.ends_with(r"BraveSoftware\Brave-Browser\Application\brave.exe")));
        } else {
            assert!(brave.iter().all(|p| p.file_name().unwrap().to_string_lossy().starts_with("brave")));
        }
    }
}
