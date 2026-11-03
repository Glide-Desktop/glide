use glide_sdk::iced::Color;
use glide_sdk::iced::widget::{button, column, container, mouse_area, row, scrollable, stack, svg};
use glide_sdk::prelude::*;
use glide_sdk::theme::{Accent as ThemeAccent, Appearance};

use crate::{Accent, DemoApp, Island, Network, Phone, Snap, Step, Theme, Welcome, Wifi};

const PHOTOS: [&str; 4] = ["Карпати на світанку", "Морський захід сонця", "Осінній ліс", "Затишна кав'ярня"];
const CONTENT_SCROLL: &str = "glide-welcome-content";

fn photo_title(selected: Option<usize>) -> &'static str {
    selected.and_then(|index| PHOTOS.get(index)).copied().unwrap_or("Оберіть світлину ліворуч")
}

fn skip_dialog() -> Dialog<Message> {
    Dialog::new(
        "Пропустити знайомство?",
        "Ви завжди зможете змінити налаштування пізніше та знову відкрити знайомство з лаунчера.",
    )
    .cancel("Залишитися в турі")
    .choice("Так, перейти до стільниці", Tone::Accent, Message::ConfirmSkip)
}

fn night_color(warmth: f32) -> Color {
    let warmth = warmth.clamp(0.0, 1.0);
    Color::from_rgb(0.102 + 0.12 * warmth, 0.102 + 0.045 * warmth, 0.125 - 0.035 * warmth)
}

fn finish_animation(content: Element<'_, Message>, leaving: bool) -> Element<'_, Message> {
    anim::animate(f32::from(leaving), anim::Speed::EXPAND, |_| content)
        .scale(|progress| 1.0 + 0.08 * progress)
        .offset(|progress| glide_sdk::iced::Vector::new(0.0, -14.0 * progress))
        .into()
}

fn step_transform(progress: f32, forward: bool) -> anim::Transform {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    anim::Transform {
        scale: 1.0 - 0.02 * remaining,
        offset: glide_sdk::iced::Vector::new(if forward { 48.0 } else { -48.0 } * remaining, 0.0),
    }
}

fn swipe_event(event: &glide_sdk::iced::Event) -> Option<Message> {
    use glide_sdk::iced::{Event, mouse, window};
    match event {
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Message::SwipeEnd),
        Event::Window(window::Event::Unfocused) => Some(Message::SwipeCancel),
        _ => None,
    }
}

