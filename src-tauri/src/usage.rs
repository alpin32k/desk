//! Plan usage limits per harness, read from local files only (no API calls,
//! no tokens):
//! - Claude Code passes `rate_limits` to the status line command. desk wraps
//!   the user's status line (see `STATUSLINE_SCRIPT`), saves that input to
//!   `state/<slug>.status` and then runs the user's own command unchanged.
//! - Codex writes `rate_limits` into `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//!
//! A session only fetches Claude's limits when it works, so desk also asks
//! Anthropic itself every 5 minutes (`account`): the same usage request
//! `/usage` makes, with Claude Code's own login. No model, no tokens.

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

/// `desk --desk-statusline`: what STATUSLINE_SCRIPT does, for Windows. Saves
/// the input, then runs the user's status line with the shell Claude Code
/// would use (Git Bash when installed, else PowerShell) and passes its output on.
pub fn run_statusline() {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    if let (Ok(slug), Ok(dir)) = (std::env::var("DESK_SLUG"), std::env::var("DESK_STATE_DIR"))
        && !slug.is_empty()
    {
        let dir = Path::new(&dir);
        let tmp = dir.join(format!(".{slug}.status.{}", std::process::id()));
        if std::fs::write(&tmp, &input).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join(format!("{slug}.status")));
        }
    }
    let user = std::env::var("DESK_STATUSLINE").unwrap_or_default();
    if user.trim().is_empty() {
        return;
    }
    let mut cmd = if !cfg!(windows) {
        let mut c = Command::new("sh");
        c.args(["-c", &user]);
        c
    } else if let Some(bash) = git_bash_exe() {
        let mut c = Command::new(bash);
        c.args(["-c", &user]);
        c
    } else {
        let mut c = Command::new("powershell.exe");
        c.args(["-NoProfile", "-Command", &user]);
        c
    };
    if let Ok(mut child) = cmd.stdin(Stdio::piped()).stdout(Stdio::inherit()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }
        let _ = child.wait();
    }
}

pub fn git_bash_exe() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH") {
        return Some(PathBuf::from(p));
    }
    let program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    let bash = Path::new(&program_files).join(r"Git\bin\bash.exe");
    bash.is_file()
        .then_some(bash)
        .or_else(|| crate::agents::which("bash"))
}

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
                "claude" => claude(state_dir, home, now),
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

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const ACCOUNT_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Claude Code's login: the access token while it is valid. desk never
/// refreshes it (that would rotate the refresh token under Claude Code).
fn claude_token(home: &Path, now: u64) -> Option<String> {
    let json = if cfg!(target_os = "macos") {
        let out = std::process::Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
            .output()
            .ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
    .or_else(|| {
        let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map_or_else(|| home.join(".claude"), PathBuf::from);
        std::fs::read_to_string(dir.join(".credentials.json")).ok()
    })?;
    token_from(&serde_json::from_str(&json).ok()?, now)
}

pub fn token_from(creds: &Value, now: u64) -> Option<String> {
    let o = creds.get("claudeAiOauth")?;
    let expires_ms = o.get("expiresAt")?.as_u64()?;
    // A minute's margin: a token about to expire gets a 401 anyway.
    (expires_ms / 1000 > now + 60).then(|| o.get("accessToken")?.as_str().map(str::to_string))?
}

/// GET the usage with `curl` (on every system desk supports); the token goes
/// in on stdin, never on the command line where other programs could see it.
fn fetch_account(token: &str) -> Option<Value> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "--fail", "--max-time", "10", "-H", "@-", USAGE_URL])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let headers = format!("Authorization: Bearer {token}\nanthropic-beta: oauth-2025-04-20\nAccept: application/json\n");
    child.stdin.take()?.write_all(headers.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then(|| serde_json::from_slice(&out.stdout).ok())?
}

/// `five_hour` / `seven_day` from the usage answer.
pub fn account_windows(v: &Value, now: u64) -> Option<Vec<Window>> {
    let w: Vec<Window> = [("five_hour", "5h"), ("seven_day", "7d")]
        .iter()
        .filter_map(|(key, label)| {
            let x = v.get(key)?;
            let at = iso_seconds(x.get("resets_at")?.as_str()?)?;
            window(label, x.get("utilization")?.as_f64()?, &Value::from(at), now)
        })
        .collect();
    (!w.is_empty()).then_some(w)
}

/// "2026-10-26T10:09:59.519952+00:00" (UTC, as the API sends it) -> Unix seconds.
pub fn iso_seconds(s: &str) -> Option<u64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let hms = &time[..time.find(|c: char| c != ':' && !c.is_ascii_digit()).unwrap_or(time.len())];
    let mut t = hms.split(':').map(|x| x.parse::<i64>().ok());
    let (h, min, sec) = (t.next()??, t.next()??, t.next().flatten().unwrap_or(0));
    let offset = time.rfind(['+', '-']).filter(|&i| i >= 8).map_or(0, |i| {
        let sign = if &time[i..=i] == "-" { -1 } else { 1 };
        let mut o = time[i + 1..].split(':').map(|x| x.parse::<i64>().unwrap_or(0));
        sign * (o.next().unwrap_or(0) * 3600 + o.next().unwrap_or(0) * 60)
    });
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + min * 60 + sec - offset).ok()
}

