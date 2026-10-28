//! Things to do inside a project: find its files (⌘F) and the commands it
//! can run, like a dev server (⌘U). Read from the project's own files only.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 20_000;
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    "__pycache__",
];

/// Files relative to the project: `git ls-files` (tracked and untracked,
/// without ignored ones) in a repository, otherwise a bounded walk.
pub fn files(root: &Path) -> Vec<String> {
    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output();
    if let Ok(out) = git
        && out.status.success()
    {
        let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(MAX_FILES)
            .map(str::to_string)
            .collect();
        v.dedup();
        return v;
    }
    let mut out = Vec::new();
    walk(root, root, 0, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            if depth < 8 && !SKIP_DIRS.contains(&name.as_str()) {
                walk(root, &path, depth + 1, out);
            }
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// `rel` inside `root` (no `..` or symlink escapes), as an absolute path.
pub fn inside(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let full = root
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("{rel}: {e}"))?;
    if full.starts_with(&root) && full.is_file() {
        Ok(full)
    } else {
        Err(tr!(
            format!("{rel} nie jest plikiem tego projektu"),
            format!("{rel} is not a file of this project")
        ))
    }
}

/// Open a project file in a code editor (VS Code, Cursor, Zed or Sublime
/// when installed), else in the system's default text editor. Never in the
/// file's default app, which for a script would run it.
pub fn open_in_editor(root: &Path, rel: &str) -> Result<(), String> {
    open_path_in_editor(&inside(root, rel)?)
}

/// `file` in a code editor or the system's text editor (never run).
pub fn open_path_in_editor(file: &Path) -> Result<(), String> {
    let file = file.to_path_buf();
    for editor in ["code", "cursor", "zed", "subl"] {
        if let Some(bin) = crate::agents::which(editor) {
            let mut cmd = std::process::Command::new(bin);
            if editor == "code" || editor == "cursor" {
                cmd.arg("--goto");
            }
            return cmd
                .arg(&file)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
    }
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-t");
        c
    } else if cfg!(windows) {
        std::process::Command::new("notepad")
    } else {
        // Not xdg-open: for a script that may mean running it.
        let editor = [
            "gnome-text-editor",
            "gedit",
            "kate",
            "mousepad",
            "xed",
            "pluma",
        ]
        .into_iter()
        .find_map(crate::agents::which)
        .ok_or(tr!(
            "nie znalazłem edytora tekstu (np. VS Code, gedit, kate)",
            "found no text editor (e.g. VS Code, gedit, kate)"
        ))?;
        std::process::Command::new(editor)
    };
    cmd.arg(&file)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Files whose text contains `query` (case-insensitive): `git grep` in a
/// repository, else the files of the bounded walk. At most 500.
pub fn search(root: &Path, query: &str) -> Vec<String> {
    const LIMIT: usize = 500;
    if query.trim().is_empty() {
        return Vec::new();
    }
    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "grep",
            "-l",
            "-I",
            "-i",
            "-F",
            "--untracked",
            "-e",
            query,
        ])
        .output();
    if let Ok(out) = git
        && (out.status.success() || out.status.code() == Some(1))
        && !String::from_utf8_lossy(&out.stderr).contains("not a git repository")
    {
        return String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(LIMIT)
            .map(str::to_string)
            .collect();
    }
    let needle = query.to_lowercase();
    files(root)
        .into_iter()
        .filter(|rel| {
            let p = root.join(rel);
            std::fs::metadata(&p).is_ok_and(|m| m.len() <= 1024 * 1024)
                && std::fs::read_to_string(&p).is_ok_and(|t| t.to_lowercase().contains(&needle))
        })
        .take(LIMIT)
        .collect()
}

