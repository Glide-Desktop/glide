static CONFIGURED_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static CONFIGURED_THEME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[must_use]
pub fn theme_name(theme: Option<&str>) -> Option<String> {
    theme.map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned)
}

#[must_use]
pub fn pick_theme(configured: Option<String>, environment: Option<&str>) -> String {
    configured.or_else(|| theme_name(environment)).unwrap_or_else(|| "default".into())
}

pub fn set_configured_theme(theme: Option<&str>) -> bool {
    let wanted = theme_name(theme);
    let Ok(mut current) = CONFIGURED_THEME.lock() else {
        return false;
    };
    if *current == wanted {
        return false;
    }
    *current = wanted;
    true
}

#[must_use]
pub fn configured_theme() -> Option<String> {
    CONFIGURED_THEME.lock().ok().and_then(|theme| theme.clone())
}

#[must_use]
pub fn effective_theme(environment: Option<&str>) -> String {
    pick_theme(configured_theme(), environment)
}

#[must_use]
pub fn configured_size() -> Option<u32> {
    match CONFIGURED_SIZE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => None,
        size => Some(size),
    }
}

pub const MIN_SIZE: u32 = 16;
pub const MAX_SIZE: u32 = 256;

#[must_use]
pub fn configured_value(size: Option<u32>) -> u32 {
    size.map_or(0, |size| size.clamp(MIN_SIZE, MAX_SIZE))
}

pub fn set_configured_size(size: Option<u32>) {
    CONFIGURED_SIZE.store(configured_value(size), std::sync::atomic::Ordering::Relaxed);
}

#[must_use]
pub fn effective_size(environment: u32) -> u32 {
    match CONFIGURED_SIZE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => environment,
        configured => configured,
    }
}
