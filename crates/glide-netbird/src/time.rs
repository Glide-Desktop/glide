use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) fn system_time(stamp: &prost_types::Timestamp) -> Option<SystemTime> {
    if stamp.seconds <= 0 && stamp.nanos <= 0 {
        return None;
    }
    let seconds = u64::try_from(stamp.seconds).ok()?;
    let nanos = u32::try_from(stamp.nanos).ok()?;
    UNIX_EPOCH.checked_add(Duration::new(seconds, nanos))
}

pub(crate) fn duration_from(duration: &prost_types::Duration) -> Option<Duration> {
    let seconds = u64::try_from(duration.seconds).ok()?;
    let nanos = u32::try_from(duration.nanos).ok()?;
    Some(Duration::new(seconds, nanos))
}

pub(crate) fn proto_duration(duration: Duration) -> prost_types::Duration {
    prost_types::Duration {
        seconds: i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        nanos: i32::try_from(duration.subsec_nanos()).unwrap_or_default(),
    }
}

const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn number(text: &str) -> Option<i64> {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())).then(|| text.parse().ok()).flatten()
}

#[must_use]
pub fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let text = text.trim();
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut date_parts = date.splitn(3, '-');
    let year = number(date_parts.next()?)?;
    let month = number(date_parts.next()?)?;
    let day = number(date_parts.next()?)?;
    let zone_at = rest.find(['Z', 'z', '+', '-'])?;
    let (clock, zone) = rest.split_at(zone_at);
    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut clock_parts = clock.splitn(3, ':');
    let hour = number(clock_parts.next()?)?;
    let minute = number(clock_parts.next()?)?;
    let second = number(clock_parts.next()?)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let (hours, minutes) = zone.get(1..)?.split_once(':')?;
            sign * (number(hours)? * 3600 + number(minutes)? * 60)
        }
    };
    let nanos = if fraction.is_empty() {
        0
    } else {
        let digits: String = fraction.chars().take(9).collect();
        number(&digits)? * 10_i64.pow(9 - digits.len() as u32)
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset;
    let seconds = u64::try_from(seconds).ok()?;
    UNIX_EPOCH.checked_add(Duration::new(seconds, u32::try_from(nanos).ok()?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remaining {
    Expired,
    LessThanMinute,
    Minutes(u64),
    Hours(u64),
    Days(u64),
}

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

fn ceil_div(value: Duration, unit: u64) -> u64 {
    let nanos = value.as_nanos();
    let unit = u128::from(unit) * 1_000_000_000;
    u64::try_from(nanos.div_ceil(unit)).unwrap_or(u64::MAX)
}

#[must_use]
pub fn remaining(left: Option<Duration>) -> Remaining {
    let Some(left) = left.filter(|left| !left.is_zero()) else {
        return Remaining::Expired;
    };
    let seconds = left.as_secs();
    if seconds < MINUTE {
        Remaining::LessThanMinute
    } else if left <= Duration::from_secs(59 * MINUTE) {
        Remaining::Minutes(ceil_div(left, MINUTE))
    } else if left <= Duration::from_secs(23 * HOUR) {
        Remaining::Hours(ceil_div(left, HOUR))
    } else {
        Remaining::Days(ceil_div(left, DAY))
    }
}

#[must_use]
pub fn remaining_until(deadline: SystemTime, now: SystemTime) -> Remaining {
    remaining(deadline.duration_since(now).ok())
}

#[must_use]
pub fn refresh_interval(left: Option<Duration>) -> Duration {
    match left {
        None => Duration::from_secs(30),
        Some(left) if left.is_zero() => Duration::from_secs(30),
        Some(left) if left <= Duration::from_secs(2 * MINUTE) => Duration::from_secs(10),
        Some(left) if left <= Duration::from_secs(HOUR) => Duration::from_secs(30),
        Some(_) => Duration::from_mins(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn rfc3339_stamps_parse_in_utc_with_offsets_and_fractions() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(UNIX_EPOCH));
        assert_eq!(parse_rfc3339("2026-10-15T11:06:48Z"), Some(at(1_792_062_408)));
        assert_eq!(parse_rfc3339("2026-10-15T14:06:48+03:00"), Some(at(1_792_062_408)));
        assert_eq!(parse_rfc3339("2026-10-15T11:06:48.5Z"), Some(at(1_792_062_408) + Duration::from_millis(500)));
        assert_eq!(parse_rfc3339("2000-02-29T00:00:00Z"), Some(at(951_782_400)));
        for bad in ["", "soon", "2026-13-01T00:00:00Z", "2026-10-15", "2026-10-15T11:06:48", "2026-10-15T25:00:00Z"] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }

    #[test]
    fn protobuf_timestamps_at_or_before_the_epoch_mean_unset() {
        assert_eq!(system_time(&prost_types::Timestamp { seconds: 0, nanos: 0 }), None);
        assert_eq!(system_time(&prost_types::Timestamp { seconds: 10, nanos: 0 }), Some(at(10)));
    }

    #[test]
    fn remaining_time_rounds_up_to_the_largest_unit_that_fits() {
        let left = |seconds| remaining(Some(Duration::from_secs(seconds)));
        assert_eq!(remaining(None), Remaining::Expired);
        assert_eq!(left(0), Remaining::Expired);
        assert_eq!(left(59), Remaining::LessThanMinute);
        assert_eq!(left(60), Remaining::Minutes(1));
        assert_eq!(left(61), Remaining::Minutes(2));
        assert_eq!(left(59 * 60), Remaining::Minutes(59));
        assert_eq!(left(59 * 60 + 1), Remaining::Hours(1));
        assert_eq!(left(23 * 3600), Remaining::Hours(23));
        assert_eq!(left(23 * 3600 + 1), Remaining::Days(1));
        assert_eq!(left(3 * 86_400 - 5), Remaining::Days(3));
        assert_eq!(remaining_until(at(100), at(200)), Remaining::Expired);
    }

    #[test]
    fn the_countdown_refreshes_faster_as_the_deadline_nears() {
        assert_eq!(refresh_interval(Some(Duration::from_secs(90))), Duration::from_secs(10));
        assert_eq!(refresh_interval(Some(Duration::from_mins(30))), Duration::from_secs(30));
        assert_eq!(refresh_interval(Some(Duration::from_hours(2))), Duration::from_mins(1));
        assert_eq!(refresh_interval(None), Duration::from_secs(30));
    }
}
