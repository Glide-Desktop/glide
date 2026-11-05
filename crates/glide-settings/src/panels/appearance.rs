use std::path::PathBuf;
use std::sync::Mutex;

use glide_config::toolkits::{QtTheme, Support};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row, space};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::{gated, switch_row, toggle};
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::services::apps;
use crate::services::config::{ConfigStore, SPEED_MAX, SPEED_MIN, multiplier_from_speed, speed_from_multiplier};
use crate::services::look::{self, FontRendering, Hinting, Smoothing};
use crate::services::run::{Debounce, blocking};
use crate::theme::{self, Accent, Appearance, ThemeState};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        NoIcons => "This theme has none of the sample icons", "У цій темі немає зразкових іконок";
        FontSample => "The quick brown fox jumps over the lazy dog · Швидкий бурий лис", "Швидкий бурий лис перестрибує через лінивого пса · The quick brown fox";
        Smoothing => "Text smoothing", "Згладжування тексту";
        SmoothingHint => "Subpixel is sharper on most LCD panels", "Субпіксельне чіткіше на більшості РК-панелей";
        Grayscale => "Standard", "Стандартне";
        Subpixel => "Subpixel", "Субпіксельне";
        SmoothingNone => "None", "Без згладжування";
        Hinting => "Hinting", "Хінтинг";
        HintingHint => "Stronger hinting aligns letters to pixels", "Сильніший хінтинг вирівнює літери за пікселями";
        HintNone => "None", "Немає";
        HintSlight => "Slight", "Легкий";
        HintMedium => "Medium", "Середній";
        HintFull => "Full", "Повний";
        RenderingFailed => "Could not change the setting:", "Не вдалося змінити налаштування:";
        CursorTheme => "Pointer", "Вказівник";
        Theme => "Theme", "Тема";
        ThemeHint => "The shell, window frames and apps", "Оболонка, рамки вікон і програми";
        Auto => "Automatic", "Автоматично";
        Schedule => "Switching automatically", "Автоматичне перемикання";
        ScheduleHint => "Light from 7:00, dark from 19:00", "Світла з 7:00, темна з 19:00";
        DarkHours => "Dark theme", "Темна тема";
        FixedHours => "Fixed hours in local time", "Фіксований час за місцевим годинником";
        FontsIcons => "Fonts and icons", "Шрифти й іконки";
        ShowSamples => "Show font and icon samples", "Показати зразки шрифту й іконок";
        HideSamples => "Hide font and icon samples", "Сховати зразки шрифту й іконок";
        Changed => "changed", "змінено";
        Reset => "Reset", "Скинути";
        CursorThemeHint => "Some apps pick it up when they start again", "Деякі програми підхоплять її після перезапуску";
        About => "About", "Довідка";
        Intro => "Glide hands its colours and shapes to apps built with other toolkits.", "Glide передає свої кольори й форми програмам інших бібліотек.";
        Slower => "Slower", "Повільніше";
        Faster => "Faster", "Швидше";
        Instant => "Everything appears instantly", "Усе з'являється миттєво";
        SpeedNormal => "Standard speed", "Стандартна швидкість";
        OtherApps => "Other apps", "Інші програми";
        CheckingToolkits => "Checking support…", "Перевіряємо підтримку…";
        ToolkitsUnavailable => "Support could not be checked", "Не вдалося перевірити підтримку";
        OtherAppsHint => "How GTK and Qt apps take Glide's colours and shapes; install what is missing with your package manager", "Як програми GTK і Qt беруть кольори й форми Glide; відсутнє встановіть менеджером пакетів";
        Gtk4 => "GTK 4 and libadwaita", "GTK 4 і libadwaita";
        Gtk4Hint => "Follow Glide without extra packages", "Слідують за Glide без додаткових пакетів";
        Gtk3 => "GTK 3, Chrome, Electron", "GTK 3, Chrome, Electron";
        Gtk3Ready => "Through the adw-gtk3 theme", "Через тему adw-gtk3";
        Gtk3Missing => "Needs the adw-gtk3 theme (adw-gtk-theme)", "Потрібна тема adw-gtk3 (adw-gtk-theme)";
        Qt => "Qt apps", "Програми Qt";
        QtReady => "Through qt6ct or qt5ct", "Через qt6ct або qt5ct";
        QtBridge => "Through the GTK 3 theme; qt6ct gives the full palette", "Через тему GTK 3; повну палітру дає qt6ct";
        QtMissing => "Needs qt6ct (and qt5ct for Qt 5 apps)", "Потрібен qt6ct (і qt5ct для програм Qt 5)";
        Kvantum => "Qt control shapes", "Форми елементів Qt";
        KvantumReady => "Kvantum draws them in the Glide style", "Kvantum малює їх у стилі Glide";
        KvantumMissing => "Install kvantum for rounded Glide controls", "Встановіть kvantum для заокруглених елементів Glide";
        Relogin => "Log out and back in so Qt apps pick it up", "Вийдіть і увійдіть знову, щоб програми Qt це підхопили";
        Ready => "Ready", "Готово";
        Partly => "Partly", "Частково";
        Missing => "Not installed", "Не встановлено";
        Optional => "Optional", "Необов'язково";
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    SetAppearance(Appearance),
    Schedule(bool),
    SetAccent(Accent),
    Animations(bool),
    Speed(f64),
    CommitSpeed,
    SettleSpeed(u64),
    RespectSystem(bool),
    Assets(Vec<String>, Vec<String>, Vec<String>),
    IconTheme(Choice),
    CursorTheme(Choice),
    Font(Choice),
    IconPreview(Option<String>, Vec<PathBuf>),
    Rendering(Option<FontRendering>),
    Smoothing(Smoothing),
    Hinting(Hinting),
    RenderingApplied(Result<String, String>),
    Toolkits(Option<Support>),
    ResetFonts,
    ToggleSamples,
    ToggleInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: Option<String>,
    pub label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn choices(default_label: &str, values: &[String]) -> Vec<Choice> {
    std::iter::once(Choice { value: None, label: default_label.to_string() })
        .chain(values.iter().map(|value| Choice { value: Some(value.clone()), label: value.clone() }))
        .collect()
}

