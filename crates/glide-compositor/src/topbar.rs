use std::collections::HashMap;
use std::time::Duration;

use glide_theme::TextStyle;
use glide_theme::motion::{self, CubicBezier, E_IO, E_OUT, E_OVER, ease_io, ease_out, lerp};
use glide_theme::shape::{R_ROUND, R1, R2, R3};
use glide_theme::spacing::{SP1, SP2};
use glide_ui::{Reveal, Tween};
use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use crate::config::Color;
use crate::global_menu::MenuPath;
use crate::paint::TextAlign;
use crate::popup_menu::text_px as text_w;
use crate::power::PowerAction;
use crate::raster::{Canvas, ShellRenderer};

#[path = "topbar_panels.rs"]
mod panels;

#[path = "topbar_calendar.rs"]
pub mod calendar;

#[path = "topbar_status.rs"]
mod status;

#[path = "topbar_tray.rs"]
mod tray;

pub use tray::{ICON as TRAY_ICON, IconArt, MAX_SCALE as TRAY_MAX_SCALE, TrayButton, TrayClick, TrayIcon};

pub use panels::{
    Detail, FieldKey, PanelAction, PowerDispatch, PromptOutcome, QsKeyMode, RowTarget, field_key, power_dispatch,
};

pub const HEIGHT: i32 = 34;

const PAD_X: i32 = glide_theme::spacing::SP2;
const BUTTON_HEIGHT: i32 = 24;
const BUTTON_PAD_X: i32 = glide_theme::spacing::SP2;
const ITEM_GAP: i32 = 2;
const BUTTON_RADIUS: f32 = glide_theme::shape::R2;
const BUTTON_MIN_WIDTH: i32 = 28;

const APP_ICON: i32 = 16;
const APP_PAD_LEFT: i32 = glide_theme::spacing::SP1;
const APP_GAP: i32 = glide_theme::spacing::SP2;
const APP_NAME_MAX: usize = 28;

const CLOCK_GAP: i32 = glide_theme::spacing::SP2;

const SEG_HEIGHT: i32 = 20;
const SEG_INSET: i32 = 2;
const SEG_GAP: i32 = 2;
const SEG_MIN_WIDTH: i32 = 25;
const SEG_RADIUS: f32 = glide_theme::shape::R1;
const SEG_PAD_X: i32 = glide_theme::spacing::SP1;

const SWITCH_WIDTH: i32 = 28;
const SWITCH_HEIGHT: i32 = 16;
const SWITCH_THUMB: i32 = 12;
const SWITCH_INSET: i32 = 2;

const COFFEE_GLYPH: f32 = 14.0;
const COFFEE_STROKE: f32 = 1.5;

const DND_WIDTH: i32 = 28;
const DND_GLYPH: f32 = 14.0;

const CLOCK_BADGE_HEIGHT: i32 = 16;
const CLOCK_BADGE_MIN_WIDTH: i32 = 16;
const CLOCK_BADGE_PAD_X: i32 = 5;

const LANG_MENU_WIDTH: i32 = 196;
const LANG_MENU_PAD: i32 = glide_theme::spacing::SP1;
const LANG_MENU_ITEM_HEIGHT: i32 = 34;
const LANG_MENU_DIVIDER: i32 = glide_theme::spacing::SP1 * 2 + 1;

const NC_WIDTH: i32 = crate::notifications::WIDTH;
const NC_PAD: i32 = glide_theme::spacing::SP3;
const NC_HEADER_HEIGHT: i32 = 24;
const NC_GAP: i32 = glide_theme::spacing::SP2;
const NC_ITEM_HEIGHT: i32 = 54;
const NC_EMPTY_HEIGHT: i32 = 60;
const NC_MAX_ITEMS: usize = 6;
const NC_TOP: i32 = NC_PAD + calendar::BLOCK_H;

const STATUS_WIFI_WIDTH: i32 = 15;
const STATUS_SPEAKER_WIDTH: i32 = 14;
const STATUS_BATT_WIDTH: i32 = 24;
const STATUS_SEP_WIDTH: i32 = 13;

const QS_WIDTH: i32 = 312;
const QS_PAD: i32 = glide_theme::spacing::SP3;
const QS_GAP: i32 = glide_theme::spacing::SP3;
const QS_GRID_GAP: i32 = glide_theme::spacing::SP2;
const QS_TILE_HEIGHT: i32 = 54;
const QS_ROW_HEIGHT: i32 = 44;
const QS_SLIDER_HEIGHT: i32 = 22;
const QS_SLIDER_GAP: i32 = 10;
const QS_SLIDER_ICON: i32 = 22;
const QS_SLIDER_TRACK_H: i32 = 6;
const QS_SLIDER_THUMB: i32 = 14;
const QS_ICON_BADGE: i32 = 28;
const QS_ACT_BTN: i32 = 28;
const QS_BATTERY_W: i32 = 72;
const QS_AVATAR: i32 = 26;
const QS_SWITCH_W: i32 = 44;
const QS_SWITCH_H: i32 = 26;
const QS_SWITCH_THUMB: i32 = 20;
const QS_SWITCH_INSET: i32 = 3;

const POPUP_GAP: i32 = glide_theme::spacing::SP2;
const POPUP_RADIUS: f32 = glide_theme::shape::R3;
const POPUP_RISE: f64 = 6.0;
const POPUP_SCALE_FROM: f64 = 0.94;
const BADGE_SCALE_FROM: f64 = 0.6;
const BADGE_QUANTUM: f64 = 16.0;
const MOTION_QUANTUM: f64 = 32.0;

#[derive(Debug, Clone, Copy)]
struct Anim {
    tween: Tween,
    duration: Duration,
    ease: CubicBezier,
}

impl Anim {
    fn at(value: f64) -> Self {
        Self { tween: Tween::at(value), duration: Duration::ZERO, ease: E_OUT }
    }

    fn target(&self) -> f64 {
        self.tween.target()
    }

    fn retarget(&mut self, to: f64, duration: Duration, ease: CubicBezier) -> bool {
        if (self.target() - to).abs() < f64::EPSILON {
            return false;
        }
        let current = self.value();
        self.tween.jump(current);
        self.duration = duration;
        self.ease = ease;
        self.tween.retarget(to, duration, |t| ease.ease_f64(t))
    }

    fn jump(&mut self, to: f64) {
        self.tween.jump(to);
    }

    fn value(&self) -> f64 {
        let ease = self.ease;
        self.tween.value(self.duration, |t| ease.ease_f64(t))
    }

    fn value_with(&self, ease: CubicBezier) -> f64 {
        self.tween.value(self.duration, |t| ease.ease_f64(t))
    }

    fn is_animating(&self) -> bool {
        self.tween.is_animating(self.duration)
    }

    fn switch(&self) -> SwitchMotion {
        SwitchMotion { thumb: self.value(), track: self.value_with(E_OUT) }
    }
}

