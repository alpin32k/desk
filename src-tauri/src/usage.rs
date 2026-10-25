//! Plan usage limits per harness, read from local files only (no API calls,
//! no tokens):
//! - Claude Code passes `rate_limits` to the status line command. desk wraps
//!   the user's status line (see `STATUSLINE_SCRIPT`), saves that input to
//!   `state/<slug>.status` and then runs the user's own command unchanged.
//! - Codex writes `rate_limits` into `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const STATUSLINE_SCRIPT: &str = r#"#!/bin/sh
# Written by desk. Saves Claude Code's status line input (it carries the plan's
# rate limits) for desk, then runs your own status line command with it.
input=$(cat)
if [ -n "$DESK_SLUG" ] && [ -n "$DESK_STATE_DIR" ]; then
  tmp="$DESK_STATE_DIR/.$DESK_SLUG.status.$$"
  printf '%s' "$input" > "$tmp" && mv -f "$tmp" "$DESK_STATE_DIR/$DESK_SLUG.status"
fi
if [ -n "$DESK_STATUSLINE" ]; then
  printf '%s' "$input" | sh -c "$DESK_STATUSLINE"
fi
exit 0
"#;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// "5h" or "7d".
    pub label: String,
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub agent: String,
    pub windows: Vec<Window>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn usage(agents: &[String], state_dir: &Path, home: &Path) -> Vec<Usage> {
    let now = now();
    agents
        .iter()
        .filter_map(|a| {
            let windows = match a.as_str() {
                "claude" => claude(state_dir, now),
                "codex" => codex(&home.join(".codex/sessions"), now),
                _ => None,
            }?;
            Some(Usage {
                agent: a.clone(),
                windows,
            })
        })
        .collect()
}

