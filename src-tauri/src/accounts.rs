//! Claude accounts: each one is a Claude Code config folder
//! (`CLAUDE_CONFIG_DIR`) with its own login, conversations and plan limits.
//! The account without a folder is Claude Code's default one (~/.claude, or
//! the `CLAUDE_CONFIG_DIR` desk itself was started with). With fewer than two
//! accounts in Options desk behaves as before: every session uses the default.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAccount {
    pub id: String,
    /// What tiles and the top bar show ("Firma", "Prywatne").
    pub name: String,
    /// Config folder as typed in Options (`~/.claude-prywatne`); "" = the default.
    pub dir: String,
}

/// An account as Options shows it: where it lives and who is logged in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    /// The config folder, `~` expanded.
    pub dir: String,
    /// The email Claude Code is logged in with; None before `/login`.
    pub email: Option<String>,
}

/// `CLAUDE_CONFIG_DIR` a session of this account gets; None for the default
/// account (the session keeps whatever Claude Code would use on its own).
pub fn session_env(dir: &str, home: &Path) -> Option<String> {
    let dir = dir.trim();
    (!dir.is_empty()).then(|| expand(dir, home).display().to_string())
}

/// `CLAUDE_CONFIG_DIR` as Claude Code sees it in a session of this account:
/// for the default account, the one desk inherited, if any.
fn env_seen(dir: &str, home: &Path) -> Option<String> {
    session_env(dir, home).or_else(|| {
        std::env::var("CLAUDE_CONFIG_DIR")
            .ok()
            .filter(|d| !d.trim().is_empty())
    })
}

/// The folder Claude Code keeps this account in (settings, projects, logins).
pub fn config_dir(dir: &str, home: &Path) -> PathBuf {
    env_seen(dir, home).map_or_else(|| home.join(".claude"), PathBuf::from)
}

/// `~` and `~/x` from the user's home; no trailing slash, so the folder hashes
/// the same way Claude Code hashes it (see `keychain_service`).
fn expand(dir: &str, home: &Path) -> PathBuf {
    let path = if dir == "~" {
        home.to_path_buf()
    } else if let Some(rest) = dir.strip_prefix("~/").or_else(|| dir.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(dir)
    };
    let s = path.display().to_string();
    let trimmed = s.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() { path } else { PathBuf::from(trimmed) }
}

/// macOS Keychain item holding the account's login. Claude Code names it
/// "Claude Code-credentials", plus the first 8 hex characters of the SHA-256
/// of `CLAUDE_CONFIG_DIR` when that is set.
pub fn keychain_service(env_dir: Option<&str>) -> String {
    match env_dir {
        None => "Claude Code-credentials".into(),
        Some(d) => {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(d.as_bytes());
            let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
            format!("Claude Code-credentials-{hex}")
        }
    }
}

/// Claude Code's login for this account: the access token while it is valid.
/// desk never refreshes it (that would rotate the refresh token under Claude Code).
pub fn token(dir: &str, home: &Path, now: u64) -> Option<String> {
    let env = env_seen(dir, home);
    let json = if cfg!(target_os = "macos") {
        let out = std::process::Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", &keychain_service(env.as_deref()), "-w"])
            .output()
            .ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
    .or_else(|| std::fs::read_to_string(config_dir(dir, home).join(".credentials.json")).ok())?;
    crate::usage::token_from(&serde_json::from_str(&json).ok()?, now)
}

/// Claude Code's global config (`.claude.json`): in `CLAUDE_CONFIG_DIR` when
/// that is set, else in the home folder.
fn global_config(dir: &str, home: &Path) -> PathBuf {
    env_seen(dir, home)
        .map_or_else(|| home.to_path_buf(), PathBuf::from)
        .join(".claude.json")
}

/// The email the account is logged in with, from Claude Code's own config.
pub fn email(dir: &str, home: &Path) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(global_config(dir, home)).ok()?).ok()?;
    v.get("oauthAccount")?
        .get("emailAddress")?
        .as_str()
        .filter(|e| !e.is_empty())
        .map(str::to_string)
}

pub fn info(accounts: &[ClaudeAccount], home: &Path) -> Vec<AccountInfo> {
    accounts
        .iter()
        .map(|a| AccountInfo {
            id: a.id.clone(),
            dir: config_dir(&a.dir, home).display().to_string(),
            email: email(&a.dir, home),
        })
        .collect()
}