impl Default for Anim {
    fn default() -> Self {
        Self::at(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SwitchMotion {
    thumb: f64,
    track: f64,
}

impl SwitchMotion {
    fn key(self) -> (u8, u8) {
        (quantize(self.thumb, MOTION_QUANTUM), quantize(self.track, MOTION_QUANTUM))
    }

    fn thumb_x(self, track_x: i32, width: i32, inset: i32, thumb: i32) -> i32 {
        switch_thumb_x(track_x, width, inset, thumb, self.thumb)
    }

    fn track_colour(self, off: Color, on: Color) -> Color {
        off.lerp(on, self.track as f32)
    }
}

fn quantize(value: f64, steps: f64) -> u8 {
    (value.clamp(0.0, 2.0) * steps).round() as u8
}

fn switch_thumb_x(track_x: i32, width: i32, inset: i32, thumb: i32, t: f64) -> i32 {
    let off = f64::from(track_x + inset);
    let on = f64::from(track_x + width - inset - thumb);
    let x = lerp(off, on, t).round() as i32;
    x.clamp(track_x, track_x + width - thumb)
}

fn segment_pill(first: (i32, i32), second: (i32, i32), t: f64) -> (i32, i32) {
    let t = t.clamp(0.0, 1.0);
    let x = lerp(f64::from(first.0), f64::from(second.0), t).round() as i32;
    let w = lerp(f64::from(first.1), f64::from(second.1), t).round() as i32;
    (x, w)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPopup {
    NotificationCenter,
    LanguageMenu,
    QuickSettings,
    MediaPlayer,
    AppMenu,
    Privacy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuRowOutcome {
    Submenu,
    Activate(glide_appmenu::Activation),
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopbarAction {
    OpenAppMenu(u16),
    AppMenuRow(MenuPath),
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    MediaSeek(u8),
    ToggleMediaPlayer,
    CycleLayout,
    SelectLayout(usize),
    ToggleCaffeine,
    ToggleDoNotDisturb,
    ToggleNotificationCenter,
    ToggleLanguageMenu,
    ToggleQuickSettings,
    ClearAllNotifications,
    ActivateNotification(u32),
    DismissNotification(u32),
    OpenInputSettings,
    ClosePopup,
    QsToggleWifi,
    QsToggleBluetooth,
    QsToggleNightLight,
    QsVolumeMute,
    QsVolumeAdjust(u8),
    QsBrightnessAdjust(u8),
    QsScreenshot,
    QsOpenSettings,
    QsPower(PowerAction),
    QsPanel(PanelAction),
    ToggleLauncher,
    TogglePrivacy,
    CalendarShift(i8),
    CalendarToday,
    Tray(TrayClick),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    MenuTitle(u16),
    MenuRow(MenuPath),
    Clock,
    Media,
    MediaPrev,
    MediaPlay,
    MediaNext,
    PopMediaPrev,
    PopMediaPlay,
    PopMediaNext,
    PopMediaSeek,
    LayoutCycle,
    LayoutPick(usize),
    LayoutDropdown,
    Caffeine,
    DoNotDisturb,
    Status,
    LangMenuItem(usize),
    LangMenuSettings,
    NcClearAll,
    NcDndToggle,
    NcItem(u32),
    NcItemDismiss(u32),
    QsWifi,
    QsBluetooth,
    QsNightLight,
    QsScreenshot,
    QsCaffeine,
    QsVolumeMute,
    QsVolumeTrack,
    QsBrightnessTrack,
    QsSettings,
    QsPowerButton(PowerAction),
    QsWifiMore,
    QsBluetoothMore,
    QsBattery,
    QsBack,
    QsDetailToggle,
    QsDetailRow(u16),
    QsPasswordField,
    QsDetailSettings,
    Search,
    Privacy,
    CalMonth,
    CalPrev,
    CalNext,
    Tray(u16),
}

impl Slot {
    const fn in_status_cluster(self) -> bool {
        matches!(self, Self::Status | Self::Search | Self::Privacy)
    }

    const fn in_privacy_popup(self) -> bool {
        let _ = self;
        false
    }

    const fn in_strip(self) -> bool {
        self.in_status_cluster()
            || matches!(
                self,
                Self::MenuTitle(_)
                    | Self::Clock
                    | Self::Media
                    | Self::MediaPrev
                    | Self::MediaPlay
                    | Self::MediaNext
                    | Self::LayoutCycle
                    | Self::LayoutPick(_)
                    | Self::LayoutDropdown
                    | Self::Caffeine
                    | Self::DoNotDisturb
                    | Self::Tray(_)
            )
    }

    const fn in_media_popup(self) -> bool {
        matches!(self, Self::PopMediaPrev | Self::PopMediaPlay | Self::PopMediaNext | Self::PopMediaSeek)
    }

    const fn in_layout(self) -> bool {
        matches!(self, Self::LayoutCycle | Self::LayoutPick(_) | Self::LayoutDropdown)
    }

    const fn in_lang_menu(self) -> bool {
        matches!(self, Self::LangMenuItem(_) | Self::LangMenuSettings)
    }

    const fn in_notification_center(self) -> bool {
        matches!(
            self,
            Self::NcClearAll
                | Self::NcDndToggle
                | Self::NcItem(_)
                | Self::NcItemDismiss(_)
                | Self::CalMonth
                | Self::CalPrev
                | Self::CalNext
        )
    }

    const fn in_app_menu(self) -> bool {
        matches!(self, Self::MenuRow(_))
    }

    const fn in_quick_settings(self) -> bool {
        !self.in_strip()
            && !self.in_media_popup()
            && !self.in_lang_menu()
            && !self.in_notification_center()
            && !self.in_app_menu()
    }

    const fn state_layer_radius(self) -> Option<f32> {
        match self {
            Self::NcDndToggle
            | Self::QsVolumeTrack
            | Self::QsBrightnessTrack
            | Self::QsDetailToggle
            | Self::QsPasswordField
            | Self::PopMediaSeek => None,
            Self::MediaPrev | Self::MediaPlay | Self::MediaNext => Some(glide_theme::shape::R1),
            Self::LayoutPick(_) => Some(SEG_RADIUS),
            Self::PopMediaPlay | Self::NcItemDismiss(_) | Self::QsPowerButton(_) | Self::QsBack => {
                Some(glide_theme::shape::R_ROUND)
            }
            _ => Some(glide_theme::shape::R2),
        }
    }

    const fn action(self) -> TopbarAction {
        match self {
            Self::MenuTitle(index) => TopbarAction::OpenAppMenu(index),
            Self::MenuRow(path) => TopbarAction::AppMenuRow(path),
            Self::Clock => TopbarAction::ToggleNotificationCenter,
            Self::Media => TopbarAction::ToggleMediaPlayer,
            Self::MediaPlay | Self::PopMediaPlay => TopbarAction::MediaPlayPause,
            Self::MediaPrev | Self::PopMediaPrev => TopbarAction::MediaPrevious,
            Self::MediaNext | Self::PopMediaNext => TopbarAction::MediaNext,
            Self::PopMediaSeek => TopbarAction::MediaSeek(0),
            Self::LayoutCycle => TopbarAction::CycleLayout,
            Self::LayoutPick(index) | Self::LangMenuItem(index) => TopbarAction::SelectLayout(index),
            Self::LayoutDropdown => TopbarAction::ToggleLanguageMenu,
            Self::Caffeine | Self::QsCaffeine => TopbarAction::ToggleCaffeine,
            Self::DoNotDisturb | Self::NcDndToggle => TopbarAction::ToggleDoNotDisturb,
            Self::Status => TopbarAction::ToggleQuickSettings,
            Self::LangMenuSettings => TopbarAction::OpenInputSettings,
            Self::NcClearAll => TopbarAction::ClearAllNotifications,
            Self::NcItem(id) => TopbarAction::ActivateNotification(id),
            Self::NcItemDismiss(id) => TopbarAction::DismissNotification(id),
            Self::QsWifi => TopbarAction::QsToggleWifi,
            Self::QsBluetooth => TopbarAction::QsToggleBluetooth,
            Self::QsNightLight => TopbarAction::QsToggleNightLight,
            Self::QsScreenshot => TopbarAction::QsScreenshot,
            Self::QsVolumeMute => TopbarAction::QsVolumeMute,
            Self::QsVolumeTrack => TopbarAction::QsVolumeAdjust(50),
            Self::QsBrightnessTrack => TopbarAction::QsBrightnessAdjust(50),
            Self::QsSettings => TopbarAction::QsOpenSettings,
            Self::QsPowerButton(action) => TopbarAction::QsPower(action),
            Self::QsWifiMore => TopbarAction::QsPanel(PanelAction::Open(Detail::Wifi)),
            Self::QsBluetoothMore => TopbarAction::QsPanel(PanelAction::Open(Detail::Bluetooth)),
            Self::QsBattery => TopbarAction::QsPanel(PanelAction::Open(Detail::Battery)),
            Self::QsBack => TopbarAction::QsPanel(PanelAction::Back),
            Self::QsDetailToggle => TopbarAction::QsPanel(PanelAction::Toggle),
            Self::QsDetailRow(index) => TopbarAction::QsPanel(PanelAction::Row(index)),
            Self::QsPasswordField => TopbarAction::QsPanel(PanelAction::FocusPassword),
            Self::QsDetailSettings => TopbarAction::QsPanel(PanelAction::Settings),
            Self::Search => TopbarAction::ToggleLauncher,
            Self::Privacy => TopbarAction::TogglePrivacy,
            Self::CalMonth => TopbarAction::CalendarToday,
            Self::CalPrev => TopbarAction::CalendarShift(-1),
            Self::CalNext => TopbarAction::CalendarShift(1),
            Self::Tray(id) => TopbarAction::Tray(TrayClick { id, button: TrayButton::Primary }),
        }
    }
}

type ScaledBuffers = std::collections::HashMap<i32, MemoryRenderBuffer>;
type QsCacheKey = (u64, QsFlags, ((u8, u8), u8, u8), Option<Slot>);
type NcCacheKey = (u64, bool, (u8, u8), Option<Slot>);
type LayerKey = (u8, i32, i32, u32, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Hover,
    Press,
    Focus,
}

impl Layer {
    const fn tag(self) -> u8 {
        match self {
            Self::Hover => 0,
            Self::Press => 1,
            Self::Focus => 2,
        }
    }

    const fn colour(self, theme: &TopbarTheme) -> Color {
        match self {
            Self::Hover => theme.button_hover,
            Self::Press => theme.button_press,
            Self::Focus => theme.active_background,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Badge {
    count: usize,
    reserved: bool,
}

#[derive(Debug)]
struct Cache<K> {
    key: Option<K>,
    buffers: ScaledBuffers,
}

impl<K> Default for Cache<K> {
    fn default() -> Self {
        Self { key: None, buffers: ScaledBuffers::new() }
    }
}

impl<K: PartialEq> Cache<K> {
    fn clear(&mut self) {
        self.key = None;
        self.buffers.clear();
    }

    fn resolve(
        &mut self, key: K, scale: i32, draw: impl FnOnce() -> Option<MemoryRenderBuffer>,
    ) -> Option<&MemoryRenderBuffer> {
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.buffers.clear();
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = self.buffers.entry(scale) {
            entry.insert(draw()?);
        }
        self.buffers.get(&scale)
    }
}

#[derive(Debug, Default)]
pub struct Topbar {
    theme: TopbarTheme,
    date: String,
    clock: String,
    unread: usize,
    layouts: Vec<String>,
    layout_index: usize,
    app_name: String,
    app_id: String,
    caffeinated: bool,
    dnd: bool,
    open_popup: Option<OpenPopup>,
    closing_popup: Option<OpenPopup>,
    popup_reveal: Reveal,
    nc_items: Vec<crate::notifications::NcItem>,
    media: Option<MediaWidget>,
    system_status: crate::system_info::SystemStatus,
    hovered: Option<Slot>,
    pressed: Option<Slot>,
    focused: Option<Slot>,
    focus_ring_last: Option<Rectangle<i32, Logical>>,
    qs_panel: panels::DetailPanel,

    hover_layers: HashMap<Slot, Anim>,
    press_layers: HashMap<Slot, Anim>,
    focus_anim: Anim,
    caffeine_anim: Anim,
    dnd_anim: Anim,
    volume_anim: Anim,
    brightness_anim: Anim,
    seek_anim: Anim,
    badge_anim: Anim,
    badge: Badge,
    segment_anim: Anim,
    slider_dragging: bool,
    scroll: crate::system_info::ScrollAccumulator,

    rev: u64,
    layouts_rev: u64,
    media_rev: u64,
    status_rev: u64,
    detail_rev: u64,
    nc_rev: u64,

    slots: Vec<(Slot, Rectangle<i32, Logical>)>,
    shown_popup_slots: Vec<(Slot, Rectangle<i32, Logical>)>,
    layout_key: Option<(u64, Rectangle<i32, Logical>)>,
    popup_rect: Option<Rectangle<i32, Logical>>,
    panel: std::collections::HashMap<PanelKey, MemoryRenderBuffer>,
    shadows: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    layers: HashMap<LayerKey, MemoryRenderBuffer>,
    separator_cache: Cache<()>,
    app_cache: Cache<(String, String)>,
    clock_cache: Cache<(String, String, usize, bool, u8)>,
    media_cache: Cache<(u64, Option<u16>)>,
    media_popup_cache: Cache<(u64, Option<u16>)>,
    layout_cache: Cache<(u64, usize, Option<Slot>, bool, u8)>,
    coffee_cache: Cache<(bool, bool, (u8, u8))>,
    dnd_cache: Cache<(bool, u8)>,
    qs_cache: Cache<QsCacheKey>,
    qs_bg_cache: Cache<(i32, i32)>,
    qs_detail_cache: Cache<(u64, u64, Option<Slot>, Detail)>,
    nc_cache: Cache<NcCacheKey>,
    lang_menu_cache: Cache<(u64, usize, Option<Slot>)>,
    raster_rev: u64,

    menu_bar: Vec<glide_appmenu::MenuItem>,
    menu_open: MenuPath,
    menu_panels: Vec<crate::global_menu::Panel>,
    menu_titles: HashMap<(usize, bool, i32), MemoryRenderBuffer>,
    menu_rasters: HashMap<(MenuPath, Option<usize>, i32), MemoryRenderBuffer>,
    app_chip_w: i32,

    calendar: calendar::CalendarView,
    indicators: status::Indicators,
    tray: tray::TrayStrip,
}

type Anchor = Option<Rectangle<i32, Logical>>;

type PanelKey = (i32, i32, u64, (i32, i32, u64));

struct MenuLayout {
    titles: Vec<(Slot, Rectangle<i32, Logical>)>,
    rows: Vec<(Slot, Rectangle<i32, Logical>)>,
    panels: Vec<crate::global_menu::Panel>,
    anchor: Option<Rectangle<i32, Logical>>,
}

impl Topbar {
    #[must_use]
    pub fn new(theme: TopbarTheme) -> Self {
        let mut bar = Self { theme, ..Self::default() };
        bar.volume_anim.jump(f64::from(bar.system_status.volume_pct));
        bar.brightness_anim.jump(f64::from(bar.system_status.brightness_pct));
        bar
    }

    pub fn reconfigure(&mut self, theme: TopbarTheme) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        self.invalidate();
    }

    fn invalidate(&mut self) {
        self.touch();
        self.panel.clear();
        self.shadows.clear();
        self.layers.clear();
        self.separator_cache.clear();
        self.app_cache.clear();
        self.clock_cache.clear();
        self.media_cache.clear();
        self.media_popup_cache.clear();
        self.layout_cache.clear();
        self.coffee_cache.clear();
        self.dnd_cache.clear();
        self.qs_cache.clear();
        self.qs_bg_cache.clear();
        self.qs_detail_cache.clear();
        self.nc_cache.clear();
        self.lang_menu_cache.clear();
        self.menu_titles.clear();
        self.menu_rasters.clear();
        self.calendar.clear();
        self.indicators.clear();
    }

    pub fn set_app_menu(&mut self, items: Vec<glide_appmenu::MenuItem>) -> bool {
        if self.menu_bar == items {
            return false;
        }
        let app_owned = self.tray_menu_id().is_none();
        let keep_open = self.open_popup == Some(OpenPopup::AppMenu)
            && crate::global_menu::item_at(&items, self.menu_open.truncated(1))
                .is_some_and(glide_appmenu::MenuItem::is_submenu);
        self.menu_bar = items;
        self.menu_titles.clear();
        self.menu_rasters.clear();
        if !app_owned {
            self.touch();
            return true;
        }
        if keep_open {
            while !self.menu_open.is_empty()
                && !crate::global_menu::item_at(&self.menu_bar, self.menu_open)
                    .is_some_and(glide_appmenu::MenuItem::is_submenu)
            {
                match self.menu_open.parent() {
                    Some(parent) => self.menu_open = parent,
                    None => break,
                }
            }
        } else if self.open_popup == Some(OpenPopup::AppMenu) {
            self.close_popup();
        }
        self.touch();
        true
    }

    #[must_use]
    pub fn app_menu(&self) -> &[glide_appmenu::MenuItem] {
        &self.menu_bar
    }

    #[must_use]
    pub fn app_menu_item(&self, path: MenuPath) -> Option<&glide_appmenu::MenuItem> {
        crate::global_menu::item_at(self.menu_items(), path)
    }

    pub fn open_app_menu(&mut self, index: usize) -> bool {
        if !crate::global_menu::item_at(&self.menu_bar, MenuPath::top(index))
            .is_some_and(glide_appmenu::MenuItem::is_submenu)
        {
            return false;
        }
        if self.tray_menu_id().is_some() {
            self.release_tray_menu();
            if self.open_popup == Some(OpenPopup::AppMenu) {
                self.menu_open = MenuPath::top(index);
                self.set_focused(None);
                self.touch();
                return true;
            }
        }
        if self.open_popup == Some(OpenPopup::AppMenu) {
            if self.menu_open.top_index() == index {
                return self.close_popup();
            }
            self.menu_open = MenuPath::top(index);
            self.set_focused(None);
            self.touch();
            return true;
        }
        self.menu_open = MenuPath::top(index);
        self.toggle_popup(OpenPopup::AppMenu)
    }

    pub fn app_menu_row(&mut self, path: MenuPath) -> MenuRowOutcome {
        let Some(item) = crate::global_menu::item_at(self.menu_items(), path) else {
            return MenuRowOutcome::Ignored;
        };
        if !item.enabled || item.separator {
            return MenuRowOutcome::Ignored;
        }
        if item.is_submenu() {
            if self.menu_open != path {
                self.menu_open = path;
                self.touch();
            }
            return MenuRowOutcome::Submenu;
        }
        let activation = item.activation.clone();
        self.close_popup();
        MenuRowOutcome::Activate(activation)
    }

    fn follow_menu_hover(&mut self, hovered: Option<Slot>) {
        if self.open_popup != Some(OpenPopup::AppMenu) {
            return;
        }
        let next = match hovered {
            Some(Slot::MenuTitle(index))
                if usize::from(index) != self.menu_open.top_index() && self.tray_menu_id().is_none() =>
            {
                self.set_focused(None);
                Some(MenuPath::top(usize::from(index)))
            }
            Some(Slot::MenuRow(path)) => {
                let submenu = crate::global_menu::item_at(self.menu_items(), path)
                    .is_some_and(glide_appmenu::MenuItem::is_submenu);
                if submenu { Some(path) } else { path.parent() }
            }
            _ => None,
        };
        if let Some(next) = next
            && next != self.menu_open
        {
            self.menu_open = next;
            self.touch();
        }
    }

    fn app_menu_nav(&mut self, dx: i32, dy: i32) -> bool {
        let focused = match self.focused {
            Some(Slot::MenuRow(path)) => Some(path),
            _ => None,
        };
        let panel_parent = focused.and_then(MenuPath::parent).unwrap_or_else(|| self.menu_open.truncated(1));
        if dy != 0 {
            let Some(panel) = self.menu_panels.iter().find(|p| p.parent == panel_parent) else {
                return false;
            };
            let Some(next) = crate::global_menu::step_enabled(panel, focused, dy > 0) else {
                return false;
            };
            self.menu_open = panel_parent;
            self.set_focused(Some(Slot::MenuRow(next)));
            self.touch();
            return true;
        }
        let submenu = focused.filter(|path| {
            crate::global_menu::item_at(self.menu_items(), *path).is_some_and(glide_appmenu::MenuItem::is_submenu)
        });
        if dx > 0
            && let Some(path) = submenu
        {
            self.menu_open = path;
            let first = crate::global_menu::item_at(self.menu_items(), path)
                .and_then(|item| item.children.iter().position(|c| c.enabled && !c.separator))
                .and_then(|index| path.child(index));
            self.set_focused(first.map(Slot::MenuRow));
            self.touch();
            return true;
        }
        if dx < 0
            && let Some(path) = focused.filter(|path| path.len() > 2)
            && let Some(parent) = path.parent()
        {
            self.menu_open = parent.parent().unwrap_or(parent);
            self.set_focused(Some(Slot::MenuRow(parent)));
            self.touch();
            return true;
        }
        let count = self.menu_items().len();
        if count == 0 {
            return false;
        }
        let step = if dx > 0 { 1 } else { count - 1 };
        let mut index = self.menu_open.top_index();
        for _ in 0..count {
            index = (index + step) % count;
            if self.menu_items()[index].is_submenu() {
                break;
            }
        }
        self.menu_open = MenuPath::top(index);
        self.set_focused(None);
        self.touch();
        true
    }

    const fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    fn hovered_in(&self, pred: fn(Slot) -> bool) -> Option<Slot> {
        self.hovered.filter(|slot| pred(*slot))
    }

    fn focused_in(&self, pred: fn(Slot) -> bool) -> Option<Slot> {
        self.focused.filter(|slot| pred(*slot))
    }

    fn focus_ring_in(&self, pred: fn(Slot) -> bool) -> Option<LocalRect> {
        let popup = self.popup_rect?;
        let rect = self.slot_rect(self.focused_in(pred)?)?;
        Some(Rectangle::new(rect.loc - popup.loc, rect.size))
    }

    fn focus_ring_rect(&self) -> Option<Rectangle<i32, Logical>> {
        let popup = self.popup_rect?;
        let local = self.focus_ring_in(popup_slots(self.open_popup?))?;
        Some(Rectangle::new(local.loc + popup.loc, local.size))
    }

    fn set_focused(&mut self, slot: Option<Slot>) {
        self.focused = slot;
        self.focus_anim.retarget(if slot.is_some() { 1.0 } else { 0.0 }, motion::t1(), E_OUT);
    }

    #[must_use]
    pub const fn has_focus(&self) -> bool {
        self.focused.is_some()
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let popup = self.popup_reveal.is_animating(motion::t2());
        let layers = self.hover_layers.values().chain(self.press_layers.values()).any(Anim::is_animating);
        let controls = [
            &self.focus_anim,
            &self.caffeine_anim,
            &self.dnd_anim,
            &self.volume_anim,
            &self.brightness_anim,
            &self.seek_anim,
            &self.badge_anim,
            &self.segment_anim,
        ]
        .into_iter()
        .any(Anim::is_animating);
        let detail = self.qs_panel.is_animating();
        let extras = self.calendar.is_animating() || self.indicators.is_animating();
        let tray = self.tray_is_animating();
        popup || layers || controls || detail || extras || tray || self.pending_settle()
    }

    fn pending_settle(&self) -> bool {
        self.closing_popup.is_some() || (self.unread == 0 && self.badge.reserved)
    }

    pub fn set_pressed(&mut self, slot: Option<Slot>) -> bool {
        if self.pressed == slot {
            return false;
        }
        if let Some(previous) = self.pressed.take()
            && let Some(layer) = self.press_layers.get_mut(&previous)
        {
            layer.retarget(0.0, motion::t1(), E_OUT);
        }
        if let Some(slot) = slot {
            self.press_layers.entry(slot).or_default().jump(1.0);
        }
        self.pressed = slot;
        true
    }

    pub const fn set_slider_dragging(&mut self, on: bool) {
        self.slider_dragging = on;
    }

    fn slide(&mut self, which: fn(&mut Self) -> &mut Anim, pct: u8) {
        let target = f64::from(pct);
        let dragging = self.slider_dragging;
        let anim = which(self);
        if dragging {
            anim.jump(target);
        } else {
            anim.retarget(target, motion::t2(), E_OUT);
        }
    }

    fn prune_layers(&mut self) {
        let settled = |anim: &Anim| anim.target() <= 0.0 && !anim.is_animating();
        self.hover_layers.retain(|_, anim| !settled(anim));
        self.press_layers.retain(|_, anim| !settled(anim));
    }

    fn shown_popup(&self) -> Option<OpenPopup> {
        self.open_popup.or(self.closing_popup)
    }

    fn popup_progress(&self) -> f64 {
        let duration = motion::t2();
        if self.popup_reveal.is_open() {
            self.popup_reveal.progress(duration, ease_out)
        } else {
            self.popup_reveal.progress(duration, ease_io)
        }
    }

    fn settle_motion(&mut self) {
        let duration = motion::t2();
        self.popup_reveal.settle(duration);
        if self.closing_popup.is_some() && !self.popup_reveal.is_visible(duration) {
            self.closing_popup = None;
            self.touch();
        }
        let reserved = self.unread > 0 || self.badge_anim.value() > 0.0;
        if reserved != self.badge.reserved {
            self.badge.reserved = reserved;
            self.touch();
        }
        if let Some(rect) = self.focus_ring_rect() {
            self.focus_ring_last = Some(rect);
        }
        self.settle_indicators();
        self.tray_settle();
        self.prune_layers();
    }

    #[must_use]
    pub fn focused_action(&self) -> Option<TopbarAction> {
        let slot = self.focused?;
        self.slot_rect(slot)?;
        Some(slot.action())
    }

    pub fn focus_adjust(&mut self, delta: i8) -> Option<TopbarAction> {
        let slot = self.focused?;
        let current = match slot {
            Slot::QsVolumeTrack => self.system_status.volume_pct,
            Slot::QsBrightnessTrack => self.system_status.brightness_pct,
            _ => return None,
        };
        let pct = u8::try_from((i32::from(current) + i32::from(delta)).clamp(0, 100)).ok()?;
        let action = if slot == Slot::QsVolumeTrack {
            self.system_status.volume_pct = pct;
            self.slide(|bar| &mut bar.volume_anim, pct);
            TopbarAction::QsVolumeAdjust(pct)
        } else {
            self.system_status.brightness_pct = pct;
            self.slide(|bar| &mut bar.brightness_anim, pct);
            TopbarAction::QsBrightnessAdjust(pct)
        };
        self.status_rev = self.status_rev.wrapping_add(1);
        self.touch();
        Some(action)
    }

    pub fn focus_nav(&mut self, dx: i32, dy: i32) -> bool {
        if dx == 0 && dy == 0 {
            return false;
        }
        if self.open_popup == Some(OpenPopup::AppMenu) {
            return self.app_menu_nav(dx, dy);
        }
        if dx != 0 && self.focused == Some(Slot::CalMonth) && self.open_popup == Some(OpenPopup::NotificationCenter) {
            return self.shift_calendar(dx.signum());
        }
        let Some(pred) = self.open_popup.map(popup_slots) else {
            return false;
        };
        let candidates: Vec<(Slot, Rectangle<i32, Logical>)> =
            self.slots.iter().copied().filter(|(slot, _)| pred(*slot)).collect();
        let current = self
            .focused
            .and_then(|focused| candidates.iter().find(|(slot, _)| *slot == focused).map(|(_, rect)| *rect));
        let next = match current {
            None => candidates
                .iter()
                .min_by_key(|(_, rect)| {
                    let centre = rect_centre(*rect);
                    (centre.y, centre.x)
                })
                .map(|(slot, _)| *slot),
            Some(from) => {
                let origin = rect_centre(from);
                candidates
                    .iter()
                    .filter(|(slot, _)| Some(*slot) != self.focused)
                    .filter_map(|(slot, rect)| {
                        let centre = rect_centre(*rect);
                        let (step_x, step_y) = (centre.x - origin.x, centre.y - origin.y);
                        let (along, across, in_lane) = if dx == 0 {
                            (
                                step_y * dy.signum(),
                                step_x,
                                spans_overlap(from.loc.x, from.size.w, rect.loc.x, rect.size.w),
                            )
                        } else {
                            (
                                step_x * dx.signum(),
                                step_y,
                                spans_overlap(from.loc.y, from.size.h, rect.loc.y, rect.size.h),
                            )
                        };
                        (along > 0 && in_lane).then_some((along + across.abs() * 2, *slot))
                    })
                    .min_by_key(|(score, _)| *score)
                    .map(|(_, slot)| slot)
            }
        };
        let Some(next) = next else {
            return false;
        };
        if self.focused == Some(next) {
            return false;
        }
        self.set_focused(Some(next));
        true
    }

    fn invalidate_stale_buffers(&mut self) {
        let current = crate::paint::raster_revision(self.theme.revision());
        if current == self.raster_rev {
            return;
        }
        self.raster_rev = current;
        self.invalidate();
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.theme.enabled
    }

    #[must_use]
    pub const fn caffeinated(&self) -> bool {
        self.caffeinated
    }

    #[must_use]
    pub const fn battery_low_pct(&self) -> u8 {
        self.theme.battery_low_pct
    }

    pub fn set_caffeinated(&mut self, on: bool) {
        if self.caffeinated == on {
            return;
        }
        self.caffeinated = on;
        self.caffeine_anim.retarget(if on { 1.0 } else { 0.0 }, motion::t2(), E_OVER);
        self.status_rev = self.status_rev.wrapping_add(1);
    }

    #[must_use]
    pub const fn dnd(&self) -> bool {
        self.dnd
    }

    pub fn set_dnd(&mut self, on: bool) -> bool {
        if self.dnd == on {
            return false;
        }
        self.dnd = on;
        self.dnd_anim.retarget(if on { 1.0 } else { 0.0 }, motion::t2(), E_OVER);
        true
    }

    #[must_use]
    pub const fn unread(&self) -> usize {
        self.unread
    }

    pub fn set_unread(&mut self, unread: usize) -> bool {
        if self.unread == unread {
            return false;
        }
        self.unread = unread;
        if unread > 0 {
            self.badge = Badge { count: unread, reserved: true };
            self.badge_anim.retarget(1.0, motion::t2(), E_OVER);
        } else {
            self.badge_anim.retarget(0.0, motion::t1(), E_OUT);
            self.badge.reserved = self.badge_anim.value() > 0.0;
        }
        self.touch();
        true
    }

    #[must_use]
    pub const fn has_open_popup(&self) -> bool {
        self.popup_reveal.is_open()
    }

    #[must_use]
    pub const fn open_popup(&self) -> Option<OpenPopup> {
        self.open_popup
    }

    pub fn close_popup(&mut self) -> bool {
        let Some(popup) = self.open_popup.take() else {
            return false;
        };
        self.closing_popup = Some(popup);
        self.popup_reveal.close(motion::t2());
        self.set_focused(None);
        self.qs_panel.drop_prompt();
        self.touch();
        true
    }

    pub fn toggle_popup(&mut self, popup: OpenPopup) -> bool {
        if self.open_popup == Some(popup) {
            return self.close_popup();
        }
        self.open_popup = Some(popup);
        self.closing_popup = None;
        self.popup_reveal.open(motion::t2());
        self.set_focused(None);
        self.qs_panel.reset();
        if popup == OpenPopup::NotificationCenter {
            self.calendar.reset();
        }
        self.touch();
        true
    }

    pub fn set_system_status(&mut self, status: crate::system_info::SystemStatus) -> bool {
        if self.system_status == status {
            return false;
        }
        let at_a_glance = !self.system_status.looks_like(&status);
        let (volume, brightness) = (status.volume_pct, status.brightness_pct);
        self.system_status = status;
        self.detail_rev = self.detail_rev.wrapping_add(1);
        if !at_a_glance {
            return self.open_popup.is_some();
        }
        self.slide(|bar| &mut bar.volume_anim, volume);
        self.slide(|bar| &mut bar.brightness_anim, brightness);
        self.status_rev = self.status_rev.wrapping_add(1);
        self.sync_indicators();
        self.touch();
        true
    }

    #[must_use]
    pub fn system_status(&self) -> &crate::system_info::SystemStatus {
        &self.system_status
    }

    pub fn set_nc_items(&mut self, items: Vec<crate::notifications::NcItem>) -> bool {
        if self.nc_items == items {
            return false;
        }
        self.nc_items = items;
        self.nc_rev = self.nc_rev.wrapping_add(1);
        self.touch();
        true
    }

    pub fn set_media(&mut self, media: Option<MediaWidget>) -> bool {
        if self.media == media {
            return false;
        }
        let progress = media.as_ref().and_then(MediaWidget::progress_permille).map_or(0.0, f64::from);
        self.media = media;
        self.seek_anim.jump(progress);
        self.media_rev = self.media_rev.wrapping_add(1);
        self.touch();
        true
    }

    fn seek_permille(&self) -> Option<u16> {
        self.media.as_ref()?.progress?;
        Some(self.seek_anim.value().clamp(0.0, 1000.0).round() as u16)
    }

    #[must_use]
    pub const fn popup_rect(&self) -> Option<Rectangle<i32, Logical>> {
        self.popup_rect
    }

    pub fn set_media_progress(&mut self, progress: Option<f32>) -> bool {
        let Some(media) = self.media.as_mut() else {
            return false;
        };
        if media.progress_permille() == progress_permille(progress) {
            return false;
        }
        media.progress = progress;
        let target = progress_permille(progress).map_or(0.0, f64::from);
        if self.slider_dragging {
            self.seek_anim.jump(target);
        } else {
            self.seek_anim.retarget(target, motion::t2(), E_OUT);
        }
        self.media_rev = self.media_rev.wrapping_add(1);
        true
    }

    #[must_use]
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn scroll_steps(&mut self, delta: crate::system_info::ScrollDelta) -> i32 {
        self.scroll.feed(delta)
    }

    pub fn set_clock(&mut self, date: String, clock: String) -> bool {
        if self.date == date && self.clock == clock {
            return false;
        }
        self.date = date;
        self.clock = clock;
        self.touch();
        true
    }

    pub fn set_layouts(&mut self, layouts: Vec<String>, active: usize) -> bool {
        if self.layouts == layouts && self.layout_index == active {
            return false;
        }
        let pill = f64::from(u8::from(active >= 1));
        if self.layouts == layouts {
            self.segment_anim.retarget(pill, motion::t2(), E_OUT);
        } else {
            self.layouts_rev = self.layouts_rev.wrapping_add(1);
            self.segment_anim.jump(pill);
        }
        self.layouts = layouts;
        self.layout_index = active;
        self.touch();
        true
    }

    pub fn set_app(&mut self, app_id: String, name: String) -> bool {
        if self.app_id == app_id && self.app_name == name {
            return false;
        }
        self.app_id = app_id;
        self.app_name = name;
        self.touch();
        true
    }

    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn rect(&self, output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        Rectangle::new(output.loc, Size::from((output.size.w, HEIGHT)))
    }

    #[must_use]
    pub fn contains(&self, output: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if self.rect(output).to_f64().contains(pos) {
            return true;
        }
        if self.popup_reveal.is_open()
            && let Some(popup) = self.popup_rect
            && popup.to_f64().contains(pos)
        {
            return true;
        }
        self.popup_reveal.is_open()
            && self.open_popup == Some(OpenPopup::AppMenu)
            && self.menu_panels.iter().any(|panel| panel.rect.to_f64().contains(pos))
    }

    #[must_use]
    pub fn action_at(&self, pos: Point<f64, Logical>) -> Option<TopbarAction> {
        self.slots.iter().find(|(_, rect)| rect.to_f64().contains(pos)).map(|(slot, rect)| match slot {
            Slot::QsVolumeTrack => {
                let ratio = ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                let pct = (ratio * 100.0).round() as u8;
                TopbarAction::QsVolumeAdjust(pct)
            }
            Slot::QsBrightnessTrack => {
                let ratio = ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                let pct = (ratio * 100.0).round() as u8;
                TopbarAction::QsBrightnessAdjust(pct)
            }
            Slot::PopMediaSeek => {
                let ratio = ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                let pct = (ratio * 100.0).round() as u8;
                TopbarAction::MediaSeek(pct)
            }
            other => other.action(),
        })
    }

    #[must_use]
    pub fn slot_at(&self, pos: Point<f64, Logical>) -> Option<Slot> {
        self.slots.iter().find(|(_, rect)| rect.to_f64().contains(pos)).map(|(slot, _)| *slot)
    }

    pub fn set_hover(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        let hovered =
            pos.and_then(|pos| self.slots.iter().find(|(_, rect)| rect.to_f64().contains(pos)).map(|(slot, _)| *slot));
        if self.hovered == hovered {
            return false;
        }
        if let Some(previous) = self.hovered.take()
            && let Some(layer) = self.hover_layers.get_mut(&previous)
        {
            layer.retarget(0.0, motion::t1(), E_IO);
        }
        if let Some(slot) = hovered.filter(|slot| slot.state_layer_radius().is_some()) {
            self.hover_layers.entry(slot).or_default().retarget(1.0, motion::t1(), E_IO);
        }
        self.hovered = hovered;
        self.tray_hover(hovered);
        self.follow_menu_hover(hovered);
        true
    }

    fn segmented(&self) -> bool {
        self.layouts.len() == 2
    }

    fn layout_control_width(&self, painter: &crate::paint::Painter) -> i32 {
        if self.segmented() {
            let track: i32 = self.layouts.iter().map(|name| segment_width(painter, name)).sum();
            SEG_PAD_X * 2 + SEG_INSET * 2 + track + SEG_GAP
        } else if self.layouts.len() >= 3 {
            let label = self.active_layout_name();
            lang_button_width(painter, label)
        } else {
            let label = self.active_layout_name();
            button_width(painter, label)
        }
    }

    fn active_layout_name(&self) -> &str {
        self.layouts.get(self.layout_index).map_or("", String::as_str)
    }

    fn clock_slot_width(&mut self, painter: &crate::paint::Painter) -> i32 {
        let (date_w, time_w) = self.clock_metrics(painter);
        let time_w = time_w.max(clock_time_width(painter, &self.clock));
        let date_w = date_w.max(if self.date.is_empty() { 0 } else { clock_date_width(painter, &self.date) });
        clock_width(date_w, time_w, painter, self.badge_shown())
    }

    fn caffeine_width() -> i32 {
        let glyph = COFFEE_GLYPH.ceil() as i32;
        glide_theme::spacing::SP1 * 2 + glyph + glide_theme::spacing::SP2 + SWITCH_WIDTH
    }

    fn layout_slots(&mut self, painter: &crate::paint::Painter, output: Rectangle<i32, Logical>) {
        let key = (self.rev, output);
        if self.layout_key == Some(key) {
            return;
        }
        self.layout_key = Some(key);
        let strip = self.rect(output);
        let y = strip.loc.y + (HEIGHT - BUTTON_HEIGHT) / 2;
        let mut right = strip.loc.x + strip.size.w - PAD_X;

        let mut slots = Vec::new();

        right = self.layout_status_cluster(painter, right, y, &mut slots);

        right -= STATUS_SEP_WIDTH;

        let coffee_width = Self::caffeine_width();
        right -= coffee_width;
        slots
            .push((Slot::Caffeine, Rectangle::new(Point::from((right, y)), Size::from((coffee_width, BUTTON_HEIGHT)))));

        let mut lang_rect = None;
        if !self.layouts.is_empty() {
            let width = self.layout_control_width(painter);
            right -= ITEM_GAP + width;
            let control_rect = Rectangle::new(Point::from((right, y)), Size::from((width, BUTTON_HEIGHT)));
            lang_rect = Some(control_rect);
            if self.segmented() {
                let mut x = right + SEG_PAD_X + SEG_INSET;
                let top = y + (BUTTON_HEIGHT - SEG_HEIGHT) / 2 + SEG_INSET;
                let height = SEG_HEIGHT - SEG_INSET * 2;
                for (index, name) in self.layouts.iter().enumerate() {
                    let seg = segment_width(painter, name);
                    slots.push((
                        Slot::LayoutPick(index),
                        Rectangle::new(Point::from((x, top)), Size::from((seg, height))),
                    ));
                    x += seg + SEG_GAP;
                }
            } else if self.layouts.len() >= 3 {
                slots.push((Slot::LayoutDropdown, control_rect));
            } else {
                slots.push((Slot::LayoutCycle, control_rect));
            }
        }

        right -= ITEM_GAP + DND_WIDTH;
        slots.push((
            Slot::DoNotDisturb,
            Rectangle::new(Point::from((right, y)), Size::from((DND_WIDTH, BUTTON_HEIGHT))),
        ));

        let clock_w = self.clock_slot_width(painter);
        let clock_x = strip.loc.x + (strip.size.w - clock_w) / 2;
        slots.push((Slot::Clock, Rectangle::new(Point::from((clock_x, y)), Size::from((clock_w, BUTTON_HEIGHT)))));
        let tray_limit = clock_x + clock_w + glide_theme::spacing::SP3;
        self.layout_tray(output, (right - ITEM_GAP, tray_limit, y), &mut slots);

        let media_rect = self.media.as_ref().map(|media| layout_media_slots(painter, media, clock_x, y, &mut slots));

        let menu_limit = media_rect.map_or(clock_x, |rect| rect.loc.x.min(clock_x)) - glide_theme::spacing::SP3;
        let mut menu = self.layout_app_menu(painter, output, strip, menu_limit);
        slots.append(&mut menu.titles);

        let mut popup_slots = std::mem::take(&mut menu.rows);
        let popup_rect = self
            .layout_popup(output, strip.loc.y, (lang_rect, media_rect, menu.anchor), &mut popup_slots)
            .or_else(|| self.layout_status_popup(&slots, output, strip.loc.y));
        if self.popup_reveal.is_open() {
            slots.extend(popup_slots.iter().copied());
        }

        self.slots = slots;
        self.shown_popup_slots = popup_slots;
        self.popup_rect = popup_rect;
        self.menu_panels = menu.panels;
    }

    fn layout_popup(
        &self, output: Rectangle<i32, Logical>, strip_y: i32, anchors: (Anchor, Anchor, Anchor),
        popup_slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> Option<Rectangle<i32, Logical>> {
        let (lang_rect, media_rect, menu_anchor) = anchors;
        match self.shown_popup() {
            Some(OpenPopup::AppMenu) => menu_anchor,
            Some(OpenPopup::LanguageMenu) => {
                lang_rect.map(|btn| layout_lang_menu_slots(&self.layouts, btn, strip_y, popup_slots))
            }
            Some(OpenPopup::NotificationCenter) => Some(layout_nc_slots(&self.nc_items, output, strip_y, popup_slots)),
            Some(OpenPopup::QuickSettings) => Some(layout_qs_slots(
                output,
                strip_y,
                (self.detail_slots(), self.system_status.battery_pct.is_some()),
                popup_slots,
            )),
            Some(OpenPopup::MediaPlayer) => {
                media_rect.map(|anchor| layout_media_popup_slots(anchor, output, strip_y, popup_slots))
            }
            Some(OpenPopup::Privacy) | None => None,
        }
    }

    fn layout_app_menu(
        &self, painter: &crate::paint::Painter, output: Rectangle<i32, Logical>, strip: Rectangle<i32, Logical>,
        limit: i32,
    ) -> MenuLayout {
        let y = strip.loc.y + (HEIGHT - BUTTON_HEIGHT) / 2;
        let left = if self.app_name.is_empty() {
            strip.loc.x + PAD_X
        } else {
            let chip = if self.app_chip_w > 0 {
                self.app_chip_w
            } else {
                app_chip_width(painter, &self.app_name, !self.app_id.is_empty())
            };
            strip.loc.x + PAD_X + chip + ITEM_GAP * 2
        };
        let title_rects = layout_menu_titles(painter, &self.menu_bar, left, limit, y);
        let titles = title_rects
            .iter()
            .filter_map(|(index, rect)| Some((Slot::MenuTitle(u16::try_from(*index).ok()?), *rect)))
            .collect();
        let anchor = self.menu_anchor(&title_rects);
        let panels = if self.shown_popup() == Some(OpenPopup::AppMenu) {
            anchor
                .map(|title| {
                    crate::global_menu::layout_panels(
                        painter,
                        self.menu_items(),
                        self.menu_open,
                        title,
                        strip.loc.y + HEIGHT + glide_theme::spacing::SP1,
                        output,
                    )
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let rows = panels
            .iter()
            .rev()
            .flat_map(|panel| panel.rows.iter())
            .filter(|row| row.enabled)
            .map(|row| (Slot::MenuRow(row.path), row.rect))
            .collect();
        MenuLayout { titles, rows, anchor: panels.first().map(|panel| panel.rect), panels }
    }

    fn badge_shown(&self) -> usize {
        if self.unread > 0 {
            self.unread
        } else if self.badge.reserved {
            self.badge.count
        } else {
            0
        }
    }

    #[must_use]
    pub fn slot_rect(&self, slot: Slot) -> Option<Rectangle<i32, Logical>> {
        self.slots.iter().find(|(candidate, _)| *candidate == slot).map(|(_, rect)| *rect)
    }

    fn layout_control_rect(&self) -> Option<Rectangle<i32, Logical>> {
        if self.segmented() {
            let first = self.slot_rect(Slot::LayoutPick(0))?;
            let last = self.slot_rect(Slot::LayoutPick(self.layouts.len() - 1))?;
            let x = first.loc.x - SEG_PAD_X - SEG_INSET;
            let width = (last.loc.x + last.size.w + SEG_INSET + SEG_PAD_X) - x;
            Some(Rectangle::new(
                Point::from((x, first.loc.y - SEG_INSET - (BUTTON_HEIGHT - SEG_HEIGHT) / 2)),
                Size::from((width, BUTTON_HEIGHT)),
            ))
        } else if self.layouts.len() >= 3 {
            self.slot_rect(Slot::LayoutDropdown)
        } else {
            self.slot_rect(Slot::LayoutCycle)
        }
    }
}

fn layout_menu_titles(
    painter: &crate::paint::Painter, bar: &[glide_appmenu::MenuItem], left: i32, limit: i32, y: i32,
) -> Vec<(usize, Rectangle<i32, Logical>)> {
    let mut x = left;
    let mut out = Vec::new();
    for (index, item) in bar.iter().enumerate() {
        if !item.is_submenu() {
            continue;
        }
        let width = crate::global_menu::title_width(painter, &item.label);
        if x + width > limit {
            break;
        }
        out.push((index, Rectangle::new(Point::from((x, y)), Size::from((width, BUTTON_HEIGHT)))));
        x += width;
    }
    out
}

fn layout_lang_menu_slots(
    layouts: &[String], btn: Rectangle<i32, Logical>, strip_y: i32, slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let menu_w = LANG_MENU_WIDTH;
    let menu_x = btn.loc.x + btn.size.w - menu_w;
    let menu_y = strip_y + HEIGHT + POPUP_GAP;
    let count = i32::try_from(layouts.len()).unwrap_or(0);
    let menu_h = LANG_MENU_PAD * 2 + count * LANG_MENU_ITEM_HEIGHT + LANG_MENU_DIVIDER + LANG_MENU_ITEM_HEIGHT;
    let rect = Rectangle::new(Point::from((menu_x, menu_y)), Size::from((menu_w, menu_h)));

    let mut item_y = menu_y + LANG_MENU_PAD;
    for index in 0..layouts.len() {
        slots.push((
            Slot::LangMenuItem(index),
            Rectangle::new(
                Point::from((menu_x + LANG_MENU_PAD, item_y)),
                Size::from((menu_w - LANG_MENU_PAD * 2, LANG_MENU_ITEM_HEIGHT)),
            ),
        ));
        item_y += LANG_MENU_ITEM_HEIGHT;
    }
    item_y += LANG_MENU_DIVIDER;
    slots.push((
        Slot::LangMenuSettings,
        Rectangle::new(
            Point::from((menu_x + LANG_MENU_PAD, item_y)),
            Size::from((menu_w - LANG_MENU_PAD * 2, LANG_MENU_ITEM_HEIGHT)),
        ),
    ));
    rect
}

fn layout_nc_slots(
    items: &[crate::notifications::NcItem], output: Rectangle<i32, Logical>, strip_y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let nc_w = NC_WIDTH;
    let nc_x = output.loc.x + (output.size.w - nc_w) / 2;
    let nc_y = strip_y + HEIGHT + POPUP_GAP;
    let count = items.len().min(NC_MAX_ITEMS);
    let content_h = if count == 0 { NC_EMPTY_HEIGHT } else { i32::try_from(count).unwrap_or(0) * NC_ITEM_HEIGHT };
    let nc_h = NC_TOP + NC_HEADER_HEIGHT + NC_GAP + content_h + NC_PAD;
    let rect = Rectangle::new(Point::from((nc_x, nc_y)), Size::from((nc_w, nc_h)));
    calendar::layout_slots(rect.loc, nc_w, slots);

    let head_y = nc_y + NC_TOP;
    let sw_x = nc_x + nc_w - NC_PAD - SWITCH_WIDTH;
    slots.push((
        Slot::NcDndToggle,
        Rectangle::new(Point::from((sw_x, head_y)), Size::from((SWITCH_WIDTH, NC_HEADER_HEIGHT))),
    ));

    let moon_x = sw_x - 8 - 14;
    let clear_w = 96;
    let clear_x = moon_x - 10 - clear_w;
    slots.push((
        Slot::NcClearAll,
        Rectangle::new(Point::from((clear_x, head_y)), Size::from((clear_w, NC_HEADER_HEIGHT))),
    ));

    let mut item_y = nc_y + NC_TOP + NC_HEADER_HEIGHT + NC_GAP;
    for item in items.iter().take(NC_MAX_ITEMS) {
        let x_side = 22;
        let x_left = nc_x + nc_w - NC_PAD - x_side;
        let x_top = item_y + 24;
        slots.push((
            Slot::NcItemDismiss(item.id),
            Rectangle::new(Point::from((x_left, x_top)), Size::from((x_side, x_side))),
        ));
        slots.push((
            Slot::NcItem(item.id),
            Rectangle::new(Point::from((nc_x + NC_PAD, item_y)), Size::from((nc_w - NC_PAD * 2, NC_ITEM_HEIGHT))),
        ));
        item_y += NC_ITEM_HEIGHT;
    }
    rect
}

type LocalRect = Rectangle<i32, Logical>;

struct QsGeometry {
    tiles: [(Slot, LocalRect); 4],
    caffeine: LocalRect,
    volume_icon: LocalRect,
    volume_track: LocalRect,
    brightness_icon: LocalRect,
    brightness_track: LocalRect,
    divider_y: i32,
    footer: LocalRect,
    battery: LocalRect,
    settings: LocalRect,
    power: [(Slot, LocalRect); 5],
    height: i32,
}

use crate::raster::rect as local_rect;

const fn band(rect: LocalRect) -> (i32, i32, i32, i32) {
    (rect.loc.x, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + rect.size.h)
}

fn qs_geometry() -> QsGeometry {
    let row_w = QS_WIDTH - QS_PAD * 2;
    let col_w = (row_w - QS_GRID_GAP) / 2;
    let col2_x = QS_PAD + col_w + QS_GRID_GAP;
    let row1_y = QS_PAD;
    let row2_y = row1_y + QS_TILE_HEIGHT + QS_GRID_GAP;
    let tiles = [
        (Slot::QsWifi, local_rect(QS_PAD, row1_y, col_w, QS_TILE_HEIGHT)),
        (Slot::QsBluetooth, local_rect(col2_x, row1_y, col_w, QS_TILE_HEIGHT)),
        (Slot::QsNightLight, local_rect(QS_PAD, row2_y, col_w, QS_TILE_HEIGHT)),
        (Slot::QsScreenshot, local_rect(col2_x, row2_y, col_w, QS_TILE_HEIGHT)),
    ];

    let caffeine_y = row2_y + QS_TILE_HEIGHT + QS_GAP;
    let caffeine = local_rect(QS_PAD, caffeine_y, row_w, QS_ROW_HEIGHT);

    let track_x = QS_PAD + QS_SLIDER_ICON + QS_GRID_GAP;
    let track_w = QS_PAD + row_w - track_x;
    let volume_y = caffeine_y + QS_ROW_HEIGHT + QS_GAP;
    let brightness_y = volume_y + QS_SLIDER_HEIGHT + QS_SLIDER_GAP;

    let divider_y = brightness_y + QS_SLIDER_HEIGHT + QS_GAP;
    let footer_y = divider_y + 1 + QS_GAP;
    let footer = local_rect(QS_PAD, footer_y, row_w, QS_ACT_BTN);
    let settings = local_rect(QS_PAD + row_w - QS_ACT_BTN, footer_y, QS_ACT_BTN, QS_ACT_BTN);
    let battery = local_rect(settings.loc.x - QS_GRID_GAP - QS_BATTERY_W, footer_y, QS_BATTERY_W, QS_ACT_BTN);
    let power_y = footer_y + QS_ACT_BTN + QS_GAP;
    let power_count = i32::try_from(panels::POWER_ROW.len()).unwrap_or(1);
    let power_gap = (row_w - panels::QS_POWER_BTN * power_count) / (power_count - 1).max(1);
    let mut power_x = QS_PAD;
    let power = panels::POWER_ROW.map(|action| {
        let rect = local_rect(power_x, power_y, panels::QS_POWER_BTN, panels::QS_POWER_BTN);
        power_x += panels::QS_POWER_BTN + power_gap;
        (Slot::QsPowerButton(action), rect)
    });

    QsGeometry {
        tiles,
        caffeine,
        volume_icon: local_rect(QS_PAD, volume_y, QS_SLIDER_ICON, QS_SLIDER_HEIGHT),
        volume_track: local_rect(track_x, volume_y, track_w, QS_SLIDER_HEIGHT),
        brightness_icon: local_rect(QS_PAD, brightness_y, QS_SLIDER_ICON, QS_SLIDER_HEIGHT),
        brightness_track: local_rect(track_x, brightness_y, track_w, QS_SLIDER_HEIGHT),
        divider_y,
        footer,
        battery,
        settings,
        power,
        height: power_y + panels::QS_POWER_BTN + QS_PAD,
    }
}

fn offset_rect(rect: LocalRect, origin: Point<i32, Logical>) -> LocalRect {
    Rectangle::new(rect.loc + origin, rect.size)
}

fn rect_centre(rect: LocalRect) -> Point<i32, Logical> {
    Point::from((rect.loc.x + rect.size.w / 2, rect.loc.y + rect.size.h / 2))
}

const fn spans_overlap(a_start: i32, a_len: i32, b_start: i32, b_len: i32) -> bool {
    a_start < b_start + b_len && b_start < a_start + a_len
}

fn popup_slots(popup: OpenPopup) -> fn(Slot) -> bool {
    match popup {
        OpenPopup::QuickSettings => Slot::in_quick_settings,
        OpenPopup::NotificationCenter => Slot::in_notification_center,
        OpenPopup::LanguageMenu => Slot::in_lang_menu,
        OpenPopup::MediaPlayer => Slot::in_media_popup,
        OpenPopup::AppMenu => Slot::in_app_menu,
        OpenPopup::Privacy => Slot::in_privacy_popup,
    }
}

const FOCUS_RING: i32 = 2;
const FOCUS_RING_STROKE: f32 = 2.0;

fn focus_ring_bounds(rect: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.loc.x - FOCUS_RING, rect.loc.y - FOCUS_RING)),
        Size::from((rect.size.w + FOCUS_RING * 2, rect.size.h + FOCUS_RING * 2)),
    )
}

fn draw_layer(
    layer: Layer, size: Size<i32, Logical>, radius: f32, theme: &TopbarTheme, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(size.w, size.h, scale)?;
    match layer {
        Layer::Focus => c.stroke(c.full(), FOCUS_RING_STROKE, radius, layer.colour(theme)),
        Layer::Hover | Layer::Press => c.fill(c.full(), layer.colour(theme), radius),
    }
    Some(c.finish())
}

#[derive(Debug, Clone, Copy)]
struct LayerJob {
    layer: Layer,
    rect: Rectangle<i32, Logical>,
    radius: f32,
    alpha: f32,
    in_popup: bool,
}

impl Topbar {
    fn layer_buffer(
        &mut self, layer: Layer, size: Size<i32, Logical>, radius: f32, scale: i32,
    ) -> Option<&MemoryRenderBuffer> {
        let key = (layer.tag(), size.w, size.h, radius.to_bits(), scale);
        if let std::collections::hash_map::Entry::Vacant(entry) = self.layers.entry(key) {
            entry.insert(draw_layer(layer, size, radius, &self.theme, scale)?);
        }
        self.layers.get(&key)
    }

    fn drawn_slot_rect(&self, slot: Slot) -> Option<Rectangle<i32, Logical>> {
        self.slot_rect(slot)
            .or_else(|| self.shown_popup_slots.iter().find(|(candidate, _)| *candidate == slot).map(|(_, rect)| *rect))
    }

    fn press_level(&self, slot: Slot) -> f64 {
        self.press_layers.get(&slot).map_or(0.0, |anim| anim.value().clamp(0.0, 1.0))
    }

    fn layer_jobs(&self) -> Vec<LayerJob> {
        let mut jobs = Vec::new();
        let pairs = [(Layer::Hover, &self.hover_layers), (Layer::Press, &self.press_layers)];
        for (layer, anims) in pairs {
            for (slot, anim) in anims {
                let mut level = anim.value().clamp(0.0, 1.0);
                if layer == Layer::Hover {
                    level *= 1.0 - self.press_level(*slot);
                }
                let alpha = level as f32;
                if alpha <= 0.0 {
                    continue;
                }
                let (Some(radius), Some(rect)) = (slot.state_layer_radius(), self.drawn_slot_rect(*slot)) else {
                    continue;
                };
                jobs.push(LayerJob { layer, rect, radius, alpha, in_popup: !slot.in_strip() });
            }
        }
        let focus = self.focus_anim.value().clamp(0.0, 1.0) as f32;
        if focus > 0.0
            && let Some(rect) = self.focus_ring_rect().or(self.focus_ring_last)
        {
            jobs.push(LayerJob {
                layer: Layer::Focus,
                rect: focus_ring_bounds(rect),
                radius: glide_theme::shape::R2 + FOCUS_RING_STROKE,
                alpha: focus,
                in_popup: true,
            });
        }
        jobs
    }
}

fn push_state_layers<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, _, _, scale, output_scale, elements) = frame.parts();
    let reveal = topbar.popup_progress();
    let popup = topbar.popup_rect;
    for job in topbar.layer_jobs() {
        let placed = match (job.in_popup, popup) {
            (true, Some(popup)) => Placed { popup, element: job.rect, progress: reveal, alpha: job.alpha },
            (true, None) => continue,
            (false, _) => Placed { popup: job.rect, element: job.rect, progress: 1.0, alpha: job.alpha },
        };
        let Some(buffer) = topbar.layer_buffer(job.layer, job.rect.size, job.radius, output_scale) else {
            continue;
        };
        push_placed(renderer, buffer, placed, scale, elements);
    }
}

fn layout_qs_slots(
    output: Rectangle<i32, Logical>, strip_y: i32, (detail, battery): (Option<Vec<(Slot, LocalRect)>>, bool),
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let geometry = qs_geometry();
    let origin = Point::from((output.loc.x + output.size.w - PAD_X - QS_WIDTH, strip_y + HEIGHT + POPUP_GAP));
    let popup = Rectangle::new(origin, Size::from((QS_WIDTH, geometry.height)));
    if let Some(detail) = detail {
        slots.extend(detail.into_iter().map(|(slot, rect)| (slot, offset_rect(rect, origin))));
        return popup;
    }

    for (slot, rect) in geometry.tiles {
        let more = match slot {
            Slot::QsWifi => Some(Slot::QsWifiMore),
            Slot::QsBluetooth => Some(Slot::QsBluetoothMore),
            _ => None,
        };
        if let Some(more) = more {
            let (body, chevron) = panels::split_tile(rect);
            slots.push((slot, offset_rect(body, origin)));
            slots.push((more, offset_rect(chevron, origin)));
        } else {
            slots.push((slot, offset_rect(rect, origin)));
        }
    }
    slots.push((Slot::QsCaffeine, offset_rect(geometry.caffeine, origin)));
    slots.push((Slot::QsVolumeMute, offset_rect(geometry.volume_icon, origin)));
    slots.push((Slot::QsVolumeTrack, offset_rect(geometry.volume_track, origin)));
    slots.push((Slot::QsBrightnessTrack, offset_rect(geometry.brightness_track, origin)));
    if battery {
        slots.push((Slot::QsBattery, offset_rect(geometry.battery, origin)));
    }
    slots.push((Slot::QsSettings, offset_rect(geometry.settings, origin)));
    for (slot, rect) in geometry.power {
        slots.push((slot, offset_rect(rect, origin)));
    }

    popup
}

#[must_use]
pub fn native_layout_name(code: &str) -> &'static str {
    match code.to_ascii_uppercase().as_str() {
        "UA" | "UKR" => "Українська",
        "US" | "ENG" | "EN" => "English",
        "PL" | "POL" => "Polski",
        "DE" | "DEU" | "GER" => "Deutsch",
        "FR" | "FRA" => "Français",
        "ES" | "ESP" => "Español",
        "IT" | "ITA" => "Italiano",
        "PT" | "POR" => "Português",
        "CZ" | "CZE" => "Čeština",
        "SK" | "SLK" => "Slovenčina",
        "NL" | "NLD" => "Nederlands",
        "SE" | "SWE" => "Svenska",
        "NO" | "NOR" => "Norsk",
        "FI" | "FIN" => "Suomi",
        "DK" | "DAN" => "Dansk",
        "TR" | "TUR" => "Türkçe",
        "RO" | "RON" => "Română",
        "HU" | "HUN" => "Magyar",
        "EL" | "GRE" => "Ελληνικά",
        "JA" | "JPN" => "日本語",
        "ZH" | "CHI" => "中文",
        "KO" | "KOR" => "한국어",
        _ => "",
    }
}

fn lang_button_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    (text_w(painter, label, &TextStyle::TOPBAR) + 29).max(BUTTON_MIN_WIDTH)
}

#[must_use]
pub fn short_layout_name(name: &str) -> String {
    let inner = name.rsplit_once('(').and_then(|(_, tail)| tail.split_once(')')).map(|(inside, _)| inside.trim());

    let candidate = inner.filter(|inside| !inside.is_empty()).unwrap_or(name);
    let word = candidate.split_whitespace().next().unwrap_or(candidate);
    let short: String = word.chars().take(3).collect();
    short.to_uppercase()
}

#[must_use]
pub fn app_display_name(app_id: &str, title: &str) -> String {
    let trimmed = app_id.trim();
    if trimmed.is_empty() {
        return truncate_name(title.trim());
    }
    let tail = trimmed.rsplit('.').next().unwrap_or(trimmed);
    let tail = tail.trim();
    if tail.is_empty() {
        return truncate_name(title.trim());
    }
    let mut chars = tail.chars();
    let name = match chars.next() {
        Some(first) if first.is_lowercase() => first.to_uppercase().collect::<String>() + chars.as_str(),
        _ => tail.to_string(),
    };
    truncate_name(&name)
}

fn truncate_name(name: &str) -> String {
    if name.chars().count() <= APP_NAME_MAX {
        return name.to_string();
    }
    let kept: String = name.chars().take(APP_NAME_MAX - 1).collect();
    format!("{}…", kept.trim_end())
}

fn ensure_time_locale() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: `setlocale` is called once, before any thread reads the
        // locale, and the empty string asks libc to honour the environment.
        unsafe { libc::setlocale(libc::LC_TIME, c"".as_ptr()) };
    });
}

fn format_local(format: &std::ffi::CStr) -> Option<String> {
    ensure_time_locale();

    let mut raw: libc::time_t = 0;
    // SAFETY: `time` writes a single `time_t` through a pointer we own.
    let now = unsafe { libc::time(&raw mut raw) };
    if now == -1 {
        return None;
    }
    calendar::refresh_timezone(now);

    // SAFETY: `tm` is a plain C struct of integers and a pointer for which
    // all-zero bytes is a valid value; `localtime_r` overwrites it below.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers address locals that outlive the call, and
    // `localtime_r` writes only the `tm` we hand it.
    let filled = unsafe { libc::localtime_r(&raw const now, &raw mut tm) };
    if filled.is_null() {
        return None;
    }

    let mut out = [0u8; 96];
    // SAFETY: `out` is a live buffer of the length we pass, `format` is a
    // NUL-terminated string, and `tm` was filled by `localtime_r` above.
    let written = unsafe { libc::strftime(out.as_mut_ptr().cast(), out.len(), format.as_ptr(), &raw const tm) };
    if written == 0 {
        return None;
    }
    std::str::from_utf8(&out[..written]).ok().map(collapse_spaces)
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[must_use]
pub fn local_clock() -> Option<String> {
    format_local(c"%H:%M")
}

#[must_use]
pub fn local_minute_of_day() -> Option<u16> {
    let clock = local_clock()?;
    let (hours, minutes) = clock.split_once(':')?;
    let hours: u16 = hours.parse().ok()?;
    let minutes: u16 = minutes.parse().ok()?;
    Some(hours * 60 + minutes)
}

fn button_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    (text_w(painter, label, &TextStyle::TOPBAR) + BUTTON_PAD_X * 2).max(BUTTON_MIN_WIDTH)
}

fn segment_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    (text_w(painter, label, &TextStyle::TOPBAR) + SP1 * 2).max(SEG_MIN_WIDTH)
}

pub const MEDIA_ART: i32 = 18;
const MEDIA_PAD: i32 = 3;
const MEDIA_GAP: i32 = glide_theme::spacing::SP2;
const MEDIA_TAG_GAP: i32 = 6;
const MEDIA_TITLE_MAX: i32 = 104;
const MEDIA_ARTIST_MAX: i32 = 58;
const MEDIA_BTN: i32 = 22;
const MEDIA_BTN_GAP: i32 = 2;
const MEDIA_SEP: i32 = 5;
const MEDIA_TINT: f32 = 0.14;
const MEDIA_LINE: i32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct MediaWidget {
    pub title: String,
    pub artist: String,
    pub source: String,
    pub art: Option<Vec<u8>>,
    pub art_large: Option<Vec<u8>>,
    pub app_icons: Vec<String>,
    pub accent: Color,
    pub playing: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub progress: Option<f32>,
}

impl MediaWidget {
    #[must_use]
    pub fn progress_permille(&self) -> Option<u16> {
        progress_permille(self.progress)
    }
}

fn progress_permille(progress: Option<f32>) -> Option<u16> {
    progress.map(|value| (value.clamp(0.0, 1.0) * 1000.0).round() as u16)
}

const fn transport_width() -> i32 {
    MEDIA_BTN * 3 + MEDIA_BTN_GAP * 2
}

fn clipped_width(painter: &crate::paint::Painter, text: &str, cap: i32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    text_w(painter, text, &TextStyle::TOPBAR).min(cap)
}

fn media_width(painter: &crate::paint::Painter, media: &MediaWidget) -> i32 {
    let title = clipped_width(painter, &media.title, MEDIA_TITLE_MAX);
    let artist = clipped_width(painter, &media.artist, MEDIA_ARTIST_MAX);
    let tags = if artist > 0 { title + MEDIA_TAG_GAP + artist } else { title };
    MEDIA_PAD + MEDIA_ART + MEDIA_GAP + tags + MEDIA_GAP + transport_width() + MEDIA_PAD
}

const MP_WIDTH: i32 = 300;
const MP_HEIGHT: i32 = 158;
const MP_PAD: i32 = glide_theme::spacing::SP3;
pub const MEDIA_ART_LARGE: i32 = 56;
const MP_ART: i32 = MEDIA_ART_LARGE;
const MP_TRACK_H: i32 = 4;
const MP_BTN: i32 = 34;
const MP_BTN_MAIN: i32 = 40;
const MP_BTN_GAP: i32 = glide_theme::spacing::SP2;

fn layout_media_popup_slots(
    anchor: Rectangle<i32, Logical>, output: Rectangle<i32, Logical>, strip_y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let centred = anchor.loc.x + (anchor.size.w - MP_WIDTH) / 2;
    let max_x = output.loc.x + output.size.w - PAD_X - MP_WIDTH;
    let x = centred.clamp(output.loc.x + PAD_X, max_x.max(output.loc.x + PAD_X));
    let y = strip_y + HEIGHT + POPUP_GAP;

    let track_y = y + MP_PAD + MP_ART + MP_PAD;
    let seek = local_rect(x + MP_PAD, track_y - 6, MP_WIDTH - MP_PAD * 2, MP_TRACK_H + 12);
    slots.push((Slot::PopMediaSeek, seek));
    let origin = Point::from((x, y));
    let buttons = [Slot::PopMediaPrev, Slot::PopMediaPlay, Slot::PopMediaNext];
    for (slot, rect) in buttons.into_iter().zip(popup_transport()) {
        slots.push((slot, offset_rect(rect, origin)));
    }
    local_rect(x, y, MP_WIDTH, MP_HEIGHT)
}

fn popup_transport() -> [LocalRect; 3] {
    let row_y = MP_HEIGHT - MP_PAD - MP_BTN_MAIN;
    let row_w = MP_BTN * 2 + MP_BTN_MAIN + MP_BTN_GAP * 2;
    let mut x = (MP_WIDTH - row_w) / 2;
    [MP_BTN, MP_BTN_MAIN, MP_BTN].map(|side| {
        let rect = local_rect(x, row_y + (MP_BTN_MAIN - side) / 2, side, side);
        x += side + MP_BTN_GAP;
        rect
    })
}

fn strip_transport(width: i32) -> [LocalRect; 3] {
    let x = width - MEDIA_PAD - transport_width();
    let y = (BUTTON_HEIGHT - MEDIA_BTN) / 2;
    [0, 1, 2].map(|i| local_rect(x + i * (MEDIA_BTN + MEDIA_BTN_GAP), y, MEDIA_BTN, MEDIA_BTN))
}

fn layout_media_slots(
    painter: &crate::paint::Painter, media: &MediaWidget, clock_x: i32, y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let width = media_width(painter, media);
    let left = clock_x - MEDIA_SEP - width;
    let origin = Point::from((left, y));
    let buttons = [Slot::MediaPrev, Slot::MediaPlay, Slot::MediaNext];
    for (slot, rect) in buttons.into_iter().zip(strip_transport(width)) {
        slots.push((slot, offset_rect(rect, origin)));
    }
    let body = local_rect(left, y, width - transport_width() - MEDIA_PAD, BUTTON_HEIGHT);
    slots.push((Slot::Media, body));
    local_rect(left, y, width, BUTTON_HEIGHT)
}

fn push_media<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let Some(rect) = topbar.slot_rect(Slot::Media) else {
        return;
    };
    let Some(media) = topbar.media.as_ref() else {
        return;
    };
    let width = media_width(painter, media);
    let theme = &topbar.theme;
    let seek = topbar.seek_permille();
    let progress = seek.map(|permille| f32::from(permille) / 1000.0);

