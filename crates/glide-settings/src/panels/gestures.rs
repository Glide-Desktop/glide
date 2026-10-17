use std::ops::RangeInclusive;

use glide_ipc::{GestureAction, GestureKind, GesturePhase, GestureSample};
use iced::widget::{Stack, column, container, row, space, text};
use iced::{Alignment, Element, Fill, Length, Padding, Subscription, Task};

use super::common::{choice, gated, switch_row, toggle};
use super::{Ctx, Pane, Panel};
use crate::i18n::Lang;
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::ipc::{self, PracticeEvent};
use crate::services::pointer::{self, PointerKind, PointerState};
use crate::services::run::{Debounce, blocking};
use crate::theme::{self, ThemeState, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Heading => "Touchpad gestures", "Жести тачпада";
        SummaryThree => "Swipe with three fingers to switch workspaces and open the overview", "Проведіть трьома пальцями, щоб перемикати простори й відкривати огляд";
        SummaryFour => "Swipe with four fingers to switch workspaces and open the overview", "Проведіть чотирма пальцями, щоб перемикати простори й відкривати огляд";
        SummaryOff => "Every swipe goes straight to the app under the pointer", "Усі свайпи отримує програма під вказівником";
        NoTouchpad => "No touchpad is connected. These settings apply as soon as one is.", "Тачпад не під'єднано. Ці налаштування запрацюють, щойно його під'єднають.";
        ShellGestures => "Desktop gestures", "Жести робочого столу";
        ShellGesturesHint => "Glide handles these swipes itself", "Ці свайпи Glide обробляє сам";
        Fingers => "Fingers", "Пальці";
        FingersHint => "Swipes with a different number of fingers go to apps", "Свайпи іншою кількістю пальців отримують програми";
        Three => "Three", "Три";
        Four => "Four", "Чотири";
        SwitchWorkspaces => "Switch workspaces", "Перемикання просторів";
        SidewaysThree => "Swipe left or right with three fingers", "Проведіть трьома пальцями вліво чи вправо";
        SidewaysFour => "Swipe left or right with four fingers", "Проведіть чотирма пальцями вліво чи вправо";
        Overview => "Open the overview", "Відкриття огляду";
        UpThree => "Swipe up with three fingers for the overview, keep going for all apps; down to go back", "Проведіть трьома пальцями вгору — огляд, далі — всі програми; вниз — назад";
        UpFour => "Swipe up with four fingers for the overview, keep going for all apps; down to go back", "Проведіть чотирма пальцями вгору — огляд, далі — всі програми; вниз — назад";
        WindowGestures => "Windows and apps", "Вікна й програми";
        WindowGesturesHint => "Glide handles these gestures itself too", "Ці жести Glide теж обробляє сам";
        SwitchApps => "Switch apps", "Перемикання програм";
        SwitchAppsThree => "Swipe sideways with three fingers and let go on the app you want", "Проведіть трьома пальцями вбік і відпустіть на потрібній програмі";
        SwitchAppsFour => "Swipe sideways with four fingers and let go on the app you want", "Проведіть чотирма пальцями вбік і відпустіть на потрібній програмі";
        WindowSize => "Maximize and minimize", "Розгортання і згортання";
        WindowSizeThree => "Swipe up with three fingers to maximize, down to restore or minimize", "Проведіть трьома пальцями вгору, щоб розгорнути, вниз — щоб відновити чи згорнути";
        WindowSizeFour => "Swipe up with four fingers to maximize, down to restore or minimize", "Проведіть чотирма пальцями вгору, щоб розгорнути, вниз — щоб відновити чи згорнути";
        PinchShell => "Pinch with three or more fingers", "Щипок трьома й більше пальцями";
        PinchShellHint => "Pinch in for all apps, spread out to show the desktop", "Зведіть пальці — всі програми, розведіть — робочий стіл";
        TouchEdges => "Touchscreen edges", "Краї сенсорного екрана";
        TouchEdgesHint => "From the bottom: overview; from the sides: workspaces; from the top: quick settings", "Від нижнього краю — огляд, від бічних — простори, від верхнього — швидкі налаштування";
        ActionLauncher => "Open all apps", "Відкрити всі програми";
        ActionSwitchApp => "Switch apps", "Перемкнути програму";
        ActionMaximize => "Maximize the window", "Розгорнути вікно";
        ActionMinimize => "Minimize the window", "Згорнути вікно";
        ActionShowDesktop => "Show the desktop", "Показати робочий стіл";
        AppGestures => "In apps", "У програмах";
        AppGesturesHint => "Glide passes these gestures on to the app under the pointer", "Ці жести Glide передає програмі під вказівником";
        Scroll => "Scroll", "Прокручування";
        ScrollHint => "Move two fingers in any direction", "Рухайте двома пальцями в будь-якому напрямку";
        Zoom => "Zoom", "Масштабування";
        ZoomHint => "Pinch two fingers together or spread them apart", "Зведіть або розведіть два пальці";
        Navigate => "Back and forward", "Назад і вперед";
        NavigateThree => "Swipe sideways with three fingers in browsers and viewers that support it", "Проведіть трьома пальцями вбік у браузерах і переглядачах, які це вміють";
        NavigateFour => "Swipe sideways with four fingers in browsers and viewers that support it", "Проведіть чотирма пальцями вбік у браузерах і переглядачах, які це вміють";
        InApps => "App", "Програма";
        Feel => "Feel", "Відчуття";
        FeelHint => "How far and which way your fingers travel", "Наскільки далеко і в який бік рухаються пальці";
        Natural => "Natural direction", "Природний напрямок";
        NaturalHint => "The desktop follows your fingers: swipe left for the next workspace", "Стіл рухається за пальцями: проведіть уліво, щоб перейти до наступного простору";
        Sensitivity => "Sensitivity", "Чутливість";
        SensitivityHint => "Higher values need a shorter swipe", "Що більше значення, то коротший потрібен свайп";
        Practice => "Try it", "Випробувати";
        PracticeHint => "See how Glide reads your swipes. While testing, gestures don't switch anything.", "Подивіться, як Glide розпізнає ваші свайпи. Під час перевірки жести нічого не перемикають.";
        Start => "Start test", "Почати перевірку";
        Stop => "Finish", "Завершити";
        Reset => "Start over", "Почати спочатку";
        Offline => "Glide isn't running, so gestures can't be tested", "Glide не запущено, тому жести не перевірити";
        Failed => "Couldn't connect to Glide. Update and restart the session, then try again.", "Не вдалося під'єднатися до Glide. Оновіть і перезапустіть сеанс, а тоді спробуйте ще раз.";
        Connecting => "Connecting…", "Під'єднання…";
        PlaceFingers => "Put your fingers on the touchpad and swipe", "Покладіть пальці на тачпад і проведіть";
        Idle => "Your swipes will appear here", "Тут з'являться ваші свайпи";
        Waiting => "Waiting for a gesture", "Очікую на жест";
        Pinch => "Pinch", "Щипок";
        Hold => "Hold", "Утримання";
        Left => "left", "вліво";
        Right => "right", "вправо";
        Up => "up", "вгору";
        Down => "down", "вниз";
        Still => "not moving yet", "ще не рухається";
        ActionNone => "Not recognised yet", "Ще не розпізнано";
        ActionNext => "Next workspace", "Наступний простір";
        ActionPrevious => "Previous workspace", "Попередній простір";
        ActionOverview => "Open the overview", "Відкрити огляд";
        ActionApp => "Passed on to the app", "Передається програмі";
        ActionDisabled => "This gesture is turned off", "Цей жест вимкнено";
        Tracking => "Tracking", "Стежу";
        Worked => "Would work", "Спрацювало б";
        TooShort => "Swipe further or faster", "Проведіть далі або швидше";
        ForApps => "For apps", "Для програм";
        Unassigned => "Not assigned", "Не призначено";
        TurnedOff => "Off", "Вимкнено";
        Goals => "Try every gesture", "Спробуйте кожен жест";
    }
}

