use std::cell::RefCell;
use std::collections::HashMap;

use glide_iced::glyph::Icon;
use glide_iced::style::{self, Paint, R2, R3, SP1, SP2, SP3, SP4, SP5, SP6, Tone, to_iced};
use glide_iced::text::text;
use glide_iced::widgets;
use glide_theme::elevation::Level;
use glide_theme::motion::{ST_HOVER, ST_PRESS};
use glide_theme::{TextStyle, component};
use iced::border::Radius;
use iced::widget::text::Wrapping;
use iced::widget::text_editor::{self as editor_widget};
use iced::widget::{
    Space, button, column, container, markdown, mouse_area, opaque, pin, row, scrollable, stack, text_editor,
    text_input, tooltip,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, mouse, window};

use crate::app::{
    Answer, App, Bar, Command, Dialog, DialogKind, GOTO_ID, Item, Menu, MenuKind, Message, REPLACE_ID, Toggle,
    shortcut_label,
};
use crate::app::{EDITOR, FIND_ID};
use crate::document::{Document, Ending};
use crate::edit::Case;
use crate::i18n::Key;
use crate::syntax::{Bridge, Highlight, Language};

pub const HEADER: f32 = 38.0;
pub const STATUS: f32 = 26.0;
pub const SIDEBAR: f32 = 248.0;
pub const SIDEBAR_MIN_WINDOW: f32 = 560.0;
const TABS: f32 = 36.0;
const TAB_PAD: f32 = SP1;
const TAB_GAP: f32 = SP1;
const CONTROL: f32 = component::SMALL_CONTROL;
const ITEM: f32 = component::MENU_ITEM;
const ROW: f32 = 28.0;
const MENU_WIDTH: f32 = 300.0;
const FIND_FIELD: f32 = 260.0;

fn view_bg(paint: &Paint) -> Color {
    to_iced(paint.tokens.view_bg).scale_alpha(paint.alpha)
}

fn sidebar_bg(paint: &Paint) -> Color {
    to_iced(paint.tokens.sidebar_bg).scale_alpha(paint.alpha)
}

fn measure(content: &str, style: TextStyle) -> f32 {
    use iced::advanced::graphics::text::{self as graphics, cosmic_text};
    thread_local! {
        static CACHE: RefCell<HashMap<(String, u32, u64), f32>> = RefCell::new(HashMap::new());
    }
    let key = (content.to_string(), style.size_px.to_bits() ^ u32::from(style.weight), glide_iced::font::revision());
    if let Some(width) = CACHE.with_borrow(|cache| cache.get(&key).copied()) {
        return width;
    }
    let mut system = graphics::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut buffer =
        cosmic_text::Buffer::new(system.raw(), cosmic_text::Metrics::new(style.size_px, style.line_height_px));
    buffer.set_size(system.raw(), None, None);
    let attributes = graphics::to_attributes(iced::Font::DEFAULT)
        .weight(cosmic_text::Weight(style.weight))
        .letter_spacing(style.tracking_em);
    buffer.set_text(system.raw(), content, &attributes, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(system.raw(), false);
    let width = graphics::measure(&buffer).0.width.ceil();
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(key, width);
    });
    width
}

const BAR_STYLE: TextStyle = TextStyle::CALL;

fn bar_width(app: &App, bar: Bar) -> f32 {
    measure(bar.key().text(app.lang), BAR_STYLE) + 2.0 * SP3
}

fn bar_start() -> f32 {
    SP2 + CONTROL + SP2
}

fn controls_width() -> f32 {
    3.0 * CONTROL + 2.0 * SP2 + SP2 + 2.0
}

#[must_use]
pub fn compact(app: &App) -> bool {
    let bars: f32 = Bar::ALL.iter().map(|bar| bar_width(app, *bar)).sum::<f32>() + SP1 * 5.0;
    bar_start() + bars + controls_width() + 120.0 > app.window.width
}

#[must_use]
pub fn bar_x(app: &App, target: Bar) -> f32 {
    if compact(app) {
        return bar_start();
    }
    let mut x = bar_start();
    for bar in Bar::ALL {
        if bar == target {
            break;
        }
        x += bar_width(app, bar) + SP1;
    }
    x
}

fn tip<'a>(paint: &Paint, child: impl Into<Element<'a, Message>>, label: &'a str) -> Element<'a, Message> {
    let paint = *paint;
    tooltip(
        child,
        widgets::surface(
            container(text(label, TextStyle::CAP)).padding([SP1, SP2]).style(move |_| {
                let mut style = style::popover(&paint);
                style.border.radius = R2.into();
                style
            }),
            &paint,
            Level::El2,
            R2,
        ),
        tooltip::Position::Bottom,
    )
    .delay(component::TOOLTIP_DELAY)
    .gap(SP2)
    .into()
}

fn icon_button<'a>(paint: &Paint, icon: Icon, message: Message, on: bool, label: &'a str) -> Element<'a, Message> {
    let paint = *paint;
    let tint = if on { paint.fg_1() } else { paint.fg_2() };
    let control = button(container(glide_iced::glyph::glyph(icon, 16.0, tint)).center(Fill))
        .on_press(message)
        .width(CONTROL)
        .height(CONTROL)
        .padding(0);
    tip(
        &paint,
        widgets::button(control, move |status| {
            if on {
                style::flat(paint.layered(paint.fill_2(), status), paint.fg_1(), R2)
            } else {
                style::action(&paint, Tone::Ghost, status)
            }
        }),
        label,
    )
}

fn rule<'a>(paint: &Paint) -> Element<'a, Message> {
    let paint = *paint;
    container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.hairline(), 0.0)).into()
}

fn vertical_rule<'a>(paint: &Paint) -> Element<'a, Message> {
    let paint = *paint;
    container(Space::new().width(1).height(Fill)).style(move |_| style::rounded(paint.hairline(), 0.0)).into()
}

fn bar_button(app: &App, paint: &Paint, bar: Bar) -> Element<'static, Message> {
    let paint = *paint;
    let open = app.menu.as_ref().is_some_and(|menu| menu.kind == MenuKind::Bar(bar) && menu.reveal.is_open());
    let label = bar.key().text(app.lang);
    let control = button(container(text(label, BAR_STYLE).wrapping(Wrapping::None)).center(Fill))
        .on_press(Message::BarPressed(bar))
        .width(bar_width(app, bar))
        .height(CONTROL)
        .padding(0);
    let control = widgets::button(control, move |status| {
        if open { style::flat(paint.fill_2(), paint.fg_1(), R2) } else { style::action(&paint, Tone::Ghost, status) }
    });
    mouse_area(control).on_enter(Message::BarHovered(bar)).into()
}