fn page_scroll(page: Element<'_, Message>, step: Step) -> Element<'_, Message> {
    scrollable(page)
        .id(CONTENT_SCROLL)
        .on_scroll(move |viewport| Message::Scrolled(step, viewport.absolute_offset().y))
        .height(Fill)
        .into()
}

#[derive(Debug, Clone)]
pub enum Message {
    GoTo(Step),
    Scrolled(Step, f32),
    Back,
    Next,
    SkipStep,
    AskSkip,
    ConfirmSkip,
    ToggleSound,
    ToggleLanguage,
    ToggleWifi,
    ConnectWifi,
    SelectNetwork(Network),
    WifiConnected(u64),
    WifiPassword(String),
    TogglePassword,
    Theme(Theme),
    Accent(Accent),
    PreviewAction,
    Island(Island),
    CycleIsland,
    DockApp(&'static str),
    Workspace(f32),
    SwipeStart,
    SwipeEnd,
    SwipeCancel,
    Search(String),
    PairPhone,
    PhonePaired,
    PhoneClipboard(bool),
    PhoneCalls(bool),
    Temperature(f32),
    NightAutomatic(bool),
    NightFocus(bool),
    Photo(usize),
    PhotoZoom,
    Favourite,
    File(usize),
    QuickLook,
    Snap(Snap),
    SnapCommit(u64),
    Install(DemoApp),
    Installed(DemoApp),
    Screenshot,
    ScreenshotDone,
    Finish,
    FinishSaved(Result<(), String>),
    FinishAnimationDone,
}

pub struct WelcomeApp {
    state: Welcome,
    swipe_start: Option<f32>,
    english: bool,
    forward: bool,
    search: String,
    flash: bool,
    wifi_generation: u64,
    night: NightDemo,
    quick_file: usize,
    dock: DockDemo,
    phone_options: PhoneOptions,
    completion: Completion,
    snap_demo: SnapDemo,
    scroll_positions: [f32; Step::ALL.len()],
}

#[derive(Default)]
struct SnapDemo {
    pending: Option<Snap>,
    generation: u64,
}

#[derive(Default, PartialEq, Eq)]
enum Completion {
    #[default]
    Idle,
    Saving,
    Leaving,
}

struct PhoneOptions {
    clipboard: bool,
    calls: bool,
}

impl Default for PhoneOptions {
    fn default() -> Self {
        Self { clipboard: true, calls: true }
    }
}

#[derive(Default)]
struct DockDemo {
    active: Option<&'static str>,
    serial: u64,
}

struct NightDemo {
    automatic: bool,
    focus: bool,
}

impl Default for NightDemo {
    fn default() -> Self {
        Self { automatic: true, focus: false }
    }
}

impl Default for WelcomeApp {
    fn default() -> Self {
        Self {
            state: Welcome::default(),
            swipe_start: None,
            english: false,
            forward: true,
            search: "48 * 12".into(),
            flash: false,
            wifi_generation: 0,
            night: NightDemo::default(),
            quick_file: 0,
            dock: DockDemo::default(),
            phone_options: PhoneOptions::default(),
            completion: Completion::Idle,
            snap_demo: SnapDemo::default(),
            scroll_positions: [0.0; Step::ALL.len()],
        }
    }
}

impl WelcomeApp {
    fn finish_swipe(&mut self, x: f32) {
        if let Some(start) = self.swipe_start.take() {
            self.state.swipe_workspace(x - start);
        }
    }

    fn select_photo(&mut self, index: usize, shell: &mut Shell<Message>) {
        if let Some(title) = PHOTOS.get(index) {
            self.state.photo = Some(index);
            shell.toast(format!("Світлина: {title}"));
        }
    }

    fn skip_step(&mut self, shell: &mut Shell<Message>) {
        if !matches!(self.state.step, Step::Welcome | Step::Finish) {
            shell.toast(format!("Крок {} пропущено", self.state.step.index()));
            self.state.next();
        }
    }

    fn toggle_sound(&mut self, shell: &mut Shell<Message>) {
        self.state.sound = !self.state.sound;
        shell.toast(if self.state.sound { "Звук увімкнено" } else { "Звук вимкнено" });
    }

    fn restore_scroll(&self, task: Task<Message>) -> Task<Message> {
        Task::batch([
            task,
            glide_sdk::iced::widget::operation::scroll_to(
                CONTENT_SCROLL,
                glide_sdk::iced::widget::operation::AbsoluteOffset {
                    x: 0.0,
                    y: self.scroll_positions[self.state.step.index()],
                },
            ),
        ])
    }

    fn step_changed(&mut self, before: Step, task: Task<Message>) -> Task<Message> {
        if self.state.step == before {
            return task;
        }
        self.forward = self.state.step.index() > before.index();
        self.swipe_start = None;
        self.restore_scroll(task)
    }

    fn select_network(&mut self, network: Network) {
        if self.state.network == network {
            return;
        }
        self.state.network = network;
        self.wifi_generation = self.wifi_generation.wrapping_add(1);
        if self.state.wifi != Wifi::Off {
            self.state.wifi = Wifi::Available;
        }
    }

    fn finish_tour(&mut self) -> Task<Message> {
        if self.completion != Completion::Idle {
            return Task::none();
        }
        self.completion = Completion::Saving;
        Task::perform(
            async {
                tokio::task::spawn_blocking(crate::mark_complete)
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())
            },
            Message::FinishSaved,
        )
    }

    fn finish_saved(&mut self, result: Result<(), String>, shell: &mut Shell<Message>) -> Task<Message> {
        if self.completion != Completion::Saving {
            return Task::none();
        }
        match result {
            Ok(()) => {
                self.completion = Completion::Leaving;
                Task::perform(async { tokio::time::sleep((anim::Speed::EXPAND.duration)()).await }, |()| {
                    Message::FinishAnimationDone
                })
            }
            Err(error) => {
                self.completion = Completion::Idle;
                shell.toast(format!("Не вдалося зберегти завершення: {error}"));
                Task::none()
            }
        }
    }

    fn accepts_message(&self, message: &Message) -> bool {
        match self.completion {
            Completion::Idle => true,
            Completion::Saving => matches!(message, Message::FinishSaved(_)),
            Completion::Leaving => matches!(message, Message::FinishAnimationDone),
        }
    }

    fn message_sound(&self, message: &Message) -> Option<crate::sound::Sound> {
        match message {
            Message::WifiPassword(_)
            | Message::Search(_)
            | Message::Scrolled(_, _)
            | Message::SwipeStart
            | Message::SwipeCancel
            | Message::FinishSaved(_)
            | Message::FinishAnimationDone
            | Message::SnapCommit(_) => None,
            Message::WifiConnected(generation)
                if self.state.wifi != Wifi::Connecting || *generation != self.wifi_generation =>
            {
                None
            }
            Message::Favourite
            | Message::WifiConnected(_)
            | Message::PhonePaired
            | Message::Installed(_)
            | Message::ScreenshotDone
            | Message::Finish => Some(crate::sound::Sound::Celebrate),
            Message::QuickLook | Message::Snap(_) => Some(crate::sound::Sound::Step),
            _ => Some(crate::sound::Sound::Click),
        }
    }

    fn launch_demo_app(&mut self, name: &'static str, shell: &mut Shell<Message>) {
        self.dock.active = Some(name);
        self.dock.serial = self.dock.serial.wrapping_add(1);
        shell.toast(format!("Запуск: {name}"));
    }

    fn cycle_island(&mut self) {
        self.state.island = match self.state.island {
            Island::Music => Island::Notification,
            Island::Notification => Island::Call,
            Island::Call => Island::Music,
        };
    }

    fn toggle_quick_look(&mut self) {
        if !self.state.quick_look_open {
            self.quick_file = self.state.file;
        }
        self.state.quick_look_open = !self.state.quick_look_open;
    }

    fn begin_snap(&mut self, target: Snap) -> Task<Message> {
        self.snap_demo.generation = self.snap_demo.generation.wrapping_add(1);
        if glide_sdk::theme::motion::t2().is_zero() {
            self.state.snap = target;
            self.snap_demo.pending = None;
            return Task::none();
        }
        self.snap_demo.pending = Some(target);
        let generation = self.snap_demo.generation;
        Task::perform(async { tokio::time::sleep(glide_sdk::theme::motion::t2()).await }, move |()| {
            Message::SnapCommit(generation)
        })
    }

    fn commit_snap(&mut self, generation: u64) {
        if generation == self.snap_demo.generation
            && let Some(target) = self.snap_demo.pending.take()
        {
            self.state.snap = target;
        }
    }

    fn demo_action(&mut self, message: &Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::ConnectWifi if self.state.wifi == Wifi::Available => {
                self.state.wifi = Wifi::Connecting;
                self.wifi_generation = self.wifi_generation.wrapping_add(1);
                delayed(600, Message::WifiConnected(self.wifi_generation))
            }
            Message::WifiConnected(generation)
                if self.state.wifi == Wifi::Connecting && self.wifi_generation == *generation =>
            {
                self.state.wifi = Wifi::Connected;
                shell.toast(format!("Демонстрацію підключення до {} завершено", self.state.network.name()));
                Task::none()
            }
            Message::PairPhone if self.state.phone == Phone::Unpaired => {
                self.state.phone = Phone::Pairing;
                delayed(700, Message::PhonePaired)
            }
            Message::PhonePaired if self.state.phone == Phone::Pairing => {
                self.state.phone = Phone::Paired;
                shell.toast("Смартфон зв'язано з комп'ютером!");
                Task::none()
            }
            Message::Install(app)
                if !self.state.installed_apps.contains(app) && !self.state.installing_apps.contains(app) =>
            {
                self.state.installing_apps.push(*app);
                delayed(800, Message::Installed(*app))
            }
            Message::Installed(app) if self.state.installing_apps.contains(app) => {
                self.state.installing_apps.retain(|pending| pending != app);
                self.state.install_demo_app(*app);
                shell.toast("Програму додано до демонстраційного дока 🎉");
                Task::none()
            }
            Message::Screenshot if !self.flash => {
                self.flash = true;
                delayed(120, Message::ScreenshotDone)
            }
            Message::ScreenshotDone => {
                self.flash = false;
                shell.toast("Демонстраційний знімок зроблено 📸");
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn preview(&self, shell: &mut Shell<Message>) {
        let appearance = match self.state.theme {
            Theme::Light => Appearance::Light,
            Theme::Dark | Theme::Auto => Appearance::Dark,
        };
        let accent = match self.state.accent {
            Accent::Blue => ThemeAccent::Blue,
            Accent::Teal => ThemeAccent::Teal,
            Accent::Green => ThemeAccent::Green,
            Accent::Amber => ThemeAccent::Amber,
            Accent::Orange => ThemeAccent::Orange,
            Accent::Red => ThemeAccent::Red,
            Accent::Purple => ThemeAccent::Purple,
            Accent::Graphite => ThemeAccent::Graphite,
        };
        shell.preview_look(Some((appearance, accent)));
    }
}

impl App for WelcomeApp {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let app = Self::default();
        app.preview(shell);
        (app, Task::none())
    }

    fn title(&self, _: &Shell<Message>) -> String {
        "Glide Welcome".into()
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        if !self.accepts_message(&message) {
            return Task::none();
        }
        let before = self.state.step;
        let mut task = Task::none();
        let sound = self.message_sound(&message);
        match message {
            Message::GoTo(step) => self.state.go_to(step),
            Message::Scrolled(step, offset) => self.scroll_positions[step.index()] = offset.max(0.0),
            Message::Back => self.state.back(),
            Message::Next => self.state.next(),
            Message::SkipStep => self.skip_step(shell),
            Message::AskSkip => shell.dialog(skip_dialog()),
            Message::ConfirmSkip => self.state.confirm_skip(),
            Message::ToggleSound => self.toggle_sound(shell),
            Message::ToggleLanguage => {
                self.english = !self.english;
                shell.toast(if self.english {
                    "Language set to: English"
                } else {
                    "Мову встановлено: Українська"
                });
            }
            Message::ToggleWifi => {
                self.wifi_generation = self.wifi_generation.wrapping_add(1);
                self.state.wifi = if self.state.wifi == Wifi::Off { Wifi::Available } else { Wifi::Off };
                shell.toast("Стан Wi-Fi змінено");
            }
            Message::SelectNetwork(network) => self.select_network(network),
            action @ (Message::ConnectWifi
            | Message::WifiConnected(_)
            | Message::PairPhone
            | Message::PhonePaired
            | Message::Install(_)
            | Message::Installed(_)
            | Message::Screenshot
            | Message::ScreenshotDone) => {
                task = self.demo_action(&action, shell);
            }
            Message::WifiPassword(value) => self.state.wifi_password = value,
            Message::TogglePassword => self.state.wifi_password_visible = !self.state.wifi_password_visible,
            Message::Theme(theme) => {
                self.state.theme = theme;
                self.preview(shell);
            }
            Message::Accent(accent) => {
                self.state.accent = accent;
                self.preview(shell);
            }
            Message::PreviewAction => {}
            Message::Island(island) => self.state.island = island,
            Message::CycleIsland => self.cycle_island(),
            Message::DockApp(name) => self.launch_demo_app(name, shell),
            Message::Workspace(distance) => self.state.swipe_workspace(distance),
            Message::SwipeStart => self.swipe_start = Some(shell.pointer().x),
            Message::SwipeEnd => self.finish_swipe(shell.pointer().x),
            Message::SwipeCancel => self.swipe_start = None,
            Message::Search(value) => self.search = value,
            Message::Temperature(kelvin) => self.state.set_temperature(kelvin.round() as u16),
            Message::PhoneClipboard(value) => self.phone_options.clipboard = value,
            Message::PhoneCalls(value) => self.phone_options.calls = value,
            Message::NightAutomatic(value) => self.night.automatic = value,
            Message::NightFocus(value) => self.night.focus = value,
            Message::Photo(index) => self.select_photo(index, shell),
            Message::PhotoZoom => {
                let title = photo_title(self.state.photo);
                shell.dialog(Dialog::new("Перегляд світлини", title).cancel("Закрити"));
            }
            Message::Favourite => shell.toast("Додано до альбому «Улюблені» ❤️"),
            Message::File(index) => self.state.file = index,
            Message::QuickLook => self.toggle_quick_look(),
            Message::Snap(snap) => task = self.begin_snap(snap),
            Message::SnapCommit(generation) => self.commit_snap(generation),
            Message::Finish => task = self.finish_tour(),
            Message::FinishSaved(result) => task = self.finish_saved(result, shell),
            Message::FinishAnimationDone => {
                if self.completion == Completion::Leaving {
                    shell.exit();
                }
            }
        }
        task = self.step_changed(before, task);
        if self.state.sound {
            let sound = if self.state.step == Step::Finish && self.state.step != before {
                Some(crate::sound::Sound::Celebrate)
            } else if self.state.step != before {
                Some(crate::sound::Sound::Step)
            } else {
                sound
            };
            if let Some(sound) = sound {
                crate::sound::play(sound);
            }
        }
        task
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        let stepper = step_navigation(paint, self.state.step);
        let header = row![
            glide_mark(paint, 28.0, glide_sdk::theme::R2),
            text("Glide", TextStyle::HEAD),
            text("v0.8.2", TextStyle::MICRO),
            Space::new().width(Fill),
            stepper,
            Space::new().width(Fill),
            sound_button(paint, self.state.sound),
            widget::compact_button(
                paint,
                if self.english { "EN" } else { "UA" },
                Tone::Ghost,
                Some(Message::ToggleLanguage)
            ),
            widget::compact_button(paint, "Пропустити", Tone::Outline, Some(Message::AskSkip)),
        ]
        .align_y(Alignment::Center)
        .spacing(SP2);

        let content = match self.state.step {
            Step::Welcome => Self::hero(paint),
            Step::Network => self.network(paint),
            Step::Appearance => self.appearance(paint),
            Step::IslandAndDock => self.island(paint),
            Step::Gestures => self.gestures(paint),
            Step::Phone => self.phone(paint),
            Step::NightLight => self.night_light(paint),
            Step::Photos => self.photos(paint),
            Step::QuickLook => self.quick_look(paint),
            Step::Snapping => self.snapping(paint),
            Step::Apps => self.apps(paint),
            Step::Screenshot => self.screenshot(paint),
            Step::Finish => self.finish(paint),
        };
        let forward = self.forward;
        let content = anim::animate(1.0, anim::Speed::ARRIVE, |_| content)
            .from(0.0)
            .keyed(self.state.step.index() as u64)
            .transform(move |progress| step_transform(progress, forward));
        let mut footer = row![].spacing(SP3).align_y(Alignment::Center);
        if self.state.step != Step::Welcome {
            footer = footer.push(widget::compact_button(paint, "← Назад", Tone::Outline, Some(Message::Back)));
        }
        footer = footer.push(Space::new().width(Fill));
        if !matches!(self.state.step, Step::Welcome | Step::Finish) {
            footer =
                footer.push(widget::compact_button(paint, "Пропустити цей крок", Tone::Ghost, Some(Message::SkipStep)));
        }
        if self.state.step != Step::Finish {
            footer = footer.push(widget::compact_button(
                paint,
                if self.state.step == Step::Welcome { "Почати →" } else { "Продовжити →" },
                Tone::Accent,
                Some(Message::Next),
            ));
        }

        let background = *paint;
        let top = container(header).height(56).padding([0.0, SP5]).center_y(56).style(move |_| container::Style {
            background: Some(background.bar_bg().into()),
            border: glide_sdk::iced::Border {
                radius: glide_sdk::iced::border::Radius::default()
                    .top_left(glide_sdk::theme::R4)
                    .top_right(glide_sdk::theme::R4),
                ..Default::default()
            },
            ..container::Style::default()
        });
        let page = container(content).width(Fill).padding([SP5, 48.0]);
        let page = if matches!(self.state.step, Step::Welcome | Step::Finish) {
            page.center_y((shell.window().size.height - 118.0).max(0.0))
        } else {
            page
        };
        let page = page_scroll(page.into(), self.state.step);
        let body = container(page).height(Fill).style(move |_| container::Style {
            background: Some(background.view_bg().into()),
            ..container::Style::default()
        });
        let bottom = container(footer).height(60).padding([0.0, SP6]).center_y(60).style(move |_| container::Style {
            background: Some(background.bar_bg().into()),
            border: glide_sdk::iced::Border {
                radius: glide_sdk::iced::border::Radius::default()
                    .bottom_left(glide_sdk::theme::R4)
                    .bottom_right(glide_sdk::theme::R4),
                ..Default::default()
            },
            ..container::Style::default()
        });
        let content = container(column![top, widget::separator(paint), body, widget::separator(paint), bottom])
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(background.win_bg().into()),
                border: glide_sdk::iced::Border { radius: glide_sdk::theme::R4.into(), ..Default::default() },
                text_color: Some(background.fg_1()),
                ..container::Style::default()
            });
        finish_animation(content.into(), self.completion == Completion::Leaving)
    }

    fn close_requested(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        if self.completion == Completion::Idle {
            shell.exit();
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.swipe_start.is_some() {
            glide_sdk::iced::event::listen_with(|event, _, _| swipe_event(&event))
        } else {
            Subscription::none()
        }
    }

    fn look_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.preview(shell);
        Task::none()
    }

    fn key(
        &mut self, event: &glide_sdk::iced::keyboard::Event, captured: bool, shell: &mut Shell<Message>,
    ) -> Task<Message> {
        use glide_sdk::iced::keyboard::{Event, Key, key::Named};
        if captured || shell.has_dialog() {
            return Task::none();
        }
        if let Event::KeyPressed { key: Key::Named(key), .. } = event {
            let message = match key {
                Named::Space if self.state.step == Step::QuickLook => Some(Message::QuickLook),
                Named::ArrowRight | Named::PageDown if self.state.step != Step::Finish => Some(Message::Next),
                Named::ArrowLeft | Named::PageUp if self.state.step != Step::Welcome => Some(Message::Back),
                _ => None,
            };
            if let Some(message) = message {
                return self.update(message, shell);
            }
        }
        Task::none()
    }
}

