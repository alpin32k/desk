//! Watches `state/` (written by the hook script) and forwards session state
//! changes to the webview as `session-state` events.

use crate::hooks::{SessionState, read_state};
use notify::{RecursiveMode, Watcher};
use std::path::Path;
use tauri::{AppHandle, Emitter};

pub fn read_all(state_dir: &Path) -> Vec<SessionState> {
    let Ok(entries) = std::fs::read_dir(state_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| read_state(&e.path()))
        .collect()
}

/// The returned watcher must be kept alive for events to keep coming.
pub fn watch(app: AppHandle, state_dir: &Path) -> notify::Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        for path in event.paths {
            if let Some(state) = read_state(&path) {
                let _ = app.emit("session-state", state);
            }
        }
    })?;
    watcher.watch(state_dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}