const TABLE: &str = "gestures";
pub const SENSITIVITY: RangeInclusive<f64> = 0.5..=2.0;
pub const PAD_WIDTH: f32 = 360.0;
pub const PAD_HEIGHT: f32 = 188.0;
const DOT: f32 = 18.0;
const DOT_GAP: f32 = 8.0;
const DEAD_ZONE: f64 = 12.0;
const TRAVEL: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Next,
    Previous,
    Overview,
    Launcher,
    Apps,
    Pinch,
    Hold,
}

impl Goal {
    pub const ALL: [Self; 7] =
        [Self::Next, Self::Previous, Self::Overview, Self::Launcher, Self::Apps, Self::Pinch, Self::Hold];

    #[must_use]
    pub fn reached(sample: &GestureSample) -> Option<Self> {
        if sample.phase != GesturePhase::End || !sample.committed {
            return None;
        }
        match (sample.kind, sample.action) {
            (GestureKind::Swipe, GestureAction::NextWorkspace) => Some(Self::Next),
            (GestureKind::Swipe, GestureAction::PreviousWorkspace) => Some(Self::Previous),
            (GestureKind::Swipe, GestureAction::OpenOverview) => Some(Self::Overview),
            (GestureKind::Swipe, GestureAction::OpenLauncher) => Some(Self::Launcher),
            (GestureKind::Swipe, GestureAction::SwitchApp) => Some(Self::Apps),
            (GestureKind::Pinch, _) if sample.progress >= 0.1 => Some(Self::Pinch),
            (GestureKind::Hold, _) => Some(Self::Hold),
            _ => None,
        }
    }

