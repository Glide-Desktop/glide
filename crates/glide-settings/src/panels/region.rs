use iced::widget::{column, row, text};
use iced::{Alignment, Element, Fill, Task};

use super::common::{error_banner, gated};
use super::{Ctx, Pane};
use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::locale::{self, FORMAT_CATEGORIES, Formats, SystemLocale};
use crate::services::run::{Generation, blocking};
use crate::services::tz::{self, Zone};
use crate::theme::{self, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Language => "Language", "Мова";
        InterfaceLanguage => "Interface language", "Мова інтерфейсу";
        InterfaceLanguageHint => "The shell and Settings", "Оболонка й налаштування";
        SystemLanguage => "System language", "Мова системи";
        SystemLanguageHint => "Apps and the login screen, after signing in again", "Програми й екран входу, після повторного входу";
        Formats => "Formats", "Формати";
        FormatsHint => "Dates, numbers and currency in apps started after the next sign-in", "Дати, числа й валюта в програмах, запущених після наступного входу";
        RegionFormats => "Region", "Регіон";
        RegionFormatsHint => "Sets every format below at once", "Задає всі формати нижче одночасно";
        Mixed => "Mixed", "Змішані";
        SameAsLanguage => "Same as the system language", "Як мова системи";
        DatesTimes => "Dates & times", "Дати й час";
        Numbers => "Numbers", "Числа";
        Currency => "Currency", "Валюта";
        Measurement => "Measurement", "Одиниці вимірювання";
        Paper => "Paper size", "Розмір паперу";
        Preview => "Preview", "Попередній перегляд";
        PreviewHint => "How apps will show this moment", "Як програми показуватимуть цей момент";
        Date => "Date", "Дата";
        Time => "Time", "Час";
        DateTime => "Date and time", "Дата й час";
        Number => "Number", "Число";
        Money => "Money", "Гроші";
        Units => "Units", "Одиниці";
        Metric => "Metric", "Метричні";
        Imperial => "US customary", "Американські";
        FirstDay => "First day of the week", "Перший день тижня";
        Installed => "Installed locales", "Установлені локалі";
        InstalledHint => "Only generated locales can be chosen. Add more in /etc/locale.gen and run locale-gen.", "Можна вибрати лише згенеровані локалі. Додайте інші в /etc/locale.gen і виконайте locale-gen.";
        NoLocales => "No locales found", "Локалей не знайдено";
        InUse => "In use", "Використовується";
        Loading => "Loading…", "Завантаження…";
        Applying => "Applying…", "Застосування…";
        Applied => "Saved. Sign out and back in for apps to pick it up.", "Збережено. Вийдіть і знову увійдіть, щоб програми це підхопили.";
        Unavailable => "localed is not available, so the system locale can only be viewed", "Служба localed недоступна, тому системну локаль можна лише переглядати";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangChoice {
    System { detected: Lang, ui: Lang },
    Explicit(Lang),
}

impl LangChoice {
    #[must_use]
    pub const fn lang(self) -> Option<Lang> {
        match self {
            Self::System { .. } => None,
            Self::Explicit(lang) => Some(lang),
        }
    }

    #[must_use]
    pub fn options(config: &ConfigStore) -> (Vec<Self>, Self) {
        let system = Self::System { detected: config.system_language(), ui: config.language() };
        let current = config.language_choice().map_or(system, Self::Explicit);
        (vec![system, Self::Explicit(Lang::Uk), Self::Explicit(Lang::En)], current)
    }
}

impl std::fmt::Display for LangChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System { detected, ui } => write!(f, "{} ({})", K::LanguageSystem.text(*ui), detected.native_name()),
            Self::Explicit(lang) => f.write_str(lang.native_name()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleChoice {
    pub code: Option<String>,
    pub label: String,
}

impl std::fmt::Display for LocaleChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub system: SystemLocale,
    pub installed: Vec<String>,
    pub formats: Vec<Formats>,
    pub zone: Option<Zone>,
    pub bus: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Box<Snapshot>),
    Interface(LangChoice),
    SystemLanguage(LocaleChoice),
    Region(LocaleChoice),
    Category(&'static str, LocaleChoice),
    Applied(Option<Result<(), String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    snapshot: Option<Snapshot>,
    loading: bool,
    busy: bool,
    error: Option<String>,
    saved: bool,
    generation: Generation,
}

fn load() -> Snapshot {
    let system = locale::read_system();
    let mut installed = locale::installed();
    for (_, value) in &system.vars {
        let value = locale::normalise(value);
        if !value.is_empty() && !installed.contains(&value) {
            installed.push(value);
        }
    }
    installed.sort();
    let formats = locale::all_formats(&installed);
    Snapshot {
        bus: zbus::blocking::Connection::system().is_ok(),
        system,
        installed,
        formats,
        zone: Some(Zone::system()),
    }
}

impl Snapshot {
    #[must_use]
    pub fn formats_for(&self, code: &str) -> Option<&Formats> {
        let code = locale::normalise(code);
        self.formats.iter().find(|formats| locale::normalise(&formats.locale) == code)
    }

    #[must_use]
    pub fn label(&self, code: &str) -> String {
        let name = locale::display_name(self.formats_for(code), code);
        let shared = self
            .installed
            .iter()
            .filter(|other| locale::display_name(self.formats_for(other), other) == name)
            .nth(1)
            .is_some();
        if shared && name != code { format!("{name} · {code}") } else { name }
    }

    #[must_use]
    pub fn choices(&self) -> Vec<LocaleChoice> {
        self.installed.iter().map(|code| LocaleChoice { code: Some(code.clone()), label: self.label(code) }).collect()
    }

    #[must_use]
    pub fn region(&self) -> Option<String> {
        let first = self.system.effective(FORMAT_CATEGORIES[0]).to_string();
        FORMAT_CATEGORIES.iter().all(|category| self.system.effective(category) == first).then_some(first)
    }
}

#[must_use]
pub fn with_region(system: &SystemLocale, code: &str) -> SystemLocale {
    FORMAT_CATEGORIES.iter().fold(system.clone(), |next, category| {
        if code == next.lang() { next.with(category, None) } else { next.with(category, Some(code)) }
    })
}

impl Page {
    fn refresh(&mut self) -> Task<Msg> {
        self.loading = true;
        let generation = self.generation.bump();
        Task::perform(blocking(load), move |snapshot| Msg::Loaded(generation, Box::new(snapshot.unwrap_or_default())))
    }

    fn apply(&mut self, next: SystemLocale) -> Task<Msg> {
        let Some(snapshot) = &mut self.snapshot else {
            return Task::none();
        };
        if snapshot.system == next {
            return Task::none();
        }
        snapshot.system = next.clone();
        self.busy = true;
        self.error = None;
        self.saved = false;
        self.generation.bump();
        Task::perform(blocking(move || locale::set_system(&next)), Msg::Applied)
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "language",
        "locale",
        "region",
        "format",
        "currency",
        "units",
        "metric",
        "paper",
        "мова",
        "локаль",
        "регіон",
        "формат",
        "валюта",
        "одиниці",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.loading || self.busy {
            return Task::none();
        }
        self.refresh()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, snapshot) => {
                if self.generation.is_current(generation) {
                    self.snapshot = Some(*snapshot);
                    self.loading = false;
                }
            }
            Msg::Interface(choice) => config.set_language(choice.lang()),
            Msg::SystemLanguage(choice) => {
                if let (Some(code), Some(snapshot)) = (choice.code, &self.snapshot) {
                    let next = snapshot.system.with("LANG", Some(&code));
                    return self.apply(next);
                }
            }
            Msg::Region(choice) => {
                if let (Some(code), Some(snapshot)) = (choice.code, &self.snapshot) {
                    let next = with_region(&snapshot.system, &code);
                    return self.apply(next);
                }
            }
            Msg::Category(category, choice) => {
                if let Some(snapshot) = &self.snapshot {
                    let code = choice.code.filter(|code| code != snapshot.system.lang());
                    let next = snapshot.system.with(category, code.as_deref());
                    return self.apply(next);
                }
            }
            Msg::Applied(result) => {
                self.busy = false;
                match result {
                    Some(Err(err)) => self.error = Some(err),
                    _ => self.saved = true,
                }
                return self.refresh();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        let t = c.theme;
        let Some(snapshot) = &self.snapshot else {
            return widgets::empty_state(t, Icon::Globe, c.t(T::Loading), "");
        };
        let notice = error_banner(t, self.error.as_deref()).or_else(|| {
            self.saved.then(|| widgets::banner(t, crate::theme::Tone::Accent, Icon::Info, c.t(T::Applied), None))
        });
        let unavailable = (!snapshot.bus)
            .then(|| widgets::banner(t, crate::theme::Tone::Warn, Icon::Info, c.t(T::Unavailable), None));
        column![
            self.hero(c, snapshot),
            notice,
            unavailable,
            self.language_group(c, config, snapshot),
            self.formats_group(c, snapshot),
            preview(c, snapshot),
            installed_group(c, snapshot),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn hero<'a>(&'a self, c: &Ctx, snapshot: &'a Snapshot) -> Element<'a, Msg> {
        let t = c.theme;
        let lang = snapshot.system.lang().to_string();
        let heading = locale::display_name(snapshot.formats_for(&lang), &lang);
        let trailing: Element<'a, Msg> =
            if self.busy { widgets::spinner_label(t, c.t(T::Applying)) } else { widgets::pill(t, Tone::Neutral, lang) };
        let region = snapshot
            .region()
            .map_or_else(|| c.t(T::Mixed).to_string(), |code| locale::display_name(snapshot.formats_for(&code), &code));
        widgets::section_hero(
            t,
            Icon::Globe,
            super::Panel::Region.color(),
            heading,
            format!("{} · {region}", c.t(T::Formats)),
            Some(trailing),
        )
    }

    fn language_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore, snapshot: &'a Snapshot) -> Element<'a, Msg> {
        let t = c.theme;
        let (options, current) = LangChoice::options(config);
        let choices = snapshot.choices();
        let selected = find_choice(&choices, snapshot.system.lang());
        let editable = snapshot.bus && !self.busy && !choices.is_empty();
        Group::new(t)
            .title(c.t(T::Language))
            .push(
                Row::new(t, c.t(T::InterfaceLanguage))
                    .subtitle(c.t(T::InterfaceLanguageHint))
                    .suffix(gated(*c, widgets::dropdown(t, options, Some(current), Msg::Interface, 260.0)))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::SystemLanguage))
                    .subtitle(c.t(T::SystemLanguageHint))
                    .suffix(picker(&t, editable, choices, selected, Msg::SystemLanguage))
                    .build(),
            )
            .build()
    }

    fn formats_group<'a>(&'a self, c: &Ctx, snapshot: &'a Snapshot) -> Element<'a, Msg> {
        let t = c.theme;
        let editable = snapshot.bus && !self.busy && !snapshot.installed.is_empty();
        let choices = snapshot.choices();
        let unset = |key: T| LocaleChoice { code: None, label: c.t(key).to_string() };
        let with_unset = |first: LocaleChoice| -> Vec<LocaleChoice> {
            std::iter::once(first).chain(choices.iter().cloned()).collect()
        };
        let (region_options, region_selected) = match snapshot.region().and_then(|code| find_choice(&choices, &code)) {
            Some(selected) => (choices.clone(), selected),
            None => (with_unset(unset(T::Mixed)), unset(T::Mixed)),
        };
        let region_row = Row::new(t, c.t(T::RegionFormats))
            .subtitle(c.t(T::RegionFormatsHint))
            .prefix(widgets::icon(Icon::Globe, 18.0, t.fg_2()))
            .suffix(picker(&t, editable, region_options, Some(region_selected), Msg::Region))
            .build();
        let category_rows = FORMAT_CATEGORIES.iter().map(|&category| {
            let options = with_unset(unset(T::SameAsLanguage));
            let selected = match snapshot.system.get(category) {
                Some(code) => find_choice(&options, code)
                    .unwrap_or_else(|| LocaleChoice { code: Some(code.to_string()), label: snapshot.label(code) }),
                None => unset(T::SameAsLanguage),
            };
            Row::new(t, c.t(category_key(category)))
                .subtitle(category_sample(c, snapshot, category))
                .suffix(picker(&t, editable, options, Some(selected), move |choice| Msg::Category(category, choice)))
                .build()
        });
        Group::new(t)
            .title(c.t(T::Formats))
            .description(c.t(T::FormatsHint))
            .push(region_row)
            .extend(category_rows)
            .build()
    }
}

fn find_choice(choices: &[LocaleChoice], code: &str) -> Option<LocaleChoice> {
    choices.iter().find(|choice| choice.code.as_deref() == Some(code)).cloned()
}

fn picker<'a>(
    t: &theme::ThemeState, enabled: bool, options: Vec<LocaleChoice>, selected: Option<LocaleChoice>,
    on_select: impl Fn(LocaleChoice) -> Msg + 'a,
) -> Element<'a, Msg> {
    widgets::inert(*t, enabled, widgets::dropdown(*t, options, selected, on_select, 260.0))
}