impl WelcomeApp {
    fn hero<'a>(paint: &Paint) -> Element<'a, Message> {
        let pillar = |symbol, title, description| {
            widget::card(
                paint,
                container(
                    column![
                        text(symbol, TextStyle::T1),
                        text(title, TextStyle::T3),
                        text(description, TextStyle::CALL)
                    ]
                    .spacing(SP2),
                )
                .height(120),
            )
        };
        let pillars = row![
            pillar("⚡", "Швидкість без зусиль", "Прямий рендеринг без зависань. Усе запускається блискавично."),
            pillar("🧘", "Філософія спокою", "Жодного візуального сміття. Чітка геометрія та приємні кольори."),
            pillar("🌊", "Плавний рух", "Вікна, меню та жести рухаються разом із вами."),
        ]
        .spacing(SP3);
        container(
            column![
                anim::hover(anim::Speed::OVERSHOOT, |_| glide_mark(paint, 96.0, glide_sdk::theme::R4)).transform(
                    |progress| anim::Transform {
                        scale: 1.0 + 0.05 * progress,
                        offset: glide_sdk::iced::Vector::new(0.0, -4.0 * progress),
                    }
                ),
                text("Ласкаво просимо в Glide", TextStyle::DISPLAY),
                text(
                    "Швидка, спокійна та передбачувана стільниця нового покоління для Wayland, написана на Rust.",
                    TextStyle::BODY
                ),
                container(pillars).width(Fill).max_width(820),
                row![
                    widget::button(paint, "Почати знайомство →", Tone::Accent, Some(Message::Next)),
                    widget::button(paint, "Пропустити та перейти до стільниці", Tone::Ghost, Some(Message::AskSkip)),
                ]
                .spacing(SP3),
            ]
            .spacing(SP3)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .into()
    }

    fn network<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let ethernet = widget::card(
            paint,
            column![
                row![
                    text("Провідний кабель (Ethernet)", TextStyle::HEAD),
                    Space::new().width(Fill),
                    text("Підключено", TextStyle::MICRO).color(paint.ok())
                ]
                .align_y(Alignment::Center),
                text("Швидкість 1000 Мбіт/с • IPv4: 192.168.1.104", TextStyle::CALL).color(paint.fg_2()),
            ]
            .spacing(SP2),
        );
        let wifi = widget::card(
            paint,
            column![
                row![
                    text("Безпровідний модуль (Wi-Fi)", TextStyle::HEAD),
                    Space::new().width(Fill),
                    widget::switch(paint, self.state.wifi != Wifi::Off, Some(|_| Message::ToggleWifi)),
                ]
                .align_y(Alignment::Center),
                text("Автоматичний швидкий роумінг 2.4 / 5 / 6 ГГц", TextStyle::CALL).color(paint.fg_3()),
            ]
            .spacing(SP2),
        );
        let networks = column![
            text("Знайдені мережі", TextStyle::MICRO).color(paint.fg_3()),
            self.network_option(paint, Network::Home),
            self.network_option(paint, Network::Guest),
        ]
        .spacing(SP2);
        column![
            heading(
                paint,
                1,
                "Підключення",
                "Підключення до Інтернету",
                "Оберіть домашній або офісний Wi-Fi для миттєвого доступу до мережі."
            ),
            row![column![ethernet, wifi].spacing(SP3).width(Fill), container(networks).width(Fill)].spacing(SP4),
        ]
        .spacing(SP5)
        .into()
    }

