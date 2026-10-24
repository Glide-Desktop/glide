use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub mod autostart;
pub mod desktop;
mod keyboard;
pub mod permissions;
pub mod process;
pub mod terminal;
pub mod toolkits;

pub use keyboard::SystemKeyboard;

fn var(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn user_dir(xdg: Option<PathBuf>, home: Option<PathBuf>, fallback: &str) -> Option<PathBuf> {
    xdg.or_else(|| home.map(|home| home.join(fallback)))
}

#[must_use]
pub fn home() -> Option<PathBuf> {
    var("HOME")
}

#[must_use]
pub fn config_home() -> Option<PathBuf> {
    user_dir(var("XDG_CONFIG_HOME"), home(), ".config")
}

#[must_use]
pub fn data_home() -> Option<PathBuf> {
    user_dir(var("XDG_DATA_HOME"), home(), ".local/share")
}

#[must_use]
pub fn cache_home() -> Option<PathBuf> {
    user_dir(var("XDG_CACHE_HOME"), home(), ".cache")
}

#[must_use]
pub fn state_home() -> Option<PathBuf> {
    user_dir(var("XDG_STATE_HOME"), home(), ".local/state")
}

#[must_use]
pub fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return std::path::Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

fn search_path(key: &str, default: &str) -> Vec<PathBuf> {
    let list = std::env::var_os(key).filter(|value| !value.is_empty()).unwrap_or_else(|| default.into());
    std::env::split_paths(&list).collect()
}

#[must_use]
pub fn data_dirs() -> Vec<PathBuf> {
    data_home().into_iter().chain(search_path("XDG_DATA_DIRS", "/usr/local/share:/usr/share")).collect()
}

#[must_use]
pub fn system_config_dirs() -> Vec<PathBuf> {
    search_path("XDG_CONFIG_DIRS", "/etc/xdg")
}

#[must_use]
pub fn config_dirs() -> Vec<PathBuf> {
    config_home().into_iter().chain(system_config_dirs()).collect()
}

#[must_use]
pub fn xdg_path(component: &str, file_name: &str) -> Option<PathBuf> {
    Some(config_home()?.join(component).join(file_name))
}

static LAST_TZSET: AtomicU64 = AtomicU64::new(0);
const TZSET_INTERVAL: u64 = 60;

unsafe extern "C" {
    fn tzset();
}

fn tzset_due(last: u64, now: u64) -> bool {
    last == 0 || now.saturating_sub(last) >= TZSET_INTERVAL || now < last
}

fn refresh_time_zone(now: u64) {
    let last = LAST_TZSET.load(Ordering::Relaxed);
    if tzset_due(last, now)
        && LAST_TZSET.compare_exchange(last, now.max(1), Ordering::Relaxed, Ordering::Relaxed).is_ok()
    {
        // SAFETY: tzset takes glibc's time zone lock, which localtime_r also takes, and only re-reads TZ and /etc/localtime.
        unsafe { tzset() };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[must_use]
pub fn local_time() -> Option<LocalTime> {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    refresh_time_zone(seconds);
    let seconds = libc::time_t::try_from(seconds).ok()?;
    let mut value = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are valid, and the output is only read after a successful call.
    if unsafe { libc::localtime_r(&raw const seconds, value.as_mut_ptr()) }.is_null() {
        return None;
    }
    // SAFETY: localtime_r returned a non-null pointer and initialized the output.
    let value = unsafe { value.assume_init() };
    let time = LocalTime {
        year: value.tm_year + 1900,
        month: u8::try_from(value.tm_mon + 1).ok()?,
        day: u8::try_from(value.tm_mday).ok()?,
        hour: u8::try_from(value.tm_hour).ok()?,
        minute: u8::try_from(value.tm_min).ok()?,
    };
    (time.hour < 24 && time.minute < 60).then_some(time)
}

#[must_use]
pub fn local_minute() -> Option<u16> {
    local_time().map(|time| u16::from(time.hour) * 60 + u16::from(time.minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_wins_over_home_and_empty_values_fall_through() {
        let home = Some(PathBuf::from("/home/me"));
        assert_eq!(user_dir(Some("/xdg".into()), home.clone(), ".config"), Some(PathBuf::from("/xdg")));
        assert_eq!(user_dir(None, home, ".config"), Some(PathBuf::from("/home/me/.config")));
        assert_eq!(user_dir(None, None, ".config"), None);
    }

    #[test]
    fn the_time_zone_is_reread_at_first_use_then_at_most_once_a_minute() {
        assert!(tzset_due(0, 1_000));
        assert!(!tzset_due(1_000, 1_000));
        assert!(!tzset_due(1_000, 1_059));
        assert!(tzset_due(1_000, 1_060));
        assert!(tzset_due(1_000, 900), "a clock set backwards rereads at once");
    }

    #[test]
    fn local_minute_is_within_a_day() {
        assert!(local_minute().is_some_and(|minute| minute < 24 * 60));
        assert!(local_minute().is_some_and(|minute| minute < 24 * 60));
    }
}