    const fn label(self) -> tr::Key {
        match self {
            Self::Next => tr::Key::ActionNext,
            Self::Previous => tr::Key::ActionPrevious,
            Self::Overview => tr::Key::ActionOverview,
            Self::Launcher => tr::Key::ActionLauncher,
            Self::Apps => tr::Key::ActionSwitchApp,
            Self::Pinch => tr::Key::Pinch,
            Self::Hold => tr::Key::Hold,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
    Still,
}

impl Direction {
    const fn label(self) -> tr::Key {
        match self {
            Self::Left => tr::Key::Left,
            Self::Right => tr::Key::Right,
            Self::Up => tr::Key::Up,
            Self::Down => tr::Key::Down,
            Self::Still => tr::Key::Still,
        }
    }

    const fn icon(self) -> Option<Icon> {
        match self {
            Self::Left => Some(Icon::ArrowLeft),
            Self::Right => Some(Icon::ArrowRight),
            Self::Up => Some(Icon::ArrowUp),
            Self::Down => Some(Icon::ArrowDown),
            Self::Still => None,
        }
    }
}

fn physical(sample: &GestureSample, natural: bool) -> (f64, f64) {
    if natural { (sample.dx, sample.dy) } else { (-sample.dx, -sample.dy) }
}

#[must_use]
pub fn direction(sample: &GestureSample, natural: bool) -> Direction {
    let (x, y) = physical(sample, natural);
    if x.abs().max(y.abs()) < DEAD_ZONE {
        Direction::Still
    } else if x.abs() > y.abs() {
        if x < 0.0 { Direction::Left } else { Direction::Right }
    } else if y < 0.0 {
        Direction::Up
    } else {
        Direction::Down
    }
}

#[must_use]
pub fn finger_offset(sample: &GestureSample, natural: bool) -> (f32, f32) {
    let (x, y) = physical(sample, natural);
    let reach_x = f64::from(PAD_WIDTH / 2.0 - 48.0);
    let reach_y = f64::from(PAD_HEIGHT / 2.0 - 28.0);
    ((x * TRAVEL).clamp(-reach_x, reach_x) as f32, (y * TRAVEL).clamp(-reach_y, reach_y) as f32)
}

#[must_use]
pub fn fingers_text(count: u32, lang: Lang) -> String {
    match lang {
        Lang::Uk => match (count % 10, count % 100) {
            (1, n) if n != 11 => format!("{count} палець"),
            (2..=4, n) if !(12..=14).contains(&n) => format!("{count} пальці"),
            _ => format!("{count} пальців"),
        },
        Lang::En if count == 1 => String::from("1 finger"),
        Lang::En => format!("{count} fingers"),
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Enabled(bool),
    Fingers(u32),
    WorkspaceSwipe(bool),
    OverviewSwipe(bool),
    AppSwipe(bool),
    PinchShell(bool),
    TouchEdges(bool),
    Natural(bool),
    Sensitivity(f64),
    CommitSensitivity,
    SettleSensitivity(u64),
    StartPractice,
    StopPractice,
    ResetPractice,
    Practice(PracticeEvent),
    Devices(PointerState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Session {
    #[default]
    Idle,
    Connecting,
    Connected,
    Failed,
}

#[derive(Debug, Default)]
pub struct Page {
    sensitivity: Option<f64>,
    settle: Debounce,
    session: Session,
    live: Option<GestureSample>,
    last: Option<GestureSample>,
    done: Vec<Goal>,
    devices: PointerState,
    loading: bool,
}

#[must_use]
pub fn fingers(config: &ConfigStore) -> u32 {
    config.int_or(TABLE, "fingers", 3).clamp(3, 4) as u32
}

#[must_use]
pub fn sensitivity(config: &ConfigStore) -> f64 {
    config
        .get_float(TABLE, "sensitivity")
        .filter(|value| value.is_finite())
        .unwrap_or(1.0)
        .clamp(*SENSITIVITY.start(), *SENSITIVITY.end())
}

fn natural(config: &ConfigStore) -> bool {
    config.bool_or(TABLE, "natural", true)
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "gesture",
        "swipe",
        "pinch",
        "three finger",
        "four finger",
        "touchpad",
        "trackpad",
        "жест",
        "свайп",
        "щипок",
        "пальц",
        "тачпад",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.session = Session::Idle;
        self.live = None;
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(pointer::read), |state| Msg::Devices(state.unwrap_or_default()))
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Enabled(on) => config.set_bool(TABLE, "enabled", on),
            Msg::Fingers(count) => config.set_int(TABLE, "fingers", i64::from(count)),
            Msg::WorkspaceSwipe(on) => config.set_bool(TABLE, "workspace-swipe", on),
            Msg::OverviewSwipe(on) => config.set_bool(TABLE, "overview-swipe", on),
            Msg::AppSwipe(on) => config.set_bool(TABLE, "app-swipe", on),
            Msg::PinchShell(on) => config.set_bool(TABLE, "pinch", on),
            Msg::TouchEdges(on) => config.set_bool(TABLE, "touch-edges", on),
            Msg::Natural(on) => config.set_bool(TABLE, "natural", on),
            Msg::Sensitivity(value) => {
                self.sensitivity = Some(value);
                return self.settle.schedule(Msg::SettleSensitivity);
            }
            Msg::SettleSensitivity(generation) if !self.settle.is_current(generation) => {}
            Msg::CommitSensitivity | Msg::SettleSensitivity(_) => {
                self.settle.cancel();
                if let Some(value) = self.sensitivity.take() {
                    config.set_float(TABLE, "sensitivity", (value * 20.0).round() / 20.0);
                }
            }
            Msg::StartPractice => self.session = Session::Connecting,
            Msg::StopPractice => {
                self.session = Session::Idle;
                self.live = None;
            }
            Msg::ResetPractice => {
                self.done.clear();
                self.last = None;
                self.live = None;
            }
            Msg::Practice(event) => self.practice(&event),
            Msg::Devices(state) => {
                self.loading = false;
                self.devices = state;
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![
            self.hero(&c, config),
            shell_group(&c, config),
            window_group(&c, config),
            app_group(&c, config),
            self.feel_group(&c, config),
            self.practice_group(&c, config),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }

    fn subscription(&self) -> Subscription<Msg> {
        if self.practising() { Subscription::run(ipc::practice).map(Msg::Practice) } else { Subscription::none() }
    }
}

impl Page {
    #[must_use]
    pub const fn practising(&self) -> bool {
        matches!(self.session, Session::Connecting | Session::Connected)
    }

    #[must_use]
    pub fn done(&self) -> &[Goal] {
        &self.done
    }

    #[must_use]
    pub const fn last(&self) -> Option<&GestureSample> {
        self.last.as_ref()
    }

    #[must_use]
    pub fn failed(&self) -> bool {
        self.session == Session::Failed
    }

    fn practice(&mut self, event: &PracticeEvent) {
        match event {
            PracticeEvent::Started if self.practising() => self.session = Session::Connected,
            PracticeEvent::Started => {}
            PracticeEvent::Sample(sample) if sample.phase == GesturePhase::End => {
                self.live = None;
                if let Some(goal) = Goal::reached(sample)
                    && !self.done.contains(&goal)
                {
                    self.done.push(goal);
                }
                self.last = Some(*sample);
            }
            PracticeEvent::Sample(sample) => self.live = Some(*sample),
            PracticeEvent::Stopped => {
                self.session = if self.session == Session::Connecting { Session::Failed } else { Session::Idle };
                self.live = None;
            }
        }
    }

    fn hero<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let enabled = config.bool_or(TABLE, "enabled", true);
        let summary = if self.devices.read && !self.devices.has(PointerKind::Touchpad) {
            tr::Key::NoTouchpad
        } else if !enabled {
            tr::Key::SummaryOff
        } else if fingers(config) == 4 {
            tr::Key::SummaryFour
        } else {
            tr::Key::SummaryThree
        };
        widgets::section_hero(
            t,
            Icon::Gesture,
            Panel::Gestures.color(),
            c.t(tr::Key::Heading),
            c.t(summary),
            Some(toggle(*c, enabled, Msg::Enabled)),
        )
    }

    fn feel_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let value = self.sensitivity.unwrap_or_else(|| sensitivity(config));
        let control = row![
            widgets::value_slider(t, SENSITIVITY, value, 0.05, Msg::Sensitivity, Some(Msg::CommitSensitivity),),
            text(format!("{value:.2}×")).size(13).color(t.fg_2()).width(Length::Fixed(48.0)).align_x(Alignment::End),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        Group::new(t)
            .title(c.t(tr::Key::Feel))
            .description(c.t(tr::Key::FeelHint))
            .push(switch_row(*c, c.t(tr::Key::Natural), c.t(tr::Key::NaturalHint), natural(config), Msg::Natural))
            .push(
                Row::new(t, c.t(tr::Key::Sensitivity))
                    .subtitle(c.t(tr::Key::SensitivityHint))
                    .suffix(gated(*c, widgets::flexible(control, 300.0)))
                    .build(),
            )
            .build()
    }

    fn practice_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let natural = natural(config);
        let mut group = Group::new(t).title(c.t(tr::Key::Practice)).description(c.t(tr::Key::PracticeHint));
        if self.failed() {
            group = group
                .push(container(widgets::banner(t, t.warn(), Icon::Warning, c.t(tr::Key::Failed), None)).padding(8));
        }
        group = group
            .push(container(pad(c, self.live.as_ref(), self.practising(), natural)).center_x(Fill).padding([12, 8]));
        if self.practising() {
            group = group.push(self.reading(c, natural)).push(goals(c, &self.done));
        }
        group.push(self.controls(c)).build()
    }

    fn reading<'a>(&'a self, c: &Ctx, natural: bool) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(sample) = self.live.as_ref().or(self.last.as_ref()) else {
            let status = if self.session == Session::Connected { tr::Key::Waiting } else { tr::Key::Connecting };
            return Row::new(t, c.t(status)).dimmed(true).build();
        };
        let heading = match sample.kind {
            GestureKind::Swipe => {
                format!("{} · {}", fingers_text(sample.fingers, c.lang), c.t(direction(sample, natural).label()))
            }
            GestureKind::Pinch => format!("{} · ×{:.2}", c.t(tr::Key::Pinch), sample.scale),
            GestureKind::Hold => format!("{} · {}", c.t(tr::Key::Hold), fingers_text(sample.fingers, c.lang)),
        };
        let live = self.live.is_some();
        let bar = container(widgets::progress_bar(t, sample.progress as f32)).width(Length::Fixed(120.0));
        let suffix = row![bar, verdict(&t, c, sample, live)].spacing(12).align_y(Alignment::Center);
        Row::new(t, heading).subtitle(c.t(action_label(sample.action))).suffix(suffix).build()
    }

    fn controls<'a>(&'a self, c: &Ctx) -> Element<'a, Msg> {
        let t = c.theme;
        let mut buttons = row![].spacing(8).align_y(Alignment::Center);
        let hint: Element<'a, Msg> = if self.practising() {
            buttons = buttons.push(widgets::secondary(t, c.t(tr::Key::Stop), Some(Msg::StopPractice)));
            if !self.done.is_empty() || self.last.is_some() {
                buttons = buttons.push(widgets::tonal(t, c.t(tr::Key::Reset), Some(Msg::ResetPractice)));
            }
            text(c.t(tr::Key::PracticeHint)).size(12).color(t.fg_3()).width(Fill).into()
        } else if c.online {
            buttons = buttons.push(widgets::primary(t, c.t(tr::Key::Start), Some(Msg::StartPractice)));
            space().width(Fill).into()
        } else {
            buttons = buttons.push(widgets::primary(t, c.t(tr::Key::Start), None));
            text(c.t(tr::Key::Offline)).size(12).color(t.fg_3()).width(Fill).into()
        };
        container(row![hint, buttons].spacing(12).align_y(Alignment::Center))
            .padding([10.0, theme::ROW_PADDING_X])
            .width(Fill)
            .into()
    }
}

