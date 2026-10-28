//! Lock: the screen stays on and the terminals stay visible, but nothing can
//! be typed into them until the owner unlocks: Touch ID or the account
//! password on macOS (LocalAuthentication), Windows Hello (PIN, fingerprint,
//! face) on Windows, the account password (PAM) on Linux. While locked:
//! - `pty_write` refuses input (enforced here, not only in the UI),
//! - the window covers the screen (macOS kiosk options also hide the Dock and
//!   menu bar and disable app switching, Force Quit and log out),
//! - every other screen goes dark under a window that stays on top, takes the
//!   clicks and never the focus; screens plugged in while locked get one too.
//!   On macOS it sits above the Dock and the menu bar and just below the
//!   Touch ID dialog, which may open on that screen,
//! - quitting desk or closing its window is refused, the menu shrinks to
//!   Edit > Copy, and the window comes back to the front if it loses focus
//!   or gets minimized.
//!
//! This guards desk's sessions, not the whole computer: the tmux sessions on
//! the `desk` socket are still reachable from any other program that runs as
//! you. For the whole machine use the system's lock screen.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Labels of the windows over the other screens: `lock-cover-0`, `-1`, ...
const COVER: &str = "lock-cover";

#[derive(Default)]
pub struct Lock {
    locked: AtomicBool,
    /// The Touch ID / password dialog is up and must keep the focus.
    authenticating: AtomicBool,
    /// Held while the covers change, so unlocking cannot race the watcher.
    covers: Mutex<()>,
}

impl Lock {
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    /// Locked and not showing the unlock dialog: desk keeps itself in front.
    pub fn holds_focus(&self) -> bool {
        self.is_locked() && !self.authenticating.load(Ordering::SeqCst)
    }

    pub fn lock(&self, app: &AppHandle) -> Result<(), String> {
        if self.locked.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if cfg!(target_os = "macos") {
            let _ = crate::menu(app, true).and_then(|m| app.set_menu(m));
        }
        kiosk(app, true);
        // Off the main thread: a command that builds windows on it deadlocks on
        // Windows. It also follows screens plugged in or out while locked.
        let app = app.clone();
        std::thread::spawn(move || {
            loop {
                let lock = app.state::<Lock>();
                let _covers = lock.covers.lock().unwrap_or_else(|e| e.into_inner());
                if !lock.is_locked() {
                    break;
                }
                cover_screens(&app, true);
                drop(_covers);
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Ok(())
    }

    /// Blocks until the user authenticates or cancels. `password` is the
    /// account password typed on the lock screen (Linux); macOS and Windows
    /// show their own dialog.
    pub fn unlock(&self, app: &AppHandle, password: Option<&str>) -> Result<(), String> {
        if !self.is_locked() {
            return Ok(());
        }
        self.authenticating.store(true, Ordering::SeqCst);
        let result = authenticate(app, tr!("odblokować desk", "unlock desk"), password);
        self.authenticating.store(false, Ordering::SeqCst);
        result?;
        self.locked.store(false, Ordering::SeqCst);
        {
            let _covers = self.covers.lock().unwrap_or_else(|e| e.into_inner());
            cover_screens(app, false);
        }
        kiosk(app, false);
        if cfg!(target_os = "macos") {
            let _ = crate::menu(app, false).and_then(|m| app.set_menu(m));
        }
        Ok(())
    }
}

/// One dark window over each screen except the one desk's window is on, or
/// none (`on` false). Windows already in place are only moved, so calling it
/// again does not flicker.
fn cover_screens(app: &AppHandle, on: bool) {
    let mut screens = Vec::new();
    if on && let Some(main) = app.get_webview_window("main") {
        let here = main.current_monitor().ok().flatten().map(|m| *m.position());
        screens = main.available_monitors().unwrap_or_default();
        screens.retain(|m| Some(*m.position()) != here);
    }
    let labels: Vec<String> = (0..screens.len()).map(|i| format!("{COVER}-{i}")).collect();
    for (label, window) in app.windows() {
        if label.starts_with(COVER) && !labels.contains(&label) {
            let _ = window.destroy();
        }
    }
    for (screen, label) in screens.iter().zip(&labels) {
        let window = match app.get_window(label) {
            Some(w) => w,
            None => {
                let at = screen.position().to_logical::<f64>(screen.scale_factor());
                let size = screen.size().to_logical::<f64>(screen.scale_factor());
                match tauri::window::WindowBuilder::new(app, label)
                    .title("desk")
                    .position(at.x, at.y)
                    .inner_size(size.width, size.height)
                    .decorations(false)
                    .shadow(false)
                    .resizable(false)
                    .always_on_top(true)
                    .visible_on_all_workspaces(true)
                    .skip_taskbar(true)
                    .focused(false)
                    .focusable(false)
                    .background_color(tauri::window::Color(11, 13, 16, 255))
                    .build()
                {
                    Ok(w) => {
                        above_everything(&w);
                        w
                    }
                    Err(_) => continue,
                }
            }
        };
        // Exact pixels, now that the window sits on its screen.
        let _ = window.set_position(*screen.position());
        let _ = window.set_size(*screen.size());
    }
}

/// Tauri's always-on-top level is below the Dock and the menu bar, which then
/// stay clickable on a covered screen. One below the screen saver's level is
/// above them and above menus, and still under the Touch ID dialog (coreautha
/// shows it at the screen saver's level), which may open on a covered screen.
#[cfg(target_os = "macos")]
fn above_everything(window: &tauri::Window) {
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior as B};
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    let ptr = ptr as usize;
    let _ = window.run_on_main_thread(move || {
        // SAFETY: the NSWindow of a window desk keeps open; used on the main thread.
        let ns = unsafe { &*(ptr as *const NSWindow) };
        ns.setLevel(NSScreenSaverWindowLevel - 1);
        ns.setCollectionBehavior(
            B::CanJoinAllSpaces | B::Stationary | B::FullScreenAuxiliary | B::IgnoresCycle,
        );
    });
}

#[cfg(not(target_os = "macos"))]
fn above_everything(_window: &tauri::Window) {}

#[cfg(target_os = "macos")]
fn kiosk(app: &AppHandle, on: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions as O};
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_simple_fullscreen(on);
        let _ = w.set_focus();
    }
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let opts = if on {
            O::HideDock
                | O::HideMenuBar
                | O::DisableProcessSwitching
                | O::DisableForceQuit
                | O::DisableSessionTermination
                | O::DisableHideApplication
                | O::DisableAppleMenu
        } else {
            O::Default
        };
        NSApplication::sharedApplication(mtm).setPresentationOptions(opts);
    });
}

