//! Language of the messages desk shows from here (errors, prompts):
//! layout.language, set in Options. Polish until the window says otherwise.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Follow the window's language ("pl", "en"; anything else stays Polish).
pub fn set(language: &str) {
    ENGLISH.store(language == "en", Ordering::Relaxed);
}

pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// The Polish or the English text, by the chosen language; only that one is
/// built (`tr!(format!(...), format!(...))`).
macro_rules! tr {
    ($pl:expr, $en:expr $(,)?) => {
        if $crate::i18n::english() {
            $en
        } else {
            $pl
        }
    };
}