fn header(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let control_paint = if app.window_focused() { paint } else { paint.faded(component::IDLE_CONTROLS) };
    let maximize = button(glide_iced::glyph::window_control(app.edge_to_edge()))
        .on_press(Message::WindowMaximize)
        .width(CONTROL)
        .height(CONTROL)
        .padding(0);
    let maximize = tip(
        &paint,
        widgets::button(maximize, move |status| style::maximize_control(&control_paint, status)),
        Key::Maximize.text(app.lang),
    );
    let mut left = row![maximize].spacing(SP2).align_y(Alignment::Center);
    if !app.shell_menu {
        left = left.push(if compact(app) {
            let open = app.menu.as_ref().is_some_and(|menu| menu.kind == MenuKind::Compact && menu.reveal.is_open());
            icon_button(&control_paint, Icon::Menu, Message::BarPressed(Bar::File), open, Key::Menu.text(app.lang))
        } else {
            iced::widget::row(Bar::ALL.map(|bar| bar_button(app, &control_paint, bar))).spacing(SP1).into()
        });
    }
    let close = button(glide_iced::glyph::window_close())
        .on_press(Message::CloseRequested)
        .width(CONTROL)
        .height(CONTROL)
        .padding(0);
    let close = tip(
        &paint,
        widgets::button(close, move |status| style::window_control(&control_paint, status)),
        Key::CloseWindow.text(app.lang),
    );
    let controls = row![
        icon_button(
            &control_paint,
            Icon::Sidebar,
            Message::Command(Command::ToggleSidebar),
            app.prefs.sidebar,
            Key::Sidebar.text(app.lang)
        ),
        icon_button(&control_paint, Icon::Plus, Message::Command(Command::New), false, Key::New.text(app.lang)),
        Space::new().width(2),
        close,
    ]
    .spacing(SP2)
    .align_y(Alignment::Center);
    let bar =
        row![left, Space::new().width(Fill), controls].align_y(Alignment::Center).padding(Padding::from([0.0, SP2]));
    let left_width = if compact(app) {
        bar_start() + CONTROL
    } else {
        bar_start() + Bar::ALL.iter().map(|bar| bar_width(app, *bar)).sum::<f32>() + SP1 * 5.0
    };
    let room = app.window.width - 2.0 * left_width.max(controls_width()) - SP4;
    let mut layers: Vec<Element<'_, Message>> = vec![bar.height(Fill).into()];
    if room > 80.0 && !app.shell_menu {
        let title = app.doc().map(|doc| {
            let dirty = if doc.dirty() { "• " } else { "" };
            format!("{dirty}{}", app.doc_name(doc))
        });
        let mut lines = column![
            text(title.unwrap_or_else(|| Key::AppName.text(app.lang).to_string()), TextStyle::WIN_TITLE)
                .wrapping(Wrapping::None)
        ]
        .align_x(Alignment::Center);
        if let Some(subtitle) = app.subtitle().filter(|_| app.window.height > 360.0) {
            lines = lines.push(text(subtitle, TextStyle::CAP).color(paint.fg_3()).wrapping(Wrapping::None));
        }
        let title_color = if app.window_focused() { paint.fg_1() } else { paint.fg_3() };
        layers.push(
            container(
                container(lines)
                    .max_width(room)
                    .clip(true)
                    .style(move |_| container::Style { text_color: Some(title_color), ..container::Style::default() }),
            )
            .center(Fill)
            .into(),
        );
        layers.reverse();
    }
    let corners = if app.edge_to_edge() { Radius::default() } else { Radius::default().top_left(R3).top_right(R3) };
    let background = if app.window_focused() { paint.bar_bg() } else { paint.bar_bg_idle() };
    let bar = container(iced::widget::stack(layers))
        .width(Fill)
        .height(HEADER)
        .style(move |_| style::rounded(background, corners));
    mouse_area(bar).on_press(Message::WindowDrag).on_double_click(Message::WindowMaximize).into()
}

#[must_use]
pub fn tab_slot(x: f32, width: f32, count: usize) -> usize {
    let slot = (width - 2.0 * TAB_PAD + TAB_GAP) / count.max(1) as f32;
    (((x - TAB_PAD) / slot.max(1.0)).floor().max(0.0) as usize).min(count.saturating_sub(1))
}

fn tab_button<'a>(app: &'a App, index: usize, doc: &'a Document) -> Element<'a, Message> {
    let paint = app.paint;
    let active = index == app.active;
    let hovered = app.hovered_tab == Some(index);
    let mut label =
        row![text(app.doc_name(doc), TextStyle::CAP).wrapping(Wrapping::None)].spacing(SP2).align_y(Alignment::Center);
    if doc.dirty() && !hovered {
        label = label.push(
            container(Space::new().width(6).height(6))
                .style(move |_| style::rounded(paint.fg_3(), glide_theme::shape::R_ROUND)),
        );
    }
    let close: Element<'a, Message> = if hovered || active {
        let control = button(container(glide_iced::glyph::glyph(Icon::Close, 10.0, paint.fg_2())).center(Fill))
            .on_press(Message::Command(Command::CloseTab(index)))
            .width(22)
            .height(22)
            .padding(0);
        widgets::button(control, move |status| style::action(&paint, Tone::Ghost, status))
    } else {
        Space::new().width(22).height(22).into()
    };
    let inner = stack![
        container(label).center_x(Fill).center_y(Fill).padding(Padding::from([0, 28])).clip(true),
        container(close).padding(Padding::from([0.0, SP1])).height(Fill).align_y(Alignment::Center),
    ];
    let lifted = app.dragged_tab() == Some(index);
    let tab = widgets::container(container(inner).width(Fill).height(Fill), move |hover, press| {
        let (background, color) = if active {
            (view_bg(&paint), paint.fg_1())
        } else {
            (
                paint.layer(Color::TRANSPARENT, ST_HOVER * hover + (ST_PRESS - ST_HOVER) * press),
                if hover > 0.5 { paint.fg_1() } else { paint.fg_2() },
            )
        };
        container::Style { text_color: Some(color), ..style::rounded(background, R2) }
    });
    let tab =
        if active { widgets::surface(tab, &paint, if lifted { Level::El2 } else { Level::El1 }, R2) } else { tab };
    mouse_area(tab)
        .on_press(Message::TabPressed(index))
        .on_enter(Message::TabHovered(Some(index)))
        .on_exit(Message::TabHovered(None))
        .on_middle_press(Message::Command(Command::CloseTab(index)))
        .into()
}

fn tab_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let tabs: Vec<Element<'_, Message>> =
        app.docs.iter().enumerate().map(|(index, doc)| tab_button(app, index, doc)).collect();
    let bar = container(iced::widget::row(tabs).spacing(TAB_GAP).height(Fill))
        .padding(Padding::from([SP1, TAB_PAD]))
        .width(Fill)
        .height(TABS)
        .style(move |_| style::rounded(paint.bar_bg(), 0.0));
    mouse_area(bar).on_move(|point| Message::TabMoved(point.x)).into()
}

fn field<'a>(paint: &Paint, input: text_input::TextInput<'a, Message>, width: Length) -> Element<'a, Message> {
    let paint = *paint;
    widgets::field(
        container(input.size(TextStyle::BODY.size_px).padding(Padding::from([SP1, 0.0])).style(move |_, status| {
            text_input::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                ..style::field(&paint, status)
            }
        }))
        .padding(Padding::from([0.0, SP2]))
        .width(width)
        .height(CONTROL)
        .align_y(Alignment::Center),
        &paint,
    )
}

fn toggle<'a>(paint: &Paint, label: &'a str, on: bool, message: Message, hint: &'a str) -> Element<'a, Message> {
    let paint = *paint;
    let control = button(container(text(label, TextStyle::CAP).font(crate::MONO)).center(Fill))
        .on_press(message)
        .width(CONTROL)
        .height(CONTROL)
        .padding(0);
    tip(&paint, widgets::button(control, move |status| style::toggle(&paint, on, status)), hint)
}

fn pill<'a>(paint: &Paint, label: &'a str, message: Message, tone: Tone) -> Element<'a, Message> {
    let paint = *paint;
    let control = button(text(label, TextStyle::CALL).wrapping(Wrapping::None))
        .on_press(message)
        .height(CONTROL)
        .padding(Padding::from([SP1, SP3]));
    widgets::button(control, move |status| style::action(&paint, tone, status))
}