/// Windows, Linux: the window fills the screen while locked.
#[cfg(not(target_os = "macos"))]
fn kiosk(app: &AppHandle, on: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_fullscreen(on);
        let _ = w.set_focus();
    }
}

/// Touch ID, Apple Watch or the account password, via the system dialog.
#[cfg(target_os = "macos")]
fn authenticate(_app: &AppHandle, reason: &str, _password: Option<&str>) -> Result<(), String> {
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let reply = block2::RcBlock::new(move |ok: Bool, err: *mut NSError| {
        let result = if ok.as_bool() {
            Ok(())
        } else if err.is_null() {
            Err(tr!("nie udało się odblokować", "could not unlock").into())
        } else {
            // SAFETY: LocalAuthentication passes a valid NSError when ok is false.
            Err(unsafe { &*err }.localizedDescription().to_string())
        };
        let _ = tx.send(result);
    });
    // SAFETY: plain LocalAuthentication calls; the reply block is retained by the framework.
    unsafe {
        let ctx = LAContext::new();
        ctx.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthentication,
            &NSString::from_str(reason),
            &reply,
        );
        rx.recv_timeout(Duration::from_secs(600))
            .map_err(|_| {
                tr!("czas na odblokowanie minął", "the time to unlock ran out").to_string()
            })?
    }
}

/// Windows Hello (PIN, fingerprint or face), its dialog owned by desk's window.
#[cfg(windows)]
fn authenticate(app: &AppHandle, reason: &str, _password: Option<&str>) -> Result<(), String> {
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult as R, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows::core::{HSTRING, factory};

    let err = |e: windows::core::Error| e.message().to_string();
    let available = UserConsentVerifier::CheckAvailabilityAsync()
        .map_err(err)?
        .join()
        .map_err(err)?;
    if available != UserConsentVerifierAvailability::Available {
        return Err(
            "Windows Hello nie jest skonfigurowane (Ustawienia → Konta → Opcje logowania: PIN)"
                .into(),
        );
    }
    let window = app.get_webview_window("main").ok_or(tr!("brak okna", "no window"))?;
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(err)?;
    // SAFETY: a valid window handle of this process and a message string.
    let op: windows_future::IAsyncOperation<R> = unsafe {
        interop.RequestVerificationForWindowAsync(HWND(hwnd.0 as _), &HSTRING::from(reason))
    }
    .map_err(err)?;
    match op.join().map_err(err)? {
        R::Verified => Ok(()),
        R::Canceled => Err("anulowano".into()),
        _ => Err(tr!("Windows Hello nie potwierdziło", "Windows Hello did not confirm").into()),
    }
}

/// The account password, checked by PAM.
#[cfg(target_os = "linux")]
fn authenticate(_app: &AppHandle, _reason: &str, password: Option<&str>) -> Result<(), String> {
    crate::pam::check_password(
        password.ok_or(tr!("wpisz hasło konta", "type the account password"))?,
    )
}
