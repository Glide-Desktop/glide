use glide_appmenu::{Activation, MenuItem, Toggle};
use glide_theme::spacing::{SP1, SP2, SP3, SP5};
use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size},
};

use crate::{
    config::Color,
    i18n::{Key, t},
    paint::TextAlign,
    popup_menu::text_px,
    raster::{Canvas, rect},
};

pub const MAX_DEPTH: usize = 4;

pub const TITLE_PAD_X: i32 = SP2;
pub const PANEL_PAD: i32 = SP1;
pub const ROW_HEIGHT: i32 = 30;
pub const SEPARATOR_HEIGHT: i32 = SP2 + 1;
const ROW_PAD_X: i32 = SP3;
const MARK_WIDTH: i32 = 18;
const ACCEL_GAP: i32 = SP5;
const ARROW_WIDTH: i32 = 14;
const MIN_WIDTH: i32 = 180;
const MAX_WIDTH: i32 = 420;
const EDGE_MARGIN: i32 = SP2;
const PANEL_RADIUS: f32 = glide_theme::shape::R3;
const ROW_RADIUS: f32 = glide_theme::shape::R2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MenuPath {
    len: u8,
    idx: [u16; MAX_DEPTH],
}

impl MenuPath {
    #[must_use]
    pub fn top(index: usize) -> Self {
        let mut idx = [0; MAX_DEPTH];
        idx[0] = u16::try_from(index).unwrap_or(u16::MAX);
        Self { len: 1, idx }
    }

    #[must_use]
    pub fn child(self, index: usize) -> Option<Self> {
        let len = usize::from(self.len);
        if len >= MAX_DEPTH {
            return None;
        }
        let mut next = self;
        next.idx[len] = u16::try_from(index).ok()?;
        next.len += 1;
        Some(next)
    }

    #[must_use]
    pub fn parent(self) -> Option<Self> {
        (self.len > 1).then(|| {
            let mut parent = self;
            parent.len -= 1;
            parent.idx[usize::from(parent.len)] = 0;
            parent
        })
    }

