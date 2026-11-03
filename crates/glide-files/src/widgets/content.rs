use std::time::SystemTime;

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{
    Wrapping, corner_badge, dot, elide, elide_lines, item_ink, item_tile, measure, table_row, text,
};
use iced::widget::{Column, Row, column, container, mouse_area, row, sensor, text_input};
use iced::{Alignment, Color, Element, Fill, Length, Padding};

use super::{divider, file_icon, paint};
use crate::app::{App, FILES_ID, Hover, MenuKind, Message, RENAME_ID, ViewMode};
use crate::config::ListColumn;
use crate::fs::tags;
use crate::fs::{
    FileCategory, FileEntry, SortBy, format_bytes, format_day, permissions_string, widest_days, widest_sizes,
};
use crate::i18n::{Key, Lang};
use crate::icons::Icon;
use crate::theme;

pub const GRID_PADDING: f32 = theme::SP4;
pub const TILE_GAP: f32 = theme::SP2;
pub const ROW_GAP: f32 = theme::SP3;
pub const LABEL_LINE: f32 = 17.0;
pub const LIST_PADDING: f32 = 0.0;
const LOCATION_WIDTH: f32 = 220.0;
const NAME_MIN: f32 = 64.0;
const NAME_ROOM: f32 = 200.0;
const DOT: f32 = theme::SP2;
const DOT_GAP: f32 = 2.0;
const LABEL_LINES: usize = 2;
const LINK_MARK: f32 = 11.0;
const GRID_TILES: [(f32, f32); crate::config::GRID_ZOOMS] =
    [(92.0, 48.0), (112.0, 64.0), (136.0, 80.0), (172.0, 108.0)];
const GRID_PERCENT: [u32; crate::config::GRID_ZOOMS] = [75, 100, 125, 160];
const LIST_ROWS: [(f32, f32); crate::config::LIST_ZOOMS] = [(28.0, 16.0), (34.0, 22.0), (40.0, 24.0), (48.0, 32.0)];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub icon: f32,
    pub tile_w: f32,
    pub tile_h: f32,
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
        let (tile_w, icon) = GRID_TILES[usize::from(grid_zoom).min(GRID_TILES.len() - 1)];
        let (row, row_icon) = LIST_ROWS[usize::from(list_zoom).min(LIST_ROWS.len() - 1)];
        let tile_h = 3.0 * theme::SP2 + icon + LABEL_LINES as f32 * LABEL_LINE;
        Self { icon, tile_w, tile_h, row, row_icon }
    }

    #[must_use]
    pub fn tile_width(&self, width: f32) -> f32 {
        let columns = self.columns(width) as f32;
        ((width - 2.0 * GRID_PADDING - (columns - 1.0) * TILE_GAP) / columns).floor().max(self.tile_w)
    }

    #[must_use]
    pub fn percent(&self, view: ViewMode) -> u32 {
        match view {
            ViewMode::Grid => GRID_TILES
                .iter()
                .position(|&(_, icon)| icon == self.icon)
                .and_then(|level| GRID_PERCENT.get(level).copied())
                .unwrap_or(100),
            ViewMode::List => (self.row / Self::default().row * 100.0).round() as u32,
        }
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
                let top = GRID_PADDING + row * (self.tile_h + ROW_GAP);
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
                let tile_w = self.tile_width(width);
                let (pitch_x, pitch_y) = (tile_w + TILE_GAP, self.tile_h + ROW_GAP);
                let (column, row) = ((x / pitch_x).floor(), (y / pitch_y).floor());
                let inside = x - column * pitch_x <= tile_w && y - row * pitch_y <= self.tile_h;
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
    pub fn origin(&self, view: ViewMode, index: usize, width: f32) -> (f32, f32) {
        match view {
            ViewMode::Grid => {
                let columns = self.columns(width);
                let (column, row) = ((index % columns) as f32, (index / columns) as f32);
                let pitch_x = self.tile_width(width) + TILE_GAP;
                (GRID_PADDING + column * pitch_x, GRID_PADDING + row * (self.tile_h + ROW_GAP))
            }
            ViewMode::List => (0.0, LIST_PADDING + index as f32 * self.row),
        }
    }

    #[must_use]
    pub fn entries_in(&self, view: ViewMode, width: f32, count: usize, from: (f32, f32), to: (f32, f32)) -> Vec<usize> {
        let (left, right) = (from.0.min(to.0), from.0.max(to.0));
        let (top, bottom) = (from.1.min(to.1), from.1.max(to.1));
        let (w, h) = match view {
            ViewMode::Grid => (self.tile_width(width), self.tile_h),
            ViewMode::List => (width, self.row),
        };
        (0..count)
            .filter(|&index| {
                let (x, y) = self.origin(view, index, width);
                x < right && x + w > left && y < bottom && y + h > top
            })
            .collect()
    }
}

