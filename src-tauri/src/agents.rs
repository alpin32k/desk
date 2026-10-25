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

/// Program and arguments for a native (Windows) session: the agent runs in
/// PowerShell, which stays open after the agent exits, like the tmux pane.
/// Elsewhere (native sessions for testing) it is the login shell script.
pub fn launch_argv(id: &str, command: &str, hooks_settings: &Path) -> Vec<String> {
    let shell = crate::env::user_shell();
    if !cfg!(windows) {
        return vec![
            shell,
            "-lc".into(),
            launch_script(id, command, hooks_settings),
        ];
    }
    let run = if id == "claude" {
        format!(
            "& {} --settings {}",
            ps_quote(crate::env::claude_bin()),
            ps_quote(&hooks_settings.display().to_string())
        )
    } else if command.trim().is_empty() {
        return vec![shell, "-NoLogo".into()];
    } else {
        let trimmed = command.trim();
        let first = first_word(trimmed).unwrap_or(trimmed);
        let program = which(first).map_or(first.to_string(), |p| p.display().to_string());
        format!("& {}{}", ps_quote(&program), &trimmed[first.len()..])
    };
    vec![
        shell,
        "-NoLogo".into(),
        "-NoExit".into(),
        "-Command".into(),
        run,
    ]
}

/// PowerShell single-quoted string literal.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
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
/// On Windows also tries the PATHEXT extensions (`claude` -> `claude.exe`,
/// `pnpm` -> `pnpm.cmd`).
pub fn which(program: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) && Path::new(program).extension().is_none() {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    let candidates = |dir: &Path| -> Vec<PathBuf> {
        exts.iter()
            .map(|e| dir.join(format!("{program}{e}")))
            .collect()
    };
    if program.contains('/') || (cfg!(windows) && program.contains('\\')) {
        return candidates(Path::new(""))
            .into_iter()
            .find(|p| is_executable(p));
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .flat_map(|dir| candidates(&dir))
            .find(|p| is_executable(p))
    })
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
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
    fn quotes_for_powershell() {
        assert_eq!(
            ps_quote(r"C:\Users\O'Neil\claude.exe"),
            r"'C:\Users\O''Neil\claude.exe'"
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_sessions_run_in_powershell() {
        let hooks = Path::new(r"C:\desk\claude-hooks.json");
        let argv = launch_argv("claude", "claude", hooks);
        assert_eq!(argv[1..4], ["-NoLogo", "-NoExit", "-Command"]);
        assert!(argv[4].starts_with("& '"), "{argv:?}");
        assert!(
            argv[4].ends_with(r"--settings 'C:\desk\claude-hooks.json'"),
            "{argv:?}"
        );
        assert_eq!(launch_argv("shell", "", hooks)[1..], ["-NoLogo"]);
        let run = launch_argv("run", "nonexistent-tool-xyz dev", hooks);
        assert_eq!(run[4], "& 'nonexistent-tool-xyz' dev");
    }

    #[test]
    #[cfg(windows)]
    fn which_uses_pathext() {
        assert!(which("cmd").is_some_and(|p| p.extension().is_some()));
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
    #[cfg(unix)]
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
