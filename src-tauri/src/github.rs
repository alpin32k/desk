//! GitHub repos the user can clone into a new project, through the `gh` CLI
//! (its login and its ssh/https choice).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    #[serde(alias = "full_name")]
    pub full_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub private: bool,
}

/// Repos the user owns, collaborates on or sees through an organization,
/// most recently pushed first.
pub fn repos() -> Result<Vec<Repo>, String> {
    let out = gh()
        .args([
            "api",
            "--paginate",
            "user/repos?per_page=100&affiliation=owner,collaborator,organization_member&sort=pushed",
            "--jq",
            ".[] | {full_name, description, private}",
        ])
        .output()
        .map_err(missing)?;
    if !out.status.success() {
        return Err(failure(&out.stderr));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}

fn parse(lines: &str) -> Vec<Repo> {
    lines
        .lines()
        .filter_map(|l| serde_json::from_str::<Repo>(l).ok())
        .filter(|r| valid_repo(&r.full_name))
        .collect()
}

/// `owner/name` from GitHub: never an option for gh or git.
fn valid_repo(full_name: &str) -> bool {
    let mut parts = full_name.split('/');
    let ok = |s: Option<&str>| {
        s.is_some_and(|s| {
            !s.is_empty()
                && !s.starts_with(['-', '.'])
                && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// Clones `repo` into `parent/name` and returns the new folder.
pub fn clone(parent: &Path, name: &str, repo: &str) -> Result<PathBuf, String> {
    let name = crate::new_project::validate_name(name)?;
    if !valid_repo(repo) {
        return Err(format!("niepoprawne repo: {repo}"));
    }
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
    let out = gh()
        .args(["repo", "clone", repo])
        .arg(&dir)
        .args(["--", "-q"])
        .output()
        .map_err(missing)?;
    if !out.status.success() {
        return Err(failure(&out.stderr));
    }
    dir.canonicalize().map_err(|e| e.to_string())
}

/// No terminal to type a password or passphrase into: fail instead of hanging.
fn gh() -> Command {
    let mut cmd = Command::new("gh");
    cmd.stdin(Stdio::null())
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

fn missing(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "brak GitHub CLI: zainstaluj gh (brew install gh) i zaloguj się: gh auth login".into()
    } else {
        format!("gh: {e}")
    }
}

fn failure(stderr: &[u8]) -> String {
    let err = String::from_utf8_lossy(stderr).trim().to_string();
    if err.contains("gh auth login") {
        "gh nie jest zalogowany: uruchom gh auth login".into()
    } else {
        err
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gh_lines_and_drops_odd_names() {
        let lines = concat!(
            r#"{"full_name":"acme/api","description":"API","private":true}"#,
            "\n",
            r#"{"full_name":"me/notes","description":null,"private":false}"#,
            "\n",
            r#"{"full_name":"-x/--upload-pack=evil","description":null,"private":false}"#,
            "\nnot json\n",
        );
        assert_eq!(
            parse(lines),
            vec![
                Repo {
                    full_name: "acme/api".into(),
                    description: Some("API".into()),
                    private: true,
                },
                Repo {
                    full_name: "me/notes".into(),
                    description: None,
                    private: false,
                },
            ]
        );
    }

    #[test]
    fn accepts_only_owner_slash_name() {
        assert!(valid_repo("knehrybecki/desk"));
        assert!(valid_repo("a-b/c.d_e"));
        assert!(!valid_repo("desk"));
        assert!(!valid_repo("a/b/c"));
        assert!(!valid_repo("-a/b"));
        assert!(!valid_repo("a/.."));
        assert!(!valid_repo("a/b c"));
    }

    #[test]
    fn clone_refuses_bad_input_before_running_gh() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("taken")).unwrap();
        let taken = clone(tmp.path(), "taken", "a/b").unwrap_err();
        assert!(taken.contains("już istnieje"), "{taken}");
        assert!(clone(tmp.path(), "x", "--help").is_err());
        assert!(clone(tmp.path(), "a/b", "a/b").is_err());
        assert!(clone(&tmp.path().join("missing"), "x", "a/b").is_err());
    }
}
