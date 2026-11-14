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

use crate::accounts::ClaudeAccount;
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
    /// Anthropic's own grading of the window: "normal", "warning" or "critical";
    /// None when only the status line knew it.
    pub severity: Option<String>,
}

/// A weekly limit of one model (Fable), on top of the plan's weekly one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLimit {
    /// As Anthropic names it, "Fable".
    pub name: String,
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: u64,
    pub severity: Option<String>,
    /// Points of the plan's weekly limit this model used, and the most it may use.
    pub share_of_weekly: Option<f64>,
    pub allowance_of_weekly: Option<f64>,
}

/// Resets on offer (Claude Code's `/limit-reset`); desk only shows them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resets {
    /// The 5-hour window reset, once a week.
    pub five_hour: Option<FiveHourReset>,
    /// Saved resets that are not used up.
    pub saved: Vec<SavedReset>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiveHourReset {
    pub available: bool,
    /// Unix seconds.
    pub next_available_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedReset {
    pub label: String,
    pub left: u32,
    pub total: u32,
    /// Unix seconds: use it by then.
    pub ends_at: Option<u64>,
    /// Works only at a usage limit.
    pub requires_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub agent: String,
    /// The Claude account (accounts.rs) with two or more of them, else None.
    pub account: Option<String>,
    pub windows: Vec<Window>,
    /// Weekly limits of single models (Claude only).
    pub models: Vec<ModelLimit>,
    /// Usage credits take over at a limit; None when unknown.
    pub credits: Option<bool>,
    pub resets: Option<Resets>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn usage(agents: &[String], accounts: &[ClaudeAccount], state_dir: &Path, home: &Path) -> Vec<Usage> {
    let now = now();
    let accounts = crate::accounts::effective(accounts);
    let split = accounts.len() > 1;
    agents
        .iter()
        .flat_map(|a| match a.as_str() {
            "claude" => claude(state_dir, &accounts, home, now)
                .into_iter()
                .map(|c| Usage {
                    agent: a.clone(),
                    account: split.then_some(c.id),
                    windows: c.windows,
                    models: c.models,
                    credits: c.credits,
                    resets: c.resets,
                })
                .collect(),
            "codex" => codex(&home.join(".codex/sessions"), now)
                .map(|windows| Usage {
                    agent: a.clone(),
                    account: None,
                    windows,
                    models: Vec::new(),
                    credits: None,
                    resets: None,
                })
                .into_iter()
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const ACCOUNT_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);

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

/// `five_hour` / `seven_day` from the usage answer, graded by its `limits`.
pub fn account_windows(v: &Value, now: u64) -> Option<Vec<Window>> {
    let w: Vec<Window> = [("five_hour", "5h", "session"), ("seven_day", "7d", "weekly_all")]
        .iter()
        .filter_map(|(key, label, kind)| {
            let x = v.get(key)?;
            let at = iso_seconds(x.get("resets_at")?.as_str()?)?;
            let mut w = window(label, x.get("utilization")?.as_f64()?, &Value::from(at), now)?;
            w.severity = limits(v)
                .find(|l| l.get("kind").and_then(Value::as_str) == Some(kind))
                .and_then(severity);
            Some(w)
        })
        .collect();
    (!w.is_empty()).then_some(w)
}

/// The answer's `limits` array.
fn limits(v: &Value) -> impl Iterator<Item = &Value> {
    v.get("limits").and_then(Value::as_array).into_iter().flatten()
}

fn severity(limit: &Value) -> Option<String> {
    limit.get("severity").and_then(Value::as_str).map(str::to_string)
}

fn seconds(v: Option<&Value>) -> Option<u64> {
    iso_seconds(v?.as_str()?)
}

/// Weekly limits of single models: `limits` of kind `weekly_scoped`, with
/// how much of the plan's week each used (`weekly_scoped_shares`).
pub fn account_models(v: &Value, now: u64) -> Vec<ModelLimit> {
    let shares: Vec<&Value> = v
        .get("weekly_scoped_shares")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    limits(v)
        .enumerate()
        .filter(|(_, l)| l.get("kind").and_then(Value::as_str) == Some("weekly_scoped"))
        .filter_map(|(i, l)| {
            let model = l.get("scope")?.get("model")?;
            let name = model
                .get("display_name")
                .and_then(Value::as_str)
                .or_else(|| model.get("id").and_then(Value::as_str))?;
            let resets_at = seconds(l.get("resets_at")).filter(|&at| at > now)?;
            let share = shares
                .iter()
                .find(|s| s.get("limit_index").and_then(Value::as_u64) == Some(i as u64));
            Some(ModelLimit {
                name: name.to_string(),
                used_percent: l.get("percent")?.as_f64()?,
                resets_at,
                severity: severity(l),
                share_of_weekly: share.and_then(|s| s.get("used_percent_of_weekly")?.as_f64()),
                allowance_of_weekly: share.and_then(|s| s.get("allowance_percent_of_weekly")?.as_f64()),
            })
        })
        .collect()
}

/// Whether usage credits take over at a limit; None when the answer does not say.
pub fn account_credits(v: &Value) -> Option<bool> {
    let extra = v.get("extra_usage").and_then(|x| x.get("is_enabled")).and_then(Value::as_bool);
    let spend = v.get("spend").and_then(|x| x.get("enabled")).and_then(Value::as_bool);
    match (extra, spend) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(false) || b.unwrap_or(false)),
    }
}

/// Resets on offer: the weekly 5-hour reset (`juniper_tide`) and saved ones
/// (`cedar_ember`), in the shape Claude Code 2.1.325 reads them.
pub fn account_resets(v: &Value, now: u64) -> Option<Resets> {
    let eligible = |x: &&Value| x.get("eligible").and_then(Value::as_bool) == Some(true);
    let five_hour = v.get("juniper_tide").filter(eligible).map(|j| FiveHourReset {
        available: j.get("available").and_then(Value::as_bool).unwrap_or(false),
        next_available_at: seconds(j.get("next_available_at")),
    });
    let saved: Vec<SavedReset> = v
        .get("cedar_ember")
        .filter(eligible)
        .and_then(|c| c.get("grants"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|g| g.get("paused").and_then(Value::as_bool) != Some(true))
        .filter_map(|g| {
            let left = u32::try_from(g.get("resets_left")?.as_u64()?).ok()?;
            let ends_at = seconds(g.get("ends_at"));
            (left > 0 && ends_at.is_none_or(|at| at > now)).then(|| SavedReset {
                label: g.get("label").and_then(Value::as_str).unwrap_or_default().to_string(),
                left,
                total: g
                    .get("resets_total")
                    .and_then(Value::as_u64)
                    .and_then(|t| u32::try_from(t).ok())
                    .unwrap_or(left),
                ends_at,
                requires_limit: g.get("use_requires_limit").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect();
    (five_hour.is_some() || !saved.is_empty()).then_some(Resets { five_hour, saved })
}

/// Model limits a session's status line carried (`rate_limits.model_scoped`).
pub fn status_models(status: &Value, now: u64) -> Vec<ModelLimit> {
    status
        .get("rate_limits")
        .and_then(|r| r.get("model_scoped"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelLimit {
                name: m.get("display_name")?.as_str()?.to_string(),
                used_percent: m.get("utilization")?.as_f64()?,
                resets_at: seconds(m.get("resets_at")).filter(|&at| at > now)?,
                severity: None,
                share_of_weekly: None,
                allowance_of_weekly: None,
            })
        })
        .collect()
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

/// The account's usage answer, asked for at most every 5 minutes (also after a
/// failure), each account on its own.
fn account_answer(dir: &str, home: &Path, now: u64) -> Option<Value> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Instant;
    type Asked = HashMap<PathBuf, (Instant, Option<Value>)>;
    static LAST: Mutex<Option<Asked>> = Mutex::new(None);
    let key = crate::accounts::config_dir(dir, home);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let last = last.get_or_insert_with(HashMap::new);
    if last.get(&key).is_none_or(|(at, _)| at.elapsed() >= ACCOUNT_EVERY) {
        let got = crate::accounts::token(dir, home, now).and_then(|t| fetch_account(&t));
        last.insert(key.clone(), (Instant::now(), got));
    }
    last.get(&key)?.1.clone()
}

/// What desk knows of one Claude account's limits.
struct AccountLimits {
    id: String,
    windows: Vec<Window>,
    models: Vec<ModelLimit>,
    credits: Option<bool>,
    resets: Option<Resets>,
}

/// Per model name the latest reading, as `freshest` does for windows.
fn freshest_models(readings: impl IntoIterator<Item = ModelLimit>) -> Vec<ModelLimit> {
    let mut out: Vec<ModelLimit> = Vec::new();
    for m in readings {
        match out.iter_mut().find(|o| o.name == m.name) {
            Some(o)
                if if m.resets_at.abs_diff(o.resets_at) < 3600 {
                    m.used_percent > o.used_percent
                } else {
                    m.resets_at > o.resets_at
                } =>
            {
                *o = m
            }
            Some(_) => {}
            None => out.push(m),
        }
    }
    out
}

/// The plan's limits of each Claude account, from the status line input of
/// its sessions (by where their transcript is, see `accounts::owner_of`) and
/// from Anthropic. Each Claude Code session keeps the limits it last fetched,
/// so an idle one shows old numbers: per window, the freshest reading wins
/// (see `freshest`). Accounts with nothing known are left out.
fn claude(state_dir: &Path, accounts: &[ClaudeAccount], home: &Path, now: u64) -> Vec<AccountLimits> {
    let statuses: Vec<Value> = std::fs::read_dir(state_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "status"))
        .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    accounts
        .iter()
        .filter_map(|a| {
            let own: Vec<&Value> = statuses
                .iter()
                .filter(|v| {
                    let transcript = v.get("transcript_path").and_then(Value::as_str).unwrap_or_default();
                    crate::accounts::owner_of(accounts, Path::new(transcript), home).is_some_and(|o| o.id == a.id)
                })
                .collect();
            let answer = account_answer(&a.dir, home, now);
            let from_api = answer.as_ref().and_then(|v| account_windows(v, now)).unwrap_or_default();
            let readings = own
                .iter()
                .filter_map(|v| claude_windows(v, now))
                .chain((!from_api.is_empty()).then(|| from_api.clone()));
            let mut windows = freshest(readings);
            // Anthropic's grading holds for the same window, whichever reading won.
            for w in &mut windows {
                w.severity = from_api
                    .iter()
                    .find(|x| x.label == w.label && x.resets_at.abs_diff(w.resets_at) < 3600)
                    .and_then(|x| x.severity.clone());
            }
            let api_models = answer.as_ref().map(|v| account_models(v, now)).unwrap_or_default();
            let models = if api_models.is_empty() {
                freshest_models(own.iter().flat_map(|v| status_models(v, now)))
            } else {
                api_models
            };
            (!windows.is_empty()).then(|| AccountLimits {
                id: a.id.clone(),
                windows,
                models,
                credits: answer.as_ref().and_then(account_credits),
                resets: answer.as_ref().and_then(|v| account_resets(v, now)),
            })
        })
        .collect()
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
        severity: None,
    })
}

/// The user's own status line command from the account's `settings.json`
/// (`config_dir`: ~/.claude for the default account). desk's `--settings`
/// replaces it, so the wrapper runs it. A project's `.claude/settings*.json`
/// is not followed: desk would run a command chosen by whoever wrote that
/// repository.
pub fn user_statusline(config_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(config_dir.join("settings.json")).ok()?;
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
            &[],
            &state,
            tmp.path(),
        );
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].windows[0].used_percent, 50.0);
        assert_eq!(u[0].account, None, "one account: no label");
        assert_eq!(u[1].windows[0].used_percent, 7.0);
    }

    #[test]
    fn each_account_gets_the_limits_of_its_own_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let state = home.join("state");
        std::fs::create_dir_all(&state).unwrap();
        let later = u64::MAX / 2;
        let status = |transcript: &str, used: u32| {
            json!({"transcript_path": transcript,
                   "rate_limits": {"five_hour": {"used_percentage": used, "resets_at": later}}})
            .to_string()
        };
        let work = home.join(".claude-work/projects/-p/a.jsonl").display().to_string();
        let own = home.join(".claude-own/projects/-p/b.jsonl").display().to_string();
        std::fs::write(state.join("w.status"), status(&work, 60)).unwrap();
        std::fs::write(state.join("o.status"), status(&own, 20)).unwrap();
        // Folders of their own: no Keychain item, no login file, no request.
        let accounts = [
            ClaudeAccount { id: "work".into(), name: "Firma".into(), dir: "~/.claude-work".into() },
            ClaudeAccount { id: "own".into(), name: "Prywatne".into(), dir: "~/.claude-own".into() },
            ClaudeAccount { id: "idle".into(), name: "Inne".into(), dir: "~/.claude-idle".into() },
        ];
        let u = usage(&["claude".into()], &accounts, &state, home);
        let got: Vec<_> = u.iter().map(|u| (u.account.as_deref(), u.windows[0].used_percent)).collect();
        assert_eq!(got, vec![(Some("work"), 60.0), (Some("own"), 20.0)], "nothing known of idle");
    }

    #[test]
    fn idle_sessions_old_limits_lose_to_fresh_ones() {
        let w = |label: &str, used: f64, resets_at: u64| Window { label: label.into(), used_percent: used, resets_at, severity: None };
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

    /// The usage answer of 2026-11-14 (fields desk does not read left out).
    const ANSWER_2026_11_14: &str = r#"{
        "five_hour": {"utilization": 61.0, "resets_at": "2026-11-14T03:49:59.598735+00:00"},
        "seven_day": {"utilization": 54.0, "resets_at": "2026-11-20T03:59:59.598753+00:00"},
        "seven_day_opus": null,
        "juniper_tide": null,
        "cedar_ember": null,
        "extra_usage": {"is_enabled": false, "monthly_limit": null},
        "limits": [
            {"kind": "session", "group": "session", "percent": 61, "severity": "normal",
             "resets_at": "2026-11-14T03:49:59.598735+00:00", "scope": null, "is_active": false},
            {"kind": "weekly_all", "group": "weekly", "percent": 54, "severity": "normal",
             "resets_at": "2026-11-20T03:59:59.598753+00:00", "scope": null, "is_active": false},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 95, "severity": "critical",
             "resets_at": "2026-11-20T03:59:59.598916+00:00",
             "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}, "is_active": true}
        ],
        "spend": {"enabled": false, "can_purchase_credits": false},
        "weekly_scoped_shares": [
            {"limit_index": 2, "used_percent_of_weekly": 48, "allowance_percent_of_weekly": 50}
        ]
    }"#;

    #[test]
    fn reads_model_limits_severity_and_credits() {
        let v: Value = serde_json::from_str(ANSWER_2026_11_14).unwrap();
        let now = iso_seconds("2026-11-14T01:00:00Z").unwrap();
        let w = account_windows(&v, now).unwrap();
        assert_eq!(w[0].severity.as_deref(), Some("normal"));
        assert_eq!(w[1].severity.as_deref(), Some("normal"));
        assert_eq!(
            account_models(&v, now),
            vec![ModelLimit {
                name: "Fable".into(),
                used_percent: 95.0,
                resets_at: iso_seconds("2026-11-20T03:59:59.598916+00:00").unwrap(),
                severity: Some("critical".into()),
                share_of_weekly: Some(48.0),
                allowance_of_weekly: Some(50.0),
            }]
        );
        assert_eq!(account_credits(&v), Some(false));
        assert_eq!(account_resets(&v, now), None);
        assert!(account_models(&v, iso_seconds("2026-11-21T00:00:00Z").unwrap()).is_empty(), "expired");
    }

    #[test]
    fn reads_resets_when_offered() {
        let v = json!({
            "juniper_tide": {"eligible": true, "available": false, "next_available_at": "2026-11-20T04:00:00Z"},
            "cedar_ember": {"eligible": true, "grants": [
                {"id": "g1", "label": "Reset", "resets_total": 2, "resets_left": 1, "ends_at": "2026-11-27T00:00:00Z",
                 "paused": false, "use_requires_limit": true, "clears": ["five_hour"]},
                {"id": "g2", "label": "Used up", "resets_total": 1, "resets_left": 0, "paused": false, "use_requires_limit": true}
            ]}
        });
        let r = account_resets(&v, 0).unwrap();
        assert_eq!(
            r.five_hour,
            Some(FiveHourReset { available: false, next_available_at: iso_seconds("2026-11-20T04:00:00Z") })
        );
        assert_eq!(
            r.saved,
            vec![SavedReset {
                label: "Reset".into(),
                left: 1,
                total: 2,
                ends_at: iso_seconds("2026-11-27T00:00:00Z"),
                requires_limit: true,
            }]
        );
        assert_eq!(account_resets(&json!({"juniper_tide": {"eligible": false}}), 0), None);
        assert_eq!(account_credits(&json!({})), None);
    }

    #[test]
    fn model_limits_from_the_status_line() {
        let s = json!({"rate_limits": {"model_scoped": [
            {"display_name": "Fable", "utilization": 80.0, "resets_at": "2026-11-20T03:59:59Z"}]}});
        let m = status_models(&s, 0);
        assert_eq!((m[0].name.as_str(), m[0].used_percent), ("Fable", 80.0));
        assert_eq!(m[0].resets_at, iso_seconds("2026-11-20T03:59:59Z").unwrap());
        assert!(status_models(&json!({}), 0).is_empty());
    }

    #[test]
    fn claude_without_api_answer_has_no_models() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("p-1.status"),
            json!({"rate_limits": {"five_hour": {"used_percentage": 50, "resets_at": u64::MAX / 2}}}).to_string(),
        )
        .unwrap();
        // A folder of its own: no Keychain item, no login file, no request.
        let accounts = [ClaudeAccount { id: "x".into(), name: "X".into(), dir: "~/.claude-x".into() }];
        let u = usage(&["claude".into()], &accounts, &state, tmp.path());
        assert!(u[0].models.is_empty());
        assert_eq!((u[0].credits, u[0].resets.clone()), (None, None));
        assert_eq!(u[0].windows[0].severity, None);
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
        let layout = crate::layout::load(&crate::env::support_dir().join("layout.json"));
        println!(
            "{:?}",
            usage(&["claude".into(), "codex".into()], &layout.claude_accounts, &state, &home)
        );
    }

    #[test]
    fn follows_only_the_user_statusline() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        assert_eq!(user_statusline(&home.join(".claude")), None);
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"statusLine":{"type":"command","command":"~/.claude/statusline.sh"}}"#,
        )
        .unwrap();
        assert_eq!(
            user_statusline(&home.join(".claude")).as_deref(),
            Some("~/.claude/statusline.sh")
        );
    }
}
