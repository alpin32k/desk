//! Project discovery: git repositories under the home directory plus projects
//! Claude Code has been used in (`~/.claude/projects`).
//!
//! Contract used by the rest of the app (implemented by the discovery task):
//!
//! ```ignore
//! #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
//! #[serde(rename_all = "camelCase")]
//! pub struct Project {
//!     pub path: String,               // absolute, canonical, no trailing slash
//!     pub name: String,               // last path component
//!     pub last_active_ms: u64,        // newest of: claude transcript mtime, .git/HEAD|index mtime; 0 if unknown
//!     pub is_git: bool,
//!     pub worktree_of: Option<String>,// main repo path when `path` is a linked git worktree
//!     pub claude_sessions: u32,       // number of *.jsonl transcripts for this cwd
//! }
//!
//! pub struct DiscoverOptions {
//!     pub home: PathBuf,
//!     pub claude_projects_dir: PathBuf, // normally ~/.claude/projects
//!     pub max_depth: usize,             // 3 = ~/a/b/c
//!     pub extra: Vec<PathBuf>,          // folders added by hand; always included if they exist
//! }
//!
//! pub fn discover(opts: &DiscoverOptions) -> Vec<Project>; // sorted by last_active_ms desc, then name
//! ```

use crate::claude_projects;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub path: String,
    pub name: String,
    pub last_active_ms: u64,
    pub is_git: bool,
    pub worktree_of: Option<String>,
    pub claude_sessions: u32,
}

pub struct DiscoverOptions {
    pub home: PathBuf,
    pub claude_projects_dir: PathBuf,
    pub max_depth: usize,
    pub extra: Vec<PathBuf>,
}

const SKIP: &[&str] = &[
    "node_modules",
    "Library",
    "Applications",
    "Movies",
    "Music",
    "Pictures",
    "Public",
    "target",
    "dist",
    "build",
    "vendor",
    "Parallels",
    "Virtual Machines.localized",
    "OrbStack",
];

fn git_dir(path: &Path) -> Option<PathBuf> {
    let dot_git = path.join(".git");
    let kind = fs::symlink_metadata(&dot_git).ok()?.file_type();
    if kind.is_dir() {
        return Some(dot_git);
    }
    if kind.is_file() {
        let contents = fs::read_to_string(&dot_git).ok()?;
        let target = contents.lines().next()?.strip_prefix("gitdir:")?.trim();
        return path.join(target).canonicalize().ok();
    }
    None
}

fn is_repo(path: &Path) -> bool {
    fs::symlink_metadata(path.join(".git"))
        .is_ok_and(|metadata| metadata.is_dir() || metadata.is_file())
}

fn scan_git(path: &Path, remaining: usize, found: &mut Vec<PathBuf>) {
    if remaining == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || SKIP.contains(&name.as_ref())
            || !entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            continue;
        }
        let path = entry.path();
        if is_repo(&path) {
            found.push(path);
        } else {
            scan_git(&path, remaining - 1, found);
        }
    }
}

fn excluded_claude(path: &Path, home: &Path) -> bool {
    path == home
        || [
            "/private/tmp",
            "/tmp",
            "/var/folders",
            "/private/var/folders",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || path
            .components()
            .any(|part| part.as_os_str() == "scratch-workspaces")
        || path
            .components()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|parts| parts[0].as_os_str() == ".claude" && parts[1].as_os_str() == "worktrees")
}

fn add_project(
    projects: &mut BTreeMap<PathBuf, Project>,
    path: &Path,
    activity: u64,
    sessions: u32,
) {
    let Ok(path) = path.canonicalize() else {
        return;
    };
    if !path.is_dir() {
        return;
    }
    let project = projects.entry(path.clone()).or_insert_with(|| {
        let git = git_dir(&path);
        let worktree_of = git.as_ref().and_then(|git| {
            let worktrees = git.parent()?;
            let dot_git = worktrees.parent()?;
            if worktrees.file_name()? != "worktrees" || dot_git.file_name()? != ".git" {
                return None;
            }
            Some(
                dot_git
                    .parent()?
                    .canonicalize()
                    .ok()?
                    .to_string_lossy()
                    .into_owned(),
            )
        });
        let last_active_ms = git.as_ref().map_or(0, |git| {
            ["HEAD", "index"]
                .iter()
                .filter_map(|name| fs::metadata(git.join(name)).ok())
                .map(|metadata| claude_projects::modified_ms(&metadata))
                .max()
                .unwrap_or(0)
        });
        Project {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.to_string_lossy().into_owned(),
            last_active_ms,
            is_git: is_repo(&path),
            worktree_of,
            claude_sessions: 0,
        }
    });
    project.last_active_ms = project.last_active_ms.max(activity);
    project.claude_sessions = project.claude_sessions.saturating_add(sessions);
}