/// The accounts sessions can use: the ones from Options, or just the default
/// (its id is the one Options gives it once a second account is added).
pub fn effective(accounts: &[ClaudeAccount]) -> Vec<ClaudeAccount> {
    if accounts.is_empty() {
        vec![ClaudeAccount {
            id: "main".into(),
            name: "claude".into(),
            dir: String::new(),
        }]
    } else {
        accounts.to_vec()
    }
}

/// The account `id`, or the first one (the default) for an unknown id.
pub fn find<'a>(accounts: &'a [ClaudeAccount], id: Option<&str>) -> Option<&'a ClaudeAccount> {
    id.and_then(|id| accounts.iter().find(|a| a.id == id))
        .or_else(|| accounts.first())
}

/// Which account a Claude transcript belongs to: the one whose folder holds
/// it; one in no account's folder goes to the first (default) account.
pub fn owner_of<'a>(accounts: &'a [ClaudeAccount], transcript: &Path, home: &Path) -> Option<&'a ClaudeAccount> {
    accounts
        .iter()
        .find(|a| transcript.starts_with(config_dir(&a.dir, home)))
        .or_else(|| accounts.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, dir: &str) -> ClaudeAccount {
        ClaudeAccount {
            id: id.into(),
            name: id.into(),
            dir: dir.into(),
        }
    }

    #[test]
    fn keychain_name_matches_claude_codes() {
        assert_eq!(keychain_service(None), "Claude Code-credentials");
        // `printf %s /Users/k/.claude-work | shasum -a 256` starts with 0df09abf.
        assert_eq!(
            keychain_service(Some("/Users/k/.claude-work")),
            "Claude Code-credentials-0df09abf"
        );
    }

    #[test]
    fn folders_expand_the_home_and_lose_the_trailing_slash() {
        let home = Path::new("/Users/k");
        assert_eq!(session_env("", home), None);
        assert_eq!(session_env("  ", home), None);
        assert_eq!(session_env("~/.claude-work/", home).as_deref(), Some("/Users/k/.claude-work"));
        assert_eq!(session_env("/opt/claude", home).as_deref(), Some("/opt/claude"));
        assert_eq!(config_dir("~/.claude-work", home), Path::new("/Users/k/.claude-work"));
    }

    #[test]
    fn email_comes_from_the_accounts_own_config() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let work = home.join(".claude-work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(
            work.join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"jan@firma.pl"}}"#,
        )
        .unwrap();
        assert_eq!(email("~/.claude-work", home).as_deref(), Some("jan@firma.pl"));
        // Not logged in yet: no config, or no oauthAccount in it.
        assert_eq!(email("~/.claude-other", home), None);
        std::fs::write(work.join(".claude.json"), "{}").unwrap();
        assert_eq!(email("~/.claude-work", home), None);
    }

    #[test]
    fn a_transcript_belongs_to_the_account_whose_folder_holds_it() {
        let home = Path::new("/Users/k");
        let list = [account("main", ""), account("work", "~/.claude-work")];
        let work = Path::new("/Users/k/.claude-work/projects/-x/a.jsonl");
        let main = Path::new("/Users/k/.claude/projects/-x/a.jsonl");
        assert_eq!(owner_of(&list, work, home).map(|a| a.id.as_str()), Some("work"));
        // `.claude-work` is not inside `.claude`: whole components are compared.
        assert_eq!(owner_of(&list, main, home).map(|a| a.id.as_str()), Some("main"));
        assert_eq!(owner_of(&list, Path::new("/elsewhere/a.jsonl"), home).map(|a| a.id.as_str()), Some("main"));
    }

    #[test]
    fn unknown_ids_fall_back_to_the_default_account() {
        let list = [account("main", ""), account("work", "~/.w")];
        assert_eq!(find(&list, Some("work")).map(|a| a.id.as_str()), Some("work"));
        assert_eq!(find(&list, Some("gone")).map(|a| a.id.as_str()), Some("main"));
        assert_eq!(find(&list, None).map(|a| a.id.as_str()), Some("main"));
        assert_eq!(effective(&[]).len(), 1);
        assert_eq!(effective(&[]).first().map(|a| a.dir.as_str()), Some(""));
    }
}
