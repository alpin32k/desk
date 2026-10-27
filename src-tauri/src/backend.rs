//! Where sessions live: tmux on macOS and Linux (they outlive desk), or
//! desk's own pseudo terminals on Windows (see native.rs; they end with desk).
//! Without tmux in PATH desk runs the tmux it ships (see `tmux::find`); with
//! none at all macOS and Linux use the native backend too, and the window says
//! so. `DESK_NATIVE_SESSIONS=1` picks it for testing.

use crate::native::Native;
use crate::pty::{PtyExit, PtyManager};
use crate::tmux::{SessionInfo, Tmux};
use std::path::Path;
use std::sync::Arc;
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Emitter};

pub enum Backend {
    Tmux { tmux: Tmux, ptys: PtyManager },
    Native(Native),
}

impl Backend {
    pub fn new(app: &AppHandle, support_dir: &Path) -> std::io::Result<Self> {
        let tmux = if cfg!(windows) || native_by_choice() {
            None
        } else {
            crate::tmux::find(support_dir)
        };
        let Some(bin) = tmux else {
            let app = app.clone();
            return Ok(Self::Native(Native::new(Arc::new(move |slug, ids| {
                for id in ids {
                    let slug = slug.to_string();
                    let _ = app.emit("pty-exit", PtyExit { id, slug });
                }
            }))));
        };
        Ok(Self::Tmux {
            tmux: Tmux::new(support_dir, bin)?,
            ptys: PtyManager::default(),
        })
    }

    /// Sessions survive closing desk (tmux) or end with it (native).
    pub fn persistent(&self) -> bool {
        matches!(self, Self::Tmux { .. })
    }

    /// macOS or Linux without tmux: sessions end with desk until it is installed.
    pub fn tmux_missing(&self) -> bool {
        !cfg!(windows) && !self.persistent() && !native_by_choice()
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        match self {
            Self::Tmux { tmux, .. } => tmux.list_sessions(),
            Self::Native(n) => n.list(),
        }
    }

    pub fn has(&self, slug: &str) -> bool {
        match self {
            Self::Tmux { tmux, .. } => tmux.has_session(slug),
            Self::Native(n) => n.has(slug),
        }
    }

    /// Start agent `id` (command `command`) in `dir` as session `slug`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        slug: &str,
        dir: &Path,
        id: &str,
        command: &str,
        hooks_settings: &Path,
        env: &[(&str, String)],
        instance: u32,
    ) -> Result<(), String> {
        match self {
            Self::Tmux { tmux, .. } => {
                let script = crate::agents::launch_script(id, command, hooks_settings);
                tmux.new_session(slug, dir, &script, env, id, instance)
            }
            Self::Native(n) => {
                let argv = crate::agents::launch_argv(id, command, hooks_settings);
                n.start(slug, dir, &argv, env, id, instance)
            }
        }
    }

    pub fn kill(&self, slug: &str) -> Result<(), String> {
        match self {
            Self::Tmux { tmux, .. } => tmux.kill_session(slug),
            Self::Native(n) => n.kill(slug),
        }
    }

    /// See `Tmux::clean_global_env`; native sessions use desk's own environment.
    pub fn clean_env(&self) {
        if let Backend::Tmux { tmux, .. } = self {
            tmux.clean_global_env();
        }
    }

    /// See `Tmux::reload_config`.
    pub fn reload_config(&self) {
        if let Backend::Tmux { tmux, .. } = self {
            tmux.reload_config();
        }
    }

    /// See `Tmux::retag_untagged`; native sessions are always tagged.
    pub fn retag_untagged(&self, agents: &[String]) {
        if let Self::Tmux { tmux, .. } = self {
            tmux.retag_untagged(agents);
        }
    }

    pub fn attach(
        &self,
        app: AppHandle,
        slug: &str,
        cols: u16,
        rows: u16,
        on_data: Channel<Response>,
    ) -> Result<u32, String> {
        match self {
            Self::Tmux { tmux, ptys } => ptys.attach(app, tmux, slug, cols, rows, on_data),
            Self::Native(n) => n.attach(
                slug,
                cols,
                rows,
                Box::new(move |bytes| on_data.send(Response::new(bytes)).is_ok()),
            ),
        }
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        match self {
            Self::Tmux { ptys, .. } => ptys.write(id, data),
            Self::Native(n) => n.write(id, data),
        }
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        match self {
            Self::Tmux { ptys, .. } => ptys.resize(id, cols, rows),
            Self::Native(n) => n.resize(id, cols, rows),
        }
    }

    /// Close a tile; the session keeps running.
    pub fn close(&self, id: u32) {
        match self {
            Self::Tmux { ptys, .. } => ptys.close(id),
            Self::Native(n) => n.detach(id),
        }
    }

    /// desk is quitting: tmux sessions stay, native ones end.
    pub fn shutdown(&self) {
        match self {
            Self::Tmux { ptys, .. } => ptys.close_all(),
            Self::Native(n) => n.kill_all(),
        }
    }
}

fn native_by_choice() -> bool {
    std::env::var_os("DESK_NATIVE_SESSIONS").is_some_and(|v| v == "1")
}