    fn network_option<'a>(&self, paint: &Paint, network: Network) -> Element<'a, Message> {
        let selected = self.state.network == network;
        let colors = *paint;
        let title =
            if network == Network::Home { "Glide-Home-5G (Рекомендована)" } else { network.name() };
        let header = button(
            row![
                text(title, TextStyle::HEAD),
                Space::new().width(Fill),
                text(network.security(), TextStyle::MICRO).color(if selected {
                    colors.acc_text()
                } else {
                    colors.fg_3()
                })
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .padding(SP3)
        .on_press(Message::SelectNetwork(network))
        .style(move |_, status| button::Style {
            text_color: colors.fg_1(),
            ..widget::button_style(&colors, Tone::Ghost, status)
        });
        let feedback = match self.state.wifi {
            Wifi::Off => "Wi-Fi вимкнено",
            Wifi::Available => "Захищена мережа",
            Wifi::Connecting => "З'єднання...",
            Wifi::Connected => "Підключено • 192.168.1.142",
        };
        let drawer = column![
            widget::password_field(
                paint,
                "Пароль мережі",
                &self.state.wifi_password,
                self.state.wifi_password_visible,
                Message::WifiPassword,
                Message::TogglePassword
            ),
            row![
                text(feedback, TextStyle::CAP).color(if self.state.wifi == Wifi::Connected {
                    paint.ok()
                } else {
                    paint.fg_3()
                }),
                Space::new().width(Fill),
                widget::compact_button(
                    paint,
                    match self.state.wifi {
                        Wifi::Connected => "Підключено",
                        Wifi::Connecting => "З'єднання...",
                        Wifi::Off | Wifi::Available => "Підключитись",
                    },
                    Tone::Accent,
                    (self.state.wifi == Wifi::Available).then_some(Message::ConnectWifi)
                )
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        ]
        .spacing(SP2);
        let content = column![header, anim::collapse(selected, container(drawer).padding(SP3).width(Fill))];
        container(content)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(if selected { colors.acc_soft() } else { colors.card_bg() }.into()),
                border: glide_sdk::iced::Border {
                    color: if selected { colors.acc_bg() } else { colors.line() },
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    fn appearance<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let themes = widget::segmented_fill(
            paint,
            &[(Theme::Light, "Світла"), (Theme::Dark, "Темна"), (Theme::Auto, "Авто (за сонцем)")],
            self.state.theme,
            Message::Theme,
        );
        let accents = [
            (Accent::Blue, "Азурит", ThemeAccent::Blue),
            (Accent::Teal, "Малахіт", ThemeAccent::Teal),
            (Accent::Green, "Хвоя", ThemeAccent::Green),
            (Accent::Amber, "Бурштин", ThemeAccent::Amber),
            (Accent::Orange, "Теракота", ThemeAccent::Orange),
            (Accent::Red, "Цинобра", ThemeAccent::Red),
            (Accent::Purple, "Аметист", ThemeAccent::Purple),
            (Accent::Graphite, "Графіт", ThemeAccent::Graphite),
        ];
        let mut grid = Column::new().spacing(SP2);
        for line in accents.chunks(4) {
            let mut buttons = row![].spacing(SP2);
            for &(accent, label, color) in line {
                buttons = buttons.push(accent_card(paint, accent, label, color, self.state.accent == accent));
            }
            grid = grid.push(buttons);
        }
        let preview = widget::card(
            paint,
            column![
                text("Живий перегляд кнопок та елементів", TextStyle::MICRO),
                row![
                    widget::button(paint, "Головна дія", Tone::Accent, Some(Message::PreviewAction)),
                    widget::button(paint, "Тональна", Tone::Tonal, Some(Message::PreviewAction)),
                    widget::button(paint, "Контур", Tone::Outline, Some(Message::PreviewAction)),
                ]
                .spacing(SP2),
                row![
                    text("Активний перемикач:", TextStyle::BODY),
                    Space::new().width(Fill),
                    widget::switch(paint, true, Some(|_| Message::PreviewAction))
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(SP4),
        );
        column![
            heading(
                paint,
                2,
                "Стиль",
                "Стиль та колірна палітра",
                "Оберіть тему оформлення та системний колірний акцент."
            ),
            row![
                column![themes, text("8 системних акцентів Meridian", TextStyle::MICRO), grid].spacing(SP4).width(Fill),
                container(preview).width(Fill),
            ]
            .spacing(SP4),
        ]
        .spacing(SP5)
        .into()
    }

    fn island<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let label = match self.state.island {
            Island::Music => "🎵 Chopin — Nocturne Op. 9",
            Island::Notification => "🔔 Telegram: Нове повідомлення!",
            Island::Call => "📞 Вхідний дзвінок: Олена",
        };
        let mut dock = row![].spacing(SP2).align_y(Alignment::End);
        for (symbol, name) in
            [("📁", "Файли"), ("🖼️", "Світлини"), ("🛍️", "Каталог програм"), ("📱", "Телефон"), ("⚙️", "Налаштування")]
        {
            dock = dock.push(dock_tile(symbol, name, self.dock.active == Some(name), self.dock.serial));
        }
        let dock = container(dock).padding([SP2, SP3]).style(|_| container::Style {
            background: Some(Color::from_rgba8(28, 27, 30, 0.9).into()),
            border: glide_sdk::iced::Border {
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.14),
                width: 1.0,
                radius: 18.0.into(),
            },
            ..container::Style::default()
        });
        let island = button(stack![
            crate::island::backdrop(self.state.island),
            container(text(label, TextStyle::CAP).color(Color::WHITE)).center_x(240).center_y(26)
        ])
        .padding(0)
        .on_press(Message::CycleIsland)
        .style(|_, _| button::Style { text_color: Color::WHITE, ..button::Style::default() });
        let topbar = container(
            row![
                text("Glide", TextStyle::HEAD).color(paint.acc_text()),
                Space::new().width(Fill),
                island,
                Space::new().width(Fill),
                text("11:45 • 98% ⚡", TextStyle::CAP).color(Color::WHITE),
            ]
            .align_y(Alignment::Center),
        )
        .padding([SP1, SP3])
        .width(Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba8(28, 27, 30, 0.9).into()),
            border: glide_sdk::iced::Border {
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.12),
                width: 1.0,
                radius: 999.0.into(),
            },
            ..container::Style::default()
        });
        let scene_paint = *paint;
        let demo = container(
            column![
                topbar,
                Space::new().height(Fill),
                container(text("💡 Натисніть на острівець вгорі або наведіть на іконки дока внизу", TextStyle::CAP))
                    .center_x(Fill),
                Space::new().height(Fill),
                container(dock).center_x(Fill),
            ]
            .spacing(SP2),
        )
        .padding(SP3)
        .height(310)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(scene_paint.sunken_bg().into()),
            border: glide_sdk::iced::Border { color: scene_paint.line_2(), width: 1.0, radius: 12.0.into() },
            ..container::Style::default()
        });
        column![
            heading(
                paint,
                3,
                "Стільниця",
                "Острівець стану та Док",
                "Спробуйте взаємодію з острівцем стану та іконками дока."
            ),
            demo,
            container(
                row![
                    widget::button(paint, "🎵 Музика", Tone::Tonal, Some(Message::Island(Island::Music))),
                    widget::button(paint, "🔔 Сповіщення", Tone::Outline, Some(Message::Island(Island::Notification))),
                    widget::button(paint, "📞 Дзвінок", Tone::Outline, Some(Message::Island(Island::Call))),
                ]
                .spacing(SP2)
            )
            .center_x(Fill),
        ]
        .spacing(SP5)
        .into()
    }

    fn gestures<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let query = self.search.trim();
        let result = crate::calculator::calculate(query)
            .map_or_else(|| format!("Швидкий запуск програми {query}"), |value| format!("= {value} (Калькулятор)"));
        let mut workspaces = row![].spacing(SP2);
        for (index, name) in ["Робота", "Інтернет", "Медіа"].into_iter().enumerate() {
            let selected = index == self.state.workspace;
            let paint = *paint;
            workspaces =
                workspaces.push(container(text(name, TextStyle::CAP)).center_x(Fill).center_y(48).style(move |_| {
                    container::Style {
                        background: Some(if selected { paint.acc_soft() } else { paint.sunken_bg() }.into()),
                        text_color: Some(if selected { paint.acc_text() } else { paint.fg_3() }),
                        border: glide_sdk::iced::Border {
                            color: if selected { paint.acc_bg() } else { paint.line() },
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..container::Style::default()
                    }
                }));
        }
        let trackpad = mouse_area(widget::card(
            paint,
            column![
                workspaces,
                Space::new().height(Fill),
                text("👆👆👆", TextStyle::T1),
                text("Потягніть мишею ліворуч / праворуч", TextStyle::BODY),
                text("Симуляція трипальцевого свайпу тачпада", TextStyle::CALL),
                Space::new().height(Fill),
            ]
            .spacing(SP2)
            .align_x(Alignment::Center)
            .height(238),
        ))
        .on_press(Message::SwipeStart);
        column![
            heading(
                paint,
                4,
                "Керування",
                "Жести та Робочі простори",
                "Свайп трьома пальцями перемикає робочі простори."
            ),
            row![
                container(trackpad).width(Fill),
                container(widget::card(
                    paint,
                    column![
                        text("Швидкий пошук у Лаунчері (Клавіша Super)", TextStyle::T3),
                        widget::text_field(paint, "Пошук", &self.search, Message::Search),
                        text(result, TextStyle::BODY),
                    ]
                    .spacing(SP3)
                ))
                .width(Fill)
            ]
            .spacing(SP4),
        ]
        .spacing(SP5)
        .into()
    }

    fn phone<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let paired = self.state.phone == Phone::Paired;
        let qr_paint = Paint::new(Appearance::Light, ThemeAccent::Blue);
        let qr =
            container(widget::qr_code(&qr_paint, "glide-phone-demo", 100.0)).padding(SP2).style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                border: glide_sdk::iced::Border { radius: 12.0.into(), ..Default::default() },
                ..container::Style::default()
            });
        let screen = container(
            column![
                qr,
                text(
                    if paired {
                        "✅ Підключено • 92% заряду"
                    } else {
                        "QR-код для демонстрації"
                    },
                    TextStyle::HEAD
                ),
            ]
            .spacing(SP3)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(326)
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(30, 29, 34).into()),
            text_color: Some(Color::WHITE),
            border: glide_sdk::iced::Border { radius: 18.0.into(), ..Default::default() },
            ..container::Style::default()
        });
        let phone = container(screen).padding(SP2).width(220).style(|_| container::Style {
            background: Some(Color::from_rgb8(21, 20, 23).into()),
            border: glide_sdk::iced::Border { color: Color::from_rgb8(45, 43, 48), width: 4.0, radius: 18.0.into() },
            ..container::Style::default()
        });
        column![
            heading(
                paint,
                5,
                "Смартфон",
                "Синхронізація зі смартфоном",
                "Спільний буфер обміну, сповіщення та вхідні дзвінки прямо на вашому екрані."
            ),
            row![
                container(
                    column![
                        widget::card(
                            paint,
                            row![
                                column![
                                    text("Спільний буфер обміну", TextStyle::HEAD),
                                    text("Скопіювали на телефоні — вставили в Glide.", TextStyle::CAP)
                                ]
                                .spacing(SP1)
                                .width(Fill),
                                widget::switch(paint, self.phone_options.clipboard, Some(Message::PhoneClipboard)),
                            ]
                            .spacing(SP2)
                            .align_y(Alignment::Center)
                        ),
                        widget::card(
                            paint,
                            row![
                                column![
                                    text("Дзвінки та SMS", TextStyle::HEAD),
                                    text("Відповідайте на дзвінки без смартфона в руках.", TextStyle::CAP)
                                ]
                                .spacing(SP1)
                                .width(Fill),
                                widget::switch(paint, self.phone_options.calls, Some(Message::PhoneCalls)),
                            ]
                            .spacing(SP2)
                            .align_y(Alignment::Center)
                        ),
                        widget::button(
                            paint,
                            match self.state.phone {
                                Phone::Paired => "З'єднано з Pixel 8 Pro",
                                Phone::Pairing => "З'єднуємо...",
                                Phone::Unpaired => "Симулювати з'єднання",
                            },
                            Tone::Accent,
                            (self.state.phone == Phone::Unpaired).then_some(Message::PairPhone)
                        ),
                    ]
                    .spacing(SP3)
                )
                .width(Fill),
                container(phone).center_x(Fill)
            ]
            .spacing(SP4)
            .align_y(Alignment::Center),
        ]
        .spacing(SP5)
        .into()
    }

    fn night_light<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let warmth = (6500.0 - f32::from(self.state.night_temperature)) / 3300.0;
        let scene = anim::animate(warmth, anim::Speed::COLOR, |amount| {
            let amount = amount.clone();
            container(
                column![
                    text("🌙", TextStyle::DISPLAY),
                    text("Затишне вечірнє читання", TextStyle::T2),
                    text("Потягніть повзунок ліворуч, щоб зігріти екран", TextStyle::CALL),
                ]
                .spacing(SP2)
                .align_x(Alignment::Center),
            )
            .center(Fill)
            .height(290)
            .style(move |_| container::Style {
                background: Some(night_color(amount.get()).into()),
                text_color: Some(Color::WHITE),
                border: glide_sdk::iced::Border { radius: 12.0.into(), ..Default::default() },
                ..container::Style::default()
            })
        });
        column![
            heading(
                paint,
                6,
                "Комфорт та зір",
                "Здоровий зір та Нічне світло",
                "Glide піклується про ваші очі: увечері екран плавно теплішає, знижуючи втому."
            ),
            row![
                container(
                    column![
                        widget::card(
                            paint,
                            column![
                                text("Колірна температура екрана", TextStyle::MICRO),
                                row![
                                    text("6500K", TextStyle::NUM),
                                    widget::slider(
                                        paint,
                                        3200.0..=6500.0,
                                        f32::from(self.state.night_temperature),
                                        Message::Temperature
                                    ),
                                    text(format!("{}K", self.state.night_temperature), TextStyle::NUM)
                                        .color(paint.acc_text()),
                                ]
                                .spacing(SP3)
                                .align_y(Alignment::Center),
                            ]
                            .spacing(SP2)
                        ),
                        widget::card(
                            paint,
                            row![
                                column![
                                    text("Автоматично із заходом сонця", TextStyle::HEAD),
                                    text("Плавний перехід без помітних стрибків.", TextStyle::CAP).color(paint.fg_3())
                                ]
                                .spacing(SP1)
                                .width(Fill),
                                widget::switch(paint, self.night.automatic, Some(Message::NightAutomatic)),
                            ]
                            .spacing(SP2)
                            .align_y(Alignment::Center)
                        ),
                        widget::card(
                            paint,
                            row![
                                column![
                                    text("Режим Фокусу (Не турбувати)", TextStyle::HEAD),
                                    text("Приховує гучні банери під час відпочинку.", TextStyle::CAP)
                                        .color(paint.fg_3())
                                ]
                                .spacing(SP1)
                                .width(Fill),
                                widget::switch(paint, self.night.focus, Some(Message::NightFocus)),
                            ]
                            .spacing(SP2)
                            .align_y(Alignment::Center)
                        ),
                    ]
                    .spacing(SP3)
                )
                .width(Fill),
                container(scene).width(Fill),
            ]
            .spacing(SP4),
        ]
        .spacing(SP5)
        .into()
    }

    fn photos<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let items = [
            ("🏔️", "Карпати", Color::from_rgb8(42, 59, 76)),
            ("🌅", "Захід сонця", Color::from_rgb8(76, 42, 59)),
            ("🌲", "Осінній парк", Color::from_rgb8(42, 76, 56)),
            ("☕", "Кав'ярня", Color::from_rgb8(61, 58, 76)),
        ];
        let mut photos = Column::new().spacing(SP3);
        for (row_index, line) in items.chunks(2).enumerate() {
            let mut tiles = row![].spacing(SP3);
            for (column_index, &(symbol, label, color)) in line.iter().enumerate() {
                let index = row_index * 2 + column_index;
                let tile = container(
                    column![
                        container(text(symbol, TextStyle::DISPLAY)).center(Fill),
                        row![text(label, TextStyle::CALL), Space::new().width(Fill), text("🤍", TextStyle::CALL)],
                    ]
                    .spacing(SP2),
                )
                .padding(SP3)
                .height(139)
                .width(Fill)
                .style(move |_| container::Style {
                    background: Some(color.into()),
                    text_color: Some(Color::WHITE),
                    border: glide_sdk::iced::Border { radius: 12.0.into(), ..Default::default() },
                    ..container::Style::default()
                });
                let hover = anim::hover(anim::Speed::OVERSHOOT, |_| tile).scale(|progress| 1.0 + 0.03 * progress);
                tiles = tiles.push(container(mouse_area(hover).on_release(Message::Photo(index))).width(Fill));
            }
            photos = photos.push(tiles);
        }
        column![
            heading(
                paint,
                7,
                "Світлини та медіа",
                "Glide Світлини: Ваші яскраві спогади",
                "Миттєве відкриття світлин без очікування, зручні альбоми та розумний пошук."
            ),
            row![
                container(photos).width(Fill),
                container(widget::card(
                    paint,
                    container(column![
                        text(
                            self.state.photo.map_or_else(
                                || photo_title(None).to_owned(),
                                |index| format!("Вибрано: {}", photo_title(Some(index)))
                            ),
                            TextStyle::T3
                        ),
                        text("Натисніть на сердечко 🤍, щоб додати улюблену світлину до вибраного з приємним звуковим відгуком.", TextStyle::CALL),
                        row![
                            widget::button(paint, "Збільшити фото", Tone::Accent, Some(Message::PhotoZoom)),
                            widget::button(paint, "❤️ Додати в Улюблені", Tone::Outline, Some(Message::Favourite))
                        ]
                        .spacing(SP2),
                    ]
                    .spacing(SP3)).center_y(258)
                ))
                .width(Fill)
            ]
            .spacing(SP4),
        ]
        .spacing(SP5)
        .into()
    }

    fn quick_look<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        const FILES: [(&str, &str, &str); 3] = [
            ("📄", "Звіт_Glide_2026.pdf", "PDF документ • 2.4 МБ"),
            ("🎵", "Chopin_Nocturne.flac", "Аудіотрек FLAC • 34 МБ"),
            ("🖼️", "План_подорожі.png", "Світлина PNG • 4.1 МБ"),
        ];
        let mut files = Column::new().spacing(SP2);
        for (index, &(icon, name, info)) in FILES.iter().enumerate() {
            files = files.push(file_row(paint, index, icon, name, info, self.state.file == index));
        }
        let right = column![
            text("Натисніть на клавіатурі або кнопку нижче:", TextStyle::BODY),
            widget::button(paint, "[ ПРОБІЛ ]", Tone::Accent, Some(Message::QuickLook)),
            text("Миттєве розгортання картки попереднього перегляду", TextStyle::CAP),
        ]
        .spacing(SP4)
        .align_x(Alignment::Center);
        let panels = row![
            container(files).width(Fill),
            container(widget::card(paint, container(right).center(Fill).height(208))).width(Fill)
        ]
        .spacing(SP4);
        let (_, name, meta) = FILES[self.quick_file];
        let popup = widget::card(
            paint,
            column![
                row![
                    text(name, TextStyle::HEAD),
                    Space::new().width(Fill),
                    widget::button(paint, "✕", Tone::Ghost, Some(Message::QuickLook))
                ]
                .align_y(Alignment::Center),
                container(text("📊", TextStyle::DISPLAY)).center_x(Fill).center_y(140).style({
                    let paint = *paint;
                    move |_| container::Style {
                        background: Some(paint.sunken_bg().into()),
                        border: glide_sdk::iced::Border { radius: 8.0.into(), ..Default::default() },
                        ..container::Style::default()
                    }
                }),
                row![
                    text(meta, TextStyle::CAP),
                    Space::new().width(Fill),
                    widget::button(paint, "Відкрити в додатку", Tone::Tonal, None)
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(SP3),
        );
        let stage: Element<'a, Message> = if self.state.quick_look_open {
            let popup = anim::animate(1.0, anim::Speed::EXPAND, |_| container(popup).width(420))
                .from(0.0)
                .scale(|value| 0.9 + 0.1 * value);
            stack![panels, container(popup).center(Fill)].into()
        } else {
            panels.into()
        };
        column![
            heading(
                paint,
                8,
                "Зручність",
                "Швидкий перегляд клавішею [Пробіл]",
                "Більше не потрібно чекати відкриття важких програм. Натисніть Пробіл — і файл відкриється миттєво."
            ),
            stage,
        ]
        .spacing(SP5)
        .into()
    }

    fn snapping<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let scene = crate::snap::view(paint, self.state.snap, self.snap_demo.pending);
        column![
            heading(
                paint,
                9,
                "Продуктивність",
                "Розумна прив'язка вікон",
                "Розміщуйте програми поруч: піднесіть вікно до краю екрана."
            ),
            row![
                container(scene).width(Fill),
                container(
                    column![
                        text("Спробуйте готові положення вікна:", TextStyle::BODY),
                        widget::button(
                            paint,
                            "◧ Розділити 50% ліворуч",
                            Tone::Outline,
                            Some(Message::Snap(Snap::Left))
                        ),
                        widget::button(
                            paint,
                            "◨ Розділити 50% праворуч",
                            Tone::Outline,
                            Some(Message::Snap(Snap::Right))
                        ),
                        widget::button(
                            paint,
                            "⊞ Розгорнути на весь екран",
                            Tone::Outline,
                            Some(Message::Snap(Snap::Maximized))
                        ),
                        widget::button(paint, "Повернути у центр", Tone::Ghost, Some(Message::Snap(Snap::Floating))),
                    ]
                    .spacing(SP3)
                )
                .width(Fill),
            ]
            .spacing(SP4),
        ]
        .spacing(SP5)
        .into()
    }

    fn apps<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let apps = [
            (DemoApp::Chrome, "Google Chrome", "Швидкий переглядач веб-сторінок", "🌐", Color::from_rgb8(234, 67, 53)),
            (DemoApp::Telegram, "Telegram", "Спілкування з друзями та каналами", "✈️", Color::from_rgb8(34, 158, 217)),
            (DemoApp::Spotify, "Spotify Music", "Мільйони треків та подкастів", "🎧", Color::from_rgb8(30, 215, 96)),
            (DemoApp::LibreOffice, "LibreOffice", "Документи, таблиці та слайди", "📝", Color::from_rgb8(16, 124, 65)),
        ];
        let mut list = Column::new().spacing(SP3);
        for pair in apps.chunks(2) {
            let mut cards = row![].spacing(SP3);
            for &(app, name, description, symbol, color) in pair {
                let installed = self.state.installed_apps.contains(&app);
                let installing = self.state.installing_apps.contains(&app);
                let badge = container(text(symbol, TextStyle::T1)).center(44).style(move |_| container::Style {
                    background: Some(color.into()),
                    border: glide_sdk::iced::Border { radius: 12.0.into(), ..Default::default() },
                    ..container::Style::default()
                });
                cards = cards.push(
                    container(widget::card(
                        paint,
                        row![
                            badge,
                            column![text(name, TextStyle::T3), text(description, TextStyle::CALL)]
                                .spacing(SP1)
                                .width(Fill),
                            container(widget::compact_button(
                                paint,
                                if installed {
                                    "Встановлено ✓"
                                } else if installing {
                                    "Встановлення..."
                                } else {
                                    "Встановити"
                                },
                                if installed { Tone::Tonal } else { Tone::Accent },
                                (!installed && !installing).then_some(Message::Install(app))
                            ))
                            .width(126),
                        ]
                        .spacing(SP3)
                        .align_y(Alignment::Center),
                    ))
                    .width(Fill),
                );
            }
            list = list.push(cards);
        }
        column![
            heading(
                paint,
                10,
                "Застосунки",
                "Усі потрібні програми в один клік",
                "Спробуйте демонстрацію каталогу програм."
            ),
            list
        ]
        .spacing(SP5)
        .into()
    }

    fn screenshot<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let accent = paint.acc_bg();
        let crop =
            container(text("🎯 Область захоплення", TextStyle::BODY)).center_x(240).center_y(160).style(move |_| {
                container::Style {
                    background: Some(Color::from_rgb8(24, 23, 28).into()),
                    text_color: Some(Color::WHITE),
                    border: glide_sdk::iced::Border { color: accent, width: 2.0, radius: 8.0.into() },
                    ..container::Style::default()
                }
            });
        let scene = container(crop).center_x(Fill).center_y(290).style(|_| container::Style {
            background: Some(Color::from_rgb8(8, 8, 10).into()),
            border: glide_sdk::iced::Border { radius: 12.0.into(), ..Default::default() },
            ..container::Style::default()
        });
        let flash = anim::animate(f32::from(self.flash), anim::Speed::PRESS, |value| {
            let value = value.clone();
            container(Space::new().width(Fill).height(Fill)).style(move |_| container::Style {
                background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.9 * value.get()).into()),
                border: glide_sdk::iced::Border { radius: glide_sdk::theme::R3.into(), ..Default::default() },
                ..container::Style::default()
            })
        });
        let scene = stack![scene, flash];
        column![
            heading(
                paint,
                11,
                "Творчість",
                "Знімки та Запис екрана (Glide Shot)",
                "Зберігайте важливі моменти клавішею PrintScreen."
            ),
            row![
                container(scene).width(Fill),
                container(
                    column![
                        widget::card(
                            paint,
                            column![
                                text("Режими захоплення", TextStyle::T3),
                                text("Виділена область • Окреме вікно • Повний екран", TextStyle::CALL)
                            ]
                            .spacing(SP2)
                        ),
                        widget::button(
                            paint,
                            "📸 Зробити тестовий знімок",
                            Tone::Accent,
                            (!self.flash).then_some(Message::Screenshot)
                        ),
                        text("Демонстрація знімка екрана", TextStyle::CALL),
                    ]
                    .spacing(SP4)
                )
                .width(Fill)
            ]
            .spacing(SP4)
            .align_y(Alignment::Center),
        ]
        .spacing(SP5)
        .into()
    }

    fn finish<'a>(&self, paint: &Paint) -> Element<'a, Message> {
        let theme = match self.state.theme {
            Theme::Light => "Світла",
            Theme::Dark | Theme::Auto => "Темна",
        };
        let accent = match self.state.accent {
            Accent::Blue => "Азурит",
            Accent::Teal => "Малахіт",
            Accent::Green => "Хвоя",
            Accent::Amber => "Бурштин",
            Accent::Orange => "Теракота",
            Accent::Red => "Цинобра",
            Accent::Purple => "Аметист",
            Accent::Graphite => "Графіт",
        };
        let network =
            if self.state.wifi == Wifi::Connected { self.state.network.name() } else { "Не підключено" };
        let summary = |label: &'static str, value: String| {
            container(widget::card(
                paint,
                column![text(label, TextStyle::MICRO).color(paint.fg_3()), text(value, TextStyle::HEAD)].spacing(SP2),
            ))
            .width(Fill)
        };
        let shortcuts =
            ["Super — Лаунчер", "Пробіл — Швидкий перегляд", "PrintScreen — Знімок екрана", "Super + L — Блокування"];
        let mut keys = row![].spacing(SP2);
        for label in shortcuts {
            let paint = *paint;
            keys =
                keys.push(container(text(label, TextStyle::CAP)).padding([5, 10]).style(move |_| container::Style {
                    background: Some(paint.fill_2().into()),
                    border: glide_sdk::iced::Border { radius: 999.0.into(), ..Default::default() },
                    ..container::Style::default()
                }));
        }
        let celebration_paint = *paint;
        let celebration = container(text("✨", TextStyle::DISPLAY)).center(76).style(move |_| container::Style {
            background: Some(celebration_paint.acc_soft().into()),
            text_color: Some(celebration_paint.acc_text()),
            border: glide_sdk::iced::Border { radius: 999.0.into(), ..Default::default() },
            ..container::Style::default()
        });
        container(
            column![
                celebration,
                text("Усе готово до вашого польоту!", TextStyle::DISPLAY),
                text("Огляд вашого знайомства з Glide", TextStyle::BODY),
                container(
                    row![
                        summary("Тема", format!("{theme} • {accent}")),
                        summary("Мережа", network.to_owned()),
                        summary("Нічне світло", format!("{}K", self.state.night_temperature)),
                        summary("Програми", format!("{} у демонстрації", self.state.installed_apps.len())),
                    ]
                    .spacing(SP2)
                )
                .width(Fill)
                .max_width(860),
                keys,
                widget::button(
                    paint,
                    "Увійти в стільницю Glide →",
                    Tone::Accent,
                    (self.completion == Completion::Idle).then_some(Message::Finish)
                ),
            ]
            .spacing(SP3)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .into()
    }
}

