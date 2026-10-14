use std::collections::HashMap;
use std::process::{Command, Stdio};

use super::run;
use super::timedate::{authorized_call, system_proxy};
use super::tz::Civil;

const DEST: &str = "org.freedesktop.locale1";
const PATH: &str = "/org/freedesktop/locale1";

pub const FORMAT_CATEGORIES: [&str; 5] = ["LC_TIME", "LC_NUMERIC", "LC_MONETARY", "LC_MEASUREMENT", "LC_PAPER"];

const MAX_LOCALES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemLocale {
    pub vars: Vec<(String, String)>,
}

impl SystemLocale {
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.iter().find(|(name, value)| name == key && !value.is_empty()).map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub fn lang(&self) -> &str {
        self.get("LANG").unwrap_or("C.UTF-8")
    }

    #[must_use]
    pub fn effective(&self, category: &str) -> &str {
        self.get("LC_ALL").or_else(|| self.get(category)).unwrap_or_else(|| self.lang())
    }

    #[must_use]
    pub fn with(&self, key: &str, value: Option<&str>) -> Self {
        let mut vars: Vec<(String, String)> = self.vars.iter().filter(|(name, _)| name != key).cloned().collect();
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            vars.push((key.to_string(), value.to_string()));
        }
        vars.sort_by_key(|(name, _)| usize::from(name != "LANG"));
        Self { vars }
    }

    #[must_use]
    pub fn assignments(&self) -> Vec<String> {
        self.vars.iter().filter(|(_, value)| !value.is_empty()).map(|(name, value)| format!("{name}={value}")).collect()
    }
}

#[must_use]
pub fn parse_assignments<'a>(lines: impl IntoIterator<Item = &'a str>) -> SystemLocale {
    let vars = lines
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            let valid = key == "LANG" || key == "LANGUAGE" || key.starts_with("LC_");
            valid.then(|| (key.to_string(), value.trim().trim_matches('"').trim_matches('\'').to_string()))
        })
        .collect();
    SystemLocale { vars }
}

#[must_use]
pub fn parse_status(text: &str) -> SystemLocale {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("System Locale:") {
            inside = true;
            lines.push(rest.trim());
        } else if inside && !trimmed.contains(": ") && trimmed.contains('=') {
            lines.push(trimmed);
        } else {
            inside = false;
        }
    }
    parse_assignments(lines)
}

#[must_use]
pub fn read_system() -> SystemLocale {
    let from_bus = system_proxy(DEST, PATH).ok().and_then(|proxy| proxy.get_property::<Vec<String>>("Locale").ok());
    if let Some(list) = from_bus {
        return parse_assignments(list.iter().map(String::as_str));
    }
    if let Some(text) = run::output("localectl", &["status"]) {
        let parsed = parse_status(&text);
        if !parsed.vars.is_empty() {
            return parsed;
        }
    }
    let text = std::fs::read_to_string("/etc/locale.conf").unwrap_or_default();
    parse_assignments(text.lines())
}

pub fn set_system(locale: &SystemLocale) -> Result<(), String> {
    authorized_call(&system_proxy(DEST, PATH)?, "SetLocale", &(locale.assignments(), true))
}

#[must_use]
pub fn normalise(locale: &str) -> String {
    let locale = locale.trim();
    let (base, modifier) = match locale.split_once('@') {
        Some((base, modifier)) => (base, Some(modifier)),
        None => (locale, None),
    };
    let base = match base.split_once('.') {
        Some((name, codeset)) if codeset.eq_ignore_ascii_case("utf8") || codeset.eq_ignore_ascii_case("utf-8") => {
            format!("{name}.UTF-8")
        }
        _ => base.to_string(),
    };
    match modifier {
        Some(modifier) => format!("{base}@{modifier}"),
        None => base,
    }
}

#[must_use]
pub fn parse_locale_list(text: &str) -> Vec<String> {
    let mut list: Vec<String> =
        text.lines().map(normalise).filter(|name| !name.is_empty() && name != "POSIX" && name != "C").collect();
    list.sort();
    list.dedup();
    list
}

#[must_use]
pub fn installed() -> Vec<String> {
    run::output("localectl", &["list-locales", "--no-pager"])
        .map(|text| parse_locale_list(&text))
        .filter(|list| !list.is_empty())
        .or_else(|| run::output("locale", &["-a"]).map(|text| parse_locale_list(&text)))
        .unwrap_or_default()
}

