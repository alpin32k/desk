// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Claude Code calls desk itself for hooks and the status line on Windows
    // (see hooks::settings_json_exe); those runs never open a window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--desk-hook") => desk_lib::hooks::run_hook(args.get(1).map_or("", String::as_str)),
        Some("--desk-statusline") => desk_lib::usage::run_statusline(),
        _ => desk_lib::run(),
    }
}