/// Show a file or folder in Finder / Explorer / the file manager.
pub fn reveal(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !crate::preview::local_path(path) || !path.exists() {
        return Err(tr!(format!("nie ma {}", path.display()), format!("no {}", path.display())));
    }
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("explorer.exe");
        // explorer wants `/select,"path"` as one argument, quoted its own way.
        c.raw_arg(format!("/select,\"{}\"", path.display()));
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        // A folder (never a file: xdg-open would open that file's default app).
        let dir = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        let mut c = std::process::Command::new("xdg-open");
        c.arg(dir);
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunTarget {
    /// What it is, e.g. "package.json" or "Cargo.toml".
    pub source: String,
    /// The shell command, e.g. "pnpm dev".
    pub command: String,
}

/// Commands the project defines: package.json scripts (with its package
/// manager), Cargo, Make targets, Django, Docker Compose. Dev servers first.
pub fn run_targets(root: &Path) -> Vec<RunTarget> {
    let mut out = Vec::new();
    let target = |source: &str, command: String| RunTarget {
        source: source.to_string(),
        command,
    };
    if let Some(scripts) = read_json(&root.join("package.json"))
        .as_ref()
        .and_then(|v| v.get("scripts"))
        .and_then(Value::as_object)
    {
        let pm = package_manager(root);
        let mut names: Vec<&String> = scripts.keys().collect();
        names.sort_by_key(|n| (script_rank(n), n.to_string()));
        for n in names {
            // A script name is data: quoted unless plainly safe, so
            // `"dev; rm -rf ~"` stays one (odd) script name for the shell.
            let n = script_arg(n);
            let command = match pm {
                "npm" => format!("npm run {n}"),
                _ => format!("{pm} {n}"),
            };
            out.push(target("package.json", command));
        }
    }
    if root.join("Cargo.toml").is_file() {
        out.push(target("Cargo.toml", "cargo run".into()));
        out.push(target("Cargo.toml", "cargo test".into()));
    }
    if root.join("manage.py").is_file() {
        out.push(target("manage.py", "python manage.py runserver".into()));
    }
    for f in [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ] {
        if root.join(f).is_file() {
            out.push(target(f, "docker compose up".into()));
            break;
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("Makefile")) {
        for t in make_targets(&text) {
            out.push(target("Makefile", format!("make {t}")));
        }
    }
    out
}

fn script_arg(name: &str) -> String {
    let plain = !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ":_.-/@".contains(c));
    if plain {
        name.to_string()
    } else {
        crate::tmux::shell_quote(name)
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if root.join("yarn.lock").is_file() {
        "yarn"
    } else if root.join("bun.lockb").is_file() || root.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    }
}

fn script_rank(name: &str) -> u8 {
    match name {
        "dev" => 0,
        "start" => 1,
        "serve" | "preview" => 2,
        n if n.starts_with("dev:") => 3,
        _ => 4,
    }
}

/// Plain `name:` rule lines; skips special (`.PHONY`), pattern and variable ones.
pub fn make_targets(makefile: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in makefile.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if rest.starts_with('=')
            || name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        if !out.iter().any(|t| t == name) {
            out.push(name.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_scripts_with_the_projects_package_manager() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::write(
            d.join("package.json"),
            r#"{"scripts":{"build":"vite build","dev":"vite","lint":"eslint ."}}"#,
        )
        .unwrap();
        std::fs::write(d.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(d.join("Cargo.toml"), "").unwrap();
        let cmds: Vec<String> = run_targets(d).into_iter().map(|t| t.command).collect();
        assert_eq!(
            cmds,
            [
                "pnpm dev",
                "pnpm build",
                "pnpm lint",
                "cargo run",
                "cargo test"
            ]
        );
        std::fs::remove_file(d.join("pnpm-lock.yaml")).unwrap();
        assert_eq!(run_targets(d)[0].command, "npm run dev");

        std::fs::write(
            d.join("package.json"),
            r#"{"scripts":{"dev; touch pwned":"x","test:e2e":"y"}}"#,
        )
        .unwrap();
        let cmds: Vec<String> = run_targets(d).into_iter().map(|t| t.command).collect();
        assert!(
            cmds.contains(&"npm run 'dev; touch pwned'".to_string()),
            "{cmds:?}"
        );
        assert!(cmds.contains(&"npm run test:e2e".to_string()));
    }

    #[test]
    fn searches_file_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/a.ts"), "const Needle = 1;").unwrap();
        std::fs::write(d.join("b.md"), "nothing here").unwrap();
        // Not a repository: the walk.
        assert_eq!(search(d, "needle"), ["src/a.ts"]);
        assert!(search(d, "  ").is_empty());
        // A repository: git grep, untracked files included, options are data.
        std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(d)
            .status()
            .unwrap();
        assert_eq!(search(d, "NEEDLE"), ["src/a.ts"]);
        assert!(search(d, "--version").is_empty());
    }

    #[test]
    fn reads_make_targets() {
        let mk =
            ".PHONY: all\nall: build\nbuild:\n\tcc x.c\nCC := gcc\n%.o: %.c\ntest-unit: build\n";
        assert_eq!(make_targets(mk), ["all", "build", "test-unit"]);
    }

    #[test]
    fn lists_files_and_keeps_paths_inside() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::create_dir_all(d.join("node_modules/x")).unwrap();
        std::fs::create_dir_all(d.join(".hidden")).unwrap();
        std::fs::write(d.join("src/a.ts"), "").unwrap();
        std::fs::write(d.join("node_modules/x/i.js"), "").unwrap();
        std::fs::write(d.join(".hidden/s"), "").unwrap();
        assert_eq!(files(d), ["src/a.ts"]);
        assert!(inside(d, "src/a.ts").is_ok());
        assert!(inside(d, "../outside").is_err());
        assert!(inside(d, "src").is_err());
    }
}

#[cfg(test)]
mod live {
    /// `cargo test live_projects -- --ignored --nocapture`: this machine's projects.
    #[test]
    #[ignore]
    fn live_projects() {
        let home = crate::env::home();
        for p in [
            "desk",
            "Documents/projects/mm-crm",
            "Documents/projects/shopmm-frontend",
        ] {
            let root = home.join(p);
            let t = std::time::Instant::now();
            let files = super::files(&root);
            let cmds: Vec<String> = super::run_targets(&root)
                .into_iter()
                .map(|t| t.command)
                .collect();
            println!(
                "{p}: {} files in {:?}; run: {cmds:?}",
                files.len(),
                t.elapsed()
            );
        }
    }
}
