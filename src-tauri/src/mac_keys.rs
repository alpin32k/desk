//! ⌘Q closes the active tile and ⌘⇧Q kills its session, like Ctrl+Shift+Q
//! and Ctrl+Shift+⌫ elsewhere. macOS hands both to its menus first (Quit, and
//! the Apple menu's Log Out), even while a page has the focus, so a local
//! monitor takes them before any menu or webview sees them and tells the
//! window which one it was. desk itself quits only from its menu.

use block2::RcBlock;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags as M};
use std::ptr::NonNull;
use tauri::{AppHandle, Emitter};

pub fn install(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a valid event for the duration of the call.
            let e = unsafe { event.as_ref() };
            let flags = e.modifierFlags();
            let q = e
                .charactersIgnoringModifiers()
                .is_some_and(|c| c.to_string().eq_ignore_ascii_case("q"));
            if !q || !flags.contains(M::Command) || flags.intersects(M::Control | M::Option) {
                return event.as_ptr();
            }
            // Held down, it would close one tile after another.
            if !e.isARepeat() {
                let action = if flags.contains(M::Shift) { "kill" } else { "close" };
                let _ = handle.emit_to("main", "shortcut", action);
            }
            std::ptr::null_mut()
        });
        // SAFETY: a key-down monitor whose block outlives it (kept below).
        let monitor =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
        // For as long as desk runs.
        std::mem::forget(monitor);
        std::mem::forget(block);
    });
}
