use iced::widget::{Column, container, row, space, text};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::{choice, gated, switch_row};
use super::desktop::sketch;
use super::{Ctx, Pane};
use crate::i18n::Key as K;
use crate::icons::Icon;
use crate::services::config::{ConfigStore, DEFAULT_BORDER_WIDTH, DEFAULT_TITLEBAR_HEIGHT};
use crate::theme::{self, ThemeState};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Preview => "Preview", "Попередній перегляд";
        SampleTitle => "Documents", "Документи";
        Buttons => "Buttons", "Кнопки";
        ButtonsHint => "Where the close button sits", "Де розташована кнопка закриття";
        CloseRight => "Close on the right", "Закриття праворуч";
        CloseRightHint => "Maximize on the left", "Розгортання ліворуч";
        CloseLeft => "Close on the left", "Закриття ліворуч";
        CloseLeftHint => "Maximize on the right, as on macOS", "Розгортання праворуч, як у macOS";
        DoubleClick => "Double-click the titlebar", "Подвійне клацання на заголовку";
        DoubleClickHint => "What happens to the window", "Що стається з вікном";
        ActionMaximize => "Maximize", "Розгорнути";
        ActionMinimize => "Minimize", "Згорнути";
        ActionNothing => "Nothing", "Нічого";
        Behaviour => "Titlebar actions", "Дії заголовка";
        BehaviourHint => "Right-click a titlebar for the window menu", "Клацніть заголовок правою кнопкою, щоб відкрити меню вікна";
    }
}

pub const COMPACT_HEIGHT: i64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleAlign {
    Left,
    Center,
}

impl TitleAlign {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
        }
    }

    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_str("decorations", "title-align") {
            Some("center") => Self::Center,
            _ => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Compact,
    Standard,
}

impl Density {
    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        if config.int_or("decorations", "height", DEFAULT_TITLEBAR_HEIGHT) < DEFAULT_TITLEBAR_HEIGHT {
            Self::Compact
        } else {
            Self::Standard
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonLayout {
    CloseRight,
    CloseLeft,
}

impl ButtonLayout {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::CloseRight => "maximize-close",
            Self::CloseLeft => "close-maximize",
        }
    }

    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_str("decorations", "button-layout") {
            Some("close-maximize") => Self::CloseLeft,
            _ => Self::CloseRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleClick {
    Maximize,
    Minimize,
    Nothing,
}

impl DoubleClick {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Maximize => "maximize",
            Self::Minimize => "minimize",
            Self::Nothing => "none",
        }
    }

    #[must_use]
    pub fn from_config(config: &ConfigStore) -> Self {
        match config.get_str("decorations", "double-click") {
            Some("minimize") => Self::Minimize,
            Some("none") => Self::Nothing,
            _ => Self::Maximize,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Msg {
    Align(TitleAlign),
    Density(Density),
    Shadow(bool),
    Border(bool),
    Buttons(ButtonLayout),
    DoubleClick(DoubleClick),
}

#[derive(Debug, Default)]
pub struct Page;

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "window",
        "titlebar",
        "title",
        "shadow",
        "border",
        "decoration",
        "button",
        "double-click",
        "вікн",
        "заголов",
        "тін",
        "рамк",
        "кнопк",
        "подвійн",
    ];

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Align(align) => config.set_str("decorations", "title-align", align.key()),
            Msg::Density(density) => config.set_int(
                "decorations",
                "height",
                match density {
                    Density::Compact => COMPACT_HEIGHT,
                    Density::Standard => DEFAULT_TITLEBAR_HEIGHT,
                },
            ),
            Msg::Shadow(on) => config.set_bool("decorations", "shadow", on),
            Msg::Border(false) => config.set_int("decorations", "border-width", 0),
            Msg::Border(true) => {
                if config.int_or("decorations", "border-width", DEFAULT_BORDER_WIDTH) <= 0 {
                    config.remove("decorations", "border-width");
                }
            }
            Msg::Buttons(layout) => config.set_str("decorations", "button-layout", layout.key()),
            Msg::DoubleClick(action) => {
                config.set_str("decorations", "double-click", action.key());
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let mock = Mock::from_config(config);
        Column::new()
            .spacing(theme::SECTION_GAP)
            .push(preview_group(&c, mock))
            .push(titlebar_group(&c, mock))
            .push(buttons_group(&c, mock))
            .push(actions_group(&c, config))
            .push(frame_group(&c, mock))
            .into()
    }
}

#[derive(Debug, Clone, Copy)]
struct Mock {
    align: TitleAlign,
    density: Density,
    layout: ButtonLayout,
    shadow: bool,
    border: bool,
    width: f32,
    height: f32,
}

impl Mock {
    fn from_config(config: &ConfigStore) -> Self {
        Self {
            align: TitleAlign::from_config(config),
            density: Density::from_config(config),
            layout: ButtonLayout::from_config(config),
            shadow: config.bool_or("decorations", "shadow", true),
            border: config.int_or("decorations", "border-width", DEFAULT_BORDER_WIDTH) > 0,
            width: 300.0,
            height: 124.0,
        }
    }
}

fn preview_group<'a>(c: &Ctx, mock: Mock) -> Element<'a, Msg> {
    let t = c.theme;
    Group::new(t)
        .title(c.t(tr::Key::Preview))
        .description(c.t(K::TitlebarsHint))
        .push(
            container(sketch::screen(
                t,
                Length::Fill,
                170.0,
                vec![sketch::centered(window_mock(&t, c.t(tr::Key::SampleTitle), mock))],
            ))
            .padding(8),
        )
        .build()
}

fn titlebar_group<'a>(c: &Ctx, mock: Mock) -> Element<'a, Msg> {
    let t = c.theme;
    let c = *c;
    Group::new(t)
        .title(c.t(K::Titlebars))
        .push(
            Row::new(t, c.t(K::TitleAlignment))
                .suffix(choice(
                    c,
                    [(TitleAlign::Left, c.t(K::AlignLeft)), (TitleAlign::Center, c.t(K::AlignCenter))],
                    mock.align,
                    Msg::Align,
                ))
                .build(),
        )
        .push(
            Row::new(t, c.t(K::TitlebarSize))
                .suffix(choice(
                    c,
                    [(Density::Compact, c.t(K::Compact)), (Density::Standard, c.t(K::Standard))],
                    mock.density,
                    Msg::Density,
                ))
                .build(),
        )
        .build()
}

fn buttons_group<'a>(c: &Ctx, mock: Mock) -> Element<'a, Msg> {
    let t = c.theme;
    let card = |option: ButtonLayout, heading: tr::Key, hint: tr::Key| {
        widgets::radio_card(
            t,
            window_mock(
                &t,
                "",
                Mock {
                    align: TitleAlign::Center,
                    density: Density::Standard,
                    layout: option,
                    shadow: false,
                    border: true,
                    width: 150.0,
                    height: 58.0,
                },
            ),
            c.t(heading),
            Some(c.t(hint).to_string()),
            mock.layout == option,
            Msg::Buttons(option),
        )
    };
    Group::new(t)
        .title(c.t(tr::Key::Buttons))
        .description(c.t(tr::Key::ButtonsHint))
        .push(
            container(gated(
                *c,
                row![
                    card(ButtonLayout::CloseRight, tr::Key::CloseRight, tr::Key::CloseRightHint),
                    card(ButtonLayout::CloseLeft, tr::Key::CloseLeft, tr::Key::CloseLeftHint),
                ]
                .spacing(12),
            ))
            .padding(8),
        )
        .build()
}

fn actions_group<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    Group::new(t)
        .title(c.t(tr::Key::Behaviour))
        .description(c.t(tr::Key::BehaviourHint))
        .push(
            Row::new(t, c.t(tr::Key::DoubleClick))
                .subtitle(c.t(tr::Key::DoubleClickHint))
                .suffix(choice(
                    *c,
                    [
                        (DoubleClick::Maximize, c.t(tr::Key::ActionMaximize)),
                        (DoubleClick::Minimize, c.t(tr::Key::ActionMinimize)),
                        (DoubleClick::Nothing, c.t(tr::Key::ActionNothing)),
                    ],
                    DoubleClick::from_config(config),
                    Msg::DoubleClick,
                ))
                .build(),
        )
        .build()
}