fn heading_icon(number: usize, tint: Color) -> Element<'static, Message> {
    let paths = match number {
        1 => r#"<path d="M5 12.55a11 11 0 0 1 14.08 0M1.42 9a16 16 0 0 1 21.16 0"/><circle cx="12" cy="20" r="1"/>"#,
        2 => r#"<circle cx="12" cy="12" r="10"/><path d="M12 2a7 7 0 0 0 0 14v6"/>"#,
        3 => r#"<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>"#,
        4 => r#"<path d="M18 11V6a2 2 0 0 0-4 0v5M14 10V4a2 2 0 0 0-4 0v7M10 10.5V6a2 2 0 0 0-4 0v8"/>"#,
        5 => r#"<rect x="5" y="2" width="14" height="20" rx="2"/><path d="M12 18h.01"/>"#,
        6 => r#"<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>"#,
        7 => {
            r#"<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/>"#
        }
        8 => r#"<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>"#,
        9 => r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M12 3v18"/>"#,
        10 => r#"<path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4ZM3 6h18M16 10a4 4 0 0 1-8 0"/>"#,
        11 => {
            r#"<path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"/><circle cx="12" cy="13" r="4"/>"#
        }
        _ => "",
    };
    let image = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2">{paths}</svg>"#
    );
    widget::svg_glyph(svg::Handle::from_memory(image.into_bytes()), 14.0, tint)
}

