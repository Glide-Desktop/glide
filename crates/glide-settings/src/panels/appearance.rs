use std::path::PathBuf;
use std::sync::Mutex;

use iced::widget::{column, container, row, space, text};
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
        IconPreviewHint => "A sample of the selected theme", "Зразок вибраної теми";
        NoIcons => "This theme has none of the sample icons", "У цій темі немає зразкових іконок";
        FontSample => "The quick brown fox jumps over the lazy dog · Швидкий бурий лис", "Швидкий бурий лис перестрибує через лінивого пса · The quick brown fox";
        TextRendering => "Text rendering in apps", "Відображення тексту в програмах";
        TextRenderingHint => "How GTK and GNOME apps draw text on this screen", "Як програми GTK і GNOME малюють текст на цьому екрані";
        Smoothing => "Smoothing", "Згладжування";
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
        CursorThemeHint => "Cursor theme for Glide and apps; X11 apps pick it up when restarted", "Тема курсора для Glide і програм; програми X11 підхоплять її після перезапуску";
        Slower => "Slower", "Повільніше";
        Faster => "Faster", "Швидше";
        Instant => "Everything appears instantly", "Усе з'являється миттєво";
        SpeedNormal => "Standard speed", "Стандартна швидкість";
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    SetAppearance(Appearance),
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
}

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
        if self.loaded {
            return preview;
        }
        self.loaded = true;
        Task::batch([
            preview,
            Task::perform(blocking(|| (apps::icon_themes(), apps::cursor_themes(), apps::font_families())), |assets| {
                let (icons, cursors, fonts) = assets.unwrap_or_default();
                Msg::Assets(icons, cursors, fonts)
            }),
            Task::perform(blocking(look::font_rendering), |rendering| Msg::Rendering(rendering.flatten())),
        ])
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::SetAppearance(appearance) => config.set_appearance(appearance),
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
                config.set_optional_str("general", "font-family", choice.value.as_deref());
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
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        column![
            Self::style_group(&c, config),
            Self::accent_group(&c, config),
            self.icons_group(&c, config),
            self.font_group(&c, config),
            self.rendering.map(|rendering| self.rendering_group(&c, rendering)),
            self.motion_group(&c, config),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

fn apply_rendering(job: impl FnOnce() -> Result<String, String> + Send + 'static) -> Task<Msg> {
    Task::perform(blocking(job), |result| Msg::RenderingApplied(result.unwrap_or_else(|| Err(String::new()))))
}

impl Page {
    fn refresh_preview(&mut self, config: &ConfigStore) -> Task<Msg> {
        let theme = config.optional_str("dock", "icon-theme");
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

    fn style_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let current = config.appearance();
        let accent = config.accent();
        let t = c.theme;
        let option = |appearance: Appearance, label: &'static str| -> Element<'a, Msg> {
            widgets::preview_card(
                t,
                mockup(&ThemeState::new(appearance, accent)),
                label,
                current == appearance,
                Some(Msg::SetAppearance(appearance)),
            )
        };
        let options =
            row![option(Appearance::Light, c.t(K::Light)), option(Appearance::Dark, c.t(K::Dark)),].spacing(24);
        Group::new(t)
            .title(c.t(K::Style))
            .description(c.t(K::StyleHint))
            .push(container(gated(*c, options)).padding([16, 12]).center_x(Fill))
            .build()
    }

    fn accent_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let current = config.accent();
        let swatches = row(Accent::ALL.iter().map(|&accent| {
            widgets::colour_swatch(
                t,
                accent.sample_color(),
                accent == current,
                Some(Msg::SetAccent(accent)),
                SWATCH_SIZE,
            )
        }))
        .spacing(2)
        .align_y(Alignment::Center);
        let name = match c.lang {
            crate::i18n::Lang::Uk => current.name_uk(),
            crate::i18n::Lang::En => current.name(),
        };
        let sample = row![
            widgets::primary(t, name, None::<Msg>),
            widgets::switch_disabled(t, true),
            widgets::pill(t, theme::Tone::Accent, current.primary_hex()),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        Group::new(t)
            .title(c.t(K::AccentColor))
            .description(c.t(K::AccentHint))
            .push(container(gated(*c, swatches.wrap())).padding([10.0, theme::ROW_PADDING_X]))
            .push(container(sample).padding([8.0, theme::ROW_PADDING_X]))
            .build()
    }

    fn icons_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let (options, selected) =
            pick(choices(c.t(K::SystemDefault), &self.icon_themes), config.optional_str("dock", "icon-theme"));
        let strip: Element<'a, Msg> = if self.preview.is_empty() {
            text(c.t(tr::Key::NoIcons)).size(12.5).color(t.fg_3()).into()
        } else {
            row(self.preview.iter().map(|path| super::apps::app_icon(&t, Some(path), "", 36.0)))
                .spacing(14)
                .align_y(Alignment::Center)
                .into()
        };
        let well = t.sunken_bg();
        let (cursors, cursor) =
            pick(choices(c.t(K::SystemDefault), &self.cursor_themes), config.optional_str("general", "cursor-theme"));
        Group::new(t)
            .title(c.t(K::IconTheme))
            .description(c.t(K::IconThemeHint))
            .push(
                Row::new(t, c.t(K::IconTheme))
                    .subtitle(c.t(tr::Key::IconPreviewHint))
                    .suffix(gated(*c, widgets::dropdown(t, options, selected, Msg::IconTheme, 240.0)))
                    .build(),
            )
            .push(
                container(
                    container(strip)
                        .padding([14, 16])
                        .center_x(Fill)
                        .style(move |_: &iced::Theme| theme::solid(well, theme::radius_r2())),
                )
                .padding([6.0, theme::ROW_PADDING_X]),
            )
            .push(
                Row::new(t, c.t(tr::Key::CursorTheme))
                    .subtitle(c.t(tr::Key::CursorThemeHint))
                    .suffix(gated(*c, widgets::dropdown(t, cursors, cursor, Msg::CursorTheme, 240.0)))
                    .build(),
            )
            .build()
    }

    fn font_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let current = config.optional_str("general", "font-family");
        let (options, selected) = pick(choices(c.t(K::SystemDefault), &self.fonts), current.clone());
        let font = current.as_deref().map_or(iced::Font::DEFAULT, |name| iced::Font::with_name(interned(name)));
        let well = t.sunken_bg();
        Group::new(t)
            .title(c.t(K::InterfaceFont))
            .description(c.t(K::InterfaceFontHint))
            .push(
                Row::new(t, c.t(K::InterfaceFont))
                    .suffix(gated(*c, widgets::dropdown(t, options, selected, Msg::Font, 240.0)))
                    .build(),
            )
            .push(
                container(
                    container(
                        column![
                            text("Aa").size(30).font(font).color(t.fg_1()),
                            text(c.t(tr::Key::FontSample)).size(14).font(font).color(t.fg_2()),
                        ]
                        .spacing(4),
                    )
                    .padding([12, 16])
                    .width(Fill)
                    .style(move |_: &iced::Theme| theme::solid(well, theme::radius_r2())),
                )
                .padding([6.0, theme::ROW_PADDING_X]),
            )
            .build()
    }

    fn rendering_group<'a>(&'a self, c: &Ctx, rendering: FontRendering) -> Element<'a, Msg> {
        let t = c.theme;
        let smoothing = vec![
            (Smoothing::Grayscale, c.t(tr::Key::Grayscale).to_string()),
            (Smoothing::Subpixel, c.t(tr::Key::Subpixel).to_string()),
            (Smoothing::None, c.t(tr::Key::SmoothingNone).to_string()),
        ];
        let hinting = vec![
            (Hinting::None, c.t(tr::Key::HintNone).to_string()),
            (Hinting::Slight, c.t(tr::Key::HintSlight).to_string()),
            (Hinting::Medium, c.t(tr::Key::HintMedium).to_string()),
            (Hinting::Full, c.t(tr::Key::HintFull).to_string()),
        ];
        let group = Group::new(t)
            .title(c.t(tr::Key::TextRendering))
            .description(c.t(tr::Key::TextRenderingHint))
            .push(
                Row::new(t, c.t(tr::Key::Smoothing))
                    .subtitle(c.t(tr::Key::SmoothingHint))
                    .suffix(widgets::segmented(t, smoothing, rendering.smoothing, Msg::Smoothing))
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::Hinting))
                    .subtitle(c.t(tr::Key::HintingHint))
                    .suffix(widgets::segmented(t, hinting, rendering.hinting, Msg::Hinting))
                    .build(),
            );
        group
            .extend(
                self.rendering_error
                    .iter()
                    .map(|err| Row::new(t, c.t(tr::Key::RenderingFailed)).subtitle(err.as_str()).build()),
            )
            .build()
    }

    fn motion_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let enabled = config.animations_enabled();
        let speed = self.speed.unwrap_or_else(|| speed_from_multiplier(config.animation_speed()));
        let mut group = Group::new(t).title(c.t(K::Motion)).push(
            Row::new(t, c.t(K::Animations))
                .subtitle(if enabled { c.t(K::AnimationsHint) } else { c.t(tr::Key::Instant) })
                .suffix(toggle(*c, enabled, Msg::Animations))
                .build(),
        );
        if enabled {
            let label = if (speed - 1.0).abs() < 0.01 {
                c.t(tr::Key::SpeedNormal).to_string()
            } else {
                format!("{speed:.2}×")
            };
            let slider = row![
                text(c.t(tr::Key::Slower)).size(12).color(t.fg_3()),
                widgets::value_slider(t, SPEED_MIN..=SPEED_MAX, speed, 0.05, Msg::Speed, Some(Msg::CommitSpeed),),
                text(c.t(tr::Key::Faster)).size(12).color(t.fg_3()),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
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

pub const SWATCH_SIZE: f32 = 24.0;

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

fn bar<'a>(color: iced::Color, width: f32, height: f32) -> Element<'a, Msg> {
    container(space())
        .width(width)
        .height(height)
        .style(move |_: &iced::Theme| theme::solid(color, theme::radius_r1()))
        .into()
}

fn mockup<'a>(t: &ThemeState) -> Element<'a, Msg> {
    let t = *t;
    let sidebar = container(
        column![
            bar(t.acc_soft(), 34.0, 7.0),
            bar(t.fill_3(), 26.0, 5.0),
            bar(t.fill_3(), 30.0, 5.0),
            bar(t.fill_3(), 22.0, 5.0),
        ]
        .spacing(6),
    )
    .padding(6)
    .height(Fill)
    .style(move |_: &iced::Theme| theme::solid(t.sidebar_bg(), theme::radius_r1()));
    let card = container(
        column![
            bar(t.fg_3(), 56.0, 5.0),
            bar(t.fill_3(), 80.0, 5.0),
            bar(t.fill_3(), 64.0, 5.0),
            space().height(Fill),
            row![space().width(Fill), bar(t.acc_bg(), 30.0, 10.0)],
        ]
        .spacing(6),
    )
    .padding(8)
    .width(Fill)
    .height(Fill)
    .style(move |_: &iced::Theme| theme::solid(t.card_bg(), theme::radius_r1()));
    let window =
        container(row![sidebar, card].spacing(6)).padding(6).width(Fill).height(Fill).style(move |_: &iced::Theme| {
            iced::widget::container::Style {
                background: Some(iced::Background::Color(t.win_bg())),
                border: iced::Border { color: t.line_2(), width: 1.0, radius: theme::radius_r2() },
                ..iced::widget::container::Style::default()
            }
        });
    container(window)
        .padding(12)
        .width(Length::Fixed(196.0))
        .height(Length::Fixed(124.0))
        .style(move |_: &iced::Theme| theme::solid(t.sunken_bg(), theme::radius_r2()))
        .into()
}
