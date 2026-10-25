pub mod agents;
pub mod backend;
pub mod claude_projects;
pub mod coffee;
pub mod discovery;
pub mod env;
pub mod git;
pub mod hooks;
pub mod layout;
pub mod lock;
pub mod native;
pub mod new_project;
pub mod preview;
pub mod project;
pub mod pty;
pub mod state_watch;
pub mod tmux;
pub mod usage;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::{Channel, Response};
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, State};

/// Coffee state lives in its own managed value so the guard thread can reach it.
pub struct AppCoffee(pub coffee::Coffee);

struct App {
    sessions: backend::Backend,
    hooks: hooks::HookFiles,
    layout_path: PathBuf,
    _watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    layout: layout::Layout,
    sessions: Vec<tmux::SessionInfo>,
    states: Vec<hooks::SessionState>,
    home: String,
    /// Sessions outlive desk (tmux); false on Windows, where they end with it.
    persistent: bool,
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
        sessions: app.sessions.list(),
        persistent: app.sessions.persistent(),
        states: state_watch::read_all(&app.hooks.state_dir),
        home: env::home().display().to_string(),
    }
}

#[tauri::command]
fn list_sessions(app: State<App>) -> Vec<tmux::SessionInfo> {
    app.sessions.list()
}

/// Ensure the tmux session for project + agent + sub-session exists; returns its slug.
#[tauri::command]
fn open_session(
    app: State<App>,
    path: String,
    agent: String,
    instance: u32,
    command: Option<String>,
) -> Result<String, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("folder does not exist: {path}"));
    }
    // Match by what the session was started for, so a slug collision or an
    // older naming scheme never attaches to another project.
    if let Some(s) = app
        .sessions
        .list()
        .into_iter()
        .find(|s| s.path == path && s.agent == agent && s.instance == instance)
    {
        return Ok(s.slug);
    }
    let command = if agent == RUN_AGENT {
        // Only a command the project itself defines (see project::run_targets).
        let command = command.unwrap_or_default();
        if !project::run_targets(dir)
            .iter()
            .any(|t| t.command == command)
        {
            return Err(format!("{path} nie definiuje polecenia „{command}”"));
        }
        command
    } else {
        let custom = layout::load(&app.layout_path).agents;
        agents::command_for(&agent, &custom).ok_or_else(|| format!("unknown agent: {agent}"))?
    };
    let slug = tmux::session_slug(dir, &agent, instance);
    if app.sessions.has(&slug) {
        return Err(format!(
            "tmux session {slug} already belongs to another folder"
        ));
    }
    let env = [
        ("DESK_SLUG", slug.clone()),
        ("DESK_STATE_DIR", app.hooks.state_dir.display().to_string()),
        // desk's --settings replaces the status line; the wrapper runs this one.
        (
            "DESK_STATUSLINE",
            usage::user_statusline(&env::home()).unwrap_or_default(),
        ),
    ];
    app.sessions.start(
        &slug,
        dir,
        &agent,
        &command,
        &app.hooks.settings,
        &env,
        instance,
    )?;
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
    app.sessions.kill(&slug)?;
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
    app.sessions.attach(handle, &slug, cols, rows, on_data)
}

#[tauri::command(async)]
fn pty_write(
    app: State<App>,
    lock: State<lock::Lock>,
    id: u32,
    data: String,
) -> Result<(), String> {
    if lock.is_locked() {
        return Err("desk jest zablokowany".into());
    }
    app.sessions.write(id, data.as_bytes())
}

#[tauri::command]
fn lock_desk(handle: AppHandle, lock: State<lock::Lock>) -> Result<(), String> {
    lock.lock(&handle)
}

/// Shows the Touch ID / password dialog, so it runs off the main thread.
#[tauri::command(async)]
fn unlock_desk(handle: AppHandle, lock: State<lock::Lock>) -> Result<(), String> {
    lock.unlock(&handle)
}

/// Show a link in the side panel at `bounds`; returns what the address bar shows.
#[tauri::command]
fn open_preview(handle: AppHandle, url: String, bounds: preview::Bounds) -> Result<String, String> {
    preview::open(&handle, &url, bounds)
}

#[tauri::command]
fn preview_bounds(handle: AppHandle, bounds: preview::Bounds) {
    preview::set_bounds(&handle, bounds);
}

#[tauri::command]
fn preview_visible(handle: AppHandle, visible: bool) {
    preview::set_visible(&handle, visible);
}