fn shell_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let enabled = config.bool_or(TABLE, "enabled", true);
    let count = fingers(config);
    let four = count == 4;
    let options = [(3_u32, c.t(tr::Key::Three)), (4_u32, c.t(tr::Key::Four))];
    let (sideways, up) =
        if four { (tr::Key::SidewaysFour, tr::Key::UpFour) } else { (tr::Key::SidewaysThree, tr::Key::UpThree) };
    Group::new(t)
        .title(c.t(tr::Key::ShellGestures))
        .description(c.t(tr::Key::ShellGesturesHint))
        .push(
            Row::new(t, c.t(tr::Key::Fingers))
                .subtitle(c.t(tr::Key::FingersHint))
                .suffix(choice(*c, options, count, Msg::Fingers))
                .dimmed(!enabled)
                .build(),
        )
        .push(
            Row::new(t, c.t(tr::Key::SwitchWorkspaces))
                .subtitle(c.t(sideways))
                .prefix(art(&t, Art::Sideways(count)))
                .suffix(toggle(*c, config.bool_or(TABLE, "workspace-swipe", true), Msg::WorkspaceSwipe))
                .dimmed(!enabled)
                .build(),
        )
        .push(
            Row::new(t, c.t(tr::Key::Overview))
                .subtitle(c.t(up))
                .prefix(art(&t, Art::Up(count)))
                .suffix(toggle(*c, config.bool_or(TABLE, "overview-swipe", true), Msg::OverviewSwipe))
                .dimmed(!enabled)
                .build(),
        )
        .build()
}

