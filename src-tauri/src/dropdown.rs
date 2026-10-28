//! Dropdown, like iTerm's hotkey window: ⌘` (Ctrl+` on Windows and Linux, or
//! any shortcut set in Options) slides desk's window down from the top of the
//! screen under the mouse, and pressing it again slides it back up and gives
//! the focus back to the app you were in. While dropped down the window spans
//! the screen's width under the menu bar, floats above other apps, follows you
//! to every Space and hides when another app takes the focus (Options can turn
//! that off). Its height is kept when you drag its edge; it cannot be moved
//! off the top. The button in place of the traffic lights, or turning the
//! shortcut off, puts the window back where it was.
//!
//! On macOS desk turns into an accessory app while docked, like iTerm's
//! hotkey window: the menu bar keeps the menus of the app you were in, and
//! desk leaves the Dock and ⌘Tab (its window cannot become an NSPanel: tao's
//! window class carries its own state).
//!
//! Nothing happens while desk is locked: the lock keeps the window in front.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewWindow,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// One step of the slide, about two frames.
const FRAME: Duration = Duration::from_millis(8);
/// Time for the focus to arrive after taking it again.
const FOCUS_SETTLE: Duration = Duration::from_millis(100);
/// How long becoming an accessory app takes to deactivate desk, at most.
const POLICY_SETTLE: Duration = Duration::from_millis(250);

/// ⌘` on macOS, Ctrl+` elsewhere (with Shift, ` is ~ on most layouts).
pub const DEFAULT_HOTKEY: &str = if cfg!(target_os = "macos") {
    "Super+Backquote"
} else {
    "Control+Backquote"
};

/// The window's minimum size while docked, so its edge can make it short
/// (tauri.conf.json has the usual one).
const DOCKED_MIN: LogicalSize<f64> = LogicalSize::new(400.0, 120.0);
const NORMAL_MIN: LogicalSize<f64> = LogicalSize::new(800.0, 500.0);

pub struct Dropdown {
    layout_path: PathBuf,
    /// The window's place before it first dropped down; Some while docked.
    normal: Mutex<Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>>,
    /// A slide is running; presses in the meantime are dropped.
    sliding: AtomicBool,
    /// The shortcut registered now.
    hotkey: Mutex<Option<Shortcut>>,
}

impl Dropdown {
    pub fn new(layout_path: PathBuf) -> Self {
        Self {
            layout_path,
            normal: Mutex::new(None),
            sliding: AtomicBool::new(false),
            hotkey: Mutex::new(None),
        }
    }

    fn options(&self) -> crate::layout::Layout {
        crate::layout::load(&self.layout_path)
    }

    pub(crate) fn docked(&self) -> bool {
        self.normal.lock().unwrap().is_some()
    }
}

/// Register the shortcut as Options say, and fit or release a docked window
/// (the height changed, or the shortcut was turned off).
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let dropdown = app.state::<Dropdown>();
    let options = dropdown.options();
    let wanted = if options.dropdown {
        let text = if options.dropdown_hotkey.is_empty() {
            DEFAULT_HOTKEY
        } else {
            &options.dropdown_hotkey
        };
        Some(
            text.parse::<Shortcut>()
                .map_err(|e| format!("skrót {text}: {e}"))?,
        )
    } else {
        None
    };
    {
        let mut hotkey = dropdown.hotkey.lock().unwrap();
        if *hotkey != wanted {
            let shortcuts = app.global_shortcut();
            if let Some(old) = hotkey.take() {
                let _ = shortcuts.unregister(old);
            }
            if let Some(new) = wanted {
                // Taken by another app (iTerm's hotkey...): say so.
                shortcuts
                    .register(new)
                    .map_err(|e| format!("skrót {}: {e}", new.into_string()))?;
                *hotkey = Some(new);
            }
        }
    }
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    if !dropdown.docked() {
        return Ok(());
    }
    if options.dropdown {
        if window.is_visible().unwrap_or(false) {
            let (at, size) = frame(app, &window, options.dropdown_height);
            let _ = window.set_position(at);
            let _ = window.set_size(size);
        }
    } else {
        undock(app, &window);
    }
    Ok(())
}

