//! Creating a new project folder from the palette.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Folder names allowed for a new project: letters, digits, `.`, `_`, `-`, space.
pub fn validate_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.starts_with('.') {
        return Err("podaj nazwę projektu".into());
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ')))
    {
        return Err(format!("niedozwolony znak w nazwie: {c:?}"));
    }
    Ok(name)
}

/// `parent/name`, created empty, optionally as a git repo on `main`.
pub fn create(parent: &Path, name: &str, git: bool) -> Result<PathBuf, String> {
    let name = validate_name(name)?;
    if !parent.is_dir() {
        return Err(format!(
            "folder nadrzędny nie istnieje: {}",
            parent.display()
        ));
    }
    let dir = parent.join(name);
    if dir.exists() {
        return Err(format!("{} już istnieje", dir.display()));
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