#[must_use]
pub fn parse_keywords(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            let value = value.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')).unwrap_or(value);
            Some((key.trim().to_string(), value.to_string()))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Formats {
    pub locale: String,
    pub language: String,
    pub territory: String,
    pub days: Vec<String>,
    pub short_days: Vec<String>,
    pub months: Vec<String>,
    pub short_months: Vec<String>,
    pub am: String,
    pub pm: String,
    pub date: String,
    pub time: String,
    pub date_time: String,
    pub first_weekday: u8,
    pub decimal_point: String,
    pub thousands_sep: String,
    pub grouping: Vec<u8>,
    pub currency: String,
    pub currency_code: String,
    pub money_decimal: String,
    pub money_thousands: String,
    pub money_grouping: Vec<u8>,
    pub money_digits: u8,
    pub symbol_first: bool,
    pub symbol_space: bool,
    pub metric: bool,
    pub paper: (u32, u32),
}

fn list(value: Option<&String>) -> Vec<String> {
    value.map(|value| value.split(';').map(str::to_string).collect()).unwrap_or_default()
}

fn grouping(value: Option<&String>) -> Vec<u8> {
    value
        .map(|value| {
            value
                .split(';')
                .filter_map(|part| part.trim().parse::<i32>().ok())
                .filter_map(|size| u8::try_from(size).ok())
                .filter(|size| *size > 0)
                .collect()
        })
        .unwrap_or_default()
}

#[must_use]
pub fn first_weekday(base_date: &str, first: &str) -> u8 {
    let base = base_date.trim();
    let base_weekday = (base.len() == 8)
        .then(|| {
            let year: i64 = base[..4].parse().ok()?;
            let month: u8 = base[4..6].parse().ok()?;
            let day: u8 = base[6..].parse().ok()?;
            Some(super::tz::weekday_of_days(super::tz::days_from_civil(year, month, day)))
        })
        .flatten()
        .unwrap_or(0);
    let first: u8 = first.trim().parse().unwrap_or(1).clamp(1, 7);
    (base_weekday + first - 1) % 7
}

#[must_use]
pub fn formats_from<S: std::hash::BuildHasher>(locale: &str, map: &HashMap<String, String, S>) -> Formats {
    let text = |key: &str| map.get(key).cloned().unwrap_or_default();
    let number = |key: &str| map.get(key).and_then(|value| value.trim().parse::<i32>().ok());
    let am_pm = list(map.get("am_pm"));
    Formats {
        locale: locale.to_string(),
        language: text("language"),
        territory: text("territory"),
        days: list(map.get("day")),
        short_days: list(map.get("abday")),
        months: list(map.get("mon")),
        short_months: list(map.get("abmon")),
        am: am_pm.first().cloned().unwrap_or_default(),
        pm: am_pm.get(1).cloned().unwrap_or_default(),
        date: text("d_fmt"),
        time: text("t_fmt"),
        date_time: text("d_t_fmt"),
        first_weekday: first_weekday(&text("week-1stday"), &text("first_weekday")),
        decimal_point: text("decimal_point"),
        thousands_sep: text("thousands_sep"),
        grouping: grouping(map.get("grouping")),
        currency: text("currency_symbol"),
        currency_code: text("int_curr_symbol").trim().to_string(),
        money_decimal: text("mon_decimal_point"),
        money_thousands: text("mon_thousands_sep"),
        money_grouping: grouping(map.get("mon_grouping")),
        money_digits: number("frac_digits")
            .and_then(|digits| u8::try_from(digits).ok())
            .filter(|digits| *digits <= 4)
            .unwrap_or(2),
        symbol_first: number("p_cs_precedes") != Some(0),
        symbol_space: number("p_sep_by_space").is_some_and(|space| space > 0),
        metric: number("measurement") != Some(2),
        paper: (
            number("height").and_then(|value| u32::try_from(value).ok()).unwrap_or(0),
            number("width").and_then(|value| u32::try_from(value).ok()).unwrap_or(0),
        ),
    }
}

#[must_use]
pub fn formats(locale: &str) -> Option<Formats> {
    let output = Command::new("locale")
        .args(["-k", "LC_IDENTIFICATION", "LC_TIME", "LC_NUMERIC", "LC_MONETARY", "LC_MEASUREMENT", "LC_PAPER"])
        .env_clear()
        .env("LC_ALL", locale)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || stderr.contains("Cannot set LC_ALL") {
        return None;
    }
    let map = parse_keywords(&String::from_utf8_lossy(&output.stdout));
    Some(formats_from(locale, &map))
}

#[must_use]
pub fn all_formats(locales: &[String]) -> Vec<Formats> {
    locales.iter().take(MAX_LOCALES).filter_map(|locale| formats(locale)).collect()
}

#[must_use]
pub fn session_time_locale() -> String {
    ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("C.UTF-8"))
}

#[must_use]
pub fn display_name(formats: Option<&Formats>, code: &str) -> String {
    match formats {
        Some(formats) if !formats.language.is_empty() && !formats.territory.is_empty() => {
            if formats.language.contains(&formats.territory) {
                formats.language.clone()
            } else {
                format!("{} ({})", formats.language, formats.territory)
            }
        }
        Some(formats) if !formats.language.is_empty() => formats.language.clone(),
        _ => code.to_string(),
    }
}

fn group_digits(digits: &str, sizes: &[u8], separator: &str) -> String {
    if sizes.is_empty() || separator.is_empty() {
        return digits.to_string();
    }
    let chars: Vec<char> = digits.chars().collect();
    let mut groups: Vec<String> = Vec::new();
    let mut end = chars.len();
    let mut index = 0;
    while end > 0 {
        let size = usize::from(sizes[index.min(sizes.len() - 1)]);
        let start = end.saturating_sub(size);
        groups.push(chars[start..end].iter().collect());
        end = start;
        index += 1;
    }
    groups.reverse();
    groups.join(separator)
}

