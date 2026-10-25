// desk's own commands go through the ACL too (an app manifest), so only the
// capability in capabilities/default.json, granted to the "main" window, can
// call them. The link preview window loads untrusted pages and gets none.
const COMMANDS: &[&str] = &[
    "bootstrap",
    "open_session",
    "kill_session",
    "pty_attach",
    "pty_write",
    "pty_resize",
    "pty_close",
    "save_layout",
    "tile_info",
    "git_branches",
    "git_switch",
    "list_projects",
    "list_sessions",
    "coffee_status",
    "coffee_set",
    "coffee_restore",
    "coffee_uninstall",
    "lock_desk",
    "unlock_desk",
    "is_locked",
    "open_preview",
    "open_external",
    "chat_dir",
    "usage",
    "list_agents",
    "create_project",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