#[derive(Debug, Default, Clone, Copy)]
enum ToolkitStatus {
    #[default]
    Checking,
    Unavailable,
    Ready(Support),
}

#[derive(Debug, Default)]
pub struct Page {
    icon_themes: Vec<String>,
    cursor_themes: Vec<String>,
    fonts: Vec<String>,
    loaded: bool,
    speed: Option<f64>,
    settle: Debounce,
    preview_requested: bool,
    preview_theme: Option<String>,
    preview: Vec<PathBuf>,
    rendering: Option<FontRendering>,
    rendering_error: Option<String>,
    toolkits: ToolkitStatus,
    disclosure: Disclosure,
}

#[derive(Debug, Default)]
struct Disclosure {
    samples: bool,
    info: bool,
}

const FONT_KEYS: [(&str, &str); 3] = [("dock", "icon-theme"), ("general", "font-family"), ("general", "cursor-theme")];
const DEFAULT_RENDERING: FontRendering = FontRendering { smoothing: Smoothing::Grayscale, hinting: Hinting::Slight };

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "theme",
        "dark",
        "light",
        "accent",
        "color",
        "colour",
        "icons",
        "font",
        "gtk",
        "qt",
        "kvantum",
        "animation",
        "motion",
        "hinting",
        "cursor",
        "pointer",
        "тема",
        "курсор",
        "вказівник",
        "темна",
        "світла",
        "акцент",
        "колір",
        "іконк",
        "шрифт",
        "анімації",
        "рух",
    ];

    fn open(&mut self, _c: Ctx, config: &ConfigStore) -> Task<Msg> {
        let preview = self.refresh_preview(config);
        let toolkits = Task::perform(blocking(Support::detect), Msg::Toolkits);
        if self.loaded {
            return Task::batch([preview, toolkits]);
        }
        self.loaded = true;
        Task::batch([
            preview,
            toolkits,
            Task::perform(blocking(|| (apps::icon_themes(), apps::cursor_themes(), apps::font_families())), |assets| {
                let (icons, cursors, fonts) = assets.unwrap_or_default();
                Msg::Assets(icons, cursors, fonts)
            }),
            Task::perform(blocking(look::font_rendering), |rendering| Msg::Rendering(rendering.flatten())),
        ])
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::SetAppearance(appearance) => {
                config.set_appearance(appearance);
                if config.appearance_scheduled() {
                    config.set_bool("topbar", "appearance-schedule", false);
                }
            }
            Msg::Schedule(on) => config.set_bool("topbar", "appearance-schedule", on),
            Msg::SetAccent(accent) => config.set_accent(accent),
            Msg::Animations(enabled) => config.set_bool("animations", "enabled", enabled),
            Msg::Speed(speed) => {
                self.speed = Some(speed);
                return self.settle.schedule(Msg::SettleSpeed);
            }
            Msg::SettleSpeed(generation) if !self.settle.is_current(generation) => {}
            Msg::CommitSpeed | Msg::SettleSpeed(_) => {
                self.settle.cancel();
                if let Some(speed) = self.speed.take() {
                    config.set_float("animations", "speed", multiplier_from_speed(speed));
                }
            }
            Msg::RespectSystem(respect) => config.set_bool("animations", "respect-system", respect),
            Msg::Assets(icons, cursors, fonts) => {
                self.icon_themes = icons;
                self.cursor_themes = cursors;
                self.fonts = fonts;
            }
            Msg::IconTheme(choice) => {
                config.set_optional_str("dock", "icon-theme", choice.value.as_deref());
                return self.refresh_preview(config);
            }
            Msg::CursorTheme(choice) => {
                config.set_optional_str("general", "cursor-theme", choice.value.as_deref());
            }
            Msg::Font(choice) => {
                let name = choice.value.as_deref();
                config.set_optional_str("general", "font-family", name);
                theme::use_ui_family(name);
            }
            Msg::IconPreview(theme, icons) => {
                if self.preview_theme == theme {
                    self.preview = icons;
                }
            }
            Msg::Rendering(rendering) => self.rendering = rendering,
            Msg::Smoothing(mode) => {
                if let Some(rendering) = self.rendering.as_mut() {
                    rendering.smoothing = mode;
                }
                return apply_rendering(move || look::set_smoothing(mode));
            }
            Msg::Hinting(mode) => {
                if let Some(rendering) = self.rendering.as_mut() {
                    rendering.hinting = mode;
                }
                return apply_rendering(move || look::set_hinting(mode));
            }
            Msg::RenderingApplied(result) => self.rendering_error = result.err(),
            Msg::Toolkits(toolkits) => {
                self.toolkits = toolkits.map_or(ToolkitStatus::Unavailable, ToolkitStatus::Ready);
            }
            Msg::ResetFonts => {
                for (table, key) in FONT_KEYS {
                    config.remove(table, key);
                }
                theme::use_ui_family(None);
                let mut tasks = vec![self.refresh_preview(config)];
                if let Some(rendering) = self.rendering.filter(|rendering| *rendering != DEFAULT_RENDERING) {
                    self.rendering = Some(DEFAULT_RENDERING);
                    if rendering.smoothing != DEFAULT_RENDERING.smoothing {
                        tasks.push(apply_rendering(|| look::set_smoothing(DEFAULT_RENDERING.smoothing)));
                    }
                    if rendering.hinting != DEFAULT_RENDERING.hinting {
                        tasks.push(apply_rendering(|| look::set_hinting(DEFAULT_RENDERING.hinting)));
                    }
                }
                return Task::batch(tasks);
            }
            Msg::ToggleSamples => self.disclosure.samples = !self.disclosure.samples,
            Msg::ToggleInfo => self.disclosure.info = !self.disclosure.info,
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![Self::view_before_wallpaper(&c, config), self.view_after_wallpaper(&c, config)]
            .spacing(theme::SECTION_GAP)
            .into()
    }
}