/// The shortcut was pressed: down if desk is not in front, up if it is.
pub fn toggle(app: &AppHandle) {
    if app.state::<crate::lock::Lock>().is_locked() {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let dropdown = app.state::<Dropdown>();
    let shown = dropdown.docked()
        && window.is_visible().unwrap_or(false)
        && !window.is_minimized().unwrap_or(false)
        && window.is_focused().unwrap_or(false);
    if shown {
        slide_up(app, window);
    } else {
        slide_down(app, window);
    }
}

/// The window lost the focus: when another app took it (not a dialog of
/// desk's own), the dropdown goes up.
pub fn blurred(app: &AppHandle) {
    let dropdown = app.state::<Dropdown>();
    if !dropdown.docked() || dropdown.sliding.load(Ordering::SeqCst) {
        return;
    }
    if !dropdown.options().dropdown_autohide {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // The focus may only be moving to a sheet or the file dialog.
        std::thread::sleep(Duration::from_millis(150));
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if app.state::<crate::lock::Lock>().is_locked()
            || !app.state::<Dropdown>().docked()
            || !window.is_visible().unwrap_or(false)
            || window.is_focused().unwrap_or(false)
            || app_active(&app)
        {
            return;
        }
        slide_up(&app, window);
    });
}

/// You dragged the docked window's edge: the page keeps the new height
/// ("dropdown-height", percent of the screen) for the next time.
pub fn resized(app: &AppHandle, size: PhysicalSize<u32>) {
    let Some(window) = held(app) else {
        return;
    };
    pin(&window);
    let Some(screen) = window.current_monitor().ok().flatten() else {
        return;
    };
    let area = screen.work_area().size.height.max(1);
    let pct = (f64::from(size.height) * 100.0 / f64::from(area)).round() as u32;
    if pct.clamp(20, 100) != app.state::<Dropdown>().options().dropdown_height {
        let _ = app.emit("dropdown-height", pct.clamp(20, 100));
    }
}

/// The docked window was moved (a drag, or its top or side edge).
pub fn moved(app: &AppHandle) {
    if let Some(window) = held(app) {
        pin(&window);
    }
}

/// The docked window, while it rests (not sliding, desk not locked).
fn held(app: &AppHandle) -> Option<WebviewWindow> {
    let dropdown = app.state::<Dropdown>();
    if !dropdown.docked()
        || dropdown.sliding.load(Ordering::SeqCst)
        || app.state::<crate::lock::Lock>().is_locked()
    {
        return None;
    }
    app.get_webview_window("main")
}