fn find_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(find) = &app.find else { return Space::new().into() };
    let lang = app.lang;
    let narrow = app.window.width < 760.0;
    let width = if narrow { Length::Fill } else { Length::Fixed(FIND_FIELD) };
    let search = field(
        &paint,
        text_input(Key::FindPlaceholder.text(lang), &find.query.text)
            .id(FIND_ID)
            .on_input(Message::FindInput)
            .on_submit(Message::FindSubmit),
        width,
    );
    let status = text(app.find_label(), TextStyle::NUM)
        .color(if find.error.is_some() { paint.danger() } else { paint.fg_3() })
        .wrapping(Wrapping::None);
    let options = row![
        icon_button(&paint, Icon::Up, Message::Command(Command::FindPrevious), false, Key::FindPrevious.text(lang)),
        icon_button(&paint, Icon::Down, Message::Command(Command::FindNext), false, Key::FindNext.text(lang)),
        toggle(&paint, "Aa", find.query.case, Message::FindToggle(Toggle::Case), Key::MatchCase.text(lang)),
        toggle(&paint, "W", find.query.word, Message::FindToggle(Toggle::Word), Key::WholeWord.text(lang)),
        toggle(&paint, ".*", find.query.regex, Message::FindToggle(Toggle::Regex), Key::Regex.text(lang)),
        status,
        Space::new().width(Fill),
        toggle(&paint, "⇄", find.replacing, Message::ToggleReplace, Key::ShowReplace.text(lang)),
        pill(&paint, Key::Done.text(lang), Message::FindClose, Tone::Neutral),
    ]
    .spacing(SP1)
    .align_y(Alignment::Center)
    .width(Fill);
    let first: Element<'_, Message> = if narrow {
        column![search, options].spacing(SP1).into()
    } else {
        row![search, options].spacing(SP2).align_y(Alignment::Center).into()
    };
    let mut body = column![first].spacing(SP1);
    if find.replacing {
        let replace = field(
            &paint,
            text_input(Key::ReplacePlaceholder.text(lang), &find.replace)
                .id(REPLACE_ID)
                .on_input(Message::ReplaceInput)
                .on_submit(Message::ReplaceOne),
            width,
        );
        body = body.push(
            row![
                replace,
                pill(&paint, Key::ReplaceOne.text(lang), Message::ReplaceOne, Tone::Neutral),
                pill(&paint, Key::ReplaceAll.text(lang), Message::ReplaceAll, Tone::Neutral),
            ]
            .spacing(SP2)
            .align_y(Alignment::Center),
        );
    }
    container(body)
        .padding(Padding::from([SP2, SP3]))
        .width(Fill)
        .style(move |_| style::rounded(paint.bar_bg(), 0.0))
        .into()
}

fn goto_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(input) = &app.goto else { return Space::new().into() };
    let line = app.doc().map_or(1, |doc| doc.cursor().position.line + 1);
    let total = app.doc().map_or(1, |doc| doc.text.matches('\n').count() + 1);
    container(
        row![
            text(Key::GoToLine.text(app.lang).trim_end_matches('…').to_string(), TextStyle::CALL),
            field(
                &paint,
                text_input(Key::LinePlaceholder.text(app.lang), input)
                    .id(GOTO_ID)
                    .on_input(Message::GotoInput)
                    .on_submit(Message::GotoSubmit),
                Length::Fixed(180.0),
            ),
            text(format!("{line} / {total}"), TextStyle::NUM).color(paint.fg_3()),
            Space::new().width(Fill),
            pill(&paint, Key::Cancel.text(app.lang), Message::Escape, Tone::Neutral),
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([SP2, SP3]))
    .width(Fill)
    .style(move |_| style::rounded(paint.bar_bg(), 0.0))
    .into()
}

fn infobar(app: &App) -> Option<Element<'_, Message>> {
    let paint = app.paint;
    let doc = app.doc().filter(|doc| doc.changed_on_disk)?;
    let gone = doc.path.as_deref().is_some_and(|path| !path.exists());
    let message = if gone { Key::DeletedOnDisk } else { Key::ChangedOnDisk };
    let mut actions = row![].spacing(SP2).align_y(Alignment::Center);
    if !gone {
        actions = actions.push(pill(&paint, Key::Reload.text(app.lang), Message::Disk(true), Tone::Neutral));
    }
    actions = actions.push(pill(&paint, Key::KeepMine.text(app.lang), Message::Disk(false), Tone::Ghost));
    let warn = to_iced(paint.tokens.warn_soft).scale_alpha(paint.alpha);
    Some(
        container(
            row![text(message.text(app.lang), TextStyle::CALL), Space::new().width(Fill), actions]
                .align_y(Alignment::Center),
        )
        .padding(Padding::from([SP2, SP3]))
        .width(Fill)
        .style(move |_| style::rounded(warn, 0.0))
        .into(),
    )
}

fn tree_row<'a>(app: &'a App, node: &'a crate::tree::Node) -> Element<'a, Message> {
    let paint = app.paint;
    let current = app.doc().and_then(|doc| doc.path.as_deref()) == Some(node.entry.path.as_path());
    let indent = SP2 + node.depth as f32 * 14.0;
    let chevron: Element<'a, Message> = if node.entry.folder {
        let icon = if node.expanded { Icon::Down } else { Icon::Right };
        glide_iced::glyph::glyph(icon, 12.0, paint.fg_3())
    } else {
        Space::new().width(12).height(12).into()
    };
    let icon = glide_iced::glyph::glyph(
        if node.entry.folder { Icon::Folder } else { Icon::File },
        15.0,
        if current { paint.acc_bg() } else { paint.fg_3() },
    );
    let label = text(node.entry.name.as_str(), if current { TextStyle::HEAD } else { TextStyle::CALL })
        .wrapping(Wrapping::None);
    let content = row![Space::new().width(indent), chevron, icon, label].spacing(SP1 + 2.0).align_y(Alignment::Center);
    let control = button(container(content).height(Fill).align_y(Alignment::Center).clip(true))
        .on_press(Message::TreeClicked(node.entry.path.clone()))
        .width(Fill)
        .height(ROW)
        .padding(0);
    let control = widgets::button(control, move |status| {
        let base = if current { paint.acc_soft() } else { Color::TRANSPARENT };
        style::flat(paint.layered(base, status), if current { paint.fg_1() } else { paint.fg_2() }, R2)
    });
    mouse_area(control).on_right_press(Message::TreeMenu(node.entry.path.clone())).into()
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let background = sidebar_bg(&paint);
    let body: Element<'_, Message> = match &app.tree {
        Some(tree) => {
            let title = row![
                text(tree.title(), TextStyle::MICRO).color(paint.fg_3()).wrapping(Wrapping::None),
                Space::new().width(Fill),
                icon_button(
                    &paint,
                    Icon::Refresh,
                    Message::Command(Command::RefreshTree),
                    false,
                    Key::Refresh.text(app.lang)
                ),
                icon_button(
                    &paint,
                    Icon::Close,
                    Message::Command(Command::CloseFolder),
                    false,
                    Key::CloseFolder.text(app.lang)
                ),
            ]
            .spacing(SP1)
            .align_y(Alignment::Center)
            .padding(Padding::from([SP1, SP2]).left(SP3));
            let rows: Vec<Element<'_, Message>> = tree.nodes.iter().map(|node| tree_row(app, node)).collect();
            let list: Element<'_, Message> = if tree.loaded && rows.is_empty() {
                container(text(Key::EmptyFolder.text(app.lang), TextStyle::CALL).color(paint.fg_3()))
                    .padding(SP4)
                    .into()
            } else {
                scrollable(iced::widget::column(rows).padding(Padding::from([0.0, SP2]))).height(Fill).into()
            };
            column![title, list].into()
        }
        None => container(
            column![
                glide_iced::glyph::glyph(Icon::Folder, 28.0, paint.fg_3()),
                text(Key::NoFolder.text(app.lang), TextStyle::CALL).color(paint.fg_2()).align_x(Alignment::Center),
                pill(
                    &paint,
                    Key::OpenFolderButton.text(app.lang),
                    Message::Command(Command::OpenFolder),
                    Tone::Neutral
                ),
            ]
            .spacing(SP3)
            .align_x(Alignment::Center),
        )
        .padding(SP4)
        .center(Fill)
        .into(),
    };
    let bottom_left = if app.edge_to_edge() || app.prefs.status_bar { 0.0 } else { R3 };
    container(body)
        .width(SIDEBAR)
        .height(Fill)
        .style(move |_| style::rounded(background, Radius::default().bottom_left(bottom_left)))
        .into()
}