fn window_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let enabled = config.bool_or(TABLE, "enabled", true);
    let other = if fingers(config) == 4 { 3 } else { 4 };
    let apps = config.bool_or(TABLE, "app-swipe", true);
    let (sideways, up) = if other == 4 {
        (tr::Key::SwitchAppsFour, tr::Key::WindowSizeFour)
    } else {
        (tr::Key::SwitchAppsThree, tr::Key::WindowSizeThree)
    };
    let row = |glyph: Art, title: tr::Key, hint: tr::Key, key: &str, on: fn(bool) -> Msg| {
        Row::new(t, c.t(title))
            .subtitle(c.t(hint))
            .prefix(art(&t, glyph))
            .suffix(toggle(*c, config.bool_or(TABLE, key, true), on))
            .dimmed(!enabled)
            .build()
    };
    Group::new(t)
        .title(c.t(tr::Key::WindowGestures))
        .description(c.t(tr::Key::WindowGesturesHint))
        .push(row(Art::Sideways(other), tr::Key::SwitchApps, sideways, "app-swipe", Msg::AppSwipe))
        .push(
            Row::new(t, c.t(tr::Key::WindowSize))
                .subtitle(c.t(up))
                .prefix(art(&t, Art::Up(other)))
                .dimmed(!enabled || !apps)
                .build(),
        )
        .push(row(Art::Pinch, tr::Key::PinchShell, tr::Key::PinchShellHint, "pinch", Msg::PinchShell))
        .push(row(Art::Up(1), tr::Key::TouchEdges, tr::Key::TouchEdgesHint, "touch-edges", Msg::TouchEdges))
        .build()
}