impl Page {
    pub(crate) fn view_before_wallpaper<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let mut page = column![desktop_preview(c), Self::theme_group(c, config),].spacing(theme::SECTION_GAP);
        if config.appearance_scheduled() {
            page = page.push(schedule_group(c));
        }
        page.push(Self::accent_group(c, config)).into()
    }

    pub(crate) fn view_after_wallpaper<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![self.fonts_group(c, config), self.motion_group(c, config)]
            .spacing(theme::SECTION_GAP)
            .push(match self.toolkits {
                ToolkitStatus::Ready(toolkits) => self.toolkits_group(c, toolkits),
                state => Group::new(c.theme)
                    .push(
                        Row::new(c.theme, c.t(tr::Key::OtherApps))
                            .suffix(
                                text(
                                    c.t(if matches!(state, ToolkitStatus::Checking) {
                                        tr::Key::CheckingToolkits
                                    } else {
                                        tr::Key::ToolkitsUnavailable
                                    }),
                                    TextStyle::CAP,
                                )
                                .color(c.theme.fg_2()),
                            )
                            .build(),
                    )
                    .build(),
            })
            .into()
    }
}

fn desktop_preview<'a>(c: &Ctx) -> Element<'a, Msg> {
    use glide_sdk::widget::mock::{at, block, bottom, dock, window};
    use iced::{Background, Border, Color, Gradient, Padding, Shadow, Vector};

    let paint = c.theme.paint;
    let bg = paint.win_bg();
    let (left, right) = (preview_tint(bg, paint.acc_bg()), preview_tint(bg, Accent::Amethyst.sample_color()));
    let width = if c.theme.compact { 300.0 } else { 560.0 };
    let line = |width| block(paint.fill_3(), width, 5.0, glide_sdk::theme::R1);
    let body = column![
        line(width * 0.40),
        line(width * 0.30),
        line(width * 0.35),
        space::vertical(),
        row![space::horizontal(), block(paint.acc_bg(), 50.0, 14.0, glide_sdk::theme::R1)],
    ]
    .spacing(6)
    .padding(10);
    let header = container(space::Space::new().width(Fill).height(16)).style(move |_| iced::widget::container::Style {
        background: Some(paint.bar_bg().into()),
        border: Border {
            radius: iced::border::Radius {
                top_left: glide_sdk::theme::R2,
                top_right: glide_sdk::theme::R2,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });
    let front =
        container(column![header, body].height(Fill)).width(width * 0.66).height(108).clip(true).style(move |_| {
            iced::widget::container::Style {
                background: Some(bg.into()),
                border: Border { color: paint.line_2(), width: 1.0, radius: glide_sdk::theme::R2.into() },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.2),
                    offset: Vector::new(0.0, 6.0),
                    blur_radius: 16.0,
                },
                ..iced::widget::container::Style::default()
            }
        });
    let windows = iced::widget::Stack::with_children([
        at(width * 0.50, 20.0, window(paint, width * 0.42, 80.0)),
        at(0.0, 0.0, front),
    ])
    .width(width)
    .height(128);
    let bar = container(block(paint.acc_bg(), 48.0, 5.0, glide_sdk::theme::R1))
        .width(Fill)
        .height(16)
        .center_x(Fill)
        .center_y(16)
        .style(move |_| iced::widget::container::Style {
            background: Some(paint.bar_bg().into()),
            border: Border {
                radius: iced::border::Radius {
                    top_left: glide_sdk::theme::R2,
                    top_right: glide_sdk::theme::R2,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        });
    let tiles = row([Accent::Azurit, Accent::Amber, Accent::Malachite, Accent::Cinnabar, Accent::Graphite]
        .into_iter()
        .map(|accent| block(accent.sample_color(), 14.0, 14.0, glide_sdk::theme::R1)))
    .spacing(4);
    let layers = iced::widget::Stack::with_children([
        container(space::Space::new().width(Fill).height(220)).into(),
        bar.into(),
        container(windows).center_x(Fill).padding(Padding::ZERO.top(38.0)).into(),
        bottom(8.0, dock(paint, tiles)),
    ]);
    container(layers)
        .width(Fill)
        .height(220)
        .clip(true)
        .style(move |_| iced::widget::container::Style {
            background: Some(Background::Gradient(Gradient::Linear(
                iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2).add_stop(0.0, left).add_stop(1.0, right),
            ))),
            border: Border { color: paint.line_2(), width: 1.0, radius: glide_sdk::theme::R2.into() },
            ..Default::default()
        })
        .into()
}

fn preview_tint(bg: iced::Color, color: iced::Color) -> iced::Color {
    iced::Color {
        r: bg.r * 0.76 + color.r * 0.24,
        g: bg.g * 0.76 + color.g * 0.24,
        b: bg.b * 0.76 + color.b * 0.24,
        a: 1.0,
    }
}

fn theme_sample<'a>(appearance: Appearance, accent: Accent) -> Element<'a, Msg> {
    let paint = ThemeState::new(appearance, accent).paint;
    let left = preview_tint(paint.win_bg(), paint.acc_bg());
    let right = preview_tint(paint.win_bg(), Accent::Amethyst.sample_color());
    let window = container(space::Space::new().width(120).height(32)).style(move |_| iced::widget::container::Style {
        background: Some(paint.card_bg().into()),
        border: iced::Border { radius: glide_sdk::theme::R1.into(), ..Default::default() },
        ..Default::default()
    });
    container(window)
        .center_x(Fill)
        .center_y(SAMPLE_HEIGHT)
        .clip(true)
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Gradient(iced::Gradient::Linear(
                iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2).add_stop(0.0, left).add_stop(1.0, right),
            ))),
            border: iced::Border { radius: glide_sdk::theme::R2.into(), ..Default::default() },
            ..Default::default()
        })
        .into()
}

