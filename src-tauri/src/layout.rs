//! Persistent layout and options in `layout.json`: lanes (columns) of project
//! paths, projects started with the app, hand-added folders.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_LANES: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
    pub version: u32,
    /// Each lane is a column of tiles, top to bottom, identified by project path.
    pub lanes: Vec<Vec<String>>,
    /// Projects whose sessions are opened when the app starts.
    pub autostart: Vec<String>,
    /// Folders added by hand, listed even when discovery does not find them.
    pub extra_folders: Vec<String>,
    pub notifications: bool,
    /// Color picked by hand per project path (index into the UI palette);
    /// projects not listed get a color derived from their path.
    pub colors: std::collections::BTreeMap<String, u32>,
    /// Agents added by hand (built-ins are always available).
    pub agents: Vec<crate::agents::CustomAgent>,
    /// Command of each "run" session (⌘U), by session key, to start it again.
    pub runs: std::collections::BTreeMap<String, String>,
    /// Card a session sits in when it is not its own project's, by key (none are
    /// set now; ⌘R conversations used to join the active card).
    pub cards: std::collections::BTreeMap<String, String>,
    /// Where "new project" creates folders; None = ~/Documents/projects or ~.
    pub projects_dir: Option<String>,
    /// Columns follow what is open and the window size; off = `lanes` as set by hand.
    pub auto_layout: bool,
    /// Color theme id (see src/themes.ts).
    pub theme: String,
    /// Where web pages open outside desk: "auto" (Brave, then Chrome),
    /// "brave", "chrome" or "system" (see browser.rs).
    pub browser: String,
    /// Terminal font size in px (⌘= / ⌘- / ⌘0, Options).
    pub font_size: f64,
    /// Session keys whose tiles are folded to their header (⌘J).
    pub minimized: Vec<String>,
    /// Names given to tiles by hand, by session key.
    pub titles: std::collections::BTreeMap<String, String>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: 1,
            lanes: vec![Vec::new(); DEFAULT_LANES],
            autostart: Vec::new(),
            extra_folders: Vec::new(),
            notifications: true,
            colors: Default::default(),
            agents: Vec::new(),
            runs: Default::default(),
            cards: Default::default(),
            projects_dir: None,
            auto_layout: true,
            theme: "desk".into(),
            browser: "auto".into(),
            font_size: 11.5,
            minimized: Vec::new(),
            titles: Default::default(),
        }
    }
}

impl Layout {
    /// At least one lane, and no project in two places.
    pub fn normalized(mut self) -> Self {
        if self.lanes.is_empty() {
            self.lanes = vec![Vec::new(); DEFAULT_LANES];
        }
        let mut seen = std::collections::HashSet::new();
        for lane in &mut self.lanes {
            lane.retain(|p| seen.insert(p.clone()));
        }
        let mut seen = std::collections::HashSet::new();
        self.autostart.retain(|p| seen.insert(p.clone()));
        let mut seen = std::collections::HashSet::new();
        self.extra_folders.retain(|p| seen.insert(p.clone()));
        self
    }
}

/// Missing file gives the default; an unreadable one is kept as
/// `layout.json.bak` and replaced by the default rather than failing startup.
pub fn load(path: &Path) -> Layout {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Layout::default();
    };
    match serde_json::from_str::<Layout>(&text) {
        Ok(l) => l.normalized(),
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("json.bak"));
            Layout::default()
        }
    }
}

/// Atomic write: temp file in the same directory, then rename.
pub fn save(path: &Path, layout: &Layout) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&layout.clone().normalized())?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_default_with_four_lanes_and_no_autostart() {
        let tmp = tempfile::tempdir().unwrap();
        let l = load(&tmp.path().join("layout.json"));
        assert_eq!(l.lanes.len(), 4);
        assert!(l.autostart.is_empty());
    }

    #[test]
    fn round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("sub/layout.json");
        let l = Layout {
            lanes: vec![vec!["/a".into(), "/b".into()], vec![], vec!["/c".into()]],
            autostart: vec!["/a".into()],
            extra_folders: vec!["/x".into()],
            notifications: false,
            colors: [("/a".to_string(), 3)].into(),
            ..Layout::default()
        };
        save(&p, &l).unwrap();
        assert_eq!(load(&p), l);
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("layout.json");
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load(&p), Layout::default());
        assert!(p.with_extension("json.bak").exists());
    }

    #[test]
    fn partial_file_fills_defaults_and_dedupes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("layout.json");
        std::fs::write(
            &p,
            r#"{"lanes":[["/a","/b"],["/a"]],"autostart":["/a","/a"]}"#,
        )
        .unwrap();
        let l = load(&p);
        assert_eq!(l.lanes, vec![vec!["/a".to_string(), "/b".into()], vec![]]);
        assert_eq!(l.autostart, vec!["/a".to_string()]);
        assert!(l.notifications);
    }
}