/// Keep the docked window at the top of its screen, full width: only its
/// bottom edge changes how tall it is (dragging the top one ends the same).
fn pin(window: &WebviewWindow) {
    let Some(screen) = window.current_monitor().ok().flatten() else {
        return;
    };
    let area = screen.work_area();
    let (Ok(at), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    if at != area.position {
        let _ = window.set_position(area.position);
    }
    if size.width != area.size.width {
        let _ = window.set_size(PhysicalSize::new(area.size.width, size.height));
    }
}

/// The button in place of the traffic lights: back to the ordinary window.
/// The shortcut stays on and brings the dropdown down again.
pub fn leave(app: &AppHandle) {
    if app.state::<Dropdown>().sliding.load(Ordering::SeqCst) {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        undock(app, &window);
    }
}

/// Where the dropdown goes: the top of the work area (under the menu bar) of
/// the screen with the mouse, full width, `height` percent tall.
fn frame(
    app: &AppHandle,
    window: &WebviewWindow,
    height: u32,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let screen = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(screen) = screen else {
        let size = window.inner_size().unwrap_or_default();
        return (PhysicalPosition::new(0, 0), size);
    };
    let area = screen.work_area();
    let pct = height.clamp(20, 100);
    let size = PhysicalSize::new(area.size.width, area.size.height * pct / 100);
    (area.position, size)
}

fn slide_down(app: &AppHandle, window: WebviewWindow) {
    let dropdown = app.state::<Dropdown>();
    if dropdown.sliding.swap(true, Ordering::SeqCst) {
        return;
    }
    let options = dropdown.options();
    let docking = {
        let mut normal = dropdown.normal.lock().unwrap();
        let docking = normal.is_none();
        if docking {
            let at = window.outer_position().unwrap_or_default();
            let size = window.inner_size().unwrap_or_default();
            *normal = Some((at, size));
            float(app, &window, true);
        }
        docking
    };
    let duration = Duration::from_millis(options.dropdown_animation.into());
    let (at, size) = frame(app, &window, options.dropdown_height);
    let above = PhysicalPosition::new(at.x, at.y - size.height as i32);
    let _ = window.unminimize();
    let _ = window.set_position(above);
    let _ = window.set_size(size);
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = window.show();
    let _ = window.set_focus();
    let app = app.clone();
    std::thread::spawn(move || {
        slide(&window, above, at, duration);
        // Becoming an accessory app (the first time down) deactivates desk a
        // moment after it took the focus: take it again while still sliding,
        // or losing it would send the dropdown straight back up.
        if docking {
            std::thread::sleep(POLICY_SETTLE.saturating_sub(duration));
        }
        if !window.is_focused().unwrap_or(false) {
            let _ = window.set_focus();
            std::thread::sleep(FOCUS_SETTLE);
        }
        app.state::<Dropdown>().sliding.store(false, Ordering::SeqCst);
    });
}

fn slide_up(app: &AppHandle, window: WebviewWindow) {
    let dropdown = app.state::<Dropdown>();
    if dropdown.sliding.swap(true, Ordering::SeqCst) {
        return;
    }
    let duration = Duration::from_millis(dropdown.options().dropdown_animation.into());
    let app = app.clone();
    std::thread::spawn(move || {
        let at = window.outer_position().unwrap_or_default();
        let height = window.outer_size().unwrap_or_default().height as i32;
        slide(&window, at, PhysicalPosition::new(at.x, at.y - height), duration);
        // Hiding the app hands the focus back to the one you were in.
        #[cfg(target_os = "macos")]
        let _ = app.hide();
        #[cfg(not(target_os = "macos"))]
        let _ = window.hide();
        let _ = window.set_position(at);
        app.state::<Dropdown>().sliding.store(false, Ordering::SeqCst);
    });
}

/// Move the window from `from` to `to` in `duration` (none: at once), fast
/// at first and slowing down.
fn slide(
    window: &WebviewWindow,
    from: PhysicalPosition<i32>,
    to: PhysicalPosition<i32>,
    duration: Duration,
) {
    let steps = (duration.as_millis() / FRAME.as_millis()).max(1) as u32;
    for step in 1..=steps {
        let y = f64::from(from.y) + f64::from(to.y - from.y) * ease(step, steps);
        let _ = window.set_position(PhysicalPosition::new(to.x, y.round() as i32));
        if step < steps {
            std::thread::sleep(duration / steps);
        }
    }
}

/// How far along the slide is after `step` of `steps` (0 to 1, ease-out).
fn ease(step: u32, steps: u32) -> f64 {
    let t = f64::from(step) / f64::from(steps);
    1.0 - (1.0 - t).powi(3)
}

/// Back to the window desk had before the first ⌘`.
fn undock(app: &AppHandle, window: &WebviewWindow) {
    let Some((at, size)) = app.state::<Dropdown>().normal.lock().unwrap().take() else {
        return;
    };
    float(app, window, false);
    let _ = window.set_position(at);
    let _ = window.set_size(size);
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Above other apps' windows and on every Space (on macOS also allowed next
/// to an app in full screen), or back to an ordinary window. The page hears
/// about it ("dropdown"): the top bar keeps no room for the traffic lights.
fn float(app: &AppHandle, window: &WebviewWindow, on: bool) {
    let _ = window.set_always_on_top(on);
    let _ = window.set_visible_on_all_workspaces(on);
    let _ = window.set_min_size(Some(if on { DOCKED_MIN } else { NORMAL_MIN }));
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy as P;
        let _ = app.set_activation_policy(if on { P::Accessory } else { P::Regular });
        dropdown_window(window, on);
    }
    let _ = app.emit("dropdown", on);
}

/// No traffic lights while docked (like iTerm's hotkey window), free to slide
/// above the screen's top edge, and allowed on a Space with an app in full
/// screen.
#[cfg(target_os = "macos")]
fn dropdown_window(window: &WebviewWindow, on: bool) {
    use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowCollectionBehavior as B};
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ptr = ptr as usize;
    unconstrained::set(on.then_some(ptr));
    let _ = window.run_on_main_thread(move || {
        // SAFETY: the NSWindow of desk's main window, used on the main thread.
        let ns = unsafe { &*(ptr as *const NSWindow) };
        let mut behavior = ns.collectionBehavior();
        behavior.set(B::FullScreenAuxiliary, on);
        ns.setCollectionBehavior(behavior);
        // Docked it stays at the top: no drag moves it (dropdown::pin
        // catches its edges).
        ns.setMovable(!on);
        for button in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(b) = ns.standardWindowButton(button) {
                b.setHidden(on);
            }
        }
    });
}