    let buffer = topbar.media_cache.resolve((topbar.media_rev, seek), output_scale, || {
        draw_media(painter, theme, media, progress, width, output_scale, Some(icons))
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_media(
    painter: &crate::paint::Painter, theme: &TopbarTheme, media: &MediaWidget, progress: Option<f32>, width: i32,
    scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?.with_icons(icons);
    let [red, green, blue, _] = media.accent.0;
    c.panel(Color([red, green, blue, MEDIA_TINT]), BUTTON_RADIUS);

    let art_top = (BUTTON_HEIGHT - MEDIA_ART) / 2;
    let art = (media.art.as_deref(), (MEDIA_PAD, art_top, MEDIA_ART));
    draw_media_art(&mut c, painter, media, art, R1);

    let style = &TextStyle::TOPBAR;
    let mut x = MEDIA_PAD + MEDIA_ART + MEDIA_GAP;
    let title = clipped_width(painter, &media.title, MEDIA_TITLE_MAX);
    c.label(painter, (x, x + title), TextAlign::Left, &media.title, theme.label, style);
    x += title;

    let artist = clipped_width(painter, &media.artist, MEDIA_ARTIST_MAX);
    if artist > 0 {
        x += MEDIA_TAG_GAP;
        c.label(painter, (x, x + artist), TextAlign::Left, &media.artist, theme.faint, style);
    }

    for (index, rect) in strip_transport(width).into_iter().enumerate() {
        let colour = if !transport_enabled(media, index) {
            theme.faint
        } else if index == 1 {
            media.accent
        } else {
            theme.muted
        };
        draw_transport_button(&mut c, media, index, rect, (MEDIA_BTN, colour));
    }

    if let Some(progress) = progress {
        let filled = (((width - MEDIA_PAD * 2) as f32) * progress.clamp(0.0, 1.0)).round() as i32;
        if filled > 0 {
            let line = local_rect(MEDIA_PAD, BUTTON_HEIGHT - MEDIA_LINE - 1, filled, MEDIA_LINE);
            c.fill(line, media.accent, R_ROUND);
        }
    }

    Some(c.finish())
}

const fn transport_enabled(media: &MediaWidget, index: usize) -> bool {
    match index {
        0 => media.can_previous,
        2 => media.can_next,
        _ => true,
    }
}

fn draw_transport_button(
    c: &mut Canvas<'_>, media: &MediaWidget, index: usize, rect: LocalRect, (icon, colour): (i32, Color),
) {
    use crate::paint::MediaGlyph;
    let (name, glyph) = match index {
        0 => ("media-skip-backward", MediaGlyph::Previous),
        2 => ("media-skip-forward", MediaGlyph::Next),
        _ if media.playing => ("media-playback-pause", MediaGlyph::Pause),
        _ => ("media-playback-start", MediaGlyph::Play),
    };
    let (x, y, side) = (rect.loc.x, rect.loc.y, rect.size.w);
    let at = (x + (side - icon) / 2, y + (side - icon) / 2);
    if !c.icon(name, icon, at, colour) {
        crate::paint::draw_media_glyph_over(&mut c.buf, c.w, c.h, x, y, side, c.scale, colour, glyph);
    }
}

fn push_media_popup<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    if topbar.shown_popup() != Some(OpenPopup::MediaPlayer) {
        return;
    }
    let (Some(rect), Some(media)) = (topbar.popup_rect, topbar.media.as_ref()) else {
        return;
    };
    let theme = &topbar.theme;
    let seek = topbar.seek_permille();
    let progress = seek.map(|permille| f32::from(permille) / 1000.0);
    let reveal = topbar.popup_progress();

    let buffer = topbar.media_popup_cache.resolve((topbar.media_rev, seek), output_scale, || {
        draw_media_popup(painter, theme, media, progress, output_scale, Some(icons))
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PopupPlacement {
    position: Point<f64, Logical>,
    size: Size<i32, Logical>,
}

fn popup_placement(popup: Rectangle<i32, Logical>, element: Rectangle<i32, Logical>, progress: f64) -> PopupPlacement {
    let p = progress.clamp(0.0, 1.0);
    let zoom = POPUP_SCALE_FROM + (1.0 - POPUP_SCALE_FROM) * p;
    let rise = -POPUP_RISE * (1.0 - p);
    let anchor_x = f64::from(popup.loc.x) + f64::from(popup.size.w) / 2.0;
    let anchor_y = f64::from(popup.loc.y);
    let x = anchor_x + (f64::from(element.loc.x) - anchor_x) * zoom;
    let y = anchor_y + (f64::from(element.loc.y) - anchor_y) * zoom + rise;
    let size = Size::from((
        (f64::from(element.size.w) * zoom).round() as i32,
        (f64::from(element.size.h) * zoom).round() as i32,
    ));
    PopupPlacement { position: Point::from((x, y)), size }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Placed {
    popup: Rectangle<i32, Logical>,
    element: Rectangle<i32, Logical>,
    progress: f64,
    alpha: f32,
}

fn push_placed<R>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, placed: Placed, scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let progress = placed.progress.clamp(0.0, 1.0);
    if progress <= 0.0 || placed.alpha <= 0.0 {
        return;
    }
    let alpha = (placed.alpha * progress as f32).clamp(0.0, 1.0);
    let (position, src, size) = if progress >= 1.0 {
        (placed.element.loc.to_f64(), None, None)
    } else {
        let placement = popup_placement(placed.popup, placed.element, progress);
        (placement.position, Some(Rectangle::from_size(placed.element.size.to_f64())), Some(placement.size))
    };
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        position.to_physical(scale),
        buffer,
        Some(alpha),
        src,
        size,
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn push_popup_buffer<R>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, popup: Rectangle<i32, Logical>, element: Rectangle<i32, Logical>,
    progress: f64, scale: Scale<f64>, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    push_placed(renderer, buffer, Placed { popup, element, progress, alpha: 1.0 }, scale, elements);
}

#[allow(clippy::too_many_arguments)]
fn push_popup_shadow<R>(
    topbar: &mut Topbar, renderer: &mut R, rect: Rectangle<i32, Logical>, progress: f64, scale: Scale<f64>,
    output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let margin = popup_shadow_margin(topbar.theme.dark);
    let key = (rect.size.w, rect.size.h, output_scale);
    if let std::collections::hash_map::Entry::Vacant(entry) = topbar.shadows.entry(key)
        && let Some(buffer) = draw_popup_shadow(topbar.theme.dark, rect.size, margin, output_scale)
    {
        entry.insert(buffer);
    }
    let Some(buffer) = topbar.shadows.get(&key) else {
        return;
    };
    let shadow = Rectangle::new(
        Point::from((rect.loc.x - margin, rect.loc.y - margin)),
        Size::from((rect.size.w + margin * 2, rect.size.h + margin * 2)),
    );
    push_popup_buffer(renderer, buffer, rect, shadow, progress, scale, elements);
}

fn popup_shadow_layers(dark: bool) -> Vec<glide_theme::elevation::ShadowLayer> {
    let appearance = if dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light };
    glide_theme::elevation::layers(glide_theme::elevation::Level::El3, appearance)
}

fn popup_shadow_margin(dark: bool) -> i32 {
    let reach = popup_shadow_layers(dark).iter().map(|layer| layer.blur + layer.dy).fold(0.0_f32, f32::max);
    reach.ceil() as i32 + glide_theme::spacing::SP1
}

fn draw_popup_shadow(dark: bool, size: Size<i32, Logical>, margin: i32, scale: i32) -> Option<MemoryRenderBuffer> {
    Some(crate::raster::shadow((size.w, size.h), margin, POPUP_RADIUS, &popup_shadow_layers(dark), scale)?.finish())
}

fn draw_media_art(
    c: &mut Canvas<'_>, painter: &crate::paint::Painter, media: &MediaWidget,
    (art, (x, y, side)): (Option<&[u8]>, (i32, i32, i32)), radius: f32,
) {
    if let Some(art) = art {
        c.blit_square(art, side, (x, y));
        return;
    }
    if let Some(loader) = c.icons.as_deref_mut() {
        for candidate in &media.app_icons {
            if let Some(pix) = loader.load(candidate, side * c.scale) {
                crate::paint::blit_square_over(&mut c.buf, c.w, c.h, pix, side, x, y, c.scale);
                return;
            }
        }
    }
    c.fill(local_rect(x, y, side, side), media.accent, radius);
    let white = Color([1.0, 1.0, 1.0, 1.0]);
    let glyph = side * 3 / 5;
    let offset = (side - glyph) / 2;
    if c.icon("audio-x-generic-symbolic", glyph, (x + offset, y + offset), white) {
        return;
    }
    let style = if side >= MP_ART { &TextStyle::DISPLAY } else { &TextStyle::TOPBAR };
    c.text(painter, (x, x + side, y, y + side), TextAlign::Center, "♪", white, style);
}

fn draw_media_popup(
    painter: &crate::paint::Painter, theme: &TopbarTheme, media: &MediaWidget, progress: Option<f32>, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::popover((MP_WIDTH, MP_HEIGHT), scale, R3, theme.popover_bg, theme.border)?.with_icons(icons);
    let art = (media.art_large.as_deref(), (MP_PAD, MP_PAD, MP_ART));
    draw_media_art(&mut c, painter, media, art, R2);

    let (left, right) = (MP_PAD + MP_ART + MP_PAD, MP_WIDTH - MP_PAD);
    let lines = [
        (6, 26, &media.title, theme.label, &TextStyle::HEAD),
        (26, 44, &media.artist, theme.muted, &TextStyle::CALL),
        (44, 58, &media.source, theme.faint, &TextStyle::CAP),
    ];
    for (top, bottom, text, colour, style) in lines {
        let area = (left, right, MP_PAD + top, MP_PAD + bottom);
        c.text(painter, area, TextAlign::Left, text, colour, style);
    }

    draw_media_popup_track(&mut c, theme, media, progress);
    for (index, rect) in popup_transport().into_iter().enumerate() {
        let main = index == 1;
        if main {
            c.fill(rect, media.accent, R_ROUND);
        } else {
            c.fill(rect, theme.track_soft, R2);
        }
        let colour = if main {
            theme.popover_bg
        } else if transport_enabled(media, index) {
            theme.label
        } else {
            theme.faint
        };
        let icon = if main { 18 } else { 14 };
        draw_transport_button(&mut c, media, index, rect, (icon, colour));
    }

    Some(c.finish())
}

fn draw_media_popup_track(c: &mut Canvas<'_>, theme: &TopbarTheme, media: &MediaWidget, progress: Option<f32>) {
    let y = MP_PAD + MP_ART + MP_PAD;
    let width = MP_WIDTH - MP_PAD * 2;
    c.fill(local_rect(MP_PAD, y, width, MP_TRACK_H), theme.track_soft, R_ROUND);
    let Some(progress) = progress else {
        return;
    };
    let filled = ((width as f32) * progress.clamp(0.0, 1.0)).round() as i32;
    if filled > 0 {
        c.fill(local_rect(MP_PAD, y, filled, MP_TRACK_H), media.accent, R_ROUND);
    }
    let thumb_x = (MP_PAD + filled - 5).clamp(MP_PAD, MP_PAD + width - 10);
    c.fill(local_rect(thumb_x, y - 3, 10, 10), theme.thumb, R_ROUND);
}

const CLOCK_STYLE: glide_theme::TextStyle = glide_theme::TextStyle {
    size_px: glide_theme::TextStyle::TOPBAR.size_px,
    line_height_px: glide_theme::TextStyle::TOPBAR.line_height_px,
    weight: glide_theme::TextStyle::TOPBAR.weight,
    ..glide_theme::TextStyle::NUM
};

const COUNT_STYLE: glide_theme::TextStyle = glide_theme::TextStyle {
    size_px: glide_theme::TextStyle::MICRO.size_px,
    line_height_px: glide_theme::TextStyle::MICRO.line_height_px,
    weight: glide_theme::TextStyle::MICRO.weight,
    ..glide_theme::TextStyle::NUM
};

fn widest_digits(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { '8' } else { c }).collect()
}

fn numeric_label_width(painter: &crate::paint::Painter, text: &str, style: &glide_theme::TextStyle) -> i32 {
    let actual = painter.text_width_styled(text, style);
    let widest = painter.text_width_styled(&widest_digits(text), style);
    actual.max(widest).ceil() as i32
}

fn clock_time_width(painter: &crate::paint::Painter, clock: &str) -> i32 {
    numeric_label_width(painter, clock, &CLOCK_STYLE)
}

fn clock_date_width(painter: &crate::paint::Painter, date: &str) -> i32 {
    text_w(painter, date, &TextStyle::TOPBAR)
}

fn clock_badge_width(painter: &crate::paint::Painter, count: usize) -> i32 {
    let text = text_w(painter, &count.to_string(), &COUNT_STYLE);
    (text + CLOCK_BADGE_PAD_X * 2).max(CLOCK_BADGE_MIN_WIDTH)
}

fn clock_width(date_w: i32, time_w: i32, painter: &crate::paint::Painter, unread: usize) -> i32 {
    let gap = if date_w == 0 { 0 } else { CLOCK_GAP };
    let mut width = date_w + gap + time_w + BUTTON_PAD_X * 2;
    if unread > 0 {
        width += CLOCK_GAP + clock_badge_width(painter, unread);
    }
    width
}

fn app_chip_width(painter: &crate::paint::Painter, name: &str, has_icon: bool) -> i32 {
    let icon = if has_icon { APP_ICON + APP_GAP } else { 0 };
    APP_PAD_LEFT + icon + text_w(painter, name, &TextStyle::HEAD) + BUTTON_PAD_X
}

#[allow(clippy::struct_excessive_bools)] // Independent [topbar] switches, not a state machine
#[derive(Debug, Clone, PartialEq)]
pub struct TopbarTheme {
    pub enabled: bool,
    pub dark: bool,
    pub background: Color,
    pub popover_bg: Color,
    pub border: Color,
    pub label: Color,
    pub muted: Color,
    pub faint: Color,
    pub button_hover: Color,
    pub button_press: Color,
    pub open_fill: Color,
    pub track: Color,
    pub track_soft: Color,
    pub segment_active: Color,
    pub thumb: Color,
    pub active_background: Color,
    pub active_label: Color,
    pub active_soft: Color,
    pub active_soft_label: Color,
    pub ok: Color,
    pub danger: Color,
    pub battery_low_pct: u8,
    pub battery_percentage: bool,
    pub clock_date: bool,
    pub clock_24h: Option<bool>,
    pub privacy_mic: Color,
    pub privacy_camera: Color,
    pub privacy_screen: Color,
}

impl TopbarTheme {
    #[must_use]
    pub fn from_tokens(t: &glide_theme::Tokens, enabled: bool) -> Self {
        let dark = t.appearance == glide_theme::Appearance::Dark;
        let ramp_step = if dark { 2 } else { 3 };
        Self {
            battery_percentage: true,
            clock_date: true,
            clock_24h: None,
            privacy_mic: glide_theme::Accent::Orange.ramp()[ramp_step],
            privacy_camera: t.ok,
            privacy_screen: glide_theme::Accent::Purple.ramp()[ramp_step],
            enabled,
            dark,
            background: t.shell_bg,
            popover_bg: t.popover_bg,
            border: t.shell_line,
            label: t.shell_fg,
            muted: t.fg_2,
            faint: t.fg_3,
            button_hover: t.state.with_alpha(glide_theme::motion::ST_HOVER),
            button_press: t.state.with_alpha(glide_theme::motion::ST_PRESS),
            open_fill: t.fill_2,
            track: t.fill_3,
            track_soft: t.fill_1,
            segment_active: t.fill_3,
            thumb: if dark {
                Color::from_hex("#cfccc7").expect("literal is valid")
            } else {
                Color([1.0, 1.0, 1.0, 1.0])
            },
            active_background: t.acc_bg,
            active_label: t.acc_fg,
            active_soft: t.acc_soft,
            active_soft_label: t.acc_soft_fg,
            ok: t.ok,
            danger: t.danger,
            battery_low_pct: BATTERY_LOW_PCT,
        }
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        glide_theme::Revision::new()
            .bool(self.enabled)
            .bool(self.dark)
            .color(self.background)
            .color(self.popover_bg)
            .color(self.border)
            .color(self.label)
            .color(self.muted)
            .color(self.faint)
            .color(self.button_hover)
            .color(self.button_press)
            .color(self.open_fill)
            .color(self.track)
            .color(self.track_soft)
            .color(self.segment_active)
            .color(self.thumb)
            .color(self.active_background)
            .color(self.active_label)
            .color(self.active_soft)
            .color(self.active_soft_label)
            .color(self.ok)
            .color(self.danger)
            .u64(u64::from(self.battery_low_pct))
            .bool(self.battery_percentage)
            .bool(self.clock_date)
            .u64(self.clock_24h.map_or(0, |h24| 1 + u64::from(h24)))
            .color(self.privacy_mic)
            .color(self.privacy_camera)
            .color(self.privacy_screen)
            .finish()
    }
}

impl Default for TopbarTheme {
    fn default() -> Self {
        let tokens = glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        Self::from_tokens(&tokens, true)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    topbar: &mut Topbar, renderer: &mut R, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader,
    output: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
    backdrop: Option<&crate::dock::Backdrop<'_>>,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: ShellRenderer,
{
    topbar.settle_motion();
    if !topbar.is_enabled() {
        return Vec::new();
    }

    topbar.invalidate_stale_buffers();
    topbar.layout_slots(painter, output);

    let strip = topbar.rect(output);
    let mut frame = Frame { renderer, painter, icons, scale, output_scale, elements: Vec::new() };

    push_state_layers(topbar, &mut frame);
    push_quick_settings(topbar, &mut frame);
    push_notification_center(topbar, &mut frame);
    push_global_menu(topbar, &mut frame);
    push_app_chip(topbar, &mut frame, strip);
    push_clock(topbar, &mut frame);
    push_media(topbar, &mut frame);
    push_media_popup(topbar, &mut frame);
    push_dnd(topbar, &mut frame);
    push_layout(topbar, &mut frame);
    push_caffeine(topbar, &mut frame);
    push_status(topbar, &mut frame);
    push_panel(topbar, &mut frame, strip, backdrop);

    frame.elements
}

struct Frame<'a, R: ShellRenderer> {
    renderer: &'a mut R,
    painter: &'a crate::paint::Painter,
    icons: &'a mut crate::icon::IconLoader,
    scale: Scale<f64>,
    output_scale: i32,
    elements: Vec<MemoryRenderBufferRenderElement<R>>,
}

type Parts<'a, 'f, R> = (
    &'f mut R,
    &'a crate::paint::Painter,
    &'f mut crate::icon::IconLoader,
    Scale<f64>,
    i32,
    &'f mut Vec<MemoryRenderBufferRenderElement<R>>,
);

impl<'a, R: ShellRenderer> Frame<'a, R> {
    fn parts(&mut self) -> Parts<'a, '_, R> {
        (&mut *self.renderer, self.painter, &mut *self.icons, self.scale, self.output_scale, &mut self.elements)
    }
}

fn push_buffer<R>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, position: Point<i32, Logical>, scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        position.to_f64().to_physical(scale),
        buffer,
        Some(1.0),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn push_panel<R: ShellRenderer>(
    topbar: &mut Topbar, frame: &mut Frame<'_, R>, strip: Rectangle<i32, Logical>,
    backdrop: Option<&crate::dock::Backdrop<'_>>,
) {
    let (renderer, _, _, scale, output_scale, elements) = frame.parts();
    let key = (
        strip.size.w,
        output_scale,
        backdrop.map_or(0, |source| source.generation),
        backdrop.map_or((0, 0, 0), |source| (source.width, source.height, source.fractional_scale.to_bits())),
    );
    if !topbar.panel.contains_key(&key)
        && let Some(buffer) = draw_panel(&topbar.theme, strip, output_scale, backdrop)
    {
        topbar.panel.insert(key, buffer);
    }
    let Some(buffer) = topbar.panel.get(&key) else {
        return;
    };
    push_buffer(renderer, buffer, strip.loc, scale, elements);
}

fn draw_panel(
    theme: &TopbarTheme, strip: Rectangle<i32, Logical>, scale: i32, backdrop: Option<&crate::dock::Backdrop<'_>>,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (strip.size.w, strip.size.h);
    let mut c = Canvas::new(w, h, scale)?;
    if backdrop.is_some_and(|source| source.blit(&mut c.buf, w, h, 0.0, strip, scale)) {
        c.fill(local_rect(0, 0, w, h), theme.background, 0.0);
    } else {
        let background = theme.background;
        crate::paint::Painter::fill_panel_background(&mut c.buf, w, h, scale, background, 0.0, [false; 4]);
    }
    c.fill(local_rect(0, h - 1, w, 1), theme.border, 0.0);
    Some(c.finish())
}

fn push_global_menu<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    push_lang_menu(topbar, frame);
    push_app_menu(topbar, frame);
    push_menu_titles(topbar, frame);
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    tray::push_tray(topbar, renderer, painter, scale, output_scale, elements);
}

fn push_menu_titles<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    let open = (topbar.open_popup == Some(OpenPopup::AppMenu) && topbar.tray_menu_id().is_none())
        .then(|| topbar.menu_open.top_index());
    let titles: Vec<(usize, Rectangle<i32, Logical>)> = topbar
        .slots
        .iter()
        .filter_map(|(slot, rect)| match slot {
            Slot::MenuTitle(index) => Some((usize::from(*index), *rect)),
            _ => None,
        })
        .collect();
    for (index, rect) in titles {
        let is_open = open == Some(index);
        let key = (index, is_open, output_scale);
        if !topbar.menu_titles.contains_key(&key)
            && let Some(item) = topbar.menu_bar.get(index)
            && let Some(buffer) = crate::global_menu::draw_title(
                painter,
                &item.label,
                rect.size.w,
                rect.size.h,
                topbar.theme.label,
                is_open.then_some(topbar.theme.open_fill),
                output_scale,
            )
        {
            topbar.menu_titles.insert(key, buffer);
        }
        if let Some(buffer) = topbar.menu_titles.get(&key) {
            push_buffer(renderer, buffer, rect.loc, scale, elements);
        }
    }
}

fn push_app_menu<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    if topbar.shown_popup() != Some(OpenPopup::AppMenu) {
        return;
    }
    let Some(anchor) = topbar.popup_rect else {
        return;
    };
    let reveal = topbar.popup_progress();
    let theme = &topbar.theme;
    let colors = crate::global_menu::MenuColors {
        background: theme.popover_bg,
        border: theme.border,
        text: theme.label,
        accel: theme.faint,
        disabled: theme.faint,
        mark: theme.active_background,
        open_row: theme.button_hover,
    };
    let panels = topbar.menu_panels.clone();
    let open = topbar.menu_open;
    for panel in panels.iter().rev() {
        let depth = panel.parent.len();
        let open_row = (open.len() > depth && open.starts_with(panel.parent)).then(|| open.truncated(depth + 1).last());
        let key = (panel.parent, open_row, output_scale);
        if !topbar.menu_rasters.contains_key(&key)
            && let Some(item) = crate::global_menu::item_at(topbar.menu_items(), panel.parent)
            && let Some(buffer) = crate::global_menu::draw_panel(&crate::global_menu::PanelDraw {
                painter,
                colors: &colors,
                items: &item.children,
                marks: panel.marks,
                size: panel.rect.size,
                open_row,
                scale: output_scale,
            })
        {
            topbar.menu_rasters.insert(key, buffer);
        }
        if let Some(buffer) = topbar.menu_rasters.get(&key) {
            push_popup_buffer(renderer, buffer, anchor, panel.rect, reveal, scale, elements);
        }
        push_popup_shadow(topbar, renderer, panel.rect, reveal, scale, output_scale, elements);
    }
}

fn push_app_chip<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, strip: Rectangle<i32, Logical>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    if topbar.app_name.is_empty() {
        return;
    }