    #[must_use]
    pub fn len(self) -> usize {
        usize::from(self.len)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn top_index(self) -> usize {
        usize::from(self.idx[0])
    }

    #[must_use]
    pub fn last(self) -> usize {
        usize::from(self.idx[usize::from(self.len.max(1)) - 1])
    }

    #[must_use]
    pub fn truncated(self, len: usize) -> Self {
        let mut out = self;
        out.len = u8::try_from(len.min(self.len())).unwrap_or(self.len);
        let kept = out.len();
        out.idx[kept..].fill(0);
        out
    }

    #[must_use]
    pub fn starts_with(self, prefix: Self) -> bool {
        prefix.len <= self.len && self.idx[..prefix.len()] == prefix.idx[..prefix.len()]
    }
}

#[must_use]
pub fn item_at(bar: &[MenuItem], path: MenuPath) -> Option<&MenuItem> {
    if path.is_empty() {
        return None;
    }
    let mut item = bar.get(path.top_index())?;
    for depth in 1..path.len() {
        item = item.children.get(usize::from(path.idx[depth]))?;
    }
    Some(item)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopCommand {
    SystemSettings,
    LockScreen,
    NewTerminal,
    OpenHome,
    Screenshot,
    Overview,
    Launcher,
    DoNotDisturb,
    Caffeine,
    PreviousWorkspace,
    NextWorkspace,
}

impl DesktopCommand {
    const ALL: [Self; 11] = [
        Self::SystemSettings,
        Self::LockScreen,
        Self::NewTerminal,
        Self::OpenHome,
        Self::Screenshot,
        Self::Overview,
        Self::Launcher,
        Self::DoNotDisturb,
        Self::Caffeine,
        Self::PreviousWorkspace,
        Self::NextWorkspace,
    ];

    fn id(self) -> u32 {
        Self::ALL
            .iter()
            .position(|command| *command == self)
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or(u32::MAX)
    }

    #[must_use]
    pub fn from_activation(activation: &Activation) -> Option<Self> {
        match activation {
            Activation::Local(id) => Self::ALL.get(usize::try_from(*id).ok()?).copied(),
            _ => None,
        }
    }

    fn item(self, key: Key) -> MenuItem {
        MenuItem::action(t(key), Activation::Local(self.id()))
    }

    fn toggle(self, key: Key, on: bool) -> MenuItem {
        MenuItem { toggle: Toggle::Check(on), ..self.item(key) }
    }
}

#[must_use]
pub fn desktop_menu(dnd: bool, caffeinated: bool) -> Vec<MenuItem> {
    use DesktopCommand as C;
    vec![
        MenuItem::submenu(
            "Glide",
            vec![
                C::SystemSettings.item(Key::SystemSettings),
                MenuItem::separator(),
                C::LockScreen.item(Key::LockScreen),
            ],
        ),
        MenuItem::submenu(
            t(Key::MenuFile),
            vec![
                C::NewTerminal.item(Key::NewTerminal),
                C::OpenHome.item(Key::OpenHome),
                MenuItem::separator(),
                C::Screenshot.item(Key::Screenshot),
            ],
        ),
        MenuItem::submenu(
            t(Key::MenuView),
            vec![
                C::Overview.item(Key::Overview),
                C::Launcher.item(Key::Applications),
                MenuItem::separator(),
                C::DoNotDisturb.toggle(Key::DoNotDisturb, dnd),
                C::Caffeine.toggle(Key::Caffeine, caffeinated),
            ],
        ),
        MenuItem::submenu(
            t(Key::MenuWorkspaces),
            vec![C::PreviousWorkspace.item(Key::PreviousWorkspace), C::NextWorkspace.item(Key::NextWorkspace)],
        ),
    ]
}

#[must_use]
pub fn terminal_command() -> String {
    glide_config::terminal::current()
}

#[must_use]
pub fn title_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    text_px(painter, label, &glide_theme::TextStyle::TOPBAR) + TITLE_PAD_X * 2
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: MenuPath,
    pub rect: Rectangle<i32, Logical>,
    pub separator: bool,
    pub enabled: bool,
    pub submenu: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    pub parent: MenuPath,
    pub rect: Rectangle<i32, Logical>,
    pub rows: Vec<Row>,
    pub marks: bool,
    pub scroll: i32,
}

fn row_height(item: &MenuItem) -> i32 {
    if item.separator { SEPARATOR_HEIGHT } else { ROW_HEIGHT }
}

fn text_width(painter: &crate::paint::Painter, text: &str) -> i32 {
    text_px(painter, text, &glide_theme::TextStyle::MENU_ITEM)
}

fn panel_size(painter: &crate::paint::Painter, items: &[MenuItem]) -> (Size<i32, Logical>, bool) {
    let marks = items.iter().any(|item| item.toggle != Toggle::None);
    let label_w = items.iter().map(|item| text_width(painter, &item.label)).max().unwrap_or(0);
    let accel_w = items
        .iter()
        .filter_map(|item| item.accel.as_deref())
        .map(|accel| text_width(painter, accel) + ACCEL_GAP)
        .max()
        .unwrap_or(0);
    let arrow_w = if items.iter().any(MenuItem::is_submenu) { ARROW_WIDTH } else { 0 };
    let mark_w = if marks { MARK_WIDTH } else { 0 };
    let width = (PANEL_PAD * 2 + ROW_PAD_X * 2 + mark_w + label_w + accel_w + arrow_w).clamp(MIN_WIDTH, MAX_WIDTH);
    let height = PANEL_PAD * 2 + items.iter().map(row_height).sum::<i32>();
    (Size::from((width, height)), marks)
}

#[must_use]
pub fn scroll_offset(items: &[MenuItem], panel_h: i32, open_row: Option<usize>) -> i32 {
    let viewport = panel_h - PANEL_PAD * 2;
    let content: i32 = items.iter().map(row_height).sum();
    let max_scroll = (content - viewport).max(0);
    let Some(open) = open_row.filter(|_| max_scroll > 0) else {
        return 0;
    };
    let row_bottom: i32 = items.iter().take(open + 1).map(row_height).sum();
    (row_bottom - viewport).clamp(0, max_scroll)
}

fn row_visible(row_y: i32, row_h: i32, panel_h: i32) -> bool {
    row_y >= PANEL_PAD && row_y + row_h <= panel_h - PANEL_PAD
}

#[derive(Debug, Clone, Copy)]
struct Placement {
    origin: Point<i32, Logical>,
    flip_from: Option<i32>,
    open_row: Option<usize>,
    top: i32,
}

fn place_panel(
    painter: &crate::paint::Painter, items: &[MenuItem], parent: MenuPath, placement: Placement,
    bounds: Rectangle<i32, Logical>,
) -> Panel {
    let (natural, marks) = panel_size(painter, items);
    let right_edge = bounds.loc.x + bounds.size.w - EDGE_MARGIN;
    let mut x = placement.origin.x;
    if x + natural.w > right_edge {
        x = placement.flip_from.map_or(right_edge - natural.w, |left| left - natural.w + PANEL_PAD);
    }
    let x = x.max(bounds.loc.x + EDGE_MARGIN);
    let top = placement.top.max(bounds.loc.y);
    let bottom_edge = bounds.loc.y + bounds.size.h - EDGE_MARGIN;
    let available = (bottom_edge - top).max(PANEL_PAD * 2 + ROW_HEIGHT);
    let size = Size::from((natural.w, natural.h.min(available)));
    let y = placement.origin.y.min(bottom_edge - size.h).max(top);
    let rect = Rectangle::new(Point::from((x, y)), size);
    let scroll = scroll_offset(items, size.h, placement.open_row);

    let mut rows = Vec::with_capacity(items.len());
    let mut row_y = PANEL_PAD - scroll;
    for (index, item) in items.iter().enumerate() {
        let h = row_height(item);
        if let Some(path) = parent.child(index) {
            rows.push(Row {
                path,
                rect: Rectangle::new(Point::from((x + PANEL_PAD, y + row_y)), Size::from((size.w - PANEL_PAD * 2, h))),
                separator: item.separator,
                enabled: item.enabled && !item.separator && row_visible(row_y, h, size.h),
                submenu: item.is_submenu(),
            });
        }
        row_y += h;
    }
    Panel { parent, rect, rows, marks, scroll }
}

fn open_row_in(open: MenuPath, parent: MenuPath) -> Option<usize> {
    let depth = parent.len();
    (open.len() > depth && open.starts_with(parent)).then(|| open.truncated(depth + 1).last())
}

#[must_use]
pub fn layout_panels(
    painter: &crate::paint::Painter, bar: &[MenuItem], open: MenuPath, title: Rectangle<i32, Logical>, top: i32,
    bounds: Rectangle<i32, Logical>,
) -> Vec<Panel> {
    let root = open.truncated(1);
    let Some(item) = item_at(bar, root) else {
        return Vec::new();
    };
    if !item.is_submenu() {
        return Vec::new();
    }
    let mut panels = vec![place_panel(
        painter,
        &item.children,
        root,
        Placement { origin: Point::from((title.loc.x, top)), flip_from: None, open_row: open_row_in(open, root), top },
        bounds,
    )];
    for depth in 2..=open.len() {
        let prefix = open.truncated(depth);
        let Some(item) = item_at(bar, prefix).filter(|item| item.is_submenu()) else {
            break;
        };
        let Some(parent) = panels.last() else {
            break;
        };
        let Some(row) = parent.rows.iter().find(|row| row.path == prefix) else {
            break;
        };
        let origin = Point::from((parent.rect.loc.x + parent.rect.size.w - PANEL_PAD, row.rect.loc.y - PANEL_PAD));
        let placement =
            Placement { origin, flip_from: Some(parent.rect.loc.x), open_row: open_row_in(open, prefix), top };
        panels.push(place_panel(painter, &item.children, prefix, placement, bounds));
    }
    panels
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuColors {
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub accel: Color,
    pub disabled: Color,
    pub mark: Color,
    pub open_row: Color,
}

pub struct PanelDraw<'a> {
    pub painter: &'a crate::paint::Painter,
    pub colors: &'a MenuColors,
    pub items: &'a [MenuItem],
    pub marks: bool,
    pub size: Size<i32, Logical>,
    pub open_row: Option<usize>,
    pub scale: i32,
}

fn draw_mark(c: &mut Canvas<'_>, colour: Color, toggle: Toggle, y: i32) {
    let side = 14;
    let x = PANEL_PAD + ROW_PAD_X - SP1;
    match toggle {
        Toggle::Check(true) => c.glyph(rect(x, y + (ROW_HEIGHT - side) / 2, side, side), |buf, w, h, scale| {
            crate::paint::draw_tick_glyph_over(buf, w, h, 13.0, 1.8, scale, colour);
        }),
        Toggle::Radio(true) => {
            let dot = 6;
            c.fill(
                rect(x + (side - dot) / 2, y + (ROW_HEIGHT - dot) / 2, dot, dot),
                colour,
                glide_theme::shape::R_ROUND,
            );
        }
        _ => {}
    }
}

fn draw_row(c: &mut Canvas<'_>, draw: &PanelDraw<'_>, item: &MenuItem, open: bool, y: i32) {
    let w = c.w;
    let row_h = row_height(item);
    if item.separator {
        c.fill(rect(PANEL_PAD + SP2, y + row_h / 2, w - (PANEL_PAD + SP2) * 2, 1), draw.colors.border, 0.0);
        return;
    }
    if open {
        c.fill(rect(PANEL_PAD, y, w - PANEL_PAD * 2, row_h), draw.colors.open_row, ROW_RADIUS);
    }
    draw_mark(c, draw.colors.mark, item.toggle, y);
    let color = if item.enabled { draw.colors.text } else { draw.colors.disabled };
    let text_left = PANEL_PAD + ROW_PAD_X + if draw.marks { MARK_WIDTH } else { 0 };
    let right = w - PANEL_PAD - ROW_PAD_X;
    let mut band = |left: i32, right: i32, align, text: &str, color: Color| {
        c.text(draw.painter, (left, right, y, y + row_h), align, text, color, &glide_theme::TextStyle::MENU_ITEM);
    };
    let mut label_right = right;
    if item.is_submenu() {
        band(right - ARROW_WIDTH, right, TextAlign::Right, "›", color);
        label_right -= ARROW_WIDTH;
    } else if let Some(accel) = item.accel.as_deref() {
        let accel_w = text_width(draw.painter, accel);
        band(right - accel_w, right, TextAlign::Right, accel, draw.colors.accel);
        label_right -= accel_w + ACCEL_GAP;
    }
    band(text_left, label_right.max(text_left + 1), TextAlign::Left, &item.label, color);
}

#[must_use]
pub fn draw_panel(draw: &PanelDraw<'_>) -> Option<MemoryRenderBuffer> {
    let (w, h) = (draw.size.w, draw.size.h);
    let mut c = Canvas::popover((w, h), draw.scale, PANEL_RADIUS, draw.colors.background, draw.colors.border)?;
    let mut y = PANEL_PAD - scroll_offset(draw.items, h, draw.open_row);
    for (index, item) in draw.items.iter().enumerate() {
        let row_h = row_height(item);
        if row_visible(y, row_h, h) {
            draw_row(&mut c, draw, item, draw.open_row == Some(index), y);
        }
        y += row_h;
    }
    Some(c.finish())
}

#[must_use]
pub fn draw_title(
    painter: &crate::paint::Painter, label: &str, width: i32, height: i32, color: Color, open_fill: Option<Color>,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, height, scale)?;
    if let Some(fill) = open_fill {
        c.fill(c.full(), fill, ROW_RADIUS);
    }
    c.label(
        painter,
        (TITLE_PAD_X, width - TITLE_PAD_X),
        TextAlign::Center,
        label,
        color,
        &glide_theme::TextStyle::TOPBAR,
    );
    Some(c.finish())
}