fn app_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let other = if fingers(config) == 4 { 3 } else { 4 };
    let navigate_row = !config.bool_or(TABLE, "app-swipe", true);
    let navigate = if other == 4 { tr::Key::NavigateFour } else { tr::Key::NavigateThree };
    let entry = |glyph: Art, title: tr::Key, hint: tr::Key| {
        Row::new(t, c.t(title))
            .subtitle(c.t(hint))
            .prefix(art(&t, glyph))
            .suffix(widgets::pill(t, Tone::Neutral, c.t(tr::Key::InApps)))
            .build()
    };
    let mut group = Group::new(t)
        .title(c.t(tr::Key::AppGestures))
        .description(c.t(tr::Key::AppGesturesHint))
        .push(entry(Art::Scroll, tr::Key::Scroll, tr::Key::ScrollHint))
        .push(entry(Art::Pinch, tr::Key::Zoom, tr::Key::ZoomHint));
    if navigate_row {
        group = group.push(entry(Art::Sideways(other), tr::Key::Navigate, navigate));
    }
    group.build()
}

const fn action_label(action: GestureAction) -> tr::Key {
    match action {
        GestureAction::None => tr::Key::ActionNone,
        GestureAction::NextWorkspace => tr::Key::ActionNext,
        GestureAction::PreviousWorkspace => tr::Key::ActionPrevious,
        GestureAction::OpenOverview => tr::Key::ActionOverview,
        GestureAction::OpenLauncher => tr::Key::ActionLauncher,
        GestureAction::SwitchApp => tr::Key::ActionSwitchApp,
        GestureAction::MaximizeWindow => tr::Key::ActionMaximize,
        GestureAction::MinimizeWindow => tr::Key::ActionMinimize,
        GestureAction::ShowDesktop => tr::Key::ActionShowDesktop,
        GestureAction::App => tr::Key::ActionApp,
        GestureAction::Disabled => tr::Key::ActionDisabled,
    }
}

