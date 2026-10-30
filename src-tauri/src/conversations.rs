//! ⌘S: a project's recent Claude Code conversations, to pick one up again
//! (`claude --resume <id>`), e.g. after its tile was closed or desk lost it.
//! They are the `*.jsonl` transcripts in `~/.claude/projects/<folder>/`, where
//! `<folder>` is the project's path with every other character a dash. Only
//! conversations someone typed in are listed (no `claude -p` or SDK runs),
//! newest first, with the title Claude Code gave them and the first message.

use crate::claude_projects::modified_ms;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    /// The title from `/rename`, else the one Claude Code gave it.
    pub title: Option<String>,
    /// The first message typed in it, on one line.
    pub prompt: String,
    pub branch: Option<String>,
    /// Last written, Unix milliseconds.
    pub modified_ms: u64,
}

/// Lines read for the first message (hooks and attachments come before it).
const HEAD_LINES: usize = 400;
/// The end of a transcript read for its newest title.
const TAIL_BYTES: u64 = 256 * 1024;
const PROMPT_CHARS: usize = 200;

/// Where Claude Code keeps the transcripts of conversations held in `cwd`.
pub fn folder(claude_projects_dir: &Path, cwd: &Path) -> PathBuf {
    let name: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    claude_projects_dir.join(name)
}

/// The `limit` newest conversations held in `cwd`.
pub fn recent(claude_projects_dir: &Path, cwd: &Path, limit: usize) -> Vec<Conversation> {
    let Ok(entries) = fs::read_dir(folder(claude_projects_dir, cwd)) else {
        return Vec::new();
    };
    let mut files: Vec<(u64, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "jsonl")
                && e.file_type().is_ok_and(|kind| kind.is_file())
        })
        .filter_map(|e| Some((modified_ms(&e.metadata().ok()?), e.path())))
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    files
        .into_iter()
        .filter_map(|(ms, path)| read(&path, cwd, ms))
        .take(limit)
        .collect()
}

/// A Claude Code session id (a UUID): the only thing `--resume` is given.
pub fn is_session_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts
            .iter()
            .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn read(path: &Path, cwd: &Path, modified_ms: u64) -> Option<Conversation> {
    let id = path.file_stem()?.to_str()?.to_string();
    if !is_session_id(&id) {
        return None;
    }
    let (prompt, branch) = first_prompt(path, cwd)?;
    Some(Conversation {
        title: title(path),
        id,
        prompt,
        branch,
        modified_ms,
    })
}

/// The first message typed and the branch it was typed on; None for a
/// conversation held elsewhere, run by a program, or with nothing typed.
fn first_prompt(path: &Path, cwd: &Path) -> Option<(String, Option<String>)> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().take(HEAD_LINES) {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let text = |k: &str| entry.get(k).and_then(|v| v.as_str());
        let flag = |k: &str| entry.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        if text("type") != Some("user") || flag("isMeta") || flag("isSidechain") {
            continue;
        }
        if text("entrypoint").is_some_and(|e| e.starts_with("sdk"))
            || text("cwd").is_some_and(|c| Path::new(c) != cwd)
        {
            return None;
        }
        // Slash commands, their output and reminders come as <tags>.
        let Some(said) = message_text(&entry).filter(|s| !s.trim_start().starts_with('<')) else {
            continue;
        };
        let one_line = said.split_whitespace().collect::<Vec<_>>().join(" ");
        if one_line.is_empty() {
            continue;
        }
        let prompt = one_line.chars().take(PROMPT_CHARS).collect();
        return Some((prompt, text("gitBranch").map(str::to_string)));
    }
    None
}

/// A user message's text: a string, or its text parts.
fn message_text(entry: &serde_json::Value) -> Option<String> {
    match entry.get("message")?.get("content")? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(parts) => {
            let texts: Vec<&str> = parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            (!texts.is_empty()).then(|| texts.join(" "))
        }
        _ => None,
    }
}

