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
    ("cursor", "cursor-agent"),
    ("shell", ""),
];

/// Official install commands of the built-in agents, (POSIX sh, PowerShell).
/// A tile that opens an agent which is not installed runs it first, in view.
fn installer(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "claude" => (
            "curl -fsSL https://claude.ai/install.sh | bash",
            "irm https://claude.ai/install.ps1 | iex",
        ),
        "codex" => ("npm install -g @openai/codex", "npm install -g @openai/codex"),
        "opencode" => (
            "curl -fsSL https://opencode.ai/install | bash",
            "npm install -g opencode-ai",
        ),
        "gemini" => (
            "npm install -g @google/gemini-cli",
            "npm install -g @google/gemini-cli",
        ),
        "aider" => (
            "curl -LsSf https://aider.chat/install.sh | sh",
            "irm https://aider.chat/install.ps1 | iex",
        ),
        "cursor" => (
            "curl -fsSL https://cursor.com/install | bash",
            "irm 'https://cursor.com/install?win32=true' | iex",
        ),
        _ => return None,
    })
}

/// Where the installers above put programs that may not be in PATH yet
/// (desk adds them to its own PATH too, see env.rs).
const INSTALL_DIRS: &str = "$HOME/.local/bin:$HOME/.opencode/bin";

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
    /// Not installed, but desk installs it when a tile opens it.
    pub installable: bool,
    pub builtin: bool,
}

