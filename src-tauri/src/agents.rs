//! Coding agents a tile can run: Claude Code (with desk's state hooks), other
//! CLIs such as Codex or opencode, a plain shell, or a command added by hand.

use crate::tmux::shell_quote;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// (id, command). `shell` has no command: the pane is just a login shell.
pub const BUILTIN: &[(&str, &str)] = &[
    ("claude", "claude"),
    ("codex", "codex"),
    ("opencode", "opencode"),
    ("gemini", "gemini"),
    ("aider", "aider"),
    ("shell", ""),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgent {
    pub id: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub command: String,
    pub installed: bool,
    pub builtin: bool,
}

/// Built-ins first, then custom ones (a custom id never shadows a built-in).
pub fn list(custom: &[CustomAgent]) -> Vec<AgentInfo> {
    let builtin = BUILTIN.iter().map(|(id, cmd)| AgentInfo {
        id: id.to_string(),
        command: cmd.to_string(),
        installed: cmd.is_empty() || which(cmd).is_some(),
        builtin: true,
    });
    let custom = custom
        .iter()
        .filter(|c| !BUILTIN.iter().any(|(id, _)| *id == c.id))
        .map(|c| AgentInfo {
            id: c.id.clone(),
            command: c.command.clone(),
            installed: first_word(&c.command).is_some_and(|w| which(w).is_some()),
            builtin: false,
        });
    builtin.chain(custom).collect()
}

/// Command line for an agent id, from the built-ins or the custom list.
pub fn command_for(id: &str, custom: &[CustomAgent]) -> Option<String> {
    BUILTIN
        .iter()
        .find(|(b, _)| *b == id)
        .map(|(_, c)| c.to_string())
        .or_else(|| {
            custom
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.command.clone())
        })
}

/// Shell script run in the tmux pane. After the agent exits the pane falls
/// back to a login shell so the tile stays usable.
pub fn launch_script(id: &str, command: &str, hooks_settings: &Path) -> String {
    let shell = shell_quote(&crate::env::user_shell());
    if id == "claude" {
        return format!(
            "{} --settings {}; exec {shell} -l",
            shell_quote(crate::env::claude_bin()),
            shell_quote(&hooks_settings.display().to_string())
        );
    }
    if command.trim().is_empty() {
        return format!("exec {shell} -l");
    }
    format!("{}; exec {shell} -l", resolve_command(command, which))
}

/// Replace the program name with its absolute path when found, so the same
/// binary runs as in the user's interactive shell (the pane's login shell may
/// order PATH differently). Arguments are kept as written.
pub fn resolve_command(command: &str, which: impl Fn(&str) -> Option<PathBuf>) -> String {
    let trimmed = command.trim();
    let Some(first) = first_word(trimmed) else {
        return trimmed.to_string();
    };
    match which(first) {
        Some(path) => format!(
            "{}{}",
            shell_quote(&path.display().to_string()),
            &trimmed[first.len()..]
        ),
        None => trimmed.to_string(),
    }
}

fn first_word(command: &str) -> Option<&str> {
    command.split_whitespace().next()
}

/// Look a program up in PATH (the interactive shell's PATH, set at startup).
pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let p = PathBuf::from(program);
        return is_executable(&p).then_some(p);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|p| is_executable(p))
    })
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_program_and_keeps_arguments() {
        let which = |w: &str| (w == "codex").then(|| PathBuf::from("/opt/bin/codex"));
        assert_eq!(
            resolve_command("codex --full-auto", which),
            "'/opt/bin/codex' --full-auto"
        );
        assert_eq!(resolve_command("  codex  ", which), "'/opt/bin/codex'");
        assert_eq!(resolve_command("unknown -x", which), "unknown -x");
        assert_eq!(resolve_command("", which), "");
    }

    #[test]
    fn launch_scripts() {
        let hooks = Path::new("/s/claude-hooks.json");
        let claude = launch_script("claude", "claude", hooks);
        assert!(
            claude.contains("--settings '/s/claude-hooks.json'"),
            "{claude}"
        );
        assert!(claude.ends_with(" -l"));
        assert!(launch_script("shell", "", hooks).starts_with("exec "));
        let custom = launch_script("mine", "definitely-not-installed-xyz --flag", hooks);
        assert!(
            custom.starts_with("definitely-not-installed-xyz --flag; exec "),
            "{custom}"
        );
    }

    #[test]
    fn lists_builtins_then_custom_without_shadowing() {
        let custom = vec![
            CustomAgent {
                id: "codex".into(),
                command: "evil".into(),
            },
            CustomAgent {
                id: "amp".into(),
                command: "amp --x".into(),
            },
        ];
        let l = list(&custom);
        assert_eq!(l[0].id, "claude");
        assert_eq!(l.iter().filter(|a| a.id == "codex").count(), 1);
        assert_eq!(l.last().unwrap().id, "amp");
        assert!(l.iter().find(|a| a.id == "shell").unwrap().installed);
        assert_eq!(command_for("codex", &custom).as_deref(), Some("codex"));
        assert_eq!(command_for("amp", &custom).as_deref(), Some("amp --x"));
        assert_eq!(command_for("nope", &custom), None);
    }

    #[test]
    fn which_finds_executables_only() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("tool");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        assert!(which(exe.to_str().unwrap()).is_none());
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(which(exe.to_str().unwrap()), Some(exe));
    }
}
