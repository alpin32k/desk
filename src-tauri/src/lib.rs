pub mod agents;
pub mod claude_projects;
pub mod coffee;
pub mod discovery;
pub mod env;
pub mod git;
pub mod hooks;
pub mod layout;
pub mod new_project;
pub mod pty;
pub mod state_watch;
pub mod tmux;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::{Channel, Response};
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, State};

/// Coffee state lives in its own managed value so the guard thread can reach it.
pub struct AppCoffee(pub coffee::Coffee);

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

#[tauri::command]
fn list_sessions(app: State<App>) -> Vec<tmux::SessionInfo> {
    app.tmux.list_sessions()
}

/// Ensure the tmux session for project + agent + sub-session exists; returns its slug.
#[tauri::command]
fn open_session(
    app: State<App>,
    path: String,
    agent: String,
    instance: u32,
) -> Result<String, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("folder does not exist: {path}"));
    }
    // Match by what the session was started for, so a slug collision or an
    // older naming scheme never attaches to another project.
    if let Some(s) = app
        .tmux
        .list_sessions()
        .into_iter()
        .find(|s| s.path == path && s.agent == agent && s.instance == instance)
    {
        return Ok(s.slug);
    }
    let custom = layout::load(&app.layout_path).agents;
    let command =
        agents::command_for(&agent, &custom).ok_or_else(|| format!("unknown agent: {agent}"))?;
    let slug = tmux::session_slug(dir, &agent, instance);
    if app.tmux.has_session(&slug) {
        return Err(format!(
            "tmux session {slug} already belongs to another folder"
        ));
    }
    let script = agents::launch_script(&agent, &command, &app.hooks.settings);
    let env = [
        ("DESK_SLUG", slug.clone()),
        ("DESK_STATE_DIR", app.hooks.state_dir.display().to_string()),
    ];
    app.tmux
        .new_session(&slug, dir, &script, &env, &agent, instance)?;
    Ok(slug)
}

#[tauri::command]
fn list_agents(app: State<App>) -> Vec<agents::AgentInfo> {
    agents::list(&layout::load(&app.layout_path).agents)
}

#[tauri::command]
fn create_project(parent: String, name: String, git: bool) -> Result<String, String> {
    new_project::create(Path::new(&parent), &name, git).map(|p| p.display().to_string())
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

#[tauri::command(async)]
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
fn tile_info(paths: Vec<String>) -> Vec<TileInfo> {
    paths
        .into_iter()
        .map(|path| TileInfo {
            branch: git::branch(Path::new(&path)),
            path,
        })
        .collect()
}

#[tauri::command]
fn coffee_status(coffee: State<AppCoffee>) -> coffee::Status {
    coffee.0.status()
}

/// May show the macOS admin password dialog, so it runs off the main thread.
#[tauri::command(async)]
fn coffee_set(coffee: State<AppCoffee>, level: coffee::Level) -> Result<coffee::Status, String> {
    coffee.0.set(level)
}

#[tauri::command(async)]
fn coffee_restore(coffee: State<AppCoffee>) -> Result<coffee::Status, String> {
    coffee.0.restore_sleep()
}

/// Git repos under ~ (3 levels) plus projects from ~/.claude/projects and
/// hand-added folders. Read-only and local; runs off the main thread.
#[tauri::command]
async fn list_projects(extra: Vec<String>) -> Vec<discovery::Project> {
    let home = env::home();
    tauri::async_runtime::spawn_blocking(move || {
        discovery::discover(&discovery::DiscoverOptions {
            claude_projects_dir: home.join(".claude/projects"),
            home,
            max_depth: 3,
            extra: extra.into_iter().map(PathBuf::from).collect(),
        })
    })
    .await
    .unwrap_or_default()
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
        .plugin(tauri_plugin_dialog::init())
        .menu(menu)
        .setup(|app| {
            let support = env::support_dir();
            let tmux = tmux::Tmux::new(&support)?;
            let hooks = hooks::install(&support)?;
            let watcher = state_watch::watch(app.handle().clone(), &hooks.state_dir).ok();
            app.manage(AppCoffee(coffee::Coffee::new(&support)));
            coffee::spawn_guard(app.handle().clone());
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
            list_projects,
            list_sessions,
            coffee_status,
            coffee_set,
            coffee_restore,
            list_agents,
            create_project,
        ])
        .build(tauri::generate_context!())
        .expect("error while building desk")
        .run(|handle, event| {
            if let RunEvent::Exit = event {
                // Detach every tile; the tmux sessions keep running.
                handle.state::<App>().ptys.close_all();
                // Sleep goes back to normal (the lid watchdog also notices desk is gone).
                handle.state::<AppCoffee>().0.shutdown(None);
            }
        });
}