/// macOS keeps a window with a title bar below the menu bar, even when it is
/// moved by code, so the slide would never show. TaoWindow (tao's window
/// class) gets its own `constrainFrameRect:toScreen:` that lets the docked
/// window go where it is put and leaves every other window to NSWindow's.
#[cfg(target_os = "macos")]
mod unconstrained {
    use objc2::encode::Encode;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{ClassType, msg_send, sel};
    use objc2_app_kit::NSWindow;
    use objc2_foundation::NSRect;
    use std::sync::Once;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// The docked window's NSWindow, or 0.
    static DOCKED: AtomicUsize = AtomicUsize::new(0);

    pub fn set(window: Option<usize>) {
        static ADD: Once = Once::new();
        ADD.call_once(add_method);
        DOCKED.store(window.unwrap_or(0), Ordering::SeqCst);
    }

    unsafe extern "C-unwind" fn constrain(
        this: &AnyObject,
        _cmd: Sel,
        frame: NSRect,
        screen: Option<&AnyObject>,
    ) -> NSRect {
        if std::ptr::from_ref(this) as usize == DOCKED.load(Ordering::SeqCst) {
            return frame;
        }
        // SAFETY: NSWindow implements it; `this` is a TaoWindow, an NSWindow subclass.
        unsafe { msg_send![super(this, NSWindow::class()), constrainFrameRect: frame, toScreen: screen] }
    }

    fn add_method() {
        let Some(class) = AnyClass::get(c"TaoWindow") else {
            return;
        };
        let rect = NSRect::ENCODING;
        let types = std::ffi::CString::new(format!("{rect}@:{rect}@")).unwrap();
        type Constrain =
            unsafe extern "C-unwind" fn(&AnyObject, Sel, NSRect, Option<&AnyObject>) -> NSRect;
        // SAFETY: TaoWindow does not define this method, so it is added (not
        // replaced); the function matches the selector's signature and `types`.
        unsafe {
            let imp = std::mem::transmute::<Constrain, Imp>(constrain);
            objc2::ffi::class_addMethod(
                std::ptr::from_ref(class).cast_mut(),
                sel!(constrainFrameRect:toScreen:),
                imp,
                types.as_ptr(),
            );
        }
    }
}

/// desk is the active app (a dialog of its own may have the focus).
#[cfg(target_os = "macos")]
fn app_active(app: &AppHandle) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            let _ = tx.send(NSApplication::sharedApplication(mtm).isActive());
        }
    });
    rx.recv_timeout(Duration::from_secs(1)).unwrap_or(true)
}

#[cfg(not(target_os = "macos"))]
fn app_active(app: &AppHandle) -> bool {
    app.webview_windows()
        .values()
        .any(|w| w.is_focused().unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slide_starts_fast_and_lands_exactly() {
        assert_eq!(ease(10, 10), 1.0);
        assert!(ease(1, 10) > 0.25, "first step covers a quarter");
        assert!(ease(5, 10) - ease(4, 10) > ease(10, 10) - ease(9, 10));
    }

    #[test]
    fn default_hotkey_parses() {
        let s: Shortcut = DEFAULT_HOTKEY.parse().unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::Backquote);
    }
}
