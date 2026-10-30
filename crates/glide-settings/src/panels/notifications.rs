use std::collections::HashMap;
use std::path::PathBuf;

use glide_config::search_providers::same_app;
use glide_config::senders::Senders;
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{self as sdk, text};
use iced::widget::{column, row};
use iced::{Alignment, Element, Task};

use super::apps::app_icon;
use super::common::{Choice, gated, toggle};
use super::{Ctx, Pane};
use crate::i18n::Lang;
use crate::icons::Icon;
use crate::services::apps::{self, DesktopApp};
use crate::services::config::{ConfigStore, parse_clock};
use crate::services::run::blocking;
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Dnd => "Do Not Disturb", "Не турбувати";
        DndOn => "Banners are hidden. Everything lands in the notification centre.", "Банери приховано. Усе потрапляє в центр сповіщень.";
        DndOnUntil => "Banners are hidden until {time}. Everything lands in the notification centre.", "Банери приховано до {time}. Усе потрапляє в центр сповіщень.";
        DndOff => "Off · banners appear as notifications arrive", "Вимкнено · банери з'являються, щойно надходять сповіщення";
        DndOffUntil => "Off · turns on at {time} on schedule", "Вимкнено · увімкнеться о {time} за розкладом";
        Quiet => "Schedule and exceptions", "Розклад і винятки";
        Schedule => "Turn on every day", "Вмикати щодня";
        ScheduleHint => "A manual toggle holds until the next scheduled change", "Ручне перемикання діє до наступної зміни за розкладом";
        TimeInvalid => "Use 24-hour HH:MM", "Формат 24 години, ГГ:ХХ";
        AllowCritical => "Let urgent ones through", "Пропускати термінові";
        AllowCriticalHint => "A low battery, password requests and the like always show", "Низький заряд, запити паролів і подібне показуються завжди";
        Banners => "Banners", "Банери";
        Duration => "Duration", "Тривалість";
        DurationHint => "Notifications with buttons or a reply stay until you close them", "Сповіщення з кнопками чи відповіддю лишаються, доки їх не закрити";
        AppDecides => "As the app asks", "Як просить програма";
        Seconds => "s", "с";
        Sounds => "Play sounds", "Відтворювати звуки";
        SoundsHint => "When an app asks for one. Silent during Do Not Disturb", "Коли програма просить звук. Тиша в режимі «Не турбувати»";
        Apps => "Apps", "Програми";
        AppsHint => "Those that sent notifications in the last 30 days", "Ті, що надсилали сповіщення за останні 30 днів";
        NoApps => "No app has sent a notification yet", "Жодна програма ще не надсилала сповіщень";
        Banner => "Banners", "Банери";
        Sound => "Sound", "Звук";
        CentreOnly => "Only in the notification centre", "Лише в центрі сповіщень";
        Last => "last", "останнє";
        System => "From the system", "Від системи";
        SystemHint => "Notices Glide shows itself", "Повідомлення, які показує сам Glide";
        Drives => "A drive or memory stick is plugged in", "Підключено флешку чи диск";
        DrivesHint => "With an Open button", "З кнопкою «Відкрити»";
        DiskSpace => "Low on disk space", "Мало місця на диску";
        DiskSpaceHint => "When less than 5 % and less than 2 GB are free", "Коли вільно менше 5 % і менше 2 ГБ";
        Headsets => "Headphones connected", "Підключено навушники";
        HeadsetsHint => "Their name and battery in the island", "Назва й заряд у острові";
        Codes => "One-time codes from messages", "Одноразові коди з повідомлень";
        CodesHint => "Offers to type the code into the field that waits for it", "Пропозиція вставити код у поле, яке його чекає";
    }
}

use tr::Key as T;

const SECTION: &str = "notifications";
const MUTED: &str = "muted-apps";
const SILENT: &str = "silent-apps";
const DURATIONS: [i64; 4] = [0, 5, 10, 20];
const SYSTEM: [(&str, T, T); 4] = [
    ("drive-notices", T::Drives, T::DrivesHint),
    ("disk-space-notices", T::DiskSpace, T::DiskSpaceHint),
    ("headset-notices", T::Headsets, T::HeadsetsHint),
    ("code-offers", T::Codes, T::CodesHint),
];

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    apps: Vec<DesktopApp>,
    icons: HashMap<String, PathBuf>,
    senders: Senders,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Lang, Box<Catalog>),
    Dnd(bool),
    Schedule(bool),
    From(String),
    To(String),
    AllowCritical(bool),
    Sounds(bool),
    Duration(Choice),
    Banners(String, String, bool),
    Sound(String, String, bool),
    System(&'static str, bool),
}

