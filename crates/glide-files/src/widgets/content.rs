use std::time::SystemTime;

use iced::widget::{
    Column, Row, button, column, container, float, mouse_area, row, rule, scrollable, sensor, space, stack, text,
    text_input,
};
use iced::{Alignment, Color, Element, Fill, Length, Padding, Vector};

use super::{divider, ellipsize, file_icon, icon};
use crate::app::{App, FILES_ID, Hover, MenuKind, Message, RENAME_ID, ViewMode};
use crate::config::ListColumn;
use crate::fs::tags;
use crate::fs::{FileEntry, SortBy, format_bytes, format_time, permissions_string};
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, R1};

const HOVER_LIFT: f32 = 4.0;
const HOVER_GROWTH: f32 = 0.06;

pub const GRID_PADDING: f32 = 20.0;
pub const TILE_GAP: f32 = 12.0;
pub const LIST_PADDING: f32 = 4.0;
const LOCATION_WIDTH: f32 = 220.0;
const HEADER_HEIGHT: f32 = 26.0;
const DOT: f32 = theme::SP2;
const GRID_ICONS: [f32; crate::config::GRID_ZOOMS] = [40.0, 52.0, 60.0, 76.0, 96.0, 128.0];
const LIST_ROWS: [(f32, f32); crate::config::LIST_ZOOMS] = [(22.0, 14.0), (26.0, 16.0), (32.0, 22.0), (40.0, 28.0)];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub icon: f32,
    pub plate: f32,
    pub tile_w: f32,
    pub tile_h: f32,
    pub label_chars: usize,
    pub row: f32,
    pub row_icon: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new(crate::config::DEFAULT_GRID_ZOOM, crate::config::DEFAULT_LIST_ZOOM)
    }
}

impl Metrics {
    #[must_use]
    pub fn new(grid_zoom: u8, list_zoom: u8) -> Self {
        let icon = GRID_ICONS[usize::from(grid_zoom).min(GRID_ICONS.len() - 1)];
        let (row, row_icon) = LIST_ROWS[usize::from(list_zoom).min(LIST_ROWS.len() - 1)];
        let plate = icon + 2.0 * theme::SP2;
        let tile_w = (icon * 1.2 + 32.0).round();
        Self { icon, plate, tile_w, tile_h: plate + 46.0, label_chars: (tile_w / 4.0) as usize, row, row_icon }
    }

    #[must_use]
    pub fn columns(&self, width: f32) -> usize {
        (((width - 2.0 * GRID_PADDING + TILE_GAP) / (self.tile_w + TILE_GAP)).floor() as usize).max(1)
    }

    #[must_use]
    pub fn span(&self, view: ViewMode, index: usize, width: f32) -> (f32, f32) {
        match view {
            ViewMode::Grid => {
                let row = (index / self.columns(width)) as f32;
                let top = GRID_PADDING + row * (self.tile_h + TILE_GAP);
                (top - GRID_PADDING, top + self.tile_h + GRID_PADDING)
            }
            ViewMode::List => {
                let top = LIST_PADDING + index as f32 * self.row;
                (top - LIST_PADDING, top + self.row + LIST_PADDING)
            }
        }
    }

    #[must_use]
    pub fn entry_at(&self, view: ViewMode, width: f32, count: usize, x: f32, y: f32) -> Option<usize> {
        let (column, row) = match view {
            ViewMode::Grid => {
                let (x, y) = (x - GRID_PADDING, y - GRID_PADDING);
                if x < 0.0 || y < 0.0 {
                    return None;
                }
                let (pitch_x, pitch_y) = (self.tile_w + TILE_GAP, self.tile_h + TILE_GAP);
                let (column, row) = ((x / pitch_x).floor(), (y / pitch_y).floor());
                let inside = x - column * pitch_x <= self.tile_w && y - row * pitch_y <= self.tile_h;
                let columns = self.columns(width);
                if !inside || column as usize >= columns {
                    return None;
                }
                (column as usize, row as usize * columns)
            }
            ViewMode::List => {
                let y = y - LIST_PADDING;
                if y < 0.0 {
                    return None;
                }
                (0, (y / self.row).floor() as usize)
            }
        };
        Some(row + column).filter(|index| *index < count)
    }