fn frame_group<'a>(c: &Ctx, mock: Mock) -> Element<'a, Msg> {
    Group::new(c.theme)
        .title(c.t(K::WindowFrame))
        .push(switch_row(*c, c.t(K::Shadows), c.t(K::ShadowsHint), mock.shadow, Msg::Shadow))
        .push(switch_row(*c, c.t(K::Border), c.t(K::BorderHint), mock.border, Msg::Border))
        .build()
}

fn round_button<'a>(t: &ThemeState, glyph: Icon, size: f32) -> Element<'a, Msg> {
    let fill = t.fill_2();
    container(widgets::icon(glyph, (size * 0.55).round(), t.fg_2()))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_: &iced::Theme| theme::solid(fill, theme::radius_round()))
        .into()
}

fn window_mock<'a>(t: &ThemeState, title: &'a str, mock: Mock) -> Element<'a, Msg> {
    let Mock { align, density, layout, shadow, border, width, height } = mock;
    let bar_px = match density {
        Density::Compact => COMPACT_HEIGHT,
        Density::Standard => DEFAULT_TITLEBAR_HEIGHT,
    };
    let bar_px = i32::try_from(bar_px).unwrap_or(0);
    let bar_height = bar_px as f32;
    let button = glide_theme::component::titlebar_button(bar_px) as f32;
    let (leading, trailing) = match layout {
        ButtonLayout::CloseRight => (Icon::Maximize, Icon::Close),
        ButtonLayout::CloseLeft => (Icon::Close, Icon::Maximize),
    };
    let label = text(title).size(12).font(theme::SEMIBOLD).color(t.fg_1());
    let title: Element<'a, Msg> = match align {
        TitleAlign::Left => container(label).width(Fill).into(),
        TitleAlign::Center => container(label).center_x(Fill).into(),
    };
    let bar_bg = t.bar_bg();
    let bar = container(
        row![round_button(t, leading, button), title, round_button(t, trailing, button),]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .padding([0, 5])
    .width(Fill)
    .height(bar_height)
    .center_y(bar_height)
    .style(move |_: &iced::Theme| theme::solid(bar_bg, iced::border::Radius::default()));
    let body = t.win_bg();
    let ring = t.line();
    container(iced::widget::column![bar, space().height(Fill)].width(Fill))
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(body)),
            border: iced::Border { color: ring, width: if border { 1.0 } else { 0.0 }, radius: theme::radius_r3() },
            shadow: if shadow {
                iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                    offset: iced::Vector::new(0.0, 6.0),
                    blur_radius: 18.0,
                }
            } else {
                iced::Shadow::default()
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}
