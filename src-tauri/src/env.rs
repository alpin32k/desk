//! Process environment for an app started from Finder: launchd gives it a
//! minimal PATH (no Homebrew, no ~/.local/bin) and often no LANG, so tmux and
//! claude would not be found or would mangle UTF-8.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Directory holding layout.json, state/, the hook script and tmux.conf.
pub fn support_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| home().join("Library/Application Support"))
        .join("desk")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into())
}

/// Adopt the login shell's PATH and make sure a UTF-8 locale is set.
pub fn init() {
    let fallback = {
        let h = home();
        let mut parts: Vec<String> = vec![
            "/opt/homebrew/bin".into(),
            "/usr/local/bin".into(),
            h.join(".local/bin").display().to_string(),
            h.join(".cargo/bin").display().to_string(),
        ];
        if let Ok(p) = std::env::var("PATH") {
            parts.push(p);
        }
        parts.join(":")
    };
    let path = login_path().unwrap_or(fallback);
    // SAFETY: called once at startup before any other thread is spawned.
    unsafe {
        std::env::set_var("PATH", path);
        if std::env::var("LANG").map(|v| v.is_empty()).unwrap_or(true) {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
    }
}

/// PATH as seen by an interactive login shell, with a 3 s timeout so a slow
/// shell profile cannot block startup.
fn login_path() -> Option<String> {
    let (tx, rx) = mpsc::channel();
    let shell = user_shell();
    std::thread::spawn(move || {
        let out = Command::new(shell)
            .args(["-ilc", "printf '__DESK_PATH__%s' \"$PATH\""])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let _ = tx.send(out);
    });
    let out = rx.recv_timeout(Duration::from_secs(3)).ok()?.ok()?;
    parse_marked_path(&String::from_utf8_lossy(&out.stdout))
}

fn parse_marked_path(stdout: &str) -> Option<String> {
    let path = stdout.rsplit("__DESK_PATH__").next()?.trim();
    (!path.is_empty() && stdout.contains("__DESK_PATH__")).then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marked_path_ignores_profile_noise() {
        let out = "zoxide: warning\n__DESK_PATH__/opt/homebrew/bin:/usr/bin";
        assert_eq!(
            parse_marked_path(out).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(parse_marked_path("no marker"), None);
        assert_eq!(parse_marked_path("__DESK_PATH__"), None);
    }
}