    #[must_use]
    pub fn entries_in(&self, view: ViewMode, width: f32, count: usize, from: (f32, f32), to: (f32, f32)) -> Vec<usize> {
        let (left, right) = (from.0.min(to.0), from.0.max(to.0));
        let (top, bottom) = (from.1.min(to.1), from.1.max(to.1));
        let overlaps = |x: f32, y: f32, w: f32, h: f32| x < right && x + w > left && y < bottom && y + h > top;
        let columns = self.columns(width);
        (0..count)
            .filter(|&index| match view {
                ViewMode::Grid => {
                    let (column, row) = ((index % columns) as f32, (index / columns) as f32);
                    let x = GRID_PADDING + column * (self.tile_w + TILE_GAP);
                    overlaps(x, GRID_PADDING + row * (self.tile_h + TILE_GAP), self.tile_w, self.tile_h)
                }
                ViewMode::List => overlaps(0.0, LIST_PADDING + index as f32 * self.row, width, self.row),
            })
            .collect()
    }
}

pub const LIST_HEADER: f32 = HEADER_HEIGHT + 1.0;

#[must_use]
pub const fn column_width(column: ListColumn) -> f32 {
    match column {
        ListColumn::Size => 92.0,
        ListColumn::Modified | ListColumn::Created => 168.0,
        ListColumn::Kind => 128.0,
        ListColumn::Permissions => 118.0,
        ListColumn::Tags => 140.0,
    }
}

#[must_use]
pub fn column_label(column: ListColumn, lang: crate::i18n::Lang) -> &'static str {
    match column {
        ListColumn::Size => Key::ColSize,
        ListColumn::Modified => Key::ColModified,
        ListColumn::Created => Key::ColCreated,
        ListColumn::Kind => Key::ColType,
        ListColumn::Permissions => Key::ColPermissions,
        ListColumn::Tags => Key::ColTags,
    }
    .text(lang)
}

#[must_use]
pub fn tag_color(color: tags::Color) -> Color {
    let [r, g, b] = color.rgb();
    Color::from_rgb8(r, g, b)
}

fn dots<'a>(entry: &FileEntry) -> Option<Element<'a, Message>> {
    let colors = tags::colors(&entry.tags);
    (!colors.is_empty()).then(|| {
        Row::with_children(colors.into_iter().map(|color| {
            let fill = tag_color(color);
            container(space().width(DOT).height(DOT)).style(move |_| theme::rounded(fill, theme::R_ROUND)).into()
        }))
        .spacing(2)
        .into()
    })
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    let body = if app.entries.is_empty() {
        empty_state(app)
    } else {
        match app.view {
            ViewMode::Grid => grid(app),
            ViewMode::List => list(app),
        }
    };
    let body =
        mouse_area(body).on_press(Message::PressedBackground).on_right_press(Message::MenuTarget(MenuKind::Background));
    sensor(body).on_show(Message::PaneResized).on_resize(Message::PaneResized).into()
}

fn empty_state(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let (key, glyph) = if app.search.as_ref().is_some_and(|search| search.pending) {
        (Key::Searching, Icon::Search)
    } else if app.is_searching() {
        (Key::NoSearchResults, Icon::Search)
    } else {
        (Key::EmptyFolder, Icon::Folder)
    };
    let body = column![icon(glyph, 48.0, t.fg_4()), text(key.text(app.lang)).size(theme::CALL).color(t.fg_3())]
        .spacing(theme::SP3)
        .align_x(Alignment::Center);
    container(body).center(Fill).into()
}

