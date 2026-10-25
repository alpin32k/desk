//! tmux on a private socket (`tmux -L desk`) with desk's own config file, so the
//! user's tmux server and ~/.tmux.conf are never touched. One session per project.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const SOCKET: &str = "desk";

const CONF: &str = "\
# Written by desk on every start; edits are overwritten.
set -g prefix None
unbind C-b
set -g status off
set -g mouse on
set -s escape-time 0
set -g history-limit 50000
set -g default-terminal tmux-256color
set -as terminal-features ',xterm-256color:RGB:extkeys:clipboard'
set -s extended-keys on
set -g allow-passthrough on
set -g focus-events on
set -g set-clipboard on
set -g window-size latest
set -g aggressive-resize on
set -g remain-on-exit off
";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub slug: String,
    pub path: String,
}

#[derive(Clone)]
pub struct Tmux {
    conf: PathBuf,
}

impl Tmux {
    pub fn new(support_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(support_dir)?;
        let conf = support_dir.join("tmux.conf");
        std::fs::write(&conf, CONF)?;
        Ok(Self { conf })
    }

    /// Base command: `tmux -u -L desk -f <conf>`.
    pub fn command(&self) -> Command {
        let mut c = Command::new("tmux");
        c.arg("-u").arg("-L").arg(SOCKET).arg("-f").arg(&self.conf);
        c
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = self
            .command()
            .args(args)
            .output()
            .map_err(|e| format!("tmux: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        // No server yet means no sessions, not an error.
        self.run(&[
            "list-sessions",
            "-F",
            "#{session_name}\t#{@desk_path}\t#{session_path}",
        ])
        .map(|s| parse_sessions(&s))
        .unwrap_or_default()
    }

    pub fn has_session(&self, slug: &str) -> bool {
        self.run(&["has-session", "-t", &format!("={slug}")])
            .is_ok()
    }

    /// Start `claude --settings <hooks>` in a new detached session; when claude
    /// exits the pane falls back to a login shell so the tile stays usable.
    pub fn new_session(
        &self,
        slug: &str,
        cwd: &Path,
        hooks_settings: &Path,
        state_dir: &Path,
    ) -> Result<(), String> {
        let shell = crate::env::user_shell();
        let script = format!(
            "{} --settings {}; exec {} -l",
            shell_quote(crate::env::claude_bin()),
            shell_quote(&hooks_settings.display().to_string()),
            shell_quote(&shell)
        );
        let cwd_s = cwd.display().to_string();
        let slug_env = format!("DESK_SLUG={slug}");
        let state_env = format!("DESK_STATE_DIR={}", state_dir.display());
        self.run(&[
            "new-session",
            "-d",
            "-s",
            slug,
            "-c",
            &cwd_s,
            "-x",
            "200",
            "-y",
            "50",
            "-e",
            &slug_env,
            "-e",
            &state_env,
            &shell,
            "-lc",
            &script,
        ])?;
        self.run(&[
            "set-option",
            "-t",
            &format!("={slug}"),
            "@desk_path",
            &cwd_s,
        ])?;
        Ok(())
    }

    pub fn kill_session(&self, slug: &str) -> Result<(), String> {
        self.run(&["kill-session", "-t", &format!("={slug}")])
            .map(|_| ())
    }
}

pub fn parse_sessions(out: &str) -> Vec<SessionInfo> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.split('\t');
            let slug = f.next()?.to_string();
            let desk_path = f.next().unwrap_or_default();
            let session_path = f.next().unwrap_or_default();
            if slug.is_empty() {
                return None;
            }
            let path = if desk_path.is_empty() {
                session_path
            } else {
                desk_path
            };
            Some(SessionInfo {
                slug,
                path: path.to_string(),
            })
        })
        .collect()
}

/// Stable tmux-safe session name: sanitized folder name + 8 hex chars of a
/// hash of the full path (two `app` folders in different places do not clash;
/// `open_session` still checks the path, so a hash collision cannot mix them up).
pub fn slug(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut clean = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            clean.push(c);
        } else if !clean.ends_with('-') {
            clean.push('-');
        }
    }
    let clean = clean.trim_matches('-');
    let clean = if clean.is_empty() { "project" } else { clean };
    format!(
        "{clean}-{:08x}",
        fnv1a(path.display().to_string().as_bytes()) as u32
    )
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_tmux_safe_and_stable() {
        let s = slug(Path::new("/Users/k/Documents/projects/mm.crm v2"));
        assert!(s.starts_with("mm-crm-v2-"), "{s}");
        assert_eq!(s.len(), "mm-crm-v2-".len() + 8);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        assert_eq!(s, slug(Path::new("/Users/k/Documents/projects/mm.crm v2")));
    }

    #[test]
    fn same_name_different_path_differs() {
        assert_ne!(slug(Path::new("/a/app")), slug(Path::new("/b/app")));
        // Collided with the old 16-bit suffix.
        assert_ne!(
            slug(Path::new("/projects/498/app")),
            slug(Path::new("/projects/784/app"))
        );
    }

    #[test]
    fn slug_of_weird_name() {
        assert!(slug(Path::new("/x/...")).starts_with("project-"));
    }

    #[test]
    fn parses_list_sessions() {
        let out = "mm-crm-1a2b\t/Users/k/mm-crm\t/Users/k/mm-crm\n\
                   old-0000\t\t/Users/k/old\n\n";
        let s = parse_sessions(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].path, "/Users/k/mm-crm");
        assert_eq!(s[1].path, "/Users/k/old");
    }

    #[test]
    fn quotes_for_shell() {
        assert_eq!(shell_quote("/a b/c'd"), r"'/a b/c'\''d'");
    }
}
