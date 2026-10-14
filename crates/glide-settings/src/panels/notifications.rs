use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::{Choice, gated, presets, toggle};
use super::network::form_row;
use super::{Ctx, Pane};
use crate::icons::Icon;
use crate::services::apps::{self, DesktopApp};
use crate::services::config::{ConfigStore, parse_clock};
use crate::services::run::blocking;
use crate::theme::{self, ThemeState};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Dnd => "Do Not Disturb", "Не турбувати";
        DndOn => "Banners are silenced. Everything still lands in the notification centre.", "Банери вимкнено. Усе й далі потрапляє до центру сповіщень.";
        DndOff => "Banners appear as notifications arrive", "Банери з'являються, щойно надходять сповіщення";
        DndHint => "The bell in the top bar toggles it too", "Також вмикається дзвіночком на верхній панелі";
        Schedule => "Turn on automatically", "Вмикати автоматично";
        ScheduleHint => "Every day between these times. A manual toggle holds until the next change.", "Щодня в цьому проміжку. Ручне перемикання діє до наступної зміни.";
        From => "From", "З";
        To => "To", "До";
        TimeInvalid => "Use 24-hour HH:MM", "Формат 24 години, ГГ:ХХ";
        AllowCritical => "Allow urgent notifications", "Пропускати термінові сповіщення";
        AllowCriticalHint => "Critical alerts, such as a low battery, still show a banner", "Критичні попередження, як-от низький заряд, усе одно показують банер";
        Banners => "Banners", "Банери";
        BannersHint => "Where banners appear and how long they stay", "Де з'являються банери і скільки тримаються";
        TopLeft => "Top left", "Угорі ліворуч";
        TopCenter => "Top centre", "Угорі по центру";
        TopRight => "Top right", "Угорі праворуч";
        Duration => "Show banners for", "Показувати банери";
        DurationHint => "Urgent and persistent notifications stay until dismissed", "Термінові та постійні сповіщення лишаються, доки їх не закрити";
        AppDecides => "As the app asks", "Як просить програма";
        Seconds => "s", "с";
        Apps => "Apps without banners", "Програми без банерів";
        AppsHint => "Their notifications go straight to the notification centre. Apps are matched by desktop ID or by the name on their notifications.", "Їхні сповіщення потрапляють одразу до центру сповіщень. Програми впізнаються за ідентифікатором або за назвою на сповіщеннях.";
        NoMuted => "Every app can show banners", "Усі програми можуть показувати банери";
        AddApp => "Choose an app…", "Оберіть програму…";
        AddName => "Or type a name", "Або введіть назву";
        Add => "Add", "Додати";
        Remove => "Allow banners", "Дозволити банери";
        PreviewApp => "Messages", "Повідомлення";
        PreviewTitle => "Olena", "Олена";
        PreviewBody => "See you at seven?", "Побачимося о сьомій?";
        Now => "now", "щойно";
    }
}

use tr::Key as T;

const SECTION: &str = "notifications";
const DURATIONS: [i64; 7] = [0, 3, 5, 8, 12, 20, 30];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
}

impl Position {
    pub const ALL: [Self; 3] = [Self::TopLeft, Self::TopCenter, Self::TopRight];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopCenter => "top-center",
            Self::TopRight => "top-right",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Self {
        Self::ALL.into_iter().find(|position| position.key() == key).unwrap_or_default()
    }

