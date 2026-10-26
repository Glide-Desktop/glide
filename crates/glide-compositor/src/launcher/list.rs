use std::collections::{HashMap, HashSet};

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size},
};

use super::{
    Canvas, DOTS_BAND, Frame, Launcher, LauncherTheme, TextAlign, cached, ensure_tile, filled,
    search::{Entry, Section, SettingsPanel, SystemCommand},
    search_rect,
};
use crate::icon::IconLoader;

const LIST_WIDTH: i32 = 720;
const LIST_MIN_WIDTH: i32 = 280;
pub(super) const ROW_HEIGHT: i32 = 56;
pub(super) const HEADER_HEIGHT: i32 = 32;
const ROW_ICON: i32 = 40;
const ROW_PAD_X: i32 = glide_theme::spacing::SP3;
const TEXT_GAP: i32 = glide_theme::spacing::SP3;
const LIST_TOP_GAP: i32 = glide_theme::spacing::SP5;
const ROW_RADIUS: f32 = glide_theme::shape::R3;
const GLYPH_SIDE: f32 = 18.0;
const GLYPH_STROKE: f32 = 1.5;
const TITLE_LINE: i32 = 20;
const SUBTITLE_LINE: i32 = 19;
const SUBTITLE_TEXT: glide_theme::TextStyle = glide_theme::TextStyle::CALL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ListRow {
    Header(Section),
    Item(usize),
}

const fn row_height(row: ListRow) -> i32 {
    match row {
        ListRow::Header(_) => HEADER_HEIGHT,
        ListRow::Item(_) => ROW_HEIGHT,
    }
}

pub(super) fn list_rect(area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let search = search_rect(area);
    let width = LIST_WIDTH.min(area.size.w - glide_theme::spacing::SP4 * 2).max(LIST_MIN_WIDTH);
    let top = search.loc.y + search.size.h + LIST_TOP_GAP;
    let bottom = area.loc.y + area.size.h - DOTS_BAND;
    Rectangle::new(
        Point::from((area.loc.x + (area.size.w - width) / 2, top)),
        Size::from((width, (bottom - top).max(ROW_HEIGHT))),
    )
}

pub(super) fn rows(entries: &[Entry]) -> Vec<ListRow> {
    let mut rows = Vec::with_capacity(entries.len() + 5);
    let mut last = None;
    for (index, entry) in entries.iter().enumerate() {
        let section = entry.section();
        if last != Some(section) {
            rows.push(ListRow::Header(section));
            last = Some(section);
        }
        rows.push(ListRow::Item(index));
    }
    rows
}

pub(super) fn layout(
    entries: &[Entry], area: Rectangle<i32, Logical>, scroll: usize,
) -> Vec<(ListRow, Rectangle<i32, Logical>)> {
    let rect = list_rect(area);
    let bottom = rect.loc.y + rect.size.h;
    let mut y = rect.loc.y;
    rows(entries)
        .into_iter()
        .skip(scroll)
        .map_while(|row| {
            let height = row_height(row);
            if y + height > bottom {
                return None;
            }
            let placed = Rectangle::new(Point::from((rect.loc.x, y)), Size::from((rect.size.w, height)));
            y += height;
            Some((row, placed))
        })
        .collect()
}

fn span(rows: &[ListRow]) -> i32 {
    rows.iter().map(|row| row_height(*row)).sum()
}

fn scroll_to_show(rows: &[ListRow], scroll: usize, target: usize, height: i32) -> usize {
    if target < scroll {
        let header_above = target > 0 && matches!(rows[target - 1], ListRow::Header(_));
        return if header_above { target - 1 } else { target };
    }
    let mut scroll = scroll;
    while scroll < target && span(&rows[scroll..=target]) > height {
        scroll += 1;
    }
    scroll
}

fn max_scroll(rows: &[ListRow], height: i32) -> usize {
    (0..rows.len()).find(|&start| span(&rows[start..]) <= height).unwrap_or(rows.len().saturating_sub(1))
}

impl Launcher {
    pub(super) fn move_list_selection(&mut self, delta: i32) -> bool {
        if self.entries.is_empty() || delta == 0 {
            return false;
        }
        let last = i64::try_from(self.entries.len()).unwrap_or(1) - 1;
        let next = match self.selected {
            None => 0,
            Some(current) => (i64::try_from(current).unwrap_or(0) + i64::from(delta)).clamp(0, last),
        };
        let next = usize::try_from(next).unwrap_or(0);
        if self.selected == Some(next) {
            return false;
        }
        self.selected = Some(next);
        self.pointer_selected = false;
        self.ensure_selection_visible();
        self.sync_hover_to_selection();
        true
    }

