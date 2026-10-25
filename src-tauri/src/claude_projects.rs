//! Reading `~/.claude/projects`: each subdirectory holds `*.jsonl` transcripts
//! for one working directory. The directory name is a lossy encoding of the
//! path (`/` and `.` become `-`), so the real path is read from the `"cwd"`
//! field of a transcript, with name decoding only as a fallback.
//!
//! Contract (implemented by the discovery task):
//!
//! ```ignore
//! pub struct ClaudeProject { pub cwd: PathBuf, pub last_active_ms: u64, pub sessions: u32 }
//! pub fn scan(claude_projects_dir: &Path) -> Vec<ClaudeProject>; // merged by cwd
//! pub fn cwd_from_transcript(path: &Path) -> Option<PathBuf>;
//! pub fn decode_dir_name(name: &str, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf>;
//! ```

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeProject {
    pub cwd: PathBuf,
    pub last_active_ms: u64,
    pub sessions: u32,
}

pub(crate) fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            duration.as_millis().min(u64::MAX as u128) as u64
        })
}

pub fn cwd_from_transcript(path: &Path) -> Option<PathBuf> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().take(64) {
        let Ok(line) = line else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if let Some(cwd) = value.get("cwd").and_then(|value| value.as_str()) {
            return Some(PathBuf::from(cwd));
        }
    }
    None
}

const MAX_SPAN: usize = 8;

/// Resolve one existing component at a time, longest encoded component first.
/// A dash may stand for a literal dash, a dot, or a component boundary.
pub fn decode_dir_name(name: &str, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    fn candidate(
        base: &Path,
        tokens: &[&str],
        next: usize,
        component: String,
        exists: &dyn Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        if next == tokens.len() {
            let path = base.join(component);
            return exists(&path).then_some(path);
        }
        for separator in ['-', '.'] {
            let mut joined = component.clone();
            joined.push(separator);
            joined.push_str(tokens[next]);
            if let Some(path) = candidate(base, tokens, next + 1, joined, exists) {
                return Some(path);
            }
        }
        None
    }
    let encoded = name.strip_prefix('-')?;
    if encoded.is_empty() {
        return None;
    }
    let tokens: Vec<_> = encoded.split('-').collect();
    let mut path = PathBuf::from("/");
    let mut offset = 0;
    while offset < tokens.len() {
        let mut found = None;
        // A single path component rarely spans more than a few dashes; the cap
        // keeps the dash/dot combinations (2^(span-1)) small.
        for end in (offset + 1..=tokens.len().min(offset + MAX_SPAN)).rev() {
            if let Some(next) = candidate(
                &path,
                &tokens[offset..end],
                1,
                tokens[offset].to_owned(),
                exists,
            ) {
                found = Some((next, end));
                break;
            }
        }
        let (next, end) = found?;
        path = next;
        offset = end;
    }
    Some(path)
}