pub const LIST_HEADER: f32 = glide_sdk::widget::TABLE_HEADER + 1.0;

const TAGS_WIDTH: f32 = 140.0;
const SORT_ARROW: f32 = 11.0 + theme::SP1;

const CELL_PADDING: f32 = theme::SP3;

const fn cell_style(column: ListColumn) -> TextStyle {
    match column {
        ListColumn::Size | ListColumn::Modified | ListColumn::Created | ListColumn::Permissions => TextStyle::NUM,
        ListColumn::Kind | ListColumn::Tags => TextStyle::CALL,
    }
}

#[must_use]
pub fn column_width(column: ListColumn, lang: Lang) -> f32 {
    let style = cell_style(column);
    let widest = |samples: &[String]| samples.iter().map(|sample| measure(sample, style)).fold(0.0, f32::max);
    let values = match column {
        ListColumn::Size => widest(&widest_sizes(lang)),
        ListColumn::Modified | ListColumn::Created => widest(&widest_days(lang)),
        ListColumn::Kind => widest(&FileCategory::ALL.map(|category| category.name(lang).to_string())),
        ListColumn::Permissions => measure(&permissions_string(0o40777), style),
        ListColumn::Tags => return TAGS_WIDTH,
    };
    let label = measure(&column_label(column, lang).to_uppercase(), TextStyle::MICRO) + SORT_ARROW;
    (values.max(label) + 2.0 * CELL_PADDING + 1.0).ceil()
}

#[must_use]
pub(crate) fn column_label(column: ListColumn, lang: crate::i18n::Lang) -> &'static str {
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
pub(crate) fn tag_color(color: tags::Color) -> Color {
    let [r, g, b] = color.rgb();
    Color::from_rgb8(r, g, b)
}

fn dots<'a>(entry: &FileEntry) -> Option<Element<'a, Message>> {
    let colors = tags::colors(&entry.tags);
    (!colors.is_empty())
        .then(|| Row::with_children(colors.into_iter().map(|color| dot(tag_color(color), DOT))).spacing(DOT_GAP).into())
}

#[must_use]
pub fn marks_width(entry: &FileEntry) -> f32 {
    let dots = tags::colors(&entry.tags).len() as f32;
    let dots = if dots > 0.0 { theme::SP2 + dots * DOT + (dots - 1.0) * DOT_GAP } else { 0.0 };
    dots + if entry.is_symlink { theme::SP2 + LINK_MARK } else { 0.0 }
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
    let place = (&app.current, matches!(app.view, ViewMode::List), app.is_searching(), app.entries.is_empty());
    let body = glide_sdk::widget::swapped(place, Some(app.theme.view_bg()), body);
    let body =
        mouse_area(body).on_press(Message::PressedBackground).on_right_press(Message::MenuTarget(MenuKind::Background));
    sensor(body).on_show(Message::PaneResized).on_resize(Message::PaneResized).into()
}

fn empty_state(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{empty_state, tonal};
    let paint = super::paint(&app.theme);
    let lang = app.lang;
    if let Some(search) = &app.search {
        if search.pending {
            return empty_state(&paint, Icon::Search, Key::Searching.text(lang), "", None);
        }
        let place = app.location_name(&app.current);
        let body = crate::i18n::nothing_found(&place, search.query.trim(), lang);
        let elsewhere = crate::fs::home_dir()
            .filter(|home| *home != app.current)
            .map(|_| tonal(&paint, Key::SearchHome.text(lang), Message::SearchHome));
        return empty_state(&paint, Icon::Search, Key::NoSearchResults.text(lang), body, elsewhere);
    }
    if app.in_trash() {
        return empty_state(&paint, Icon::Trash, Key::TrashEmpty.text(lang), Key::TrashEmptyHint.text(lang), None);
    }
    let offered = app.chooser.as_ref().is_none_or(crate::chooser::State::new_folder_offered);
    let create = offered.then(|| tonal(&paint, Key::NewFolder.text(lang), Message::NewFolder));
    empty_state(&paint, Icon::Folder, Key::EmptyFolder.text(lang), Key::EmptyFolderHint.text(lang), create)
}

fn files<'a>(app: &'a App, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    glide_sdk::widget::overlay_scrollable(&paint(&app.theme), content)
        .id(FILES_ID)
        .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
        .width(Fill)
        .height(Fill)
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

fn rename_field(app: &App, style: TextStyle) -> Option<Element<'_, Message>> {
    let renaming = app.renaming.as_ref()?;
    let input =
        text_input("", &renaming.text).id(RENAME_ID).on_input(Message::RenameInput).on_submit(Message::RenameSubmit);
    Some(glide_sdk::widget::inline_field(&super::paint(&app.theme), input, style))
}