    const fn label(self) -> T {
        match self {
            Self::TopLeft => T::TopLeft,
            Self::TopCenter => T::TopCenter,
            Self::TopRight => T::TopRight,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppChoice {
    pub id: String,
    pub name: String,
}

impl std::fmt::Display for AppChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    AppsLoaded(Vec<DesktopApp>),
    Dnd(bool),
    Schedule(bool),
    From(String),
    To(String),
    AllowCritical(bool),
    Position(Position),
    Duration(Choice),
    Mute(AppChoice),
    NameInput(String),
    MuteName,
    Unmute(String),
}

#[derive(Debug, Default)]
pub struct Page {
    apps: Vec<DesktopApp>,
    requested: bool,
    from: Option<String>,
    to: Option<String>,
    name: String,
}

#[must_use]
pub fn mute_key(id: &str) -> String {
    id.strip_suffix(".desktop").unwrap_or(id).to_string()
}

#[must_use]
pub fn add_muted(list: &[String], entry: &str) -> Option<Vec<String>> {
    let entry = entry.trim();
    if entry.is_empty() || list.iter().any(|item| item.eq_ignore_ascii_case(entry)) {
        return None;
    }
    let mut next = list.to_vec();
    next.push(entry.to_string());
    Some(next)
}

fn set_clock(config: &mut ConfigStore, key: &str, value: String) -> String {
    if parse_clock(&value).is_some() {
        config.set_str(SECTION, key, value.trim());
    }
    value
}

fn mute(config: &mut ConfigStore, entry: &str) -> bool {
    let next = add_muted(&config.get_str_array(SECTION, "muted-apps"), entry);
    if let Some(next) = &next {
        config.set_str_array(SECTION, "muted-apps", next);
    }
    next.is_some()
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["notification", "banner", "dnd", "disturb", "toast", "сповіщення", "банер", "турбувати"];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        if self.requested {
            return Task::none();
        }
        self.requested = true;
        let lang = std::env::var("LANG").unwrap_or_default();
        Task::perform(blocking(move || apps::visible(&apps::installed(&lang))), |apps| {
            Msg::AppsLoaded(apps.unwrap_or_default())
        })
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::AppsLoaded(apps) => self.apps = apps,
            Msg::Dnd(on) => config.set_bool(SECTION, "do-not-disturb", on),
            Msg::Schedule(on) => config.set_bool(SECTION, "schedule", on),
            Msg::From(value) => self.from = Some(set_clock(config, "from", value)),
            Msg::To(value) => self.to = Some(set_clock(config, "to", value)),
            Msg::AllowCritical(on) => config.set_bool(SECTION, "allow-critical", on),
            Msg::Position(position) => config.set_str(SECTION, "position", position.key()),
            Msg::Duration(duration) => config.set_int(SECTION, "banner-seconds", duration.value),
            Msg::Mute(choice) => {
                mute(config, &mute_key(&choice.id));
            }
            Msg::NameInput(name) => self.name = name,
            Msg::MuteName => {
                if mute(config, &self.name) {
                    self.name.clear();
                }
            }
            Msg::Unmute(entry) => {
                let list: Vec<String> =
                    config.get_str_array(SECTION, "muted-apps").into_iter().filter(|item| *item != entry).collect();
                config.set_str_array(SECTION, "muted-apps", &list);
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let dnd = config.bool_or(SECTION, "do-not-disturb", false);
        let position = Position::from_key(&config.str_or(SECTION, "position", "top-right"));
        let hero = widgets::section_hero(
            c.theme,
            if dnd { Icon::Moon } else { Icon::Bell },
            super::Panel::Notifications.color(),
            c.t(T::Dnd),
            c.t(if dnd { T::DndOn } else { T::DndOff }),
            Some(toggle(c, dnd, Msg::Dnd)),
        );
        column![hero, self.quiet(&c, config), banners(&c, config, position, dnd), self.muted(&c, config),]
            .spacing(theme::SECTION_GAP)
            .into()
    }
}

impl Page {
    fn quiet<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let schedule = config.bool_or(SECTION, "schedule", false);
        let mut group = Group::new(t).description(c.t(T::DndHint)).push(
            Row::new(t, c.t(T::Schedule))
                .subtitle(c.t(T::ScheduleHint))
                .prefix(widgets::icon(Icon::Clock, 18.0, t.fg_2()))
                .suffix(toggle(*c, schedule, Msg::Schedule))
                .build(),
        );
        if schedule {
            let from = self.from.clone().unwrap_or_else(|| config.str_or(SECTION, "from", "22:00"));
            let to = self.to.clone().unwrap_or_else(|| config.str_or(SECTION, "to", "07:00"));
            let from_bad = parse_clock(&from).is_none();
            let to_bad = parse_clock(&to).is_none();
            group = group.push(form_row(
                t,
                &format!("{} – {}", c.t(T::From), c.t(T::To)),
                (from_bad || to_bad).then(|| c.t(T::TimeInvalid)),
                gated(
                    *c,
                    row![
                        widgets::field(t, "22:00", &from, Msg::From, None, from_bad).width(Fill),
                        text("–").size(14).color(t.fg_3()),
                        widgets::field(t, "07:00", &to, Msg::To, None, to_bad).width(Fill),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ),
            ));
        }
        group
            .push(
                Row::new(t, c.t(T::AllowCritical))
                    .subtitle(c.t(T::AllowCriticalHint))
                    .prefix(widgets::icon(Icon::Warning, 18.0, t.fg_2()))
                    .suffix(toggle(*c, config.bool_or(SECTION, "allow-critical", true), Msg::AllowCritical))
                    .build(),
            )
            .build()
    }

    fn muted<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let muted = config.get_str_array(SECTION, "muted-apps");
        let mut group = Group::new(t).title(c.t(T::Apps)).description(c.t(T::AppsHint));
        if muted.is_empty() {
            group = group.push(widgets::list_empty(t, Icon::Bell, c.t(T::NoMuted), ""));
        }
        for entry in &muted {
            let resolved = self.apps.iter().find(|app| mute_key(&app.id).eq_ignore_ascii_case(entry));
            let (title, subtitle) = match resolved {
                Some(app) => (app.name.clone(), entry.clone()),
                None => (entry.clone(), String::new()),
            };
            group = group.push(
                Row::new(t, title)
                    .subtitle(subtitle)
                    .prefix(widgets::icon(Icon::Apps, 18.0, t.fg_2()))
                    .suffix(gated(*c, widgets::secondary(t, c.t(T::Remove), Some(Msg::Unmute(entry.clone())))))
                    .build(),
            );
        }
        let choices: Vec<AppChoice> = self
            .apps
            .iter()
            .filter(|app| !muted.iter().any(|entry| entry.eq_ignore_ascii_case(&mute_key(&app.id))))
            .map(|app| AppChoice { id: app.id.clone(), name: app.name.clone() })
            .collect();
        let ready = !self.name.trim().is_empty();
        group
            .push(
                container(gated(
                    *c,
                    row![
                        widgets::chooser(t, choices, c.t(T::AddApp), Msg::Mute, 260.0),
                        widgets::field(
                            t,
                            c.t(T::AddName),
                            &self.name,
                            Msg::NameInput,
                            ready.then_some(Msg::MuteName),
                            false,
                        )
                        .width(Fill),
                        widgets::secondary(t, c.t(T::Add), ready.then_some(Msg::MuteName)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ))
                .padding([6.0, theme::ROW_PADDING_X]),
            )
            .build()
    }
}

fn banners<'a>(c: &Ctx, config: &'a ConfigStore, position: Position, dnd: bool) -> Element<'a, Msg> {
    let t = c.theme;
    let cards = row(Position::ALL.map(|option| {
        container(widgets::radio_card(
            t,
            screen(t, option, dnd),
            c.t(option.label()),
            None,
            option == position,
            Msg::Position(option),
        ))
        .width(Fill)
        .into()
    }))
    .spacing(12);
    let seconds = config.int_or(SECTION, "banner-seconds", 0);
    let (options, selected) = presets(&DURATIONS, seconds, |seconds| {
        if seconds == 0 { c.t(T::AppDecides).to_string() } else { format!("{seconds} {}", c.t(T::Seconds)) }
    });
    Group::new(t)
        .title(c.t(T::Banners))
        .description(c.t(T::BannersHint))
        .push(container(gated(*c, cards)).padding([8.0, theme::ROW_PADDING_X - 4.0]))
        .push(
            Row::new(t, c.t(T::Duration))
                .subtitle(c.t(T::DurationHint))
                .suffix(gated(*c, widgets::dropdown(t, options, selected, Msg::Duration, 200.0)))
                .build(),
        )
        .push(container(preview(c)).padding([8.0, theme::ROW_PADDING_X]))
        .build()
}

#[must_use]
pub fn screen<'a>(t: ThemeState, position: Position, dnd: bool) -> Element<'a, Msg> {
    let desk = t.fill_2();
    let bar = t.fill_3();
    let banner = if dnd { t.fill_3() } else { t.acc_bg() };
    let toast = container(space())
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(14.0))
        .style(move |_: &iced::Theme| theme::solid(banner, theme::radius_r1()));
    let placed = match position {
        Position::TopLeft => container(toast).align_left(Fill),
        Position::TopCenter => container(toast).center_x(Fill),
        Position::TopRight => container(toast).align_right(Fill),
    };
    container(
        column![
            container(space())
                .width(Fill)
                .height(Length::Fixed(6.0))
                .style(move |_: &iced::Theme| theme::solid(bar, theme::radius_r1())),
            placed.padding([0, 6]),
        ]
        .spacing(5),
    )
    .width(Length::Fixed(132.0))
    .height(Length::Fixed(72.0))
    .padding(4)
    .style(move |_: &iced::Theme| theme::solid(desk, theme::radius_r2()))
    .into()
}

fn preview<'a>(c: &Ctx) -> Element<'a, Msg> {
    let t = c.theme;
    container(
        row![
            widgets::tile(Icon::Bell, super::Panel::Notifications.color(), 32.0),
            column![
                text(format!("{} · {}", c.t(T::PreviewApp), c.t(T::Now))).size(11.5).color(t.fg_3()),
                text(c.t(T::PreviewTitle)).size(13.5).font(theme::SEMIBOLD).color(t.fg_1()),
                text(c.t(T::PreviewBody)).size(13).color(t.fg_2()),
            ]
            .spacing(1)
            .width(Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(12)
    .max_width(360)
    .style(move |_: &iced::Theme| theme::card_style(&t))
    .into()
}