fn heading<'a>(
    paint: &Paint, number: usize, category: &'a str, title: &'a str, description: &'a str,
) -> Element<'a, Message> {
    column![
        row![
            heading_icon(number, paint.acc_text()),
            text(format!("Крок {number} із 12 • {category}"), TextStyle::MICRO).color(paint.acc_text()),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        text(title, TextStyle::T1),
        text(description, TextStyle::BODY).color(paint.fg_2()),
    ]
    .spacing(SP2)
    .into()
}

fn delayed(milliseconds: u64, message: Message) -> Task<Message> {
    Task::perform(async move { tokio::time::sleep(std::time::Duration::from_millis(milliseconds)).await }, move |()| {
        message
    })
}

fn accent_card(
    paint: &Paint, accent: Accent, label: &'static str, color: ThemeAccent, selected: bool,
) -> Element<'static, Message> {
    let paint = *paint;
    let color = glide_sdk::theme::to_iced(color.ramp()[3]);
    let swatch = container(Space::new().width(24).height(24)).style(move |_| container::Style {
        background: Some(color.into()),
        border: glide_sdk::iced::Border { radius: 999.0.into(), ..Default::default() },
        ..container::Style::default()
    });
    button(
        container(column![swatch, text(label, TextStyle::MICRO)].spacing(SP2).align_x(Alignment::Center))
            .center_x(Fill),
    )
    .width(Fill)
    .padding(SP2)
    .on_press(Message::Accent(accent))
    .style(move |_, status| button::Style {
        background: Some(
            if selected {
                paint.acc_soft()
            } else if status == button::Status::Hovered {
                paint.fill_1()
            } else {
                paint.card_bg()
            }
            .into(),
        ),
        text_color: paint.fg_1(),
        border: glide_sdk::iced::Border {
            color: if selected { paint.acc_bg() } else { paint.line() },
            width: 1.0,
            radius: 8.0.into(),
        },
        ..button::Style::default()
    })
    .into()
}

