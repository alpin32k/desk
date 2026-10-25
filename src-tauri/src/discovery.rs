//! Project discovery: git repositories under the home directory plus projects
//! Claude Code has been used in (`~/.claude/projects`).
//!
//! Contract used by the rest of the app (implemented by the discovery task):
//!
//! ```ignore
//! #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
//! #[serde(rename_all = "camelCase")]
//! pub struct Project {
//!     pub path: String,               // absolute, canonical, no trailing slash
//!     pub name: String,               // last path component
//!     pub last_active_ms: u64,        // newest of: claude transcript mtime, .git/HEAD|index mtime; 0 if unknown
//!     pub is_git: bool,
//!     pub worktree_of: Option<String>,// main repo path when `path` is a linked git worktree
//!     pub claude_sessions: u32,       // number of *.jsonl transcripts for this cwd
//! }
//!
//! pub struct DiscoverOptions {
//!     pub home: PathBuf,
//!     pub claude_projects_dir: PathBuf, // normally ~/.claude/projects
//!     pub max_depth: usize,             // 3 = ~/a/b/c
//!     pub extra: Vec<PathBuf>,          // folders added by hand; always included if they exist
//! }
//!
//! pub fn discover(opts: &DiscoverOptions) -> Vec<Project>; // sorted by last_active_ms desc, then name
//! ```