fn fixed(value: f64, digits: u8) -> (String, String) {
    let text = format!("{:.*}", usize::from(digits), value.abs());
    match text.split_once('.') {
        Some((whole, fraction)) => (whole.to_string(), fraction.to_string()),
        None => (text, String::new()),
    }
}

#[must_use]
pub fn format_number(value: f64, digits: u8, formats: &Formats) -> String {
    let (whole, fraction) = fixed(value, digits);
    let point = if formats.decimal_point.is_empty() { "." } else { formats.decimal_point.as_str() };
    let mut out = String::new();
    if value < 0.0 {
        out.push('-');
    }
    out.push_str(&group_digits(&whole, &formats.grouping, &formats.thousands_sep));
    if !fraction.is_empty() {
        out.push_str(point);
        out.push_str(&fraction);
    }
    out
}

#[must_use]
pub fn format_money(value: f64, formats: &Formats) -> String {
    let (whole, fraction) = fixed(value, formats.money_digits);
    let point = if formats.money_decimal.is_empty() { "." } else { formats.money_decimal.as_str() };
    let mut amount = group_digits(&whole, &formats.money_grouping, &formats.money_thousands);
    if !fraction.is_empty() {
        amount.push_str(point);
        amount.push_str(&fraction);
    }
    let symbol = if formats.currency.is_empty() { formats.currency_code.as_str() } else { formats.currency.as_str() };
    if symbol.is_empty() {
        return amount;
    }
    let space = if formats.symbol_space { "\u{a0}" } else { "" };
    if formats.symbol_first { format!("{symbol}{space}{amount}") } else { format!("{amount}{space}{symbol}") }
}

#[must_use]
pub fn paper_name(height: u32, width: u32) -> String {
    match (height, width) {
        (297, 210) => String::from("A4"),
        (279, 216) => String::from("Letter"),
        (356, 216) => String::from("Legal"),
        (0, _) | (_, 0) => String::new(),
        _ => format!("{width} × {height} mm"),
    }
}

fn pick(names: &[String], index: usize) -> &str {
    names.get(index).map_or("", String::as_str)
}

fn padded(value: u8) -> String {
    format!("{value:>2}")
}

fn two_digit_year(year: i64) -> String {
    format!("{:02}", year.rem_euclid(100))
}

#[must_use]
pub fn strftime(pattern: &str, when: &Civil, formats: &Formats, zone: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        let mut pad = true;
        let mut spec = chars.next().unwrap_or('%');
        while matches!(spec, '-' | '_' | '0' | '^' | '#' | 'E' | 'O') {
            if spec == '-' {
                pad = false;
            }
            spec = chars.next().unwrap_or('%');
        }
        let two = |value: u8| {
            if pad { format!("{value:02}") } else { value.to_string() }
        };
        let hour12 = match when.hour % 12 {
            0 => 12,
            hour => hour,
        };
        let meridiem = if when.hour < 12 { formats.am.as_str() } else { formats.pm.as_str() };
        let weekday = usize::from(when.weekday);
        let month = usize::from(when.month.saturating_sub(1));
        match spec {
            'a' => out.push_str(pick(&formats.short_days, weekday)),
            'A' => out.push_str(pick(&formats.days, weekday)),
            'b' | 'h' => out.push_str(pick(&formats.short_months, month)),
            'B' => out.push_str(pick(&formats.months, month)),
            'd' => out.push_str(&two(when.day)),
            'e' => out.push_str(&padded(when.day)),
            'm' => out.push_str(&two(when.month)),
            'y' => out.push_str(&two_digit_year(when.year)),
            'Y' | 'G' => out.push_str(&when.year.to_string()),
            'H' => out.push_str(&two(when.hour)),
            'k' => out.push_str(&padded(when.hour)),
            'I' => out.push_str(&two(hour12)),
            'l' => out.push_str(&padded(hour12)),
            'M' => out.push_str(&two(when.minute)),
            'S' => out.push_str(&two(when.second)),
            'p' => out.push_str(meridiem),
            'P' => out.push_str(&meridiem.to_lowercase()),
            'Z' => out.push_str(zone),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => {
                let nested = if meridiem.is_empty() { "%H:%M:%S" } else { "%I:%M:%S %p" };
                out.push_str(&strftime(nested, when, formats, zone));
            }
            'R' => out.push_str(&strftime("%H:%M", when, formats, zone)),
            'T' => out.push_str(&strftime("%H:%M:%S", when, formats, zone)),
            'D' => out.push_str(&strftime("%m/%d/%y", when, formats, zone)),
            'F' => out.push_str(&strftime("%Y-%m-%d", when, formats, zone)),
            'x' => out.push_str(&strftime(&formats.date, when, formats, zone)),
            'X' => out.push_str(&strftime(&formats.time, when, formats, zone)),
            'c' => out.push_str(&strftime(&formats.date_time, when, formats, zone)),
            '%' => out.push('%'),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out.trim().to_string()
}
