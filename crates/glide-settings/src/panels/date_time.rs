use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::{choice, error_banner, switch_row};
use super::{Ctx, Pane};
use crate::i18n::Lang;
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::locale;
use crate::services::run::{Generation, blocking};
use crate::services::timedate::{self, TimeState, ZoneEntry};
use crate::services::tz::{self, Civil, Zone};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Synchronised => "Synchronised", "Синхронізовано";
        NotSynchronised => "Not synchronised", "Не синхронізовано";
        ManualTime => "Set manually", "Встановлено вручну";
        TimeZone => "Time zone", "Часовий пояс";
        TimeZoneHint => "Used by the clock, the calendar and notification times", "Використовується годинником, календарем і часом сповіщень";
        Change => "Change…", "Змінити…";
        Done => "Done", "Готово";
        SearchZones => "Search for a city or a region", "Пошук міста або регіону";
        NoZones => "No matching time zones", "Немає відповідних часових поясів";
        LoadingZones => "Loading time zones…", "Завантаження часових поясів…";
        MoreZones => "Keep typing to narrow the list", "Уточніть запит, щоб звузити список";
        DateAndTime => "Date & time", "Дата й час";
        Automatic => "Set time automatically", "Встановлювати час автоматично";
        AutomaticHint => "Synchronise with network time servers", "Синхронізувати із серверами точного часу";
        NoNtp => "No time synchronisation service is installed", "Службу синхронізації часу не встановлено";
        Date => "Date", "Дата";
        DateHint => "Year, month and day", "Рік, місяць і день";
        Time => "Time", "Час";
        TimeHint => "24-hour, in the selected time zone", "24-годинний формат, у вибраному поясі";
        SetTime => "Set", "Встановити";
        InvalidDate => "Use YYYY-MM-DD", "Формат РРРР-ММ-ДД";
        InvalidTime => "Use HH:MM", "Формат ГГ:ХХ";
        HardwareClock => "Hardware clock", "Апаратний годинник";
        HardwareClockHint => "How the firmware clock is kept", "Як зберігається час у мікропрограмі";
        RtcUtc => "UTC", "UTC";
        RtcLocal => "Local time", "Місцевий час";
        RtcLocalWarn => "Keeping the hardware clock in local time breaks daylight saving changes. Use timedatectl set-local-rtc 0 to fix it.", "Апаратний годинник у місцевому часі ламає переведення годинника. Виправте командою timedatectl set-local-rtc 0.";
        TopBarClock => "Clock in the top bar", "Годинник у верхній панелі";
        TopBarClockHint => "How Glide shows the time", "Як Glide показує час";
        TimeFormat => "Time format", "Формат часу";
        FormatAuto => "Automatic", "Автоматично";
        Format24 => "24-hour", "24 години";
        Format12 => "12-hour", "12 годин";
        ShowDate => "Show the date", "Показувати дату";
        ShowDateHint => "Weekday, day and month next to the time", "День тижня, число й місяць поруч із часом";
        WeekStart => "Week starts on", "Тиждень починається з";
        WeekStartHint => "Follows the date format of your region", "Відповідає формату дати вашого регіону";
        Unavailable => "timedated is not available, so the time can only be viewed", "Служба timedated недоступна, тому час можна лише переглядати";
        Applying => "Applying…", "Застосування…";
    }
}

use tr::Key as T;

const MAX_RESULTS: usize = 8;

const WEEKDAYS_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const WEEKDAYS_UK: [&str; 7] = ["неділя", "понеділок", "вівторок", "середа", "четвер", "п'ятниця", "субота"];
const WEEK_START_UK: [&str; 7] = ["неділі", "понеділка", "вівторка", "середи", "четверга", "п'ятниці", "суботи"];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTHS_UK: [&str; 12] = [
    "січня",
    "лютого",
    "березня",
    "квітня",
    "травня",
    "червня",
    "липня",
    "серпня",
    "вересня",
    "жовтня",
    "листопада",
    "грудня",
];

#[must_use]
pub fn weekday_name(weekday: u8, lang: Lang) -> &'static str {
    let index = usize::from(weekday % 7);
    match lang {
        Lang::Uk => WEEKDAYS_UK[index],
        Lang::En => WEEKDAYS_EN[index],
    }
}

