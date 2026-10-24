use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalType {
    pub offset: i32,
    pub dst: bool,
    pub abbr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleDay {
    Julian(u16),
    Ordinal(u16),
    Month { month: u8, week: u8, weekday: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Transition {
    day: RuleDay,
    time: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    std: LocalType,
    dst: Option<(LocalType, Transition, Transition)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    transitions: Vec<(i64, usize)>,
    types: Vec<LocalType>,
    footer: Option<Rule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub weekday: u8,
}

#[must_use]
pub const fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = month as i64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[must_use]
pub const fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month as u8, day as u8)
}

#[must_use]
pub(crate) const fn weekday_of_days(days: i64) -> u8 {
    (days + 4).rem_euclid(7) as u8
}

#[must_use]
pub const fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[must_use]
pub const fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[must_use]
pub const fn civil(seconds: i64) -> Civil {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    Civil {
        year,
        month,
        day,
        hour: (rest / 3600) as u8,
        minute: (rest % 3600 / 60) as u8,
        second: (rest % 60) as u8,
        weekday: weekday_of_days(days),
    }
}

#[must_use]
pub const fn from_civil(year: i64, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> i64 {
    days_from_civil(year, month, day) * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64
}

#[must_use]
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
}

#[must_use]
pub fn format_offset(offset: i32) -> String {
    let sign = if offset < 0 { '−' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be_i32(bytes: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be_i64(bytes: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

struct Counts {
    isut: usize,
    isstd: usize,
    leap: usize,
    time: usize,
    types: usize,
    chars: usize,
}

fn counts(bytes: &[u8], at: usize) -> Option<Counts> {
    if bytes.get(at..at + 4)? != b"TZif" {
        return None;
    }
    let field = |index: usize| be_u32(bytes, at + 20 + index * 4).map(|n| n as usize);
    Some(Counts {
        isut: field(0)?,
        isstd: field(1)?,
        leap: field(2)?,
        time: field(3)?,
        types: field(4)?,
        chars: field(5)?,
    })
}

const fn block_len(counts: &Counts, time_size: usize) -> usize {
    counts.time * time_size
        + counts.time
        + counts.types * 6
        + counts.chars
        + counts.leap * (time_size + 4)
        + counts.isstd
        + counts.isut
}

fn abbreviation(chars: &[u8], index: usize) -> String {
    let tail = chars.get(index..).unwrap_or_default();
    let end = tail.iter().position(|byte| *byte == 0).unwrap_or(tail.len());
    String::from_utf8_lossy(&tail[..end]).into_owned()
}

impl Zone {
    #[must_use]
    pub fn utc() -> Self {
        Self {
            transitions: Vec::new(),
            types: vec![LocalType { offset: 0, dst: false, abbr: String::from("UTC") }],
            footer: None,
        }
    }

    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let first = counts(bytes, 0)?;
        let version = *bytes.get(4)?;
        let (header, data_counts, time_size) = if version >= b'2' {
            let second = 44 + block_len(&first, 4);
            let data = counts(bytes, second)?;
            (second, data, 8)
        } else {
            (0, first, 4)
        };
        let start = header + 44;
        let times_end = start + data_counts.time * time_size;
        let indices_end = times_end + data_counts.time;
        let types_end = indices_end + data_counts.types * 6;
        let chars_end = types_end + data_counts.chars;
        let chars = bytes.get(types_end..chars_end)?;
        let mut types = Vec::with_capacity(data_counts.types);
        for index in 0..data_counts.types {
            let at = indices_end + index * 6;
            types.push(LocalType {
                offset: be_i32(bytes, at)?,
                dst: *bytes.get(at + 4)? != 0,
                abbr: abbreviation(chars, usize::from(*bytes.get(at + 5)?)),
            });
        }
        if types.is_empty() {
            return None;
        }
        let mut transitions = Vec::with_capacity(data_counts.time);
        for index in 0..data_counts.time {
            let time = if time_size == 8 {
                be_i64(bytes, start + index * 8)?
            } else {
                i64::from(be_i32(bytes, start + index * 4)?)
            };
            let kind = usize::from(*bytes.get(times_end + index)?);
            if kind >= types.len() {
                return None;
            }
            transitions.push((time, kind));
        }
        let footer = if time_size == 8 {
            let rest = bytes.get(header + 44 + block_len(&data_counts, 8)..)?;
            std::str::from_utf8(rest)
                .ok()
                .and_then(|text| text.trim_matches('\n').lines().next().map(str::to_string))
                .and_then(|text| parse_rule(&text))
        } else {
            None
        };
        Some(Self { transitions, types, footer })
    }

    #[must_use]
    pub fn load(name: &str) -> Option<Self> {
        if name.is_empty() || name.contains("..") || name.starts_with('/') {
            return None;
        }
        std::fs::read(zoneinfo_dir().join(name)).ok().and_then(|bytes| Self::parse(&bytes))
    }

    #[must_use]
    pub fn system() -> Self {
        std::fs::read("/etc/localtime").ok().and_then(|bytes| Self::parse(&bytes)).unwrap_or_else(Self::utc)
    }

    #[must_use]
    pub fn at(&self, utc: i64) -> LocalType {
        if let Some(footer) = &self.footer
            && self.transitions.last().is_none_or(|(last, _)| utc >= *last)
        {
            return footer.at(utc);
        }
        let index = self.transitions.partition_point(|(time, _)| *time <= utc);
        let kind = if index == 0 { 0 } else { self.transitions[index - 1].1 };
        self.types[kind].clone()
    }

    #[must_use]
    pub fn local(&self, utc: i64) -> Civil {
        civil(utc + i64::from(self.at(utc).offset))
    }

    #[must_use]
    pub fn to_utc(&self, local: i64) -> i64 {
        let guess = local - i64::from(self.at(local).offset);
        local - i64::from(self.at(guess).offset)
    }
}

#[must_use]
pub(crate) fn zoneinfo_dir() -> PathBuf {
    std::env::var_os("TZDIR")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("/usr/share/zoneinfo"), PathBuf::from)
}

fn take_name(text: &str) -> Option<(String, &str)> {
    if let Some(rest) = text.strip_prefix('<') {
        let end = rest.find('>')?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    let end = text.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(text.len());
    (end >= 3).then(|| (text[..end].to_string(), &text[end..]))
}

fn take_clock(text: &str) -> Option<(i32, &str)> {
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let end = body.find(|c: char| !(c.is_ascii_digit() || c == ':')).unwrap_or(body.len());
    if end == 0 {
        return None;
    }
    let mut parts = body[..end].split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    let seconds: i32 = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    let value = hours * 3600 + minutes * 60 + seconds;
    Some((if negative { -value } else { value }, &body[end..]))
}

fn parse_day(text: &str) -> Option<RuleDay> {
    if let Some(rest) = text.strip_prefix('J') {
        return rest.parse().ok().map(RuleDay::Julian);
    }
    if let Some(rest) = text.strip_prefix('M') {
        let mut parts = rest.split('.');
        let month: u8 = parts.next()?.parse().ok()?;
        let week: u8 = parts.next()?.parse().ok()?;
        let weekday: u8 = parts.next()?.parse().ok()?;
        return ((1..=12).contains(&month) && (1..=5).contains(&week) && weekday <= 6).then_some(RuleDay::Month {
            month,
            week,
            weekday,
        });
    }
    text.parse().ok().map(RuleDay::Ordinal)
}

fn parse_transition(text: &str) -> Option<Transition> {
    let (day, time) = match text.split_once('/') {
        Some((day, time)) => (day, take_clock(time)?.0),
        None => (text, 7200),
    };
    Some(Transition { day: parse_day(day)?, time })
}

#[must_use]
pub fn parse_rule(text: &str) -> Option<Rule> {
    let (std_name, rest) = take_name(text.trim())?;
    let (std_offset, rest) = take_clock(rest)?;
    let std = LocalType { offset: -std_offset, dst: false, abbr: std_name };
    if rest.is_empty() {
        return Some(Rule { std, dst: None });
    }
    let (dst_name, rest) = take_name(rest)?;
    let (dst_offset, rest) = if rest.starts_with(',') || rest.is_empty() {
        (std.offset + 3600, rest)
    } else {
        let (offset, rest) = take_clock(rest)?;
        (-offset, rest)
    };
    let rest = rest.strip_prefix(',')?;
    let (start, end) = rest.split_once(',')?;
    Some(Rule {
        std,
        dst: Some((
            LocalType { offset: dst_offset, dst: true, abbr: dst_name },
            parse_transition(start)?,
            parse_transition(end)?,
        )),
    })
}

fn day_of_year(year: i64, day: RuleDay) -> i64 {
    match day {
        RuleDay::Julian(n) => {
            let n = i64::from(n.clamp(1, 365)) - 1;
            if is_leap(year) && n >= 59 { n + 1 } else { n }
        }
        RuleDay::Ordinal(n) => i64::from(n.min(365)),
        RuleDay::Month { month, week, weekday } => {
            let first = days_from_civil(year, month, 1);
            let first_weekday = i64::from(weekday_of_days(first));
            let mut date = 1 + (i64::from(weekday) - first_weekday).rem_euclid(7) + (i64::from(week) - 1) * 7;
            while date > i64::from(days_in_month(year, month)) {
                date -= 7;
            }
            first + date - 1 - days_from_civil(year, 1, 1)
        }
    }
}

fn transition_utc(year: i64, transition: Transition, offset: i32) -> i64 {
    (days_from_civil(year, 1, 1) + day_of_year(year, transition.day)) * 86_400 + i64::from(transition.time)
        - i64::from(offset)
}

impl Rule {
    #[must_use]
    pub fn at(&self, utc: i64) -> LocalType {
        let Some((dst, start, end)) = &self.dst else {
            return self.std.clone();
        };
        let year = civil(utc + i64::from(self.std.offset)).year;
        let begins = transition_utc(year, *start, self.std.offset);
        let ends = transition_utc(year, *end, dst.offset);
        let in_dst = if begins < ends { utc >= begins && utc < ends } else { !(utc >= ends && utc < begins) };
        if in_dst { dst.clone() } else { self.std.clone() }
    }
}

#[must_use]
pub fn parse_zone_tab(text: &str) -> Vec<String> {
    let mut zones: Vec<String> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(2))
        .map(|zone| zone.trim().to_string())
        .filter(|zone| !zone.is_empty())
        .collect();
    zones.sort();
    zones.dedup();
    zones
}

#[must_use]
pub fn city_name(zone: &str) -> String {
    zone.rsplit('/').next().unwrap_or(zone).replace('_', " ")
}

#[must_use]
pub fn region_name(zone: &str) -> String {
    match zone.rsplit_once('/') {
        Some((region, _)) => region.replace('_', " ").replace('/', " · "),
        None => String::new(),
    }
}