fn grid(app: &App) -> Element<'_, Message> {
    let metrics = app.metrics();
    let columns = metrics.columns(app.pane.width);
    let tile_w = metrics.tile_width(app.pane.width);
    let pitch = metrics.tile_h + ROW_GAP;
    let total = app.entries.len().div_ceil(columns);
    let shown = visible_rows(total, pitch, GRID_PADDING, app.scroll_offset(), app.pane.height);
    let rows = app.entries.chunks(columns).skip(shown.start).take(shown.len()).map(|chunk| {
        container(Row::with_children(chunk.iter().map(|entry| tile(app, entry, &metrics, tile_w))).spacing(TILE_GAP))
            .height(pitch)
            .into()
    });
    let body = std::iter::once(spacer(shown.start as f32 * pitch))
        .chain(rows)
        .chain(std::iter::once(spacer((total - shown.end) as f32 * pitch)));
    files(app, Column::with_children(body).padding(GRID_PADDING).width(Fill))
}

fn tile<'a>(app: &'a App, entry: &'a FileEntry, metrics: &Metrics, tile_w: f32) -> Element<'a, Message> {
    let paint = paint(&app.theme);
    let selected = app.selected_level(&entry.path);
    let key = Hover::Entry(entry.path.clone());
    let badge = entry.is_symlink.then(|| corner_badge(&paint, Icon::Link, 10.0));
    let editing = app.renaming.as_ref().is_some_and(|renaming| renaming.path == entry.path);
    let style = TextStyle { line_height_px: LABEL_LINE, ..TextStyle::CALL };
    let room = tile_w - 2.0 * theme::SP1;
    let label: Element<'a, Message> = if let Some(field) = rename_field(app, style).filter(|_| editing) {
        container(field).width(Length::Fixed(room)).into()
    } else {
        let style = if selected > 0.5 { TextStyle { weight: 560, ..style } } else { style };
        let dotted = tags::colors(&entry.tags).len() as f32;
        let marks = if dotted > 0.0 { dotted * DOT + (dotted - 1.0) * DOT_GAP + theme::SP1 } else { 0.0 };
        let name = text(elide_lines(&entry.name, room - marks, style, LABEL_LINES), style)
            .wrapping(Wrapping::WordOrGlyph)
            .align_x(Alignment::Center)
            .color(item_ink(&paint, selected, false));
        let mut line = row![name].spacing(theme::SP1).align_y(Alignment::Center);
        if let Some(dots) = dots(entry) {
            line = line.push(dots);
        }
        line.into()
    };
    let art = file_icon(app, entry, metrics.icon);
    let state = (app.hover(&key), app.lift(&key));
    let tile = item_tile(&paint, art, badge, label, metrics.icon, (tile_w, metrics.tile_h), selected, state);
    mouse_area(tile)
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
    let room = name_width(app, &metrics);
    let columns = sized_columns(app);
    let rows = app
        .entries
        .iter()
        .enumerate()
        .skip(shown.start)
        .take(shown.len())
        .map(|(_, entry)| list_row(app, entry, now, &metrics, room, &columns));
    let body = std::iter::once(spacer(shown.start as f32 * metrics.row))
        .chain(rows)
        .chain(std::iter::once(spacer((total - shown.end) as f32 * metrics.row)));
    let rows = Column::with_children(body).padding(Padding::ZERO.vertical(LIST_PADDING)).width(Fill);
    column![list_header(app), divider(&app.theme), files(app, rows)].into()
}

fn list_header(app: &App) -> Element<'_, Message> {
    let lang = app.lang;
    let mut cells = vec![header_cell(app, SortBy::Name.label(lang), Some(SortBy::Name), None, Fill)];
    for (column, width) in sized_columns(app) {
        let width = Length::Fixed(width);
        cells.push(header_cell(app, column_label(column, lang), column.sort(), Some(column), width));
    }
    if app.is_searching() {
        cells.push(header_cell(app, Key::ColLocation.text(lang), None, None, Length::Fixed(LOCATION_WIDTH)));
    }
    let header = glide_sdk::widget::table_header(&super::paint(&app.theme), cells);
    mouse_area(header).on_right_press(Message::MenuTarget(MenuKind::Columns)).into()
}

fn header_cell<'a>(
    app: &'a App, label: &'a str, sort: Option<SortBy>, column: Option<ListColumn>, width: Length,
) -> Element<'a, Message> {
    let active = sort.is_some() && sort == Some(app.sort.by);
    let mut cell = glide_sdk::widget::HeaderCell::new(&super::paint(&app.theme), label)
        .width(width)
        .end(column == Some(ListColumn::Size));
    if active {
        cell = cell.sort_mark(if app.sort.ascending { Icon::ArrowUp } else { Icon::ArrowDown });
    }
    match column {
        Some(column) => mouse_area(Element::from(
            cell.target(app.is_column_target(column)).lit(app.hover(&Hover::Column(column)) > 0.5),
        ))
        .on_press(Message::ColumnPressed(column))
        .on_enter(Message::Hover(Hover::Column(column), true))
        .on_exit(Message::Hover(Hover::Column(column), false))
        .into(),
        None => cell.on_press(sort.map(Message::SortBy)).into(),
    }
}