fn editor_style(paint: &Paint) -> impl Fn(&iced::Theme, editor_widget::Status) -> editor_widget::Style + use<> {
    let paint = *paint;
    move |_, _| editor_widget::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: paint.fg_4(),
        value: paint.fg_1(),
        selection: paint.acc_soft(),
    }
}

struct Preview {
    paint: Paint,
}

fn fenced(language: Language, code: &str) -> std::sync::Arc<crate::syntax::Lines> {
    thread_local! {
        static CACHE: RefCell<HashMap<(Language, String), std::sync::Arc<crate::syntax::Lines>>> =
            RefCell::new(HashMap::new());
    }
    let key = (language, code.to_string());
    if let Some(lines) = CACHE.with_borrow(|cache| cache.get(&key).cloned()) {
        return lines;
    }
    let lines = std::sync::Arc::new(crate::syntax::highlight(language, code));
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, std::sync::Arc::clone(&lines));
    });
    lines
}

impl<'a> markdown::Viewer<'a, Message> for Preview {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::Link(url)
    }

    fn code_block(
        &self, settings: markdown::Settings, language: Option<&'a str>, code: &'a str, _lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        let paint = self.paint;
        let language = language.and_then(Language::from_key).unwrap_or(Language::Plain);
        let palette = crate::syntax::Palette::new(&paint.tokens, crate::MONO);
        let lines = fenced(language, code.trim_end_matches('\n'));
        let rows = code.trim_end_matches('\n').split('\n').enumerate().map(|(index, line)| {
            let mut spans: Vec<iced::widget::text::Span<'a, (), iced::Font>> = Vec::new();
            let mut at = 0;
            for (range, style) in lines.get(index).map(Vec::as_slice).unwrap_or_default() {
                if range.start < at || range.end > line.len() {
                    continue;
                }
                if range.start > at {
                    spans.push(iced::widget::span(&line[at..range.start]).font(crate::MONO));
                }
                let highlight = palette.format(*style);
                spans.push(
                    iced::widget::span(&line[range.clone()])
                        .font(highlight.font.unwrap_or(crate::MONO))
                        .color_maybe(highlight.color),
                );
                at = range.end;
            }
            if at < line.len() || line.is_empty() {
                spans.push(iced::widget::span(&line[at..]).font(crate::MONO));
            }
            iced::widget::rich_text(spans).size(settings.code_size).into()
        });
        let background = paint.fill_1();
        let border = paint.line();
        container(
            scrollable(container(iced::widget::column(rows)).padding(settings.code_size)).direction(
                iced::widget::scrollable::Direction::Horizontal(iced::widget::scrollable::Scrollbar::default()),
            ),
        )
        .width(Fill)
        .style(move |_| container::Style {
            text_color: Some(paint.fg_1()),
            background: Some(Background::Color(background)),
            border: Border { color: border, width: 1.0, radius: R2.into() },
            ..container::Style::default()
        })
        .into()
    }
}

fn editor(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(doc) = app.doc() else {
        return container(
            column![
                text(Key::EmptyTitle.text(app.lang), TextStyle::T3),
                text(Key::EmptyBody.text(app.lang), TextStyle::CALL).color(paint.fg_2()),
            ]
            .spacing(SP2)
            .align_x(Alignment::Center),
        )
        .center(Fill)
        .into();
    };
    let blocked = app.dialog.is_some() || app.menu_open();
    let editor = text_editor(&doc.content)
        .id(EDITOR)
        .on_action(Message::Edit)
        .key_binding(move |press| if blocked { None } else { crate::app::binding(press) })
        .font(app.font())
        .size(app.font_size())
        .line_height(iced::widget::text::LineHeight::Relative(1.5))
        .padding(Padding { top: SP3, bottom: SP6, left: SP5, right: SP4 })
        .height(Fill)
        .wrapping(Wrapping::WordOrGlyph)
        .style(editor_style(&paint))
        .highlight_with::<Bridge>(app.settings(doc), Highlight::to_format);
    let editor = mouse_area(editor).on_right_press(Message::Command(Command::ContextMenu));
    if !app.previewing() {
        return editor.into();
    }
    let items = app.markdown.as_ref().map(|(_, _, items)| items.as_slice()).unwrap_or_default();
    let mut settings = markdown::Settings::with_text_size(
        app.font_size(),
        markdown::Style {
            font: iced::Font::DEFAULT,
            inline_code_highlight: iced::advanced::text::Highlight {
                background: Background::Color(paint.fill_2()),
                border: Border { radius: 4.0.into(), ..Border::default() },
            },
            inline_code_padding: Padding::from([0.0, 3.0]),
            inline_code_color: paint.fg_1(),
            inline_code_font: crate::MONO,
            code_block_font: crate::MONO,
            link_color: paint.acc_bg(),
        },
    );
    settings.code_size = (app.font_size() * 0.9).into();
    let viewer = Preview { paint };
    let preview = scrollable(
        container(markdown::view_with(items, settings, &viewer)).padding(Padding::from([SP4, SP5])).width(Fill),
    )
    .height(Fill);
    row![container(editor).width(Fill), vertical_rule(&paint), container(preview).width(Fill)].into()
}

fn status_button<'a>(paint: &Paint, label: String, message: Message) -> Element<'a, Message> {
    let paint = *paint;
    let control = button(text(label, TextStyle::CAP).wrapping(Wrapping::None))
        .on_press(message)
        .height(STATUS - 4.0)
        .padding(Padding::from([2.0, SP2]));
    widgets::button(control, move |status| style::action(&paint, Tone::Ghost, status))
}

fn status_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let lang = app.lang;
    let mut left = row![].spacing(SP3).align_y(Alignment::Center);
    let mut right = row![].spacing(SP1).align_y(Alignment::Center);
    if let Some(doc) = app.doc() {
        let cursor = doc.cursor().position;
        let line_start =
            crate::document::offset(&doc.text, iced::widget::text_editor::Position { line: cursor.line, column: 0 });
        let caret = crate::document::offset(&doc.text, cursor);
        let column = doc.text[line_start..caret].chars().count() + 1;
        left = left.push(
            text(
                format!("{} {}, {} {column}", Key::Line.text(lang), cursor.line + 1, Key::Column.text(lang)),
                TextStyle::CAP,
            )
            .color(paint.fg_2()),
        );
        let selected = doc.selected();
        if !selected.is_empty() {
            let count = doc.text[selected].chars().count();
            left = left.push(text(format!("{count} {}", Key::Selected.text(lang)), TextStyle::CAP).color(paint.fg_3()));
        }
        if doc.text.len() <= 2 * 1024 * 1024 {
            left = left
                .push(text(format!("{} {}", doc.words(), Key::Words.text(lang)), TextStyle::CAP).color(paint.fg_3()));
        }
        let language = if doc.pinned.is_some() {
            doc.language().name().to_string()
        } else {
            format!("{} · {}", doc.language().name(), Key::Detected.text(lang))
        };
        let indent = if app.prefs.spaces {
            format!("{}: {}", Key::Spaces.text(lang), app.prefs.tab_width)
        } else {
            format!("{}: {}", Key::Tabs.text(lang), app.prefs.tab_width)
        };
        let width = app.window.width;
        right = right
            .push(status_button(&paint, language, Message::StatusMenu(MenuKind::Syntax, width - MENU_WIDTH - SP2)))
            .push(status_button(&paint, indent, Message::StatusMenu(MenuKind::TabWidths, width - MENU_WIDTH - SP2)))
            .push(status_button(
                &paint,
                doc.ending.name().to_string(),
                Message::StatusMenu(MenuKind::Endings, width - MENU_WIDTH - SP2),
            ))
            .push(text(doc.encoding.name(), TextStyle::CAP).color(paint.fg_3()));
    }
    let corners =
        if app.edge_to_edge() { Radius::default() } else { Radius::default().bottom_left(R3).bottom_right(R3) };
    let background = paint.bar_bg();
    container(row![left, Space::new().width(Fill), right].align_y(Alignment::Center))
        .padding(Padding::from([0.0, SP3]).right(SP2))
        .width(Fill)
        .height(STATUS)
        .clip(true)
        .style(move |_| style::rounded(background, corners))
        .into()
}