#[derive(Debug, Default)]
pub struct Page {
    catalog: Catalog,
    lang: Option<Lang>,
    from: Option<String>,
    to: Option<String>,
}

#[must_use]
pub(crate) fn names_app(entry: &str, key: &str, name: &str) -> bool {
    let entry = entry.trim();
    !entry.is_empty() && (same_app(entry, key) || (!name.is_empty() && same_app(entry, name)))
}

#[must_use]
pub(crate) fn listed(list: &[String], key: &str, name: &str) -> bool {
    list.iter().any(|entry| names_app(entry, key, name))
}

#[must_use]
pub(crate) fn with_app(list: &[String], key: &str, name: &str, present: bool) -> Vec<String> {
    let mut next: Vec<String> = list.iter().filter(|entry| !names_app(entry, key, name)).cloned().collect();
    if present {
        next.push(key.to_string());
    }
    next
}

fn set_list(config: &mut ConfigStore, key: &str, list: &[String]) {
    if list.is_empty() {
        config.remove(SECTION, key);
    } else {
        config.set_str_array(SECTION, key, list);
    }
}

fn set_clock(config: &mut ConfigStore, key: &str, value: String) -> String {
    if parse_clock(&value).is_some() {
        config.set_str(SECTION, key, value.trim());
    }
    value
}