fn automatic_theme_sample<'a>(accent: Accent) -> Element<'a, Msg> {
    let light = ThemeState::new(Appearance::Light, accent).paint;
    let dark = ThemeState::new(Appearance::Dark, accent).paint;
    let half = |color, width, height, radius, left| {
        container(space::Space::new().width(width).height(height)).style(move |_| iced::widget::container::Style {
            background: Some(color),
            border: iced::Border {
                radius: iced::border::Radius {
                    top_left: if left { radius } else { 0.0 },
                    bottom_left: if left { radius } else { 0.0 },
                    top_right: if left { 0.0 } else { radius },
                    bottom_right: if left { 0.0 } else { radius },
                },
                ..Default::default()
            },
            ..Default::default()
        })
    };
    let background = row![
        half(preview_tint(light.win_bg(), light.acc_bg()).into(), Fill, SAMPLE_HEIGHT, glide_sdk::theme::R2, true),
        half(
            preview_tint(dark.win_bg(), Accent::Amethyst.sample_color()).into(),
            Fill,
            SAMPLE_HEIGHT,
            glide_sdk::theme::R2,
            false
        ),
    ];
    let window = container(row![
        half(light.card_bg().into(), Length::Fixed(60.0), 32.0, glide_sdk::theme::R1, true),
        half(dark.card_bg().into(), Length::Fixed(60.0), 32.0, glide_sdk::theme::R1, false)
    ]);
    container(iced::widget::stack![background, container(window).center_x(Fill).center_y(SAMPLE_HEIGHT)]).into()
}

