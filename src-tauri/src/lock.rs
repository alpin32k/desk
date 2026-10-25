//! Lock: the screen stays on and the terminals stay visible, but nothing can
//! be typed into them until the owner unlocks with Touch ID or the account
//! password (macOS LocalAuthentication). While locked:
//! - `pty_write` refuses input (enforced here, not only in the UI),
//! - the window covers the screen and macOS kiosk options hide the Dock and
//!   menu bar and disable app switching, Force Quit and log out,
//! - quitting desk or closing its window is refused.
//!
//! This guards desk's sessions, not the whole Mac: for that use the macOS lock screen.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

#[derive(Default)]
pub struct Lock(AtomicBool);

impl Lock {
    pub fn is_locked(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn lock(&self, app: &AppHandle) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("blokada wymaga macOS (Touch ID / hasło)".into());
        }
        self.0.store(true, Ordering::SeqCst);
        kiosk(app, true);
        Ok(())
    }

    /// Blocks until the user authenticates or cancels.
    pub fn unlock(&self, app: &AppHandle) -> Result<(), String> {
        if !self.is_locked() {
            return Ok(());
        }
        authenticate("odblokować desk")?;
        self.0.store(false, Ordering::SeqCst);
        kiosk(app, false);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn kiosk(app: &AppHandle, on: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions as O};
    use tauri::Manager;
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

#[cfg(not(target_os = "macos"))]
fn kiosk(_app: &AppHandle, _on: bool) {}

/// Touch ID, Apple Watch or the account password, via the system dialog.
#[cfg(target_os = "macos")]
fn authenticate(reason: &str) -> Result<(), String> {
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
            Err("nie udało się odblokować".into())
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
            .map_err(|_| "czas na odblokowanie minął".to_string())?
    }
}

#[cfg(not(target_os = "macos"))]
fn authenticate(_reason: &str) -> Result<(), String> {
    Err("blokada wymaga macOS".into())
}
