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
    "project_files",
    "search_files",
    "reveal_path",
    "browser_name",
    "update_check",
    "update_install",
    "open_file",
    "preview_file",
    "run_targets",
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
    "dropdown_apply",
    "dropdown_leave",
    "open_preview",
    "preview_bounds",
    "preview_visible",
    "preview_close",
    "preview_nav",
    "read_text",
    "write_text",
    "open_external",
    "chat_dir",
    "usage",
    "list_agents",
    "create_project",
    "github_repos",
    "clone_project",
    "dropped_paths",
];

fn main() {
    let mut attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // tauri-build embeds the Common Controls manifest only in the app
        // binary; test binaries then fail to start (STATUS_ENTRYPOINT_NOT_FOUND).
        // Embed it in every binary instead.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