fn apply_rendering(job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Task<Msg> {
    Task::perform(blocking(job), |result| Msg::RenderingApplied(result.unwrap_or_else(|| Err(String::new()))))
}

impl Page {
    fn refresh_preview(&mut self, config: &ConfigStore) -> Task<Msg> {
        let theme = Some(config.icon_theme());
        if self.preview_requested && self.preview_theme == theme {
            return Task::none();
        }
        self.preview_requested = true;
        self.preview_theme.clone_from(&theme);
        Task::perform(
            blocking(move || {
                let icons = look::icon_preview(theme.as_deref());
                (theme, icons)
            }),
            |result| {
                let (theme, icons) = result.unwrap_or_default();
                Msg::IconPreview(theme, icons)
            },
        )
    }

    fn theme_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let scheduled = config.appearance_scheduled();
        let current = config.appearance();
        let accent = config.accent();
        let t = c.theme;
        let paint = t.paint;
        let option = |preview: Element<'a, Msg>, label: &'static str, selected: bool, message: Msg| {
            let content =
                column![container(preview).center_x(Fill), text(label, TextStyle::CAP).color(paint.fg_1())].spacing(6);
            let width = if t.compact { Length::Fixed(SAMPLE_WIDTH + 16.0) } else { Fill };
            let button = iced::widget::button(content).padding(8).width(width).on_press(message.clone()).style(
                move |_, status| iced::widget::button::Style {
                    background: Some(paint.layered(paint.card_bg(), status).into()),
                    text_color: paint.fg_1(),
                    border: iced::Border {
                        color: if selected { paint.acc_bg() } else { paint.line() },
                        width: if selected { 2.0 } else { 1.0 },
                        radius: glide_sdk::theme::R3.into(),
                    },
                    ..iced::widget::button::Style::default()
                },
            );
            Element::from(
                glide_sdk::widget::focusable(&paint, button, glide_sdk::theme::R3).inset().on_activate(Some(message)),
            )
        };
        let preview = |appearance| theme_sample(appearance, accent);
        let options = row![
            option(
                preview(Appearance::Light),
                c.t(K::Light),
                !scheduled && current == Appearance::Light,
                Msg::SetAppearance(Appearance::Light)
            ),
            option(
                preview(Appearance::Dark),
                c.t(K::Dark),
                !scheduled && current == Appearance::Dark,
                Msg::SetAppearance(Appearance::Dark)
            ),
            option(automatic_theme_sample(accent), c.t(tr::Key::Auto), scheduled, Msg::Schedule(true)),
        ]
        .spacing(10);
        let options: Element<'a, Msg> = if t.compact { options.wrap().into() } else { options.into() };
        let heading = row![
            text(c.t(tr::Key::Theme), TextStyle::HEAD).color(t.fg_1()),
            space::horizontal(),
            text(c.t(tr::Key::ThemeHint), TextStyle::CAP).color(t.fg_2()),
        ]
        .align_y(Alignment::Center);
        column![heading, gated(*c, options)].spacing(8).into()
    }

    fn accent_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let current = config.accent();
        let paint = t.paint;
        let options = row(Accent::ALL.iter().map(|&accent| {
            let name = match c.lang {
                crate::i18n::Lang::Uk => accent.name_uk(),
                crate::i18n::Lang::En => accent.name(),
            };
            let selected = accent == current;
            let dot = widgets::status_dot(accent.sample_color(), 14.0);
            let content = row![dot, text(name, TextStyle::CAP)].spacing(6).align_y(Alignment::Center);
            let button = iced::widget::button(content).padding([5, 8]).on_press(Msg::SetAccent(accent)).style(
                move |_, status| iced::widget::button::Style {
                    background: Some(paint.layered(paint.card_bg(), status).into()),
                    text_color: paint.fg_1(),
                    border: iced::Border {
                        color: if selected { paint.acc_bg() } else { paint.line() },
                        width: if selected { 2.0 } else { 1.0 },
                        radius: glide_sdk::theme::R2.into(),
                    },
                    ..iced::widget::button::Style::default()
                },
            );
            glide_sdk::widget::focusable(&paint, button, glide_sdk::theme::R2)
                .inset()
                .on_activate(Some(Msg::SetAccent(accent)))
                .into()
        }))
        .spacing(6)
        .wrap();
        let heading = row![
            text(c.t(K::AccentColor), TextStyle::HEAD).color(t.fg_1()),
            space::horizontal(),
            text(c.t(K::AccentHint), TextStyle::CAP).color(t.fg_2()),
        ]
        .align_y(Alignment::Center);
        column![heading, gated(*c, options)].spacing(8).into()
    }

    fn fonts_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let icon_theme = config.optional_str("dock", "icon-theme");
        let (options, selected) = pick(choices(c.t(K::SystemDefault), &self.icon_themes), icon_theme.clone());
        let strip: Element<'a, Msg> = if self.preview.is_empty() {
            text(c.t(tr::Key::NoIcons), TextStyle::CAP).color(t.fg_3()).into()
        } else {
            row(self.preview.iter().map(|path| super::apps::app_icon(&t, Some(path), "", 36.0)))
                .spacing(14)
                .align_y(Alignment::Center)
                .into()
        };
        let cursor_theme = config.optional_str("general", "cursor-theme");
        let (cursors, cursor) = pick(choices(c.t(K::SystemDefault), &self.cursor_themes), cursor_theme.clone());
        let family = config.optional_str("general", "font-family");
        let (fonts, font_choice) = pick(choices(c.t(K::SystemDefault), &self.fonts), family.clone());
        let font = family.as_deref().map_or(iced::Font::DEFAULT, |name| iced::Font::with_name(interned(name)));
        let rendering_changed = self.rendering.is_some_and(|rendering| rendering != DEFAULT_RENDERING);
        let changed = [icon_theme.is_some(), family.is_some(), cursor_theme.is_some(), rendering_changed]
            .into_iter()
            .filter(|changed| *changed)
            .count();
        let reset = (changed > 0).then(|| {
            glide_sdk::widget::reset_action(
                &widgets::paint(&t),
                format!("{changed} {}", c.t(tr::Key::Changed)),
                c.t(tr::Key::Reset),
                c.writable.then_some(Msg::ResetFonts),
            )
        });
        let samples = glide_sdk::widget::focusable(
            &t.paint,
            glide_sdk::widget::info_button(&t.paint, self.disclosure.samples, Msg::ToggleSamples),
            theme::R2,
        )
        .on_activate(Some(Msg::ToggleSamples));
        let samples = glide_sdk::widget::tooltip(
            &t.paint,
            samples,
            c.t(if self.disclosure.samples { tr::Key::HideSamples } else { tr::Key::ShowSamples }),
        );
        let actions = row![samples, reset].spacing(8).align_y(Alignment::Center);
        let mut group = Group::new(t)
            .title(c.t(tr::Key::FontsIcons))
            .suffix(actions)
            .push(
                Row::new(t, c.t(K::IconTheme))
                    .subtitle(c.t(K::IconThemeHint))
                    .changed(icon_theme.is_some())
                    .suffix(
                        container(gated(
                            *c,
                            glide_sdk::widget::dropdown_compact(&t.paint, options, selected, Msg::IconTheme, 180.0),
                        ))
                        .width(180),
                    )
                    .build(),
            )
            .push(
                Row::new(t, c.t(K::InterfaceFont))
                    .subtitle(c.t(K::InterfaceFontHint))
                    .changed(family.is_some())
                    .suffix(
                        container(gated(
                            *c,
                            glide_sdk::widget::dropdown_compact(&t.paint, fonts, font_choice, Msg::Font, 180.0),
                        ))
                        .width(180),
                    )
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::CursorTheme))
                    .subtitle(c.t(tr::Key::CursorThemeHint))
                    .changed(cursor_theme.is_some())
                    .suffix(
                        container(gated(
                            *c,
                            glide_sdk::widget::dropdown_compact(&t.paint, cursors, cursor, Msg::CursorTheme, 180.0),
                        ))
                        .width(180),
                    )
                    .build(),
            );
        if let Some(rendering) = self.rendering {
            group = self.rendering_rows(c, group, rendering);
        }
        if self.disclosure.samples {
            group = group.push(widgets::sample_well(t, container(strip).center_x(Fill))).push(widgets::sample_well(
                t,
                column![
                    text("Aa", TextStyle { size_px: 30.0, line_height_px: 36.0, weight: 400, ..TextStyle::T1 })
                        .font(font)
                        .color(t.fg_1()),
                    text(c.t(tr::Key::FontSample), TextStyle::BODY).font(font).color(t.fg_2()),
                ]
                .spacing(4),
            ));
        }
        group.build()
    }

    fn rendering_rows<'a>(&'a self, c: &Ctx, group: Group<'a, Msg>, rendering: FontRendering) -> Group<'a, Msg> {
        let t = c.theme;
        let smoothing = vec![
            (Smoothing::None, c.t(tr::Key::SmoothingNone).to_string()),
            (Smoothing::Grayscale, c.t(tr::Key::Grayscale).to_string()),
            (Smoothing::Subpixel, c.t(tr::Key::Subpixel).to_string()),
        ];
        let hinting = vec![
            (Hinting::None, c.t(tr::Key::HintNone).to_string()),
            (Hinting::Slight, c.t(tr::Key::HintSlight).to_string()),
            (Hinting::Medium, c.t(tr::Key::HintMedium).to_string()),
            (Hinting::Full, c.t(tr::Key::HintFull).to_string()),
        ];
        group
            .push(
                Row::new(t, c.t(tr::Key::Smoothing))
                    .subtitle(c.t(tr::Key::SmoothingHint))
                    .changed(rendering.smoothing != DEFAULT_RENDERING.smoothing)
                    .suffix(widgets::segmented(t, smoothing, rendering.smoothing, Msg::Smoothing))
                    .stacked(t.compact)
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::Hinting))
                    .subtitle(c.t(tr::Key::HintingHint))
                    .changed(rendering.hinting != DEFAULT_RENDERING.hinting)
                    .suffix(widgets::segmented(t, hinting, rendering.hinting, Msg::Hinting))
                    .stacked(t.compact)
                    .build(),
            )
            .extend(
                self.rendering_error
                    .iter()
                    .map(|err| Row::new(t, c.t(tr::Key::RenderingFailed)).subtitle(err.as_str()).build()),
            )
    }

    fn motion_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let enabled = config.animations_enabled();
        let speed = self.speed.unwrap_or_else(|| speed_from_multiplier(config.animation_speed()));
        let animation = Row::new(t, c.t(K::Animations)).suffix(toggle(*c, enabled, Msg::Animations));
        let animation = if enabled { animation } else { animation.subtitle(c.t(tr::Key::Instant)) };
        let mut group = Group::new(t).title(c.t(K::Motion)).push(animation.build());
        if enabled {
            let label = if (speed - 1.0).abs() < 0.01 {
                c.t(tr::Key::SpeedNormal).to_string()
            } else {
                format!("{speed:.2}×")
            };
            let slider = glide_sdk::widget::range_slider(
                &widgets::paint(&t),
                c.t(tr::Key::Slower),
                widgets::value_slider(t, SPEED_MIN..=SPEED_MAX, speed, 0.05, Msg::Speed, Some(Msg::CommitSpeed)),
                c.t(tr::Key::Faster),
            );
            group = group.push(
                Row::new(t, c.t(K::AnimationSpeed))
                    .subtitle(label)
                    .suffix(gated(*c, widgets::flexible(slider, 300.0)))
                    .build(),
            );
        }
        group
            .push(switch_row(
                *c,
                c.t(K::RespectSystemMotion),
                c.t(K::RespectSystemMotionHint),
                config.respect_system_motion(),
                Msg::RespectSystem,
            ))
            .build()
    }
}