const fn category_key(category: &str) -> T {
    match category.as_bytes() {
        b"LC_TIME" => T::DatesTimes,
        b"LC_NUMERIC" => T::Numbers,
        b"LC_MONETARY" => T::Currency,
        b"LC_MEASUREMENT" => T::Measurement,
        _ => T::Paper,
    }
}

fn now_parts(snapshot: &Snapshot) -> (tz::Civil, String) {
    let now = tz::now();
    snapshot
        .zone
        .as_ref()
        .map_or_else(|| (tz::civil(now), String::from("UTC")), |zone| (zone.local(now), zone.at(now).abbr))
}

fn category_sample(c: &Ctx, snapshot: &Snapshot, category: &str) -> String {
    let code = snapshot.system.effective(category);
    let Some(formats) = snapshot.formats_for(code) else {
        return code.to_string();
    };
    let (now, zone) = now_parts(snapshot);
    match category {
        "LC_TIME" => format!(
            "{} · {}",
            locale::strftime(&formats.date, &now, formats, &zone),
            locale::strftime(&formats.time, &now, formats, &zone)
        ),
        "LC_NUMERIC" => locale::format_number(1_234_567.89, 2, formats),
        "LC_MONETARY" => locale::format_money(1234.5, formats),
        "LC_MEASUREMENT" => units(c, formats),
        _ => locale::paper_name(formats.paper.0, formats.paper.1),
    }
}

