//! Process environment for an app started from Finder: launchd gives it a
//! minimal PATH (no Homebrew, no ~/.local/bin) and often no LANG, so tmux and
//! claude would not be found or would mangle UTF-8.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

/// Directory holding layout.json, state/, the hook script and tmux.conf.
pub fn support_dir() -> PathBuf {
    // A separate data folder (layout, state) for a second desk, e.g. to test.
    if let Some(dir) = std::env::var_os("DESK_SUPPORT_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    dirs::data_dir()
        .unwrap_or_else(|| home().join("Library/Application Support"))
        .join("desk")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn user_shell() -> String {
    if cfg!(windows) {
        // PowerShell 7 when installed, else the one that ships with Windows.
        return crate::agents::which("pwsh")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "powershell.exe".into());
    }
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into())
}

static CLAUDE: OnceLock<String> = OnceLock::new();

/// The `claude` the user gets in an interactive terminal (absolute path when
/// known). A login-only shell can resolve an older copy earlier in PATH, e.g.
/// Homebrew's, when ~/.local/bin is added only by ~/.zshrc.
pub fn claude_bin() -> &'static str {
    CLAUDE.get().map(String::as_str).unwrap_or("claude")
}

/// Adopt the interactive login shell's PATH and `claude`, and make sure a
/// UTF-8 locale is set.
pub fn init() {
    let inherited = inherited_session_vars(std::env::vars().map(|(k, _)| k));
    // SAFETY: called once at startup before any other thread is spawned.
    unsafe {
        for k in inherited {
            std::env::remove_var(k);
        }
    }
    if cfg!(windows) {
        // Apps on Windows get the user's PATH from the environment already.
        if let Some(c) = crate::agents::which("claude") {
            let _ = CLAUDE.set(c.display().to_string());
        }
        return;
    }
    let fallback = {
        let h = home();
        let mut parts: Vec<String> = vec![
            h.join(".local/bin").display().to_string(),
            "/opt/homebrew/bin".into(),
            "/usr/local/bin".into(),
            h.join(".cargo/bin").display().to_string(),
        ];
        if let Ok(p) = std::env::var("PATH") {
            parts.push(p);
        }
        parts.join(":")
    };
    let probe = probe_shell();
    let path = probe.as_ref().map(|p| p.path.clone()).unwrap_or(fallback);
    if let Some(claude) = probe.and_then(|p| p.claude) {
        let _ = CLAUDE.set(claude);
    }
    // SAFETY: called once at startup before any other thread is spawned.
    unsafe {
        std::env::set_var("PATH", path);
        if std::env::var("LANG").map(|v| v.is_empty()).unwrap_or(true) {
            std::env::set_var("LANG", "en_US.UTF-8");
        }
    }
}

/// Variables desk inherits when it is started from inside a Claude Code
/// session (e.g. a terminal in the Claude app). Passed on, they would make
/// every agent in desk a child of that session: no transcript saved, the
/// host app's API endpoint and tokens. Your shell profile sets your own again.
fn inherited_session_vars(names: impl Iterator<Item = String>) -> Vec<String> {
    let names: Vec<String> = names.collect();
    if !names
        .iter()
        .any(|k| k == "CLAUDECODE" || k == "CLAUDE_CODE_CHILD_SESSION")
    {
        return Vec::new();
    }
    names
        .into_iter()
        .filter(|k| {
            (k.starts_with("CLAUDE") && k != "CLAUDE_CONFIG_DIR")
                || k.starts_with("ANTHROPIC_")
                || k.starts_with("MCP_")
                || k == "AI_AGENT"
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct Probe {
    path: String,
    claude: Option<String>,
}

/// Ask an interactive login shell for PATH and `command -v claude`, with a
/// 3 s timeout so a slow shell profile cannot block startup.
fn probe_shell() -> Option<Probe> {
    let (tx, rx) = mpsc::channel();
    let shell = user_shell();
    std::thread::spawn(move || {
        let out = Command::new(shell)
            .args([
                "-ilc",
                "printf '__DESK_PATH__%s__DESK_CLAUDE__%s__DESK_END__' \"$PATH\" \"$(command -v claude)\"",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let _ = tx.send(out);
    });
    let out = rx.recv_timeout(Duration::from_secs(3)).ok()?.ok()?;
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

fn parse_probe(stdout: &str) -> Option<Probe> {
    let rest = stdout.rsplit_once("__DESK_PATH__")?.1;
    let (path, rest) = rest.split_once("__DESK_CLAUDE__")?;
    let claude = rest.split_once("__DESK_END__")?.0.trim();
    let path = path.trim();
    (!path.is_empty()).then(|| Probe {
        path: path.to_string(),
        // Only an absolute path: an alias or function name is not runnable from tmux.
        claude: claude.starts_with('/').then(|| claude.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_a_parent_claude_sessions_variables() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let inside = names(&[
            "PATH",
            "CLAUDECODE",
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CONFIG_DIR",
            "ANTHROPIC_BASE_URL",
            "MCP_CONNECTION_NONBLOCKING",
            "AI_AGENT",
            "HOME",
        ]);
        assert_eq!(
            inherited_session_vars(inside.into_iter()),
            names(&[
                "CLAUDECODE",
                "CLAUDE_CODE_CHILD_SESSION",
                "CLAUDE_CODE_SESSION_ID",
                "ANTHROPIC_BASE_URL",
                "MCP_CONNECTION_NONBLOCKING",
                "AI_AGENT",
            ])
        );
        // Started normally: your own ANTHROPIC_* settings stay.
        let outside = names(&["PATH", "ANTHROPIC_API_KEY", "CLAUDE_CONFIG_DIR"]);
        assert!(inherited_session_vars(outside.into_iter()).is_empty());
    }

    #[test]
    fn probe_ignores_profile_noise() {
        let out =
            "zoxide: warning\n__DESK_PATH__/a/bin:/usr/bin__DESK_CLAUDE__/a/bin/claude__DESK_END__";
        assert_eq!(
            parse_probe(out),
            Some(Probe {
                path: "/a/bin:/usr/bin".into(),
                claude: Some("/a/bin/claude".into())
            })
        );
    }

    #[test]
    fn probe_without_claude_or_path() {
        let out = "__DESK_PATH__/usr/bin__DESK_CLAUDE__claude: aliased to x__DESK_END__";
        assert_eq!(parse_probe(out).unwrap().claude, None);
        assert_eq!(
            parse_probe("__DESK_PATH____DESK_CLAUDE____DESK_END__"),
            None
        );
        assert_eq!(parse_probe("no marker"), None);
    }
}
