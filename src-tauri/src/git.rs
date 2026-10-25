//! Current git branch read straight from `.git/HEAD` (no `git` process per poll).

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

#[cfg(test)]
mod tests {
    use super::*;

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