fn units(c: &Ctx, formats: &Formats) -> String {
    c.t(if formats.metric { T::Metric } else { T::Imperial }).to_string()
}

fn preview(c: &Ctx, snapshot: &Snapshot) -> Element<'static, Msg> {
    let t = c.theme;
    let (now, zone) = now_parts(snapshot);
    let formats = |category: &str| snapshot.formats_for(snapshot.system.effective(category));
    let mut rows: Vec<(T, String)> = Vec::new();
    if let Some(formats) = formats("LC_TIME") {
        for (key, pattern) in [(T::Date, &formats.date), (T::Time, &formats.time), (T::DateTime, &formats.date_time)] {
            rows.push((key, locale::strftime(pattern, &now, formats, &zone)));
        }
        let first_day = formats.days.get(usize::from(formats.first_weekday));
        rows.push((T::FirstDay, first_day.cloned().unwrap_or_default()));
    }
    if let Some(formats) = formats("LC_NUMERIC") {
        rows.push((T::Number, locale::format_number(1_234_567.89, 2, formats)));
    }
    if let Some(formats) = formats("LC_MONETARY") {
        rows.push((T::Money, locale::format_money(1234.5, formats)));
    }
    if let Some(formats) = formats("LC_MEASUREMENT") {
        rows.push((T::Units, units(c, formats)));
    }
    if let Some(formats) = formats("LC_PAPER") {
        let name = locale::paper_name(formats.paper.0, formats.paper.1);
        if !name.is_empty() {
            rows.push((T::Paper, name));
        }
    }
    let rows = rows.into_iter().map(|(key, value)| (c.t(key).to_string(), value)).collect();
    column![Group::new(t).title(c.t(T::Preview)).description(c.t(T::PreviewHint)).build(), widgets::info_grid(t, rows),]
        .spacing(8)
        .into()
}

fn installed_group(c: &Ctx, snapshot: &Snapshot) -> Element<'static, Msg> {
    let t = c.theme;
    let group = Group::new(t).title(c.t(T::Installed)).description(c.t(T::InstalledHint));
    if snapshot.installed.is_empty() {
        return group.push(Row::new(t, c.t(T::NoLocales)).dimmed(true).build()).build();
    }
    let used: Vec<String> = snapshot.system.vars.iter().map(|(_, value)| locale::normalise(value)).collect();
    let rows = snapshot.installed.iter().map(|code| {
        let formats = snapshot.formats_for(code);
        let name = locale::display_name(formats, code);
        let sample = formats.map_or_else(String::new, |formats| locale::format_money(1234.5, formats));
        let suffix = row![
            used.contains(code).then(|| widgets::pill(t, Tone::Accent, c.t(T::InUse))),
            text(sample).size(13).color(t.fg_3()),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let subtitle = if name == *code { String::new() } else { code.clone() };
        Row::new(t, name).subtitle(subtitle).suffix(suffix).build()
    });
    column![group.extend(rows).build()].width(Fill).into()
}