/// The newest title near the end of the transcript, `/rename`'s first.
fn title(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    let tail = String::from_utf8_lossy(&tail);
    // Mid-file, the first line is cut.
    let lines = tail.lines().skip(usize::from(start > 0));
    let (mut custom, mut ai) = (None, None);
    for line in lines {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let field = |k: &str| {
            entry
                .get(k)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        match entry.get("type").and_then(|t| t.as_str()) {
            Some("custom-title") => custom = field("customTitle").or(custom),
            Some("ai-title") => ai = field("aiTitle").or(ai),
            _ => {}
        }
    }
    custom.or(ai)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::time::{Duration, SystemTime};

    const A: &str = "0f096eef-0a11-4750-93d4-7f09562dc824";
    const B: &str = "18f4a809-bf1a-49c3-b7a3-ccbc13fbcd9f";
    const C: &str = "e873f5be-20e1-435c-b644-bacf67a3fb73";

    fn write(dir: &Path, id: &str, seconds: u64, lines: &[serde_json::Value]) {
        fs::create_dir_all(dir).unwrap();
        let mut file = File::create(dir.join(format!("{id}.jsonl"))).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    fn said(cwd: &Path, entrypoint: &str, content: serde_json::Value) -> serde_json::Value {
        json!({"type": "user", "cwd": cwd, "entrypoint": entrypoint, "gitBranch": "main",
               "message": {"role": "user", "content": content}})
    }

    #[test]
    fn the_folder_is_the_path_with_dashes() {
        let root = Path::new("/h/.claude/projects");
        assert_eq!(
            folder(root, Path::new("/Users/a/Projects/desk.worktrees/my_app")),
            root.join("-Users-a-Projects-desk-worktrees-my-app")
        );
    }

    #[test]
    fn only_session_ids_are_resumed() {
        assert!(is_session_id(A));
        assert!(!is_session_id("0f096eef-0a11-4750-93d4-7f09562dc82"));
        assert!(!is_session_id("0f096eef-0a11-4750-93d4-7f09562dc82; rm"));
        assert!(!is_session_id("--resume"));
        assert!(!is_session_id(""));
    }

    #[test]
    fn lists_typed_conversations_newest_first_with_their_titles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("projects");
        let cwd = temp.path().join("desk.app");
        let dir = folder(&root, &cwd);
        write(&dir, A, 10, &[
            json!({"type": "queue-operation"}),
            said(&cwd, "cli", json!("<command-name>/clear</command-name>")),
            said(&cwd, "cli", json!([{"type": "text", "text": "odpal mi\n  apke"}])),
            json!({"type": "ai-title", "aiTitle": "Old title"}),
            json!({"type": "ai-title", "aiTitle": "Uruchamianie aplikacji"}),
        ]);
        write(&dir, B, 30, &[
            said(&cwd, "cli", json!("dropdown znika")),
            json!({"type": "ai-title", "aiTitle": "Tryb dropdown"}),
            json!({"type": "custom-title", "customTitle": "Mój tytuł"}),
            json!({"type": "ai-title", "aiTitle": "Later AI title"}),
        ]);
        // A `claude -p` run, one held elsewhere, one with nothing typed.
        write(&dir, C, 40, &[said(&cwd, "sdk-cli", json!("tmux ls"))]);
        write(&dir, "11111111-2222-3333-4444-555555555555", 50, &[said(&temp.path().join("other"), "cli", json!("hej"))]);
        write(&dir, "21111111-2222-3333-4444-555555555555", 60, &[json!({"type": "summary"})]);
        write(&dir, "not-a-session", 70, &[said(&cwd, "cli", json!("x"))]);

        let found = recent(&root, &cwd, 10);
        assert_eq!(
            found,
            vec![
                Conversation {
                    id: B.into(),
                    title: Some("Mój tytuł".into()),
                    prompt: "dropdown znika".into(),
                    branch: Some("main".into()),
                    modified_ms: 30_000,
                },
                Conversation {
                    id: A.into(),
                    title: Some("Uruchamianie aplikacji".into()),
                    prompt: "odpal mi apke".into(),
                    branch: Some("main".into()),
                    modified_ms: 10_000,
                },
            ]
        );
        assert_eq!(recent(&root, &cwd, 1).len(), 1);
        assert!(recent(&root, &temp.path().join("nowhere"), 10).is_empty());
    }

    #[test]
    fn the_title_is_read_from_the_end_of_a_long_transcript() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().to_path_buf();
        let filler = json!({"type": "assistant", "text": "x".repeat(1000)});
        let mut lines = vec![said(&cwd, "cli", json!("first"))];
        lines.extend(std::iter::repeat_n(filler, 400));
        lines.push(json!({"type": "ai-title", "aiTitle": "Na końcu"}));
        write(temp.path(), A, 1, &lines);
        assert_eq!(title(&temp.path().join(format!("{A}.jsonl"))).as_deref(), Some("Na końcu"));
    }
}
