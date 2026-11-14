//! What each Claude Code session cost in the current 5-hour window, from its
//! transcripts (`<config dir>/projects/<project>/<session>.jsonl`, with
//! subagents in `<session>/subagents/`), weighed with `pricing`. Read only;
//! each file is read on from where the last scan stopped.

use crate::pricing::{self, Tokens};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct FileState {
    /// Bytes read so far (up to the last whole line).
    offset: u64,
    /// `(message.id, requestId)` → its place in `messages`: Claude Code
    /// writes a message's usage on every block of it; the last one counts.
    seen: HashMap<(String, String), usize>,
    /// `(timestamp, cost)` of each message.
    messages: Vec<(u64, f64)>,
    /// The project folder the session ran in, by name.
    cwd: String,
}

#[derive(Default)]
pub struct Scanner {
    files: HashMap<PathBuf, FileState>,
}

impl Scanner {
    /// Cost and project name per session transcript (a subagent counts to
    /// its session's), from messages at or after `window_start`.
    pub fn scan(&mut self, projects_dir: &Path, window_start: u64) -> HashMap<String, (f64, String)> {
        let mut out: HashMap<String, (f64, String)> = HashMap::new();
        for (file, session) in transcripts(projects_dir) {
            let modified = std::fs::metadata(&file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            if modified.is_none_or(|m| m < window_start) {
                continue;
            }
            let state = self.files.entry(file.clone()).or_default();
            read_on(&file, state);
            let cost: f64 = state.messages.iter().filter(|(t, _)| *t >= window_start).map(|(_, c)| c).sum();
            if cost <= 0.0 {
                continue;
            }
            let entry = out.entry(session.display().to_string()).or_default();
            entry.0 += cost;
            if entry.1.is_empty() {
                entry.1 = state.cwd.clone();
            }
        }
        out
    }
}

/// Every transcript under `projects_dir` with the session transcript it
/// counts to: itself, or `<proj>/<session>.jsonl` for a subagent's.
fn transcripts(projects_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let dir = |p: &Path| std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).collect::<Vec<_>>();
    let jsonl = |p: &Path| p.extension().is_some_and(|x| x == "jsonl");
    let mut out = Vec::new();
    for project in dir(projects_dir).into_iter().filter(|p| p.is_dir()) {
        for entry in dir(&project) {
            if jsonl(&entry) {
                out.push((entry.clone(), entry));
            } else if entry.is_dir() {
                let session = entry.with_extension("jsonl");
                for sub in dir(&entry.join("subagents")).into_iter().filter(|p| jsonl(p)) {
                    out.push((sub, session.clone()));
                }
            }
        }
    }
    out
}

/// Reads the whole lines added since the last scan; a file shorter than
/// what was read is read again from the start.
fn read_on(file: &Path, state: &mut FileState) {
    let Ok(mut f) = std::fs::File::open(file) else { return };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len < state.offset {
        *state = FileState::default();
    }
    let mut bytes = Vec::new();
    if f.seek(SeekFrom::Start(state.offset)).is_err() || f.read_to_end(&mut bytes).is_err() {
        return;
    }
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return };
    for line in bytes[..end].split(|b| *b == b'\n') {
        if let Ok(v) = serde_json::from_slice::<Value>(line) {
            take(&v, state);
        }
    }
    state.offset += end as u64 + 1;
}

