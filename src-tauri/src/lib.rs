pub mod claude_projects;
pub mod discovery;
pub mod env;
pub mod git;
pub mod hooks;
pub mod layout;
pub mod pty;
pub mod state_watch;
pub mod tmux;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::{Channel, Response};
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, State};

struct App {
    tmux: tmux::Tmux,
    hooks: hooks::HookFiles,
    layout_path: PathBuf,
    ptys: pty::PtyManager,
    _watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    layout: layout::Layout,
    sessions: Vec<tmux::SessionInfo>,
    states: Vec<hooks::SessionState>,
    home: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TileInfo {
    path: String,
    branch: Option<String>,
    /// Foreground command in the session (`claude`, `zsh`, ...); None when
    /// the tmux session no longer exists.
    command: Option<String>,
}

#[tauri::command]
fn bootstrap(app: State<App>) -> Bootstrap {
    Bootstrap {
        layout: layout::load(&app.layout_path),
        sessions: app.tmux.list_sessions(),
        states: state_watch::read_all(&app.hooks.state_dir),
        home: env::home().display().to_string(),
    }
}

/// Ensure the project's tmux session exists; returns its slug.
#[tauri::command]
fn open_session(app: State<App>, path: String) -> Result<String, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("folder does not exist: {path}"));
    }
    let slug = tmux::slug(dir);
    if !app.tmux.has_session(&slug) {
        app.tmux
            .new_session(&slug, dir, &app.hooks.settings, &app.hooks.state_dir)?;
    }
    Ok(slug)
}

#[tauri::command]
fn kill_session(app: State<App>, slug: String) -> Result<(), String> {
    app.tmux.kill_session(&slug)?;
    let _ = std::fs::remove_file(app.hooks.state_dir.join(format!("{slug}.json")));
    Ok(())
}

#[tauri::command]
fn pty_attach(
    handle: AppHandle,
    app: State<App>,
    slug: String,
    cols: u16,
    rows: u16,
    on_data: Channel<Response>,
) -> Result<u32, String> {
    app.ptys
        .attach(handle, &app.tmux, &slug, cols, rows, on_data)
}

#[tauri::command]
fn pty_write(app: State<App>, id: u32, data: String) -> Result<(), String> {
    app.ptys.write(id, data.as_bytes())
}

#[tauri::command]
fn pty_resize(app: State<App>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    app.ptys.resize(id, cols, rows)
}

#[tauri::command]
fn pty_close(app: State<App>, id: u32) {
    app.ptys.close(id);
}

#[tauri::command]
fn save_layout(app: State<App>, layout: layout::Layout) -> Result<(), String> {
    layout::save(&app.layout_path, &layout).map_err(|e| e.to_string())
}

#[tauri::command]
fn tile_info(app: State<App>, paths: Vec<String>) -> Vec<TileInfo> {
    let sessions = app.tmux.list_sessions();
    paths
        .into_iter()
        .map(|path| {
            let slug = tmux::slug(Path::new(&path));
            let command = sessions
                .iter()
                .find(|s| s.slug == slug)
                .map(|s| s.command.clone());
            TileInfo {
                branch: git::branch(Path::new(&path)),
                path,
                command,
            }
        })
        .collect()
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // No "Close Window" (⌘W closes a tile) and no Select All / Undo, which
    // would act on the page instead of the focused terminal.
    let app_menu = Submenu::with_items(
        app,
        "desk",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &edit, &window])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .menu(menu)
        .setup(|app| {
            let support = env::support_dir();
            let tmux = tmux::Tmux::new(&support)?;
            let hooks = hooks::install(&support)?;
            let watcher = state_watch::watch(app.handle().clone(), &hooks.state_dir).ok();
            app.manage(App {
                tmux,
                hooks,
                layout_path: support.join("layout.json"),
                ptys: pty::PtyManager::default(),
                _watcher: Mutex::new(watcher),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            open_session,
            kill_session,
            pty_attach,
            pty_write,
            pty_resize,
            pty_close,
            save_layout,
            tile_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while building desk")
        .run(|handle, event| {
            if let RunEvent::Exit = event {
                // Detach every tile; the tmux sessions keep running.
                handle.state::<App>().ptys.close_all();
            }
        });
}
