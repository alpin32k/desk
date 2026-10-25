//! Claude Code hooks injected with `claude --settings <file>` (merged with the
//! user's own hooks, see docs/hooks.md). Each hook runs `desk-hook.sh`, which
//! writes the event payload to `state/<DESK_SLUG>.json`; desk watches that dir.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SCRIPT: &str = r#"#!/bin/sh
# Written by desk. Records the Claude Code session state for the desk app.
# Does nothing for sessions not started by desk.
[ -n "$DESK_SLUG" ] && [ -n "$DESK_STATE_DIR" ] || { cat >/dev/null; exit 0; }
payload=$(cat)
[ -n "$payload" ] || payload=null
mkdir -p "$DESK_STATE_DIR" || exit 0
tmp="$DESK_STATE_DIR/.$DESK_SLUG.$$"
printf '{"event":"%s","ts":%s,"payload":%s}\n' "$1" "$(date +%s)" "$payload" > "$tmp" &&
  mv -f "$tmp" "$DESK_STATE_DIR/$DESK_SLUG.json"
exit 0
"#;

/// Events desk listens to.
pub const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PostToolUse",
    "PermissionRequest",
    "Notification",
    "Stop",
    "SessionEnd",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Working,
    Waiting,
    Idle,
    /// Claude exited; the pane is back at the shell.
    Exited,
}