/// Built-ins first, then custom ones (a custom id never shadows a built-in).
pub fn list(custom: &[CustomAgent]) -> Vec<AgentInfo> {
    let builtin = BUILTIN.iter().map(|(id, cmd)| {
        let installed = is_installed(id, cmd);
        AgentInfo {
            id: id.to_string(),
            command: cmd.to_string(),
            installed,
            installable: !installed && installer(id).is_some(),
            builtin: true,
        }
    });
    let custom = custom
        .iter()
        .filter(|c| !BUILTIN.iter().any(|(id, _)| *id == c.id))
        .map(|c| AgentInfo {
            id: c.id.clone(),
            command: c.command.clone(),
            installed: is_installed(&c.id, &c.command),
            installable: false,
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

fn is_installed(id: &str, command: &str) -> bool {
    let program = if id == "claude" {
        crate::env::claude_bin()
    } else {
        match first_word(command) {
            Some(w) => w,
            None => return true, // the login shell
        }
    };
    which(program).is_some()
}

/// The install command for `id` when it is a built-in that is not installed.
fn pending_install(id: &str, command: &str) -> Install {
    installer(id).filter(|_| !is_installed(id, command))
}

/// Shell script run in the tmux pane. After the agent exits the pane falls
/// back to a login shell so the tile stays usable.
pub fn launch_script(id: &str, command: &str, hooks_settings: &Path) -> String {
    script(id, command, hooks_settings, pending_install(id, command))
}

/// An install step: `installer`'s (POSIX sh, PowerShell) commands, or None.
type Install = Option<(&'static str, &'static str)>;

/// `launch_script`, with the install step given: tests do not depend on
/// which agents the machine has.
fn script(id: &str, command: &str, hooks_settings: &Path, install: Install) -> String {
    let shell = shell_quote(&crate::env::user_shell());
    let run = if id == "claude" {
        format!(
            "{} --settings {}",
            shell_quote(crate::env::claude_bin()),
            shell_quote(&hooks_settings.display().to_string())
        )
    } else if command.trim().is_empty() {
        return format!("exec {shell} -l");
    } else {
        resolve_command(command, which)
    };
    let run = match install {
        Some((sh, _)) => install_then_run(id, sh, &run),
        None => run,
    };
    format!("{run}; exec {shell} -l")
}

/// `/bin/sh` script: install the agent with its official installer, then run
/// it. A missing curl or npm, or a failed install, leaves the login shell.
fn install_then_run(id: &str, install: &str, run: &str) -> String {
    let need = first_word(install).unwrap_or("sh");
    let hint = if need == "npm" { " (Node.js: https://nodejs.org)" } else { "" };
    let say = |text: String| format!("echo {}", shell_quote(&text));
    let script = [
        say(format!("desk: {id} nie jest zainstalowany. Instaluję: {install}")),
        format!(
            "command -v {need} >/dev/null 2>&1 || {{ {}; exit 1; }}",
            say(format!("desk: do instalacji potrzebny jest {need}{hint}"))
        ),
        format!("{install} || {{ {}; exit 1; }}", say("desk: instalacja się nie udała".into())),
        format!("PATH=\"{INSTALL_DIRS}:$PATH\"; export PATH"),
        format!("exec {run}"),
    ]
    .join("\n");
    format!("/bin/sh -c {}", shell_quote(&script))
}

/// Program and arguments for a native (Windows) session: the agent runs in
/// PowerShell, which stays open after the agent exits, like the tmux pane.
/// Elsewhere (native sessions for testing) it is the login shell script.
pub fn launch_argv(id: &str, command: &str, hooks_settings: &Path) -> Vec<String> {
    argv(id, command, hooks_settings, pending_install(id, command))
}

fn argv(id: &str, command: &str, hooks_settings: &Path, install: Install) -> Vec<String> {
    let shell = crate::env::user_shell();
    if !cfg!(windows) {
        return vec![
            shell,
            "-lc".into(),
            script(id, command, hooks_settings, install),
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
    let run = match install {
        // Installers add their folder to the user's PATH in the registry.
        Some((_, ps)) => format!(
            "Write-Host {}; {ps}; $env:Path = [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + [Environment]::GetEnvironmentVariable('Path', 'Machine') + \";$HOME\\.local\\bin\"; {run}",
            ps_quote(&format!("desk: {id} nie jest zainstalowany. Instaluję: {ps}"))
        ),
        None => run,
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
        let claude = script("claude", "claude", hooks, None);
        assert!(
            claude.contains("--settings '/s/claude-hooks.json'"),
            "{claude}"
        );
        assert!(claude.ends_with(" -l"));
        assert!(script("shell", "", hooks, None).starts_with("exec "));
        let custom = script("mine", "definitely-not-installed-xyz --flag", hooks, None);
        assert!(
            custom.starts_with("definitely-not-installed-xyz --flag; exec "),
            "{custom}"
        );
        // Not installed: the installer runs first, then the same claude line.
        let install = script("claude", "claude", hooks, installer("claude"));
        assert!(install.starts_with("/bin/sh -c '"), "{install}");
        assert!(install.contains("https://claude.ai/install.sh"), "{install}");
        assert!(install.ends_with(" -l"), "{install}");
    }

    #[test]
    #[cfg(unix)]
    fn a_missing_agent_is_installed_in_view_then_run() {
        let (sh, _) = installer("codex").unwrap();
        let script = install_then_run("codex", sh, "codex --x");
        assert!(script.starts_with("/bin/sh -c '"), "{script}");
        assert!(script.contains("command -v npm"), "{script}");
        assert!(script.ends_with("exec codex --x'"), "{script}");
        // The script itself runs: a fake npm "installs" codex into ~/.local/bin.
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let npm = bin.join("npm");
        std::fs::write(
            &npm,
            "#!/bin/sh\nmkdir -p \"$HOME/.local/bin\"\nprintf '#!/bin/sh\\necho ran \"$@\"\\n' > \"$HOME/.local/bin/codex\"\nchmod +x \"$HOME/.local/bin/codex\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&npm, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .env("HOME", tmp.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&out.stdout);
        assert!(out.contains("Instaluję: npm install -g @openai/codex"), "{out}");
        assert!(out.ends_with("ran --x\n"), "{out}");
        // Without npm: a hint, and the agent does not start.
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .env("HOME", tmp.path().join("empty"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&out.stdout);
        assert!(out.contains("potrzebny jest npm (Node.js"), "{out}");
        assert!(!out.contains("ran"), "{out}");
        assert!(installer("shell").is_none() && installer("mine").is_none());
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
        let argv = super::argv("claude", "claude", hooks, None);
        assert_eq!(argv[1..4], ["-NoLogo", "-NoExit", "-Command"]);
        assert!(argv[4].starts_with("& '"), "{argv:?}");
        assert!(
            argv[4].ends_with(r"--settings 'C:\desk\claude-hooks.json'"),
            "{argv:?}"
        );
        assert_eq!(super::argv("shell", "", hooks, None)[1..], ["-NoLogo"]);
        let run = super::argv("run", "nonexistent-tool-xyz dev", hooks, None);
        assert_eq!(run[4], "& 'nonexistent-tool-xyz' dev");
        let install = super::argv("claude", "claude", hooks, installer("claude"));
        assert!(install[4].starts_with("Write-Host "), "{install:?}");
        assert!(install[4].contains("irm https://claude.ai/install.ps1 | iex"), "{install:?}");
        assert!(install[4].ends_with(r"--settings 'C:\desk\claude-hooks.json'"), "{install:?}");
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
