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
# Right click goes to a program that uses the mouse (vim...), not to tmux's
# menus (split, swap, kill...): desk lays out its tiles itself. Options can
# turn them on: tmux's own bindings are then appended below.
bind -n MouseDown3Pane if -F -t = \"#{mouse_any_flag}\" { select-pane -t = ; send-keys -M }
unbind -n M-MouseDown3Pane
unbind -n MouseDown3Status
unbind -n M-MouseDown3Status
unbind -n MouseDown3StatusLeft
unbind -n M-MouseDown3StatusLeft
";

/// The `list-keys` lines that bind a right click (`bind-key -T root
/// MouseDown3Pane ...`), each one a command `source-file` takes back.
fn right_click_bindings(list_keys: &str) -> String {
    list_keys
        .lines()
        .filter(|l| l.split_whitespace().nth(3).is_some_and(|k| k.contains("MouseDown3")))
        .map(|l| format!("{l}\n"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub slug: String,
    pub path: String,
    /// Agent id (`claude`, `codex`, ...); sessions from before agents existed are `claude`.
    pub agent: String,
    /// 0 for the main session of a project+agent, 1.. for sub-sessions.
    pub instance: u32,
}

#[derive(Clone)]
pub struct Tmux {
    conf: PathBuf,
    socket: String,
    /// `tmux` from PATH, or desk's own copy (see `find`).
    bin: PathBuf,
}

/// The tmux to run: the one in PATH, else the one shipped inside desk (macOS
/// app, AppImage). That one runs from a copy in the data folder, so a tmux
/// server left running never runs from an app an update replaced or from an
/// AppImage that is no longer mounted. None: no tmux at all.
pub fn find(support_dir: &Path) -> Option<PathBuf> {
    let works = |bin: &Path| {
        Command::new(bin)
            .arg("-V")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if works(Path::new("tmux")) {
        return Some("tmux".into());
    }
    let bundled = std::env::current_exe().ok()?.with_file_name("tmux");
    let copy = install_copy(&bundled, support_dir)?;
    works(&copy).then_some(copy)
}

/// `bundled` as `<support_dir>/bin/tmux`, rewritten only when it differs.
fn install_copy(bundled: &Path, support_dir: &Path) -> Option<PathBuf> {
    let bytes = std::fs::read(bundled).ok()?;
    let copy = support_dir.join("bin").join("tmux");
    if std::fs::read(&copy).ok().as_deref() != Some(&bytes[..]) {
        std::fs::create_dir_all(copy.parent()?).ok()?;
        // Renamed into place: a running server keeps the file it started from.
        let new = copy.with_extension("new");
        std::fs::write(&new, &bytes).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)).ok()?;
        }
        std::fs::rename(&new, &copy).ok()?;
    }
    Some(copy)
}

impl Tmux {
    pub fn new(support_dir: &Path, bin: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(support_dir)?;
        let conf = support_dir.join("tmux.conf");
        std::fs::write(&conf, CONF)?;
        Ok(Self {
            conf,
            socket: SOCKET.to_string(),
            bin,
        })
    }