pub fn discover(opts: &DiscoverOptions) -> Vec<Project> {
    let mut projects = BTreeMap::new();
    let mut repos = Vec::new();
    scan_git(&opts.home, opts.max_depth, &mut repos);
    for path in repos.iter().chain(&opts.extra) {
        add_project(&mut projects, path, 0, 0);
    }
    let home = opts
        .home
        .canonicalize()
        .unwrap_or_else(|_| opts.home.clone());
    for project in claude_projects::scan(&opts.claude_projects_dir) {
        if !excluded_claude(&project.cwd, &home) {
            add_project(
                &mut projects,
                &project.cwd,
                project.last_active_ms,
                project.sessions,
            );
        }
    }
    let mut projects: Vec<_> = projects.into_values().collect();
    projects.sort_by(|a, b| {
        b.last_active_ms
            .cmp(&a.last_active_ms)
            .then(a.name.cmp(&b.name))
    });
    projects
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    fn options(root: &Path, depth: usize) -> DiscoverOptions {
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        DiscoverOptions {
            home,
            claude_projects_dir: root.join("claude"),
            max_depth: depth,
            extra: vec![],
        }
    }

    fn repo(path: &Path) {
        fs::create_dir_all(path.join(".git")).unwrap();
    }

    fn session(root: &Path, name: &str, cwd: &Path, seconds: u64) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        fs::write(&path, serde_json::json!({"cwd": cwd}).to_string()).unwrap();
        // Write access: Windows refuses to set times through a read-only handle.
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn depth_skip_rules_and_repo_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let mut opts = options(temp.path(), 1);
        repo(&opts.home.join("one"));
        repo(&opts.home.join("one/nested"));
        repo(&opts.home.join("group/two"));
        repo(&opts.home.join("group/deeper/three"));
        for name in SKIP.iter().copied().chain([".hidden"]) {
            repo(&opts.home.join(name).join("ignored"));
        }
        assert_eq!(
            discover(&opts)
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["one"]
        );
        opts.max_depth = 2;
        assert_eq!(discover(&opts).len(), 2);
        opts.max_depth = 3;
        assert_eq!(discover(&opts).len(), 3);
        opts.max_depth = 0;
        assert!(discover(&opts).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn scan_never_descends_symlinks_or_symlink_git_markers() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let opts = options(temp.path(), 4);
        repo(&temp.path().join("outside"));
        symlink(temp.path().join("outside"), opts.home.join("linked")).unwrap();
        symlink(&opts.home, opts.home.join("loop")).unwrap();
        fs::create_dir(opts.home.join("fake")).unwrap();
        symlink(
            temp.path().join("outside/.git"),
            opts.home.join("fake/.git"),
        )
        .unwrap();
        assert!(discover(&opts).is_empty());
    }

    #[test]
    fn worktrees_from_git_and_claude_use_gitdir_activity() {
        // Keep Claude fixtures outside the OS temporary trees that discovery excludes.
        let temp = tempfile::tempdir_in(".").unwrap();
        let mut opts = options(temp.path(), 1);
        let main = temp.path().join("main");
        repo(&main);
        let main = main.canonicalize().unwrap();
        let gitdir = main.join(".git/worktrees/branch");
        fs::create_dir_all(&gitdir).unwrap();
        for (name, seconds) in [("HEAD", 10), ("index", 20)] {
            let file = File::create(gitdir.join(name)).unwrap();
            file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
                .unwrap();
        }
        let worktree = opts.home.join("branch");
        fs::create_dir(&worktree).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        let projects = discover(&opts);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].worktree_of.as_deref(), main.to_str());
        assert_eq!(projects[0].last_active_ms, 20_000);
        opts.max_depth = 0;
        session(
            &opts.claude_projects_dir,
            "-branch",
            &worktree.canonicalize().unwrap(),
            30,
        );
        let projects = discover(&opts);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].worktree_of.as_deref(), main.to_str());
        assert_eq!(projects[0].last_active_ms, 30_000);
        assert_eq!(projects[0].claude_sessions, 1);
        assert!(projects[0].is_git);
    }

    #[test]
    fn merges_sources_filters_claude_and_sorts_activity_then_name() {
        let temp = tempfile::tempdir_in(".").unwrap();
        let mut opts = options(temp.path(), 1);
        let a = opts.home.join("alpha");
        let b = opts.home.join("beta");
        let c = opts.home.join("recent");
        for path in [&a, &b, &c] {
            repo(path);
        }
        session(
            &opts.claude_projects_dir,
            "-a",
            &a.canonicalize().unwrap(),
            10,
        );
        session(
            &opts.claude_projects_dir,
            "-a-again",
            &a.canonicalize().unwrap().join("."),
            20,
        );
        session(
            &opts.claude_projects_dir,
            "-b",
            &b.canonicalize().unwrap(),
            20,
        );
        session(
            &opts.claude_projects_dir,
            "-c",
            &c.canonicalize().unwrap(),
            30,
        );
        let tmp = tempfile::tempdir().unwrap();
        for (name, path) in [
            ("-home", opts.home.clone()),
            ("-missing", opts.home.join("missing")),
            ("-tmp", tmp.path().to_owned()),
            ("-scratch", opts.home.join("scratch-workspaces/project")),
            ("-worktree", opts.home.join(".claude/worktrees/project")),
        ] {
            if name == "-scratch" || name == "-worktree" {
                fs::create_dir_all(&path).unwrap();
            }
            session(
                &opts.claude_projects_dir,
                name,
                &path.canonicalize().unwrap_or(path),
                40,
            );
        }
        opts.extra = vec![a.clone(), a.join("."), opts.home.join("missing")];
        let projects = discover(&opts);
        assert_eq!(
            projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["recent", "alpha", "beta"]
        );
        assert_eq!(projects[1].claude_sessions, 2);
        assert_eq!(projects[1].last_active_ms, 20_000);
        let json = serde_json::to_value(&projects[1]).unwrap();
        assert_eq!(json["claudeSessions"], 2);
        assert_eq!(json["isGit"], true);
        opts.extra.push(tmp.path().to_owned());
        assert_eq!(discover(&opts).len(), 4);
        for path in [
            "/private/tmp/x",
            "/tmp/x",
            "/var/folders/x",
            "/x/scratch-workspaces/y",
            "/x/.claude/worktrees/y",
        ] {
            assert!(excluded_claude(Path::new(path), &opts.home));
        }
        assert!(!excluded_claude(Path::new("/tmp-project"), &opts.home));
    }
}
