// First: its tr! macro is used by the modules below.
#[macro_use]
pub mod i18n;
pub mod agents;
pub mod backend;
pub mod browser;
pub mod claude_projects;
pub mod coffee;
pub mod conversations;
pub mod discovery;
pub mod drag;
pub mod dropdown;
pub mod env;
pub mod git;
pub mod github;
pub mod hooks;
pub mod layout;
pub mod lock;
#[cfg(target_os = "macos")]
mod mac_keys;
pub mod native;
pub mod new_project;
#[cfg(target_os = "linux")]
mod pam;
pub mod preview;
pub mod project;
pub mod pty;
pub mod state_watch;
pub mod tmux;
mod window_place;
pub mod update;
pub mod usage;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::ipc::{Channel, Response};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, State};

/// Coffee state lives in its own managed value so the guard thread can reach it.
pub struct AppCoffee(pub coffee::Coffee);

struct App {
    sessions: backend::Backend,
    hooks: hooks::HookFiles,
    layout_path: PathBuf,
    /// tmux's right-click menu is on (Options); see `Tmux::reload_config`.
    tmux_menu: AtomicBool,
    _watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Bootstrap {
    layout: layout::Layout,
    sessions: Vec<tmux::SessionInfo>,
    states: Vec<hooks::SessionState>,
    home: String,
    /// Where "new project" suggests creating folders until one is chosen.
    projects_dir: String,
    /// Sessions outlive desk (tmux); false on Windows, where they end with it.
    persistent: bool,
    /// macOS or Linux without tmux: native sessions until it is installed.
    tmux_missing: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TileInfo {
    path: String,
    branch: Option<String>,
}

#[tauri::command]
fn bootstrap(app: State<App>) -> Bootstrap {
    let layout = layout::load(&app.layout_path);
    i18n::set(&layout.language);
    Bootstrap {
        layout,
        sessions: app.sessions.list(),
        persistent: app.sessions.persistent(),
        tmux_missing: app.sessions.tmux_missing(),
        states: state_watch::read_all(&app.hooks.state_dir),
        home: env::home().display().to_string(),
        projects_dir: new_project::default_parent(&env::home())
            .display()
            .to_string(),
    }
}

#[tauri::command]
fn list_sessions(app: State<App>) -> Vec<tmux::SessionInfo> {
    app.sessions.list()
}

/// ⌘S: the project's newest Claude conversations, to resume one.
#[tauri::command(async)]
fn recent_conversations(path: String) -> Vec<conversations::Conversation> {
    conversations::recent(&env::home().join(".claude/projects"), Path::new(&path), 40)
}

/// Ensure the tmux session for project + agent + sub-session exists; returns its slug.
#[tauri::command]
fn open_session(
    app: State<App>,
    path: String,
    agent: String,
    instance: u32,
    command: Option<String>,
    resume: Option<String>,
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
            return Err(tr!(
                format!("{path} nie definiuje polecenia „{command}”"),
                format!("{path} does not define the command “{command}”")
            ));
        }
        command
    } else if let Some(id) = resume {
        // ⌘S: a conversation picked up again; its id goes into the command line.
        if agent != "claude" || !conversations::is_session_id(&id) {
            return Err(format!("cannot resume {agent} {id}"));
        }
        format!("claude --resume {id}")
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
async fn github_repos() -> Result<Vec<github::Repo>, String> {
    tauri::async_runtime::spawn_blocking(github::repos)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn clone_project(parent: String, name: String, repo: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        github::clone(Path::new(&parent), &name, &repo).map(|p| p.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
        return Err(tr!("desk jest zablokowany", "desk is locked").into());
    }
    app.sessions.write(id, data.as_bytes())
}

#[tauri::command]
fn lock_desk(handle: AppHandle, lock: State<lock::Lock>) -> Result<(), String> {
    lock.lock(&handle)
}

/// Shows the system's dialog (Touch ID, Windows Hello) or checks the typed
/// password (Linux), so it runs off the main thread.
#[tauri::command(async)]
fn unlock_desk(
    handle: AppHandle,
    lock: State<lock::Lock>,
    password: Option<String>,
) -> Result<(), String> {
    lock.unlock(&handle, password.as_deref())
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
fn read_text(path: String) -> Result<String, String> {
    preview::read_text(Path::new(&path))
}

#[tauri::command]
fn write_text(path: String, text: String) -> Result<(), String> {
    preview::write_text(Path::new(&path), &text)
}

#[tauri::command]
fn preview_nav(handle: AppHandle, action: String) -> Result<(), String> {
    preview::navigate(&handle, &action)
}

/// Plan limits for the given agents: local files, plus Claude's account usage every 5 minutes.
#[tauri::command(async)]
async fn usage(app: State<'_, App>, agents: Vec<String>) -> Result<Vec<usage::Usage>, String> {
    // Off the main thread: every 5 minutes this asks Anthropic (up to 10 s).
    let state = app.hooks.state_dir.clone();
    tauri::async_runtime::spawn_blocking(move || usage::usage(&agents, &state, &env::home()))
        .await
        .map_err(|e| e.to_string())
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
fn open_external(url: String, browser: Option<String>) -> Result<(), String> {
    preview::open_external(&url, browser.as_deref().unwrap_or("auto"))
}

/// Options changed the dropdown (⌘`): register or drop the shortcut, refit the window.
#[tauri::command]
fn dropdown_apply(handle: AppHandle) -> Result<(), String> {
    dropdown::apply(&handle)
}

/// The button in place of the traffic lights: from the dropdown back to the window.
#[tauri::command]
fn dropdown_leave(handle: AppHandle) {
    dropdown::leave(&handle);
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
    if app.tmux_menu.swap(layout.tmux_menu, Ordering::Relaxed) != layout.tmux_menu {
        app.sessions.reload_config(layout.tmux_menu);
    }
    i18n::set(&layout.language);
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
async fn search_files(path: String, query: String) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || project::search(Path::new(&path), &query))
        .await
        .unwrap_or_default()
}

/// The browser web pages go to ("Brave", "Chrome"), or None for the system's.
#[tauri::command]
fn browser_name(browser: String) -> Option<&'static str> {
    browser::find(&browser).map(|b| b.name)
}

/// A newer desk release, if any (an error without the GitHub CLI).
#[tauri::command]
async fn update_check() -> Result<Option<update::Update>, String> {
    tauri::async_runtime::spawn_blocking(update::check)
        .await
        .map_err(|e| e.to_string())?
}

/// Download and put `version` in place, start it and quit.
#[tauri::command]
async fn update_install(handle: AppHandle, version: String) -> Result<(), String> {
    let start = tauri::async_runtime::spawn_blocking(move || update::install(&version))
        .await
        .map_err(|e| e.to_string())??;
    let mut cmd = if cfg!(windows) {
        let (exe, args) = start.split_first().ok_or(tr!("nic do uruchomienia", "nothing to run"))?;
        let mut c = std::process::Command::new(exe);
        c.args(args);
        c
    } else {
        // Start the new desk once this one is gone (arguments stay arguments).
        let mut c = std::process::Command::new("/bin/sh");
        c.arg("-c")
            .arg(r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; shift; exec "$@""#)
            .arg("sh")
            .arg(std::process::id().to_string())
            .args(&start);
        c
    };
    cmd.spawn().map_err(|e| format!("start: {e}"))?;
    handle.exit(0);
    Ok(())
}

#[tauri::command]
fn dropped_paths() -> Vec<String> {
    drag::paths()
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    project::reveal(Path::new(&path))
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
            // No ⌘Q: that closes a tile (see mac_keys.rs).
            &MenuItem::with_id(app, "quit", "Quit desk", true, None::<&str>)?,
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
        // The system clipboard for the terminals: the webview refuses script
        // writes (OSC 52 from tmux) and reads outside a click.
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        dropdown::toggle(app);
                    }
                })
                .build(),
        )
        .register_uri_scheme_protocol(preview::SCHEME, |_ctx, req| preview::serve(&req));
    // A menu only on macOS: on Windows and Linux it is a bar in the window whose
    // Edit > Copy (Ctrl+C) would take Ctrl+C away from the terminals.
    let builder = if cfg!(target_os = "macos") {
        builder.menu(|app| menu(app, false)).on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
            }
        })
    } else {
        builder
    };
    builder
        .setup(|app| {
            let support = env::support_dir();
            let sessions = backend::Backend::new(app.handle(), &support)?;
            let saved = layout::load(&support.join("layout.json"));
            i18n::set(&saved.language);
            let ids: Vec<String> = agents::list(&saved.agents)
                .into_iter()
                .map(|a| a.id)
                .collect();
            sessions.retag_untagged(&ids);
            sessions.clean_env();
            sessions.reload_config(saved.tmux_menu);
            let hooks = hooks::install(&support)?;
            let watcher = state_watch::watch(app.handle().clone(), &hooks.state_dir).ok();
            app.manage(lock::Lock::default());
            #[cfg(target_os = "macos")]
            mac_keys::install(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                window_place::restore(&w, &window_place::file(&support));
            }
            app.manage(AppCoffee(coffee::Coffee::new(&support)));
            coffee::spawn_guard(app.handle().clone());
            app.manage(App {
                sessions,
                hooks,
                layout_path: support.join("layout.json"),
                tmux_menu: AtomicBool::new(saved.tmux_menu),
                _watcher: Mutex::new(watcher),
            });
            app.manage(dropdown::Dropdown::new(support.join("layout.json")));
            // Taken by another app: desk still starts, Options shows why.
            if let Err(e) = dropdown::apply(app.handle()) {
                eprintln!("dropdown: {e}");
            }
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
            search_files,
            reveal_path,
            browser_name,
            update_check,
            update_install,
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
            dropdown_apply,
            dropdown_leave,
            open_preview,
            preview_bounds,
            preview_visible,
            preview_close,
            preview_nav,
            read_text,
            write_text,
            open_external,
            chat_dir,
            usage,
            list_agents,
            create_project,
            github_repos,
            clone_project,
            dropped_paths,
            recent_conversations,
        ])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if matches!(event, tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)) {
                window_place::save_soon(window.app_handle(), window_place::file(&env::support_dir()));
            }
            let locked = window.state::<lock::Lock>().is_locked();
            if matches!(event, tauri::WindowEvent::Focused(false)) && !locked {
                dropdown::blurred(window.app_handle());
            }
            if matches!(event, tauri::WindowEvent::Resized(_)) {
                dropdown::resized(window.app_handle());
            }
            if matches!(event, tauri::WindowEvent::Moved(_)) {
                dropdown::moved(window.app_handle());
            }
            if !locked {
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

    fn call(label: &str, url: &str, cmd: &str) -> Result<String, String> {
        let app = mock_builder()
            .manage(super::lock::Lock::default())
            .invoke_handler(tauri::generate_handler![super::is_locked])
            .build(super::context())
            .unwrap();
        let url: url::Url = url.parse().unwrap();
        let w = WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url.clone()))
            .build()
            .unwrap();
        invoke(&w, url, cmd)
    }

    /// The side panel: a second webview inside the main window.
    fn call_panel(url: &str, cmd: &str) -> Result<String, String> {
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
        invoke(&Panel(panel), url, cmd)
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
        cmd: &str,
    ) -> Result<String, String> {
        get_ipc_response(
            w,
            InvokeRequest {
                cmd: cmd.into(),
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
    /// The app's own origin: tauri://localhost, http://tauri.localhost on Windows.
    const LOCAL: &str = if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };

    /// Pages that are not desk's: a previewed file, a web page.
    const UNTRUSTED: [&str; 2] = ["deskpreview://localhost/t/mock.html", "https://example.com/"];

    #[test]
    fn only_the_main_window_can_call_desk_commands() {
        assert!(call("main", LOCAL, "is_locked").is_ok());
        // Granted by webview, not window: the panel shares the main window.
        assert!(call_panel(LOCAL, "is_locked").is_err());
        for url in UNTRUSTED {
            let denied = call("preview", url, "is_locked");
            assert!(denied.is_err(), "{url}: {denied:?}");
            let denied = call_panel(url, "is_locked");
            assert!(denied.is_err(), "panel {url}: {denied:?}");
        }
    }

    /// dropdown_apply registers a global shortcut (a key taken from every
    /// app) and dropdown_leave moves the window: the ACL lets only desk's own
    /// page ask for them.
    #[test]
    fn only_the_main_window_can_set_the_dropdown_shortcut() {
        let denied = |r: Result<String, String>| r.is_err_and(|e| e.contains("not allowed"));
        for cmd in ["dropdown_apply", "dropdown_leave"] {
            // Past the ACL (this mock app has no handler for it, so "not found").
            let main = call("main", LOCAL, cmd);
            assert!(main.as_ref().is_err_and(|e| e.contains("not found")), "{cmd}: {main:?}");
            assert!(denied(call_panel(LOCAL, cmd)), "{cmd}");
            for url in UNTRUSTED {
                assert!(denied(call("preview", url, cmd)), "{cmd} {url}");
                assert!(denied(call_panel(url, cmd)), "{cmd} panel {url}");
            }
        }
    }
}