#[must_use]
pub fn verdict_tone(sample: &GestureSample, live: bool) -> Tone {
    if live {
        return Tone::Accent;
    }
    match sample.action {
        _ if sample.committed && sample.action != GestureAction::App => Tone::Ok,
        GestureAction::App | GestureAction::Disabled | GestureAction::None => Tone::Neutral,
        _ => Tone::Warn,
    }
}

fn verdict<'a>(t: &ThemeState, c: &Ctx, sample: &GestureSample, live: bool) -> Element<'a, Msg> {
    let tone = verdict_tone(sample, live);
    let label = match (tone, sample.action) {
        (Tone::Accent, _) => tr::Key::Tracking,
        (Tone::Ok, _) => tr::Key::Worked,
        (Tone::Warn, _) => tr::Key::TooShort,
        (_, GestureAction::App) => tr::Key::ForApps,
        (_, GestureAction::Disabled) => tr::Key::TurnedOff,
        _ => tr::Key::Unassigned,
    };
    widgets::status_pill(*t, tone, c.t(label))
}

fn goals<'a>(c: &Ctx, done: &[Goal]) -> Element<'a, Msg> {
    let t = c.theme;
    let mut pills = row![].spacing(6).align_y(Alignment::Center);
    for goal in Goal::ALL {
        let reached = done.contains(&goal);
        let label = if reached { format!("✓ {}", c.t(goal.label())) } else { c.t(goal.label()).to_string() };
        let tone = if reached { Tone::Ok } else { Tone::Neutral };
        pills = pills.push(widgets::pill(t, tone, label));
    }
    let header = row![
        text(c.t(tr::Key::Goals)).size(13).color(t.fg_2()).width(Fill),
        text(format!("{}/{}", done.len(), Goal::ALL.len()))
            .size(13)
            .font(theme::SEMIBOLD)
            .color(if done.len() == Goal::ALL.len() { t.ok() } else { t.fg_3() }),
    ];
    container(column![header, pills.wrap()].spacing(8)).padding([10.0, theme::ROW_PADDING_X]).width(Fill).into()
}

fn dots<'a>(color: iced::Color, count: u32, size: f32, gap: f32) -> Element<'a, Msg> {
    let mut line = row![].spacing(gap).align_y(Alignment::Center);
    for _ in 0..count.clamp(1, 5) {
        line = line.push(
            container(space())
                .width(size)
                .height(size)
                .style(move |_: &iced::Theme| theme::solid(color, theme::radius_round())),
        );
    }
    line.into()
}

fn dots_width(count: u32, size: f32, gap: f32) -> f32 {
    let count = count.clamp(1, 5) as f32;
    count * size + (count - 1.0) * gap
}