#[tauri::command]
fn preview_close(handle: AppHandle) {
    preview::close(&handle);
}

#[tauri::command]
fn preview_nav(handle: AppHandle, action: String) -> Result<(), String> {
    preview::navigate(&handle, &action)
}

/// Plan limits for the given agents, from local files only.
#[tauri::command(async)]
fn usage(app: State<App>, agents: Vec<String>) -> Vec<usage::Usage> {
    usage::usage(&agents, &app.hooks.state_dir, &env::home())
}

/// Folder for plain conversations (⌘R) that belong to no project.
#[tauri::command]
fn chat_dir() -> Result<String, String> {
    let dir = env::support_dir().join("rozmowa");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Claude reads this as the folder's instructions; yours to edit, never overwritten.
    let md = dir.join("CLAUDE.md");
    if !md.exists() {
        std::fs::write(&md, CHAT_MD).map_err(|e| e.to_string())?;
    }
    Ok(dir.display().to_string())
}

const CHAT_MD: &str = "# Rozmowa

To nie jest projekt, tylko zwykła rozmowa, jak w czacie. Rozmawiamy o czymkolwiek.
Odpowiadaj wprost i zwięźle. Nie przeglądaj tego folderu, nie twórz plików i nie
uruchamiaj poleceń, chyba że wyraźnie o to poproszę.
";

#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    preview::open_external(&url)
}

#[tauri::command]
fn is_locked(lock: State<lock::Lock>) -> bool {
    lock.is_locked()
}

#[tauri::command]
fn pty_resize(app: State<App>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    app.sessions.resize(id, cols, rows)
}

#[tauri::command]
fn pty_close(app: State<App>, id: u32) {
    app.sessions.close(id);
}

#[tauri::command]
fn save_layout(app: State<App>, layout: layout::Layout) -> Result<(), String> {
    layout::save(&app.layout_path, &layout).map_err(|e| e.to_string())
}

/// Sessions started with ⌘U: a project command (dev server, tests...).
const RUN_AGENT: &str = "run";

#[tauri::command]
async fn project_files(path: String) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || project::files(Path::new(&path)))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn open_file(path: String, file: String) -> Result<(), String> {
    project::open_in_editor(Path::new(&path), &file)
}

#[tauri::command]
fn preview_file(
    handle: AppHandle,
    path: String,
    file: String,
    bounds: preview::Bounds,
) -> Result<String, String> {
    let full = project::inside(Path::new(&path), &file)?;
    preview::open(&handle, &full.display().to_string(), bounds)
}

#[tauri::command]
fn run_targets(path: String) -> Vec<project::RunTarget> {
    project::run_targets(Path::new(&path))
}

#[tauri::command]
fn git_branches(path: String) -> Result<Vec<String>, String> {
    git::branches(Path::new(&path))
}