/// One transcript line: the session's folder, and an assistant message's cost.
fn take(v: &Value, state: &mut FileState) {
    if let Some(name) = v.get("cwd").and_then(Value::as_str).and_then(|c| Path::new(c).file_name()) {
        state.cwd = name.to_string_lossy().into_owned();
    }
    if v.get("type").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let text = |x: Option<&Value>| x.and_then(Value::as_str).map(str::to_string);
    let message = v.get("message");
    let (Some(id), Some(request), Some(usage), Some(at)) = (
        text(message.and_then(|m| m.get("id"))),
        text(v.get("requestId")),
        message.and_then(|m| m.get("usage")),
        text(v.get("timestamp")).and_then(|t| crate::usage::iso_seconds(&t)),
    ) else {
        return;
    };
    let n = |x: Option<&Value>| x.and_then(Value::as_u64).unwrap_or(0);
    let split = usage.get("cache_creation");
    let (write_5m, write_1h) = match split {
        Some(c) if c.is_object() => (n(c.get("ephemeral_5m_input_tokens")), n(c.get("ephemeral_1h_input_tokens"))),
        _ => (n(usage.get("cache_creation_input_tokens")), 0),
    };
    let tokens = Tokens {
        input: n(usage.get("input_tokens")),
        write_5m,
        write_1h,
        read: n(usage.get("cache_read_input_tokens")),
        output: n(usage.get("output_tokens")),
    };
    let model = text(message.and_then(|m| m.get("model"))).unwrap_or_default();
    let fast = usage.get("speed").and_then(Value::as_str) == Some("fast");
    let cost = pricing::cost(&model, &tokens, fast);
    match state.seen.get(&(id.clone(), request.clone())) {
        Some(&i) => state.messages[i] = (at, cost),
        None => {
            state.seen.insert((id, request), state.messages.len());
            state.messages.push((at, cost));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    const START: &str = "2026-11-13T23:00:00Z";

    fn start() -> u64 {
        crate::usage::iso_seconds(START).unwrap()
    }

    fn entry(id: &str, at: &str, usage: serde_json::Value) -> String {
        json!({"type": "assistant", "requestId": format!("req-{id}"), "timestamp": at, "cwd": "/Users/x/Projects/crm",
               "message": {"id": id, "model": "claude-opus-5-5", "usage": usage}})
        .to_string()
            + "\n"
    }

    fn input(n: u64) -> serde_json::Value {
        json!({"input_tokens": n, "output_tokens": 0, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0})
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn only(r: &HashMap<String, (f64, String)>) -> f64 {
        assert_eq!(r.len(), 1, "{r:?}");
        r.values().next().unwrap().0
    }

    #[test]
    fn counts_a_message_once() {
        let tmp = tempfile::tempdir().unwrap();
        let e = entry("m1", "2026-11-14T00:00:00Z", input(1_000_000));
        write(&tmp.path().join("-p/s1.jsonl"), &format!("{{\"type\":\"user\"}}\n{e}{e}{e}"));
        let r = Scanner::default().scan(tmp.path(), start());
        assert!((only(&r) - 4.0).abs() < 1e-9);
        assert_eq!(r.values().next().unwrap().1, "crm");
    }

    #[test]
    fn ignores_before_the_window() {
        let tmp = tempfile::tempdir().unwrap();
        let text = entry("m1", "2026-11-13T22:00:00Z", input(1_000_000)) + &entry("m2", "2026-11-14T00:00:00Z", input(500_000));
        write(&tmp.path().join("-p/s1.jsonl"), &text);
        assert!((only(&Scanner::default().scan(tmp.path(), start())) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn subagents_count_to_their_session() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("-p/s1.jsonl"), &entry("m1", "2026-11-14T00:00:00Z", input(1_000_000)));
        write(&tmp.path().join("-p/s1/subagents/agent-a.jsonl"), &entry("m2", "2026-11-14T00:10:00Z", input(1_000_000)));
        let r = Scanner::default().scan(tmp.path(), start());
        assert!((only(&r) - 8.0).abs() < 1e-9);
        assert_eq!(r.keys().next().unwrap(), &tmp.path().join("-p/s1.jsonl").display().to_string());
    }

    #[test]
    fn splits_cache_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let usage = json!({"input_tokens": 0, "output_tokens": 0, "cache_read_input_tokens": 0,
            "cache_creation_input_tokens": 2_000_000,
            "cache_creation": {"ephemeral_5m_input_tokens": 1_000_000, "ephemeral_1h_input_tokens": 1_000_000}});
        write(&tmp.path().join("-p/s1.jsonl"), &entry("m1", "2026-11-14T00:00:00Z", usage));
        // Opus 5.5: 5 $ for 5-minute writes, 8 $ for 1-hour ones.
        assert!((only(&Scanner::default().scan(tmp.path(), start())) - 13.0).abs() < 1e-9);
    }

    #[test]
    fn scan_is_incremental() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("-p/s1.jsonl");
        write(&path, &entry("m1", "2026-11-14T00:00:00Z", input(1_000_000)));
        let mut s = Scanner::default();
        assert!((only(&s.scan(tmp.path(), start())) - 4.0).abs() < 1e-9);
        let len = std::fs::metadata(&path).unwrap().len();
        assert_eq!(s.files[&path].offset, len);
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(entry("m2", "2026-11-14T00:30:00Z", input(250_000)).as_bytes()).unwrap();
        f.write_all(b"{\"type\":\"assist").unwrap(); // a line still being written
        assert!((only(&s.scan(tmp.path(), start())) - 5.0).abs() < 1e-9);
        assert!(s.files[&path].offset < std::fs::metadata(&path).unwrap().len(), "the half line waits");
    }

    #[test]
    fn rereads_a_truncated_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("-p/s1.jsonl");
        let text = entry("m1", "2026-11-14T00:00:00Z", input(1_000_000)) + &entry("m2", "2026-11-14T00:01:00Z", input(1_000_000));
        write(&path, &text);
        let mut s = Scanner::default();
        assert!((only(&s.scan(tmp.path(), start())) - 8.0).abs() < 1e-9);
        write(&path, &entry("m3", "2026-11-14T00:02:00Z", input(250_000)));
        assert!((only(&s.scan(tmp.path(), start())) - 1.0).abs() < 1e-9);
    }
}