fn files<'a>(app: &'a App, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let t = &app.theme;
    scrollable(content)
        .id(FILES_ID)
        .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
        .width(Fill)
        .height(Fill)
        .style(move |_, status| theme::scroller(t, status))
        .into()
}

#[must_use]
pub fn visible_rows(rows: usize, row_height: f32, padding: f32, scroll: f32, height: f32) -> std::ops::Range<usize> {
    let first = ((scroll - padding) / row_height).floor().max(0.0) as usize;
    let last = ((scroll + height - padding) / row_height).ceil().max(0.0) as usize;
    first.saturating_sub(OVERSCAN).min(rows)..(last + OVERSCAN).min(rows)
}

const OVERSCAN: usize = 2;

fn spacer<'a>(height: f32) -> Element<'a, Message> {
    iced::widget::space().width(Fill).height(height).into()
}

fn rename_field(app: &App, size: f32) -> Option<Element<'_, Message>> {
    let renaming = app.renaming.as_ref()?;
    let t = &app.theme;
    Some(
        text_input("", &renaming.text)
            .id(RENAME_ID)
            .on_input(Message::RenameInput)
            .on_submit(Message::RenameSubmit)
            .size(size)
            .padding(Padding::from([1, 4]))
            .style(move |_, status| theme::field(t, status))
            .into(),
    )
}

fn grid(app: &App) -> Element<'_, Message> {
    let metrics = app.metrics();
    let columns = metrics.columns(app.pane.width);
    let pitch = metrics.tile_h + TILE_GAP;
    let total = app.entries.len().div_ceil(columns);
    let shown = visible_rows(total, pitch, GRID_PADDING, app.scroll_offset(), app.pane.height);
    let rows = app.entries.chunks(columns).skip(shown.start).take(shown.len()).map(|chunk| {
        container(Row::with_children(chunk.iter().map(|entry| tile(app, entry, &metrics))).spacing(TILE_GAP))
            .height(pitch)
            .into()
    });
    let body = std::iter::once(spacer(shown.start as f32 * pitch))
        .chain(rows)
        .chain(std::iter::once(spacer((total - shown.end) as f32 * pitch)));
    files(app, Column::with_children(body).padding(GRID_PADDING).width(Fill))
}

fn tile<'a>(app: &'a App, entry: &'a FileEntry, metrics: &Metrics) -> Element<'a, Message> {
    let t = &app.theme;
    let selected = app.selection.contains(&entry.path) || app.is_drop_target(&entry.path);
    let key = Hover::Entry(entry.path.clone());
    let hover = app.hover(&key);
    let raised = app.lift(&key);
    let lift = raised * HOVER_LIFT;
    let art = float(file_icon(app, entry, metrics.icon))
        .scale(1.0 + raised * HOVER_GROWTH)
        .translate(move |_, _| Vector::new(0.0, -lift));
    let art: Element<'a, Message> = art.into();
    let art: Element<'a, Message> = if entry.is_symlink {
        stack![
            art,
            container(
                container(icon(Icon::Link, 10.0, t.fg_1()))
                    .padding(2)
                    .style(move |_| theme::rounded(t.popover_bg(), R1)),
            )
            .align_bottom(Fill)
            .align_left(Fill),
        ]
        .into()
    } else {
        art
    };
    let plate =
        container(art).center(Length::Fixed(metrics.plate)).style(move |_| theme::tile_plate(t, selected, hover));
    let tile_w = metrics.tile_w;
    let editing = app.renaming.as_ref().is_some_and(|renaming| renaming.path == entry.path);
    let label: Element<'a, Message> = if let Some(field) = rename_field(app, 12.0).filter(|_| editing) {
        container(field).width(Length::Fixed(tile_w)).into()
    } else {
        let name = text(ellipsize(&entry.name, metrics.label_chars))
            .size(theme::CAP)
            .align_x(Alignment::Center)
            .color(if selected { t.acc_soft_fg() } else { t.fg_1() });
        let mut line = row![].spacing(theme::SP1).align_y(Alignment::Center);
        if let Some(dots) = dots(entry) {
            line = line.push(dots);
        }
        container(line.push(name))
            .padding(Padding::from([1.0, theme::SP1]))
            .max_width(tile_w)
            .style(move |_| if selected { theme::rounded(t.acc_soft(), R1) } else { container::Style::default() })
            .into()
    };
    let body = container(column![plate, label].spacing(theme::SP1).align_x(Alignment::Center))
        .center_x(Fill)
        .height(Fill)
        .clip(true);
    mouse_area(
        button(body)
            .width(Length::Fixed(tile_w))
            .height(Length::Fixed(metrics.tile_h))
            .padding(0)
            .style(move |_, _| theme::plain(t.fg_1())),
    )
    .on_press(Message::Pressed(entry.path.clone()))
    .on_right_press(Message::Secondary(entry.path.clone()))
    .on_enter(Message::Hover(key.clone(), true))
    .on_exit(Message::Hover(key, false))
    .into()
}

