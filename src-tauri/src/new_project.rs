//! Creating a new project folder from the palette.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Folder names allowed for a new project: letters, digits, `.`, `_`, `-`, space.
pub fn validate_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.starts_with('.') {
        return Err(tr!("podaj nazwę projektu", "give the project a name").into());
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ')))
    {
        return Err(tr!(
            format!("niedozwolony znak w nazwie: {c:?}"),
            format!("character not allowed in the name: {c:?}")
        ));
    }
    Ok(name)
}

/// Where new projects go until one is created: the first existing folder from
/// a list of usual places (spelled as on disk), else the home folder.
pub fn default_parent(home: &Path) -> PathBuf {
    const USUAL: [&str; 9] = [
        "Projects",
        "Developer",
        "Code",
        "dev",
        "src",
        "repos",
        "workspace",
        "Documents/Projects",
        "Documents/GitHub",
    ];
    USUAL
        .iter()
        .find_map(|rel| on_disk(home, rel))
        .unwrap_or_else(|| home.to_path_buf())
}

/// `base/rel` matched case-insensitively one component at a time, so
/// "Projects" finds `~/projects` and returns it as spelled on disk.
fn on_disk(base: &Path, rel: &str) -> Option<PathBuf> {
    rel.split('/').try_fold(base.to_path_buf(), |dir, part| {
        std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .find(|e| {
                e.file_name().to_string_lossy().eq_ignore_ascii_case(part)
                    && e.path().is_dir()
            })
            .map(|e| dir.join(e.file_name()))
    })
}

/// `parent/name`, created empty, optionally as a git repo on `main`.
pub fn create(parent: &Path, name: &str, git: bool) -> Result<PathBuf, String> {
    let name = validate_name(name)?;
    if !parent.is_dir() {
        return Err(tr!(
            format!("folder nadrzędny nie istnieje: {}", parent.display()),
            format!("the parent folder does not exist: {}", parent.display())
        ));
    }
    let dir = parent.join(name);
    if dir.exists() {
        return Err(tr!(
            format!("{} już istnieje", dir.display()),
            format!("{} already exists", dir.display())
        ));
    }
    std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
    if git {
        let out = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&dir)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
    }
    dir.canonicalize().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        assert_eq!(validate_name("  my-app_2.0 "), Ok("my-app_2.0"));
        assert!(validate_name("").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name(".hidden").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a;rm").is_err());
        assert_eq!(validate_name("Nowy projekt"), Ok("Nowy projekt"));
    }

    #[test]
    fn default_parent_finds_a_usual_folder_as_spelled_on_disk() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(default_parent(home.path()), home.path());
        std::fs::create_dir_all(home.path().join("Documents/github")).unwrap();
        assert_eq!(default_parent(home.path()), home.path().join("Documents/github"));
        std::fs::create_dir(home.path().join("projects")).unwrap();
        assert_eq!(default_parent(home.path()), home.path().join("projects"));
        // A file with the name does not count.
        let other = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("Projects"), "").unwrap();
        assert_eq!(default_parent(other.path()), other.path());
    }

    #[test]
    fn creates_git_repo_and_refuses_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = create(tmp.path(), "app", true).unwrap();
        assert!(dir.join(".git").is_dir());
        assert_eq!(crate::git::branch(&dir).as_deref(), Some("main"));
        assert!(create(tmp.path(), "app", true).is_err());
        let plain = create(tmp.path(), "plain", false).unwrap();
        assert!(!plain.join(".git").exists());
        assert!(create(&tmp.path().join("missing"), "x", false).is_err());
    }
}