fn column_value(app: &App, entry: &FileEntry, column: ListColumn, now: SystemTime) -> String {
    let lang = app.lang;
    let time = |stamp: Option<SystemTime>| stamp.map_or_else(|| String::from("—"), |time| format_day(time, now, lang));
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
    app: &'a App, entry: &'a FileEntry, now: SystemTime, metrics: &Metrics, room: f32, columns: &[(ListColumn, f32)],
) -> Element<'a, Message> {
    let paint = paint(&app.theme);
    let selected = app.selected_level(&entry.path);
    let key = Hover::Entry(entry.path.clone());
    let hover = app.hover(&key);
    let secondary = item_ink(&paint, selected, true);
    let cell = move |value: String, width: f32, style: TextStyle, end: bool| {
        let value = text(value, style).color(secondary).wrapping(Wrapping::None);
        let value = container(value).width(Fill);
        let value = if end { value.align_right(Fill) } else { value };
        container(value).padding(Padding::ZERO.horizontal(CELL_PADDING)).width(Length::Fixed(width)).clip(true)
    };

    let editing = app.renaming.as_ref().is_some_and(|renaming| renaming.path == entry.path);
    let style = if selected > 0.5 { TextStyle { weight: 560, ..TextStyle::CALL } } else { TextStyle::CALL };
    let title: Element<'a, Message> = if let Some(field) = rename_field(app, TextStyle::CALL).filter(|_| editing) {
        field
    } else {
        let room = room - marks_width(entry);
        let shown = elide(&entry.name, room, style);
        container(text(shown, style).wrapping(Wrapping::None)).max_width(room).clip(true).into()
    };
    let mut name = row![file_icon(app, entry, metrics.row_icon), title].spacing(theme::SP2).align_y(Alignment::Center);
    if let Some(dots) = dots(entry) {
        name = name.push(dots);
    }
    if entry.is_symlink {
        name = name.push(super::icon(Icon::Link, LINK_MARK, secondary));
    }

    let name = container(name).padding(Padding::ZERO.horizontal(CELL_PADDING)).width(Fill).clip(true);
    let mut cells = row![name].align_y(Alignment::Center);
    for &(column, width) in columns {
        let value = column_value(app, entry, column, now);
        cells = cells.push(cell(value, width, cell_style(column), column == ListColumn::Size));
    }
    if app.is_searching() {
        let room = LOCATION_WIDTH - 2.0 * CELL_PADDING;
        let shown = elide(&location(app, entry), room, TextStyle::CALL);
        cells = cells.push(cell(shown, LOCATION_WIDTH, TextStyle::CALL, false));
    }

    mouse_area(table_row(&paint, cells, selected, false, hover, metrics.row))
        .on_press(Message::Pressed(entry.path.clone()))
        .on_right_press(Message::Secondary(entry.path.clone()))
        .on_enter(Message::Hover(key.clone(), true))
        .on_exit(Message::Hover(key, false))
        .into()
}

fn location(app: &App, entry: &FileEntry) -> String {
    let parent = entry.path.parent().unwrap_or(&app.current);
    let Ok(relative) = parent.strip_prefix(&app.current) else {
        return parent.display().to_string();
    };
    if relative.as_os_str().is_empty() {
        return app.location_name(&app.current);
    }
    let mut dir = app.current.clone();
    let names: Vec<String> = relative
        .iter()
        .map(|component| {
            dir.push(component);
            app.location_name(&dir)
        })
        .collect();
    names.join("/")
}

fn name_width(app: &App, metrics: &Metrics) -> f32 {
    let columns: f32 = sized_columns(app).iter().map(|&(_, width)| width).sum();
    let location = if app.is_searching() { LOCATION_WIDTH } else { 0.0 };
    (app.pane.width - 2.0 * CELL_PADDING - theme::SP2 - columns - location - metrics.row_icon).max(NAME_MIN)
}

#[must_use]
pub fn shown_columns(app: &App) -> Vec<ListColumn> {
    sized_columns(app).into_iter().map(|(column, _)| column).collect()
}

fn sized_columns(app: &App) -> Vec<(ListColumn, f32)> {
    let location = if app.is_searching() { LOCATION_WIDTH } else { 0.0 };
    let mut room = app.pane.width - location - NAME_ROOM;
    app.columns
        .iter()
        .map(|&column| (column, column_width(column, app.lang)))
        .take_while(|&(_, width)| {
            room -= width;
            room >= 0.0
        })
        .collect()
}