    pub(super) fn ensure_selection_visible(&mut self) {
        if !self.is_listing() {
            return;
        }
        let Some(selected) = self.selected else {
            return;
        };
        let rows = rows(&self.entries);
        let Some(target) = rows.iter().position(|row| *row == ListRow::Item(selected)) else {
            return;
        };
        let height = list_rect(self.area).size.h;
        self.list_scroll = scroll_to_show(&rows, self.list_scroll, target, height);
    }

    pub(super) fn scroll_list(&mut self, forward: bool) -> bool {
        let rows = rows(&self.entries);
        let limit = max_scroll(&rows, list_rect(self.area).size.h);
        let next = if forward { (self.list_scroll + 1).min(limit) } else { self.list_scroll.saturating_sub(1) };
        if next == self.list_scroll {
            return false;
        }
        self.list_scroll = next;
        true
    }

    #[must_use]
    pub fn entry_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
        if !self.is_listing() {
            return None;
        }
        layout(&self.entries, area, self.list_scroll).into_iter().find_map(|(row, rect)| match row {
            ListRow::Item(index) if rect.to_f64().contains(pos) => Some(index),
            _ => None,
        })
    }

    #[must_use]
    pub fn hit_at(&self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> Option<usize> {
        if self.is_listing() { self.entry_at(area, pos) } else { self.app_at(area, pos) }
    }

    fn entry_text(&self, entry: &Entry) -> (String, String) {
        match entry {
            Entry::App(index) => self.apps.get(*index).map_or_else(Default::default, |app| {
                let subtitle = if !app.generic_name.is_empty() && app.generic_name != app.name {
                    app.generic_name.clone()
                } else {
                    app.comment.clone()
                };
                (app.name.clone(), subtitle)
            }),
            Entry::Action { app, action } => self
                .apps
                .get(*app)
                .and_then(|owner| {
                    let action = owner.actions.get(*action)?;
                    Some((action.name.clone(), owner.name.clone()))
                })
                .unwrap_or_default(),
            Entry::Calculator { expression, value } => (
                format!("= {value}"),
                format!("{expression} \u{b7} {}", crate::i18n::t(crate::i18n::Key::CalculatorCopyHint)),
            ),
            Entry::Command(command) => (command.title().to_string(), String::new()),
            Entry::Settings(panel) => (panel.title().to_string(), String::new()),
            Entry::Window { index, app } => self.windows.get(*index).map_or_else(Default::default, |window| {
                let owner = app.and_then(|app| self.apps.get(app)).map_or(window.app_id.as_str(), |app| &app.name);
                let title = if window.title.is_empty() { owner } else { &window.title };
                (title.to_string(), owner.to_string())
            }),
            Entry::Run(command) => (command.clone(), crate::i18n::t(crate::i18n::Key::RunHint).to_string()),
            Entry::File(index) => {
                self.recent.get(*index).map_or_else(Default::default, |file| (file.name.clone(), file.folder.clone()))
            }
            Entry::Provided { index, app } => self.provided.get(*index).map_or_else(Default::default, |found| {
                let owner = app.and_then(|app| self.apps.get(app)).map_or(found.desktop_id.as_str(), |app| &app.name);
                let subtitle = if found.description.is_empty() {
                    owner.to_string()
                } else {
                    format!("{} · {owner}", found.description)
                };
                (found.name.clone(), subtitle)
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RowGlyph {
    Calculator,
    Window,
    File,
    Run,
    Command(SystemCommand),
    Settings(SettingsPanel),
}

enum RowIcon {
    Tile(usize),
    Glyph(RowGlyph),
}

const fn row_icon(entry: &Entry) -> RowIcon {
    match entry {
        Entry::App(index) => RowIcon::Tile(*index),
        Entry::Action { app, .. } | Entry::Window { app: Some(app), .. } | Entry::Provided { app: Some(app), .. } => {
            RowIcon::Tile(*app)
        }
        Entry::Window { app: None, .. } => RowIcon::Glyph(RowGlyph::Window),
        Entry::File(_) | Entry::Provided { app: None, .. } => RowIcon::Glyph(RowGlyph::File),
        Entry::Run(_) => RowIcon::Glyph(RowGlyph::Run),
        Entry::Calculator { .. } => RowIcon::Glyph(RowGlyph::Calculator),
        Entry::Command(command) => RowIcon::Glyph(RowGlyph::Command(*command)),
        Entry::Settings(panel) => RowIcon::Glyph(RowGlyph::Settings(*panel)),
    }
}

type RowKey = (String, String, i32, i32);

#[derive(Debug, Default)]
pub(super) struct ListCache {
    rows: HashMap<RowKey, MemoryRenderBuffer>,
    headers: HashMap<(String, i32, i32), MemoryRenderBuffer>,
    glyphs: HashMap<(RowGlyph, i32), MemoryRenderBuffer>,
    layer: HashMap<(i32, i32), MemoryRenderBuffer>,
}

fn draw_glyph(
    painter: &crate::paint::Painter, theme: &LauncherTheme, glyph: RowGlyph, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = filled(ROW_ICON, ROW_ICON, scale, theme.glyph_tile, glide_theme::shape::R2)?;
    paint_glyph(&mut c.buf, painter, glyph, theme.glyph, scale);
    Some(c.finish())
}

fn paint_glyph(buf: &mut [u8], painter: &crate::paint::Painter, glyph: RowGlyph, fg: glide_theme::Color, scale: i32) {
    use crate::paint as p;
    let (w, h) = (ROW_ICON, ROW_ICON);
    let stroked: fn(&mut [u8], i32, i32, f32, f32, i32, glide_theme::Color) = match glyph {
        RowGlyph::Calculator | RowGlyph::Window | RowGlyph::File | RowGlyph::Run => {
            let label = match glyph {
                RowGlyph::Window => "\u{25a1}",
                RowGlyph::File => "\u{2261}",
                RowGlyph::Run => ">_",
                _ => "=",
            };
            return painter.draw_label_over_styled(
                buf,
                w,
                h,
                0,
                w,
                scale,
                p::TextAlign::Center,
                label,
                fg.to_buffer_bytes(),
                &glide_theme::TextStyle::T3,
            );
        }
        RowGlyph::Command(SystemCommand::LogOut) => {
            return p::draw_close_glyph_over(buf, w, h, 10.0, GLYPH_STROKE, scale, fg);
        }
        RowGlyph::Command(SystemCommand::Sleep)
        | RowGlyph::Settings(SettingsPanel::Appearance | SettingsPanel::NightLight) => {
            return p::draw_moon_glyph_over(buf, w, h, GLYPH_SIDE, scale, fg);
        }
        RowGlyph::Settings(SettingsPanel::Sound) => {
            return p::draw_speaker_glyph_over(buf, w, h, GLYPH_SIDE, GLYPH_STROKE, false, scale, fg);
        }
        RowGlyph::Command(SystemCommand::Lock)
        | RowGlyph::Settings(SettingsPanel::Vpn | SettingsPanel::Netbird | SettingsPanel::Privacy) => {
            p::draw_lock_glyph_over
        }
        RowGlyph::Command(SystemCommand::Restart | SystemCommand::ShutDown)
        | RowGlyph::Settings(SettingsPanel::Power) => p::draw_power_glyph_over,
        RowGlyph::Settings(SettingsPanel::Network) | RowGlyph::Command(SystemCommand::ScreenMirroring) => {
            p::draw_wifi_waves_glyph_over
        }
        RowGlyph::Settings(SettingsPanel::Bluetooth) => p::draw_bluetooth_glyph_over,
        RowGlyph::Settings(SettingsPanel::Displays) => p::draw_sun_glyph_over,
        RowGlyph::Settings(SettingsPanel::Keyboard | SettingsPanel::Shortcuts) => p::draw_keyboard_glyph_over,
        RowGlyph::Settings(
            SettingsPanel::Notifications
            | SettingsPanel::Wallpaper
            | SettingsPanel::Desktop
            | SettingsPanel::TopBar
            | SettingsPanel::Windows
            | SettingsPanel::Multitasking
            | SettingsPanel::Printers
            | SettingsPanel::Mouse
            | SettingsPanel::Gestures
            | SettingsPanel::Accessibility
            | SettingsPanel::Users
            | SettingsPanel::DateTime
            | SettingsPanel::Region
            | SettingsPanel::Apps
            | SettingsPanel::Startup
            | SettingsPanel::Storage
            | SettingsPanel::About,
        ) => p::draw_gear_glyph_over,
    };
    stroked(buf, w, h, GLYPH_SIDE, GLYPH_STROKE, scale, fg);
}

fn text_width(list_width: i32) -> i32 {
    (list_width - ROW_PAD_X * 2 - ROW_ICON - TEXT_GAP).max(1)
}

fn draw_row_text(
    painter: &crate::paint::Painter, theme: &LauncherTheme, title: &str, subtitle: &str, width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, ROW_HEIGHT, scale)?;
    let (left, head) = (TextAlign::Left, &glide_theme::TextStyle::HEAD);
    if subtitle.is_empty() {
        c.label(painter, (0, width), left, title, theme.row_title, head);
    } else {
        let top = (ROW_HEIGHT - TITLE_LINE - SUBTITLE_LINE) / 2;
        let middle = top + TITLE_LINE;
        c.text(painter, (0, width, top, middle), left, title, theme.row_title, head);
        c.text(painter, (0, width, middle, middle + SUBTITLE_LINE), left, subtitle, theme.row_subtitle, &SUBTITLE_TEXT);
    }
    Some(c.finish())
}

fn draw_header(
    painter: &crate::paint::Painter, theme: &LauncherTheme, label: &str, width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, HEADER_HEIGHT, scale)?;
    let bottom = HEADER_HEIGHT - glide_theme::spacing::SP1;
    let band = (ROW_PAD_X, width - ROW_PAD_X, bottom - glide_theme::spacing::SP4, bottom);
    c.text(painter, band, TextAlign::Left, label, theme.section_label, &glide_theme::TextStyle::MICRO);
    Some(c.finish())
}

fn draw_layer(theme: &LauncherTheme, width: i32, scale: i32) -> Option<MemoryRenderBuffer> {
    Some(filled(width, ROW_HEIGHT, scale, theme.cell_layer, ROW_RADIUS)?.finish())
}

pub(super) fn push_list<R>(launcher: &mut Launcher, frame: &mut Frame<'_, R>, icons: &mut IconLoader)
where
    R: crate::raster::ShellRenderer,
{
    let (painter, theme, output_scale) = (frame.painter, frame.theme, frame.output_scale);
    let placed = layout(&launcher.entries, frame.area, launcher.list_scroll);
    let list_width = list_rect(frame.area).size.w;
    let text_w = text_width(list_width);
    let mut used_rows: HashSet<RowKey> = HashSet::new();
    let mut used_headers: HashSet<(String, i32, i32)> = HashSet::new();

    for (row, rect) in placed {
        let top = Point::from((rect.loc.x, rect.loc.y + frame.rise));
        match row {
            ListRow::Header(section) => {
                let key = (section.label().to_string(), list_width, output_scale);
                used_headers.insert(key.clone());
                if let Some(buffer) = cached(&mut launcher.list_cache.headers, key, || {
                    draw_header(painter, theme, section.label(), list_width, output_scale)
                }) {
                    frame.push(buffer, top, None, frame.alpha);
                }
            }
            ListRow::Item(index) => {
                let Some(entry) = launcher.entries.get(index).cloned() else {
                    continue;
                };
                let (title, subtitle) = launcher.entry_text(&entry);
                let icon_loc = Point::from((top.x + ROW_PAD_X, top.y + (ROW_HEIGHT - ROW_ICON) / 2));
                match row_icon(&entry) {
                    RowIcon::Tile(app) => {
                        ensure_tile(launcher, icons, painter, theme, app, output_scale);
                        if let Some(buffer) = launcher.tiles.get(app).and_then(|cache| cache.get(&output_scale)) {
                            let size = Some((
                                Size::from((super::ICON_SIZE, super::ICON_SIZE)),
                                Size::from((ROW_ICON, ROW_ICON)),
                            ));
                            frame.push(buffer, icon_loc, size, frame.alpha);
                        }
                    }
                    RowIcon::Glyph(glyph) => {
                        if let Some(buffer) = cached(&mut launcher.list_cache.glyphs, (glyph, output_scale), || {
                            draw_glyph(painter, theme, glyph, output_scale)
                        }) {
                            frame.push(buffer, icon_loc, None, frame.alpha);
                        }
                    }
                }

                let key = (title, subtitle, text_w, output_scale);
                used_rows.insert(key.clone());
                if let Some(buffer) = cached(&mut launcher.list_cache.rows, key.clone(), || {
                    draw_row_text(painter, theme, &key.0, &key.1, text_w, output_scale)
                }) {
                    let text_loc = Point::from((top.x + ROW_PAD_X + ROW_ICON + TEXT_GAP, top.y));
                    frame.push(buffer, text_loc, None, frame.alpha);
                }

                let layer_alpha = frame.alpha * launcher.row_layer_alpha(index) as f32;
                if layer_alpha > 0.0
                    && let Some(buffer) = cached(&mut launcher.list_cache.layer, (list_width, output_scale), || {
                        draw_layer(theme, list_width, output_scale)
                    })
                {
                    frame.push(buffer, top, None, layer_alpha);
                }
            }
        }
    }

    launcher.list_cache.rows.retain(|key, _| used_rows.contains(key) || key.3 != output_scale);
    launcher.list_cache.headers.retain(|key, _| used_headers.contains(key) || key.2 != output_scale);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::DesktopApp;

    fn area() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1000)))
    }

    fn apps(names: &[&str]) -> Vec<DesktopApp> {
        names
            .iter()
            .map(|name| DesktopApp {
                id: name.to_lowercase(),
                name: (*name).to_string(),
                command: "true".to_string(),
                ..DesktopApp::default()
            })
            .collect()
    }

    fn typed(launcher: &mut Launcher, text: &str) {
        for ch in text.chars() {
            launcher.push_query(ch);
        }
    }

    fn many(count: usize) -> Launcher {
        let names: Vec<String> = (0..count).map(|i| format!("Item {i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        Launcher::new(apps(&refs), area())
    }

    #[test]
    fn every_section_opens_with_a_header() {
        let entries = vec![Entry::App(0), Entry::App(1), Entry::Command(SystemCommand::Lock)];
        assert_eq!(
            rows(&entries),
            [
                ListRow::Header(Section::Apps),
                ListRow::Item(0),
                ListRow::Item(1),
                ListRow::Header(Section::Commands),
                ListRow::Item(2),
            ]
        );
    }

    #[test]
    fn the_list_sits_below_the_search_field() {
        let search = search_rect(area());
        let list = list_rect(area());
        assert!(list.loc.y >= search.loc.y + search.size.h);
        let left = list.loc.x;
        let right = area().size.w - (list.loc.x + list.size.w);
        assert!((left - right).abs() <= 1);
    }

    #[test]
    fn typing_switches_to_the_list_and_clearing_back_to_the_grid() {
        let mut launcher = Launcher::new(apps(&["Files", "Firefox"]), area());
        assert!(!launcher.is_listing());
        typed(&mut launcher, "fi");
        assert!(launcher.is_listing());
        assert_eq!(launcher.selected_entry(), Some(Entry::App(0)));
        launcher.clear_text();
        assert!(!launcher.is_listing());
        assert_eq!(launcher.selected(), None);
    }

    #[test]
    fn a_row_click_lands_on_the_entry_under_the_pointer() {
        let mut launcher = Launcher::new(apps(&["Files", "Firefox"]), area());
        typed(&mut launcher, "fi");
        for (row, rect) in layout(&launcher.entries, area(), 0) {
            let centre =
                Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)));
            let expected = match row {
                ListRow::Item(index) => Some(index),
                ListRow::Header(_) => None,
            };
            assert_eq!(launcher.entry_at(area(), centre), expected);
            assert_eq!(launcher.hit_at(area(), centre), expected);
        }
        assert_eq!(launcher.app_at(area(), Point::from((960.0, 400.0))), None);
    }

    #[test]
    fn up_and_down_walk_the_entries_and_stop_at_the_ends() {
        let mut launcher = Launcher::new(apps(&["Fire", "Firefox", "Firebird"]), area());
        typed(&mut launcher, "fir");
        assert_eq!(launcher.entries.len(), 3);
        assert!(!launcher.move_selection(0, -1), "already at the first entry");
        assert!(launcher.move_selection(0, 1));
        assert_eq!(launcher.selected(), Some(1));
        assert!(launcher.move_selection(1, 0), "Tab steps like Down");
        assert!(!launcher.move_selection(0, 1));
    }

    #[test]
    fn walking_past_the_bottom_scrolls_the_selection_into_view() {
        let mut launcher = many(40);
        typed(&mut launcher, "item");
        let shown = crate::launcher::search::LIST_APP_LIMIT;
        assert_eq!(launcher.entries.len(), shown);
        let small = Rectangle::new(Point::from((0, 0)), Size::from((1920, 420)));
        launcher.set_area(small);
        for _ in 0..shown {
            launcher.move_selection(0, 1);
        }
        let selected = launcher.selected().expect("selection");
        let visible: Vec<ListRow> =
            layout(&launcher.entries, small, launcher.list_scroll).into_iter().map(|(row, _)| row).collect();
        assert!(launcher.list_scroll > 0, "the list had to scroll");
        assert!(visible.contains(&ListRow::Item(selected)));

        for _ in 0..shown {
            launcher.move_selection(0, -1);
        }
        assert_eq!(launcher.list_scroll, 0, "the header comes back into view too");
    }

    #[test]
    fn the_wheel_scrolls_the_list_within_bounds() {
        let mut launcher = many(40);
        typed(&mut launcher, "item");
        launcher.set_area(Rectangle::new(Point::from((0, 0)), Size::from((1920, 420))));
        assert!(!launcher.scroll_list(false));
        let mut steps = 0;
        while launcher.scroll_list(true) {
            steps += 1;
            assert!(steps < 100);
        }
        assert!(steps > 0);
        let rows = rows(&launcher.entries);
        let height = list_rect(launcher.area).size.h;
        assert!(span(&rows[launcher.list_scroll..]) <= height);
    }

    #[test]
    fn paging_keys_do_nothing_while_listing() {
        let mut launcher = many(80);
        typed(&mut launcher, "item");
        assert_eq!(launcher.page_count(), 1);
        assert!(!launcher.turn_page(true));
    }

    #[test]
    fn row_text_describes_each_kind_of_entry() {
        let mut list = apps(&["Firefox"]);
        list[0].generic_name = "Web Browser".into();
        list[0].actions = vec![crate::icon::DesktopAction {
            id: "p".into(),
            name: "New Private Window".into(),
            command: "firefox --private-window".into(),
        }];
        let launcher = Launcher::new(list, area());
        assert_eq!(launcher.entry_text(&Entry::App(0)), ("Firefox".to_string(), "Web Browser".to_string()));
        assert_eq!(
            launcher.entry_text(&Entry::Action { app: 0, action: 0 }),
            ("New Private Window".to_string(), "Firefox".to_string())
        );
        let (title, subtitle) =
            launcher.entry_text(&Entry::Calculator { expression: "6*7".into(), value: "42".into() });
        assert_eq!(title, "= 42");
        assert!(subtitle.starts_with("6*7"));
    }

    #[test]
    fn the_run_prompt_lists_only_the_typed_command() {
        let mut launcher = Launcher::new(apps(&["Firefox"]), area());
        launcher.begin_run_mode();
        assert!(launcher.is_listing(), "an empty run prompt hides the app grid");
        assert!(launcher.entries().is_empty());
        launcher.insert_text("firefox");
        assert_eq!(launcher.entries(), [Entry::Run("firefox".into())], "no app matches, only the command");
        assert_eq!(launcher.entry_text(&Entry::Run("firefox".into())).0, "firefox");
    }

    #[test]
    fn a_window_row_shows_its_title_over_its_app() {
        let mut launcher = Launcher::new(apps(&["Firefox"]), area());
        let window = |id, title: &str, app_id: &str| super::super::search::OpenWindow {
            id,
            title: title.into(),
            app_id: app_id.into(),
        };
        launcher.set_windows(vec![window(3, "Inbox", "firefox"), window(4, "", "foot")]);
        assert_eq!(
            launcher.entry_text(&Entry::Window { index: 0, app: Some(0) }),
            ("Inbox".to_string(), "Firefox".to_string())
        );
        assert_eq!(
            launcher.entry_text(&Entry::Window { index: 1, app: None }),
            ("foot".to_string(), "foot".to_string())
        );
        assert!(matches!(row_icon(&Entry::Window { index: 0, app: Some(0) }), RowIcon::Tile(0)));
        assert!(matches!(row_icon(&Entry::Window { index: 1, app: None }), RowIcon::Glyph(RowGlyph::Window)));
    }
}