pub fn state_for(event: &str, notification_type: Option<&str>) -> Option<State> {
    Some(match event {
        "UserPromptSubmit" | "PostToolUse" => State::Working,
        "PermissionRequest" => State::Waiting,
        "Notification" if notification_type == Some("idle_prompt") => State::Idle,
        "Notification" => State::Waiting,
        "SessionStart" | "Stop" => State::Idle,
        "SessionEnd" => State::Exited,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub slug: String,
    pub state: State,
    pub event: String,
    /// Unix seconds.
    pub ts: u64,
    pub message: Option<String>,
}

#[derive(Deserialize)]
struct Record {
    event: String,
    #[serde(default)]
    ts: u64,
    #[serde(default)]
    payload: serde_json::Value,
}

pub fn parse_state(slug: &str, text: &str) -> Option<SessionState> {
    let r: Record = serde_json::from_str(text.trim()).ok()?;
    let field = |k: &str| {
        r.payload
            .get(k)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let event = field("hook_event_name").unwrap_or(r.event);
    let state = state_for(&event, field("notification_type").as_deref())?;
    Some(SessionState {
        slug: slug.to_string(),
        state,
        event,
        ts: r.ts,
        message: field("message"),
    })
}

pub fn read_state(path: &Path) -> Option<SessionState> {
    let slug = state_slug(path)?;
    parse_state(&slug, &std::fs::read_to_string(path).ok()?)
}

/// `state/<slug>.json` -> slug; temp files (leading dot) are ignored.
pub fn state_slug(path: &Path) -> Option<String> {
    if path.extension()? != "json" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    (!stem.starts_with('.')).then(|| stem.to_string())
}

pub struct HookFiles {
    pub settings: PathBuf,
    pub state_dir: PathBuf,
}

/// (Re)write the hook script and the settings file passed to `--settings`.
pub fn install(support_dir: &Path) -> std::io::Result<HookFiles> {
    let state_dir = support_dir.join("state");
    std::fs::create_dir_all(&state_dir)?;
    let script = support_dir.join("desk-hook.sh");
    let statusline = support_dir.join("desk-statusline.sh");
    for (path, text) in [
        (&script, SCRIPT),
        (&statusline, crate::usage::STATUSLINE_SCRIPT),
    ] {
        std::fs::write(path, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    let settings = support_dir.join("claude-hooks.json");
    std::fs::write(&settings, settings_json(&script, &statusline).to_string())?;
    Ok(HookFiles {
        settings,
        state_dir,
    })
}

/// Hooks for session state, plus a status line wrapper that records the plan's
/// rate limits and then runs the user's own status line (see usage.rs).
pub fn settings_json(script: &Path, statusline: &Path) -> serde_json::Value {
    let quoted = crate::tmux::shell_quote(&script.display().to_string());
    let hooks: serde_json::Map<String, serde_json::Value> = EVENTS
        .iter()
        .map(|e| {
            let entry = serde_json::json!([{
                "hooks": [{ "type": "command", "command": format!("{quoted} {e}"), "timeout": 5 }]
            }]);
            (e.to_string(), entry)
        })
        .collect();
    serde_json::json!({
        "hooks": hooks,
        "statusLine": {
            "type": "command",
            "command": crate::tmux::shell_quote(&statusline.display().to_string()),
            "refreshInterval": 10
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_events_to_states() {
        assert_eq!(state_for("UserPromptSubmit", None), Some(State::Working));
        assert_eq!(state_for("PostToolUse", None), Some(State::Working));
        assert_eq!(state_for("Stop", None), Some(State::Idle));
        assert_eq!(state_for("SessionStart", None), Some(State::Idle));
        assert_eq!(state_for("SessionEnd", None), Some(State::Exited));
        assert_eq!(state_for("PermissionRequest", None), Some(State::Waiting));
        assert_eq!(
            state_for("Notification", Some("permission_prompt")),
            Some(State::Waiting)
        );
        assert_eq!(state_for("Notification", None), Some(State::Waiting));
        assert_eq!(
            state_for("Notification", Some("idle_prompt")),
            Some(State::Idle)
        );
        assert_eq!(state_for("PreCompact", None), None);
    }

    #[test]
    fn parses_record_written_by_script() {
        let text = r#"{"event":"Notification","ts":1761390000,"payload":{"session_id":"s","cwd":"/p","hook_event_name":"Notification","notification_type":"permission_prompt","message":"Claude needs your permission to use Bash"}}"#;
        let s = parse_state("mm-crm-1a2b", text).unwrap();
        assert_eq!(s.state, State::Waiting);
        assert_eq!(s.ts, 1761390000);
        assert_eq!(
            s.message.as_deref(),
            Some("Claude needs your permission to use Bash")
        );
    }

    #[test]
    fn falls_back_to_event_argument_and_rejects_garbage() {
        let s = parse_state("x", r#"{"event":"Stop","ts":1,"payload":null}"#).unwrap();
        assert_eq!(s.state, State::Idle);
        assert!(parse_state("x", "").is_none());
        assert!(parse_state("x", r#"{"event":"Unknown","payload":{}}"#).is_none());
    }

    #[test]
    fn state_file_names() {
        assert_eq!(
            state_slug(Path::new("/s/mm-crm-1a2b.json")).as_deref(),
            Some("mm-crm-1a2b")
        );
        assert_eq!(state_slug(Path::new("/s/.mm-crm-1a2b.123")), None);
        assert_eq!(state_slug(Path::new("/s/.x.json")), None);
    }

    #[test]
    fn settings_quote_script_path_with_spaces() {
        let v = settings_json(
            Path::new("/Users/k/Library/Application Support/desk/desk-hook.sh"),
            Path::new("/s/desk-statusline.sh"),
        );
        assert_eq!(v["statusLine"]["command"], "'/s/desk-statusline.sh'");
        let cmd = v["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(
            cmd,
            "'/Users/k/Library/Application Support/desk/desk-hook.sh' Stop"
        );
        assert_eq!(v["hooks"].as_object().unwrap().len(), EVENTS.len());
    }

    #[test]
    fn script_writes_state_file_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let files = install(tmp.path()).unwrap();
        let script = tmp.path().join("desk-hook.sh");
        let run = |env: bool, payload: &str| {
            use std::io::Write;
            let mut c = std::process::Command::new(&script);
            c.arg("UserPromptSubmit")
                .stdin(std::process::Stdio::piped());
            c.env_remove("DESK_SLUG").env_remove("DESK_STATE_DIR");
            if env {
                c.env("DESK_SLUG", "proj-0001")
                    .env("DESK_STATE_DIR", &files.state_dir);
            }
            let mut child = c.spawn().unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(payload.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
        };
        let out = files.state_dir.join("proj-0001.json");
        run(false, r#"{"hook_event_name":"UserPromptSubmit"}"#);
        assert!(!out.exists(), "no state outside desk sessions");
        run(true, r#"{"hook_event_name":"UserPromptSubmit","cwd":"/p"}"#);
        let s = read_state(&out).unwrap();
        assert_eq!(s.state, State::Working);
        assert_eq!(s.slug, "proj-0001");
    }
}
