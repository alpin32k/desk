//! The main window opens where it was closed: the same monitor, place and
//! size, kept in `window.json`. (tauri-plugin-window-state halved both with
//! a Retina laptop next to a non-Retina monitor.) On macOS the place is in
//! points, the coordinates all screens share; elsewhere in physical pixels,
//! which there are the shared ones. A place on no connected monitor is left
//! alone, and the window opens where it would anyway.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, Monitor, WebviewWindow};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A save is waiting for the window to stop moving.
static PENDING: Mutex<bool> = Mutex::new(false);

pub fn file(support_dir: &Path) -> PathBuf {
    support_dir.join("window.json")
}

pub fn restore(window: &WebviewWindow, file: &Path) {
    let Some(place) = load(file) else {
        return;
    };
    let screens: Vec<Place> = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(screen)
        .collect();
    if !on_a_screen(&place, &screens) {
        return;
    }
    if cfg!(target_os = "macos") {
        let _ = window.set_size(tauri::LogicalSize::new(place.width, place.height));
        let _ = window.set_position(tauri::LogicalPosition::new(place.x, place.y));
    } else {
        let _ = window.set_size(tauri::PhysicalSize::new(place.width as u32, place.height as u32));
        let _ = window.set_position(tauri::PhysicalPosition::new(place.x as i32, place.y as i32));
    }
}

/// The window moved or was resized: write its place once it has stopped
/// (half a second), read then, when macOS knows which screen it is on.
pub fn save_soon(app: &AppHandle, file: PathBuf) {
    {
        let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        if *pending {
            return;
        }
        *pending = true;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = false;
        // Locked, the window fills its screen: that is not where it lives.
        if app.state::<crate::lock::Lock>().is_locked() {
            return;
        }
        if let Some(place) = app.get_webview_window("main").and_then(|w| read(&w)) {
            let _ = save(&file, place);
        }
    });
}

fn read(window: &WebviewWindow) -> Option<Place> {
    if window.is_minimized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return None;
    }
    let pos = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    if cfg!(target_os = "macos") {
        let scale = window.scale_factor().ok()?;
        let pos = pos.to_logical::<f64>(scale);
        let size = size.to_logical::<f64>(scale);
        Some(Place { x: pos.x, y: pos.y, width: size.width, height: size.height })
    } else {
        Some(Place {
            x: pos.x as f64,
            y: pos.y as f64,
            width: size.width as f64,
            height: size.height as f64,
        })
    }
}

/// A monitor's rectangle in the same units as `Place`.
fn screen(m: &Monitor) -> Place {
    if cfg!(target_os = "macos") {
        let pos = m.position().to_logical::<f64>(m.scale_factor());
        let size = m.size().to_logical::<f64>(m.scale_factor());
        Place { x: pos.x, y: pos.y, width: size.width, height: size.height }
    } else {
        Place {
            x: m.position().x as f64,
            y: m.position().y as f64,
            width: m.size().width as f64,
            height: m.size().height as f64,
        }
    }
}

/// Enough of the window's top edge is on some screen to grab it there.
fn on_a_screen(place: &Place, screens: &[Place]) -> bool {
    const GRAB: f64 = 50.0;
    place.width >= GRAB
        && place.height >= GRAB
        && screens.iter().any(|s| {
            let left = place.x.max(s.x);
            let right = (place.x + place.width).min(s.x + s.width);
            right - left >= GRAB && place.y >= s.y && place.y < s.y + s.height - GRAB
        })
}

fn load(file: &Path) -> Option<Place> {
    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

fn save(file: &Path, place: Place) -> std::io::Result<()> {
    std::fs::write(file, serde_json::to_string(&place).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Place = Place { x: 0.0, y: 0.0, width: 1512.0, height: 982.0 };
    const MONITOR: Place = Place { x: -2560.0, y: -458.0, width: 2560.0, height: 1440.0 };

    #[test]
    fn opens_only_on_a_connected_screen() {
        let screens = [LAPTOP, MONITOR];
        let on_monitor = Place { x: -2200.0, y: -300.0, width: 1512.0, height: 882.0 };
        assert!(on_a_screen(&on_monitor, &screens));
        // The monitor is unplugged: the laptop alone does not hold that place.
        assert!(!on_a_screen(&on_monitor, &[LAPTOP]));
        // Title bar above the screen, or a sliver left on it.
        assert!(!on_a_screen(&Place { y: -100.0, ..on_monitor }, &[LAPTOP]));
        assert!(!on_a_screen(&Place { x: 1490.0, y: 100.0, ..on_monitor }, &[LAPTOP]));
        assert!(!on_a_screen(&Place { width: 10.0, ..on_monitor }, &screens));
    }

    #[test]
    fn keeps_the_place_in_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(tmp.path());
        assert_eq!(load(&f), None);
        let place = Place { x: -2200.5, y: -300.0, width: 1512.0, height: 882.0 };
        save(&f, place).unwrap();
        assert_eq!(load(&f), Some(place));
        std::fs::write(&f, "{").unwrap();
        assert_eq!(load(&f), None);
    }
}
