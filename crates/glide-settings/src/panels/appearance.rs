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
        Schedule => "Switch automatically", "Перемикати автоматично";
        ScheduleHint => "Light from 7:00, dark from 19:00", "Світла з 7:00, темна з 19:00";
        CursorThemeHint => "Cursor theme for Glide and apps; X11 apps pick it up when restarted", "Тема курсора для Glide і програм; програми X11 підхоплять її після перезапуску";
        Slower => "Slower", "Повільніше";
        Faster => "Faster", "Швидше";
        Instant => "Everything appears instantly", "Усе з'являється миттєво";
        SpeedNormal => "Standard speed", "Стандартна швидкість";
        OtherApps => "Other apps", "Інші програми";
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
    toolkits: Option<Support>,
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
            Msg::Toolkits(toolkits) => self.toolkits = toolkits,
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
            self.toolkits.map(|toolkits| toolkits_group(&c, toolkits)),
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
        let current = c.theme.appearance();
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
            .push(switch_row(
                *c,
                c.t(tr::Key::Schedule),
                c.t(tr::Key::ScheduleHint),
                config.appearance_scheduled(),
                Msg::Schedule,
            ))
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
            widgets::primary(t, name, Some(Msg::SetAccent(current))),
            widgets::switch(t, true, move |_| Msg::SetAccent(current)),
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
            text(c.t(tr::Key::NoIcons), TextStyle::CAP).color(t.fg_3()).into()
        } else {
            row(self.preview.iter().map(|path| super::apps::app_icon(&t, Some(path), "", 36.0)))
                .spacing(14)
                .align_y(Alignment::Center)
                .into()
        };
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
            .push(widgets::sample_well(t, container(strip).center_x(Fill)))
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
        Group::new(t)
            .title(c.t(K::InterfaceFont))
            .description(c.t(K::InterfaceFontHint))
            .push(
                Row::new(t, c.t(K::InterfaceFont))
                    .suffix(gated(*c, widgets::dropdown(t, options, selected, Msg::Font, 240.0)))
                    .build(),
            )
            .push(widgets::sample_well(
                t,
                column![
                    text("Aa", TextStyle { size_px: 30.0, line_height_px: 36.0, weight: 400, ..TextStyle::T1 })
                        .font(font)
                        .color(t.fg_1()),
                    text(c.t(tr::Key::FontSample), TextStyle::BODY).font(font).color(t.fg_2()),
                ]
                .spacing(4),
            ))
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
                    .stacked(t.compact)
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::Hinting))
                    .subtitle(c.t(tr::Key::HintingHint))
                    .suffix(widgets::segmented(t, hinting, rendering.hinting, Msg::Hinting))
                    .stacked(t.compact)
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

pub const SWATCH_SIZE: f32 = 24.0;

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

fn toolkits_group<'a>(c: &Ctx, toolkits: Support) -> Element<'a, Msg> {
    let t = c.theme;
    let rows = toolkit_rows(toolkits).map(|(label, hint, tone, status)| {
        Row::new(t, c.t(label)).subtitle(c.t(hint)).suffix(widgets::pill(t, tone, c.t(status))).build()
    });
    let session = std::env::var("QT_QPA_PLATFORMTHEME").ok();
    let relogin = needs_relogin(toolkits.qt_platform_theme(), session.as_deref())
        .then(|| Row::new(t, c.t(tr::Key::Relogin)).build());
    Group::new(t)
        .title(c.t(tr::Key::OtherApps))
        .description(c.t(tr::Key::OtherAppsHint))
        .extend(rows)
        .extend(relogin)
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

fn mockup<'a>(t: &ThemeState) -> Element<'a, Msg> {
    use glide_sdk::widget::mock::{block, outlined_surface, surface};
    let t = *t;
    let bar = |color, width: f32, height: f32| block(color, width, height, theme::R1);
    let sidebar = surface(
        t.sidebar_bg(),
        theme::R1,
        column![
            bar(t.acc_soft(), 34.0, 7.0),
            bar(t.fill_3(), 26.0, 5.0),
            bar(t.fill_3(), 30.0, 5.0),
            bar(t.fill_3(), 22.0, 5.0),
        ]
        .spacing(6),
    )
    .padding(6)
    .height(Fill);
    let card = surface(
        t.card_bg(),
        theme::R1,
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
    .height(Fill);
    let window = outlined_surface(t.win_bg(), t.line_2(), 1.0, theme::R2, row![sidebar, card].spacing(6))
        .padding(6)
        .width(Fill)
        .height(Fill);
    surface(t.sunken_bg(), theme::R2, window)
        .padding(12)
        .width(Length::Fixed(196.0))
        .height(Length::Fixed(124.0))
        .into()
}

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