fn action(app: &App, key: Key, command: Command, enabled: bool) -> Item {
    Item::Action {
        label: key.text(app.lang).to_string(),
        shortcut: shortcut_label(command),
        command,
        enabled,
        check: None,
    }
}

fn check(app: &App, key: Key, command: Command, on: bool, enabled: bool) -> Item {
    Item::Action {
        label: key.text(app.lang).to_string(),
        shortcut: shortcut_label(command),
        command,
        enabled,
        check: Some(on),
    }
}

fn labelled(label: String, command: Command, enabled: bool, on: Option<bool>) -> Item {
    Item::Action { label, shortcut: None, command, enabled, check: on }
}

#[allow(clippy::too_many_lines)] // The whole menu bar as one table, so menus and shortcuts read side by side
fn bar_items(app: &App, bar: Bar) -> Vec<Item> {
    let doc = app.doc();
    let selection = doc.is_some_and(|doc| !doc.selected().is_empty());
    let path = doc.is_some_and(|doc| doc.path.is_some());
    let has = doc.is_some();
    match bar {
        Bar::File => vec![
            action(app, Key::New, Command::New, true),
            action(app, Key::NewWindow, Command::NewWindow, true),
            Item::Separator,
            action(app, Key::Open, Command::Open, true),
            action(app, Key::OpenFolder, Command::OpenFolder, true),
            action(app, Key::OpenRecent, Command::Recent, !app.recent.is_empty()),
            Item::Separator,
            action(app, Key::Save, Command::Save, has),
            action(app, Key::SaveAs, Command::SaveAs, has),
            action(app, Key::SaveAll, Command::SaveAll, app.docs.iter().any(Document::dirty)),
            action(app, Key::Revert, Command::Revert, path && doc.is_some_and(Document::dirty)),
            Item::Separator,
            action(app, Key::ShowInFiles, Command::ShowInFiles, path),
            action(app, Key::CopyPath, Command::CopyPath, path),
            Item::Separator,
            action(app, Key::CloseTab, Command::Close, has),
            action(app, Key::CloseFolder, Command::CloseFolder, app.tree.is_some()),
            action(app, Key::CloseWindow, Command::CloseWindow, true),
        ],
        Bar::Edit => vec![
            action(app, Key::Undo, Command::Undo, doc.is_some_and(|doc| doc.history.can_undo())),
            action(app, Key::Redo, Command::Redo, doc.is_some_and(|doc| doc.history.can_redo())),
            Item::Separator,
            action(app, Key::Cut, Command::Cut, selection),
            action(app, Key::Copy, Command::Copy, selection),
            action(app, Key::Paste, Command::Paste, has),
            action(app, Key::SelectAll, Command::SelectAll, has),
            Item::Separator,
            action(app, Key::DuplicateLine, Command::DuplicateLine, has),
            action(app, Key::DeleteLine, Command::DeleteLine, has),
            action(app, Key::MoveLineUp, Command::MoveLine(true), has),
            action(app, Key::MoveLineDown, Command::MoveLine(false), has),
            action(app, Key::JoinLines, Command::JoinLines, has),
            action(app, Key::SortLines, Command::SortLines, selection),
            Item::Separator,
            action(
                app,
                Key::ToggleComment,
                Command::ToggleComment,
                doc.is_some_and(|doc| {
                    doc.language().line_comment().is_some() || doc.language().block_comment().is_some()
                }),
            ),
            action(app, Key::Indent, Command::Indent, has),
            action(app, Key::Unindent, Command::Unindent, has),
            Item::Separator,
            action(app, Key::Uppercase, Command::Case(Case::Upper), has),
            action(app, Key::Lowercase, Command::Case(Case::Lower), has),
            action(app, Key::TitleCase, Command::Case(Case::Title), has),
            action(app, Key::InvertCase, Command::Case(Case::Invert), has),
            Item::Separator,
            action(app, Key::TrimTrailing, Command::TrimTrailing, has),
            action(app, Key::InsertDate, Command::InsertDate, has),
        ],
        Bar::View => vec![
            check(app, Key::Sidebar, Command::ToggleSidebar, app.prefs.sidebar, true),
            check(app, Key::StatusBar, Command::ToggleStatusBar, app.prefs.status_bar, true),
            Item::Separator,
            check(app, Key::Monospace, Command::ToggleMonospace, app.prefs.monospace, true),
            check(
                app,
                Key::Preview,
                Command::TogglePreview,
                app.previewing(),
                doc.is_some_and(|doc| doc.language() == Language::Markdown),
            ),
            Item::Separator,
            action(app, Key::ZoomIn, Command::ZoomIn, true),
            action(app, Key::ZoomOut, Command::ZoomOut, true),
            action(app, Key::ZoomReset, Command::ZoomReset, app.zoom != 0.0),
            Item::Separator,
            check(app, Key::HiddenFiles, Command::ToggleHidden, app.prefs.hidden_files, true),
            action(app, Key::Refresh, Command::RefreshTree, app.tree.is_some()),
            Item::Separator,
            check(app, Key::FullScreen, Command::FullScreen, app.frame == crate::app::Frame::Fullscreen, true),
        ],
        Bar::Search => vec![
            action(app, Key::Find, Command::Find, has),
            action(app, Key::Replace, Command::Replace, has),
            Item::Separator,
            action(app, Key::FindNext, Command::FindNext, has),
            action(app, Key::FindPrevious, Command::FindPrevious, has),
            action(app, Key::UseSelection, Command::UseSelection, selection),
            Item::Separator,
            action(app, Key::GoToLine, Command::GoToLine, has),
        ],
        Bar::Format => {
            let language = doc.map_or(Language::Plain, Document::language);
            let ending = doc.map_or(Ending::Lf, |doc| doc.ending);
            vec![
                labelled(format!("{} {}", Key::Syntax.text(app.lang), language.name()), Command::Syntax, has, None),
                Item::Separator,
                check(app, Key::SpacesForTabs, Command::ToggleSpaces, app.prefs.spaces, true),
                labelled(
                    format!("{}: {}", Key::TabWidth.text(app.lang), app.prefs.tab_width),
                    Command::TabWidths,
                    true,
                    None,
                ),
                check(app, Key::AutoIndent, Command::ToggleAutoIndent, app.prefs.auto_indent, true),
                Item::Separator,
                labelled(format!("{} {}", Key::LineEndings.text(app.lang), ending.name()), Command::Endings, has, None),
                check(app, Key::TrimOnSave, Command::ToggleTrimOnSave, app.prefs.trim_on_save, true),
                Item::Separator,
                action(app, Key::Statistics, Command::Statistics, has),
            ]
        }
        Bar::Help => {
            vec![action(app, Key::Shortcuts, Command::Shortcuts, true), action(app, Key::About, Command::About, true)]
        }
    }
}

