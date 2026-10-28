#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalNow {
    pub year: i32,
    pub month0: u32,
    pub day: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

pub fn refresh_timezone(now: libc::time_t) {
    unsafe extern "C" {
        fn tzset();
    }
    static LAST_MINUTE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(i64::MIN);
    let minute = now.div_euclid(60);
    if LAST_MINUTE.swap(minute, std::sync::atomic::Ordering::Relaxed) != minute {
        // SAFETY: `tzset` takes glibc's timezone lock, which `localtime_r`
        // also takes, and only re-reads `TZ` / `/etc/localtime`.
        unsafe { tzset() };
    }
}

impl LocalNow {
    #[must_use]
    pub fn now() -> Option<Self> {
        let mut raw: libc::time_t = 0;
        // SAFETY: `time` writes a single `time_t` through a pointer we own.
        let now = unsafe { libc::time(&raw mut raw) };
        if now == -1 {
            return None;
        }
        refresh_timezone(now);
        // SAFETY: `tm` is a plain C struct of integers and a pointer for which
        // all-zero bytes is a valid value; `localtime_r` overwrites it below.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers address locals that outlive the call, and
        // `localtime_r` writes only the `tm` we hand it.
        let filled = unsafe { libc::localtime_r(&raw const now, &raw mut tm) };
        if filled.is_null() {
            return None;
        }
        Some(Self {
            year: tm.tm_year + 1900,
            month0: u32::try_from(tm.tm_mon).ok()?.min(11),
            day: u32::try_from(tm.tm_mday).ok()?,
            weekday: u32::try_from(tm.tm_wday).ok()? % 7,
            hour: u32::try_from(tm.tm_hour).ok()?,
            minute: u32::try_from(tm.tm_min).ok()?,
        })
    }
}

#[must_use]
pub fn format_time(hour: u32, minute: u32, h24: bool) -> String {
    if h24 {
        return format!("{hour:02}:{minute:02}");
    }
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let twelve = match hour % 12 {
        0 => 12,
        other => other,
    };
    format!("{twelve}:{minute:02} {suffix}")
}

#[must_use]
pub fn locale_is_us(lc_all: Option<&str>, lc_time: Option<&str>, lang: Option<&str>) -> bool {
    let locale = [lc_all, lc_time, lang]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    let base = locale.split(['.', '@']).next().unwrap_or_default();
    base.eq_ignore_ascii_case("en_US")
}

#[must_use]
pub fn env_locale_is_us() -> bool {
    let var = |name: &str| std::env::var(name).ok();
    locale_is_us(var("LC_ALL").as_deref(), var("LC_TIME").as_deref(), var("LANG").as_deref())
}

#[must_use]
pub const fn default_24h(us_locale: bool) -> bool {
    !us_locale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_use_the_requested_clock() {
        assert_eq!(format_time(14, 5, true), "14:05");
        assert_eq!(format_time(9, 41, true), "09:41");
        assert_eq!(format_time(14, 5, false), "2:05 PM");
        assert_eq!(format_time(0, 0, false), "12:00 AM");
        assert_eq!(format_time(12, 30, false), "12:30 PM");
    }

    #[test]
    fn only_a_us_locale_defaults_to_twelve_hours() {
        assert!(locale_is_us(None, None, Some("en_US.UTF-8")));
        assert!(!locale_is_us(None, None, Some("en_GB.UTF-8")));
        assert!(!locale_is_us(None, None, Some("uk_UA.UTF-8")));
        assert!(locale_is_us(None, Some("en_US"), Some("uk_UA.UTF-8")));
        assert!(!locale_is_us(Some("C"), Some("en_US"), None));
        assert!(!default_24h(true));
        assert!(default_24h(false));
    }
}