    /// Base command: `tmux -u -L desk -f <conf>`.
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.bin);
        c.arg("-u")
            .arg("-L")
            .arg(&self.socket)
            .arg("-f")
            .arg(&self.conf);
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
            "#{session_name}\t#{@desk_path}\t#{session_path}\t#{@desk_agent}\t#{@desk_instance}",
        ])
        .map(|s| parse_sessions(&s))
        .unwrap_or_default()
    }

    /// Write the config, with tmux's right-click menus when `menu` (Options),
    /// and give it to the running server: one from an older desk (sessions
    /// outlive it) keeps the config it started with.
    pub fn reload_config(&self, menu: bool) {
        let mut text = CONF.to_string();
        if menu {
            text += &self.default_menus();
        }
        if std::fs::write(&self.conf, text).is_err() {
            return;
        }
        let conf = self.conf.display().to_string();
        let _ = self.run(&["source-file", &conf]); // no server yet: it starts with it
    }

    /// tmux's own right-click bindings, from a throwaway server without a config.
    fn default_menus(&self) -> String {
        let out = Command::new(&self.bin)
            .args(["-f", "/dev/null", "-L", "desk-defaults", "list-keys", "-T", "root"])
            .output();
        out.map(|o| right_click_bindings(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    }

    /// Drop variables a server started by an older desk took from whatever
    /// started it (see `env::inherited_session_vars`): new sessions get the
    /// server's global environment.
    pub fn clean_global_env(&self) {
        let Ok(out) = self.run(&["show-environment", "-g"]) else {
            return; // no server yet: it starts from desk's cleaned environment
        };
        let names = out
            .lines()
            .filter_map(|l| l.split('=').next())
            .filter(|n| !n.starts_with('-'))
            .map(str::to_string);
        for name in crate::env::inherited_session_vars(names) {
            let _ = self.run(&["set-environment", "-g", "-u", &name]);
        }
    }

    pub fn has_session(&self, slug: &str) -> bool {
        self.run(&["has-session", "-t", &format!("={slug}")])
            .is_ok()
    }

    /// Start `script` (see `agents::launch_script`) in a new detached session
    /// and tag it with the project path, agent and sub-session number.
    pub fn new_session(
        &self,
        slug: &str,
        cwd: &Path,
        script: &str,
        env: &[(&str, String)],
        agent: &str,
        instance: u32,
    ) -> Result<(), String> {
        let shell = crate::env::user_shell();
        let cwd_s = cwd.display().to_string();
        let env: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let mut args: Vec<&str> = vec![
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
        ];
        for e in &env {
            args.extend(["-e", e]);
        }
        args.extend([shell.as_str(), "-lc", script]);
        self.run(&args)?;
        if let Err(e) = self.tag(slug, &cwd_s, agent, instance) {
            // An untagged session would later be taken for another one.
            let _ = self.kill_session(slug);
            return Err(e);
        }
        Ok(())
    }

    /// Record what the session was started for (`list_sessions` reads it back).
    fn tag(&self, slug: &str, path: &str, agent: &str, instance: u32) -> Result<(), String> {
        // `set-option -t` takes a pane target: `=name` alone is not a session there.
        let target = format!("={slug}:");
        let instance = instance.to_string();
        for (k, v) in [
            ("@desk_path", path),
            ("@desk_agent", agent),
            ("@desk_instance", instance.as_str()),
        ] {
            self.run(&["set-option", "-t", &target, k, v])?;
        }
        Ok(())
    }

    /// Tag sessions that lost their tags (desk 0.1 dev builds failed to set
    /// them): the agent and sub-session number are recovered from the name,
    /// older names without them are main Claude sessions.
    pub fn retag_untagged(&self, agents: &[String]) {
        let Ok(out) = self.run(&[
            "list-sessions",
            "-F",
            "#{session_name}\t#{@desk_path}\t#{session_path}",
        ]) else {
            return;
        };
        for line in out.lines() {
            let mut f = line.split('\t');
            let (Some(slug), Some(""), Some(path)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            let (agent, instance) =
                origin_of(slug, Path::new(path), agents).unwrap_or(("claude", 0));
            let _ = self.tag(slug, path, agent, instance);
        }
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
            let agent = f.next().unwrap_or_default();
            let instance = f.next().unwrap_or_default().parse().unwrap_or(0);
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
                agent: if agent.is_empty() { "claude" } else { agent }.to_string(),
                instance,
            })
        })
        .collect()
}

/// Which agent and sub-session a session name was made for, if any.
pub fn origin_of<'a>(slug: &str, path: &Path, agents: &'a [String]) -> Option<(&'a str, u32)> {
    agents.iter().find_map(|a| {
        (0..=64)
            .find(|&n| session_slug(path, a, n) == slug)
            .map(|n| (a.as_str(), n))
    })
}

