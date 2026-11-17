use std::sync::atomic::{AtomicU8, Ordering};

pub use glide_app::i18n::Lang;

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_language(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

#[must_use]
pub fn language() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Uk,
    }
}