    let theme = &topbar.theme;
    let name = &topbar.app_name;
    let app_id = &topbar.app_id;
    let mut chip_w = topbar.app_chip_w;
    let buffer = topbar
        .app_cache
        .resolve((app_id.clone(), name.clone()), output_scale, || {
            let icon = icons.load(app_id, APP_ICON * scale_or_one(output_scale));
            chip_w = app_chip_width(painter, name, icon.is_some());
            draw_app_chip(painter, theme, name, icon, chip_w, output_scale)
        })
        .cloned();
    if chip_w != topbar.app_chip_w {
        topbar.app_chip_w = chip_w;
        topbar.touch();
    }
    let Some(buffer) = buffer else {
        return;
    };
    let buffer = &buffer;

    let position = Point::from((strip.loc.x + PAD_X, strip.loc.y + (HEIGHT - BUTTON_HEIGHT) / 2));
    push_buffer(renderer, buffer, position, scale, elements);
}

fn draw_app_chip(
    painter: &crate::paint::Painter, theme: &TopbarTheme, name: &str, icon: Option<&[u8]>, width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?;
    c.panel(theme.track_soft, BUTTON_RADIUS);

    let mut text_left = APP_PAD_LEFT;
    if let Some(icon) = icon {
        let top = (BUTTON_HEIGHT - APP_ICON) / 2;
        c.blit(icon, local_rect(APP_PAD_LEFT, top, APP_ICON, APP_ICON));
        text_left += APP_ICON + APP_GAP;
    }
    let span = (text_left, width - BUTTON_PAD_X);
    c.label(painter, span, TextAlign::Left, name, theme.label, &TextStyle::HEAD);
    Some(c.finish())
}

