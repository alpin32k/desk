//! Current git branch read straight from `.git/HEAD` (no `git` process per poll),
//! and switching branches from a tile (`git switch`, which never drops changes).

use std::path::{Path, PathBuf};

/// The git directory for a work tree: `.git` itself, or the target of a
/// `gitdir:` file in a linked worktree.
pub fn git_dir(work_tree: &Path) -> Option<PathBuf> {
    let dot_git = work_tree.join(".git");
    let meta = std::fs::symlink_metadata(&dot_git).ok()?;
    if meta.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    let target = Path::new(target);
    Some(if target.is_absolute() {
        target.to_path_buf()
    } else {
        work_tree.join(target)
    })
}

pub fn branch(work_tree: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir(work_tree)?.join("HEAD")).ok()?;
    parse_head(&head)
}

pub fn parse_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(r) = head.strip_prefix("ref:") {
        let r = r.trim();
        return Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_string());
    }
    (head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit())).then(|| head[..7].to_string())
}

/// Branches to switch to: local ones, most recent commit first, then remote
/// ones without a local branch (`git switch x` creates x tracking origin/x).
pub fn branches(work_tree: &Path) -> Result<Vec<String>, String> {
    let out = git(
        work_tree,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    Ok(parse_refs(&out))
}

pub fn parse_refs(out: &str) -> Vec<String> {
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for r in out.lines().map(str::trim) {
        if let Some(b) = r.strip_prefix("refs/heads/") {
            local.push(b.to_string());
        } else if let Some((_, b)) = r
            .strip_prefix("refs/remotes/")
            .and_then(|x| x.split_once('/'))
            && b != "HEAD"
        {
            remote.push(b.to_string());
        }
    }
    for b in remote {
        if !local.contains(&b) {
            local.push(b);
        }
    }
    local
}

/// `git switch <branch>`: git refuses (and says why) when local changes would
/// be overwritten, so nothing is lost. Only names from `branches` are accepted.
pub fn switch(work_tree: &Path, branch: &str) -> Result<(), String> {
    if branch.starts_with('-') || !branches(work_tree)?.iter().any(|b| b == branch) {
        return Err(format!("nie ma brancha {branch}"));
    }
    git(work_tree, &["switch", "--end-of-options", branch]).map(|_| ())
}

fn git(work_tree: &Path, args: &[&str]) -> Result<String, String> {
    // A repository's own config cannot start a program through fsmonitor;
    // its hooks still run, as with `git switch` in a terminal.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(work_tree)
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_local_then_remote_only_branches() {
        let out = "refs/heads/main\nrefs/heads/feat/x\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/main\nrefs/remotes/origin/fix/y\n";
        assert_eq!(parse_refs(out), vec!["main", "feat/x", "fix/y"]);
    }

    #[test]
    fn switches_without_losing_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let run = |args: &[&str]| git(d, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        std::fs::write(d.join("a.txt"), "1").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "1",
        ]);
        run(&["branch", "other"]);
        assert_eq!(branches(d).unwrap().len(), 2);
        switch(d, "other").unwrap();
        assert_eq!(branch(d).as_deref(), Some("other"));
        assert!(switch(d, "--orphan").is_err(), "only listed branches");
        run(&["config", "core.fsmonitor", "touch fsmonitor-ran"]);
        switch(d, "main").unwrap();
        switch(d, "other").unwrap();
        assert!(
            !d.join("fsmonitor-ran").exists(),
            "repo config ran a program"
        );

        // A change that the other branch would overwrite: git refuses.
        std::fs::write(d.join("a.txt"), "2").unwrap();
        run(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qam",
            "2",
        ]);
        std::fs::write(d.join("a.txt"), "dirty").unwrap();
        assert!(switch(d, "main").is_err());
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "dirty");
    }

    #[test]
    fn parses_branch_and_detached_head() {
        assert_eq!(
            parse_head("ref: refs/heads/etap-0\n").as_deref(),
            Some("etap-0")
        );
        assert_eq!(
            parse_head("ref: refs/heads/feat/x").as_deref(),
            Some("feat/x")
        );
        assert_eq!(
            parse_head("0123456789abcdef0123456789abcdef01234567\n").as_deref(),
            Some("0123456")
        );
        assert_eq!(parse_head("garbage"), None);
    }

    #[test]
    fn reads_repo_and_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            main.join(".git/worktrees/wt/HEAD"),
            "ref: refs/heads/feature\n",
        )
        .unwrap();
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/wt").display()),
        )
        .unwrap();

        assert_eq!(branch(&main).as_deref(), Some("main"));
        assert_eq!(branch(&wt).as_deref(), Some("feature"));
        assert_eq!(branch(tmp.path()), None);
    }
}