#[must_use]
#[allow(clippy::too_many_lines)] // Every popup menu as one table, the source for both drawing and keyboard order
pub fn menu_items(app: &App, kind: &MenuKind) -> Vec<Item> {
    match kind {
        MenuKind::Bar(bar) => bar_items(app, *bar),
        MenuKind::Compact => Bar::ALL
            .into_iter()
            .flat_map(|bar| {
                std::iter::once(Item::Header(bar.key().text(app.lang).to_string())).chain(bar_items(app, bar))
            })
            .collect(),
        MenuKind::Context => {
            let doc = app.doc();
            let selection = doc.is_some_and(|doc| !doc.selected().is_empty());
            vec![
                action(app, Key::Undo, Command::Undo, doc.is_some_and(|doc| doc.history.can_undo())),
                action(app, Key::Redo, Command::Redo, doc.is_some_and(|doc| doc.history.can_redo())),
                Item::Separator,
                action(app, Key::Cut, Command::Cut, selection),
                action(app, Key::Copy, Command::Copy, selection),
                action(app, Key::Paste, Command::Paste, true),
                action(app, Key::SelectAll, Command::SelectAll, true),
                Item::Separator,
                action(app, Key::ToggleComment, Command::ToggleComment, true),
                action(app, Key::DuplicateLine, Command::DuplicateLine, true),
                action(app, Key::Uppercase, Command::Case(Case::Upper), true),
                action(app, Key::Lowercase, Command::Case(Case::Lower), true),
                Item::Separator,
                action(app, Key::UseSelection, Command::UseSelection, selection),
                labelled(Key::Syntax.text(app.lang).to_string(), Command::Syntax, true, None),
            ]
        }
        MenuKind::Syntax => {
            let doc = app.doc();
            let pinned = doc.and_then(|doc| doc.pinned);
            let detected = doc.map_or(Language::Plain, |doc| doc.detected);
            let mut items = vec![
                labelled(
                    format!("{} ({})", Key::Automatic.text(app.lang), detected.name()),
                    Command::SetLanguage(None),
                    true,
                    Some(pinned.is_none()),
                ),
                Item::Separator,
            ];
            items.extend(Language::ALL.into_iter().map(|language| {
                labelled(
                    language.name().to_string(),
                    Command::SetLanguage(Some(language)),
                    true,
                    Some(pinned == Some(language)),
                )
            }));
            items
        }
        MenuKind::TabWidths => {
            let mut items: Vec<Item> = [2, 4, 8]
                .into_iter()
                .map(|width| {
                    labelled(width.to_string(), Command::TabWidth(width), true, Some(app.prefs.tab_width == width))
                })
                .collect();
            items.push(Item::Separator);
            items.push(check(app, Key::SpacesForTabs, Command::ToggleSpaces, app.prefs.spaces, true));
            items
        }
        MenuKind::Endings => {
            let ending = app.doc().map_or(Ending::Lf, |doc| doc.ending);
            vec![
                labelled(
                    Key::UnixEndings.text(app.lang).to_string(),
                    Command::SetEnding(Ending::Lf),
                    true,
                    Some(ending == Ending::Lf),
                ),
                labelled(
                    Key::WindowsEndings.text(app.lang).to_string(),
                    Command::SetEnding(Ending::CrLf),
                    true,
                    Some(ending == Ending::CrLf),
                ),
            ]
        }
        MenuKind::Recent => {
            if app.recent.is_empty() {
                return vec![labelled(Key::NoRecent.text(app.lang).to_string(), Command::ClearRecent, false, None)];
            }
            let mut items: Vec<Item> = app
                .recent
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let name = path
                        .file_name()
                        .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
                    labelled(name, Command::OpenRecent(index), true, None)
                })
                .collect();
            items.push(Item::Separator);
            items.push(labelled(Key::ClearRecent.text(app.lang).to_string(), Command::ClearRecent, true, None));
            items
        }
        MenuKind::Tree(_) => vec![
            labelled(Key::OpenItem.text(app.lang).to_string(), Command::TreeOpen, true, None),
            labelled(Key::ShowInFiles.text(app.lang).to_string(), Command::TreeShow, true, None),
            labelled(Key::CopyPath.text(app.lang).to_string(), Command::TreeCopyPath, true, None),
            Item::Separator,
            labelled(Key::Refresh.text(app.lang).to_string(), Command::RefreshTree, true, None),
        ],
    }
}

#[must_use]
pub fn menu_actions(app: &App, kind: &MenuKind) -> Vec<Command> {
    menu_items(app, kind)
        .into_iter()
        .filter_map(|item| match item {
            Item::Action { command, enabled: true, .. } => Some(command),
            _ => None,
        })
        .collect()
}

fn opens_more(command: Command) -> bool {
    matches!(command, Command::Recent | Command::Syntax | Command::TabWidths | Command::Endings)
}

fn submenu_of(command: Command) -> Option<MenuKind> {
    match command {
        Command::Recent => Some(MenuKind::Recent),
        Command::Syntax => Some(MenuKind::Syntax),
        Command::TabWidths => Some(MenuKind::TabWidths),
        Command::Endings => Some(MenuKind::Endings),
        _ => None,
    }
}

fn export_items(app: &App, items: Vec<Item>, commands: &mut Vec<Command>) -> Vec<glide_app::menubar::Entry> {
    use glide_app::menubar::Entry;
    items
        .into_iter()
        .filter_map(|item| match item {
            Item::Separator => Some(Entry::Separator),
            Item::Header(_) => None,
            Item::Action { label, shortcut, command, enabled, check } => {
                if let Some(kind) = submenu_of(command) {
                    let children = export_items(app, menu_items(app, &kind), commands);
                    return Some(Entry::Submenu {
                        label: label.trim_end_matches('…').to_string(),
                        enabled: enabled && !children.is_empty(),
                        children,
                    });
                }
                commands.push(command);
                Some(Entry::Action {
                    id: u32::try_from(commands.len()).unwrap_or(u32::MAX),
                    label,
                    enabled,
                    check,
                    shortcut: shortcut.map(str::to_string),
                })
            }
        })
        .collect()
}

#[must_use]
pub fn export_menu(app: &App) -> (Vec<glide_app::menubar::Entry>, Vec<Command>) {
    let mut commands = Vec::new();
    let menu = Bar::ALL
        .into_iter()
        .map(|bar| glide_app::menubar::Entry::Submenu {
            label: bar.key().text(app.lang).to_string(),
            enabled: true,
            children: export_items(app, bar_items(app, bar), &mut commands),
        })
        .collect();
    (menu, commands)
}

fn menu_row(paint: &Paint, item: Item, index: Option<usize>, selected: bool) -> (Element<'static, Message>, f32) {
    let paint = *paint;
    match item {
        Item::Separator => (
            container(
                container(Space::new().width(Fill).height(1)).style(move |_| style::rounded(paint.hairline(), 0.0)),
            )
            .padding(Padding::from([SP1, SP2]))
            .into(),
            9.0,
        ),
        Item::Header(label) => (
            container(text(label, TextStyle::MICRO).color(paint.fg_3()))
                .padding(Padding::from([SP2, SP3]).bottom(SP1))
                .into(),
            26.0,
        ),
        Item::Action { label, shortcut, command, enabled, check } => {
            let mark: Element<'static, Message> = match check {
                Some(true) => {
                    glide_iced::glyph::glyph(Icon::Check, 14.0, if enabled { paint.acc_bg() } else { paint.fg_4() })
                }
                _ => Space::new().width(14).height(14).into(),
            };
            let mut content =
                row![mark, text(label, TextStyle::MENU_ITEM).wrapping(Wrapping::None), Space::new().width(Fill)]
                    .spacing(SP2)
                    .align_y(Alignment::Center);
            if let Some(shortcut) = shortcut {
                content = content.push(text(shortcut, TextStyle::CAP).color(if enabled {
                    paint.fg_3()
                } else {
                    paint.fg_4()
                }));
            }
            if opens_more(command) {
                content = content.push(glide_iced::glyph::glyph(Icon::Right, 12.0, paint.fg_3()));
            }
            let control = button(container(content).clip(true))
                .on_press_maybe(enabled.then_some(Message::Command(command)))
                .width(Fill)
                .height(ITEM)
                .padding(Padding::from([0.0, SP2]));
            let control = widgets::button(control, move |status| {
                let mut style = style::menu_item(&paint, status);
                if selected {
                    style.background = Some(Background::Color(paint.fill_2()));
                }
                style
            });
            let element = match index {
                Some(index) => mouse_area(control).on_enter(Message::MenuHovered(index)).into(),
                None => control,
            };
            (element, ITEM)
        }
    }
}