fn load(lang: Lang, theme: Option<&str>) -> Catalog {
    let apps = apps::installed(lang.key());
    let icons = apps::icon_paths(apps.iter().map(|app| app.icon.as_str()), theme);
    Catalog { apps, icons, senders: Senders::load() }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

struct AppLine {
    key: String,
    name: String,
    icon: Option<PathBuf>,
    count: u64,
    last: Option<u64>,
}

impl Page {
    fn lines(&self, config: &ConfigStore) -> Vec<AppLine> {
        let resolve = |key: &str, fallback: &str| {
            let app = self.catalog.apps.iter().find(|app| same_app(&app.id, key));
            let name = app.map_or_else(
                || if fallback.is_empty() { key.to_string() } else { fallback.to_string() },
                |app| app.name.clone(),
            );
            (name, app.and_then(|app| self.catalog.icons.get(&app.icon).cloned()))
        };
        let mut lines: Vec<AppLine> = self
            .catalog
            .senders
            .recent(now())
            .into_iter()
            .map(|(key, sender)| {
                let (name, icon) = resolve(key, &sender.name);
                AppLine { key: key.to_string(), name, icon, count: sender.count, last: Some(sender.last) }
            })
            .collect();
        for entry in config.get_str_array(SECTION, MUTED).into_iter().chain(config.get_str_array(SECTION, SILENT)) {
            let key = entry.trim().to_string();
            if key.is_empty() || lines.iter().any(|line| names_app(&entry, &line.key, &line.name)) {
                continue;
            }
            let (name, icon) = resolve(&key, "");
            lines.push(AppLine { key, name, icon, count: 0, last: None });
        }
        lines
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["notification", "banner", "dnd", "disturb", "sound", "сповіщення", "банер", "турбувати", "звук"];

    fn open(&mut self, c: Ctx, config: &ConfigStore) -> Task<Msg> {
        self.lang = Some(c.lang);
        let lang = c.lang;
        let theme = Some(config.icon_theme());
        Task::perform(blocking(move || load(lang, theme.as_deref())), move |catalog| {
            Msg::Loaded(lang, Box::new(catalog.unwrap_or_default()))
        })
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(lang, catalog) => {
                if self.lang == Some(lang) {
                    self.catalog = *catalog;
                }
            }
            Msg::Dnd(on) => config.set_bool(SECTION, "do-not-disturb", on),
            Msg::Schedule(on) => config.set_bool(SECTION, "schedule", on),
            Msg::From(value) => self.from = Some(set_clock(config, "from", value)),
            Msg::To(value) => self.to = Some(set_clock(config, "to", value)),
            Msg::AllowCritical(on) => config.set_bool(SECTION, "allow-critical", on),
            Msg::Sounds(on) => config.set_bool(SECTION, "sounds", on),
            Msg::Duration(duration) => config.set_int(SECTION, "banner-seconds", duration.value),
            Msg::Banners(key, name, on) => {
                let list = with_app(&config.get_str_array(SECTION, MUTED), &key, &name, !on);
                set_list(config, MUTED, &list);
            }
            Msg::Sound(key, name, on) => {
                let list = with_app(&config.get_str_array(SECTION, SILENT), &key, &name, !on);
                set_list(config, SILENT, &list);
            }
            Msg::System(key, on) => {
                if on {
                    config.remove(SECTION, key);
                } else {
                    config.set_bool(SECTION, key, false);
                }
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![
            hero(&c, config),
            self.quiet(&c, config),
            banners(&c, config),
            self.apps(&c, config),
            system(&c, config)
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

fn hero<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let dnd = config.bool_or(SECTION, "do-not-disturb", false);
    let schedule = config.bool_or(SECTION, "schedule", false);
    let from = config.str_or(SECTION, "from", "22:00");
    let to = config.str_or(SECTION, "to", "07:00");
    let hint = match (dnd, schedule) {
        (true, true) => c.t(T::DndOnUntil).replace("{time}", &to),
        (true, false) => c.t(T::DndOn).to_string(),
        (false, true) => c.t(T::DndOffUntil).replace("{time}", &from),
        (false, false) => c.t(T::DndOff).to_string(),
    };
    widgets::section_hero(
        c.theme,
        if dnd { Icon::Moon } else { Icon::Bell },
        super::Panel::Notifications.color(),
        c.t(T::Dnd),
        hint,
        Some(toggle(*c, dnd, Msg::Dnd)),
    )
}

impl Page {
    fn quiet<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let schedule = config.bool_or(SECTION, "schedule", false);
        let from = self.from.clone().unwrap_or_else(|| config.str_or(SECTION, "from", "22:00"));
        let to = self.to.clone().unwrap_or_else(|| config.str_or(SECTION, "to", "07:00"));
        let (from_bad, to_bad) = (parse_clock(&from).is_none(), parse_clock(&to).is_none());
        let mut times = row![].spacing(8).align_y(Alignment::Center);
        if schedule {
            times = times
                .push(widgets::field(t, "22:00", &from, Msg::From, None, from_bad).width(76))
                .push(text("—", TextStyle::BODY).color(t.fg_3()))
                .push(widgets::field(t, "07:00", &to, Msg::To, None, to_bad).width(76));
        }
        let subtitle = if schedule && (from_bad || to_bad) { T::TimeInvalid } else { T::ScheduleHint };
        Group::new(t)
            .title(c.t(T::Quiet))
            .push(
                Row::new(t, c.t(T::Schedule))
                    .subtitle(c.t(subtitle))
                    .changed(schedule)
                    .suffix(
                        row![gated(*c, times), toggle(*c, schedule, Msg::Schedule)]
                            .spacing(12)
                            .align_y(Alignment::Center),
                    )
                    .stacked(t.compact)
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::AllowCritical))
                    .subtitle(c.t(T::AllowCriticalHint))
                    .changed(!config.bool_or(SECTION, "allow-critical", true))
                    .suffix(toggle(*c, config.bool_or(SECTION, "allow-critical", true), Msg::AllowCritical))
                    .build(),
            )
            .build()
    }

    fn apps<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let paint = widgets::paint(&t);
        let muted = config.get_str_array(SECTION, MUTED);
        let silent = config.get_str_array(SECTION, SILENT);
        let lines = self.lines(config);
        let mut group = Group::new(t).title(c.t(T::Apps)).description(c.t(T::AppsHint));
        if lines.is_empty() {
            return group.push(widgets::list_empty(t, Icon::Bell, c.t(T::NoApps), "")).build();
        }
        let notifications = match c.lang {
            Lang::Uk => ["сповіщення", "сповіщення", "сповіщень"],
            Lang::En => ["notification", "notifications", "notifications"],
        };
        for line in lines {
            let banners = !listed(&muted, &line.key, &line.name);
            let sound = !listed(&silent, &line.key, &line.name);
            let subtitle = match (banners, line.last) {
                (false, _) => c.t(T::CentreOnly).to_string(),
                (true, Some(last)) => format!(
                    "{} · {} {}",
                    glide_sdk::i18n::count(line.count, c.lang, notifications),
                    c.t(T::Last),
                    glide_sdk::i18n::ago(now().saturating_sub(last), c.lang)
                ),
                (true, None) => String::new(),
            };
            let (key, name) = (line.key.clone(), line.name.clone());
            let (sound_key, sound_name) = (line.key.clone(), line.name.clone());
            let switches = row![
                sdk::labelled_switch(
                    &paint,
                    c.t(T::Banner),
                    banners,
                    c.writable.then_some(move |on| Msg::Banners(key.clone(), name.clone(), on)),
                ),
                sdk::labelled_switch(
                    &paint,
                    c.t(T::Sound),
                    sound,
                    c.writable.then_some(move |on| Msg::Sound(sound_key.clone(), sound_name.clone(), on)),
                ),
            ]
            .spacing(16)
            .align_y(Alignment::Center);
            group = group.push(
                Row::new(t, line.name.clone())
                    .subtitle(subtitle)
                    .prefix(app_icon(&t, line.icon.as_ref(), &line.name, 28.0))
                    .suffix(switches)
                    .stacked(t.compact)
                    .build(),
            );
        }
        group.build()
    }
}

fn banners<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let seconds = config.int_or(SECTION, "banner-seconds", 0);
    let label = |seconds: i64| {
        if seconds == 0 { c.t(T::AppDecides).to_string() } else { format!("{seconds} {}", c.t(T::Seconds)) }
    };
    let mut choices: Vec<(i64, String)> = DURATIONS.iter().map(|value| (*value, label(*value))).collect();
    if !DURATIONS.contains(&seconds) {
        choices.push((seconds, label(seconds)));
    }
    Group::new(t)
        .title(c.t(T::Banners))
        .push(
            Row::new(t, c.t(T::Duration))
                .subtitle(c.t(T::DurationHint))
                .changed(seconds != 0)
                .suffix(gated(
                    *c,
                    widgets::segmented(t, choices, seconds, |value| Msg::Duration(Choice::new(value, ""))),
                ))
                .stacked(t.compact)
                .build(),
        )
        .push(
            Row::new(t, c.t(T::Sounds))
                .subtitle(c.t(T::SoundsHint))
                .changed(!config.bool_or(SECTION, "sounds", true))
                .suffix(toggle(*c, config.bool_or(SECTION, "sounds", true), Msg::Sounds))
                .build(),
        )
        .build()
}

