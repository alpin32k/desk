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
