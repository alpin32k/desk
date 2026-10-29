//! Files dropped on the window, by path.
//!
//! The main webview handles drag and drop itself (`dragDropEnabled: false`, so
//! tiles can be dragged between lanes), and WebKit hides where a dropped file
//! lives: the page gets its bytes, not its path. macOS keeps the last drag on
//! the drag pasteboard, so the page asks for the paths from its `drop` handler.

/// Paths of the files in the last drag; empty when there were none (or off macOS).
pub fn paths() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        mac::paths()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::ClassType;
    use objc2_app_kit::{NSPasteboard, NSPasteboardNameDrag};
    use objc2_foundation::{NSArray, NSURL};

    pub fn paths() -> Vec<String> {
        let pb = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag });
        let classes = NSArray::from_slice(&[NSURL::class()]);
        // SAFETY: an array of classes and no options is what the method takes.
        let Some(items) = (unsafe { pb.readObjectsForClasses_options(&classes, None) }) else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let url = item.downcast_ref::<NSURL>().filter(|u| u.isFileURL())?;
                // A file reference (file:///.file/id=...) becomes a path here.
                let path = url.filePathURL()?.path()?;
                Some(path.to_string())
            })
            .collect()
    }
}
