//! Each Claude session's status for the usage popover: model, cost, context
//! and prompt cache, from the status line input desk saves per session
//! (`state/<slug>.status`, see `usage::STATUSLINE_SCRIPT`).

use crate::accounts::ClaudeAccount;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// The tile's slug: the file's name.
    pub slug: String,
    pub session_id: String,
    pub transcript: String,
    /// The Claude account (accounts.rs) the transcript belongs to.
    pub account: String,
    /// As Claude Code shows it, "Opus 5.5".
    pub model: String,
    pub model_id: String,
    pub effort: Option<String>,
    pub fast: bool,
    /// Since the session started, at API prices (Claude Code's own sum).
    pub cost: f64,
    pub context_percent: Option<f64>,
    pub cache_warm: Option<bool>,
    /// Unix seconds.
    pub cache_expires_at: Option<u64>,
    /// Tokens the next message sends again when the cache is cold.
    pub recache_tokens: Option<u64>,
}

/// Every session with a status file; one without a session id (or not
/// readable) is left out.
pub fn read(state_dir: &Path, accounts: &[ClaudeAccount], home: &Path) -> Vec<SessionStatus> {
    let accounts = crate::accounts::effective(accounts);
    std::fs::read_dir(state_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "status"))
        .filter_map(|e| {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            let slug = e.path().file_stem()?.to_string_lossy().into_owned();
            parse(slug, &v, &accounts, home)
        })
        .collect()
}

fn parse(slug: String, v: &Value, accounts: &[ClaudeAccount], home: &Path) -> Option<SessionStatus> {
    let text = |x: Option<&Value>| x.and_then(Value::as_str).map(str::to_string);
    let session_id = text(v.get("session_id")).filter(|s| !s.is_empty())?;
    let transcript = text(v.get("transcript_path")).unwrap_or_default();
    let account = crate::accounts::owner_of(accounts, Path::new(&transcript), home)?.id.clone();
    let model = v.get("model");
    let model_id = text(model.and_then(|m| m.get("id"))).unwrap_or_default();
    let cache = v.get("prompt_cache");
    Some(SessionStatus {
        slug,
        session_id,
        transcript,
        account,
        model: text(model.and_then(|m| m.get("display_name"))).unwrap_or_else(|| model_id.clone()),
        model_id,
        effort: text(v.get("effort").and_then(|e| e.get("level"))),
        fast: v.get("fast_mode").and_then(Value::as_bool).unwrap_or(false),
        cost: v
            .get("cost")
            .and_then(|c| c.get("total_cost_usd"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        context_percent: v
            .get("context_window")
            .and_then(|c| c.get("used_percentage"))
            .and_then(Value::as_f64),
        cache_warm: cache.and_then(|c| c.get("warm")).and_then(Value::as_bool),
        cache_expires_at: cache.and_then(|c| c.get("expires_at")).and_then(Value::as_u64),
        recache_tokens: cache
            .and_then(|c| c.get("recache_tokens_if_cold"))
            .and_then(Value::as_u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status(transcript: &str) -> serde_json::Value {
        json!({
            "session_id": "723e7c62", "transcript_path": transcript,
            "model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
            "effort": {"level": "xhigh"}, "fast_mode": false,
            "cost": {"total_cost_usd": 0.44},
            "context_window": {"used_percentage": 7},
            "prompt_cache": {"warm": true, "expires_at": 1794621281u64, "recache_tokens_if_cold": 73985},
            "rate_limits": {"five_hour": {"used_percentage": 50, "resets_at": 1794628200u64}}
        })
    }

    #[test]
    fn reads_a_status_file() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("desk-dropdown-fcb86df5.status"), status("/t/a.jsonl").to_string()).unwrap();
        std::fs::write(state.join("desk-dropdown-fcb86df5.json"), "{}").unwrap();
        assert_eq!(
            read(&state, &[], tmp.path()),
            vec![SessionStatus {
                slug: "desk-dropdown-fcb86df5".into(),
                session_id: "723e7c62".into(),
                transcript: "/t/a.jsonl".into(),
                account: "main".into(),
                model: "Opus 5.5".into(),
                model_id: "claude-opus-5-5".into(),
                effort: Some("xhigh".into()),
                fast: false,
                cost: 0.44,
                context_percent: Some(7.0),
                cache_warm: Some(true),
                cache_expires_at: Some(1794621281),
                recache_tokens: Some(73985),
            }]
        );
    }

    #[test]
    fn skips_files_without_a_session() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.status"), json!({"model": {}}).to_string()).unwrap();
        std::fs::write(tmp.path().join("b.status"), "not json").unwrap();
        assert!(read(tmp.path(), &[], tmp.path()).is_empty());
    }

    #[test]
    fn sessions_belong_to_their_account() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let work = home.join(".claude-work/projects/-p/a.jsonl").display().to_string();
        let own = home.join(".claude-own/projects/-p/b.jsonl").display().to_string();
        std::fs::write(home.join("w.status"), status(&work).to_string()).unwrap();
        std::fs::write(home.join("o.status"), status(&own).to_string()).unwrap();
        let accounts = [
            ClaudeAccount { id: "work".into(), name: "Firma".into(), dir: "~/.claude-work".into() },
            ClaudeAccount { id: "own".into(), name: "Prywatne".into(), dir: "~/.claude-own".into() },
        ];
        let mut got: Vec<_> = read(home, &accounts, home).into_iter().map(|s| (s.slug, s.account)).collect();
        got.sort();
        assert_eq!(got, vec![("o".into(), "own".into()), ("w".into(), "work".into())]);
    }
}