/// Stable tmux-safe session name: sanitized folder name + 8 hex chars of a
/// hash of the full path (two `app` folders in different places do not clash;
/// `open_session` still checks the path, so a hash collision cannot mix them up).
pub fn slug(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let clean = sanitize(&name);
    let clean = if clean.is_empty() { "project" } else { &clean };
    format!(
        "{clean}-{:08x}",
        fnv1a(path.display().to_string().as_bytes()) as u32
    )
}

/// Session name for a project + agent + sub-session. The main Claude session
/// keeps the plain `slug(path)` so sessions from earlier versions still match.
pub fn session_slug(path: &Path, agent: &str, instance: u32) -> String {
    if agent == "claude" && instance == 0 {
        return slug(path);
    }
    let base = slug(path);
    let name = base.rsplit_once('-').map_or(base.as_str(), |(n, _)| n);
    let agent = sanitize(agent);
    let n = if instance > 0 {
        instance.to_string()
    } else {
        String::new()
    };
    let key = format!("{}\0{agent}\0{instance}", path.display());
    format!("{name}-{agent}{n}-{:08x}", fnv1a(key.as_bytes()) as u32)
}

fn sanitize(s: &str) -> String {
    let mut clean = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            clean.push(c);
        } else if !clean.ends_with('-') {
            clean.push('-');
        }
    }
    clean.trim_matches('-').to_string()
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
    fn right_click_bindings_are_picked_from_list_keys() {
        let keys = "bind-key -T root MouseDown1Pane select-pane -t = \\; send-keys -M
bind-key -T root MouseDown3Pane            if-shell -F -t = \"#{mouse_any_flag}\" { display-menu x }
bind-key -T root M-MouseDown3Status        display-menu -T y
bind-key -T root WheelUpPane               send-keys -M
";
        assert_eq!(
            right_click_bindings(keys),
            "bind-key -T root MouseDown3Pane            if-shell -F -t = \"#{mouse_any_flag}\" { display-menu x }
bind-key -T root M-MouseDown3Status        display-menu -T y
"
        );
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
    fn session_slugs_per_agent_and_instance() {
        let p = Path::new("/Users/k/mm-crm");
        assert_eq!(session_slug(p, "claude", 0), slug(p));
        let codex = session_slug(p, "codex", 0);
        let sub = session_slug(p, "claude", 1);
        assert!(codex.starts_with("mm-crm-codex-"), "{codex}");
        assert!(sub.starts_with("mm-crm-claude1-"), "{sub}");
        assert_ne!(codex, sub);
        assert_ne!(session_slug(p, "codex", 1), codex);
        assert!(codex.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn slug_of_weird_name() {
        assert!(slug(Path::new("/x/...")).starts_with("project-"));
    }

    #[test]
    fn parses_list_sessions() {
        let out = "mm-crm-1a2b\t/Users/k/mm-crm\t/Users/k/mm-crm\tcodex\t2\n\
                   old-0000\t\t/Users/k/old\n\n";
        let s = parse_sessions(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].path, "/Users/k/mm-crm");
        assert_eq!(s[0].agent, "codex");
        assert_eq!(s[0].instance, 2);
        assert_eq!(s[1].path, "/Users/k/old");
        assert_eq!(s[1].agent, "claude");
        assert_eq!(s[1].instance, 0);
    }

    #[test]
    fn quotes_for_shell() {
        assert_eq!(shell_quote("/a b/c'd"), r"'/a b/c'\''d'");
    }

    #[test]
    fn recovers_agent_and_instance_from_the_name() {
        let p = Path::new("/Users/x/projects/shop");
        let agents = vec!["claude".to_string(), "codex".to_string()];
        let codex = session_slug(p, "codex", 0);
        let sub = session_slug(p, "claude", 2);
        assert_eq!(origin_of(&codex, p, &agents), Some(("codex", 0)));
        assert_eq!(origin_of(&sub, p, &agents), Some(("claude", 2)));
        assert_eq!(origin_of("shop-08c5", p, &agents), None);
    }

    #[test]
    fn bundled_tmux_is_copied_once_and_replaced_when_it_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let bundled = tmp.path().join("tmux");
        std::fs::write(&bundled, "v1").unwrap();
        let support = tmp.path().join("support");
        let copy = install_copy(&bundled, &support).unwrap();
        assert_eq!(copy, support.join("bin/tmux"));
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "v1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&copy).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let first = std::fs::metadata(&copy).unwrap().modified().unwrap();
        install_copy(&bundled, &support).unwrap();
        assert_eq!(std::fs::metadata(&copy).unwrap().modified().unwrap(), first);
        std::fs::write(&bundled, "v2").unwrap();
        install_copy(&bundled, &support).unwrap();
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "v2");
        assert!(install_copy(&tmp.path().join("none"), &support).is_none());
    }

    /// Real tmux on a throwaway socket: a new session must come back tagged
    /// (tmux 3.6 rejects `set-option -t =name`, which left every session untagged).
    #[test]
    fn new_sessions_are_tagged_and_untagged_ones_repaired() {
        if Command::new("tmux").arg("-V").output().is_err() {
            eprintln!("tmux not installed; skipped");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let t = Tmux {
            socket: format!("desk-test-{}", std::process::id()),
            ..Tmux::new(tmp.path(), "tmux".into()).unwrap()
        };
        let dir = tmp.path().canonicalize().unwrap();
        let slug = session_slug(&dir, "codex", 1);
        t.new_session(&slug, &dir, "sleep 60", &[], "codex", 1)
            .unwrap();
        let want = SessionInfo {
            slug: slug.clone(),
            path: dir.display().to_string(),
            agent: "codex".into(),
            instance: 1,
        };
        assert_eq!(t.list_sessions(), vec![want.clone()]);

        for k in ["@desk_path", "@desk_agent", "@desk_instance"] {
            t.run(&["set-option", "-u", "-t", &format!("={slug}:"), k])
                .unwrap();
        }
        assert_eq!(
            t.list_sessions()[0].agent,
            "claude",
            "untagged reads as claude"
        );
        t.retag_untagged(&["claude".into(), "codex".into()]);
        assert_eq!(t.list_sessions(), vec![want]);

        // A server started by `pnpm tauri dev` keeps the script's variables.
        for (k, v) in [
            ("npm_command", "run-script"),
            ("COREPACK_ROOT", "/x"),
            ("KEEP_ME", "1"),
        ] {
            t.run(&["set-environment", "-g", k, v]).unwrap();
        }
        t.clean_global_env();
        let env = t.run(&["show-environment", "-g"]).unwrap();
        assert!(
            !env.contains("COREPACK_ROOT=") && !env.contains("npm_command="),
            "{env}"
        );
        assert!(env.contains("KEEP_ME=1"));

        // Right click: tmux's menu is gone, also from a server started before.
        let right_click = |t: &Tmux| t.run(&["list-keys", "-T", "root"]).unwrap();
        assert!(!right_click(&t).contains("display-menu"), "{}", right_click(&t));
        t.run(&["unbind", "-n", "MouseDown3Pane"]).unwrap();
        t.run(&["bind", "-n", "MouseDown3Pane", "display-menu", "x", "", ""]).unwrap();
        t.reload_config(false);
        assert!(!right_click(&t).contains("display-menu"), "{}", right_click(&t));
        // ...and comes back when turned on in Options, then goes again.
        t.reload_config(true);
        assert!(right_click(&t).contains("Horizontal Split"), "{}", right_click(&t));
        t.reload_config(false);
        assert!(!right_click(&t).contains("display-menu"), "{}", right_click(&t));
        let _ = t.run(&["kill-server"]);
    }
}