#[must_use]
pub fn long_date(when: &Civil, lang: Lang) -> String {
    let (months, suffix) = match lang {
        Lang::Uk => (MONTHS_UK, " р."),
        Lang::En => (MONTHS_EN, ""),
    };
    format!(
        "{}, {} {} {}{suffix}",
        weekday_name(when.weekday, lang),
        when.day,
        months[usize::from(when.month.clamp(1, 12) - 1)],
        when.year
    )
}

fn named_abbr(abbr: &str) -> Option<&str> {
    (!abbr.is_empty() && !abbr.starts_with(['+', '-'])).then_some(abbr)
}

fn zone_detail(entry: &ZoneEntry, abbr: Option<&str>) -> String {
    let offset = tz::format_offset(entry.offset);
    [entry.region.as_str(), abbr.unwrap_or_default(), &offset]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[must_use]
pub fn clock_text(when: &Civil, twelve_hour: bool) -> String {
    if twelve_hour {
        let hour = match when.hour % 12 {
            0 => 12,
            hour => hour,
        };
        let suffix = if when.hour < 12 { "AM" } else { "PM" };
        format!("{hour}:{:02} {suffix}", when.minute)
    } else {
        format!("{:02}:{:02}", when.hour, when.minute)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockFormat {
    Auto,
    H24,
    H12,
}

impl ClockFormat {
    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_bool("topbar", "clock-24h") {
            Some(true) => Self::H24,
            Some(false) => Self::H12,
            None => Self::Auto,
        }
    }

    #[must_use]
    pub fn twelve_hour(self, locale_twelve: bool) -> bool {
        match self {
            Self::Auto => locale_twelve,
            Self::H24 => false,
            Self::H12 => true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub state: TimeState,
    pub zone: Option<Zone>,
    pub week_start: Option<u8>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Box<Snapshot>),
    ZonesLoaded(Vec<ZoneEntry>),
    TogglePicker,
    Query(String),
    PickZone(String),
    Ntp(bool),
    DateText(String),
    TimeText(String),
    ApplyTime,
    Applied(Option<Result<(), String>>),
    Format(ClockFormat),
    ShowDate(bool),
}

#[allow(clippy::struct_excessive_bools)] // Independent view flags, not a state machine
#[derive(Debug, Default)]
pub struct Page {
    state: Option<TimeState>,
    zone: Option<Zone>,
    week_start: Option<u8>,
    zones: Vec<ZoneEntry>,
    loading_zones: bool,
    now: i64,
    picking: bool,
    query: String,
    date_text: String,
    time_text: String,
    busy: bool,
    error: Option<String>,
    generation: Generation,
    locale_twelve: bool,
}

fn snapshot(with_locale: bool) -> Snapshot {
    let state = timedate::read();
    let zone = Zone::load(&state.timezone).or_else(|| Some(Zone::system()));
    let week_start =
        with_locale.then(|| locale::formats(&locale::session_time_locale()).map(|f| f.first_weekday)).flatten();
    Snapshot { state, zone, week_start }
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        let generation = self.generation.bump();
        let with_locale = self.week_start.is_none();
        Task::perform(blocking(move || snapshot(with_locale)), move |snapshot| {
            Msg::Loaded(generation, Box::new(snapshot.unwrap_or_default()))
        })
    }

    fn local_now(&self) -> Civil {
        self.zone.as_ref().map_or_else(|| tz::civil(self.now), |zone| zone.local(self.now))
    }

    fn fill_manual(&mut self) {
        let now = self.local_now();
        self.date_text = format!("{:04}-{:02}-{:02}", now.year, now.month, now.day);
        self.time_text = format!("{:02}:{:02}", now.hour, now.minute);
    }

    fn manual_target(&self) -> Option<i64> {
        let (year, month, day) = timedate::parse_date(&self.date_text)?;
        let (hour, minute, second) = timedate::parse_time(&self.time_text)?;
        let local = tz::from_civil(year, month, day, hour, minute, second);
        Some(self.zone.as_ref().map_or(local, |zone| zone.to_utc(local)))
    }

    fn apply(&mut self, job: impl FnOnce() -> Result<(), String> + Send + 'static) -> Task<Msg> {
        self.busy = true;
        self.error = None;
        self.generation.bump();
        Task::perform(blocking(job), Msg::Applied)
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "time",
        "date",
        "clock",
        "timezone",
        "time zone",
        "ntp",
        "24-hour",
        "calendar",
        "час",
        "дата",
        "годинник",
        "часовий пояс",
        "синхронізація",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.now = tz::now();
        self.locale_twelve = timedate::env_uses_12_hour();
        let mut tasks = vec![self.refresh()];
        if self.zones.is_empty() && !self.loading_zones {
            self.loading_zones = true;
            tasks.push(Task::perform(blocking(timedate::zones), |zones| Msg::ZonesLoaded(zones.unwrap_or_default())));
        }
        Task::batch(tasks)
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.now = tz::now();
        if self.busy { Task::none() } else { self.refresh() }
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, snapshot) => {
                if self.generation.is_current(generation) {
                    let first = self.state.is_none();
                    let Snapshot { state, zone, week_start } = *snapshot;
                    self.state = Some(state);
                    self.zone = zone;
                    self.week_start = week_start.or(self.week_start);
                    self.now = tz::now();
                    if first || self.date_text.is_empty() {
                        self.fill_manual();
                    }
                }
            }
            Msg::ZonesLoaded(zones) => {
                self.zones = zones;
                self.loading_zones = false;
            }
            Msg::TogglePicker => {
                self.picking = !self.picking;
                self.query.clear();
            }
            Msg::Query(query) => self.query = query,
            Msg::PickZone(zone) => {
                self.picking = false;
                self.query.clear();
                if let Some(state) = &mut self.state {
                    state.timezone.clone_from(&zone);
                }
                self.zone = Zone::load(&zone).or_else(|| self.zone.take());
                return self.apply(move || timedate::set_timezone(&zone));
            }
            Msg::Ntp(enabled) => {
                if let Some(state) = &mut self.state {
                    state.ntp = enabled;
                }
                if !enabled {
                    self.fill_manual();
                }
                return self.apply(move || timedate::set_ntp(enabled));
            }
            Msg::DateText(value) => self.date_text = value,
            Msg::TimeText(value) => self.time_text = value,
            Msg::ApplyTime => {
                if let Some(target) = self.manual_target() {
                    return self.apply(move || timedate::set_time(target));
                }
            }
            Msg::Applied(result) => {
                self.busy = false;
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.refresh();
            }
            Msg::Format(format) => match format {
                ClockFormat::Auto => config.remove("topbar", "clock-24h"),
                ClockFormat::H24 => config.set_bool("topbar", "clock-24h", true),
                ClockFormat::H12 => config.set_bool("topbar", "clock-24h", false),
            },
            Msg::ShowDate(show) => config.set_bool("topbar", "clock-date", show),
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        let t = c.theme;
        let unavailable = self
            .state
            .as_ref()
            .is_some_and(|state| !state.available)
            .then(|| widgets::banner(t, t.warn(), Icon::Info, c.t(T::Unavailable), None));
        column![
            self.hero(c, config),
            error_banner(t, self.error.as_deref()),
            unavailable,
            self.zone_group(c),
            self.time_group(c),
            self.clock_group(c, config),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn hero<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let now = self.local_now();
        let twelve = ClockFormat::from_config(config).twelve_hour(self.locale_twelve);
        let local = self.zone.as_ref().map(|zone| zone.at(self.now));
        let zone_name = self
            .state
            .as_ref()
            .map(|state| state.timezone.clone())
            .filter(|zone| !zone.is_empty())
            .unwrap_or_else(|| String::from("UTC"));
        let mut zone_line = vec![tz::city_name(&zone_name)];
        if let Some(local) = &local {
            zone_line.extend(named_abbr(&local.abbr).map(str::to_string));
            zone_line.push(tz::format_offset(local.offset));
        }
        let zone_line = zone_line.join(" · ");
        let status: Element<'a, Msg> = match &self.state {
            _ if self.busy => widgets::spinner_label(t, c.t(T::Applying)),
            Some(state) if state.ntp && state.synchronized => widgets::status_pill(t, Tone::Ok, c.t(T::Synchronised)),
            Some(state) if state.ntp => widgets::status_pill(t, Tone::Warn, c.t(T::NotSynchronised)),
            Some(state) if state.available => widgets::status_pill(t, Tone::Neutral, c.t(T::ManualTime)),
            _ => widgets::gap(0.0),
        };
        let color = super::Panel::DateTime.color();
        let body = row![
            column![
                text(clock_text(&now, twelve)).size(56).font(theme::SEMIBOLD).color(t.fg_1()),
                text(long_date(&now, c.lang)).size(15).color(t.fg_1()),
                text(zone_line).size(13).color(t.fg_2()),
            ]
            .spacing(2)
            .width(Fill),
            column![widgets::tile(Icon::Clock, color, 64.0), status].spacing(12).align_x(Alignment::End),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        container(body)
            .padding([20, 24])
            .width(Fill)
            .style(move |_: &iced::Theme| theme::hero_card_style(&t, color))
            .into()
    }

    fn zone_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let current = self.state.as_ref().map(|state| state.timezone.clone()).unwrap_or_default();
        let subtitle = self
            .zones
            .iter()
            .find(|entry| entry.name == current)
            .map_or_else(|| current.clone(), |entry| zone_detail(entry, None));
        let note = |key: T| Row::new(t, c.t(key)).dimmed(true).build();
        let editable = self.state.as_ref().is_some_and(|state| state.available) && !self.busy;
        let toggle = if self.picking {
            widgets::primary(t, c.t(T::Done), Some(Msg::TogglePicker))
        } else {
            widgets::secondary(t, c.t(T::Change), editable.then_some(Msg::TogglePicker))
        };
        let mut group = Group::new(t).title(c.t(T::TimeZone)).description(c.t(T::TimeZoneHint)).push(
            Row::new(t, if current.is_empty() { String::from("UTC") } else { tz::city_name(&current) })
                .prefix(widgets::icon(Icon::Globe, 18.0, t.fg_2()))
                .subtitle(subtitle)
                .suffix(toggle)
                .build(),
        );
        if !self.picking {
            return group.build();
        }
        group = group.push(
            container(widgets::field(t, c.t(T::SearchZones), &self.query, Msg::Query, None, false).width(Fill))
                .padding([theme::ROW_PADDING_Y + 2.0, theme::ROW_PADDING_X]),
        );
        if self.loading_zones {
            return group.push(note(T::LoadingZones)).build();
        }
        let matches: Vec<&ZoneEntry> =
            self.zones.iter().filter(|entry| timedate::matches(entry, &self.query)).collect();
        if matches.is_empty() {
            return group.push(note(T::NoZones)).build();
        }
        let local = self.zone.as_ref().map_or(0, |zone| zone.at(self.now).offset);
        group
            .extend(matches.iter().take(MAX_RESULTS).map(|entry| self.zone_row(c, entry, entry.name == current, local)))
            .extend((matches.len() > MAX_RESULTS).then(|| note(T::MoreZones)))
            .build()
    }

    fn zone_row(&self, c: &Ctx, entry: &ZoneEntry, selected: bool, local: i32) -> Element<'_, Msg> {
        let t = c.theme;
        let when = tz::civil(self.now + i64::from(entry.offset));
        let suffix = row![
            column![
                text(format!("{:02}:{:02}", when.hour, when.minute)).size(13).font(theme::SEMIBOLD).color(t.fg_1()),
                text(offset_delta(entry.offset - local, c.lang)).size(11.5).color(t.fg_3()),
            ]
            .align_x(Alignment::End),
            if selected {
                widgets::icon(Icon::Check, 16.0, t.acc_text())
            } else {
                iced::widget::space().width(16).into()
            },
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        Row::new(t, entry.city.clone())
            .subtitle(zone_detail(entry, named_abbr(&entry.abbr)))
            .suffix(suffix)
            .on_press(Msg::PickZone(entry.name.clone()))
            .build()
    }

    fn time_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let state = self.state.clone().unwrap_or_default();
        let ntp_control: Element<'_, Msg> = if state.can_ntp && !self.busy {
            widgets::switch(t, state.ntp, Msg::Ntp)
        } else {
            widgets::switch_disabled(t, state.ntp)
        };
        let mut group = Group::new(t).title(c.t(T::DateAndTime)).push(
            Row::new(t, c.t(T::Automatic))
                .subtitle(if state.can_ntp || !state.available { c.t(T::AutomaticHint) } else { c.t(T::NoNtp) })
                .suffix(ntp_control)
                .build(),
        );
        if state.available && !state.ntp {
            let date_ok = timedate::parse_date(&self.date_text).is_some();
            let time_ok = timedate::parse_time(&self.time_text).is_some();
            let submit = (date_ok && time_ok && !self.busy).then_some(Msg::ApplyTime);
            group = group
                .push(
                    Row::new(t, c.t(T::Date))
                        .subtitle(if date_ok { c.t(T::DateHint) } else { c.t(T::InvalidDate) })
                        .suffix(widgets::flexible(
                            widgets::field(t, "2026-09-24", &self.date_text, Msg::DateText, submit.clone(), !date_ok)
                                .width(Fill),
                            180.0,
                        ))
                        .build(),
                )
                .push(
                    Row::new(t, c.t(T::Time))
                        .subtitle(if time_ok { c.t(T::TimeHint) } else { c.t(T::InvalidTime) })
                        .suffix(widgets::flexible(
                            row![
                                widgets::field(t, "14:05", &self.time_text, Msg::TimeText, submit.clone(), !time_ok,)
                                    .width(Length::Fill),
                                widgets::primary(t, c.t(T::SetTime), submit),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            240.0,
                        ))
                        .build(),
                );
        }
        let (tone, rtc) = if state.local_rtc { (Tone::Warn, T::RtcLocal) } else { (Tone::Neutral, T::RtcUtc) };
        group = group.push(
            Row::new(t, c.t(T::HardwareClock))
                .subtitle(c.t(T::HardwareClockHint))
                .suffix(widgets::pill(t, tone, c.t(rtc)))
                .build(),
        );
        let warning = state.local_rtc.then(|| widgets::banner(t, t.warn(), Icon::Warning, c.t(T::RtcLocalWarn), None));
        column![group.build(), warning].spacing(12).into()
    }

    fn clock_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let format = ClockFormat::from_config(config);
        let now = self.local_now();
        let example = clock_text(&now, format.twelve_hour(self.locale_twelve));
        let formats = [
            (ClockFormat::Auto, c.t(T::FormatAuto)),
            (ClockFormat::H24, c.t(T::Format24)),
            (ClockFormat::H12, c.t(T::Format12)),
        ];
        let week_start = self.week_start.map(|weekday| {
            let name = match c.lang {
                Lang::Uk => WEEK_START_UK[usize::from(weekday % 7)],
                Lang::En => weekday_name(weekday, c.lang),
            };
            Row::new(t, c.t(T::WeekStart))
                .subtitle(c.t(T::WeekStartHint))
                .suffix(text(name).size(13).color(t.fg_2()))
                .build()
        });
        Group::new(t)
            .title(c.t(T::TopBarClock))
            .description(c.t(T::TopBarClockHint))
            .push(
                Row::new(t, c.t(T::TimeFormat))
                    .subtitle(example)
                    .suffix(choice(*c, formats, format, Msg::Format))
                    .build(),
            )
            .push(switch_row(
                *c,
                c.t(T::ShowDate),
                c.t(T::ShowDateHint),
                config.bool_or("topbar", "clock-date", true),
                Msg::ShowDate,
            ))
            .extend(week_start)
            .build()
    }
}

#[must_use]
pub fn offset_delta(seconds: i32, lang: Lang) -> String {
    if seconds == 0 {
        return String::from(match lang {
            Lang::Uk => "як тут",
            Lang::En => "same time",
        });
    }
    let minutes = seconds.unsigned_abs() / 60;
    let (hours, rest) = (minutes / 60, minutes % 60);
    let amount = if rest == 0 { format!("{hours}") } else { format!("{hours}:{rest:02}") };
    let unit = match lang {
        Lang::Uk => "год",
        Lang::En => "h",
    };
    let sign = if seconds > 0 { '+' } else { '−' };
    format!("{sign}{amount} {unit}")
}