/// Newest status line snapshot that carries rate limits (they are account-wide).
fn claude(state_dir: &Path, now: u64) -> Option<Vec<Window>> {
    let mut files: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(state_dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "status"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.iter().find_map(|(_, p)| {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
        claude_windows(&v, now)
    })
}

pub fn claude_windows(status: &Value, now: u64) -> Option<Vec<Window>> {
    let rl = status.get("rate_limits")?;
    let w: Vec<Window> = [("five_hour", "5h"), ("seven_day", "7d")]
        .iter()
        .filter_map(|(key, label)| {
            let x = rl.get(key)?;
            window(
                label,
                x.get("used_percentage")?.as_f64()?,
                x.get("resets_at")?,
                now,
            )
        })
        .collect();
    (!w.is_empty()).then_some(w)
}

/// Newest rollout with a `rate_limits` event; only the last few files are read.
fn codex(sessions: &Path, now: u64) -> Option<Vec<Window>> {
    let mut files = Vec::new();
    collect_rollouts(sessions, 3, &mut files);
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.iter().take(10).find_map(|(_, p)| {
        let text = std::fs::read_to_string(p).ok()?;
        text.lines()
            .rev()
            .filter(|l| l.contains("\"rate_limits\""))
            .find_map(|l| {
                let v: Value = serde_json::from_str(l).ok()?;
                codex_windows(v.get("payload")?.get("rate_limits")?, now)
            })
    })
}

/// Walk YYYY/MM/DD newest first, collecting rollout files from at most a few days.
fn collect_rollouts(dir: &Path, depth: usize, out: &mut Vec<(SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    entries.reverse();
    for p in entries {
        if out.len() >= 10 {
            return;
        }
        if depth > 0 && p.is_dir() {
            collect_rollouts(&p, depth - 1, out);
        } else if p.extension().is_some_and(|x| x == "jsonl")
            && let Ok(t) = p.metadata().and_then(|m| m.modified())
        {
            out.push((t, p));
        }
    }
}

pub fn codex_windows(rl: &Value, now: u64) -> Option<Vec<Window>> {
    let w: Vec<Window> = ["primary", "secondary"]
        .iter()
        .filter_map(|key| {
            let x = rl.get(key)?;
            let minutes = x.get("window_minutes")?.as_u64()?;
            let label = if minutes <= 24 * 60 {
                format!("{}h", minutes / 60)
            } else {
                format!("{}d", minutes / (24 * 60))
            };
            window(
                &label,
                x.get("used_percent")?.as_f64()?,
                x.get("resets_at")?,
                now,
            )
        })
        .collect();
    (!w.is_empty()).then_some(w)
}

/// A window whose reset time has passed no longer says anything: skip it.
fn window(label: &str, used: f64, resets_at: &Value, now: u64) -> Option<Window> {
    let resets_at = resets_at
        .as_u64()
        .or_else(|| resets_at.as_f64().map(|f| f as u64))?;
    (resets_at > now).then(|| Window {
        label: label.to_string(),
        used_percent: used,
        resets_at,
    })
}

/// The user's own status line command from ~/.claude/settings.json. desk's
/// `--settings` replaces it, so the wrapper runs it. A project's
/// `.claude/settings*.json` is not followed: desk would run a command chosen
/// by whoever wrote that repository.
pub fn user_statusline(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(".claude/settings.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let sl = v.get("statusLine")?;
    (sl.get("type")?.as_str()? == "command")
        .then(|| sl.get("command")?.as_str().map(str::to_string))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_rate_limits_from_status_line_input() {
        let v = json!({"model": {}, "rate_limits": {
            "five_hour": {"used_percentage": 42.5, "resets_at": 2000},
            "seven_day": {"used_percentage": 18, "resets_at": 999}
        }});
        let w = claude_windows(&v, 1000).unwrap();
        assert_eq!(w.len(), 1, "expired 7d window dropped");
        assert_eq!(w[0].label, "5h");
        assert_eq!(w[0].used_percent, 42.5);
        assert!(claude_windows(&json!({"model": {}}), 0).is_none());
    }

    #[test]
    fn codex_rate_limits_from_rollout_event() {
        let rl = json!({"primary": {"used_percent": 23.0, "window_minutes": 300, "resets_at": 5000},
                        "secondary": {"used_percent": 4.0, "window_minutes": 10080, "resets_at": 9000}});
        let w = codex_windows(&rl, 1000).unwrap();
        assert_eq!(w[0].label, "5h");
        assert_eq!(w[1].label, "7d");
        assert_eq!(w[1].used_percent, 4.0);
    }

    #[test]
    fn reads_newest_codex_rollout_and_status_files() {
        let tmp = tempfile::tempdir().unwrap();
        let day = tmp.path().join(".codex/sessions/2026/10/25");
        std::fs::create_dir_all(&day).unwrap();
        let line = json!({"type": "event_msg", "payload": {"type": "token_count", "rate_limits":
            {"primary": {"used_percent": 7.0, "window_minutes": 300, "resets_at": u64::MAX / 2}}}});
        std::fs::write(
            day.join("rollout-a.jsonl"),
            format!("{{}}\n{line}\n{{\"x\":1}}\n"),
        )
        .unwrap();
        let state = tmp.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("p-1.status"),
            json!({"rate_limits": {"five_hour": {"used_percentage": 50, "resets_at": u64::MAX / 2}}}).to_string(),
        )
        .unwrap();
        let u = usage(
            &["claude".into(), "codex".into(), "shell".into()],
            &state,
            tmp.path(),
        );
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].windows[0].used_percent, 50.0);
        assert_eq!(u[1].windows[0].used_percent, 7.0);
    }

    /// `cargo test live_usage -- --ignored --nocapture` prints this machine's limits.
    #[test]
    #[ignore]
    fn live_usage() {
        let home = crate::env::home();
        let state = crate::env::support_dir().join("state");
        println!(
            "{:?}",
            usage(&["claude".into(), "codex".into()], &state, &home)
        );
    }

    #[test]
    fn follows_only_the_user_statusline() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        assert_eq!(user_statusline(&home), None);
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"statusLine":{"type":"command","command":"~/.claude/statusline.sh"}}"#,
        )
        .unwrap();
        assert_eq!(
            user_statusline(&home).as_deref(),
            Some("~/.claude/statusline.sh")
        );
    }
}