type ToolkitRow = (tr::Key, tr::Key, theme::Tone, tr::Key);

fn toolkit_rows(toolkits: Support) -> [ToolkitRow; 4] {
    use theme::Tone;
    use tr::Key;
    let gtk3 = if toolkits.adw_gtk3 {
        (Key::Gtk3, Key::Gtk3Ready, Tone::Ok, Key::Ready)
    } else {
        (Key::Gtk3, Key::Gtk3Missing, Tone::Warn, Key::Missing)
    };
    let qt = match toolkits.qt {
        Some(QtTheme::Gtk3) => (Key::Qt, Key::QtBridge, Tone::Neutral, Key::Partly),
        Some(_) => (Key::Qt, Key::QtReady, Tone::Ok, Key::Ready),
        None => (Key::Qt, Key::QtMissing, Tone::Warn, Key::Missing),
    };
    let kvantum = if toolkits.kvantum {
        (Key::Kvantum, Key::KvantumReady, Tone::Ok, Key::Ready)
    } else {
        (Key::Kvantum, Key::KvantumMissing, Tone::Neutral, Key::Optional)
    };
    [(Key::Gtk4, Key::Gtk4Hint, Tone::Ok, Key::Ready), gtk3, qt, kvantum]
}

fn needs_relogin(detected: Option<&str>, session: Option<&str>) -> bool {
    let chosen_by_glide =
        |value: &str| [QtTheme::Qt6ct, QtTheme::Qt5ct, QtTheme::Gtk3].map(QtTheme::plugin).contains(&value);
    detected.is_some() && detected != session && session.is_none_or(chosen_by_glide)
}