#[must_use]
pub fn step_enabled(panel: &Panel, from: Option<MenuPath>, forward: bool) -> Option<MenuPath> {
    let enabled: Vec<MenuPath> = panel.rows.iter().filter(|row| row.enabled).map(|row| row.path).collect();
    if enabled.is_empty() {
        return None;
    }
    let position = from.and_then(|path| enabled.iter().position(|p| *p == path));
    let next = match (position, forward) {
        (None, true) => 0,
        (None, false) => enabled.len() - 1,
        (Some(i), true) => (i + 1) % enabled.len(),
        (Some(i), false) => (i + enabled.len() - 1) % enabled.len(),
    };
    enabled.get(next).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_walk_down_and_back_up() {
        let top = MenuPath::top(2);
        let child = top.child(3).and_then(|p| p.child(1)).expect("within depth");
        assert_eq!(child.len(), 3);
        assert_eq!(child.top_index(), 2);
        assert_eq!(child.last(), 1);
        assert_eq!(child.parent().and_then(MenuPath::parent), Some(top));
        assert!(child.starts_with(top));
        assert!(!top.starts_with(child));
        assert_eq!(child.truncated(1), top);
    }

    #[test]
    fn a_path_refuses_to_grow_past_the_depth_limit() {
        let mut path = MenuPath::top(0);
        for _ in 1..MAX_DEPTH {
            path = path.child(0).expect("under the limit");
        }
        assert_eq!(path.child(0), None);
    }

    #[test]
    fn items_resolve_through_submenus() {
        let bar = desktop_menu(false, true);
        let caffeine = item_at(&bar, MenuPath::top(2).child(4).expect("depth")).expect("item");
        assert_eq!(caffeine.toggle, Toggle::Check(true));
        assert_eq!(DesktopCommand::from_activation(&caffeine.activation), Some(DesktopCommand::Caffeine));
        assert!(item_at(&bar, MenuPath::top(9)).is_none());
    }

    #[test]
    fn every_desktop_command_round_trips_through_its_activation() {
        for command in DesktopCommand::ALL {
            let activation = Activation::Local(command.id());
            assert_eq!(DesktopCommand::from_activation(&activation), Some(command));
        }
        assert_eq!(DesktopCommand::from_activation(&Activation::DbusMenu(0)), None);
    }

    #[test]
    fn stepping_skips_disabled_rows_and_wraps() {
        let parent = MenuPath::top(0);
        let row = |i: usize, enabled: bool| Row {
            path: parent.child(i).expect("depth"),
            rect: Rectangle::default(),
            separator: !enabled,
            enabled,
            submenu: false,
        };
        let panel = Panel {
            parent,
            rect: Rectangle::default(),
            rows: vec![row(0, true), row(1, false), row(2, true)],
            marks: false,
            scroll: 0,
        };
        let first = step_enabled(&panel, None, true);
        assert_eq!(first, parent.child(0));
        assert_eq!(step_enabled(&panel, first, true), parent.child(2));
        assert_eq!(step_enabled(&panel, parent.child(2), true), parent.child(0));
        assert_eq!(step_enabled(&panel, first, false), parent.child(2));
    }

    fn tall_bar(rows: usize, submenu_at: usize) -> Vec<MenuItem> {
        let children = (0..rows)
            .map(|i| {
                if i == submenu_at {
                    MenuItem::submenu(&format!("sub {i}"), vec![MenuItem::separator()])
                } else {
                    MenuItem::action(&format!("row {i}"), Activation::Local(0))
                }
            })
            .collect();
        vec![MenuItem::submenu("Tall", children)]
    }

    fn assert_rows_inside(panel: &Panel) {
        for row in panel.rows.iter().filter(|row| row.enabled) {
            assert!(row.rect.loc.y >= panel.rect.loc.y + PANEL_PAD);
            assert!(row.rect.loc.y + row.rect.size.h <= panel.rect.loc.y + panel.rect.size.h - PANEL_PAD);
        }
    }

    #[test]
    fn a_menu_taller_than_the_output_stays_below_the_strip_and_on_screen() {
        let painter = crate::paint::Painter::new(14.0, None);
        let bar = tall_bar(60, usize::MAX);
        let bounds = Rectangle::new(Point::from((0, 0)), Size::from((1366, 768)));
        let top = 40;
        let panels = layout_panels(
            &painter,
            &bar,
            MenuPath::top(0),
            Rectangle::new(Point::from((100, 4)), Size::from((60, 28))),
            top,
            bounds,
        );
        let panel = &panels[0];
        assert!(panel.rect.loc.y >= top);
        assert!(panel.rect.loc.y + panel.rect.size.h <= 768 - EDGE_MARGIN);
        assert_eq!(panel.scroll, 0);
        assert_rows_inside(panel);
        assert!(panel.rows.iter().any(|row| !row.enabled));
        let last_reachable = step_enabled(panel, None, false).expect("a visible row");
        let row = panel.rows.iter().find(|r| r.path == last_reachable);
        assert!(row.is_some_and(|r| r.rect.loc.y + r.rect.size.h <= 768));
    }

    #[test]
    fn an_open_submenu_row_past_the_fold_scrolls_into_view() {
        let painter = crate::paint::Painter::new(14.0, None);
        let bar = tall_bar(60, 55);
        let bounds = Rectangle::new(Point::from((0, 0)), Size::from((1366, 768)));
        let open = MenuPath::top(0).child(55).expect("depth");
        let panels = layout_panels(
            &painter,
            &bar,
            open,
            Rectangle::new(Point::from((100, 4)), Size::from((60, 28))),
            40,
            bounds,
        );
        let panel = &panels[0];
        assert!(panel.scroll > 0);
        assert_rows_inside(panel);
        let row = panel.rows.iter().find(|row| row.path == open).expect("row");
        assert!(row.enabled);
        assert_eq!(panel.scroll, scroll_offset(&bar[0].children, panel.rect.size.h, Some(55)));
        assert_eq!(panels.len(), 2);
        assert!(panels[1].rect.loc.y >= 40);
    }

    #[test]
    fn short_menus_never_scroll() {
        let items = desktop_menu(false, false).remove(2).children;
        assert_eq!(scroll_offset(&items, 1000, Some(4)), 0);
    }
}
