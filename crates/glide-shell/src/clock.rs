use std::ffi::CStr;
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::i18n::{Key, Lang};

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
        if cfg!(target_env = "musl") {
            follow_localtime();
        }
        // SAFETY: `tzset` takes glibc's timezone lock, which `localtime_r`
        // also takes, and only re-reads `TZ` / `/etc/localtime`.
        unsafe { tzset() };
    }
}

const LOCALTIME: &str = "/etc/localtime";

const TZ_SPELLINGS: [&CStr; 2] = [c"TZ=:/etc/localtime", c"TZ=/etc/localtime"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoneFile {
    Unseen,
    Missing,
    At { inode: u64, modified: i64 },
}

impl ZoneFile {
    fn read() -> Self {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(LOCALTIME)
            .map_or(Self::Missing, |meta| Self::At { inode: meta.ino(), modified: meta.mtime() })
    }
}

fn zone_replaced(seen: &mut ZoneFile, current: ZoneFile) -> bool {
    let before = std::mem::replace(seen, current);
    before != ZoneFile::Unseen && before != current
}

fn tz_spelling(tz: Option<&std::ffi::OsStr>) -> Option<usize> {
    use std::os::unix::ffi::OsStrExt;
    let tz = tz?.as_bytes();
    TZ_SPELLINGS.iter().position(|spelling| spelling.to_bytes().strip_prefix(b"TZ=") == Some(tz))
}

fn follow_localtime() {
    static SEEN: Mutex<ZoneFile> = Mutex::new(ZoneFile::Unseen);
    let tz = std::env::var_os("TZ");
    let ours = tz_spelling(tz.as_deref());
    if tz.is_some() && ours.is_none() {
        return;
    }
    if !zone_replaced(&mut SEEN.lock().unwrap_or_else(PoisonError::into_inner), ZoneFile::read()) {
        return;
    }
    let next = TZ_SPELLINGS[usize::from(ours == Some(0))];
    // SAFETY: musl's `tzset` re-reads the zone file only when `TZ` reads
    // differently, so the two spellings alternate. `putenv` keeps the pointer
    // instead of copying and both strings are static, so a value another
    // thread's `getenv` already holds is never freed.
    unsafe { libc::putenv(next.as_ptr().cast_mut()) };
}

#[must_use]
pub fn twelve_hour_format(time_format: &str) -> Option<bool> {
    if time_format.is_empty() || time_format == "%H:%M:%S" {
        return None;
    }
    Some(["%p", "%P", "%I", "%l", "%r"].iter().any(|code| time_format.contains(code)))
}

fn locale_time_format() -> Option<String> {
    // SAFETY: an empty name selects the locale the environment names;
    // `newlocale` returns a fresh object or null and touches no global state.
    let locale = unsafe { libc::newlocale(libc::LC_TIME_MASK, c"".as_ptr(), std::ptr::null_mut()) };
    if locale.is_null() {
        return None;
    }
    // SAFETY: `locale` is the live object created above, and the string it
    // hands back belongs to it until `freelocale`.
    let raw = unsafe { libc::nl_langinfo_l(libc::T_FMT, locale) };
    // SAFETY: a non-null result is a NUL-terminated string owned by `locale`,
    // copied here before the locale is freed.
    let format = (!raw.is_null()).then(|| unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned());
    // SAFETY: `locale` came from `newlocale`, is freed once, and nothing
    // borrowed from it outlives this call.
    unsafe { libc::freelocale(locale) };
    format
}

#[must_use]
pub fn env_prefers_12h() -> bool {
    static TWELVE: OnceLock<bool> = OnceLock::new();
    *TWELVE
        .get_or_init(|| locale_time_format().as_deref().and_then(twelve_hour_format).unwrap_or_else(env_locale_is_us))
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
    format_time_in(crate::i18n::language(), hour, minute, h24)
}

#[must_use]
pub fn format_time_in(lang: Lang, hour: u32, minute: u32, h24: bool) -> String {
    if h24 {
        return format!("{hour:02}:{minute:02}");
    }
    let suffix = if hour < 12 { Key::ClockAm } else { Key::ClockPm }.text(lang);
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
pub const fn default_24h(twelve_hour_locale: bool) -> bool {
    !twelve_hour_locale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_use_the_requested_clock() {
        let en = |hour, minute, h24| format_time_in(Lang::En, hour, minute, h24);
        assert_eq!(en(14, 5, true), "14:05");
        assert_eq!(en(9, 41, true), "09:41");
        assert_eq!(en(14, 5, false), "2:05 PM");
        assert_eq!(en(0, 0, false), "12:00 AM");
        assert_eq!(en(12, 30, false), "12:30 PM");
        assert_eq!(format_time_in(Lang::Uk, 14, 5, false), "2:05 пп");
        assert_eq!(format_time_in(Lang::Uk, 9, 5, false), "9:05 дп");
    }

    #[test]
    fn the_locale_time_format_decides_twelve_hours_and_the_c_default_decides_nothing() {
        assert_eq!(twelve_hour_format("%r"), Some(true), "en_US");
        assert_eq!(twelve_hour_format("%I:%M:%S %p"), Some(true));
        assert_eq!(twelve_hour_format("%l:%M:%S %P %Z"), Some(true));
        assert_eq!(twelve_hour_format("%T"), Some(false), "uk_UA, en_GB, de_DE");
        assert_eq!(twelve_hour_format("%H:%M:%S"), None, "C, or musl without locale data");
        assert_eq!(twelve_hour_format(""), None);
    }

    #[test]
    fn a_replaced_zone_file_is_a_change_but_the_first_look_is_not() {
        let zone = |inode, modified| ZoneFile::At { inode, modified };
        let mut seen = ZoneFile::Unseen;
        assert!(!zone_replaced(&mut seen, zone(1, 10)));
        assert!(!zone_replaced(&mut seen, zone(1, 10)));
        assert!(zone_replaced(&mut seen, zone(2, 10)), "timedatectl relinks to another zone");
        assert!(zone_replaced(&mut seen, zone(2, 11)), "the zone file rewritten in place");
        assert!(zone_replaced(&mut seen, ZoneFile::Missing));
        assert!(!zone_replaced(&mut seen, ZoneFile::Missing));
    }

    #[test]
    fn only_an_unset_tz_or_one_of_our_two_spellings_is_taken_over() {
        use std::ffi::OsStr;
        assert_eq!(tz_spelling(None), None);
        assert_eq!(tz_spelling(Some(OsStr::new(":/etc/localtime"))), Some(0));
        assert_eq!(tz_spelling(Some(OsStr::new("/etc/localtime"))), Some(1));
        assert_eq!(tz_spelling(Some(OsStr::new("Europe/Kyiv"))), None);
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