fn push_clock<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    if topbar.clock.is_empty() {
        return;
    }
    let Some(rect) = topbar.slot_rect(Slot::Clock) else {
        return;
    };
    let width = rect.size.w;

    let theme = &topbar.theme;
    let date = &topbar.date;
    let clock = &topbar.clock;
    let badge = BadgePop { count: topbar.badge_shown(), pop: topbar.badge_anim.value() };
    let open = topbar.open_popup == Some(OpenPopup::NotificationCenter);
    let buffer =
        topbar.clock_cache.resolve((date.clone(), clock.clone(), badge.count, open, badge.key()), output_scale, || {
            draw_clock(painter, theme, date, clock, badge, open, width, output_scale)
        });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BadgePop {
    count: usize,
    pop: f64,
}

impl BadgePop {
    fn key(self) -> u8 {
        quantize(self.pop, BADGE_QUANTUM)
    }

    fn scale(self) -> f64 {
        lerp(BADGE_SCALE_FROM, 1.0, self.pop.max(0.0))
    }

    fn visible(self) -> bool {
        self.count > 0 && self.pop > 0.0
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_clock(
    painter: &crate::paint::Painter, theme: &TopbarTheme, date: &str, clock: &str, badge: BadgePop, open: bool,
    width: i32, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?;
    if open {
        c.panel(theme.open_fill, BUTTON_RADIUS);
    }

    let date_w = clock_date_width(painter, date);
    let time_w = clock_time_width(painter, clock);
    let lead = ((width - clock_width(date_w, time_w, painter, badge.count)) / 2).max(0);

    let mut time_left = BUTTON_PAD_X + lead;
    if !date.is_empty() {
        let span = (time_left, time_left + date_w);
        c.label(painter, span, TextAlign::Left, date, theme.muted, &TextStyle::TOPBAR);
        time_left += date_w + CLOCK_GAP;
    }
    let span = (time_left, time_left + time_w);
    c.label(painter, span, TextAlign::Left, clock, theme.label, &CLOCK_STYLE);

    if badge.visible() {
        let badge_w = clock_badge_width(painter, badge.count);
        let badge_x = time_left + time_w + CLOCK_GAP;
        let badge_y = (BUTTON_HEIGHT - CLOCK_BADGE_HEIGHT) / 2;
        let pill = scaled_about_centre(local_rect(badge_x, badge_y, badge_w, CLOCK_BADGE_HEIGHT), badge.scale());
        let alpha = badge.pop.clamp(0.0, 1.0) as f32;
        c.fill(pill, theme.active_background.with_alpha(alpha), R_ROUND);
        c.label(
            painter,
            (badge_x, badge_x + badge_w),
            TextAlign::Center,
            &badge.count.to_string(),
            theme.active_label.with_alpha(alpha),
            &COUNT_STYLE,
        );
    }

    Some(c.finish())
}

fn scaled_about_centre(rect: LocalRect, factor: f64) -> LocalRect {
    let w = (f64::from(rect.size.w) * factor).round() as i32;
    let h = (f64::from(rect.size.h) * factor).round() as i32;
    local_rect(rect.loc.x + (rect.size.w - w) / 2, rect.loc.y + (rect.size.h - h) / 2, w.max(0), h.max(0))
}

fn push_layout<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    let Some(rect) = topbar.layout_control_rect() else {
        return;
    };
    if topbar.layouts.is_empty() {
        return;
    }

    let theme = &topbar.theme;
    let layouts = &topbar.layouts;
    let active = topbar.layout_index;
    let hovered = topbar.hovered_in(Slot::in_layout);
    let open = topbar.open_popup == Some(OpenPopup::LanguageMenu);
    let segmented = topbar.segmented();
    let width = rect.size.w;
    let pill = topbar.segment_anim.value();

    let buffer = topbar.layout_cache.resolve(
        (topbar.layouts_rev, active, hovered, open, quantize(pill, MOTION_QUANTUM)),
        output_scale,
        || {
            let label = layouts.get(active).map_or("", String::as_str);
            if segmented {
                draw_segmented(painter, theme, layouts, SegmentState { active, hovered, pill }, width, output_scale)
            } else if layouts.len() >= 3 {
                draw_lang_dropdown_button(
                    painter,
                    theme,
                    label,
                    width,
                    hovered == Some(Slot::LayoutDropdown),
                    open,
                    output_scale,
                )
            } else {
                draw_button(painter, theme, label, width, hovered == Some(Slot::LayoutCycle), output_scale)
            }
        },
    );
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_lang_dropdown_button(
    painter: &crate::paint::Painter, theme: &TopbarTheme, text: &str, width: i32, hovered: bool, open: bool, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?;
    if open {
        c.panel(theme.open_fill, BUTTON_RADIUS);
    }
    let style = &TextStyle::TOPBAR;
    let span = (9, 9 + text_w(painter, text, style));
    c.label(painter, span, TextAlign::Left, text, hover_colour(theme, open || hovered), style);

    let side = 10;
    let chevron = local_rect(width - 6 - side, (BUTTON_HEIGHT - side) / 2, side, side);
    c.glyph(chevron, |b, w, h, s| {
        crate::paint::draw_chevron_glyph_over(b, w, h, 9.0, 1.5, open, s, theme.faint);
    });
    Some(c.finish())
}

fn push_lang_menu<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    if topbar.shown_popup() != Some(OpenPopup::LanguageMenu) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let theme = &topbar.theme;
    let layouts = &topbar.layouts;
    let active = topbar.layout_index;
    let hovered = topbar.hovered_in(Slot::in_lang_menu);
    let width = rect.size.w;
    let height = rect.size.h;
    let reveal = topbar.popup_progress();

    let buffer = topbar.lang_menu_cache.resolve((topbar.layouts_rev, active, hovered), output_scale, || {
        draw_lang_menu(painter, theme, layouts, (active, hovered), (width, height), output_scale)
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn draw_lang_menu(
    painter: &crate::paint::Painter, theme: &TopbarTheme, layouts: &[String], (active, hovered): (usize, Option<Slot>),
    size: (i32, i32), scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::popover(size, scale, R3, theme.popover_bg, theme.border)?;
    let width = size.0;
    let style = &TextStyle::TOPBAR;
    let text_left = LANG_MENU_PAD + 6 + 14 + 6;
    let mut y = LANG_MENU_PAD;
    for (index, code) in layouts.iter().enumerate() {
        let is_active = index == active;
        if is_active {
            let tick_side = 14;
            let top = y + (LANG_MENU_ITEM_HEIGHT - tick_side) / 2;
            c.glyph(local_rect(LANG_MENU_PAD + 6, top, tick_side, tick_side), |b, w, h, s| {
                crate::paint::draw_tick_glyph_over(b, w, h, 13.0, 1.8, s, theme.active_background);
            });
        }
        let (top, bottom) = (y, y + LANG_MENU_ITEM_HEIGHT);
        let colour = hover_colour(theme, is_active || hovered == Some(Slot::LangMenuItem(index)));
        c.text(painter, (text_left, text_left + 40, top, bottom), TextAlign::Left, code, colour, style);
        let native = native_layout_name(code);
        if !native.is_empty() {
            let area = (width - 120, width - LANG_MENU_PAD - 8, top, bottom);
            c.text(painter, area, TextAlign::Right, native, theme.faint, style);
        }
        y += LANG_MENU_ITEM_HEIGHT;
    }

    let divider = local_rect(LANG_MENU_PAD + 4, y + 4, width - (LANG_MENU_PAD + 4) * 2, 1);
    c.fill(divider, theme.border, 0.0);
    y += LANG_MENU_DIVIDER;

    c.text(
        painter,
        (text_left, width - LANG_MENU_PAD - 8, y, y + LANG_MENU_ITEM_HEIGHT),
        TextAlign::Left,
        crate::i18n::t(crate::i18n::Key::InputSettings),
        hover_colour(theme, hovered == Some(Slot::LangMenuSettings)),
        style,
    );
    Some(c.finish())
}

fn push_notification_center<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    if topbar.shown_popup() != Some(OpenPopup::NotificationCenter) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let reveal = topbar.popup_progress();
    calendar::push(topbar, renderer, painter, icons, rect, reveal, scale, output_scale, elements);
    let theme = &topbar.theme;
    let dnd = topbar.dnd;
    let switch = topbar.dnd_anim.switch();
    let hovered = topbar
        .hovered_in(Slot::in_notification_center)
        .filter(|slot| !matches!(slot, Slot::CalMonth | Slot::CalPrev | Slot::CalNext));
    let width = rect.size.w;
    let height = rect.size.h;
    let nc_rev = topbar.nc_rev;
    let items = &topbar.nc_items;

    let buffer = topbar.nc_cache.resolve((nc_rev, dnd, switch.key(), hovered), output_scale, || {
        let ctx = NcRenderContext { painter, theme, dnd, switch, hovered };
        let mut canvas =
            Canvas::popover((width, height), output_scale, R3, theme.popover_bg, theme.border)?.with_icons(Some(icons));
        draw_notification_center(&mut canvas, &ctx, items);
        Some(canvas.finish())
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

struct NcRenderContext<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    dnd: bool,
    switch: SwitchMotion,
    hovered: Option<Slot>,
}

const fn hover_colour(theme: &TopbarTheme, hovered: bool) -> Color {
    if hovered { theme.label } else { theme.muted }
}

fn draw_nc_dnd_switch(c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>, head_y: i32) -> i32 {
    let sw_x = c.w - NC_PAD - SWITCH_WIDTH;
    let sw_y = head_y + (NC_HEADER_HEIGHT - SWITCH_HEIGHT) / 2;
    let track = local_rect(sw_x, sw_y, SWITCH_WIDTH, SWITCH_HEIGHT);
    draw_switch(c, ctx.theme, track, (SWITCH_INSET, SWITCH_THUMB), ctx.switch);

    let side = 14;
    let moon_x = sw_x - 8 - side;
    let moon_y = head_y + (NC_HEADER_HEIGHT - side) / 2;
    let colour = if ctx.dnd { ctx.theme.active_label } else { ctx.theme.muted };
    if !c.icon("notifications-disabled", side, (moon_x, moon_y), colour) {
        c.glyph(local_rect(moon_x, moon_y, side, side), |b, w, h, s| {
            crate::paint::draw_moon_glyph_over(b, w, h, 14.0, s, colour);
        });
    }
    moon_x
}

fn draw_nc_header(c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>) {
    use crate::i18n::{Key, t};
    let (top, bottom) = (NC_TOP, NC_TOP + NC_HEADER_HEIGHT);
    c.text(
        ctx.painter,
        (NC_PAD + 2, NC_PAD + 120, top, bottom),
        TextAlign::Left,
        t(Key::Notifications),
        ctx.theme.muted,
        &TextStyle::MICRO,
    );

    let moon_x = draw_nc_dnd_switch(c, ctx, top);

    let clear_w = 96;
    let clear_x = moon_x - 10 - clear_w;
    c.text(
        ctx.painter,
        (clear_x, clear_x + clear_w, top, bottom),
        TextAlign::Center,
        t(Key::ClearAll),
        hover_colour(ctx.theme, ctx.hovered == Some(Slot::NcClearAll)),
        &TextStyle::TOPBAR,
    );
}

fn draw_nc_item_icon(c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>, item: &crate::notifications::NcItem, item_y: i32) {
    let side = 32;
    let (x, y) = (NC_PAD + 4, item_y + (NC_ITEM_HEIGHT - side) / 2);
    if let Some(icon) = item.icon.as_deref() {
        let source = crate::notifications::ICON_SOURCE_PX;
        crate::paint::blit_icon_over(&mut c.buf, c.w, c.h, icon, source, side, x, y, c.scale);
    } else {
        let rect = local_rect(x, y, side, side);
        c.fill(rect, ctx.theme.track_soft, R2);
        let label = initial(&item.app_name);
        c.text(ctx.painter, band(rect), TextAlign::Center, &label, ctx.theme.label, &TextStyle::HEAD);
    }
}

fn nc_count_badge_width(painter: &crate::paint::Painter, item: &crate::notifications::NcItem) -> i32 {
    let label = format!("+{}", item.count.saturating_sub(1));
    text_w(painter, &label, &TextStyle::MICRO) + SP2
}

fn nc_count_badge_left(painter: &crate::paint::Painter, width: i32, item: &crate::notifications::NcItem) -> i32 {
    let time_left = width - NC_PAD - 60;
    if item.count < 2 {
        return time_left;
    }
    time_left - nc_count_badge_width(painter, item) - SP1
}

fn draw_nc_item_count(c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>, item: &crate::notifications::NcItem, item_y: i32) {
    if item.count < 2 {
        return;
    }
    let label = format!("+{}", item.count - 1);
    let left = nc_count_badge_left(ctx.painter, c.w, item);
    let rect = local_rect(left, item_y + 8, nc_count_badge_width(ctx.painter, item), 15);
    c.fill(rect, ctx.theme.track_soft, R_ROUND);
    c.text(ctx.painter, band(rect), TextAlign::Center, &label, ctx.theme.muted, &TextStyle::MICRO);
}

fn draw_nc_item_dismiss(c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>, id: u32, item_y: i32) {
    let side = 22;
    let colour = hover_colour(ctx.theme, ctx.hovered == Some(Slot::NcItemDismiss(id)));
    let rect = local_rect(c.w - NC_PAD - side, item_y + 24, side, side);
    c.glyph(rect, |b, w, h, s| {
        crate::paint::draw_close_glyph_over(b, w, h, 9.0, 1.8, s, colour);
    });
}

fn draw_nc_item(
    c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>, item: &crate::notifications::NcItem, item_y: i32, is_first: bool,
) {
    let (theme, width) = (ctx.theme, c.w);
    if !is_first {
        c.fill(local_rect(NC_PAD, item_y, width - NC_PAD * 2, 1), theme.border, 0.0);
    }

    draw_nc_item_icon(c, ctx, item, item_y);

    let text_left = NC_PAD + 4 + 32 + 10;
    let text_right = width - NC_PAD - 48;
    let title_right = text_right.min(nc_count_badge_left(ctx.painter, width, item) - SP2);
    let title = (text_left, title_right, item_y + 8, item_y + 26);
    c.text(ctx.painter, title, TextAlign::Left, &item.title, theme.label, &TextStyle::HEAD);
    let body = (text_left, text_right, item_y + 28, item_y + 46);
    c.text(ctx.painter, body, TextAlign::Left, &item.body, theme.muted, &TextStyle::CALL);
    let time = (width - NC_PAD - 60, width - NC_PAD - 4, item_y + 8, item_y + 24);
    c.text(ctx.painter, time, TextAlign::Right, &item.time, theme.faint, &TextStyle::MICRO);

    draw_nc_item_count(c, ctx, item, item_y);
    draw_nc_item_dismiss(c, ctx, item.id, item_y);
}

fn draw_notification_center(c: &mut Canvas<'_>, ctx: &NcRenderContext<'_>, items: &[crate::notifications::NcItem]) {
    draw_nc_header(c, ctx);

    let mut item_y = NC_TOP + NC_HEADER_HEIGHT + NC_GAP;
    if items.is_empty() {
        c.text(
            ctx.painter,
            (NC_PAD, c.w - NC_PAD, item_y, item_y + NC_EMPTY_HEIGHT),
            TextAlign::Center,
            crate::i18n::t(crate::i18n::Key::NoNewNotifications),
            ctx.theme.muted,
            &TextStyle::TOPBAR,
        );
        return;
    }
    for (i, item) in items.iter().take(NC_MAX_ITEMS).enumerate() {
        draw_nc_item(c, ctx, item, item_y, i == 0);
        item_y += NC_ITEM_HEIGHT;
    }
}

fn draw_button(
    painter: &crate::paint::Painter, theme: &TopbarTheme, text: &str, width: i32, hovered: bool, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?;
    let span = (BUTTON_PAD_X, width - BUTTON_PAD_X);
    let colour = hover_colour(theme, hovered);
    c.label(painter, span, TextAlign::Center, text, colour, &TextStyle::TOPBAR);
    Some(c.finish())
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SegmentState {
    active: usize,
    hovered: Option<Slot>,
    pill: f64,
}

fn segment_spans(painter: &crate::paint::Painter, layouts: &[String]) -> Vec<(i32, i32)> {
    let mut x = SEG_PAD_X + SEG_INSET;
    layouts
        .iter()
        .map(|name| {
            let seg = segment_width(painter, name);
            let span = (x, seg);
            x += seg + SEG_GAP;
            span
        })
        .collect()
}

fn draw_segmented(
    painter: &crate::paint::Painter, theme: &TopbarTheme, layouts: &[String], state: SegmentState, width: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?;
    let track_top = (BUTTON_HEIGHT - SEG_HEIGHT) / 2;
    let track = local_rect(SEG_PAD_X, track_top, width - SEG_PAD_X * 2, SEG_HEIGHT);
    c.fill(track, theme.track_soft, BUTTON_RADIUS);

    let spans = segment_spans(painter, layouts);
    if let (Some(first), Some(second)) = (spans.first(), spans.get(1)) {
        let (pill_x, pill_w) = segment_pill(*first, *second, state.pill);
        let pill = local_rect(pill_x, track_top + SEG_INSET, pill_w, SEG_HEIGHT - SEG_INSET * 2);
        c.fill(pill, theme.segment_active, SEG_RADIUS);
    }

    for (index, (name, (x, seg))) in layouts.iter().zip(spans).enumerate() {
        let colour = if index == state.active {
            theme.label
        } else if state.hovered == Some(Slot::LayoutPick(index)) {
            theme.muted
        } else {
            theme.faint
        };
        c.label(painter, (x, x + seg), TextAlign::Center, name, colour, &TextStyle::TOPBAR);
    }
    Some(c.finish())
}

fn push_caffeine<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, _, icons, scale, output_scale, elements) = frame.parts();
    let Some(rect) = topbar.slot_rect(Slot::Caffeine) else {
        return;
    };
    let hovered = topbar.hovered == Some(Slot::Caffeine);
    let on = topbar.caffeinated;
    let switch = topbar.caffeine_anim.switch();
    let theme = &topbar.theme;
    let width = rect.size.w;

    let buffer = topbar.coffee_cache.resolve((on, hovered, switch.key()), output_scale, || {
        draw_caffeine(theme, width, hovered, on, switch, output_scale, Some(icons))
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_caffeine(
    theme: &TopbarTheme, width: i32, hovered: bool, on: bool, switch: SwitchMotion, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?.with_icons(icons);
    let colour = hover_colour(theme, on || hovered);
    let side = 14;
    if !c.icon(caffeine_icon(on), side, (SP1, (BUTTON_HEIGHT - side) / 2), colour) {
        let (cx, cy) = (SP1 as f32 + COFFEE_GLYPH / 2.0, BUTTON_HEIGHT as f32 / 2.0);
        let (glyph, stroke) = (COFFEE_GLYPH, COFFEE_STROKE);
        crate::paint::draw_coffee_glyph_over(
            &mut c.buf,
            width,
            BUTTON_HEIGHT,
            cx,
            cy,
            glyph,
            stroke,
            scale,
            colour,
            on,
        );
    }
    let track_y = (BUTTON_HEIGHT - SWITCH_HEIGHT) / 2;
    let track = local_rect(width - SP1 - SWITCH_WIDTH, track_y, SWITCH_WIDTH, SWITCH_HEIGHT);
    draw_switch(&mut c, theme, track, (SWITCH_INSET, SWITCH_THUMB), switch);
    Some(c.finish())
}

const fn caffeine_icon(on: bool) -> &'static str {
    if on { "caffeine-cup-full" } else { "caffeine-cup-empty" }
}

fn push_dnd<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, _, icons, scale, output_scale, elements) = frame.parts();
    let Some(rect) = topbar.slot_rect(Slot::DoNotDisturb) else {
        return;
    };
    let hovered = topbar.hovered == Some(Slot::DoNotDisturb);
    let on = topbar.dnd_anim.value_with(E_OUT);
    let theme = &topbar.theme;
    let width = rect.size.w;

    let buffer = topbar.dnd_cache.resolve((hovered, quantize(on, MOTION_QUANTUM)), output_scale, || {
        draw_dnd(theme, width, hovered, on, output_scale, Some(icons))
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_dnd(
    theme: &TopbarTheme, width: i32, hovered: bool, on: f64, scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?.with_icons(icons);
    if on > 0.0 {
        let fill = theme.active_background;
        c.panel(fill.with_alpha(0.0).lerp(fill, on as f32), BUTTON_RADIUS);
    }
    let colour = hover_colour(theme, hovered).lerp(theme.active_label, on as f32);
    let side = 14;
    let at = ((width - side) / 2, (BUTTON_HEIGHT - side) / 2);
    if !c.icon("notifications-disabled", side, at, colour) {
        crate::paint::draw_moon_glyph_over(&mut c.buf, width, BUTTON_HEIGHT, DND_GLYPH, scale, colour);
    }
    Some(c.finish())
}

fn push_status<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    status::push_popups(topbar, renderer, painter, scale, output_scale, elements);
    status::push_cluster(topbar, renderer, painter, icons, scale, output_scale, elements);
}

const BATTERY_LOW_PCT: u8 = 20;

fn push_quick_settings<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    if topbar.shown_popup() != Some(OpenPopup::QuickSettings) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let reveal = topbar.popup_progress();
    {
        let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
        panels::push_detail(topbar, renderer, painter, icons, rect, reveal, scale, output_scale, elements);
    }
    let slide = topbar.qs_panel.slide_progress();
    if slide < 1.0 {
        push_qs_main(topbar, frame, rect, (reveal, slide));
    }
    let (renderer, _, _, scale, output_scale, elements) = frame.parts();
    let theme = &topbar.theme;
    if let Some(background) = topbar.qs_bg_cache.resolve((rect.size.w, rect.size.h), output_scale, || {
        let size = (rect.size.w, rect.size.h);
        Some(Canvas::popover(size, output_scale, POPUP_RADIUS, theme.popover_bg, theme.border)?.finish())
    }) {
        push_popup_buffer(renderer, background, rect, rect, reveal, scale, elements);
    }
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn push_qs_main<R: ShellRenderer>(
    topbar: &mut Topbar, frame: &mut Frame<'_, R>, rect: Rectangle<i32, Logical>, (reveal, slide): (f64, f64),
) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let main_view = topbar.qs_panel.view.is_none();
    let hovered = topbar.hovered_in(Slot::in_quick_settings).filter(|_| main_view);
    let caption = hovered.or_else(|| topbar.focused_in(Slot::in_quick_settings)).filter(|_| main_view).and_then(
        |slot| match slot {
            Slot::QsPowerButton(action) => Some(action),
            _ => None,
        },
    );
    let flags = QsFlags { caffeinated: topbar.caffeinated, caption };
    let motion = QsMotion {
        caffeine: topbar.caffeine_anim.switch(),
        volume: percent(topbar.volume_anim.value()),
        brightness: percent(topbar.brightness_anim.value()),
    };
    let theme = &topbar.theme;
    let status = &topbar.system_status;
    let Some(buffer) = topbar.qs_cache.resolve((topbar.status_rev, flags, motion.key(), hovered), output_scale, || {
        let ctx = QsRenderContext { painter, theme, hovered, caption };
        let mut canvas = Canvas::new(rect.size.w, rect.size.h, output_scale)?.with_icons(Some(icons));
        draw_quick_settings(&mut canvas, &ctx, status, flags.caffeinated, motion);
        Some(canvas.finish())
    }) else {
        return;
    };
    let shift = (slide * f64::from(QS_PAD)).round() as i32;
    let alpha = (1.0 - slide) as f32;
    push_placed(
        renderer,
        buffer,
        Placed {
            popup: rect,
            element: Rectangle::new(Point::from((rect.loc.x - shift, rect.loc.y)), rect.size),
            progress: reveal,
            alpha,
        },
        scale,
        elements,
    );
}

struct QsRenderContext<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    hovered: Option<Slot>,
    caption: Option<PowerAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QsFlags {
    caffeinated: bool,
    caption: Option<PowerAction>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct QsMotion {
    caffeine: SwitchMotion,
    volume: u8,
    brightness: u8,
}

impl QsMotion {
    fn key(self) -> ((u8, u8), u8, u8) {
        (self.caffeine.key(), self.volume, self.brightness)
    }
}

fn percent(value: f64) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
}

type GlyphPainter = fn(&mut [u8], i32, i32, f32, f32, i32, Color);

struct QsTileSpec<'a> {
    rect: LocalRect,
    on: bool,
    title: &'a str,
    subtitle: &'a str,
    icon_name: &'a str,
    glyph: GlyphPainter,
    more: bool,
}

fn draw_qs_icon_badge(
    c: &mut Canvas<'_>, theme: &TopbarTheme, (x, y): (i32, i32), on: bool, (icon_name, glyph): (&str, GlyphPainter),
) {
    let (bg, fg) = if on { (theme.active_background, theme.active_label) } else { (theme.track, theme.muted) };
    let badge = local_rect(x, y, QS_ICON_BADGE, QS_ICON_BADGE);
    c.fill(badge, bg, R_ROUND);
    let side = 16;
    let at = (x + (QS_ICON_BADGE - side) / 2, y + (QS_ICON_BADGE - side) / 2);
    if !c.icon(icon_name, side, at, fg) {
        c.glyph(badge, |b, w, h, s| glyph(b, w, h, 15.0, 1.5, s, fg));
    }
}

fn draw_two_lines(
    c: &mut Canvas<'_>, painter: &crate::paint::Painter, (left, right): (i32, i32),
    (title_y, subtitle_y): ((i32, i32), (i32, i32)), (title, title_colour): (&str, Color),
    (subtitle, subtitle_colour): (&str, Color),
) {
    let lines =
        [(title_y, title, title_colour, &TextStyle::HEAD), (subtitle_y, subtitle, subtitle_colour, &TextStyle::CAP)];
    for ((top, bottom), text, colour, style) in lines {
        c.text(painter, (left, right, top, bottom), TextAlign::Left, text, colour, style);
    }
}

fn draw_qs_tile(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, spec: &QsTileSpec<'_>) {
    let rect = spec.rect;
    let theme = ctx.theme;
    c.fill(rect, if spec.on { theme.active_soft } else { theme.track_soft }, R2);

    let ico_x = rect.loc.x + 10;
    let ico_y = rect.loc.y + (rect.size.h - QS_ICON_BADGE) / 2;
    draw_qs_icon_badge(c, theme, (ico_x, ico_y), spec.on, (spec.icon_name, spec.glyph));

    let txt_right = if spec.more {
        let (body, chevron) = panels::split_tile(rect);
        panels::draw_tile_more(c, ctx, chevron, spec.on);
        body.loc.x + body.size.w
    } else {
        rect.loc.x + rect.size.w - 6
    };
    let (title_colour, subtitle_colour) =
        if spec.on { (theme.active_soft_label, theme.active_soft_label) } else { (theme.label, theme.muted) };
    let y = rect.loc.y;
    draw_two_lines(
        c,
        ctx.painter,
        (ico_x + QS_ICON_BADGE + SP2, txt_right),
        ((y + 10, y + 26), (y + 28, y + 44)),
        (spec.title, title_colour),
        (spec.subtitle, subtitle_colour),
    );
}

fn draw_switch(
    c: &mut Canvas<'_>, theme: &TopbarTheme, track: LocalRect, (inset, thumb): (i32, i32), switch: SwitchMotion,
) {
    let colour = switch.track_colour(theme.track, theme.active_background);
    c.fill(track, colour, R_ROUND);
    let thumb_x = switch.thumb_x(track.loc.x, track.size.w, inset, thumb);
    let knob = local_rect(thumb_x, track.loc.y + inset, thumb, thumb);
    c.fill(knob, theme.thumb, R_ROUND);
}

fn draw_qs_switch(c: &mut Canvas<'_>, theme: &TopbarTheme, (sw_x, sw_y): (i32, i32), switch: SwitchMotion) {
    let track = local_rect(sw_x, sw_y, QS_SWITCH_W, QS_SWITCH_H);
    draw_switch(c, theme, track, (QS_SWITCH_INSET, QS_SWITCH_THUMB), switch);
}

fn draw_qs_caffeine_row(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, on: bool, switch: SwitchMotion,
) {
    use crate::i18n::{Key, t};
    c.fill(rect, ctx.theme.track_soft, R2);

    let ico_x = rect.loc.x + 10;
    let ico_y = rect.loc.y + (rect.size.h - QS_ICON_BADGE) / 2;
    let icon_name = if on { "caffeine-cup-full" } else { "caffeine-cup-empty" };
    let glyph: GlyphPainter = |b, w, h, side, stroke, s, c| {
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        crate::paint::draw_coffee_glyph_over(b, w, h, cx, cy, side, stroke, s, c, false);
    };
    draw_qs_icon_badge(c, ctx.theme, (ico_x, ico_y), on, (icon_name, glyph));

    let subtitle = t(if on { Key::CaffeineOn } else { Key::CaffeineOff });
    let y = rect.loc.y;
    draw_two_lines(
        c,
        ctx.painter,
        (ico_x + QS_ICON_BADGE + SP2, rect.loc.x + rect.size.w - 10 - QS_SWITCH_W - SP2),
        ((y + 6, y + 22), (y + 22, y + 38)),
        (t(Key::Caffeine), ctx.theme.label),
        (subtitle, ctx.theme.muted),
    );

    let sw_x = rect.loc.x + rect.size.w - 10 - QS_SWITCH_W;
    let sw_y = rect.loc.y + (rect.size.h - QS_SWITCH_H) / 2;
    draw_qs_switch(c, ctx.theme, (sw_x, sw_y), switch);
}

struct QsSliderSpec<'a> {
    icon: LocalRect,
    track: LocalRect,
    pct: u8,
    muted: bool,
    icon_name: &'a str,
}

fn draw_qs_slider(
    c: &mut Canvas<'_>, theme: &TopbarTheme, spec: &QsSliderSpec<'_>,
    draw_icon: impl FnOnce(&mut [u8], i32, i32, i32, Color),
) {
    let icon = spec.icon;
    let side = 16;
    let at = (icon.loc.x + (icon.size.w - side) / 2, icon.loc.y + (icon.size.h - side) / 2);
    if !c.icon(spec.icon_name, side, at, theme.muted) {
        c.glyph(icon, |b, w, h, s| draw_icon(b, w, h, s, theme.muted));
    }

    let track = spec.track;
    let tr_y = track.loc.y + (track.size.h - QS_SLIDER_TRACK_H) / 2;
    let bar = |width| local_rect(track.loc.x, tr_y, width, QS_SLIDER_TRACK_H);
    c.fill(bar(track.size.w), theme.track, R_ROUND);

    let fill_w = (((track.size.w as f32) * f32::from(spec.pct.min(100))) / 100.0).round() as i32;
    if fill_w > 0 {
        let colour = if spec.muted { theme.track_soft } else { theme.active_background };
        c.fill(bar(fill_w), colour, R_ROUND);
    }

    let thumb_x =
        (track.loc.x + fill_w - QS_SLIDER_THUMB / 2).clamp(track.loc.x, track.loc.x + track.size.w - QS_SLIDER_THUMB);
    let thumb_y = track.loc.y + (track.size.h - QS_SLIDER_THUMB) / 2;
    let thumb = local_rect(thumb_x, thumb_y, QS_SLIDER_THUMB, QS_SLIDER_THUMB);
    c.fill(thumb, theme.thumb, R_ROUND);
}

fn initial(name: &str) -> String {
    name.chars().next().unwrap_or('?').to_uppercase().to_string()
}

fn draw_qs_avatar(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, x: i32, y: i32, user_name: &str) {
    let theme = ctx.theme;
    let top = theme.active_background.state_layer(theme.active_label, AVATAR_GRADIENT_LIFT);
    c.gradient(local_rect(x, y, QS_AVATAR, QS_AVATAR), R_ROUND, top, theme.active_background);
    c.text(
        ctx.painter,
        (x, x + QS_AVATAR, y, y + QS_AVATAR),
        TextAlign::Center,
        &initial(user_name),
        theme.active_label,
        &TextStyle::OV_LABEL,
    );
}

const AVATAR_GRADIENT_LIFT: f32 = glide_theme::motion::ST_DRAG;

fn draw_qs_footer(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &QsGeometry, status: &crate::system_info::SystemStatus,
) {
    let user_name = status.user_name.as_str();
    let footer = geometry.footer;
    let (x, y) = (footer.loc.x, footer.loc.y);
    let avatar_y = y + (footer.size.h - QS_AVATAR) / 2;
    draw_qs_avatar(c, ctx, x, avatar_y, user_name);

    let acts_left = match status.battery_pct {
        Some(pct) => {
            draw_qs_battery_pill(c, ctx, geometry.battery, (pct, status.battery_charging));
            geometry.battery.loc.x
        }
        None => geometry.settings.loc.x,
    };
    let (footer_text, footer_color) = match ctx.caption {
        Some(action) => (crate::i18n::t(panels::power_label(action)), ctx.theme.muted),
        None => (user_name, ctx.theme.label),
    };
    c.text(
        ctx.painter,
        (x + QS_AVATAR + SP2, acts_left - SP2, y, y + footer.size.h),
        TextAlign::Left,
        footer_text,
        footer_color,
        &TextStyle::HEAD,
    );

    let mut buttons = vec![(
        Slot::QsSettings,
        geometry.settings,
        crate::paint::draw_gear_glyph_over as GlyphPainter,
        "emblem-system",
    )];
    buttons.extend(geometry.power.iter().filter_map(|(slot, rect)| match slot {
        Slot::QsPowerButton(action) => Some((*slot, *rect, panels::power_glyph(*action), panels::power_icon(*action))),
        _ => None,
    }));

    for (slot, rect, draw_glyph, icon_name) in buttons {
        draw_qs_action_button(c, ctx, slot, rect, (draw_glyph, icon_name));
    }
}

fn draw_battery(
    c: &mut Canvas<'_>, theme: &TopbarTheme, rect: LocalRect, (pct, charging): (u8, bool),
    (side, icon_fg, fg): (i32, Color, Color),
) {
    let at = (rect.loc.x + (rect.size.w - side) / 2, rect.loc.y + (rect.size.h - side) / 2);
    if c.icon(&battery_icon_name(pct, charging), side, at, icon_fg) {
        return;
    }
    c.glyph(rect, |buf, canvas_width, canvas_height, scale| {
        crate::paint::draw_battery_glyph_over(
            buf,
            &crate::paint::BatteryRenderArgs {
                canvas_width,
                canvas_height,
                pct,
                charging,
                scale,
                fg,
                ok_color: theme.ok,
                danger_color: theme.danger,
            },
        );
    });
}

fn draw_qs_battery_pill(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, (pct, charging): (u8, bool)) {
    let theme = ctx.theme;
    c.fill(rect, theme.track_soft, R_ROUND);
    let low = !charging && pct <= theme.battery_low_pct;
    let fg = if low {
        theme.danger
    } else if ctx.hovered == Some(Slot::QsBattery) {
        theme.label
    } else {
        theme.muted
    };
    let glyph_x = rect.loc.x + SP1;
    let glyph = local_rect(glyph_x, rect.loc.y, STATUS_BATT_WIDTH, rect.size.h);
    draw_battery(c, theme, glyph, (pct, charging), (16, fg, fg));
    let text_x = glyph_x + STATUS_BATT_WIDTH + status::BATTERY_TEXT_GAP;
    c.text(
        ctx.painter,
        (text_x, rect.loc.x + rect.size.w - SP1, rect.loc.y, rect.loc.y + rect.size.h),
        TextAlign::Left,
        &format!("{pct}%"),
        fg,
        &TextStyle::NUM,
    );
}

fn battery_icon_name(pct: u8, charging: bool) -> String {
    let step = ((pct.min(100) + 10) / 20) * 20;
    if charging { format!("battery-{step:03}-charging") } else { format!("battery-{step:03}") }
}

fn draw_qs_action_button(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, slot: Slot, rect: LocalRect,
    (draw_glyph, icon_name): (GlyphPainter, &str),
) {
    if matches!(slot, Slot::QsPowerButton(_)) {
        c.fill(rect, ctx.theme.track_soft, R_ROUND);
    }
    let fg = if ctx.hovered == Some(slot) { ctx.theme.label } else { ctx.theme.muted };
    let side = 16;
    let at = (rect.loc.x + (rect.size.w - side) / 2, rect.loc.y + (rect.size.h - side) / 2);
    if !c.icon(icon_name, side, at, fg) {
        c.glyph(rect, |b, w, h, s| draw_glyph(b, w, h, 15.0, 1.4, s, fg));
    }
}

const fn wifi_icon_name(level: u8) -> &'static str {
    match level {
        0 => "network-wireless-signal-none",
        1 => "network-wireless-signal-weak",
        2 => "network-wireless-signal-ok",
        3 => "network-wireless-signal-good",
        _ => "network-wireless-signal-excellent",
    }
}

const fn volume_icon_name(muted: bool, pct: u8) -> &'static str {
    if muted || pct == 0 {
        "audio-volume-muted"
    } else if pct < 33 {
        "audio-volume-low"
    } else if pct < 66 {
        "audio-volume-medium"
    } else {
        "audio-volume-high"
    }
}

fn draw_qs_tiles(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &QsGeometry, status: &crate::system_info::SystemStatus,
) {
    use crate::i18n::{Key, t};
    let wifi_sub =
        if status.wifi_level > 0 && status.wifi_ssid.is_empty() { t(Key::Connected) } else { &status.wifi_ssid };
    let bt_icon = if status.bluetooth_on { "bluetooth-active" } else { "bluetooth-disabled" };
    let night = status.night_light_on;
    let specs = [
        QsTileSpec {
            rect: geometry.tiles[0].1,
            on: status.wifi_enabled,
            title: t(Key::WiFi),
            subtitle: wifi_sub,
            icon_name: wifi_icon_name(status.wifi_level),
            glyph: crate::paint::draw_wifi_waves_glyph_over,
            more: true,
        },
        QsTileSpec {
            rect: geometry.tiles[1].1,
            on: status.bluetooth_on,
            title: t(Key::Bluetooth),
            subtitle: t(if status.bluetooth_on { Key::On } else { Key::Off }),
            icon_name: bt_icon,
            glyph: crate::paint::draw_bluetooth_glyph_over,
            more: true,
        },
        QsTileSpec {
            rect: geometry.tiles[2].1,
            on: night,
            title: t(Key::NightLight),
            subtitle: t(if night { Key::On } else { Key::LessBlue }),
            icon_name: "night-light",
            glyph: |b, w, h, side, _, s, c| crate::paint::draw_moon_glyph_over(b, w, h, side, s, c),
            more: false,
        },
        QsTileSpec {
            rect: geometry.tiles[3].1,
            on: false,
            title: t(Key::Screenshot),
            subtitle: t(Key::WholeScreen),
            icon_name: "accessories-screenshot",
            glyph: crate::paint::draw_camera_glyph_over,
            more: false,
        },
    ];
    for spec in &specs {
        draw_qs_tile(c, ctx, spec);
    }
}

fn draw_qs_sliders(
    c: &mut Canvas<'_>, theme: &TopbarTheme, geometry: &QsGeometry, status: &crate::system_info::SystemStatus,
    motion: QsMotion,
) {
    let muted = status.volume_muted;
    draw_qs_slider(
        c,
        theme,
        &QsSliderSpec {
            icon: geometry.volume_icon,
            track: geometry.volume_track,
            pct: motion.volume,
            muted,
            icon_name: volume_icon_name(muted, motion.volume),
        },
        |b, w, h, s, c| crate::paint::draw_speaker_glyph_over(b, w, h, 15.0, 1.4, muted, s, c),
    );
    draw_qs_slider(
        c,
        theme,
        &QsSliderSpec {
            icon: geometry.brightness_icon,
            track: geometry.brightness_track,
            pct: motion.brightness,
            muted: false,
            icon_name: "display-brightness",
        },
        |b, w, h, s, c| crate::paint::draw_sun_glyph_over(b, w, h, 15.0, 1.4, s, c),
    );
}

fn draw_quick_settings(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, status: &crate::system_info::SystemStatus, caffeinated: bool,
    motion: QsMotion,
) {
    let geometry = qs_geometry();
    draw_qs_tiles(c, ctx, &geometry, status);
    draw_qs_caffeine_row(c, ctx, geometry.caffeine, caffeinated, motion.caffeine);
    draw_qs_sliders(c, ctx.theme, &geometry, status, motion);
    let divider = local_rect(QS_PAD, geometry.divider_y, c.w - QS_PAD * 2, 1);
    c.fill(divider, ctx.theme.border, 0.0);
    draw_qs_footer(c, ctx, &geometry, status);
}

const fn scale_or_one(scale: i32) -> i32 {
    if scale < 1 { 1 } else { scale }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{buffer_from, canvas};

    fn output() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)))
    }

    fn painter() -> crate::paint::Painter {
        crate::paint::Painter::new(14.0, None)
    }

    fn topbar_with(layouts: &[&str]) -> Topbar {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.set_layouts(layouts.iter().map(|s| (*s).to_string()).collect(), 0);
        bar.set_clock("Mon 21 Sep".to_string(), "12:34".to_string());
        bar.set_app("org.gnome.Nautilus".to_string(), "Nautilus".to_string());
        bar.layout_slots(&painter(), output());
        bar
    }

    mod global_menu_bar {
        use super::*;
        use glide_appmenu::{Activation, MenuItem};

        fn leaf(label: &str, id: i32) -> MenuItem {
            MenuItem::action(label, Activation::DbusMenu(id))
        }

        fn sample() -> Vec<MenuItem> {
            vec![
                MenuItem::submenu(
                    "File",
                    vec![
                        leaf("New", 1),
                        MenuItem::separator(),
                        MenuItem::submenu("Recent", vec![leaf("a.txt", 2), leaf("b.txt", 3)]),
                        MenuItem { enabled: false, ..leaf("Print", 4) },
                    ],
                ),
                MenuItem::submenu("Edit", vec![leaf("Undo", 5)]),
            ]
        }

        fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
            Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)))
        }

        fn bar() -> Topbar {
            let mut bar = topbar_with(&["US", "UA"]);
            assert!(bar.set_app_menu(sample()));
            bar.layout_slots(&painter(), output());
            bar
        }

        fn relayout(bar: &mut Topbar) {
            bar.layout_slots(&painter(), output());
        }

        fn row(path: &[usize]) -> Slot {
            let mut menu = MenuPath::top(path[0]);
            for index in &path[1..] {
                menu = menu.child(*index).expect("within depth");
            }
            Slot::MenuRow(menu)
        }

        #[test]
        fn titles_sit_between_the_app_chip_and_the_clock() {
            let bar = bar();
            let file = bar.slot_rect(Slot::MenuTitle(0)).expect("File title");
            let edit = bar.slot_rect(Slot::MenuTitle(1)).expect("Edit title");
            let clock = bar.slot_rect(Slot::Clock).expect("clock");
            assert!(file.loc.x > PAD_X, "the app chip keeps its place");
            assert!(edit.loc.x >= file.loc.x + file.size.w);
            assert!(edit.loc.x + edit.size.w < clock.loc.x);
        }

        #[test]
        fn titles_that_would_reach_the_clock_are_dropped() {
            let mut bar = topbar_with(&["US", "UA"]);
            let long: Vec<MenuItem> =
                (0..40).map(|i| MenuItem::submenu(&format!("Menu number {i}"), vec![leaf("x", i)])).collect();
            bar.set_app_menu(long);
            relayout(&mut bar);
            let clock = bar.slot_rect(Slot::Clock).expect("clock");
            let rightmost = bar
                .slots
                .iter()
                .filter(|(slot, _)| matches!(slot, Slot::MenuTitle(_)))
                .map(|(_, rect)| rect.loc.x + rect.size.w)
                .max()
                .expect("some titles fit");
            assert!(rightmost < clock.loc.x);
        }

        #[test]
        fn opening_a_title_lays_out_only_enabled_rows() {
            let mut bar = bar();
            assert!(bar.open_app_menu(0));
            relayout(&mut bar);
            assert_eq!(bar.open_popup(), Some(OpenPopup::AppMenu));
            assert!(bar.slot_rect(row(&[0, 0])).is_some());
            assert!(bar.slot_rect(row(&[0, 1])).is_none(), "separators are not targets");
            assert!(bar.slot_rect(row(&[0, 3])).is_none(), "disabled rows are not targets");
            let panel = bar.popup_rect.expect("panel");
            let title = bar.slot_rect(Slot::MenuTitle(0)).expect("title");
            assert!(panel.loc.y >= title.loc.y + title.size.h);
        }

        #[test]
        fn clicking_the_open_title_again_closes_it() {
            let mut bar = bar();
            bar.open_app_menu(0);
            assert!(bar.open_app_menu(0));
            assert!(!bar.has_open_popup());
        }

        #[test]
        fn hovering_another_title_switches_menus_while_one_is_open() {
            let mut bar = bar();
            bar.open_app_menu(0);
            relayout(&mut bar);
            let edit = bar.slot_rect(Slot::MenuTitle(1)).expect("Edit title");
            bar.set_hover(Some(centre(edit)));
            relayout(&mut bar);
            assert!(bar.slot_rect(row(&[1, 0])).is_some(), "Edit's rows are live");
            assert!(bar.slot_rect(row(&[0, 0])).is_none());
        }

        #[test]
        fn hovering_a_submenu_row_cascades_and_a_leaf_collapses_it() {
            let mut bar = bar();
            bar.open_app_menu(0);
            relayout(&mut bar);
            let recent = bar.slot_rect(row(&[0, 2])).expect("Recent row");
            bar.set_hover(Some(centre(recent)));
            relayout(&mut bar);
            assert_eq!(bar.menu_panels.len(), 2);
            let cascade = bar.slot_rect(row(&[0, 2, 1])).expect("b.txt row");
            assert!(cascade.loc.x > recent.loc.x);
            assert!(bar.contains(output(), centre(cascade)), "a click on the cascade must not count as outside");

            let new = bar.slot_rect(row(&[0, 0])).expect("New row");
            bar.set_hover(Some(centre(new)));
            relayout(&mut bar);
            assert_eq!(bar.menu_panels.len(), 1);
        }

        #[test]
        fn activating_a_leaf_closes_the_menu_and_reports_it() {
            let mut bar = bar();
            bar.open_app_menu(0);
            relayout(&mut bar);
            let MenuRowOutcome::Activate(activation) = bar.app_menu_row(MenuPath::top(0).child(0).expect("depth"))
            else {
                panic!("New is a plain item");
            };
            assert_eq!(activation, Activation::DbusMenu(1));
            assert!(!bar.has_open_popup());
        }

        #[test]
        fn disabled_and_submenu_rows_do_not_activate() {
            let mut bar = bar();
            bar.open_app_menu(0);
            let print = MenuPath::top(0).child(3).expect("depth");
            assert_eq!(bar.app_menu_row(print), MenuRowOutcome::Ignored);
            let recent = MenuPath::top(0).child(2).expect("depth");
            assert_eq!(bar.app_menu_row(recent), MenuRowOutcome::Submenu);
            assert!(bar.has_open_popup());
        }

        #[test]
        fn keyboard_walks_rows_cascades_and_neighbouring_menus() {
            let mut bar = bar();
            bar.open_app_menu(0);
            relayout(&mut bar);
            assert!(bar.focus_nav(0, 1));
            assert_eq!(bar.focused, Some(row(&[0, 0])));
            assert!(bar.focus_nav(0, 1));
            assert_eq!(bar.focused, Some(row(&[0, 2])), "skips the separator");
            assert!(bar.focus_nav(0, 1));
            assert_eq!(bar.focused, Some(row(&[0, 0])), "wraps past the disabled row");

            bar.set_focused(Some(row(&[0, 2])));
            assert!(bar.focus_nav(1, 0));
            relayout(&mut bar);
            assert_eq!(bar.focused, Some(row(&[0, 2, 0])));
            assert!(bar.focus_nav(-1, 0));
            relayout(&mut bar);
            assert_eq!(bar.focused, Some(row(&[0, 2])));

            bar.set_focused(Some(row(&[0, 0])));
            assert!(bar.focus_nav(1, 0));
            relayout(&mut bar);
            assert!(bar.slot_rect(row(&[1, 0])).is_some(), "Right on a leaf moves to Edit");
        }

        #[test]
        fn losing_the_menu_closes_an_open_dropdown() {
            let mut bar = bar();
            bar.open_app_menu(0);
            assert!(bar.set_app_menu(Vec::new()));
            assert!(!bar.has_open_popup());
            relayout(&mut bar);
            assert!(bar.slot_rect(Slot::MenuTitle(0)).is_none());
        }

        #[test]
        fn the_desktop_menu_is_a_valid_menu_bar() {
            let mut bar = topbar_with(&["US"]);
            bar.set_app(String::new(), String::new());
            bar.set_app_menu(crate::global_menu::desktop_menu(false, false));
            relayout(&mut bar);
            let first = bar.slot_rect(Slot::MenuTitle(0)).expect("Glide menu");
            assert_eq!(first.loc.x, PAD_X, "with no app the menus start at the edge");
        }
    }

    fn playing() -> MediaWidget {
        MediaWidget {
            title: "Пісня про рушник".to_string(),
            artist: "Хор".to_string(),
            source: "Rhythmbox".to_string(),
            art: None,
            art_large: None,
            app_icons: Vec::new(),
            accent: Color([0.9, 0.63, 0.29, 1.0]),
            playing: true,
            can_next: true,
            can_previous: true,
            progress: Some(0.38),
        }
    }

    fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)))
    }

    fn click(bar: &Topbar, slot: Slot) -> Option<TopbarAction> {
        bar.action_at(centre(bar.slot_rect(slot).expect("slot")))
    }

    fn with_popup(mut bar: Topbar, popup: OpenPopup) -> Topbar {
        bar.toggle_popup(popup);
        bar.layout_slots(&painter(), output());
        bar
    }

    fn playing_bar() -> Topbar {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());
        bar
    }

    fn inside(outer: Rectangle<i32, Logical>, rect: Rectangle<i32, Logical>) -> bool {
        rect.loc.x >= outer.loc.x
            && rect.loc.y >= outer.loc.y
            && rect.loc.x + rect.size.w <= outer.loc.x + outer.size.w
            && rect.loc.y + rect.size.h <= outer.loc.y + outer.size.h
    }

    #[test]
    fn media_grows_left_of_a_fixed_clock_with_ordered_transport_targets() {
        let silent = topbar_with(&["US", "UA"]);
        assert!(silent.media.is_none());
        assert!(silent.slot_rect(Slot::Media).is_none());
        assert!(silent.slot_rect(Slot::MediaPlay).is_none());

        let bar = playing_bar();
        let rect = |slot| bar.slot_rect(slot).expect("slot");
        let clock = rect(Slot::Clock);
        assert_eq!(silent.slot_rect(Slot::Clock), Some(clock), "the clock is the anchor");
        assert!(rect(Slot::Media).loc.x < clock.loc.x);
        let (prev, play, next) = (rect(Slot::MediaPrev), rect(Slot::MediaPlay), rect(Slot::MediaNext));
        assert!(prev.loc.x + prev.size.w <= play.loc.x);
        assert!(play.loc.x + play.size.w <= next.loc.x);
        assert!(next.loc.x + next.size.w <= clock.loc.x);
        for (slot, want) in [
            (Slot::MediaPrev, TopbarAction::MediaPrevious),
            (Slot::MediaPlay, TopbarAction::MediaPlayPause),
            (Slot::MediaNext, TopbarAction::MediaNext),
        ] {
            assert_eq!(click(&bar, slot), Some(want));
        }
        let media = rect(Slot::Media);
        let name = Point::from((f64::from(media.loc.x + 8), centre(media).y));
        assert_eq!(bar.action_at(name), Some(TopbarAction::ToggleMediaPlayer));
    }

    #[test]
    fn the_media_widget_width_follows_title_and_artist_up_to_a_cap() {
        let painter = painter();
        let width = |title: String, artist: &str| {
            media_width(&painter, &MediaWidget { title, artist: artist.to_string(), ..playing() })
        };
        let (short, long) = (width("Hi".into(), "Хор"), width("x".repeat(400), "Хор"));
        assert!(long > short);
        assert!(
            long <= MEDIA_PAD * 2
                + MEDIA_ART
                + MEDIA_GAP * 2
                + MEDIA_TITLE_MAX
                + MEDIA_TAG_GAP
                + MEDIA_ARTIST_MAX
                + transport_width()
        );
        assert!(width("Hi".into(), "") < short);
    }

    #[test]
    fn the_player_popup_needs_a_track_and_seeks_where_the_pointer_lands() {
        let silent = with_popup(topbar_with(&["US", "UA"]), OpenPopup::MediaPlayer);
        assert!(silent.popup_rect().is_none(), "silence has no player to show");

        let bar = with_popup(playing_bar(), OpenPopup::MediaPlayer);
        let popup = bar.popup_rect().expect("popup rect");
        for slot in [Slot::PopMediaPrev, Slot::PopMediaPlay, Slot::PopMediaNext, Slot::PopMediaSeek] {
            assert!(inside(popup, bar.slot_rect(slot).expect("popup slot")));
        }
        let track = bar.slot_rect(Slot::PopMediaSeek).expect("seek slot");
        let y = centre(track).y;
        let middle = f64::from(track.loc.x) + f64::from(track.size.w) / 2.0;
        assert_eq!(bar.action_at(Point::from((middle, y))), Some(TopbarAction::MediaSeek(50)));
        let start = Point::from((f64::from(track.loc.x), y));
        assert_eq!(bar.action_at(start), Some(TopbarAction::MediaSeek(0)));

        let media = bar.slot_rect(Slot::Media).expect("media slot");
        let next = bar.slot_rect(Slot::MediaNext).expect("next slot");
        let widget_centre = i32::midpoint(media.loc.x, next.loc.x + next.size.w + MEDIA_PAD);
        assert!((widget_centre - (popup.loc.x + popup.size.w / 2)).abs() <= 1);
    }

    #[test]
    fn setters_only_dirty_the_bar_when_something_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.set_media_progress(Some(0.5)), "nothing is playing");
        assert!(bar.set_media(Some(playing())));
        assert!(!bar.set_media(Some(playing())));
        assert!(bar.set_media_progress(Some(0.5)));
        assert!(!bar.set_media_progress(Some(0.5)));
        assert!(bar.set_media_progress(None));
        let paused = MediaWidget { playing: false, ..playing() };
        assert!(bar.set_media(Some(paused)));
        assert!(bar.set_media(None) && !bar.set_media(None));

        assert!(!bar.set_clock("Mon 21 Sep".to_string(), "12:34".to_string()));
        assert!(bar.set_clock("Mon 21 Sep".to_string(), "12:35".to_string()));
        assert!(bar.set_clock("Tue 22 Sep".to_string(), "12:35".to_string()));
        let two = || vec!["US".to_string(), "UA".to_string()];
        assert!(!bar.set_layouts(two(), 0));
        assert!(bar.set_layouts(two(), 1));
        assert!(bar.set_layouts(vec!["US".to_string()], 0));
        assert!(!bar.set_app("org.gnome.Nautilus".to_string(), "Nautilus".to_string()));
        assert!(bar.set_app("kitty".to_string(), "Kitty".to_string()));
        assert!(!bar.set_dnd(false) && bar.set_dnd(true) && !bar.set_dnd(true) && bar.set_dnd(false));
        assert!(!bar.set_unread(0) && bar.set_unread(1) && !bar.set_unread(1) && bar.set_unread(0));
    }

    #[test]
    fn every_control_sits_inside_the_strip_and_reports_its_own_action() {
        let bar = topbar_with(&["US", "UA"]);
        let strip = bar.rect(output());
        assert_eq!(strip, Rectangle::new(output().loc, Size::from((1920, HEIGHT))));
        for (slot, rect) in &bar.slots {
            assert!(rect.loc.y >= strip.loc.y && rect.loc.y + rect.size.h <= HEIGHT);
            assert!(rect.loc.x + rect.size.w <= strip.size.w - PAD_X + 1);
            assert_eq!(bar.action_at(centre(*rect)), Some(slot.action()));
        }
        assert_eq!(bar.action_at(Point::from((600.0, 17.0))), None, "empty space");
        assert!(!bar.contains(output(), Point::from((100.0, f64::from(HEIGHT) + 1.0))));
        assert!(bar.contains(output(), Point::from((100.0, 4.0))));
        let disabled = Topbar::new(TopbarTheme { enabled: false, ..TopbarTheme::default() });
        assert!(!disabled.contains(output(), Point::from((100.0, 4.0))));

        let painter = painter();
        let chip = PAD_X + app_chip_width(&painter, &bar.app_name, true);
        let clock = text_clock_width(&painter, &bar.date, &bar.clock, bar.unread);
        assert!(chip < (output().size.w - clock) / 2, "the app chip never reaches the clock");
    }

    #[test]
    fn layouts_are_a_segmented_control_a_dropdown_or_a_single_cycle_button() {
        let two = topbar_with(&["US", "UA"]);
        assert!(two.segmented());
        assert!(two.slot_rect(Slot::LayoutCycle).is_none());
        let control = two.layout_control_rect().expect("control rect");
        let seg = |index| two.slot_rect(Slot::LayoutPick(index)).expect("segment");
        assert!(inside(control, seg(0)) && inside(control, seg(1)));
        assert!(seg(0).loc.x + seg(0).size.w <= seg(1).loc.x);
        assert_eq!(click(&two, Slot::LayoutPick(1)), Some(TopbarAction::SelectLayout(1)));
        let dnd = two.slot_rect(Slot::DoNotDisturb).expect("dnd slot");
        assert!(dnd.loc.x + dnd.size.w <= seg(0).loc.x);

        let three = topbar_with(&["US", "UA", "PL"]);
        assert!(!three.segmented());
        assert!(three.slot_rect(Slot::LayoutDropdown).is_some());
        assert!(three.slot_rect(Slot::LayoutPick(0)).is_none());

        let one = topbar_with(&["US"]);
        let coffee = one.slot_rect(Slot::Caffeine).expect("caffeine slot");
        let cycle = one.slot_rect(Slot::LayoutCycle).expect("layout slot");
        assert!(cycle.loc.x + cycle.size.w <= coffee.loc.x);

        let mut none = Topbar::new(TopbarTheme::default());
        none.layout_slots(&painter(), output());
        assert!(none.slot_rect(Slot::LayoutCycle).is_none());
        assert!(none.slot_rect(Slot::LayoutPick(0)).is_none());
        assert!(none.slot_rect(Slot::Caffeine).is_some());

        let painter = painter();
        let (narrow, wide) = (segment_width(&painter, "US"), segment_width(&painter, "UKR"));
        assert!(wide >= narrow && narrow >= SEG_MIN_WIDTH);
        assert!(Topbar::caffeine_width() > SWITCH_WIDTH);
        assert!(Topbar::caffeine_width() >= BUTTON_MIN_WIDTH);
    }

    #[test]
    fn the_language_menu_and_notification_centre_open_report_and_close() {
        let mut bar = with_popup(topbar_with(&["US", "UA", "PL"]), OpenPopup::LanguageMenu);
        assert_eq!(bar.open_popup(), Some(OpenPopup::LanguageMenu));
        for index in 0..3 {
            assert!(bar.slot_rect(Slot::LangMenuItem(index)).is_some());
        }
        assert_eq!(click(&bar, Slot::LangMenuSettings), Some(TopbarAction::OpenInputSettings));
        let settings = centre(bar.slot_rect(Slot::LangMenuSettings).expect("settings"));
        assert!(bar.contains(output(), settings));
        assert!(bar.close_popup());
        bar.layout_slots(&painter(), output());
        assert!(!bar.has_open_popup());
        assert!(bar.slot_rect(Slot::LangMenuItem(0)).is_none());

        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_nc_items(vec![crate::notifications::NcItem {
            id: 42,
            app_name: "Telegram".to_string(),
            title: "Hello".to_string(),
            body: "World".to_string(),
            time: "2 хв".to_string(),
            icon: None,
            count: 1,
        }]);
        let bar = with_popup(bar, OpenPopup::NotificationCenter);
        assert_eq!(bar.open_popup(), Some(OpenPopup::NotificationCenter));
        assert!(bar.slot_rect(Slot::NcClearAll).is_some());
        assert!(bar.slot_rect(Slot::NcDndToggle).is_some());
        assert_eq!(click(&bar, Slot::NcItem(42)), Some(TopbarAction::ActivateNotification(42)));
        assert_eq!(click(&bar, Slot::NcItemDismiss(42)), Some(TopbarAction::DismissNotification(42)));
    }

    #[test]
    fn hover_is_paint_state_that_changes_only_between_controls() {
        let mut bar = topbar_with(&["US", "UA"]);
        let before = bar.layout_key;
        let coffee = centre(bar.slot_rect(Slot::Caffeine).expect("caffeine slot"));
        assert!(bar.set_hover(Some(coffee)));
        assert!(!bar.set_hover(Some(coffee)), "same control, no repaint");
        assert_eq!(bar.hovered_in(Slot::in_layout), None);
        assert_eq!(bar.hovered_in(Slot::in_media_popup), None);
        assert_eq!(bar.hovered_in(Slot::in_quick_settings), None);
        assert_eq!(bar.hovered_in(Slot::in_strip), Some(Slot::Caffeine));
        bar.layout_slots(&painter(), output());
        assert_eq!(bar.layout_key, before, "hover is paint state, not geometry");
        assert!(bar.set_hover(None) && !bar.set_hover(None));
        assert!(bar.set_unread(4));
        bar.layout_slots(&painter(), output());
        assert_ne!(bar.layout_key, before, "a badge changes the clock width");
    }

    #[test]
    fn names_are_shortened_for_the_strip() {
        assert_eq!(short_layout_name("Ukrainian"), "UKR");
        assert_eq!(short_layout_name("English (US)"), "US");
        assert_eq!(short_layout_name("Polish (qwertz)"), "QWE");
        assert_eq!(app_display_name("org.gnome.Nautilus", ""), "Nautilus");
        assert_eq!(app_display_name("kitty", ""), "Kitty");
        assert_eq!(app_display_name("", "Some Window"), "Some Window");
        let shown = app_display_name(&"a".repeat(80), "");
        assert!(shown.chars().count() <= APP_NAME_MAX && shown.ends_with('…'));
    }

    fn text_clock_width(painter: &crate::paint::Painter, date: &str, clock: &str, unread: usize) -> i32 {
        clock_width(clock_date_width(painter, date), clock_time_width(painter, clock), painter, unread)
    }

    #[test]
    fn the_clock_reserves_room_for_both_the_date_and_the_time() {
        let painter = painter();
        let with_date = text_clock_width(&painter, "Mon 21 Sep", "12:34", 0);
        let without = text_clock_width(&painter, "", "12:34", 0);
        assert!(with_date > without);
    }

    #[test]
    fn the_clock_keeps_its_width_as_the_minutes_tick() {
        let painter = painter();
        let reference = text_clock_width(&painter, "Mon 21 Sep", "88:88", 0);
        for time in ["11:11", "12:34", "00:00", "19:07", "23:59"] {
            assert_eq!(text_clock_width(&painter, "Mon 21 Sep", time, 0), reference, "{time}");
        }
        assert_eq!(widest_digits("12:34"), "88:88");
    }

    fn at(day: u32, weekday: u32, month0: u32, hour: u32) -> calendar::LocalNow {
        calendar::LocalNow { year: 2026, month0, day, weekday, hour, minute: 5 }
    }

    #[test]
    fn the_clock_box_holds_its_width_across_days_months_and_hours() {
        let painter = painter();
        let mut bar = topbar_with(&["US", "UA"]);
        let mut widths = Vec::new();
        for (day, weekday, month0) in [(24, 4, 8), (1, 3, 0), (31, 5, 4), (7, 0, 10)] {
            for hour in [0, 9, 13, 23] {
                bar.set_now(at(day, weekday, month0, hour));
                widths.push(bar.clock_slot_width(&painter));
            }
        }
        assert!(widths.windows(2).all(|pair| pair[0] == pair[1]), "{widths:?}");
    }

    #[test]
    fn turning_the_date_off_leaves_only_the_time() {
        let theme = TopbarTheme { clock_date: false, clock_24h: Some(true), ..TopbarTheme::default() };
        let mut bar = Topbar::new(theme);
        bar.set_now(at(24, 4, 8, 14));
        assert_eq!(bar.date, "");
        assert_eq!(bar.clock, "14:05");
        bar.reconfigure(TopbarTheme { clock_24h: Some(false), ..TopbarTheme::default() });
        bar.set_now(at(24, 4, 8, 15));
        assert_eq!(bar.clock, "3:05 PM");
        assert!(!bar.date.is_empty());
    }

    #[test]
    fn numbers_are_set_in_tabular_figures_without_uppercase_tracking() {
        for style in [CLOCK_STYLE, COUNT_STYLE] {
            assert!(style.tabular);
            assert!(!style.uppercase);
        }
        assert!((CLOCK_STYLE.size_px - glide_theme::TextStyle::TOPBAR.size_px).abs() < f32::EPSILON);
    }

    #[test]
    fn a_battery_reading_keeps_its_width_within_a_digit_count() {
        let painter = painter();
        let reference = numeric_label_width(&painter, "88%", &COUNT_STYLE);
        for pct in ["11%", "47%", "90%"] {
            assert_eq!(numeric_label_width(&painter, pct, &COUNT_STYLE), reference, "{pct}");
        }
    }

    #[test]
    fn an_unread_notification_count_expands_the_clock() {
        let painter = painter();
        let without_unread = text_clock_width(&painter, "Mon 21 Sep", "12:34", 0);
        let with_unread = text_clock_width(&painter, "Mon 21 Sep", "12:34", 3);
        assert!(with_unread > without_unread);
    }

    #[test]
    fn local_clock_reads_as_a_wall_clock() {
        let clock = local_clock().expect("the system clock must be readable");
        assert_eq!(clock.len(), 5);
        assert_eq!(clock.as_bytes()[2], b':');
    }

    #[test]
    fn the_local_calendar_date_is_readable() {
        let now = calendar::LocalNow::now().expect("the system clock must be readable");
        assert!((1..=31).contains(&now.day));
        assert!(now.weekday < 7 && now.month0 < 12 && now.hour < 24 && now.minute < 60);
    }

    #[test]
    fn quick_settings_opens_with_correct_slots_and_can_be_closed() {
        let bar = topbar_with(&["US", "UA"]);
        assert!(bar.slot_rect(Slot::Status).is_some() && !bar.has_open_popup());
        let mut bar = with_popup(bar, OpenPopup::QuickSettings);
        assert_eq!(bar.open_popup(), Some(OpenPopup::QuickSettings));
        for slot in [
            Slot::QsBluetooth,
            Slot::QsNightLight,
            Slot::QsVolumeMute,
            Slot::QsBrightnessTrack,
            Slot::QsSettings,
            Slot::QsWifiMore,
            Slot::QsBluetoothMore,
            Slot::Caffeine,
        ]
        .into_iter()
        .chain(panels::POWER_ROW.map(Slot::QsPowerButton))
        {
            assert!(bar.slot_rect(slot).is_some(), "{slot:?}");
        }
        for (slot, action) in [
            (Slot::QsWifi, TopbarAction::QsToggleWifi),
            (Slot::QsScreenshot, TopbarAction::QsScreenshot),
            (Slot::QsCaffeine, TopbarAction::ToggleCaffeine),
        ] {
            assert_eq!(click(&bar, slot), Some(action));
        }
        let centre = centre(bar.slot_rect(Slot::QsWifi).expect("wifi slot"));

        let vol_slot = bar.slot_rect(Slot::QsVolumeTrack).expect("vol slot");
        let quarter = Point::from((
            f64::from(vol_slot.loc.x) + f64::from(vol_slot.size.w) * 0.25,
            f64::from(vol_slot.loc.y + vol_slot.size.h / 2),
        ));
        assert_eq!(bar.action_at(quarter), Some(TopbarAction::QsVolumeAdjust(25)));

        assert!(bar.contains(output(), centre));
        assert!(bar.close_popup());
        bar.layout_slots(&painter(), output());
        assert!(!bar.has_open_popup());
        assert!(bar.slot_rect(Slot::QsWifi).is_none());
    }

    #[test]
    fn the_slider_hit_area_matches_the_drawn_track() {
        let geometry = qs_geometry();
        let track = geometry.volume_track;
        assert_eq!(
            track.loc.x,
            geometry.volume_icon.loc.x + QS_SLIDER_ICON + QS_GRID_GAP,
            "clicking the visual start of the track must read as 0%"
        );
        assert_eq!(track.loc.x + track.size.w, QS_WIDTH - QS_PAD);
        assert_eq!(geometry.brightness_track.loc.x, track.loc.x);
    }

    #[test]
    fn the_quick_settings_popup_is_exactly_as_tall_as_its_content() {
        let geometry = qs_geometry();
        let power_bottom = geometry.power[0].1.loc.y + geometry.power[0].1.size.h;
        assert_eq!(geometry.height, power_bottom + QS_PAD);
        assert!(geometry.footer.loc.y + geometry.footer.size.h < geometry.power[0].1.loc.y);
        let settings = (Slot::QsSettings, geometry.settings);
        for (_, rect) in geometry.tiles.iter().chain(geometry.power.iter()).chain(std::iter::once(&settings)) {
            assert!(rect.loc.x >= QS_PAD);
            assert!(rect.loc.x + rect.size.w <= QS_WIDTH - QS_PAD);
            assert!(rect.loc.y + rect.size.h <= geometry.height - QS_PAD);
        }
        let mut bar = topbar_with(&["US", "UA"]);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        let popup = bar.popup_rect().expect("popup rect");
        assert_eq!(popup.size.h, geometry.height);
    }

    #[test]
    fn every_popup_hangs_the_same_distance_below_the_strip() {
        let mut bar = topbar_with(&["US", "UA", "PL"]);
        bar.set_media(Some(playing()));
        let mut tops = Vec::new();
        for popup in
            [OpenPopup::QuickSettings, OpenPopup::NotificationCenter, OpenPopup::LanguageMenu, OpenPopup::MediaPlayer]
        {
            bar.close_popup();
            bar.toggle_popup(popup);
            bar.layout_slots(&painter(), output());
            tops.push(bar.popup_rect().expect("popup rect").loc.y);
        }
        assert!(tops.iter().all(|top| *top == HEIGHT + POPUP_GAP), "{tops:?}");
    }

    #[test]
    fn a_theme_built_from_light_tokens_is_not_dark() {
        let tokens = glide_theme::Tokens::new(glide_theme::Appearance::Light, glide_theme::Accent::Blue);
        let theme = TopbarTheme::from_tokens(&tokens, true);
        assert!(!theme.dark);
        assert_ne!(theme.revision(), TopbarTheme::default().revision());
    }

    #[test]
    fn the_status_block_grows_to_hold_the_battery_instead_of_adding_a_slot() {
        let mut bar = topbar_with(&["US", "UA"]);
        let before = bar.slot_rect(Slot::Status).expect("status block");
        let strip_slots = bar.slots.len();

        let status = crate::system_info::SystemStatus { battery_pct: Some(82), ..Default::default() };
        bar.set_system_status(status);
        bar.layout_slots(&painter(), output());
        let after = bar.slot_rect(Slot::Status).expect("status block");
        assert_eq!(bar.slots.len(), strip_slots, "no per-indicator slot appears");
        assert!(after.size.w > before.size.w);
        assert_eq!(after.loc.x + after.size.w, before.loc.x + before.size.w, "the block stays pinned to the edge");
        let search = bar.slot_rect(Slot::Search).expect("search slot");
        assert!(search.loc.x + search.size.w <= after.loc.x);
    }

    #[test]
    fn hiding_the_battery_percentage_narrows_the_status_block() {
        let status = crate::system_info::SystemStatus { battery_pct: Some(82), ..Default::default() };
        let mut with_pct = topbar_with(&["US"]);
        with_pct.set_system_status(status.clone());
        with_pct.layout_slots(&painter(), output());
        let mut bare = Topbar::new(TopbarTheme { battery_percentage: false, ..TopbarTheme::default() });
        bare.set_system_status(status);
        bare.layout_slots(&painter(), output());
        let wide = with_pct.slot_rect(Slot::Status).expect("block").size.w;
        let narrow = bare.slot_rect(Slot::Status).expect("block").size.w;
        assert!(narrow < wide);
    }

    #[test]
    fn the_right_cluster_runs_from_the_edge_and_ends_before_caffeine() {
        let mut bar = topbar_with(&["US", "UA"]);
        let status = crate::system_info::SystemStatus {
            battery_pct: Some(50),
            bluetooth_on: true,
            vpn_active: true,
            privacy: crate::system_info::Privacy { microphone: vec!["Firefox".into()], ..Default::default() },
            ..Default::default()
        };
        bar.set_system_status(status);
        bar.layout_slots(&painter(), output());
        let order = [Slot::Status, Slot::Search, Slot::Privacy, Slot::Caffeine];
        let rects: Vec<_> =
            order.iter().map(|slot| bar.slot_rect(*slot).unwrap_or_else(|| panic!("{slot:?}"))).collect();
        let right_edge = output().size.w - PAD_X;
        assert_eq!(rects[0].loc.x + rects[0].size.w, right_edge);
        for pair in rects.windows(2) {
            assert!(pair[1].loc.x + pair[1].size.w <= pair[0].loc.x, "{pair:?}");
        }
    }

    #[test]
    fn clicking_the_indicators_reports_their_actions() {
        let mut bar = topbar_with(&["US"]);
        bar.set_system_status(crate::system_info::SystemStatus {
            battery_pct: Some(50),
            bluetooth_on: true,
            ..Default::default()
        });
        bar.layout_slots(&painter(), output());
        let click = |bar: &Topbar, slot: Slot| {
            let rect = bar.slot_rect(slot).expect("slot");
            bar.action_at(Point::from((
                f64::from(rect.loc.x + rect.size.w / 2),
                f64::from(rect.loc.y + rect.size.h / 2),
            )))
        };
        assert_eq!(click(&bar, Slot::Search), Some(TopbarAction::ToggleLauncher));
        assert_eq!(click(&bar, Slot::Status), Some(TopbarAction::ToggleQuickSettings));
        let block = bar.slot_rect(Slot::Status).expect("status block");
        let y = f64::from(block.loc.y + block.size.h / 2);
        for x in [block.loc.x + 1, block.loc.x + block.size.w - 1] {
            let pos = Point::from((f64::from(x), y));
            assert_eq!(bar.slot_at(pos), Some(Slot::Status), "one hit target");
        }
        assert!(bar.set_hover(Some(Point::from((f64::from(block.loc.x + 1), y)))));
        assert!(
            !bar.set_hover(Some(Point::from((f64::from(block.loc.x + block.size.w - 1), y)))),
            "the whole block shares one hover state"
        );
    }

    #[test]
    fn the_battery_lives_in_quick_settings_behind_its_pill() {
        let mut bar = topbar_with(&["US"]);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        assert_eq!(bar.slot_rect(Slot::QsBattery), None, "no battery, no pill");

        bar.set_system_status(crate::system_info::SystemStatus { battery_pct: Some(50), ..Default::default() });
        bar.layout_slots(&painter(), output());
        let pill = bar.slot_rect(Slot::QsBattery).expect("battery pill");
        let gear = bar.slot_rect(Slot::QsSettings).expect("settings");
        assert!(pill.loc.x + pill.size.w <= gear.loc.x);
        assert_eq!(pill.loc.y, gear.loc.y);
        assert!(bar.popup_rect().expect("qs").contains(pill.loc));
        assert_eq!(
            bar.action_at(Point::from((
                f64::from(pill.loc.x + pill.size.w / 2),
                f64::from(pill.loc.y + pill.size.h / 2),
            ))),
            Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Battery)))
        );

        bar.set_focused(Some(Slot::QsBattery));
        assert!(bar.open_detail(Detail::Battery));
        bar.layout_slots(&painter(), output());
        assert_eq!(bar.focused, Some(Slot::QsBack));
        assert!(bar.slot_rect(Slot::QsDetailToggle).is_none(), "no radio");
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused_action(), Some(TopbarAction::QsPanel(PanelAction::Settings)));
        assert!(bar.close_detail());
        assert_eq!(bar.focused, Some(Slot::QsBattery));
    }

    #[test]
    fn privacy_dots_appear_when_capture_starts_and_leave_after_fading() {
        let mut bar = topbar_with(&["US"]);
        assert_eq!(bar.privacy_dots(), [false; 3]);
        assert!(bar.set_screen_capture(vec!["obs".into()]));
        assert_eq!(bar.privacy_dots(), [false, false, true]);
        bar.layout_slots(&painter(), output());
        assert!(bar.slot_rect(Slot::Privacy).is_some());
        assert!(bar.set_screen_capture(Vec::new()));
        assert_eq!(bar.privacy_dots(), [false, false, true], "held while fading");
        std::thread::sleep(glide_theme::motion::t2() + Duration::from_millis(20));
        bar.settle_motion();
        assert_eq!(bar.privacy_dots(), [false; 3]);
        bar.layout_slots(&painter(), output());
        assert_eq!(bar.slot_rect(Slot::Privacy), None);
    }

    #[test]
    fn the_clock_popover_puts_the_calendar_above_the_notifications() {
        let mut bar = topbar_with(&["US"]);
        bar.set_now(at(24, 4, 8, 12));
        bar.toggle_popup(OpenPopup::NotificationCenter);
        bar.layout_slots(&painter(), output());
        let popup = bar.popup_rect().expect("clock popover");
        let month = bar.slot_rect(Slot::CalMonth).expect("month title");
        let next = bar.slot_rect(Slot::CalNext).expect("next month");
        let clear = bar.slot_rect(Slot::NcClearAll).expect("clear all");
        assert!(popup.contains(month.loc) && popup.contains(next.loc));
        assert!(month.loc.y + month.size.h < clear.loc.y);
        assert!(clear.loc.y - popup.loc.y >= calendar::BLOCK_H);
    }

    #[test]
    fn arrows_on_the_month_title_page_months_and_enter_returns_to_today() {
        let mut bar = topbar_with(&["US"]);
        bar.set_now(at(24, 4, 8, 12));
        bar.toggle_popup(OpenPopup::NotificationCenter);
        bar.layout_slots(&painter(), output());
        let today = bar.calendar.month();
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused_action(), Some(TopbarAction::CalendarToday));
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.calendar.month(), today.shifted(1));
        assert!(bar.focus_nav(-1, 0));
        assert!(bar.focus_nav(-1, 0));
        assert_eq!(bar.calendar.month(), today.shifted(-1));
        assert!(bar.calendar_today());
        assert_eq!(bar.calendar.month(), today);
        assert!(bar.close_popup());
        bar.shift_calendar(3);
        bar.toggle_popup(OpenPopup::NotificationCenter);
        assert_eq!(bar.calendar.month(), today, "reopening starts on today");
    }

    fn quick_settings() -> Topbar {
        with_popup(topbar_with(&["US", "UA"]), OpenPopup::QuickSettings)
    }

    #[test]
    fn arrows_walk_the_quick_settings_tiles_and_enter_reports_them() {
        let mut none = topbar_with(&["US", "UA"]);
        assert!(!none.focus_nav(1, 0) && !none.has_focus());
        assert_eq!(none.focus_adjust(5), None, "no popup, no focus");

        let mut bar = quick_settings();
        assert!(!bar.focus_nav(0, 0), "a zero step never moves focus");
        assert!(!bar.has_focus());
        assert_eq!(bar.focused_action(), None);
        for (dx, dy, slot, action) in [
            (1, 0, Slot::QsWifi, Some(TopbarAction::QsToggleWifi)),
            (1, 0, Slot::QsWifiMore, Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Wifi)))),
            (1, 0, Slot::QsBluetooth, Some(TopbarAction::QsToggleBluetooth)),
            (0, 1, Slot::QsScreenshot, Some(TopbarAction::QsScreenshot)),
            (-1, 0, Slot::QsNightLight, None),
            (0, -1, Slot::QsWifi, None),
        ] {
            assert!(bar.focus_nav(dx, dy));
            assert_eq!(bar.focused, Some(slot));
            if action.is_some() {
                assert_eq!(bar.focused_action(), action);
            }
        }
        assert!(!bar.focus_nav(-1, 0), "nothing lies left of the first tile");
        assert!(!bar.focus_nav(0, -1), "nothing lies above the first row");
        assert!(bar.close_popup());
        assert!(!bar.has_focus() && bar.focused_action().is_none());

        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        bar.toggle_popup(OpenPopup::QuickSettings);
        assert!(!bar.has_focus(), "toggling the popup shut drops focus");
    }

    #[test]
    fn arrows_reach_every_row_of_quick_settings_from_top_to_bottom() {
        let mut bar = quick_settings();
        bar.focus_nav(0, 1);
        let mut visited = vec![bar.focused.expect("focus")];
        while bar.focus_nav(0, 1) {
            visited.push(bar.focused.expect("focus"));
        }
        assert_eq!(
            visited,
            [
                Slot::QsWifi,
                Slot::QsNightLight,
                Slot::QsCaffeine,
                Slot::QsVolumeTrack,
                Slot::QsBrightnessTrack,
                Slot::QsPowerButton(PowerAction::Restart),
            ]
        );
        let mut sideways = Vec::new();
        while bar.focus_nav(-1, 0) {
            sideways.push(bar.focused.expect("focus"));
        }
        while bar.focus_nav(1, 0) {
            sideways.push(bar.focused.expect("focus"));
        }
        assert_eq!(
            sideways,
            [
                Slot::QsPowerButton(PowerAction::Sleep),
                Slot::QsPowerButton(PowerAction::Lock),
                Slot::QsPowerButton(PowerAction::Sleep),
                Slot::QsPowerButton(PowerAction::Restart),
                Slot::QsPowerButton(PowerAction::ShutDown),
                Slot::QsPowerButton(PowerAction::LogOut),
            ]
        );
        assert!(bar.focus_nav(0, -1));
        assert_eq!(bar.focused, Some(Slot::QsSettings));
    }

    #[test]
    fn adjusting_a_focused_slider_clamps_to_the_percent_range() {
        let mut bar = quick_settings();
        bar.set_system_status(crate::system_info::SystemStatus {
            volume_pct: 98,
            brightness_pct: 3,
            ..Default::default()
        });

        bar.focused = Some(Slot::QsVolumeTrack);
        assert_eq!(bar.focus_adjust(5), Some(TopbarAction::QsVolumeAdjust(100)));
        assert_eq!(bar.focus_adjust(5), Some(TopbarAction::QsVolumeAdjust(100)));
        assert_eq!(bar.focus_adjust(-5), Some(TopbarAction::QsVolumeAdjust(95)));

        bar.focused = Some(Slot::QsBrightnessTrack);
        assert_eq!(bar.focus_adjust(-5), Some(TopbarAction::QsBrightnessAdjust(0)));
        assert_eq!(bar.focus_adjust(5), Some(TopbarAction::QsBrightnessAdjust(5)));

        bar.focused = Some(Slot::QsWifi);
        assert_eq!(bar.focus_adjust(5), None, "tiles are not sliders");
    }

    #[test]
    fn a_slider_adjustment_repaints_quick_settings() {
        let mut bar = quick_settings();
        let before = bar.status_rev;
        bar.focused = Some(Slot::QsVolumeTrack);
        bar.focus_adjust(5);
        assert_ne!(bar.status_rev, before);
        assert_eq!(bar.system_status().volume_pct, 55);
    }

    #[test]
    fn arrows_walk_the_language_menu_top_to_bottom() {
        let mut bar = with_popup(topbar_with(&["US", "UA", "PL"]), OpenPopup::LanguageMenu);
        bar.focus_nav(0, 1);
        assert_eq!(bar.focused, Some(Slot::LangMenuItem(0)));
        bar.focus_nav(0, 1);
        bar.focus_nav(0, 1);
        assert_eq!(bar.focused, Some(Slot::LangMenuItem(2)));
        bar.focus_nav(0, 1);
        assert_eq!(bar.focused, Some(Slot::LangMenuSettings));
        assert!(!bar.focus_nav(0, 1));
        assert_eq!(bar.focused_action(), Some(TopbarAction::OpenInputSettings));
    }

    #[test]
    fn arrows_stay_in_their_lane_across_the_notification_center() {
        let mut bar = topbar_with(&["US", "UA"]);
        let item = |id: u32| crate::notifications::NcItem {
            id,
            app_name: "Telegram".to_string(),
            title: "Hello".to_string(),
            body: "World".to_string(),
            time: "2 хв".to_string(),
            icon: None,
            count: 1,
        };
        bar.set_nc_items(vec![item(1), item(2)]);
        let mut bar = with_popup(bar, OpenPopup::NotificationCenter);
        bar.focus_nav(0, 1);
        assert_eq!(bar.focused, Some(Slot::CalMonth), "the calendar heads the clock popover");
        bar.set_focused(Some(Slot::NcClearAll));
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::NcItem(1)), "the dismiss button shares no column with clear-all");
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused, Some(Slot::NcItemDismiss(1)));
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::NcItemDismiss(2)));
        assert!(bar.focus_nav(-1, 0));
        assert_eq!(bar.focused, Some(Slot::NcItem(2)));
        assert_eq!(bar.focused_action(), Some(TopbarAction::ActivateNotification(2)));
    }

    #[test]
    fn the_focus_ring_is_reported_in_popup_local_coordinates() {
        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        let popup = bar.popup_rect().expect("popup rect");
        let slot = bar.slot_rect(Slot::QsWifi).expect("wifi slot");
        let ring = bar.focus_ring_in(Slot::in_quick_settings).expect("ring for the focused tile");
        assert_eq!(ring.loc, slot.loc - popup.loc);
        assert_eq!(ring.size, slot.size);
        assert!(ring.loc.x >= FOCUS_RING);
        assert!(ring.loc.y >= FOCUS_RING);
        assert!(ring.loc.x + ring.size.w + FOCUS_RING <= popup.size.w);
        assert!(ring.loc.y + ring.size.h + FOCUS_RING <= popup.size.h);
        assert_eq!(bar.focus_ring_in(Slot::in_notification_center), None);
    }

    #[test]
    fn hover_and_focus_are_independent_states() {
        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        let tile = centre(bar.slot_rect(Slot::QsBluetooth).expect("bluetooth tile"));
        assert!(bar.set_hover(Some(tile)));
        assert_eq!(bar.hovered_in(Slot::in_quick_settings), Some(Slot::QsBluetooth));
        assert_eq!(bar.focused_in(Slot::in_quick_settings), Some(Slot::QsWifi));
    }

    const _: () = assert!(FOCUS_RING == 2 && FOCUS_RING_STROKE == 2.0);

    #[test]
    fn popup_lifecycle_executes_full_opening_and_closing_animations() {
        for popup in [OpenPopup::LanguageMenu, OpenPopup::NotificationCenter, OpenPopup::QuickSettings] {
            let mut bar = Topbar::new(TopbarTheme::default());
            assert_eq!(bar.open_popup(), None);
            assert_eq!(bar.shown_popup(), None);

            bar.toggle_popup(popup);
            assert_eq!(bar.open_popup(), Some(popup));
            assert_eq!(bar.shown_popup(), Some(popup));
            assert_eq!(bar.closing_popup, None);
            let p_open = bar.popup_progress();
            assert!((0.0..=1.0).contains(&p_open));

            let closed = bar.close_popup();
            assert!(closed);
            assert_eq!(bar.open_popup(), None);
            assert_eq!(bar.closing_popup, Some(popup));
            assert_eq!(bar.shown_popup(), Some(popup));
            let p_close = bar.popup_progress();
            assert!((0.0..=1.0).contains(&p_close));

            bar.settle_motion();
            assert_eq!(bar.closing_popup, Some(popup));
            assert_eq!(bar.shown_popup(), Some(popup));

            std::thread::sleep(motion::t2() + Duration::from_millis(20));
            bar.settle_motion();
            assert_eq!(bar.closing_popup, None);
            assert_eq!(bar.shown_popup(), None);
            assert_eq!(bar.open_popup(), None);
        }
    }

    #[test]
    fn cache_key_resolution_properly_invalidates_stale_entries() {
        let mut cache = Cache::default();
        let draws = std::cell::Cell::new(0usize);
        let draw = || {
            draws.set(draws.get() + 1);
            let buf = canvas(10, 10, 1)?;
            Some(buffer_from(&buf, 10, 10, 1))
        };
        for (key, total) in [
            ((1u64, 0usize, None, false), 1),
            ((1, 0, None, false), 1),
            ((2, 0, None, false), 2),
            ((2, 1, None, false), 3),
            ((2, 1, Some(Slot::LayoutDropdown), false), 4),
            ((2, 1, Some(Slot::LayoutDropdown), true), 5),
        ] {
            assert!(cache.resolve(key, 1, draw).is_some());
            assert_eq!(draws.get(), total, "{key:?}");
        }

        let mut by_scale = Cache::default();
        draws.set(0);
        for (key, scale, total) in [(10u64, 1, 1), (10, 2, 2), (10, 1, 2), (10, 2, 2), (11, 1, 3)] {
            let _ = by_scale.resolve(key, scale, draw);
            assert_eq!(draws.get(), total, "{key} @ {scale}");
        }
    }

    #[test]
    fn system_status_and_layout_changes_advance_revision_counters() {
        let mut bar = Topbar::new(TopbarTheme::default());
        let s_rev0 = bar.status_rev;
        let status = crate::system_info::SystemStatus { volume_pct: 75, ..Default::default() };
        assert!(bar.set_system_status(status));
        assert_eq!(bar.status_rev, s_rev0.wrapping_add(1));

        let l_rev0 = bar.layouts_rev;
        assert!(bar.set_layouts(vec!["US".to_string(), "UA".to_string()], 0));
        assert_eq!(bar.layouts_rev, l_rev0.wrapping_add(1));
    }

    #[test]
    fn a_closing_popup_keeps_the_pump_alive_until_settle_retires_it() {
        let mut bar = quick_settings();
        assert!(bar.close_popup());
        assert!(bar.is_animating());
        std::thread::sleep(motion::t2() + Duration::from_millis(20));
        assert!(bar.is_animating(), "the faded popup still needs one settle pass to be retired");
        bar.settle_motion();
        assert_eq!(bar.closing_popup, None);
        assert!(!bar.is_animating());
    }

    #[test]
    fn leaving_a_control_early_fades_the_hover_layer_from_where_it_got_to() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_hover(Some(centre(bar.slot_rect(Slot::Caffeine).expect("caffeine"))));
        std::thread::sleep(Duration::from_millis(40));
        let partway = bar.hover_layers[&Slot::Caffeine].value();
        assert!(partway > 0.0 && partway < 1.0, "partway was {partway}");

        bar.set_hover(None);
        let reversed = bar.hover_layers[&Slot::Caffeine].value();
        assert!((reversed - partway).abs() < 0.1, "no flash: {partway} vs {reversed}");
        assert!(bar.is_animating());

        std::thread::sleep(motion::t1() + Duration::from_millis(20));
        bar.prune_layers();
        assert!(bar.hover_layers.is_empty());
    }

    #[test]
    fn a_press_layer_lands_at_once_and_fades_on_release() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(bar.set_pressed(Some(Slot::Caffeine)));
        assert!((bar.press_layers[&Slot::Caffeine].value() - 1.0).abs() < f64::EPSILON);
        assert!(bar.set_pressed(None));
        assert!(bar.press_layers[&Slot::Caffeine].is_animating());
        assert!((bar.press_layers[&Slot::Caffeine].target()).abs() < f64::EPSILON);
    }

    #[test]
    fn the_badge_pops_in_from_six_tenths_and_holds_its_room_while_fading_out() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(bar.set_unread(3));
        assert!(bar.badge_anim.is_animating());
        assert!((bar.badge_anim.target() - 1.0).abs() < f64::EPSILON);
        assert_eq!(bar.badge_shown(), 3);
        let start = BadgePop { count: 3, pop: 0.0 };
        assert!((start.scale() - BADGE_SCALE_FROM).abs() < f64::EPSILON);
        assert!(!start.visible());
        assert!((BadgePop { count: 3, pop: 1.0 }.scale() - 1.0).abs() < f64::EPSILON);

        std::thread::sleep(motion::t2() + Duration::from_millis(20));
        assert!(bar.set_unread(0));
        assert_eq!(bar.badge_shown(), 3, "the pill keeps its width while it shrinks");
        assert!(bar.is_animating());

        std::thread::sleep(motion::t1() + Duration::from_millis(20));
        bar.settle_motion();
        assert_eq!(bar.badge_shown(), 0);
        assert!(!bar.is_animating());
    }

    #[test]
    fn picking_the_other_segment_slides_the_pill_instead_of_jumping() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(bar.set_layouts(vec!["US".to_string(), "UA".to_string()], 1));
        assert!(bar.segment_anim.is_animating());
        assert!((bar.segment_anim.target() - 1.0).abs() < f64::EPSILON);

        assert!(bar.set_layouts(vec!["US".to_string()], 0));
        assert!(!bar.segment_anim.is_animating(), "a new layout set jumps");

        assert_eq!(segment_pill((10, 20), (40, 30), 0.0), (10, 20));
        assert_eq!(segment_pill((10, 20), (40, 30), 0.5), (25, 25));
        assert_eq!(segment_pill((10, 20), (40, 30), 1.0), (40, 30));
        assert_eq!(segment_pill((10, 20), (40, 30), 1.4), (40, 30));
    }

    #[test]
    fn the_switch_thumb_travels_between_the_insets_and_never_leaves_the_track() {
        let off = switch_thumb_x(100, SWITCH_WIDTH, SWITCH_INSET, SWITCH_THUMB, 0.0);
        let on = switch_thumb_x(100, SWITCH_WIDTH, SWITCH_INSET, SWITCH_THUMB, 1.0);
        assert_eq!(off, 100 + SWITCH_INSET);
        assert_eq!(on, 100 + SWITCH_WIDTH - SWITCH_INSET - SWITCH_THUMB);
        let overshoot = switch_thumb_x(100, SWITCH_WIDTH, SWITCH_INSET, SWITCH_THUMB, 1.3);
        assert!(overshoot >= on);
        assert!(overshoot <= 100 + SWITCH_WIDTH - SWITCH_THUMB);
        let undershoot = switch_thumb_x(100, SWITCH_WIDTH, SWITCH_INSET, SWITCH_THUMB, -0.3);
        assert!(undershoot >= 100);
    }

    #[test]
    fn toggling_caffeine_drives_thumb_and_track_from_the_same_anim() {
        let mut bar = topbar_with(&["US", "UA"]);
        let rest = bar.caffeine_anim.switch();
        assert!(rest.thumb.abs() < f64::EPSILON);
        assert!(rest.track.abs() < f64::EPSILON);
        bar.set_caffeinated(true);
        assert!(bar.caffeine_anim.is_animating());
        std::thread::sleep(motion::t2() + Duration::from_millis(20));
        let settled = bar.caffeine_anim.switch();
        assert!((settled.thumb - 1.0).abs() < f64::EPSILON);
        assert!((settled.track - 1.0).abs() < f64::EPSILON);
        let on = settled.track_colour(bar.theme.track, bar.theme.active_background);
        for (got, want) in on.0.iter().zip(bar.theme.active_background.0) {
            assert!((got - want).abs() < 1e-5, "{on:?}");
        }
    }

    #[test]
    fn quantised_motion_keys_reuse_one_buffer_across_nearby_frames() {
        assert_eq!(quantize(0.51, BADGE_QUANTUM), quantize(0.53, BADGE_QUANTUM));
        assert_ne!(quantize(0.0, BADGE_QUANTUM), quantize(1.0, BADGE_QUANTUM));
        assert_eq!(quantize(9.0, BADGE_QUANTUM), quantize(2.0, BADGE_QUANTUM));

        let mut cache = Cache::default();
        let draws = std::cell::Cell::new(0usize);
        let draw = || {
            draws.set(draws.get() + 1);
            let buf = canvas(4, 4, 1)?;
            Some(buffer_from(&buf, 4, 4, 1))
        };
        for pop in [0.50, 0.51, 0.52, 0.53] {
            let _ = cache.resolve(quantize(pop, BADGE_QUANTUM), 1, draw);
        }
        assert_eq!(draws.get(), 1);
        let _ = cache.resolve(quantize(0.75, BADGE_QUANTUM), 1, draw);
        assert_eq!(draws.get(), 2);
    }

    #[test]
    fn state_layer_buffers_are_keyed_by_kind_size_radius_and_scale() {
        let mut bar = topbar_with(&["US", "UA"]);
        let size = Size::from((40, 24));
        assert!(bar.layer_buffer(Layer::Hover, size, glide_theme::shape::R2, 1).is_some());
        assert_eq!(bar.layers.len(), 1);
        let _ = bar.layer_buffer(Layer::Hover, size, glide_theme::shape::R2, 1);
        assert_eq!(bar.layers.len(), 1);
        let _ = bar.layer_buffer(Layer::Press, size, glide_theme::shape::R2, 1);
        let _ = bar.layer_buffer(Layer::Hover, size, glide_theme::shape::R1, 1);
        let _ = bar.layer_buffer(Layer::Hover, size, glide_theme::shape::R2, 2);
        let _ = bar.layer_buffer(Layer::Focus, size, glide_theme::shape::R2, 1);
        assert_eq!(bar.layers.len(), 5);
        bar.reconfigure(TopbarTheme { dark: false, ..TopbarTheme::default() });
        assert!(bar.layers.is_empty(), "a theme change drops every layer");
    }

    #[test]
    fn hovering_a_tile_raises_a_layer_job_over_that_tile_only() {
        let mut bar = quick_settings();
        let tile = bar.slot_rect(Slot::QsWifi).expect("wifi tile");
        bar.set_hover(Some(centre(tile)));
        std::thread::sleep(motion::t1() + Duration::from_millis(20));
        let jobs = bar.layer_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].layer, Layer::Hover);
        assert_eq!(jobs[0].rect, tile);
        assert!(jobs[0].in_popup);
        assert!((jobs[0].alpha - 1.0).abs() < f32::EPSILON);

        bar.focus_nav(1, 0);
        std::thread::sleep(motion::t1() + Duration::from_millis(20));
        let jobs = bar.layer_jobs();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[1].layer, Layer::Focus);
        assert_eq!(jobs[1].rect, focus_ring_bounds(tile));
    }

    #[test]
    fn the_focus_ring_fades_out_where_it_last_was() {
        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        bar.settle_motion();
        let ring = bar.focus_ring_last.expect("ring recorded");
        assert_eq!(Some(ring), bar.slot_rect(Slot::QsWifi));
        bar.set_focused(None);
        assert!(bar.focus_anim.is_animating());
        assert_eq!(bar.focus_ring_rect(), None);
        let jobs = bar.layer_jobs();
        assert!(jobs.iter().any(|job| job.layer == Layer::Focus && job.rect == focus_ring_bounds(ring)));
    }

    #[test]
    fn slots_without_a_state_layer_never_get_a_hover_anim() {
        let mut bar = quick_settings();
        let track = centre(bar.slot_rect(Slot::QsVolumeTrack).expect("volume track"));
        assert!(bar.set_hover(Some(track)));
        assert_eq!(bar.hovered, Some(Slot::QsVolumeTrack));
        assert!(bar.hover_layers.is_empty());
    }

    #[test]
    fn a_disabled_topbar_never_asks_for_frames() {
        let mut bar = Topbar::new(TopbarTheme { enabled: false, ..TopbarTheme::default() });
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.close_popup();
        assert!(!bar.is_animating());
    }

    #[test]
    fn hovering_a_power_button_captions_it_in_the_footer() {
        let mut bar = quick_settings();
        let restart = Slot::QsPowerButton(PowerAction::Restart);
        bar.set_hover(Some(centre(bar.slot_rect(restart).expect("restart"))));
        assert_eq!(bar.hovered, Some(Slot::QsPowerButton(PowerAction::Restart)));
        assert_eq!(panels::power_label(PowerAction::Restart), crate::i18n::Key::CommandRestart);
    }
}