#[tauri::command]
fn git_switch(path: String, branch: String) -> Result<(), String> {
    git::switch(Path::new(&path), &branch)
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
fn coffee_uninstall(coffee: State<AppCoffee>) -> Result<coffee::Status, String> {
    coffee.0.uninstall_helper()
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

/// While locked only Edit remains: no Hide, Minimize, Full Screen or Quit
/// that could take the window off screen.
pub(crate) fn menu(app: &AppHandle, locked: bool) -> tauri::Result<Menu<tauri::Wry>> {
    if locked {
        let edit =
            Submenu::with_items(app, "Edit", true, &[&PredefinedMenuItem::copy(app, None)?])?;
        return Menu::with_items(app, &[&edit]);
    }
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

/// One expansion only: it embeds Info.plist, which may be defined once.
fn context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env::init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol(preview::SCHEME, |_ctx, req| preview::serve(&req));
    // A menu only on macOS: on Windows and Linux it is a bar in the window whose
    // Edit > Copy (Ctrl+C) would take Ctrl+C away from the terminals.
    let builder = if cfg!(target_os = "macos") {
        builder.menu(|app| menu(app, false))
    } else {
        builder
    };
    builder
        .setup(|app| {
            let support = env::support_dir();
            let sessions = backend::Backend::new(app.handle(), &support)?;
            let custom = layout::load(&support.join("layout.json")).agents;
            let ids: Vec<String> = agents::list(&custom).into_iter().map(|a| a.id).collect();
            sessions.retag_untagged(&ids);
            let hooks = hooks::install(&support)?;
            let watcher = state_watch::watch(app.handle().clone(), &hooks.state_dir).ok();
            app.manage(lock::Lock::default());
            app.manage(AppCoffee(coffee::Coffee::new(&support)));
            coffee::spawn_guard(app.handle().clone());
            app.manage(App {
                sessions,
                hooks,
                layout_path: support.join("layout.json"),
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
            git_branches,
            project_files,
            open_file,
            preview_file,
            run_targets,
            git_switch,
            list_projects,
            list_sessions,
            coffee_status,
            coffee_set,
            coffee_restore,
            coffee_uninstall,
            lock_desk,
            unlock_desk,
            is_locked,
            open_preview,
            preview_bounds,
            preview_visible,
            preview_close,
            preview_nav,
            open_external,
            chat_dir,
            usage,
            list_agents,
            create_project,
        ])
        .on_window_event(|window, event| {
            if !window.state::<lock::Lock>().is_locked() || window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => api.prevent_close(),
                // Locked: keep the window up front, whatever tried to move it away
                // (except the unlock dialog itself).
                tauri::WindowEvent::Focused(false) | tauri::WindowEvent::Resized(_)
                    if window.state::<lock::Lock>().holds_focus() =>
                {
                    if let Some(w) = window.app_handle().get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                }
                _ => {}
            }
        })
        .build(context())
        .expect("error while building desk")
        .run(|handle, event| {
            if let RunEvent::ExitRequested { api, .. } = &event
                && handle.state::<lock::Lock>().is_locked()
            {
                api.prevent_exit();
            }
            if let RunEvent::Exit = event {
                // Detach every tile; tmux sessions keep running, native ones end.
                handle.state::<App>().sessions.shutdown();
                // Sleep goes back to normal (the lid watchdog also notices desk is gone).
                handle.state::<AppCoffee>().0.shutdown(None);
            }
        });
}

#[cfg(test)]
mod acl_tests {
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    fn call(label: &str, url: &str) -> Result<String, String> {
        let app = mock_builder()
            .manage(super::lock::Lock::default())
            .invoke_handler(tauri::generate_handler![super::is_locked])
            .build(super::context())
            .unwrap();
        let url: url::Url = url.parse().unwrap();
        let w = WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url.clone()))
            .build()
            .unwrap();
        invoke(&w, url)
    }

    /// The side panel: a second webview inside the main window.
    fn call_panel(url: &str) -> Result<String, String> {
        let app = mock_builder()
            .manage(super::lock::Lock::default())
            .invoke_handler(tauri::generate_handler![super::is_locked])
            .build(super::context())
            .unwrap();
        let main: url::Url = "tauri://localhost".parse().unwrap();
        let w = WebviewWindowBuilder::new(&app, "main", WebviewUrl::External(main))
            .build()
            .unwrap();
        let url: url::Url = url.parse().unwrap();
        let panel = w
            .as_ref()
            .window()
            .add_child(
                tauri::webview::WebviewBuilder::new(
                    crate::preview::LABEL,
                    WebviewUrl::External(url.clone()),
                ),
                tauri::LogicalPosition::new(0.0, 0.0),
                tauri::LogicalSize::new(100.0, 100.0),
            )
            .unwrap();
        invoke(&Panel(panel), url)
    }

    struct Panel(tauri::Webview<tauri::test::MockRuntime>);
    impl AsRef<tauri::Webview<tauri::test::MockRuntime>> for Panel {
        fn as_ref(&self) -> &tauri::Webview<tauri::test::MockRuntime> {
            &self.0
        }
    }

    fn invoke<W: AsRef<tauri::Webview<tauri::test::MockRuntime>>>(
        w: &W,
        url: url::Url,
    ) -> Result<String, String> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: "is_locked".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url,
                body: InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|b| format!("{:?}", b.deserialize::<serde_json::Value>().ok()))
        .map_err(|e| e.to_string())
    }

    /// The preview window loads untrusted pages: none of desk's commands
    /// (pty_write would type into a terminal) may be reachable from it.
    #[test]
    fn only_the_main_window_can_call_desk_commands() {
        // The app's own origin: tauri://localhost, http://tauri.localhost on Windows.
        let local = if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        };
        assert!(call("main", local).is_ok());
        // Granted by webview, not window: the panel shares the main window.
        assert!(call_panel(local).is_err());
        for url in [
            "deskpreview://localhost/t/mock.html",
            "https://example.com/",
        ] {
            let denied = call("preview", url);
            assert!(denied.is_err(), "{url}: {denied:?}");
            let denied = call_panel(url);
            assert!(denied.is_err(), "panel {url}: {denied:?}");
        }
    }
}