fn file_row(
    paint: &Paint, index: usize, icon: &'static str, name: &'static str, info: &'static str, selected: bool,
) -> Element<'static, Message> {
    let paint = *paint;
    let selection = text(if selected { "ВИБРАНО" } else { "" }, TextStyle::MICRO).color(paint.acc_text());
    button(
        row![
            text(icon, TextStyle::T2),
            column![text(name, TextStyle::HEAD), text(info, TextStyle::CAP).color(paint.fg_3())].spacing(SP1),
            Space::new().width(Fill),
            selection,
        ]
        .spacing(SP3)
        .align_y(Alignment::Center),
    )
    .padding([10, 14])
    .width(Fill)
    .on_press(Message::File(index))
    .style(move |_, status| button::Style {
        background: Some(
            if selected {
                paint.acc_soft()
            } else if status == button::Status::Hovered {
                paint.fill_1()
            } else {
                paint.card_bg()
            }
            .into(),
        ),
        text_color: paint.fg_1(),
        border: glide_sdk::iced::Border {
            color: if selected { paint.acc_bg() } else { paint.line() },
            width: 1.0,
            radius: glide_sdk::theme::R2.into(),
        },
        ..button::Style::default()
    })
    .into()
}

fn dock_tile(symbol: &'static str, name: &'static str, active: bool, serial: u64) -> Element<'static, Message> {
    let face = container(text(symbol, TextStyle::T1)).center(42).style(|_| container::Style {
        background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.09).into()),
        border: glide_sdk::iced::Border { radius: 12.0.into(), ..Default::default() },
        ..container::Style::default()
    });
    let bounce = anim::animate(1.0, anim::Speed::EXPAND, |_| face)
        .from(if active { 0.0 } else { 1.0 })
        .keyed(if active { serial } else { 0 })
        .transform(|progress| {
            let lift = (progress * std::f32::consts::PI).sin().max(0.0);
            anim::Transform { scale: 1.0 + 0.15 * lift, offset: glide_sdk::iced::Vector::new(0.0, -16.0 * lift) }
        });
    let tile = anim::hover(anim::Speed::OVERSHOOT, |_| bounce).transform(|progress| anim::Transform {
        scale: 1.0 + 0.35 * progress,
        offset: glide_sdk::iced::Vector::new(0.0, -6.0 * progress),
    });
    mouse_area(tile).on_release(Message::DockApp(name)).into()
}

fn step_navigation(paint: &Paint, selected: Step) -> Element<'static, Message> {
    let dots = Step::ALL.into_iter().fold(row![].spacing(4), |row, step| row.push(step_dot(paint, step, selected)));
    let paint = *paint;
    container(stack![crate::stepper::thumb(selected.index(), paint.acc_bg()), dots].width(360).height(24))
        .padding(3)
        .style(move |_| container::Style {
            background: Some(paint.fill_1().into()),
            border: glide_sdk::iced::Border { radius: glide_sdk::theme::R_ROUND.into(), ..Default::default() },
            ..container::Style::default()
        })
        .into()
}

fn step_dot(paint: &Paint, step: Step, selected: Step) -> Element<'static, Message> {
    let paint = *paint;
    button(container(text(step.index().to_string(), TextStyle::MICRO)).center(24))
        .width(24)
        .height(24)
        .padding(0)
        .on_press(Message::GoTo(step))
        .style(move |_, status| {
            let active = selected == step;
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: if hovered && !active { Some(paint.fill_2().into()) } else { None },
                text_color: if active {
                    paint.acc_fg()
                } else if step.index() < selected.index() {
                    paint.acc_text()
                } else {
                    paint.fg_3()
                },
                border: glide_sdk::iced::Border { radius: 999.0.into(), ..Default::default() },
                ..button::Style::default()
            }
        })
        .into()
}

fn sound_button(paint: &Paint, enabled: bool) -> Element<'static, Message> {
    const ON: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round'><path d='M11 5 6 9H2v6h4l5 4zM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14'/></svg>";
    const OFF: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round'><path d='M11 5 6 9H2v6h4l5 4zM17 9l6 6m0-6-6 6'/></svg>";
    let paint = *paint;
    let icon = svg(svg::Handle::from_memory(if enabled { ON } else { OFF }.as_bytes()))
        .width(16)
        .height(16)
        .style(move |_, _| svg::Style { color: Some(paint.fg_2()) });
    button(container(icon).center(Fill))
        .width(28)
        .height(28)
        .padding(0)
        .on_press(Message::ToggleSound)
        .style(move |_, status| widget::button_style(&paint, Tone::Ghost, status))
        .into()
}