fn fingers_layers<'a>(t: &ThemeState, sample: &GestureSample, natural: bool) -> Vec<Element<'a, Msg>> {
    let mut layers = Vec::new();
    if let Some(glyph) = direction(sample, natural).icon()
        && sample.kind == GestureKind::Swipe
    {
        layers.push(container(widgets::icon(glyph, 64.0, t.acc_soft())).center_x(Fill).center_y(Fill).into());
    }
    let gap = match sample.kind {
        GestureKind::Pinch => (DOT_GAP * sample.scale as f32).clamp(0.0, 60.0),
        _ => DOT_GAP,
    };
    let (x, y) = if sample.kind == GestureKind::Swipe { finger_offset(sample, natural) } else { (0.0, 0.0) };
    let width = dots_width(sample.fingers, DOT, gap);
    let left = (PAD_WIDTH / 2.0 + x - width / 2.0).max(0.0);
    let top = (PAD_HEIGHT / 2.0 + y - DOT / 2.0).max(0.0);
    layers
        .push(container(dots(t.acc_bg(), sample.fingers, DOT, gap)).padding(Padding::ZERO.top(top).left(left)).into());
    layers
}

fn pad<'a>(c: &Ctx, live: Option<&GestureSample>, practising: bool, natural: bool) -> Element<'a, Msg> {
    let t = c.theme;
    let well = t.sunken_bg();
    let edge = t.line_2();
    let surface = container(space()).width(Fill).height(Fill).style(move |_: &iced::Theme| {
        let mut style = theme::solid(well, theme::radius_r3());
        style.border.color = edge;
        style.border.width = 1.0;
        style
    });
    let mut layers: Vec<Element<'a, Msg>> = vec![surface.into()];
    if let Some(sample) = live {
        layers.extend(fingers_layers(&t, sample, natural));
    } else {
        let (caption, color) = if practising { (tr::Key::PlaceFingers, t.fg_2()) } else { (tr::Key::Idle, t.fg_3()) };
        let ghost = iced::Color { a: 0.35, ..t.fg_3() };
        layers.push(
            container(
                column![dots(ghost, 3, DOT, DOT_GAP), text(c.t(caption)).size(13).color(color),]
                    .spacing(14)
                    .align_x(Alignment::Center),
            )
            .center_x(Fill)
            .center_y(Fill)
            .into(),
        );
    }
    container(Stack::with_children(layers).width(Fill).height(Fill))
        .width(Length::Fixed(PAD_WIDTH))
        .height(Length::Fixed(PAD_HEIGHT))
        .clip(true)
        .into()
}

#[derive(Debug, Clone, Copy)]
enum Art {
    Sideways(u32),
    Up(u32),
    Scroll,
    Pinch,
}

fn art<'a>(t: &ThemeState, art: Art) -> Element<'a, Msg> {
    let accent = t.acc_bg();
    let arrow = t.acc_text();
    let content: Element<'a, Msg> = match art {
        Art::Sideways(count) => row![
            widgets::icon(Icon::ArrowLeft, 11.0, arrow),
            dots(accent, count, 5.0, 2.0),
            widgets::icon(Icon::ArrowRight, 11.0, arrow),
        ]
        .spacing(3)
        .align_y(Alignment::Center)
        .into(),
        Art::Up(count) => column![widgets::icon(Icon::ArrowUp, 11.0, arrow), dots(accent, count, 5.0, 2.0),]
            .spacing(2)
            .align_x(Alignment::Center)
            .into(),
        Art::Scroll => column![
            widgets::icon(Icon::ArrowUp, 9.0, arrow),
            dots(accent, 2, 5.0, 2.0),
            widgets::icon(Icon::ArrowDown, 9.0, arrow),
        ]
        .spacing(1)
        .align_x(Alignment::Center)
        .into(),
        Art::Pinch => widgets::icon(Icon::Pinch, 18.0, arrow),
    };
    let well = t.fill_1();
    let edge = t.line_2();
    container(content)
        .center_x(Length::Fixed(48.0))
        .center_y(Length::Fixed(34.0))
        .style(move |_: &iced::Theme| {
            let mut style = theme::solid(well, theme::radius_r2());
            style.border.color = edge;
            style.border.width = 1.0;
            style
        })
        .into()
}
