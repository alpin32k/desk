//! The main window opens where it was closed: the same monitor, place and
//! size, kept in `window.json`. (tauri-plugin-window-state halved both with
//! a Retina laptop next to a non-Retina monitor.) On macOS the place is in
//! points, the coordinates all screens share; elsewhere in physical pixels,
//! which there are the shared ones. A place on no connected monitor is left
//! alone, and the window opens where it would anyway. The dropdown
//! (dropdown.rs) places the window in the same units.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

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
    put(window, place);
}

/// Move the window there and give it this size, in that order: a resize
/// still on the way from a screen with another scale reaches the page after
/// the move, read with the new scale, and leaves it half (or twice) the
/// window. The top left corner stays where it was put.
pub(crate) fn put(window: &WebviewWindow, place: Place) {
    move_to(window, place.x, place.y);
    if cfg!(target_os = "macos") {
        let _ = window.set_size(tauri::LogicalSize::new(place.width, place.height));
    } else {
        let _ = window.set_size(tauri::PhysicalSize::new(place.width as u32, place.height as u32));
    }
}

/// Move the window's top left corner there. (Physical pixels on macOS would
/// be read with the scale of the screen the window is on now, not the one
/// it goes to: on the way from a Retina screen to another they halve.)
pub(crate) fn move_to(window: &WebviewWindow, x: f64, y: f64) {
    if cfg!(target_os = "macos") {
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    } else {
        let _ = window.set_position(tauri::PhysicalPosition::new(x.round() as i32, y.round() as i32));
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
        // Docked as the dropdown, it is at the top of the screen: neither.
        if app.try_state::<crate::dropdown::Dropdown>().is_some_and(|d| d.docked()) {
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
    here(window)
}

/// Where the window is now and how big.
pub(crate) fn here(window: &WebviewWindow) -> Option<Place> {
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
pub(crate) fn screen(m: &Monitor) -> Place {
    shared(m, *m.position(), *m.size())
}

/// The part of a monitor windows go in (without the menu bar, the Dock or
/// the taskbar), in the same units as `Place`.
pub(crate) fn work_area(m: &Monitor) -> Place {
    let area = m.work_area();
    shared(m, area.position, area.size)
}

/// Each monitor gives its pixels in its own scale; on macOS only its points
/// line up with the other screens'.
fn shared(m: &Monitor, pos: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> Place {
    if cfg!(target_os = "macos") {
        let pos = pos.to_logical::<f64>(m.scale_factor());
        let size = size.to_logical::<f64>(m.scale_factor());
        Place { x: pos.x, y: pos.y, width: size.width, height: size.height }
    } else {
        Place { x: pos.x as f64, y: pos.y as f64, width: size.width as f64, height: size.height as f64 }
    }
}

/// The mouse, in the same units as `Place`. (On macOS tao gives its points
/// times the main screen's scale.)
pub(crate) fn cursor(window: &WebviewWindow) -> Option<(f64, f64)> {
    let at = window.cursor_position().ok()?;
    if cfg!(target_os = "macos") {
        let scale = window.primary_monitor().ok().flatten()?.scale_factor();
        Some((at.x / scale, at.y / scale))
    } else {
        Some((at.x, at.y))
    }
}

/// The first of `screens` the point is on.
pub(crate) fn under(screens: &[Place], (x, y): (f64, f64)) -> Option<usize> {
    screens
        .iter()
        .position(|s| x >= s.x && x < s.x + s.width && y >= s.y && y < s.y + s.height)
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
    fn finds_the_screen_under_the_mouse() {
        // A Retina laptop with a non-Retina monitor to its right, reaching
        // above it.
        let right = Place { x: 1512.0, y: -303.0, width: 2560.0, height: 1440.0 };
        let screens = [LAPTOP, right];
        assert_eq!(under(&screens, (1000.0, 500.0)), Some(0));
        assert_eq!(under(&screens, (1512.0, -100.0)), Some(1));
        assert_eq!(under(&screens, (4000.0, 1100.0)), Some(1));
        // Under the laptop, where the monitor does not reach.
        assert_eq!(under(&screens, (1000.0, 1000.0)), None);
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