fn glide_mark(paint: &Paint, side: f32, radius: f32) -> Element<'static, Message> {
    const MARK: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2.2' stroke-linecap='round' stroke-linejoin='round'><path d='M4 16l8-8 8 8M7 19l5-5 5 5'/></svg>";
    let paint = *paint;
    let glyph = svg(svg::Handle::from_memory(MARK.as_bytes()))
        .width(side * 0.55)
        .height(side * 0.55)
        .style(move |_, _| svg::Style { color: Some(paint.acc_fg()) });
    container(glyph)
        .center(side)
        .style(move |_| container::Style {
            background: Some(paint.acc_bg().into()),
            border: glide_sdk::iced::Border { radius: radius.into(), ..Default::default() },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_sdk::Look;
    use glide_sdk::app::{Event, Harness};
    use glide_sdk::iced::{Point, Size};

    #[test]
    fn step_entrance_moves_from_the_navigation_direction_and_settles_at_full_size() {
        let forward = step_transform(0.0, true);
        let backward = step_transform(0.0, false);
        assert_eq!(forward.offset.x, 48.0);
        assert_eq!(backward.offset.x, -48.0);
        assert_eq!(forward.scale, 0.98);
        let settled = step_transform(1.0, true);
        assert_eq!(settled.offset.x, 0.0);
        assert_eq!(settled.scale, 1.0);
    }

    #[test]
    fn trackpad_drag_changes_the_workspace_only_after_the_threshold() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::GoTo(Step::Gestures));
        app.send(Event::Pointer(Point::new(200.0, 200.0)));
        app.message(Message::SwipeStart);
        app.send(Event::Pointer(Point::new(170.0, 200.0)));
        app.message(Message::SwipeEnd);
        assert_eq!(app.app().state.workspace, 0);
        app.send(Event::Pointer(Point::new(200.0, 200.0)));
        app.message(Message::SwipeStart);
        app.send(Event::Pointer(Point::new(150.0, 200.0)));
        app.message(Message::SwipeEnd);
        assert_eq!(app.app().state.workspace, 1);
    }

    #[test]
    fn clicking_the_island_cycles_its_three_demo_states() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        assert_eq!(app.app().state.island, Island::Music);
        app.message(Message::CycleIsland);
        assert_eq!(app.app().state.island, Island::Notification);
        app.message(Message::CycleIsland);
        assert_eq!(app.app().state.island, Island::Call);
        app.message(Message::CycleIsland);
        assert_eq!(app.app().state.island, Island::Music);
    }

    #[test]
    fn trackpad_release_is_global_and_losing_focus_cancels_the_drag() {
        use glide_sdk::iced::{Event as IcedEvent, mouse, window};

        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::GoTo(Step::Gestures));
        app.send(Event::Pointer(Point::new(200.0, 200.0)));
        app.message(Message::SwipeStart);
        app.send(Event::Pointer(Point::new(20.0, 20.0)));
        app.message(swipe_event(&IcedEvent::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))).unwrap());
        assert_eq!(app.app().state.workspace, 1);

        app.message(Message::SwipeStart);
        app.message(swipe_event(&IcedEvent::Window(window::Event::Unfocused)).unwrap());
        app.send(Event::Pointer(Point::new(200.0, 200.0)));
        app.message(Message::SwipeEnd);
        assert_eq!(app.app().state.workspace, 1);
        assert!(app.app().swipe_start.is_none());
    }

    #[test]
    fn skip_confirmation_opens_a_modal_before_the_summary() {
        use glide_sdk::overlay::Answer;

        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::AskSkip);
        assert!(app.shell().has_dialog());
        assert_eq!(app.app().state.step, Step::Welcome);
        app.send(Event::Dialog(Answer::Chose(0)));
        assert!(!app.shell().has_dialog());
        assert_eq!(app.app().state.step, Step::Welcome);
        app.message(Message::AskSkip);
        app.send(Event::Dialog(Answer::Chose(1)));
        assert!(!app.shell().has_dialog());
        assert_eq!(app.app().state.step, Step::Finish);
    }

    #[test]
    fn skipping_a_step_advances_without_skipping_the_welcome_or_finale() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::SkipStep);
        assert_eq!(app.app().state.step, Step::Welcome);
        app.message(Message::Next);
        app.message(Message::SkipStep);
        assert_eq!(app.app().state.step, Step::Appearance);
        app.message(Message::GoTo(Step::Finish));
        app.message(Message::SkipStep);
        assert_eq!(app.app().state.step, Step::Finish);
    }

    #[test]
    fn scroll_positions_belong_to_the_step_that_reported_them() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(850.0, 580.0));
        app.message(Message::ToggleSound);
        app.message(Message::GoTo(Step::Network));
        app.message(Message::Scrolled(Step::Network, 125.0));
        app.message(Message::GoTo(Step::Phone));
        app.message(Message::Scrolled(Step::Phone, 60.0));
        app.message(Message::Scrolled(Step::Network, 100.0));
        assert_eq!(app.app().scroll_positions[Step::Network.index()], 100.0);
        assert_eq!(app.app().scroll_positions[Step::Phone.index()], 60.0);
        assert_eq!(app.app().scroll_positions[Step::Appearance.index()], 0.0);
    }

    #[test]
    fn scrollable_reports_the_step_and_accepts_restored_offsets() {
        use glide_sdk::iced::advanced::widget::{Id, Operation, Tree, operation};
        use glide_sdk::iced::advanced::{Layout, Shell as WidgetShell, clipboard, layout};
        use glide_sdk::iced::{Event as IcedEvent, Font, Pixels, Rectangle, Renderer, Vector, mouse};

        #[derive(Default)]
        struct InspectOffset(f32);
        impl Operation for InspectOffset {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn scrollable(
                &mut self, id: Option<&Id>, _: Rectangle, _: Rectangle, translation: Vector,
                _: &mut dyn operation::Scrollable,
            ) {
                if id == Some(&Id::new(CONTENT_SCROLL)) {
                    self.0 = translation.y;
                }
            }
        }

        let size = Size::new(300.0, 200.0);
        let mut page = page_scroll(Space::new().width(300).height(1000).into(), Step::Network);
        let mut tree = Tree::new(&page);
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let node = page.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut messages = Vec::new();
        let mut shell = WidgetShell::new(&mut messages);
        page.as_widget_mut().update(
            &mut tree,
            &IcedEvent::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -125.0 } }),
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(50.0, 50.0)),
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        assert!(matches!(messages.as_slice(), [Message::Scrolled(Step::Network, offset)] if *offset == 125.0));

        let mut restore = operation::scrollable::scroll_to::<()>(
            Id::new(CONTENT_SCROLL),
            operation::scrollable::AbsoluteOffset { x: Some(0.0), y: Some(60.0) },
        );
        page.as_widget_mut().operate(&mut tree, Layout::new(&node), &renderer, &mut restore);
        let mut actual = InspectOffset::default();
        page.as_widget_mut().operate(&mut tree, Layout::new(&node), &renderer, &mut actual);
        assert_eq!(actual.0, 60.0);
    }

    #[test]
    fn cancelling_wifi_prevents_a_delayed_connection_from_turning_it_back_on() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::ConnectWifi);
        assert_eq!(app.app().state.wifi, Wifi::Connecting);
        app.message(Message::ToggleWifi);
        app.message(Message::ToggleWifi);
        app.message(Message::ConnectWifi);
        app.message(Message::WifiConnected(1));
        assert_eq!(app.app().state.wifi, Wifi::Connecting);
        app.message(Message::ToggleWifi);
        app.message(Message::WifiConnected(4));
        assert_eq!(app.app().state.wifi, Wifi::Off);
    }

    #[test]
    fn choosing_another_network_cancels_the_old_connection_attempt() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::ConnectWifi);
        let old = app.app().wifi_generation;
        app.message(Message::SelectNetwork(Network::Guest));
        app.message(Message::WifiConnected(old));
        assert_eq!(app.app().state.network, Network::Guest);
        assert_eq!(app.app().state.wifi, Wifi::Available);
        app.message(Message::ConnectWifi);
        let current = app.app().wifi_generation;
        app.message(Message::WifiConnected(current));
        assert_eq!(app.app().state.wifi, Wifi::Connected);
    }

    #[test]
    fn simulated_installations_can_complete_independently_without_duplicates() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::Install(DemoApp::Chrome));
        app.message(Message::Install(DemoApp::Chrome));
        app.message(Message::Install(DemoApp::Telegram));
        assert_eq!(app.app().state.installing_apps.len(), 2);
        app.message(Message::Installed(DemoApp::Telegram));
        app.message(Message::Installed(DemoApp::Chrome));
        app.message(Message::Installed(DemoApp::Chrome));
        assert_eq!(app.app().state.installed_apps.len(), 2);
        assert!(app.app().state.installing_apps.is_empty());
    }

    #[test]
    fn quick_look_keeps_the_file_it_opened_until_reopened() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::GoTo(Step::QuickLook));
        app.message(Message::File(1));
        app.message(Message::QuickLook);
        app.message(Message::File(2));
        assert_eq!(app.app().quick_file, 1);
        app.message(Message::QuickLook);
        app.message(Message::QuickLook);
        assert_eq!(app.app().quick_file, 2);
    }

    #[test]
    fn clicking_the_same_dock_tile_restarts_its_bounce() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::DockApp("Файли"));
        assert_eq!(app.app().dock.active, Some("Файли"));
        let first = app.app().dock.serial;
        app.message(Message::DockApp("Файли"));
        assert_ne!(app.app().dock.serial, first);
        app.message(Message::DockApp("Світлини"));
        assert_eq!(app.app().dock.active, Some("Світлини"));
    }

    #[test]
    fn phone_demo_options_can_be_toggled_independently() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(850.0, 580.0));
        app.message(Message::ToggleSound);
        app.message(Message::PhoneCalls(false));
        assert!(app.app().phone_options.clipboard);
        assert!(!app.app().phone_options.calls);
        app.message(Message::PhoneClipboard(false));
        assert!(!app.app().phone_options.clipboard);
    }

    #[test]
    fn night_preview_warms_as_the_temperature_is_lowered() {
        let cool = night_color(0.0);
        let warm = night_color(1.0);
        assert!(warm.r > cool.r && warm.b < cool.b);
        assert_eq!(night_color(-1.0), cool);
        assert_eq!(night_color(2.0), warm);
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(850.0, 580.0));
        app.message(Message::ToggleSound);
        app.message(Message::NightFocus(true));
        app.message(Message::NightAutomatic(false));
        app.message(Message::Temperature(3499.6));
        app.message(Message::ConfirmSkip);
        assert!(app.app().night.focus);
        assert!(!app.app().night.automatic);
        assert_eq!(app.app().state.night_temperature, 3500);
        assert_eq!(app.app().state.step, Step::Finish);
    }

    #[test]
    fn finish_waits_for_saved_marker_and_allows_retry_after_failure() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::Finish);
        assert!(app.app().completion == Completion::Saving);
        assert!(!app.exiting());
        app.message(Message::FinishSaved(Err("disk error".into())));
        assert!(app.app().completion == Completion::Idle);
        assert!(!app.exiting());
        app.message(Message::Finish);
        app.message(Message::FinishSaved(Ok(())));
        assert!(app.app().completion == Completion::Leaving);
        assert!(!app.exiting());
        app.message(Message::Back);
        assert!(app.app().completion == Completion::Leaving);
        app.message(Message::FinishAnimationDone);
        assert!(app.exiting());
    }

    #[test]
    fn a_later_snap_replaces_the_pending_target() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        app.message(Message::Snap(Snap::Left));
        let first = app.app().snap_demo.generation;
        app.message(Message::Snap(Snap::Right));
        let second = app.app().snap_demo.generation;
        app.message(Message::SnapCommit(first));
        assert_eq!(app.app().state.snap, Snap::Floating);
        assert_eq!(app.app().snap_demo.pending, Some(Snap::Right));
        app.message(Message::SnapCommit(second));
        assert_eq!(app.app().state.snap, Snap::Right);
        assert_eq!(app.app().snap_demo.pending, None);
    }

    #[test]
    fn gallery_starts_unselected_and_rejects_out_of_range_photos() {
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        assert_eq!(app.app().state.photo, None);
        app.message(Message::Photo(2));
        assert_eq!(app.app().state.photo, Some(2));
        app.message(Message::Photo(100));
        assert_eq!(app.app().state.photo, Some(2));
    }

    #[test]
    fn navigation_keys_do_not_override_focused_inputs() {
        use glide_sdk::iced::keyboard::{
            self, Key,
            key::{Code, Named, Physical},
        };
        let mut app = Harness::<WelcomeApp>::new("glide-welcome", Look::default(), Size::new(1120.0, 760.0));
        app.message(Message::ToggleSound);
        let key = Key::Named(Named::ArrowRight);
        let event = keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: Physical::Code(Code::ArrowRight),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        };
        app.send(Event::Key(event.clone(), true));
        assert_eq!(app.app().state.step, Step::Welcome);
        app.send(Event::Key(event, false));
        assert_eq!(app.app().state.step, Step::Network);
    }

    #[test]
    fn sliding_step_indicator_does_not_intercept_step_clicks() {
        use glide_sdk::iced::advanced::widget::Tree;
        use glide_sdk::iced::advanced::{Layout, Shell as WidgetShell, clipboard, layout};
        use glide_sdk::iced::{Event as IcedEvent, Font, Pixels, Rectangle, Renderer, mouse};
        let paint = Paint::new(Appearance::Dark, ThemeAccent::Teal);
        let mut navigation = step_navigation(&paint, Step::Welcome);
        let mut tree = Tree::new(&navigation);
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let size = Size::new(366.0, 30.0);
        let node = navigation.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let cursor = mouse::Cursor::Available(Point::new(183.0, 15.0));
        let mut messages = Vec::new();
        for event in
            [mouse::Event::ButtonPressed(mouse::Button::Left), mouse::Event::ButtonReleased(mouse::Button::Left)]
        {
            let mut shell = WidgetShell::new(&mut messages);
            navigation.as_widget_mut().update(
                &mut tree,
                &IcedEvent::Mouse(event),
                Layout::new(&node),
                cursor,
                &renderer,
                &mut clipboard::Null,
                &mut shell,
                &Rectangle::with_size(size),
            );
        }
        assert!(matches!(messages.as_slice(), [Message::GoTo(Step::NightLight)]));
    }
}