fn card<'a>(paint: &Paint, content: impl Into<Element<'a, Message>>, width: f32) -> Element<'a, Message> {
    let paint = *paint;
    opaque(widgets::surface(
        container(content).padding(SP1).width(width).style(move |_| style::popover(&paint)),
        &paint,
        Level::El3,
        R3,
    ))
}

fn menu_layer<'a>(app: &'a App, menu: &'a Menu) -> Element<'a, Message> {
    let progress = menu.reveal.value(glide_theme::motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let mut rows: Vec<Element<'static, Message>> = Vec::new();
    let mut height = 2.0 * SP1 + 2.0;
    let mut index = 0;
    for item in menu_items(app, &menu.kind) {
        let actionable = matches!(item, Item::Action { enabled: true, .. });
        let position = actionable.then_some(index);
        let (element, size) = menu_row(&paint, item, position, actionable && menu.selected == Some(index));
        if actionable {
            index += 1;
        }
        rows.push(element);
        height += size;
    }
    let window = app.window;
    let width = MENU_WIDTH.min(window.width - 2.0 * SP2);
    let height = height.min((window.height - 2.0 * SP3).max(1.0));
    let bar = iced::widget::scrollable::Scrollbar::new().width(4).scroller_width(4).spacing(SP1);
    let content = scrollable(iced::widget::column(rows))
        .id("glide-text-menu")
        .direction(iced::widget::scrollable::Direction::Vertical(bar))
        .height(height - 2.0 * SP1 - 2.0);
    let card = card(&paint, content, width);
    let upward = menu.at.y > window.height * 0.6;
    let x = menu.at.x.min(window.width - width - SP2).max(SP2);
    let y = if upward { menu.at.y - height - SP1 } else { menu.at.y };
    let y = y.min(window.height - height - SP2).max(SP2) + (1.0 - progress) * component::POPOVER_RISE;
    stack![
        mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Dismiss)
            .on_right_press(Message::Dismiss)
            .on_middle_press(Message::Dismiss),
        pin(widgets::scale(card, component::POPOVER_CLOSED_SCALE + (1.0 - component::POPOVER_CLOSED_SCALE) * progress))
            .x(x)
            .y(y),
    ]
    .into()
}

fn stat_row<'a>(paint: &Paint, label: &'a str, value: String) -> Element<'a, Message> {
    row![text(label, TextStyle::CALL).color(paint.fg_2()), Space::new().width(Fill), text(value, TextStyle::NUM)]
        .align_y(Alignment::Center)
        .into()
}

fn shortcut_rows(app: &App, paint: &Paint) -> Element<'static, Message> {
    let mut rows: Vec<Element<'static, Message>> = Vec::new();
    for bar in Bar::ALL {
        let items: Vec<(String, &'static str)> = bar_items(app, bar)
            .into_iter()
            .filter_map(|item| match item {
                Item::Action { label, shortcut: Some(shortcut), .. } => Some((label, shortcut)),
                _ => None,
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        rows.push(text(bar.key().text(app.lang).to_string(), TextStyle::MICRO).color(paint.fg_3()).into());
        for (label, shortcut) in items {
            rows.push(
                row![
                    text(label, TextStyle::CALL),
                    Space::new().width(Fill),
                    text(shortcut, TextStyle::CAP).color(paint.fg_2())
                ]
                .align_y(Alignment::Center)
                .into(),
            );
        }
    }
    iced::widget::column(rows).spacing(SP1 + 2.0).into()
}

#[allow(clippy::too_many_lines)] // One arm per dialog sharing a single card, scrim and reveal
fn dialog<'a>(app: &'a App, dialog: &'a Dialog) -> Element<'a, Message> {
    let progress = dialog.reveal.value(glide_theme::motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let lang = app.lang;
    let (title, body, buttons, width): (String, Element<'a, Message>, Vec<Element<'a, Message>>, f32) = match &dialog
        .kind
    {
        DialogKind::Unsaved { docs, .. } => {
            let title = match docs.as_slice() {
                [id] => {
                    let name =
                        app.docs.iter().find(|doc| doc.id == *id).map(|doc| app.doc_name(doc)).unwrap_or_default();
                    format!("{} «{name}»?", Key::UnsavedOne.text(lang))
                }
                _ => Key::UnsavedMany.text(lang).to_string(),
            };
            let mut body = column![text(Key::UnsavedBody.text(lang), TextStyle::CALL).color(paint.fg_2())].spacing(SP1);
            if docs.len() > 1 {
                for doc in app.docs.iter().filter(|doc| docs.contains(&doc.id)) {
                    body = body.push(text(format!("• {}", app.doc_name(doc)), TextStyle::CALL));
                }
            }
            (
                title,
                body.into(),
                vec![
                    pill(&paint, Key::DontSave.text(lang), Message::Dialog(Answer::Discard), Tone::Danger),
                    Space::new().width(Fill).into(),
                    pill(&paint, Key::Cancel.text(lang), Message::Dialog(Answer::Cancel), Tone::Neutral),
                    accent_pill(&paint, Key::Save.text(lang), Message::Dialog(Answer::Save)),
                ],
                420.0,
            )
        }
        DialogKind::About => (
            Key::AppName.text(lang).to_string(),
            column![
                text(Key::AboutBody.text(lang), TextStyle::CALL).color(paint.fg_2()),
                text(format!("glide-text {}", env!("CARGO_PKG_VERSION")), TextStyle::CAP).color(paint.fg_3()),
            ]
            .spacing(SP2)
            .into(),
            vec![
                Space::new().width(Fill).into(),
                accent_pill(&paint, Key::Close.text(lang), Message::Dialog(Answer::Cancel)),
            ],
            400.0,
        ),
        DialogKind::Statistics => {
            let stats = app.doc().map(|doc| crate::edit::stats(&doc.text));
            let doc = app.doc();
            let mut body = column![].spacing(SP2);
            if let (Some(stats), Some(doc)) = (stats, doc) {
                body = body
                    .push(stat_row(&paint, Key::Lines.text(lang), stats.lines.to_string()))
                    .push(stat_row(&paint, Key::WordsLong.text(lang), stats.words.to_string()))
                    .push(stat_row(&paint, Key::Characters.text(lang), stats.characters.to_string()))
                    .push(stat_row(
                        &paint,
                        Key::CharactersNoSpaces.text(lang),
                        stats.characters_without_spaces.to_string(),
                    ))
                    .push(stat_row(&paint, Key::Bytes.text(lang), stats.bytes.to_string()))
                    .push(stat_row(
                        &paint,
                        Key::Encoding.text(lang),
                        format!("{} · {}", doc.encoding.name(), doc.ending.name()),
                    ));
            }
            (
                Key::Statistics.text(lang).trim_end_matches('…').to_string(),
                body.into(),
                vec![
                    Space::new().width(Fill).into(),
                    accent_pill(&paint, Key::Close.text(lang), Message::Dialog(Answer::Cancel)),
                ],
                360.0,
            )
        }
        DialogKind::Shortcuts => (
            Key::Shortcuts.text(lang).to_string(),
            scrollable(container(shortcut_rows(app, &paint)).padding(Padding::from([0.0, SP2]).left(0.0)))
                .height(Length::Fixed((app.window.height - 220.0).clamp(120.0, 520.0)))
                .into(),
            vec![
                Space::new().width(Fill).into(),
                accent_pill(&paint, Key::Close.text(lang), Message::Dialog(Answer::Cancel)),
            ],
            460.0,
        ),
    };
    let card = container(
        column![text(title, TextStyle::T3), body, iced::widget::row(buttons).spacing(SP2).align_y(Alignment::Center)]
            .spacing(SP3),
    )
    .padding(SP4)
    .width(width)
    .max_width(app.window.width - SP6)
    .style(move |_| style::popover(&paint));
    let scrim = Color { a: 0.32 * progress, ..Color::BLACK };
    let corner = if app.edge_to_edge() { 0.0 } else { R3 };
    stack![
        opaque(
            mouse_area(container(Space::new().width(Fill).height(Fill)).style(move |_| style::rounded(scrim, corner)))
                .on_press(Message::Noop)
        ),
        container(widgets::scale(
            opaque(widgets::surface(card, &paint, Level::El3, R3)),
            component::DIALOG_CLOSED_SCALE + (1.0 - component::DIALOG_CLOSED_SCALE) * progress
        ))
        .center(Fill),
    ]
    .into()
}

fn accent_pill<'a>(paint: &Paint, label: &'a str, message: Message) -> Element<'a, Message> {
    let paint = *paint;
    let control = button(text(label, TextStyle::CALL).wrapping(Wrapping::None))
        .on_press(message)
        .height(component::CONTROL)
        .padding(Padding::from([SP2, SP4]));
    widgets::button(control, move |status| {
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => paint.acc_bg_hover(),
            _ => paint.acc_bg(),
        };
        style::flat(background, paint.acc_fg(), R2)
    })
}