fn tone_color(t: &ThemeState, tone: theme::Tone) -> iced::Color {
    match tone {
        theme::Tone::Ok => t.ok(),
        theme::Tone::Warn | theme::Tone::Danger => t.warn(),
        _ => t.fg_4(),
    }
}

fn short_name(label: tr::Key) -> &'static str {
    match label {
        tr::Key::Gtk4 => "GTK 4",
        tr::Key::Gtk3 => "GTK 3",
        tr::Key::Qt => "Qt",
        _ => "Kvantum",
    }
}

impl Page {
    fn toolkits_group<'a>(&'a self, c: &Ctx, toolkits: Support) -> Element<'a, Msg> {
        let t = c.theme;
        let paint = widgets::paint(&t);
        let rows = toolkit_rows(toolkits);
        let chips = rows.iter().map(|(label, _, tone, _)| (tone_color(&t, *tone), short_name(*label).to_string()));
        let summary = Row::new(t, c.t(tr::Key::OtherApps))
            .suffix(
                row![
                    glide_sdk::widget::status_chips(&paint, chips.collect()),
                    glide_sdk::widget::info_button(&paint, self.disclosure.info, Msg::ToggleInfo),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .stacked(t.compact)
            .build();
        let session = std::env::var("QT_QPA_PLATFORMTHEME").ok();
        let relogin = needs_relogin(toolkits.qt_platform_theme(), session.as_deref())
            .then(|| Row::new(t, c.t(tr::Key::Relogin)).build());
        let mut group = Group::new(t).push(summary);
        if self.disclosure.info {
            group = group
                .push(Row::new(t, c.t(tr::Key::Intro)).subtitle(c.t(tr::Key::OtherAppsHint)).dimmed(false).build())
                .extend(rows.map(|(label, hint, tone, status)| {
                    Row::new(t, c.t(label)).subtitle(c.t(hint)).suffix(widgets::pill(t, tone, c.t(status))).build()
                }));
        }
        group.extend(relogin).build()
    }
}