/// The account's limits, asked for at most every 5 minutes (also after a failure).
fn account(home: &Path, now: u64) -> Option<Vec<Window>> {
    use std::sync::Mutex;
    use std::time::Instant;
    static LAST: Mutex<Option<(Instant, Option<Value>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref().is_none_or(|(at, _)| at.elapsed() >= ACCOUNT_EVERY) {
        let got = claude_token(home, now).and_then(|t| fetch_account(&t));
        *last = Some((Instant::now(), got));
    }
    account_windows(last.as_ref()?.1.as_ref()?, now)
}

/// The plan's limits from every session's status line input. Each Claude Code
/// session keeps the limits it last fetched, so an idle one shows old numbers:
/// per window, the freshest reading wins (see `freshest`).
fn claude(state_dir: &Path, home: &Path, now: u64) -> Option<Vec<Window>> {
    let readings = std::fs::read_dir(state_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "status"))
        .filter_map(|e| {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            claude_windows(&v, now)
        })
        .chain(account(home, now));
    let w = freshest(readings);
    (!w.is_empty()).then_some(w)
}

/// Per window label, the latest reading: within a window usage only grows, and
/// a reset moves `resets_at` on by hours. So the later window wins, and in the
/// same one (reset times differ by rounding only) the higher share.
/// 5h comes before 7d.
pub fn freshest(readings: impl IntoIterator<Item = Vec<Window>>) -> Vec<Window> {
    let mut out: Vec<Window> = Vec::new();
    for w in readings.into_iter().flatten() {
        match out.iter_mut().find(|o| o.label == w.label) {
            Some(o)
                if if w.resets_at.abs_diff(o.resets_at) < 3600 {
                    w.used_percent > o.used_percent
                } else {
                    w.resets_at > o.resets_at
                } =>
            {
                *o = w
            }
            Some(_) => {}
            None => out.push(w),
        }
    }
    out.sort_by_key(|w| if w.label == "5h" { 0 } else { 1 });
    out
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

    #[test]
    fn idle_sessions_old_limits_lose_to_fresh_ones() {
        let w = |label: &str, used: f64, resets_at: u64| Window { label: label.into(), used_percent: used, resets_at };
        let got = freshest([
            vec![w("7d", 75.0, 9000)],                // idle since this morning
            vec![w("5h", 13.0, 30_000), w("7d", 80.0, 9000)], // working now
            vec![w("5h", 40.0, 12_000)],              // before the last 5h reset
            vec![w("7d", 76.0, 9000)],
        ]);
        assert_eq!(got, vec![w("5h", 13.0, 30_000), w("7d", 80.0, 9000)]);
        assert!(freshest(Vec::<Vec<Window>>::new()).is_empty());
        // Same window, reset times rounded differently: the higher share wins.
        let got = freshest([vec![w("7d", 80.0, 9000)], vec![w("7d", 84.0, 8999)]]);
        assert_eq!(got, vec![w("7d", 84.0, 8999)]);
    }

    #[test]
    fn reads_the_account_usage_answer() {
        assert_eq!(iso_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_seconds("2026-10-26T10:09:59.519952+00:00"), Some(1_793_009_399));
        assert_eq!(iso_seconds("2026-10-26T12:09:59+02:00"), Some(1_793_009_399));
        assert_eq!(iso_seconds("nonsense"), None);
        let v = json!({
            "five_hour": {"utilization": 12.0, "resets_at": "2026-10-26T10:09:59.519952+00:00"},
            "seven_day": {"utilization": 84.0, "resets_at": "2026-10-28T02:59:59.519975+00:00"},
            "seven_day_opus": null
        });
        let w = account_windows(&v, 1_792_960_000).unwrap();
        assert_eq!((w[0].label.as_str(), w[0].used_percent, w[0].resets_at), ("5h", 12.0, 1_793_009_399));
        assert_eq!((w[1].label.as_str(), w[1].used_percent), ("7d", 84.0));
    }

    #[test]
    fn uses_only_a_valid_login() {
        let creds = |expires_ms: u64| json!({"claudeAiOauth": {"accessToken": "tok", "expiresAt": expires_ms}});
        assert_eq!(token_from(&creds(2_000_000), 1000).as_deref(), Some("tok"));
        assert_eq!(token_from(&creds(1_030_000), 1000), None, "about to expire");
        assert_eq!(token_from(&json!({}), 1000), None);
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