fn notice<'a>(app: &'a App, message: &'a str, at: std::time::Instant) -> Element<'a, Message> {
    let progress = App::reveal(at);
    let paint = app.paint.faded(progress);
    let pill = container(text(message, TextStyle::CALL))
        .padding(Padding::from([SP2, SP4]))
        .max_width(app.window.width - 48.0)
        .style(move |_| style::popover(&paint));
    let bottom = if app.prefs.status_bar { STATUS } else { 0.0 } + SP5 + (1.0 - progress) * component::POPOVER_RISE;
    container(widgets::surface(pill, &paint, Level::El3, R3))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(Padding::from([0.0, SP5]).bottom(bottom))
        .into()
}

fn resize_handles<'a>() -> Element<'a, Message> {
    const EDGE: f32 = 5.0;
    const CORNER: f32 = 10.0;
    let handle = |width: Length, height: Length, direction: window::Direction, cursor: mouse::Interaction| {
        mouse_area(Space::new().width(width).height(height))
            .on_press(Message::WindowResize(direction))
            .interaction(cursor)
    };
    let fixed = Length::Fixed;
    let down = mouse::Interaction::ResizingDiagonallyDown;
    let up = mouse::Interaction::ResizingDiagonallyUp;
    let horizontal = mouse::Interaction::ResizingHorizontally;
    let vertical = mouse::Interaction::ResizingVertically;
    let band = |left, middle, right, left_cursor, right_cursor| {
        row![
            handle(fixed(CORNER), fixed(EDGE), left, left_cursor),
            handle(Fill, fixed(EDGE), middle, vertical),
            handle(fixed(CORNER), fixed(EDGE), right, right_cursor),
        ]
    };
    column![
        band(window::Direction::NorthWest, window::Direction::North, window::Direction::NorthEast, down, up),
        row![
            handle(fixed(EDGE), Fill, window::Direction::West, horizontal),
            Space::new().width(Fill).height(Fill),
            handle(fixed(EDGE), Fill, window::Direction::East, horizontal),
        ]
        .height(Fill),
        band(window::Direction::SouthWest, window::Direction::South, window::Direction::SouthEast, up, down),
    ]
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let mut main = column![];
    if app.docs.len() > 1 {
        main = main.push(tab_bar(app)).push(rule(&paint));
    }
    if app.find.is_some() {
        main = main.push(find_bar(app)).push(rule(&paint));
    }
    if app.goto.is_some() {
        main = main.push(goto_bar(app)).push(rule(&paint));
    }
    if let Some(bar) = infobar(app) {
        main = main.push(bar).push(rule(&paint));
    }
    main = main.push(editor(app));
    let background = view_bg(&paint);
    let bottom_right = if app.edge_to_edge() || app.prefs.status_bar { 0.0 } else { R3 };
    let bottom_left = if app.sidebar_visible() { 0.0 } else { bottom_right };
    let main = container(main).width(Fill).height(Fill).style(move |_| {
        style::rounded(background, Radius::default().bottom_right(bottom_right).bottom_left(bottom_left))
    });
    let mut middle = row![];
    if app.sidebar_visible() {
        middle = middle.push(sidebar(app)).push(vertical_rule(&paint));
    }
    middle = middle.push(main);
    let mut body = column![header(app), rule(&paint), middle.height(Fill)];
    if app.prefs.status_bar {
        body = body.push(rule(&paint)).push(status_bar(app));
    }
    let body = mouse_area(body).on_move(Message::Pointer);
    let mut layers: Vec<Element<'_, Message>> = vec![body.into()];
    if !app.edge_to_edge() {
        layers.push(resize_handles());
    }
    if let Some((text, at)) = &app.notice {
        layers.push(notice(app, text, *at));
    }
    if let Some(menu) = &app.menu {
        layers.push(menu_layer(app, menu));
    }
    if let Some(dialog) = app.dialog.as_ref().or(app.closing.as_ref()) {
        layers.push(self::dialog(app, dialog));
    }
    iced::widget::stack(layers).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_tab_lands_in_the_slot_under_the_pointer() {
        let width = 2.0 * TAB_PAD + 3.0 * 200.0 + 2.0 * TAB_GAP;
        assert_eq!(tab_slot(TAB_PAD + 1.0, width, 3), 0);
        assert_eq!(tab_slot(TAB_PAD + 200.0 + TAB_GAP + 1.0, width, 3), 1);
        assert_eq!(tab_slot(width + 50.0, width, 3), 2);
        assert_eq!(tab_slot(-10.0, width, 0), 0);
    }

    #[test]
    fn every_enabled_menu_row_is_reachable_from_the_keyboard() {
        let (app, _) = App::launch(None, &[]);
        for bar in Bar::ALL {
            let items = bar_items(&app, bar);
            let enabled = items.iter().filter(|item| matches!(item, Item::Action { enabled: true, .. })).count();
            assert_eq!(menu_actions(&app, &MenuKind::Bar(bar)).len(), enabled);
            assert!(enabled > 0, "{bar:?} has nothing to do");
        }
        let compact = menu_items(&app, &MenuKind::Compact);
        assert_eq!(compact.iter().filter(|item| matches!(item, Item::Header(_))).count(), Bar::ALL.len());
    }

    #[test]
    fn the_menu_bar_collapses_on_narrow_windows() {
        let (mut app, _) = App::launch(None, &[]);
        app.window = iced::Size::new(1200.0, 700.0);
        assert!(!compact(&app));
        assert!(bar_x(&app, Bar::Edit) > bar_x(&app, Bar::File));
        app.window = iced::Size::new(420.0, 400.0);
        assert!(compact(&app));
        assert_eq!(bar_x(&app, Bar::Help), bar_x(&app, Bar::File));
    }

    #[test]
    fn the_exported_menu_bar_mirrors_the_window_menus_with_real_submenus() {
        use glide_app::menubar::Entry;
        fn walk<'a>(entries: &'a [Entry], actions: &mut usize, syntax: &mut Option<&'a [Entry]>) {
            for entry in entries {
                match entry {
                    Entry::Action { id, .. } => {
                        *actions += 1;
                        assert_eq!(usize::try_from(*id).unwrap(), *actions, "ids follow the command table");
                    }
                    Entry::Submenu { children, .. } => {
                        if children.len() > Language::ALL.len() {
                            *syntax = Some(children);
                        }
                        walk(children, actions, syntax);
                    }
                    Entry::Separator => {}
                }
            }
        }
        let (app, _) = App::launch(None, &[]);
        let (menu, commands) = export_menu(&app);
        assert_eq!(menu.len(), Bar::ALL.len());
        let mut actions = 0;
        let mut syntax = None;
        walk(&menu, &mut actions, &mut syntax);
        assert_eq!(actions, commands.len());
        assert!(syntax.is_some(), "the highlight mode list is a submenu, not an in-window popup");
        assert!(!commands.iter().any(|command| opens_more(*command)), "commands that open popups are expanded");
    }
}