pub fn scan(claude_projects_dir: &Path) -> Vec<ClaudeProject> {
    let mut projects: BTreeMap<PathBuf, ClaudeProject> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(claude_projects_dir) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(transcripts) = fs::read_dir(entry.path()) else {
            continue;
        };
        let mut sessions = 0_u32;
        let mut newest = None;
        for transcript in transcripts.flatten() {
            if transcript
                .path()
                .extension()
                .is_none_or(|ext| ext != "jsonl")
                || !transcript.file_type().is_ok_and(|kind| kind.is_file())
            {
                continue;
            }
            let Ok(metadata) = transcript.metadata() else {
                continue;
            };
            sessions = sessions.saturating_add(1);
            let key = (metadata.modified().unwrap_or(UNIX_EPOCH), transcript.path());
            if newest.as_ref().is_none_or(|previous| key > *previous) {
                newest = Some(key);
            }
        }
        let cwd = newest
            .as_ref()
            .and_then(|(_, path)| cwd_from_transcript(path))
            .or_else(|| decode_dir_name(&entry.file_name().to_string_lossy(), &Path::exists));
        let Some(cwd) = cwd.and_then(|path| path.canonicalize().ok()) else {
            continue;
        };
        let last_active_ms = newest.map_or(0, |(mtime, _)| {
            mtime.duration_since(UNIX_EPOCH).map_or(0, |duration| {
                duration.as_millis().min(u64::MAX as u128) as u64
            })
        });
        let project = projects.entry(cwd.clone()).or_insert(ClaudeProject {
            cwd,
            last_active_ms: 0,
            sessions: 0,
        });
        project.sessions = project.sessions.saturating_add(sessions);
        project.last_active_ms = project.last_active_ms.max(last_active_ms);
    }
    projects.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{Duration, SystemTime};

    pub(crate) fn transcript(path: &Path, cwd: &Path, seconds: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = File::create(path).unwrap();
        writeln!(file, "invalid json\n{{\"cwd\":42}}").unwrap();
        writeln!(file, "{}", serde_json::json!({"cwd": cwd})).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn transcript_head_is_bounded_and_tolerates_invalid_lines() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.jsonl");
        transcript(&path, temp.path(), 1);
        assert_eq!(cwd_from_transcript(&path), Some(temp.path().to_owned()));
        fs::write(
            &path,
            format!("{}{{\"cwd\":\"/too-late\"}}\n", "{}\n".repeat(64)),
        )
        .unwrap();
        assert_eq!(cwd_from_transcript(&path), None);
        fs::write(
            &path,
            format!("{}{{\"cwd\":\"/last\"}}\n", "{}\n".repeat(63)),
        )
        .unwrap();
        assert_eq!(cwd_from_transcript(&path), Some(PathBuf::from("/last")));
    }

    #[test]
    fn decoding_prefers_longest_existing_component_and_preserves_punctuation() {
        let existing = [
            "/Users",
            "/Users/kamil",
            "/Users/kamil/Documents",
            "/Users/kamil/Documents/my-app",
            "/Users/kamil/Documents/my-app/example.test-app",
        ];
        let exists = |path: &Path| existing.iter().any(|item| path == Path::new(item));
        assert_eq!(
            decode_dir_name("-Users-kamil-Documents-my-app-example-test-app", &exists),
            Some(PathBuf::from(existing[4]))
        );
        assert_eq!(decode_dir_name("-Users-kamil-missing", &exists), None);
        assert_eq!(decode_dir_name("Users-kamil", &exists), None);
        // Long names that exist nowhere finish quickly instead of exploring 2^30 candidates.
        let calls = std::cell::Cell::new(0u32);
        let long = format!("-{}", vec!["x"; 31].join("-"));
        assert_eq!(
            decode_dir_name(&long, &|_| {
                calls.set(calls.get() + 1);
                false
            }),
            None
        );
        assert!(calls.get() < 1000, "{} exists() calls", calls.get());
        assert_eq!(
            decode_dir_name("-a-b", &|path| ["/a", "/a-b", "/a/b"]
                .iter()
                .any(|p| path == Path::new(p))),
            Some(PathBuf::from("/a-b"))
        );
    }

    #[test]
    fn scan_uses_newest_transcript_merges_aliases_and_decodes_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("projects");
        let cwd = temp.path().join("my-app.test");
        fs::create_dir(&cwd).unwrap();
        transcript(
            &root.join("-leading-dash/old.jsonl"),
            &temp.path().join("wrong"),
            10,
        );
        transcript(&root.join("-leading-dash/new.jsonl"), &cwd, 20);
        transcript(&root.join("-other/session.jsonl"), &cwd.join("."), 30);
        let encoded = cwd.to_string_lossy().replace(['/', '.'], "-");
        fs::create_dir_all(root.join(&encoded)).unwrap();
        fs::write(root.join(encoded).join("invalid.jsonl"), "invalid").unwrap();
        let projects = scan(&root);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].cwd, cwd.canonicalize().unwrap());
        assert_eq!(projects[0].sessions, 4);
        assert!(projects[0].last_active_ms >= 30_000);
    }
}