fn schedule_group<'a>(c: &Ctx) -> Element<'a, Msg> {
    let t = c.theme;
    Group::new(t)
        .title(c.t(tr::Key::Schedule))
        .description(c.t(tr::Key::ScheduleHint))
        .push(
            Row::new(t, c.t(tr::Key::DarkHours))
                .subtitle(c.t(tr::Key::FixedHours))
                .suffix(text("19:00 – 07:00", TextStyle::CALL).color(t.fg_2()))
                .build(),
        )
        .build()
}

fn pick(options: Vec<Choice>, current: Option<String>) -> (Vec<Choice>, Option<Choice>) {
    let selected = options
        .iter()
        .find(|choice| choice.value == current)
        .cloned()
        .or_else(|| current.map(|value| Choice { label: value.clone(), value: Some(value) }))
        .or_else(|| options.first().cloned());
    (options, selected)
}

static FONT_NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn interned(name: &str) -> &'static str {
    let Ok(mut names) = FONT_NAMES.lock() else {
        return "sans-serif";
    };
    if let Some(known) = names.iter().find(|known| **known == name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.push(leaked);
    leaked
}

const SAMPLE_WIDTH: f32 = 196.0;
const SAMPLE_HEIGHT: f32 = 56.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn statuses(toolkits: Support) -> Vec<tr::Key> {
        toolkit_rows(toolkits).iter().map(|row| row.3).collect()
    }

    #[test]
    fn missing_toolkit_packages_are_named_and_the_gtk3_bridge_counts_as_partial() {
        use tr::Key::{Missing, Optional, Partly, Ready};
        assert_eq!(statuses(Support::default()), [Ready, Missing, Missing, Optional]);
        let bridge = Support { qt: Some(QtTheme::Gtk3), ..Support::default() };
        assert_eq!(statuses(bridge), [Ready, Missing, Partly, Optional]);
        let full = Support { adw_gtk3: true, kvantum: true, qt: Some(QtTheme::Qt6ct) };
        assert_eq!(statuses(full), [Ready; 4]);
    }

    #[test]
    fn a_relogin_is_asked_only_when_the_session_lacks_what_glide_would_export() {
        assert!(needs_relogin(Some("qt6ct"), None));
        assert!(needs_relogin(Some("qt6ct"), Some("gtk3")), "qt6ct installed after the gtk3 bridge");
        assert!(!needs_relogin(Some("qt6ct"), Some("qt6ct")));
        assert!(!needs_relogin(Some("qt6ct"), Some("kde")), "a value the user chose stays");
        assert!(!needs_relogin(None, None));
    }
}