fn system<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    SYSTEM
        .iter()
        .fold(Group::new(t).title(c.t(T::System)).description(c.t(T::SystemHint)), |group, (key, title, hint)| {
            let on = config.bool_or(SECTION, key, true);
            let key: &'static str = key;
            group.push(
                Row::new(t, c.t(*title))
                    .subtitle(c.t(*hint))
                    .changed(!on)
                    .suffix(toggle(*c, on, move |on| Msg::System(key, on)))
                    .build(),
            )
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_is_found_by_its_desktop_id_or_the_name_on_its_notifications() {
        let list = vec!["org.telegram.desktop.desktop".to_string(), "Steam".to_string()];
        assert!(listed(&list, "org.telegram.desktop", "Telegram"));
        assert!(listed(&list, "steam", "Steam"));
        assert!(!listed(&list, "firefox", "Firefox"));
    }

    #[test]
    fn an_app_whose_desktop_id_ends_in_desktop_is_found_by_the_key_settings_wrote() {
        let list = vec!["org.telegram.desktop".to_string()];
        assert!(listed(&list, "org.telegram.desktop", "Telegram"));
        assert!(with_app(&list, "org.telegram.desktop", "Telegram", false).is_empty());
        assert_eq!(with_app(&list, "org.telegram.desktop", "Telegram", true), ["org.telegram.desktop"]);
        assert!(listed(&["firefox".to_string()], "firefox.desktop", "Firefox"));
        assert!(!listed(&list, "org.signal.desktop", "Signal"));
    }

    #[test]
    fn a_sender_and_a_muted_entry_both_resolve_to_the_installed_app() {
        let telegram = DesktopApp {
            id: "org.telegram.desktop.desktop".to_string(),
            name: "Telegram".to_string(),
            icon: "telegram".to_string(),
            ..DesktopApp::default()
        };
        let mut senders = Senders::default();
        senders.note("org.telegram.desktop", "org.telegram.desktop", now());
        let catalog = Catalog {
            apps: vec![telegram],
            icons: HashMap::from([("telegram".to_string(), PathBuf::from("/icons/telegram.svg"))]),
            senders,
        };
        let page = Page { catalog, ..Page::default() };
        let config = ConfigStore::parse("[notifications]\nmuted-apps = [\"org.telegram.desktop\"]\n");
        let lines = page.lines(&config);
        assert_eq!(lines.len(), 1, "the muted entry is the sender, not a second line");
        assert_eq!(lines[0].name, "Telegram");
        assert_eq!(lines[0].icon, Some(PathBuf::from("/icons/telegram.svg")));
        assert_eq!(lines[0].key, "org.telegram.desktop");
        let muted_only = Page { catalog: Catalog { apps: page.catalog.apps, ..Catalog::default() }, ..Page::default() };
        let lines = muted_only.lines(&config);
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].key.as_str(), lines[0].name.as_str()), ("org.telegram.desktop", "Telegram"));
    }

    #[test]
    fn switching_an_app_rewrites_only_its_own_entries() {
        let list = vec!["Steam".to_string(), "firefox".to_string()];
        assert_eq!(with_app(&list, "firefox", "Firefox", false), ["Steam"]);
        assert_eq!(
            with_app(&list, "org.telegram.desktop", "Telegram", true),
            ["Steam", "firefox", "org.telegram.desktop"]
        );
        assert_eq!(with_app(&list, "steam", "Steam", true), ["firefox", "steam"], "no duplicate entry");
    }
}