fn list(app: &App) -> Element<'_, Message> {
    let metrics = app.metrics();
    let now = SystemTime::now();
    let total = app.entries.len();
    let height = app.pane.height - LIST_HEADER;
    let shown = visible_rows(total, metrics.row, LIST_PADDING, app.scroll_offset(), height);
    let rows = app
        .entries
        .iter()
        .enumerate()
        .skip(shown.start)
        .take(shown.len())
        .map(|(index, entry)| list_row(app, entry, index % 2 == 1, now, &metrics));
    let body = std::iter::once(spacer(shown.start as f32 * metrics.row))
        .chain(rows)
        .chain(std::iter::once(spacer((total - shown.end) as f32 * metrics.row)));
    let rows = Column::with_children(body).padding(Padding::from([LIST_PADDING, 8.0])).width(Fill);
    column![list_header(app), divider(&app.theme), files(app, rows)].into()
}

fn list_header(app: &App) -> Element<'_, Message> {
    let lang = app.lang;
    let mut header = row![header_cell(app, SortBy::Name.label(lang), Some(SortBy::Name), None, Fill)];
    for &column in &app.columns {
        let width = Length::Fixed(column_width(column));
        header = header.push(header_cell(app, column_label(column, lang), column.sort(), Some(column), width));
    }
    if app.is_searching() {
        header = header.push(header_cell(app, Key::ColLocation.text(lang), None, None, Length::Fixed(LOCATION_WIDTH)));
    }
    let header = header.padding(Padding::from([0, 16])).height(HEADER_HEIGHT).align_y(Alignment::Center);
    mouse_area(header).on_right_press(Message::MenuTarget(MenuKind::Columns)).into()
}

fn header_cell<'a>(
    app: &'a App, label: &'a str, sort: Option<SortBy>, column: Option<ListColumn>, width: Length,
) -> Element<'a, Message> {
    let t = &app.theme;
    let active = sort.is_some() && sort == Some(app.sort.by);
    let target = column.is_some_and(|column| app.is_column_target(column));
    let mut content = row![text(label).size(theme::CAP)].spacing(theme::SP1).align_y(Alignment::Center);
    if active {
        let arrow = if app.sort.ascending { Icon::ArrowUp } else { Icon::ArrowDown };
        content = content.push(iced::widget::space::horizontal()).push(icon(arrow, 11.0, t.fg_3()));
    }
    let separated = column.is_some() || sort.is_none();
    let style = move |_: &iced::Theme, status| {
        let highlighted = active || target || theme::highlighted(status);
        theme::plain(if highlighted { t.fg_1() } else { t.fg_2() })
    };
    let padding = Padding::from([4, 8]).left(if separated { 8.0 } else { 0.0 });
    let cell: Element<'a, Message> = match column {
        Some(column) => mouse_area(
            container(content)
                .padding(padding)
                .width(Fill)
                .style(move |_| if target { theme::rounded(t.fill_2(), R1) } else { container::Style::default() }),
        )
        .on_press(Message::ColumnPressed(column))
        .on_enter(Message::Hover(Hover::Column(column), true))
        .on_exit(Message::Hover(Hover::Column(column), false))
        .into(),
        None => {
            button(content).on_press_maybe(sort.map(Message::SortBy)).padding(padding).width(Fill).style(style).into()
        }
    };
    let mut cell_row = row![].align_y(Alignment::Center).width(width);
    if separated {
        cell_row = cell_row.push(container(rule::vertical(1).style(move |_| theme::line(t))).height(14));
    }
    cell_row.push(cell).into()
}

fn column_value(app: &App, entry: &FileEntry, column: ListColumn, now: SystemTime) -> String {
    let lang = app.lang;
    let time = |stamp: Option<SystemTime>| stamp.map_or_else(|| String::from("—"), |time| format_time(time, now, lang));
    match column {
        ListColumn::Size if entry.is_dir => String::from("—"),
        ListColumn::Size => format_bytes(entry.size, lang),
        ListColumn::Modified => time(entry.modified),
        ListColumn::Created => time(entry.created),
        ListColumn::Kind => entry.category.name(lang).to_string(),
        ListColumn::Permissions => permissions_string(entry.mode),
        ListColumn::Tags => tags::words(&entry.tags).join(", "),
    }
}

fn list_row<'a>(
    app: &'a App, entry: &'a FileEntry, stripe: bool, now: SystemTime, metrics: &Metrics,
) -> Element<'a, Message> {
    let t = &app.theme;
    let selected = app.selection.contains(&entry.path) || app.is_drop_target(&entry.path);
    let key = Hover::Entry(entry.path.clone());
    let hover = app.hover(&key);
    let secondary = if selected { t.acc_soft_fg() } else { t.fg_2() };
    let cell = move |value: String, width: f32| {
        container(text(value).size(theme::CAP).color(secondary).wrapping(text::Wrapping::None))
            .padding(Padding::from([0.0, theme::SP2]))
            .width(Length::Fixed(width))
            .clip(true)
    };

    let editing = app.renaming.as_ref().is_some_and(|renaming| renaming.path == entry.path);
    let title: Element<'a, Message> = match rename_field(app, theme::CALL).filter(|_| editing) {
        Some(field) => field,
        None => text(&entry.name).size(theme::CALL).wrapping(text::Wrapping::None).into(),
    };
    let mut name = row![file_icon(app, entry, metrics.row_icon), title].spacing(theme::SP2).align_y(Alignment::Center);
    if let Some(dots) = dots(entry) {
        name = name.push(dots);
    }
    if entry.is_symlink {
        name = name.push(icon(Icon::Link, 11.0, secondary));
    }

    let mut cells = row![container(name).width(Fill).clip(true)].align_y(Alignment::Center);
    for &column in &app.columns {
        cells = cells.push(cell(column_value(app, entry, column, now), column_width(column)));
    }
    if app.is_searching() {
        cells = cells.push(cell(location(app, entry), LOCATION_WIDTH));
    }

    mouse_area(
        button(container(cells).height(Fill).align_y(Alignment::Center))
            .width(Fill)
            .height(Length::Fixed(metrics.row))
            .padding(Padding::from([0, 8]))
            .style(move |_, status| theme::list_row(t, selected, stripe, hover, status)),
    )
    .on_press(Message::Pressed(entry.path.clone()))
    .on_right_press(Message::Secondary(entry.path.clone()))
    .on_enter(Message::Hover(key.clone(), true))
    .on_exit(Message::Hover(key, false))
    .into()
}

fn location(app: &App, entry: &FileEntry) -> String {
    let parent = entry.path.parent().unwrap_or(&app.current);
    match parent.strip_prefix(&app.current) {
        Ok(relative) if relative.as_os_str().is_empty() => app.location_name(&app.current),
        Ok(relative) => relative.display().to_string(),
        Err(_) => parent.display().to_string(),
    }
}
