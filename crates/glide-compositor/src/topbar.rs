use std::collections::HashMap;
use std::time::Duration;

use glide_theme::TextStyle;
use glide_theme::motion::{self, CubicBezier, E_IO, E_OUT, E_OVER, ease_io, ease_out, lerp};
use glide_theme::shape::{R_ROUND, R1, R2, R3, R4};
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
use crate::power_dialog::is_destructive;
use crate::raster::{Canvas, ShellRenderer};
use crate::system_info::PowerProfile;

#[path = "topbar_panels.rs"]
mod panels;

#[path = "topbar_calendar.rs"]
pub mod calendar;

#[path = "topbar_island.rs"]
mod island;

#[path = "topbar_glyphs.rs"]
mod glyphs;

pub use island::IslandAlert;

#[path = "topbar_status.rs"]
mod status;

#[path = "topbar_tray.rs"]
mod tray;

pub use tray::{ICON as TRAY_ICON, IconArt, MAX_SCALE as TRAY_MAX_SCALE, TrayButton, TrayClick, TrayIcon};

pub use panels::{
    Detail, FieldKey, NetbirdPending, NetbirdRow, NetbirdUpdate, PanelAction, PromptOutcome, QsKeyMode, RowTarget,
    field_key,
};

pub const HEIGHT: i32 = 34;

const PAD_X: i32 = glide_theme::spacing::SP2;
const BUTTON_HEIGHT: i32 = 24;
const BUTTON_PAD_X: i32 = glide_theme::spacing::SP2;
const ITEM_GAP: i32 = 2;
const BUTTON_RADIUS: f32 = glide_theme::shape::R2;
const BUTTON_MIN_WIDTH: i32 = 28;

const APP_ICON: i32 = 14;
const APP_PAD_LEFT: i32 = 5;
const APP_PAD_RIGHT: i32 = 9;
const APP_GAP: i32 = 7;
const APP_STYLE: TextStyle = TextStyle {
    size_px: TextStyle::TOPBAR.size_px,
    line_height_px: TextStyle::TOPBAR.line_height_px,
    ..TextStyle::HEAD
};
const APP_NAME_MAX: usize = 36;

const SEG_HEIGHT: i32 = 20;
const SEG_INSET: i32 = 2;
const SEG_GAP: i32 = 2;
const SEG_MIN_WIDTH: i32 = 25;
const SEG_RADIUS: f32 = glide_theme::shape::R1;
const SEG_PAD_X: i32 = 3;
const SEG_TEXT_PAD: i32 = 5;
const SEG_LIFT_DARK: f32 = 0.35;
const SEG_LIFT_LIGHT: f32 = 0.08;
const SEG_STYLE: TextStyle =
    TextStyle { size_px: 10.5, line_height_px: 14.0, weight: 500, tracking_em: 0.03, ..TextStyle::CAP };

const SWITCH_WIDTH: i32 = 28;
const SWITCH_HEIGHT: i32 = 16;
const SWITCH_THUMB: i32 = 12;
const SWITCH_INSET: i32 = 2;

const COFFEE_GLYPH: i32 = status::ICON;
const COFFEE_PAD: i32 = 6;
const COFFEE_GAP: i32 = 7;
const COFFEE_PAD_END: i32 = 6;

const LANG_MENU_WIDTH: i32 = 196;
const LANG_MENU_PAD: i32 = glide_theme::spacing::SP1;
const LANG_MENU_ITEM_HEIGHT: i32 = 34;
const LANG_MENU_DIVIDER: i32 = glide_theme::spacing::SP1 * 2 + 1;

const STATUS_SEP_WIDTH: i32 = 13;

const QS_WIDTH: i32 = 352;
const QS_PAD: i32 = glide_theme::spacing::SP3;
const QS_GAP: i32 = glide_theme::spacing::SP3;
const QS_GRID_GAP: i32 = glide_theme::spacing::SP2;
const QS_RADIUS: f32 = R4;
const QS_HEADER: i32 = 28;
const QS_AVATAR: i32 = 28;
const QS_TILE_HEIGHT: i32 = 52;
const QS_TILE_INSET: i32 = 11;
const QS_SPLIT_INSET: i32 = 9;
const QS_SPLIT_GAP: i32 = 7;
const QS_SPLIT_END: i32 = 7;
const QS_CHEVRON: i32 = 22;
const QS_ICON_BADGE: i32 = 28;
const QS_CARD_PAD_X: i32 = glide_theme::spacing::SP3;
const QS_CARD_PAD_Y: i32 = glide_theme::spacing::SP2;
const QS_SLIDER_HEAD: i32 = 24;
const QS_SLIDER_HEIGHT: i32 = 22;
const QS_SLIDER_TRACK_H: i32 = 6;
const QS_SLIDER_THUMB: i32 = 16;
const QS_SLIDER_ICON: i32 = 24;
const QS_READOUT_W: i32 = 34;
const QS_PILL_PAD: i32 = glide_theme::spacing::SP2;
const QS_PILL_CHEVRON: i32 = 9;
const QS_PILL_MAX: i32 = 175;
const QS_BATTERY_HEAD: i32 = 40;
const QS_SEGMENTS_H: i32 = 34;
const QS_SEGMENT_INSET: i32 = 3;
const QS_BATTERY_GLYPH_W: i32 = 24;
const QS_BATTERY_GLYPH_H: i32 = 13;
const QS_POWER_BTN: i32 = 36;
const QS_POWER_ROW: i32 = 40;
const QS_CONFIRM_PAD: i32 = 10;
const QS_CONFIRM_BADGE: i32 = 24;
const QS_CONFIRM_NOTE: i32 = 14;
const QS_CONFIRM_PILL: i32 = 28;
const QS_CONFIRM_PILL_PAD: i32 = glide_theme::spacing::SP3;
const QS_CONFIRM_PILL_GAP: i32 = 6;
const QS_CONFIRM_PILLS_TOP: i32 = 2;
const QS_ARM_RISE: f64 = 6.0;
const QS_CHEVRON_NUDGE: i32 = 2;
const QS_DRILL: i32 = 22;
const QS_DRAWER_TOP: i32 = SP1 * 2 + 1;
const QS_DRAWER_ITEM: i32 = 36;
const QS_DRAWER_GAP: i32 = 2;
const QS_DRAWER_BOTTOM: i32 = 2;
const QS_DRAWER_ICON: i32 = 22;
const QS_DRAWER_TICK: i32 = 12;

const POPUP_GAP: i32 = glide_theme::spacing::SP2;
const SHADOW_CACHE_LIMIT: usize = 48;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Drawer {
    Output,
    Input,
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
    DismissIslandAlert,
    QsToggleWifi,
    QsToggleBluetooth,
    QsToggleNetbird,
    QsToggleNightLight,
    QsToggleAirplane,
    QsToggleDarkTheme,
    QsSetPowerProfile(PowerProfile),
    QsVolumeMute,
    QsVolumeAdjust(u8),
    QsMicMute,
    QsMicAdjust(u8),
    QsToggleDrawer(Drawer),
    QsUseDevice(Drawer, u16),
    QsBrightnessAdjust(u8),
    QsScreenshot,
    QsOpenSettings,
    QsArmPower(PowerAction),
    QsDisarmPower,
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
    IslandMedia,
    IslandAlert,
    IslandAlertClose,
    IslandToMedia,
    IslandToNotifications,
    IslandCollapse,
    PopMediaPrev,
    PopMediaPlay,
    PopMediaNext,
    PopMediaSeek,
    LayoutCycle,
    LayoutPick(usize),
    LayoutDropdown,
    Caffeine,
    Status,
    LangMenuItem(usize),
    LangMenuSettings,
    NcClearAll,
    NcDndToggle,
    NcItem(u32),
    NcItemDismiss(u32),
    QsWifi,
    QsBluetooth,
    QsNetbird,
    QsNightLight,
    QsScreenshot,
    QsAirplane,
    QsDnd,
    QsDarkTheme,
    QsCaffeine,
    QsVolumeMute,
    QsVolumeTrack,
    QsMicMute,
    QsMicTrack,
    QsMicMore,
    QsOutputPill,
    QsInputPill,
    QsDrawerItem(Drawer, u16),
    QsBrightnessTrack,
    QsProfile(PowerProfile),
    QsSettings,
    QsPowerButton(PowerAction),
    QsPowerCancel,
    QsPowerConfirm(PowerAction),
    QsWifiMore,
    QsBluetoothMore,
    QsNetbirdMore,
    QsVolumeMore,
    QsBattery,
    QsBack,
    QsDetailToggle,
    QsDetailRow(u16),
    QsPasswordField,
    QsPromptReveal,
    QsPromptCancel,
    QsPromptConnect,
    QsChargeLimit,
    QsDetailSettings,
    Search,
    Privacy,
    CalMonth,
    CalPrev,
    CalNext,
    Tray(u16),
}

impl Slot {
    #[must_use]
    pub const fn is_slider(self) -> bool {
        matches!(self, Self::QsVolumeTrack | Self::QsMicTrack | Self::QsBrightnessTrack)
    }

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
                    | Self::IslandMedia
                    | Self::IslandAlert
                    | Self::IslandAlertClose
                    | Self::LayoutCycle
                    | Self::LayoutPick(_)
                    | Self::LayoutDropdown
                    | Self::Caffeine
                    | Self::Tray(_)
            )
    }

    const fn in_media_popup(self) -> bool {
        matches!(
            self,
            Self::PopMediaPrev
                | Self::PopMediaPlay
                | Self::PopMediaNext
                | Self::PopMediaSeek
                | Self::IslandToNotifications
                | Self::IslandCollapse
        )
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
                | Self::IslandToMedia
                | Self::IslandCollapse
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
            | Self::QsMicTrack
            | Self::QsBrightnessTrack
            | Self::QsDetailToggle
            | Self::QsPasswordField
            | Self::IslandAlert
            | Self::PopMediaSeek => None,
            Self::Clock
            | Self::IslandMedia
            | Self::IslandAlertClose
            | Self::IslandToMedia
            | Self::IslandToNotifications
            | Self::IslandCollapse
            | Self::NcClearAll
            | Self::CalMonth
            | Self::CalPrev
            | Self::CalNext
            | Self::QsPromptReveal => Some(glide_theme::shape::R1),
            Self::LayoutPick(_) | Self::QsProfile(_) => Some(SEG_RADIUS),
            Self::PopMediaPlay
            | Self::NcItemDismiss(_)
            | Self::QsPowerButton(_)
            | Self::QsPowerCancel
            | Self::QsPowerConfirm(_)
            | Self::QsPromptCancel
            | Self::QsPromptConnect
            | Self::QsWifi
            | Self::QsBluetooth
            | Self::QsNetbird => Some(glide_theme::shape::R_ROUND),
            _ => Some(glide_theme::shape::R2),
        }
    }

    const fn action(self) -> TopbarAction {
        match self {
            Self::MenuTitle(index) => TopbarAction::OpenAppMenu(index),
            Self::MenuRow(path) => TopbarAction::AppMenuRow(path),
            Self::Clock | Self::IslandAlert | Self::IslandToNotifications => TopbarAction::ToggleNotificationCenter,
            Self::IslandMedia | Self::IslandToMedia => TopbarAction::ToggleMediaPlayer,
            Self::IslandAlertClose => TopbarAction::DismissIslandAlert,
            Self::IslandCollapse => TopbarAction::ClosePopup,
            Self::PopMediaPlay => TopbarAction::MediaPlayPause,
            Self::PopMediaPrev => TopbarAction::MediaPrevious,
            Self::PopMediaNext => TopbarAction::MediaNext,
            Self::PopMediaSeek => TopbarAction::MediaSeek(0),
            Self::LayoutCycle => TopbarAction::CycleLayout,
            Self::LayoutPick(index) | Self::LangMenuItem(index) => TopbarAction::SelectLayout(index),
            Self::LayoutDropdown => TopbarAction::ToggleLanguageMenu,
            Self::Caffeine | Self::QsCaffeine => TopbarAction::ToggleCaffeine,
            Self::NcDndToggle | Self::QsDnd => TopbarAction::ToggleDoNotDisturb,
            Self::Status => TopbarAction::ToggleQuickSettings,
            Self::LangMenuSettings => TopbarAction::OpenInputSettings,
            Self::NcClearAll => TopbarAction::ClearAllNotifications,
            Self::NcItem(id) => TopbarAction::ActivateNotification(id),
            Self::NcItemDismiss(id) => TopbarAction::DismissNotification(id),
            Self::QsWifi => TopbarAction::QsToggleWifi,
            Self::QsBluetooth => TopbarAction::QsToggleBluetooth,
            Self::QsNetbird => TopbarAction::QsToggleNetbird,
            Self::QsNightLight => TopbarAction::QsToggleNightLight,
            Self::QsScreenshot => TopbarAction::QsScreenshot,
            Self::QsAirplane => TopbarAction::QsToggleAirplane,
            Self::QsDarkTheme => TopbarAction::QsToggleDarkTheme,
            Self::QsVolumeMute => TopbarAction::QsVolumeMute,
            Self::QsVolumeTrack => TopbarAction::QsVolumeAdjust(50),
            Self::QsMicMute => TopbarAction::QsMicMute,
            Self::QsMicTrack => TopbarAction::QsMicAdjust(50),
            Self::QsOutputPill => TopbarAction::QsToggleDrawer(Drawer::Output),
            Self::QsInputPill => TopbarAction::QsToggleDrawer(Drawer::Input),
            Self::QsDrawerItem(drawer, index) => TopbarAction::QsUseDevice(drawer, index),
            Self::QsBrightnessTrack => TopbarAction::QsBrightnessAdjust(50),
            Self::QsProfile(profile) => TopbarAction::QsSetPowerProfile(profile),
            Self::QsSettings => TopbarAction::QsOpenSettings,
            Self::QsPowerButton(action) => TopbarAction::QsArmPower(action),
            Self::QsPowerCancel => TopbarAction::QsDisarmPower,
            Self::QsPowerConfirm(action) => TopbarAction::QsPower(action),
            Self::QsWifiMore => TopbarAction::QsPanel(PanelAction::Open(Detail::Wifi)),
            Self::QsBluetoothMore => TopbarAction::QsPanel(PanelAction::Open(Detail::Bluetooth)),
            Self::QsNetbirdMore => TopbarAction::QsPanel(PanelAction::Open(Detail::Netbird)),
            Self::QsVolumeMore | Self::QsMicMore => TopbarAction::QsPanel(PanelAction::Open(Detail::Sound)),
            Self::QsBattery => TopbarAction::QsPanel(PanelAction::Open(Detail::Battery)),
            Self::QsBack => TopbarAction::QsPanel(PanelAction::Back),
            Self::QsDetailToggle => TopbarAction::QsPanel(PanelAction::Toggle),
            Self::QsDetailRow(index) => TopbarAction::QsPanel(PanelAction::Row(index)),
            Self::QsPasswordField => TopbarAction::QsPanel(PanelAction::FocusPassword),
            Self::QsPromptReveal => TopbarAction::QsPanel(PanelAction::PromptReveal),
            Self::QsChargeLimit => TopbarAction::QsPanel(PanelAction::ChargeLimit),
            Self::QsPromptCancel => TopbarAction::QsPanel(PanelAction::PromptCancel),
            Self::QsPromptConnect => TopbarAction::QsPanel(PanelAction::PromptSubmit),
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
type QsCacheKey = ((u64, u64, String), QsFlags, QsMotion, Option<Slot>, QsShape);
type QsPowerKey = (Option<Slot>, Option<(PowerAction, Option<u64>)>, (i32, i32));
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
    mic_anim: Anim,
    brightness_anim: Anim,
    seek_anim: Anim,
    badge_anim: Anim,
    badge: Badge,
    segment_anim: Anim,
    profile_anim: Anim,
    qs_power: Option<ArmedPower>,
    arm_anim: Anim,
    qs_drawer: Option<Drawer>,
    drawer_shown: Option<Drawer>,
    drawer_anim: Anim,
    drawer_extent: Option<(Drawer, i32)>,
    slider_dragging: bool,
    scroll: crate::system_info::ScrollAccumulator,

    rev: u64,
    layouts_rev: u64,
    media_rev: u64,
    track_rev: u64,
    status_rev: u64,
    detail_rev: u64,
    nc_rev: u64,

    slots: Vec<(Slot, Rectangle<i32, Logical>)>,
    shown_popup_slots: Vec<(Slot, Rectangle<i32, Logical>)>,
    layout_key: Option<(u64, Rectangle<i32, Logical>)>,
    popup_rect: Option<Rectangle<i32, Logical>>,
    panel: std::collections::HashMap<PanelKey, MemoryRenderBuffer>,
    shadows: std::collections::HashMap<(i32, i32, i32, u32), MemoryRenderBuffer>,
    layers: HashMap<LayerKey, MemoryRenderBuffer>,
    separator_cache: Cache<()>,
    app_cache: Cache<(String, String)>,
    layout_cache: Cache<(u64, usize, Option<Slot>, bool, u8)>,
    coffee_cache: Cache<(bool, bool, (u8, u8))>,
    qs_cache: Cache<QsCacheKey>,
    qs_row_cache: Cache<QsPowerKey>,
    qs_confirm_cache: Cache<QsPowerKey>,
    qs_bg_cache: Cache<(i32, i32)>,
    qs_detail_cache: Cache<(u64, u64, Option<Slot>, Detail)>,
    lang_menu_cache: Cache<(u64, usize, Option<Slot>)>,
    raster_rev: u64,

    menu_bar: Vec<glide_appmenu::MenuItem>,
    menu_open: MenuPath,
    menu_panels: Vec<crate::global_menu::Panel>,
    menu_titles: HashMap<(usize, bool, bool, i32), MemoryRenderBuffer>,
    menu_rasters: HashMap<(MenuPath, Option<usize>, i32), MemoryRenderBuffer>,
    app_chip_w: i32,

    calendar: calendar::CalendarView,
    island: island::Island,
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
        bar.mic_anim.jump(f64::from(bar.system_status.mic_pct));
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
        self.layout_cache.clear();
        self.coffee_cache.clear();
        self.qs_cache.clear();
        self.qs_row_cache.clear();
        self.qs_confirm_cache.clear();
        self.qs_bg_cache.clear();
        self.qs_detail_cache.clear();
        self.lang_menu_cache.clear();
        self.menu_titles.clear();
        self.menu_rasters.clear();
        self.calendar.clear();
        self.island.clear();
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
        let popup = self.popup_reveal.is_animating(motion::t2());
        let layers = self.hover_layers.values().chain(self.press_layers.values()).any(Anim::is_animating);
        let controls = [
            &self.focus_anim,
            &self.caffeine_anim,
            &self.dnd_anim,
            &self.volume_anim,
            &self.mic_anim,
            &self.brightness_anim,
            &self.seek_anim,
            &self.badge_anim,
            &self.segment_anim,
            &self.profile_anim,
            &self.arm_anim,
            &self.drawer_anim,
        ]
        .into_iter()
        .any(Anim::is_animating);
        let detail = self.qs_panel.is_animating();
        let extras = self.calendar.is_animating() || self.indicators.is_animating() || self.island.is_animating();
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

    pub const fn keep_dragged_levels(&self, sample: &mut crate::system_info::SystemStatus) {
        if !self.slider_dragging {
            return;
        }
        let shown = &self.system_status;
        (sample.volume_pct, sample.volume_muted) = (shown.volume_pct, shown.volume_muted);
        (sample.mic_pct, sample.mic_muted) = (shown.mic_pct, shown.mic_muted);
        sample.brightness_pct = shown.brightness_pct;
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
            self.forget_power();
            self.forget_drawer();
            self.touch();
        }
        if self.qs_drawer.is_none() && self.drawer_shown.is_some() && !self.drawer_anim.is_animating() {
            self.drawer_shown = None;
        }
        let extent = self.drawer_extent_now();
        if extent != self.drawer_extent {
            self.drawer_extent = extent;
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
            Slot::QsMicTrack => self.system_status.mic_pct,
            Slot::QsBrightnessTrack => self.system_status.brightness_pct,
            _ => return None,
        };
        let pct = u8::try_from((i32::from(current) + i32::from(delta)).clamp(0, 100)).ok()?;
        let action = match slot {
            Slot::QsVolumeTrack => {
                self.system_status.volume_pct = pct;
                self.slide(|bar| &mut bar.volume_anim, pct);
                TopbarAction::QsVolumeAdjust(pct)
            }
            Slot::QsMicTrack => {
                self.system_status.mic_pct = pct;
                self.slide(|bar| &mut bar.mic_anim, pct);
                TopbarAction::QsMicAdjust(pct)
            }
            _ => {
                self.system_status.brightness_pct = pct;
                self.slide(|bar| &mut bar.brightness_anim, pct);
                TopbarAction::QsBrightnessAdjust(pct)
            }
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
                        let beneath = slot.is_slider() && spans_overlap(rect.loc.x, rect.size.w, origin.x, 1);
                        let (along, across, in_lane) = if dx == 0 {
                            (
                                step_y * dy.signum(),
                                if beneath { 0 } else { step_x },
                                spans_overlap(from.loc.x, from.size.w, rect.loc.x, rect.size.w),
                            )
                        } else {
                            (
                                step_x * dx.signum(),
                                step_y,
                                spans_overlap(from.loc.y, from.size.h, rect.loc.y, rect.size.h),
                            )
                        };
                        (along > 0 && in_lane).then_some((along + across.abs(), *slot))
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
        self.sync_island();
        self.touch();
        true
    }

    pub fn toggle_popup(&mut self, popup: OpenPopup) -> bool {
        if self.open_popup == Some(popup) {
            return self.close_popup();
        }
        if popup == OpenPopup::MediaPlayer && self.media.is_none() {
            return false;
        }
        self.open_popup = Some(popup);
        self.closing_popup = None;
        self.popup_reveal.open(motion::t2());
        self.set_focused(None);
        self.qs_panel.reset();
        self.forget_power();
        self.forget_drawer();
        if popup == OpenPopup::NotificationCenter {
            self.calendar.reset();
            self.reset_island_scroll();
            self.island_forget_alert();
        }
        self.sync_island();
        self.touch();
        true
    }

    fn forget_power(&mut self) {
        self.qs_power = None;
        self.arm_anim.jump(0.0);
    }

    fn qs_shape(&self, painter: Option<&crate::paint::Painter>) -> QsShape {
        use crate::i18n::{Key, t};
        let armed = self.qs_power.map(|armed| armed.action);
        let pills = painter.map_or_else(QsPills::default, |painter| {
            let (cancel, confirm) = armed.map_or((0, 0), |action| {
                let confirm = t(crate::power_dialog::confirm_key(action));
                (qs_confirm_pill_width(painter, t(Key::PowerCancel)), qs_confirm_pill_width(painter, confirm))
            });
            let output = qs_pill_width(painter, &self.output_label());
            let input = qs_pill_width(painter, &self.input_label());
            QsPills { output, input, cancel, confirm }
        });
        QsShape {
            mic: self.system_status.mic_available,
            battery: self.system_status.battery_pct.is_some(),
            profile: self.system_status.power_profile.is_some(),
            netbird: self.qs_panel.netbird.installed,
            armed,
            drawer: self.drawer_extent_now(),
            pills,
        }
    }

    fn input_label(&self) -> String {
        self.qs_panel.active_input().map_or_else(
            || crate::i18n::t(crate::i18n::Key::TbMicrophone).to_string(),
            |input| input.description.clone(),
        )
    }

    fn qs_main_key(&self, painter: &crate::paint::Painter) -> QsCacheKey {
        let main_view = self.qs_panel.view.is_none();
        let hovered = self.hovered_in(Slot::in_quick_settings).filter(|slot| {
            main_view && !matches!(slot, Slot::QsPowerButton(_) | Slot::QsPowerCancel | Slot::QsPowerConfirm(_))
        });
        let dragging = self.pressed.filter(|slot| self.slider_dragging && slot.is_slider());
        let flags = QsFlags {
            caffeinated: self.caffeinated,
            dnd: self.dnd,
            dark: self.theme.dark,
            dragging,
            drawer: self.qs_drawer,
        };
        let motion = QsMotion {
            volume: percent(self.volume_anim.value()),
            mic: percent(self.mic_anim.value()),
            brightness: percent(self.brightness_anim.value()),
            profile: quantize(self.profile_anim.value(), MOTION_QUANTUM),
        };
        let battery = self.system_status.battery_pct.map(|_| panels::battery_head_line(&self.system_status));
        let revs = (self.status_rev, self.qs_panel.rev, battery.unwrap_or_default());
        (revs, flags, motion, hovered, self.qs_shape(Some(painter)))
    }

    fn output_label(&self) -> String {
        self.qs_panel.active_output().map_or_else(
            || crate::i18n::t(crate::i18n::Key::SettingsSound).to_string(),
            |output| output.description.clone(),
        )
    }

    #[must_use]
    pub fn armed_power(&self) -> Option<PowerAction> {
        self.qs_power.map(|armed| armed.action)
    }

    pub fn arm_power(&mut self, action: PowerAction) -> bool {
        if self.open_popup != Some(OpenPopup::QuickSettings) || self.armed_power() == Some(action) {
            return false;
        }
        let deadline = is_destructive(action).then(|| std::time::Instant::now() + crate::power_dialog::COUNTDOWN);
        self.qs_power = Some(ArmedPower { action, deadline });
        self.arm_anim.retarget(1.0, motion::t2(), E_OUT);
        if self.focused.is_some() {
            self.set_focused(Some(Slot::QsPowerConfirm(action)));
        }
        self.touch();
        true
    }

    pub fn disarm_power(&mut self) -> bool {
        let Some(armed) = self.qs_power.take() else {
            return false;
        };
        self.arm_anim.retarget(0.0, motion::t2(), E_OUT);
        if self.focused.is_some() {
            self.set_focused(Some(Slot::QsPowerButton(armed.action)));
        }
        self.touch();
        true
    }

    #[must_use]
    pub fn power_counting(&self) -> bool {
        self.open_popup == Some(OpenPopup::QuickSettings) && self.qs_power.is_some_and(|armed| armed.deadline.is_some())
    }

    pub fn power_due(&mut self, now: std::time::Instant) -> Option<PowerAction> {
        if !self.power_counting() {
            return None;
        }
        let armed = self.qs_power?;
        if armed.seconds_left(now)? > 0 {
            return None;
        }
        self.forget_power();
        self.touch();
        Some(armed.action)
    }

    pub fn qs_escape(&mut self) -> bool {
        self.open_popup == Some(OpenPopup::QuickSettings)
            && (self.close_drawer() || self.disarm_power() || self.close_detail())
    }

    fn forget_drawer(&mut self) {
        self.qs_drawer = None;
        self.drawer_shown = None;
        self.drawer_anim.jump(0.0);
    }

    #[must_use]
    pub const fn drawer(&self) -> Option<Drawer> {
        self.qs_drawer
    }

    pub fn toggle_drawer(&mut self, drawer: Drawer) -> bool {
        if self.open_popup != Some(OpenPopup::QuickSettings) || self.qs_panel.view.is_some() {
            return false;
        }
        if self.qs_drawer == Some(drawer) {
            return self.close_drawer();
        }
        if self.drawer_shown != Some(drawer) {
            self.drawer_anim.jump(0.0);
        }
        self.qs_drawer = Some(drawer);
        self.drawer_shown = Some(drawer);
        self.drawer_anim.retarget(1.0, motion::t2(), E_OUT);
        self.touch();
        true
    }

    pub fn close_drawer(&mut self) -> bool {
        let Some(drawer) = self.qs_drawer.take() else {
            return false;
        };
        self.drawer_anim.retarget(0.0, motion::t2(), E_OUT);
        if matches!(self.focused, Some(Slot::QsDrawerItem(..))) {
            self.set_focused(Some(match drawer {
                Drawer::Output => Slot::QsOutputPill,
                Drawer::Input => Slot::QsInputPill,
            }));
        }
        self.touch();
        true
    }

    #[must_use]
    pub fn drawer_device(&self, drawer: Drawer, index: u16) -> Option<crate::qs_net::AudioOutput> {
        self.qs_panel.devices(drawer).get(usize::from(index)).cloned()
    }

    fn drawer_extent_now(&self) -> Option<(Drawer, i32)> {
        let drawer = self.drawer_shown?;
        let full = qs_drawer_height(self.qs_panel.devices(drawer).len());
        let progress = f64::from(quantize(self.drawer_anim.value(), MOTION_QUANTUM)) / MOTION_QUANTUM;
        let visible = (f64::from(full) * progress.clamp(0.0, 1.0)).round() as i32;
        (visible > 0).then_some((drawer, visible))
    }

    pub fn set_system_status(&mut self, status: crate::system_info::SystemStatus) -> bool {
        if self.system_status == status {
            return false;
        }
        let at_a_glance = !self.system_status.looks_like(&status);
        let (volume, mic, brightness) = (status.volume_pct, status.mic_pct, status.brightness_pct);
        match (self.system_status.power_profile, status.power_profile) {
            (Some(before), Some(after)) if before != after => {
                self.profile_anim.retarget(profile_index(after), motion::t2(), E_OUT);
            }
            (None, Some(after)) => self.profile_anim.jump(profile_index(after)),
            _ => {}
        }
        self.system_status = status;
        self.detail_rev = self.detail_rev.wrapping_add(1);
        if !at_a_glance {
            return self.open_popup.is_some();
        }
        self.slide(|bar| &mut bar.volume_anim, volume);
        self.slide(|bar| &mut bar.mic_anim, mic);
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
        let gone = media.is_none();
        self.media = media;
        self.seek_anim.jump(progress);
        self.media_rev = self.media_rev.wrapping_add(1);
        self.track_rev = self.track_rev.wrapping_add(1);
        if gone && self.open_popup == Some(OpenPopup::MediaPlayer) {
            self.close_popup();
        }
        self.sync_island();
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
        if self.rect(output).to_f64().contains(pos) || self.island_target(output).to_f64().contains(pos) {
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
            Slot::QsMicTrack => {
                let ratio = ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                let pct = (ratio * 100.0).round() as u8;
                TopbarAction::QsMicAdjust(pct)
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

    const fn caffeine_width() -> i32 {
        COFFEE_PAD + COFFEE_GLYPH + COFFEE_GAP + SWITCH_WIDTH + COFFEE_PAD_END
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

        let mut island_slots = Vec::new();
        let island = self.layout_island(painter, strip, &mut island_slots);
        let tray_limit = island.loc.x + island.size.w + glide_theme::spacing::SP3;
        self.layout_tray(output, (right - ITEM_GAP, tray_limit, y), &mut slots);

        let menu_limit = island.loc.x - glide_theme::spacing::SP3;
        let mut menu = self.layout_app_menu(painter, output, strip, menu_limit);
        slots.append(&mut menu.titles);
        if !(self.popup_reveal.is_open() && Self::island_owns(self.open_popup)) {
            slots.extend(island_slots);
        }

        let mut popup_slots = std::mem::take(&mut menu.rows);
        let popup_rect = self
            .layout_popup(painter, (output, strip), (lang_rect, menu.anchor), &mut popup_slots)
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
        &self, painter: &crate::paint::Painter, (output, strip): (Rectangle<i32, Logical>, Rectangle<i32, Logical>),
        anchors: (Anchor, Anchor), popup_slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
    ) -> Option<Rectangle<i32, Logical>> {
        let (lang_rect, menu_anchor) = anchors;
        let strip_y = strip.loc.y;
        match self.shown_popup() {
            Some(OpenPopup::AppMenu) => menu_anchor,
            Some(OpenPopup::LanguageMenu) => {
                lang_rect.map(|btn| layout_lang_menu_slots(&self.layouts, btn, strip_y, popup_slots))
            }
            Some(popup @ (OpenPopup::NotificationCenter | OpenPopup::MediaPlayer)) => {
                self.layout_island_popup(painter, popup, strip, popup_slots)
            }
            Some(OpenPopup::QuickSettings) => Some(layout_qs_slots(
                output,
                strip_y,
                QsLayoutInput {
                    shape: self.qs_shape(Some(painter)),
                    volume: self.system_status.volume_available,
                    brightness: self.system_status.brightness_available,
                    drawer_items: self.qs_drawer.map_or(0, |drawer| self.qs_panel.devices(drawer).len()),
                    detail: self.detail_slots(painter),
                },
                popup_slots,
            )),
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

type LocalRect = Rectangle<i32, Logical>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum QsTile {
    Wifi,
    Bluetooth,
    NightLight,
    Dnd,
    Caffeine,
    Airplane,
    Screenshot,
    DarkTheme,
    Netbird,
}

const QS_TILES: [QsTile; 8] = [
    QsTile::Wifi,
    QsTile::Bluetooth,
    QsTile::NightLight,
    QsTile::Dnd,
    QsTile::Caffeine,
    QsTile::Airplane,
    QsTile::Screenshot,
    QsTile::DarkTheme,
];

fn qs_tiles(shape: &QsShape) -> impl Iterator<Item = QsTile> {
    QS_TILES.into_iter().chain(shape.netbird.then_some(QsTile::Netbird))
}

impl QsTile {
    const fn slot(self) -> Slot {
        match self {
            Self::Wifi => Slot::QsWifi,
            Self::Bluetooth => Slot::QsBluetooth,
            Self::Netbird => Slot::QsNetbird,
            Self::NightLight => Slot::QsNightLight,
            Self::Dnd => Slot::QsDnd,
            Self::Caffeine => Slot::QsCaffeine,
            Self::Airplane => Slot::QsAirplane,
            Self::Screenshot => Slot::QsScreenshot,
            Self::DarkTheme => Slot::QsDarkTheme,
        }
    }

    const fn more(self) -> Option<Slot> {
        match self {
            Self::Wifi => Some(Slot::QsWifiMore),
            Self::Bluetooth => Some(Slot::QsBluetoothMore),
            Self::Netbird => Some(Slot::QsNetbirdMore),
            _ => None,
        }
    }
}

const QS_PROFILES: [PowerProfile; 3] = [PowerProfile::PowerSaver, PowerProfile::Balanced, PowerProfile::Performance];

fn profile_index(profile: PowerProfile) -> f64 {
    let index = QS_PROFILES.iter().position(|candidate| *candidate == profile).unwrap_or(1);
    f64::from(u8::try_from(index).unwrap_or(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct QsPills {
    output: i32,
    input: i32,
    cancel: i32,
    confirm: i32,
}

#[allow(clippy::struct_excessive_bools)] // Independent facts about which optional rows and tiles the popup has
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct QsShape {
    mic: bool,
    battery: bool,
    profile: bool,
    netbird: bool,
    armed: Option<PowerAction>,
    drawer: Option<(Drawer, i32)>,
    pills: QsPills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QsDrawerArea {
    drawer: Drawer,
    rect: LocalRect,
}

impl QsDrawerArea {
    fn items(self, count: usize) -> impl Iterator<Item = (u16, LocalRect)> {
        let top = self.rect.loc.y + QS_DRAWER_TOP;
        (0..count.max(1)).filter_map(move |index| {
            let index = u16::try_from(index).ok()?;
            let y = top + i32::from(index) * (QS_DRAWER_ITEM + QS_DRAWER_GAP);
            Some((index, local_rect(self.rect.loc.x, y, self.rect.size.w, QS_DRAWER_ITEM)))
        })
    }

    fn visible(self, rect: LocalRect) -> bool {
        rect.loc.y + rect.size.h <= self.rect.loc.y + self.rect.size.h
    }
}

fn qs_drawer_height(items: usize) -> i32 {
    let rows = i32::try_from(items.max(1)).unwrap_or(1);
    QS_DRAWER_TOP + rows * QS_DRAWER_ITEM + (rows - 1) * QS_DRAWER_GAP + QS_DRAWER_BOTTOM
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArmedPower {
    action: PowerAction,
    deadline: Option<std::time::Instant>,
}

impl ArmedPower {
    fn seconds_left(self, now: std::time::Instant) -> Option<u64> {
        let left = self.deadline?.saturating_duration_since(now);
        Some(left.as_millis().div_ceil(1000).try_into().unwrap_or(u64::MAX))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QsSliderRow {
    icon: LocalRect,
    label: LocalRect,
    readout: LocalRect,
    drill: Option<LocalRect>,
    track: LocalRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QsConfirm {
    rect: LocalRect,
    badge: LocalRect,
    question: LocalRect,
    note: Option<LocalRect>,
    cancel: LocalRect,
    confirm: LocalRect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QsGeometry {
    avatar: LocalRect,
    name: LocalRect,
    settings: LocalRect,
    tiles: Vec<(QsTile, LocalRect)>,
    sliders: LocalRect,
    volume: QsSliderRow,
    mic: Option<QsSliderRow>,
    brightness: QsSliderRow,
    drawer: Option<QsDrawerArea>,
    battery_card: Option<LocalRect>,
    battery_head: Option<LocalRect>,
    segments: Option<LocalRect>,
    power_row: LocalRect,
    power: [(PowerAction, LocalRect); 5],
    confirm: Option<QsConfirm>,
    height: i32,
}

use crate::raster::rect as local_rect;

const fn band(rect: LocalRect) -> (i32, i32, i32, i32) {
    (rect.loc.x, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + rect.size.h)
}

fn qs_badge(tile: QsTile, rect: LocalRect) -> LocalRect {
    let inset = if tile.more().is_some() { QS_SPLIT_INSET } else { QS_TILE_INSET };
    local_rect(rect.loc.x + inset, rect.loc.y + (rect.size.h - QS_ICON_BADGE) / 2, QS_ICON_BADGE, QS_ICON_BADGE)
}

fn qs_chevron(rect: LocalRect) -> LocalRect {
    let x = rect.loc.x + rect.size.w - QS_SPLIT_END - QS_CHEVRON;
    local_rect(x, rect.loc.y + (rect.size.h - QS_CHEVRON) / 2, QS_CHEVRON, QS_CHEVRON)
}

fn qs_slider_row(x: i32, y: i32, width: i32, (label_w, drill): (i32, bool)) -> QsSliderRow {
    let icon = local_rect(x, y, QS_SLIDER_ICON, QS_SLIDER_HEAD);
    let right = x + width;
    let drill = drill.then(|| local_rect(right - QS_DRILL, y + (QS_SLIDER_HEAD - QS_DRILL) / 2, QS_DRILL, QS_DRILL));
    let readout_right = drill.map_or(right, |drill| drill.loc.x - SP1);
    let readout = local_rect(readout_right - QS_READOUT_W, y, QS_READOUT_W, QS_SLIDER_HEAD);
    let label_x = x + QS_SLIDER_ICON + SP1;
    let label_w = label_w.min(readout.loc.x - SP2 - QS_MIC_DOT - SP1 - label_x).max(0);
    QsSliderRow {
        icon,
        label: local_rect(label_x, y, label_w, QS_SLIDER_HEAD),
        readout,
        drill,
        track: local_rect(x, y + QS_SLIDER_HEAD + SP1, width, QS_SLIDER_HEIGHT),
    }
}

fn qs_segments(track: LocalRect) -> [(PowerProfile, LocalRect); 3] {
    let inner_w = track.size.w - QS_SEGMENT_INSET * 2;
    let count = i32::try_from(QS_PROFILES.len()).unwrap_or(1);
    let edge = |index: i32| track.loc.x + QS_SEGMENT_INSET + inner_w * index / count;
    let mut index = 0;
    QS_PROFILES.map(|profile| {
        let (left, right) = (edge(index), edge(index + 1));
        index += 1;
        let rect = local_rect(left, track.loc.y + QS_SEGMENT_INSET, right - left, track.size.h - QS_SEGMENT_INSET * 2);
        (profile, rect)
    })
}

fn qs_confirm(row: LocalRect, countdown: bool, pills: QsPills) -> QsConfirm {
    let inset = 1 + QS_CONFIRM_PAD;
    let (left, right) = (row.loc.x + inset, row.loc.x + row.size.w - inset);
    let mut y = row.loc.y + inset;
    let badge = local_rect(left, y, QS_CONFIRM_BADGE, QS_CONFIRM_BADGE);
    let text_x = left + QS_CONFIRM_BADGE + SP2;
    let question = local_rect(text_x, y, right - text_x, QS_CONFIRM_BADGE);
    y += QS_CONFIRM_BADGE + SP2;
    let note = if countdown {
        let note = local_rect(text_x, y, right - text_x, QS_CONFIRM_NOTE);
        y += QS_CONFIRM_NOTE + SP2;
        Some(note)
    } else {
        None
    };
    y += QS_CONFIRM_PILLS_TOP;
    let confirm = local_rect(right - pills.confirm, y, pills.confirm, QS_CONFIRM_PILL);
    let cancel_x = confirm.loc.x - QS_CONFIRM_PILL_GAP - pills.cancel;
    let cancel = local_rect(cancel_x, y, pills.cancel, QS_CONFIRM_PILL);
    let bottom = y + QS_CONFIRM_PILL + inset;
    QsConfirm {
        rect: local_rect(row.loc.x, row.loc.y, row.size.w, bottom - row.loc.y),
        badge,
        question,
        note,
        cancel,
        confirm,
    }
}

fn qs_geometry(shape: &QsShape) -> QsGeometry {
    let row_w = QS_WIDTH - QS_PAD * 2;
    let mut y = QS_PAD;
    let avatar = local_rect(QS_PAD, y, QS_AVATAR, QS_HEADER);
    let settings = local_rect(QS_WIDTH - QS_PAD - QS_HEADER, y, QS_HEADER, QS_HEADER);
    let name_x = QS_PAD + QS_AVATAR + SP2;
    let name = local_rect(name_x, y, settings.loc.x - SP2 - name_x, QS_HEADER);
    y += QS_HEADER + QS_GAP;

    let col_w = (row_w - QS_GRID_GAP) / 2;
    let grid_y = y;
    let tiles: Vec<(QsTile, LocalRect)> = qs_tiles(shape)
        .zip(0..)
        .map(|(tile, index)| {
            let (column, row) = (index % 2, index / 2);
            let x = QS_PAD + column * (col_w + QS_GRID_GAP);
            (tile, local_rect(x, grid_y + row * (QS_TILE_HEIGHT + QS_GRID_GAP), col_w, QS_TILE_HEIGHT))
        })
        .collect();
    let rows = i32::try_from(tiles.len().div_ceil(2)).unwrap_or(1);
    y += rows * QS_TILE_HEIGHT + (rows - 1) * QS_GRID_GAP + QS_GAP;

    let inner_x = QS_PAD + 1 + QS_CARD_PAD_X;
    let inner_w = row_w - 2 * (1 + QS_CARD_PAD_X);
    let opened = |which: Drawer| shape.drawer.filter(|(drawer, _)| *drawer == which).map_or(0, |(_, h)| h);
    let below =
        |row: &QsSliderRow, drawer: Drawer| row.track.loc.y + QS_SLIDER_HEIGHT + opened(drawer) + QS_CARD_PAD_Y * 2 + 1;
    let area = |row: &QsSliderRow, drawer: Drawer| {
        let height = opened(drawer);
        (height > 0).then(|| QsDrawerArea {
            drawer,
            rect: local_rect(inner_x, row.track.loc.y + QS_SLIDER_HEIGHT, inner_w, height),
        })
    };
    let volume = qs_slider_row(inner_x, y + 1 + QS_CARD_PAD_Y, inner_w, (shape.pills.output, true));
    let mic =
        shape.mic.then(|| qs_slider_row(inner_x, below(&volume, Drawer::Output), inner_w, (shape.pills.input, true)));
    let brightness_y = mic.as_ref().map_or_else(|| below(&volume, Drawer::Output), |mic| below(mic, Drawer::Input));
    let brightness = qs_slider_row(inner_x, brightness_y, inner_w, (i32::MAX, false));
    let drawer = area(&volume, Drawer::Output).or_else(|| mic.as_ref().and_then(|mic| area(mic, Drawer::Input)));
    let sliders_bottom = brightness.track.loc.y + QS_SLIDER_HEIGHT + QS_CARD_PAD_Y + 1;
    let sliders = local_rect(QS_PAD, y, row_w, sliders_bottom - y);
    y = sliders_bottom + QS_GAP;

    let (mut battery_card, mut battery_head, mut segments) = (None, None, None);
    if shape.battery || shape.profile {
        let (x, w) = (QS_PAD + 1 + SP1, row_w - 2 * (1 + SP1));
        let mut cursor = y + 1 + SP1;
        if shape.battery {
            battery_head = Some(local_rect(x, cursor, w, QS_BATTERY_HEAD));
            cursor += QS_BATTERY_HEAD + SP1;
        }
        if shape.profile {
            segments = Some(local_rect(x, cursor, w, QS_SEGMENTS_H));
            cursor += QS_SEGMENTS_H + SP1;
        }
        battery_card = Some(local_rect(QS_PAD, y, row_w, cursor + 1 - y));
        y = cursor + 1 + QS_GAP;
    }

    let power_row = local_rect(QS_PAD, y, row_w, QS_POWER_ROW);
    let last = i32::try_from(panels::POWER_ROW.len()).unwrap_or(1) - 1;
    let button_y = y + (QS_POWER_ROW - QS_POWER_BTN) / 2;
    let mut index = 0;
    let power = panels::POWER_ROW.map(|action| {
        let x = QS_PAD + (row_w - QS_POWER_BTN) * index / last.max(1);
        index += 1;
        (action, local_rect(x, button_y, QS_POWER_BTN, QS_POWER_BTN))
    });
    let confirm = shape.armed.map(|action| qs_confirm(power_row, is_destructive(action), shape.pills));
    let session = confirm.map_or(QS_POWER_ROW, |strip| strip.rect.size.h);

    QsGeometry {
        avatar,
        name,
        settings,
        tiles,
        sliders,
        volume,
        mic,
        brightness,
        drawer,
        battery_card,
        battery_head,
        segments,
        power_row,
        power,
        confirm,
        height: y + session + QS_PAD,
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
    let island = Topbar::island_owns(topbar.shown_popup()).then(|| topbar.island_content_alpha());
    for job in topbar.layer_jobs() {
        let placed = match (job.in_popup, popup, island) {
            (true, Some(_), Some(fade)) => {
                Placed { popup: job.rect, element: job.rect, progress: 1.0, alpha: job.alpha * fade }
            }
            (true, Some(popup), None) => Placed { popup, element: job.rect, progress: reveal, alpha: job.alpha },
            (true, None, _) => continue,
            (false, _, _) => Placed { popup: job.rect, element: job.rect, progress: 1.0, alpha: job.alpha },
        };
        let Some(buffer) = topbar.layer_buffer(job.layer, job.rect.size, job.radius, output_scale) else {
            continue;
        };
        push_placed(renderer, buffer, placed, scale, elements);
    }
}

struct QsLayoutInput {
    shape: QsShape,
    volume: bool,
    brightness: bool,
    drawer_items: usize,
    detail: Option<Vec<(Slot, LocalRect)>>,
}

fn layout_qs_slots(
    output: Rectangle<i32, Logical>, strip_y: i32, input: QsLayoutInput,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let geometry = qs_geometry(&input.shape);
    let origin = Point::from((output.loc.x + output.size.w - PAD_X - QS_WIDTH, strip_y + HEIGHT + POPUP_GAP));
    let popup = Rectangle::new(origin, Size::from((QS_WIDTH, geometry.height)));
    let mut push = |slot: Slot, rect: LocalRect| slots.push((slot, offset_rect(rect, origin)));
    if let Some(detail) = input.detail {
        for (slot, rect) in detail {
            push(slot, rect);
        }
        return popup;
    }

    push(Slot::QsSettings, geometry.settings);
    for &(tile, rect) in &geometry.tiles {
        if let Some(more) = tile.more() {
            push(tile.slot(), qs_badge(tile, rect));
            push(more, rect);
        } else {
            push(tile.slot(), rect);
        }
    }
    if input.volume {
        push(Slot::QsVolumeMute, geometry.volume.icon);
        push(Slot::QsOutputPill, geometry.volume.label);
        if let Some(drill) = geometry.volume.drill {
            push(Slot::QsVolumeMore, drill);
        }
        push(Slot::QsVolumeTrack, geometry.volume.track);
    }
    if let Some(mic) = geometry.mic {
        push(Slot::QsMicMute, mic.icon);
        push(Slot::QsInputPill, mic.label);
        if let Some(drill) = mic.drill {
            push(Slot::QsMicMore, drill);
        }
        push(Slot::QsMicTrack, mic.track);
    }
    if let Some(area) = geometry.drawer {
        for (index, rect) in area.items(input.drawer_items).take(input.drawer_items) {
            if area.visible(rect) {
                push(Slot::QsDrawerItem(area.drawer, index), rect);
            }
        }
    }
    if input.brightness {
        push(Slot::QsBrightnessTrack, geometry.brightness.track);
    }
    if let Some(head) = geometry.battery_head {
        push(Slot::QsBattery, head);
    }
    if let Some(track) = geometry.segments {
        for (profile, rect) in qs_segments(track) {
            push(Slot::QsProfile(profile), rect);
        }
    }
    match (geometry.confirm, input.shape.armed) {
        (Some(strip), Some(action)) => {
            push(Slot::QsPowerCancel, strip.cancel);
            push(Slot::QsPowerConfirm(action), strip.confirm);
        }
        _ => {
            for (action, rect) in geometry.power {
                push(Slot::QsPowerButton(action), rect);
            }
        }
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
    let title_trimmed = title.trim();
    if !title_trimmed.is_empty() {
        return truncate_name(title_trimmed);
    }
    let trimmed = app_id.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let tail = trimmed.rsplit('.').next().unwrap_or(trimmed);
    let tail = tail.trim();
    if tail.is_empty() {
        return String::new();
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
    (text_w(painter, label, &SEG_STYLE) + SEG_TEXT_PAD * 2).max(SEG_MIN_WIDTH)
}

pub const MEDIA_ART: i32 = 17;
pub const MEDIA_ART_LARGE: i32 = 56;

#[derive(Debug, Clone, PartialEq)]
pub struct MediaWidget {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub length: Option<Duration>,
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

const fn transport_enabled(media: &MediaWidget, index: usize) -> bool {
    match index {
        0 => media.can_previous,
        2 => media.can_next,
        _ => true,
    }
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
    push_rounded_popup_shadow(topbar, renderer, (rect, POPUP_RADIUS), progress, scale, output_scale, elements);
}

#[allow(clippy::too_many_arguments)]
fn push_rounded_popup_shadow<R>(
    topbar: &mut Topbar, renderer: &mut R, (rect, radius): (Rectangle<i32, Logical>, f32), progress: f64,
    scale: Scale<f64>, output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let margin = popup_shadow_margin(topbar.theme.dark);
    let key = (rect.size.w, rect.size.h, output_scale, radius.to_bits());
    if topbar.shadows.len() >= SHADOW_CACHE_LIMIT && !topbar.shadows.contains_key(&key) {
        topbar.shadows.clear();
    }
    if let std::collections::hash_map::Entry::Vacant(entry) = topbar.shadows.entry(key)
        && let Some(buffer) = draw_popup_shadow(topbar.theme.dark, rect.size, (margin, radius), output_scale)
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

fn draw_popup_shadow(
    dark: bool, size: Size<i32, Logical>, (margin, radius): (i32, f32), scale: i32,
) -> Option<MemoryRenderBuffer> {
    Some(crate::raster::shadow((size.w, size.h), margin, radius, &popup_shadow_layers(dark), scale)?.finish())
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
    let style = if side >= MEDIA_ART_LARGE { &TextStyle::DISPLAY } else { &TextStyle::TOPBAR };
    c.text(painter, (x, x + side, y, y + side), TextAlign::Center, "♪", white, style);
}

fn widest_digits(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_digit() { '8' } else { c }).collect()
}

fn numeric_label_width(painter: &crate::paint::Painter, text: &str, style: &glide_theme::TextStyle) -> i32 {
    let actual = painter.text_width_styled(text, style);
    let widest = painter.text_width_styled(&widest_digits(text), style);
    actual.max(widest).ceil() as i32
}

fn app_chip_width(painter: &crate::paint::Painter, name: &str, has_icon: bool) -> i32 {
    let icon = if has_icon { APP_ICON + APP_GAP } else { 0 };
    APP_PAD_LEFT + icon + text_w(painter, name, &APP_STYLE) + APP_PAD_RIGHT
}

#[allow(clippy::struct_excessive_bools)] // Independent [topbar] switches, not a state machine
#[derive(Debug, Clone, PartialEq)]
pub struct TopbarTheme {
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
    pub ok_soft: Color,
    pub danger_soft: Color,
    pub line: Color,
    pub line_2: Color,
    pub fill_2: Color,
    pub bezel_hi: Color,
    pub acc_ring: Color,
    pub island_bg: Color,
    pub island_line: Color,
    pub ghost: Color,
    pub card: Color,
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
    pub fn from_tokens(t: &glide_theme::Tokens) -> Self {
        let dark = t.appearance == glide_theme::Appearance::Dark;
        let ramp_step = if dark { 2 } else { 3 };
        Self {
            battery_percentage: true,
            clock_date: true,
            clock_24h: None,
            privacy_mic: glide_theme::Accent::Orange.ramp()[ramp_step],
            privacy_camera: t.ok,
            privacy_screen: glide_theme::Accent::Purple.ramp()[ramp_step],
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
            ok_soft: t.ok_soft,
            danger_soft: t.danger_soft,
            line: t.line,
            line_2: t.line_2,
            fill_2: t.fill_2,
            bezel_hi: Color::from_rgb8(255, 255, 255, if dark { BEZEL_HI_DARK } else { BEZEL_HI_LIGHT }),
            acc_ring: t.acc_ring,
            island_bg: t.island_bg,
            island_line: t.island_line,
            ghost: t.fg_4,
            card: t.card_bg,
            battery_low_pct: BATTERY_LOW_PCT,
        }
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        glide_theme::Revision::new()
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
            .color(self.ok_soft)
            .color(self.danger_soft)
            .color(self.line)
            .color(self.line_2)
            .color(self.fill_2)
            .color(self.bezel_hi)
            .color(self.acc_ring)
            .color(self.island_bg)
            .color(self.island_line)
            .color(self.ghost)
            .color(self.card)
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
        Self::from_tokens(&tokens)
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
    topbar.invalidate_stale_buffers();
    topbar.layout_slots(painter, output);

    let strip = topbar.rect(output);
    let mut frame = Frame { renderer, painter, icons, scale, output_scale, elements: Vec::new() };

    push_state_layers(topbar, &mut frame);
    push_quick_settings(topbar, &mut frame);
    island::push(topbar, &mut frame, output);
    push_global_menu(topbar, &mut frame);
    push_app_chip(topbar, &mut frame, strip);
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
        let lit = is_open || topbar.hovered == u16::try_from(index).ok().map(Slot::MenuTitle);
        let key = (index, is_open, lit, output_scale);
        if !topbar.menu_titles.contains_key(&key)
            && let Some(item) = topbar.menu_bar.get(index)
            && let Some(buffer) = crate::global_menu::draw_title(
                painter,
                &item.label,
                rect.size.w,
                rect.size.h,
                hover_colour(&topbar.theme, lit),
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
    let mut text_left = APP_PAD_LEFT;
    if let Some(icon) = icon {
        let top = (BUTTON_HEIGHT - APP_ICON) / 2;
        c.blit(icon, local_rect(APP_PAD_LEFT, top, APP_ICON, APP_ICON));
        text_left += APP_ICON + APP_GAP;
    }
    let span = (text_left, width - APP_PAD_RIGHT);
    c.label(painter, span, TextAlign::Left, name, theme.label, &APP_STYLE);
    Some(c.finish())
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

fn scaled_about_centre(rect: LocalRect, factor: f64) -> LocalRect {
    let w = (f64::from(rect.size.w) * factor).round() as i32;
    let h = (f64::from(rect.size.h) * factor).round() as i32;
    local_rect(rect.loc.x + (rect.size.w - w) / 2, rect.loc.y + (rect.size.h - h) / 2, w.max(0), h.max(0))
}

fn push_layout<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
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
                let state = (hovered == Some(Slot::LayoutDropdown), open);
                draw_lang_dropdown_button(painter, theme, label, width, state, output_scale, Some(icons))
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
    painter: &crate::paint::Painter, theme: &TopbarTheme, text: &str, width: i32, (hovered, open): (bool, bool),
    scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, BUTTON_HEIGHT, scale)?.with_icons(icons);
    if open {
        c.panel(theme.open_fill, BUTTON_RADIUS);
    }
    let style = &TextStyle::TOPBAR;
    let span = (9, 9 + text_w(painter, text, style));
    c.label(painter, span, TextAlign::Left, text, hover_colour(theme, open || hovered), style);

    let side = 10;
    let chevron = local_rect(width - 6 - side, (BUTTON_HEIGHT - side) / 2, side, side);
    if !glyphs::themed(&mut c, if open { "pan-up" } else { "pan-down" }, side, chevron, theme.faint) {
        c.glyph(chevron, |b, w, h, s| {
            crate::paint::draw_chevron_glyph_over(b, w, h, 9.0, 1.5, open, s, theme.faint);
        });
    }
    Some(c.finish())
}

fn push_lang_menu<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
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
        draw_lang_menu(painter, theme, layouts, (active, hovered), (width, height), output_scale, Some(icons))
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_popup_buffer(renderer, buffer, rect, rect, reveal, scale, elements);
    push_popup_shadow(topbar, renderer, rect, reveal, scale, output_scale, elements);
}

fn draw_lang_menu(
    painter: &crate::paint::Painter, theme: &TopbarTheme, layouts: &[String], (active, hovered): (usize, Option<Slot>),
    size: (i32, i32), scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::popover(size, scale, R3, theme.popover_bg, theme.border)?.with_icons(icons);
    let width = size.0;
    let style = &TextStyle::TOPBAR;
    let text_left = LANG_MENU_PAD + 6 + 14 + 6;
    let mut y = LANG_MENU_PAD;
    for (index, code) in layouts.iter().enumerate() {
        let is_active = index == active;
        if is_active {
            let tick_side = 14;
            let top = y + (LANG_MENU_ITEM_HEIGHT - tick_side) / 2;
            let tick = local_rect(LANG_MENU_PAD + 6, top, tick_side, tick_side);
            if !glyphs::themed(&mut c, "object-select", tick_side, tick, theme.active_background) {
                c.glyph(tick, |b, w, h, s| {
                    crate::paint::draw_tick_glyph_over(b, w, h, 13.0, 1.8, s, theme.active_background);
                });
            }
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

const fn hover_colour(theme: &TopbarTheme, hovered: bool) -> Color {
    if hovered { theme.label } else { theme.muted }
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
        let lift = if theme.dark { SEG_LIFT_DARK } else { SEG_LIFT_LIGHT };
        c.fill(
            local_rect(pill.loc.x, pill.loc.y + 1, pill.size.w, pill.size.h),
            Color([0.0, 0.0, 0.0, lift]),
            SEG_RADIUS,
        );
        c.fill(pill, theme.card, SEG_RADIUS);
    }

    for (index, (name, (x, seg))) in layouts.iter().zip(spans).enumerate() {
        let colour = if index == state.active {
            theme.label
        } else if state.hovered == Some(Slot::LayoutPick(index)) {
            theme.muted
        } else {
            theme.faint
        };
        c.label(painter, (x, x + seg), TextAlign::Center, name, colour, &SEG_STYLE);
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
    let cup = local_rect(COFFEE_PAD, 0, COFFEE_GLYPH, BUTTON_HEIGHT);
    let name = if on { "caffeine-cup-full" } else { "caffeine-cup-empty" };
    if !glyphs::themed_on_ink(&mut c, (name, "caffeine-cup-empty"), COFFEE_GLYPH, cup, colour) {
        glyphs::glyph(&mut c, glyphs::Glyph::Cup, COFFEE_GLYPH, cup, colour);
    }
    let track_y = (BUTTON_HEIGHT - SWITCH_HEIGHT) / 2;
    let track = local_rect(width - COFFEE_PAD_END - SWITCH_WIDTH, track_y, SWITCH_WIDTH, SWITCH_HEIGHT);
    draw_switch(&mut c, theme, track, (SWITCH_INSET, SWITCH_THUMB), switch);
    Some(c.finish())
}

fn push_status<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    status::push_popups(topbar, renderer, painter, scale, output_scale, elements);
    status::push_cluster(topbar, renderer, painter, icons, scale, output_scale, elements);
}

const BATTERY_LOW_PCT: u8 = 20;
const BEZEL_HI_DARK: f32 = 0.12;
const BEZEL_HI_LIGHT: f32 = 0.55;

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
        push_qs_power(topbar, frame, rect, (reveal, slide));
        push_qs_main(topbar, frame, rect, (reveal, slide));
    }
    let (renderer, _, _, scale, output_scale, elements) = frame.parts();
    let theme = &topbar.theme;
    if let Some(background) = topbar.qs_bg_cache.resolve((rect.size.w, rect.size.h), output_scale, || {
        draw_qs_background((rect.size.w, rect.size.h), theme, output_scale)
    }) {
        push_popup_buffer(renderer, background, rect, rect, reveal, scale, elements);
    }
    push_rounded_popup_shadow(topbar, renderer, (rect, QS_RADIUS), reveal, scale, output_scale, elements);
}

fn draw_qs_background((w, h): (i32, i32), theme: &TopbarTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::popover((w, h), scale, QS_RADIUS, theme.popover_bg, theme.line)?;
    let inset = QS_RADIUS.ceil() as i32;
    c.fill(local_rect(inset, 0, w - inset * 2, 1), theme.bezel_hi, 0.0);
    Some(c.finish())
}

fn qs_main_motion(slide: f64) -> (i32, f32) {
    ((slide * f64::from(QS_PAD)).round() as i32, (1.0 - slide) as f32)
}

fn push_qs_main<R: ShellRenderer>(
    topbar: &mut Topbar, frame: &mut Frame<'_, R>, rect: Rectangle<i32, Logical>, (reveal, slide): (f64, f64),
) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let key = topbar.qs_main_key(painter);
    let (_, flags, motion, hovered, shape) = key;
    let (output, input) = (topbar.output_label(), topbar.input_label());
    let theme = &topbar.theme;
    let status = &topbar.system_status;
    let panel = &topbar.qs_panel;
    let Some(buffer) = topbar.qs_cache.resolve(key, output_scale, || {
        let ctx = QsRenderContext { painter, theme, hovered };
        let mut canvas = Canvas::new(rect.size.w, rect.size.h, output_scale)?.with_icons(Some(icons));
        let geometry = qs_geometry(&shape);
        let draw = QsDraw { status, panel, geometry, flags, motion, labels: (&output, &input) };
        draw_quick_settings(&mut canvas, &ctx, &draw);
        Some(canvas.finish())
    }) else {
        return;
    };
    let (shift, alpha) = qs_main_motion(slide);
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

fn push_qs_power<R: ShellRenderer>(
    topbar: &mut Topbar, frame: &mut Frame<'_, R>, rect: Rectangle<i32, Logical>, (reveal, slide): (f64, f64),
) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let geometry = qs_geometry(&topbar.qs_shape(Some(painter)));
    let main_view = topbar.qs_panel.view.is_none();
    let hovered = topbar.hovered_in(Slot::in_quick_settings).filter(|_| main_view);
    let (shift, alpha) = qs_main_motion(slide);
    let arm = topbar.arm_anim.value().clamp(0.0, 1.0);
    let theme = &topbar.theme;
    let mut place = |buffer: &MemoryRenderBuffer, at: LocalRect, (rise, opacity): (f64, f32)| {
        let loc = Point::from((rect.loc.x + at.loc.x - shift, rect.loc.y + at.loc.y + rise.round() as i32));
        let placed = Placed { popup: rect, element: Rectangle::new(loc, at.size), progress: reveal, alpha: opacity };
        push_placed(renderer, buffer, placed, scale, elements);
    };

    let row_alpha = (1.0 - arm) as f32 * alpha;
    if row_alpha > 0.0 {
        let row = geometry.power_row;
        let lit = hovered.filter(|slot| matches!(slot, Slot::QsPowerButton(_)));
        let key: QsPowerKey = (lit, None, (row.size.w, row.size.h));
        if let Some(buffer) = topbar
            .qs_row_cache
            .resolve(key, output_scale, || draw_qs_power_row(theme, &geometry, lit, (output_scale, Some(&mut *icons))))
        {
            place(buffer, row, (arm * QS_ARM_RISE, row_alpha));
        }
    }

    let (Some(strip), Some(armed)) = (geometry.confirm, topbar.qs_power) else {
        return;
    };
    let lit = hovered.filter(|slot| matches!(slot, Slot::QsPowerCancel | Slot::QsPowerConfirm(_)));
    let seconds = armed.seconds_left(std::time::Instant::now());
    let key: QsPowerKey = (lit, Some((armed.action, seconds)), (strip.rect.size.w, strip.rect.size.h));
    if let Some(buffer) = topbar.qs_confirm_cache.resolve(key, output_scale, || {
        draw_qs_confirm(painter, theme, &strip, (armed.action, seconds, lit), (output_scale, Some(&mut *icons)))
    }) {
        place(buffer, strip.rect, (-(1.0 - arm) * QS_ARM_RISE, arm as f32 * alpha));
    }
}

struct QsRenderContext<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    hovered: Option<Slot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QsFlags {
    caffeinated: bool,
    dnd: bool,
    dark: bool,
    dragging: Option<Slot>,
    drawer: Option<Drawer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QsMotion {
    volume: u8,
    mic: u8,
    brightness: u8,
    profile: u8,
}

impl QsMotion {
    fn profile_position(self) -> f64 {
        f64::from(self.profile) / MOTION_QUANTUM
    }
}

struct QsDraw<'a> {
    status: &'a crate::system_info::SystemStatus,
    panel: &'a panels::DetailPanel,
    geometry: QsGeometry,
    flags: QsFlags,
    motion: QsMotion,
    labels: (&'a str, &'a str),
}

fn percent(value: f64) -> u8 {
    value.clamp(0.0, 100.0).round() as u8
}

type GlyphPainter = fn(&mut [u8], i32, i32, f32, f32, i32, Color);

const QS_NAME: TextStyle = TextStyle { size_px: 13.0, line_height_px: 18.0, weight: 630, ..TextStyle::CAP };
const QS_INITIAL: TextStyle = TextStyle { weight: 650, ..TextStyle::CAP };
const QS_TITLE: TextStyle = TextStyle { line_height_px: 15.0, weight: 600, ..TextStyle::CAP };
const QS_SUB: TextStyle = TextStyle { size_px: 10.5, line_height_px: 13.0, weight: 400, ..TextStyle::CAP };
const QS_LABEL: TextStyle = TextStyle { weight: 550, ..TextStyle::CAP };
const QS_NOTE: TextStyle = TextStyle { size_px: 11.0, line_height_px: 14.0, weight: 400, ..TextStyle::CAP };
const QS_PILL_TEXT: TextStyle = TextStyle { size_px: 11.5, line_height_px: 14.0, weight: 600, ..TextStyle::CAP };
const QS_READOUT: TextStyle = TextStyle { size_px: 12.0, line_height_px: 16.0, weight: 500, ..TextStyle::NUM };
const QS_BATTERY_PCT: TextStyle = TextStyle { line_height_px: 16.0, weight: 600, ..TextStyle::NUM };
const QS_SOFT_DIM: f32 = 0.85;
const QS_SLIDER_THUMB_HOVER: i32 = 18;
const QS_SLIDER_THUMB_DRAG: i32 = 19;
const QS_SLIDER_RING_HOVER: i32 = 4;
const QS_SLIDER_RING_DRAG: i32 = 6;
const QS_MUTED_FILL: f32 = 0.35;
const QS_MUTED_THUMB: f32 = 0.7;
const QS_THUMB_SHADOW: Color = Color([0.0, 0.0, 0.0, 0.18]);
const QS_BATTERY_TIP_INSET: i32 = 4;
const QS_BATTERY_TIP: i32 = 3;
const QS_BATTERY_FILL_INSET: i32 = 3;
const QS_BATTERY_GAP: i32 = 10;
const QS_SEGMENT_ICON: i32 = 14;
const QS_MIC_DOT: i32 = 6;
const QS_MIC_MUTED_DOT: f32 = 0.5;
const QS_MIC_GLOW: f32 = 0.25;
const QS_SEGMENT_GAP: i32 = 6;
const QS_TILE_TEXT_GAP: i32 = 1;
const QS_CHEVRON_GLYPH: i32 = 13;

fn qs_symbol(c: &mut Canvas<'_>, rect: LocalRect, (icon, glyph): (&str, GlyphPainter), side: i32, colour: Color) {
    let at = (rect.loc.x + (rect.size.w - side) / 2, rect.loc.y + (rect.size.h - side) / 2);
    if !c.icon(icon, side, at, colour) {
        let glyph_side = (side - 1) as f32;
        c.glyph(rect, |b, w, h, s| glyph(b, w, h, glyph_side, 1.5, s, colour));
    }
}

fn dimmed(colour: Color, factor: f32) -> Color {
    colour.with_alpha(colour.0[3] * factor)
}

fn qs_pill_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    let text = text_w(painter, label, &QS_LABEL);
    (QS_PILL_PAD * 2 + text + SP1 + QS_PILL_CHEVRON).min(QS_PILL_MAX)
}

fn qs_confirm_pill_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    text_w(painter, label, &QS_PILL_TEXT) + QS_CONFIRM_PILL_PAD * 2
}

struct QsTileSpec<'a> {
    tile: QsTile,
    rect: LocalRect,
    on: bool,
    title: &'a str,
    subtitle: &'a str,
    icon_name: &'a str,
    glyph: GlyphPainter,
}

fn draw_qs_chevron(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, (on, hovered): (bool, bool)) {
    let theme = ctx.theme;
    let colour = match (hovered, on) {
        (true, true) => theme.active_soft_label,
        (true, false) => theme.label,
        (false, true) => dimmed(theme.active_soft_label, QS_SOFT_DIM),
        (false, false) => theme.faint,
    };
    let nudged =
        Rectangle::new(Point::from((rect.loc.x + if hovered { QS_CHEVRON_NUDGE } else { 0 }, rect.loc.y)), rect.size);
    let side = QS_CHEVRON_GLYPH;
    let at = (nudged.loc.x + (nudged.size.w - side) / 2, nudged.loc.y + (nudged.size.h - side) / 2);
    if !c.icon("go-next", side, at, colour) {
        c.text_in(ctx.painter, nudged, TextAlign::Center, "\u{203a}", colour, &TextStyle::HEAD);
    }
}

fn draw_qs_tile(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, spec: &QsTileSpec<'_>) {
    let (rect, theme) = (spec.rect, ctx.theme);
    let more = spec.tile.more();
    let hovered = ctx.hovered.is_some_and(|slot| slot == spec.tile.slot() || Some(slot) == more);
    if spec.on {
        c.fill(rect, theme.active_soft, R2);
    } else {
        c.fill(rect, theme.track_soft, R2);
        c.stroke(rect, 1.0, R2, if hovered { theme.line_2 } else { theme.line });
    }

    let badge = qs_badge(spec.tile, rect);
    let (well, symbol) =
        if spec.on { (theme.active_background, theme.active_label) } else { (theme.fill_2, theme.muted) };
    c.fill(badge, well, R_ROUND);
    qs_symbol(c, badge, (spec.icon_name, spec.glyph), 15, symbol);

    let (text_left, text_right) = if more.is_some() {
        let chevron = qs_chevron(rect);
        draw_qs_chevron(c, ctx, chevron, (spec.on, ctx.hovered.is_some_and(|slot| Some(slot) == more)));
        (badge.loc.x + QS_ICON_BADGE + QS_SPLIT_GAP, chevron.loc.x - QS_SPLIT_GAP)
    } else {
        (badge.loc.x + QS_ICON_BADGE + SP2, rect.loc.x + rect.size.w - QS_TILE_INSET)
    };
    let (title_colour, subtitle_colour) = if spec.on {
        (theme.active_soft_label, dimmed(theme.active_soft_label, QS_SOFT_DIM))
    } else {
        (theme.label, theme.faint)
    };
    let title_h = QS_TITLE.line_height_px as i32;
    let sub_h = QS_SUB.line_height_px as i32;
    let top = rect.loc.y + (rect.size.h - title_h - QS_TILE_TEXT_GAP - sub_h) / 2;
    let sub_top = top + title_h + QS_TILE_TEXT_GAP;
    c.text(
        ctx.painter,
        (text_left, text_right, top, top + title_h),
        TextAlign::Left,
        spec.title,
        title_colour,
        &QS_TITLE,
    );
    c.text(
        ctx.painter,
        (text_left, text_right, sub_top, sub_top + sub_h),
        TextAlign::Left,
        spec.subtitle,
        subtitle_colour,
        &QS_SUB,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QsThumb {
    Rest,
    Hover,
    Drag,
}

impl QsThumb {
    const fn of(slot: Slot, hovered: Option<Slot>, dragging: Option<Slot>) -> Self {
        if matches!(dragging, Some(dragged) if same_slot(dragged, slot)) {
            Self::Drag
        } else if matches!(hovered, Some(hover) if same_slot(hover, slot)) {
            Self::Hover
        } else {
            Self::Rest
        }
    }

    const fn side(self) -> i32 {
        match self {
            Self::Rest => QS_SLIDER_THUMB,
            Self::Hover => QS_SLIDER_THUMB_HOVER,
            Self::Drag => QS_SLIDER_THUMB_DRAG,
        }
    }

    const fn ring(self) -> i32 {
        match self {
            Self::Rest => 0,
            Self::Hover => QS_SLIDER_RING_HOVER,
            Self::Drag => QS_SLIDER_RING_DRAG,
        }
    }
}

const fn same_slot(a: Slot, b: Slot) -> bool {
    matches!(
        (a, b),
        (Slot::QsVolumeTrack, Slot::QsVolumeTrack)
            | (Slot::QsMicTrack, Slot::QsMicTrack)
            | (Slot::QsBrightnessTrack, Slot::QsBrightnessTrack)
    )
}

#[derive(Debug, Clone, Copy)]
struct QsTrack {
    pct: u8,
    muted: bool,
    enabled: bool,
    thumb: QsThumb,
}

fn draw_qs_track(c: &mut Canvas<'_>, theme: &TopbarTheme, rect: LocalRect, track: QsTrack) {
    let groove_y = rect.loc.y + (rect.size.h - QS_SLIDER_TRACK_H) / 2;
    let bar = |width| local_rect(rect.loc.x, groove_y, width, QS_SLIDER_TRACK_H);
    if !track.enabled {
        c.fill(bar(rect.size.w), theme.track_soft, R1);
        return;
    }
    let groove = if track.thumb == QsThumb::Rest { theme.fill_2 } else { theme.track };
    c.fill(bar(rect.size.w), groove, R1);
    let fill_w = ((f64::from(rect.size.w) * f64::from(track.pct.min(100))) / 100.0).round() as i32;
    if fill_w > 0 {
        let colour = if track.muted { dimmed(theme.faint, QS_MUTED_FILL) } else { theme.active_background };
        c.fill(bar(fill_w), colour, R1);
    }

    let centre = (rect.loc.x + fill_w, rect.loc.y + rect.size.h / 2);
    let circle = |side: i32| local_rect(centre.0 - side / 2, centre.1 - side / 2, side, side);
    if track.muted {
        c.fill(circle(QS_SLIDER_THUMB), dimmed(theme.faint, QS_MUTED_THUMB), R_ROUND);
        return;
    }
    let side = track.thumb.side();
    if track.thumb.ring() > 0 {
        c.fill(circle(side + track.thumb.ring() * 2), theme.acc_ring, R_ROUND);
    }
    let shadow = circle(side + 2);
    c.fill(Rectangle::new(Point::from((shadow.loc.x, shadow.loc.y + 1)), shadow.size), QS_THUMB_SHADOW, R_ROUND);
    let thumb = circle(side);
    c.fill(thumb, theme.thumb, R_ROUND);
    c.stroke(thumb, 1.0, R_ROUND, theme.line);
}

fn draw_qs_readout(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, value: Option<u8>) {
    let text = value.map_or_else(|| "\u{2014}".to_string(), |pct| format!("{pct}%"));
    c.text_in(ctx.painter, rect, TextAlign::Right, &text, ctx.theme.muted, &QS_READOUT);
}

fn draw_qs_device_pill(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, (label, open): (&str, bool)) {
    let theme = ctx.theme;
    if open {
        c.fill(rect, theme.active_soft, R2);
    }
    let text_right = rect.loc.x + rect.size.w - QS_PILL_PAD - SP1 - QS_PILL_CHEVRON;
    let (top, bottom) = (rect.loc.y, rect.loc.y + rect.size.h);
    let text_colour = if open { theme.active_soft_label } else { theme.label };
    c.text(
        ctx.painter,
        (rect.loc.x + QS_PILL_PAD, text_right, top, bottom),
        TextAlign::Left,
        label,
        text_colour,
        &QS_LABEL,
    );
    let chevron =
        local_rect(text_right + SP1, top + (rect.size.h - QS_PILL_CHEVRON) / 2, QS_PILL_CHEVRON, QS_PILL_CHEVRON);
    let colour = if open { theme.active_soft_label } else { theme.faint };
    let icon = if open { "pan-up" } else { "pan-down" };
    if !c.icon(icon, QS_PILL_CHEVRON, (chevron.loc.x, chevron.loc.y), colour) {
        let side = QS_PILL_CHEVRON as f32;
        c.glyph(chevron, |b, w, h, s| crate::paint::draw_chevron_glyph_over(b, w, h, side, 1.8, open, s, colour));
    }
}

fn qs_device_kind(icon: &str) -> crate::i18n::Key {
    use crate::i18n::Key;
    match icon {
        "audio-headset" => Key::QsKindHeadset,
        "audio-headphones" => Key::QsKindHeadphones,
        "video-display" => Key::QsDisplay,
        "audio-input-microphone" => Key::TbMicrophone,
        "camera-web" => Key::QsKindWebcam,
        _ => Key::QsKindSpeakers,
    }
}

fn draw_qs_drawer_item(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, rect: LocalRect, device: &crate::qs_net::AudioOutput,
) {
    use crate::i18n::{Key, t};
    let theme = ctx.theme;
    if device.active {
        c.fill(rect, theme.active_soft, R2);
    }
    let icon =
        local_rect(rect.loc.x + SP2, rect.loc.y + (rect.size.h - QS_DRAWER_ICON) / 2, QS_DRAWER_ICON, QS_DRAWER_ICON);
    let (well, symbol) =
        if device.active { (theme.active_background, theme.active_label) } else { (theme.fill_2, theme.muted) };
    c.fill(icon, well, R1);
    let speaker: GlyphPainter =
        |b, w, h, side, stroke, s, c| crate::paint::draw_speaker_glyph_over(b, w, h, side, stroke, false, s, c);
    qs_symbol(c, icon, (device.icon, speaker), 12, symbol);

    let mut right = rect.loc.x + rect.size.w - SP2;
    if device.active {
        let tick = local_rect(
            right - QS_DRAWER_TICK,
            rect.loc.y + (rect.size.h - QS_DRAWER_TICK) / 2,
            QS_DRAWER_TICK,
            QS_DRAWER_TICK,
        );
        qs_symbol(
            c,
            tick,
            ("object-select", crate::paint::draw_tick_glyph_over),
            QS_DRAWER_TICK,
            theme.active_soft_label,
        );
        right = tick.loc.x - SP2;
    }
    let (title_colour, sub_colour) = if device.active {
        (theme.active_soft_label, dimmed(theme.active_soft_label, QS_SOFT_DIM))
    } else {
        (theme.label, theme.faint)
    };
    let left = icon.loc.x + QS_DRAWER_ICON + SP2;
    let title_h = QS_LABEL.line_height_px as i32;
    let sub_h = QS_SUB.line_height_px as i32 + 1;
    let top = rect.loc.y + (rect.size.h - title_h - sub_h) / 2;
    c.text(
        ctx.painter,
        (left, right, top, top + title_h),
        TextAlign::Left,
        &device.description,
        title_colour,
        &QS_LABEL,
    );
    let kind = if device.active { Key::QsDefaultDevice } else { qs_device_kind(device.icon) };
    c.text(
        ctx.painter,
        (left, right, top + title_h, top + title_h + sub_h),
        TextAlign::Left,
        t(kind),
        sub_colour,
        &QS_SUB,
    );
}

fn draw_qs_drawer(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, area: QsDrawerArea, panel: &panels::DetailPanel) {
    use crate::i18n::{Key, t};
    let (theme, origin) = (ctx.theme, area.rect.loc);
    let devices = panel.devices(area.drawer);
    let Some(sub) = Canvas::new(area.rect.size.w, area.rect.size.h, c.scale) else {
        return;
    };
    let mut sub = sub.with_icons(c.icons.as_deref_mut());
    sub.fill(local_rect(0, SP1, area.rect.size.w, 1), theme.line, 0.0);
    for (index, rect) in area.items(devices.len()) {
        let local = Rectangle::new(rect.loc - origin, rect.size);
        if let Some(device) = devices.get(usize::from(index)) {
            draw_qs_drawer_item(&mut sub, ctx, local, device);
        } else {
            let message = match (panel.devices_ready(area.drawer), area.drawer) {
                (false, _) => Key::QsSearching,
                (true, Drawer::Output) => Key::QsNoOutputs,
                (true, Drawer::Input) => Key::QsNoInputs,
            };
            sub.text_in(ctx.painter, local, TextAlign::Center, t(message), theme.faint, &TextStyle::CAP);
        }
    }
    let pixels = sub.buf;
    c.blit(&pixels, area.rect);
}

fn initial(name: &str) -> String {
    name.chars().next().unwrap_or('?').to_uppercase().to_string()
}

fn draw_qs_header(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, geometry: &QsGeometry, user_name: &str) {
    let theme = ctx.theme;
    let top = theme.active_background.state_layer(theme.active_label, AVATAR_GRADIENT_LIFT);
    c.gradient(geometry.avatar, R_ROUND, top, theme.active_background);
    c.text_in(ctx.painter, geometry.avatar, TextAlign::Center, &initial(user_name), theme.active_label, &QS_INITIAL);
    c.text_in(ctx.painter, geometry.name, TextAlign::Left, user_name, theme.label, &QS_NAME);
    let colour = if ctx.hovered == Some(Slot::QsSettings) { theme.label } else { theme.muted };
    qs_symbol(c, geometry.settings, ("emblem-system", crate::paint::draw_gear_glyph_over), 16, colour);
}

const AVATAR_GRADIENT_LIFT: f32 = glide_theme::motion::ST_DRAG;

fn draw_qs_battery_glyph(c: &mut Canvas<'_>, rect: LocalRect, pct: u8, colour: Color) {
    let shell = dimmed(colour, QS_SOFT_DIM);
    c.stroke(rect, 1.5, R1, shell);
    let tip_h = rect.size.h - QS_BATTERY_TIP_INSET * 2;
    let tip = local_rect(rect.loc.x + rect.size.w + 1, rect.loc.y + QS_BATTERY_TIP_INSET, QS_BATTERY_TIP - 1, tip_h);
    c.fill(tip, shell, 0.0);
    let inner_w = rect.size.w - QS_BATTERY_FILL_INSET * 2;
    let fill_w = inner_w * i32::from(pct.min(100)) / 100;
    if fill_w > 0 {
        let fill = local_rect(
            rect.loc.x + QS_BATTERY_FILL_INSET,
            rect.loc.y + QS_BATTERY_FILL_INSET,
            fill_w,
            rect.size.h - QS_BATTERY_FILL_INSET * 2,
        );
        c.fill(fill, colour, R1);
    }
}

fn draw_qs_power_row(
    theme: &TopbarTheme, geometry: &QsGeometry, lit: Option<Slot>,
    (scale, icons): (i32, Option<&mut crate::icon::IconLoader>),
) -> Option<MemoryRenderBuffer> {
    let row = geometry.power_row;
    let mut c = Canvas::new(row.size.w, row.size.h, scale)?.with_icons(icons);
    for (action, rect) in geometry.power {
        let button = Rectangle::new(rect.loc - row.loc, rect.size);
        let hover = lit == Some(Slot::QsPowerButton(action));
        c.fill(button, theme.track_soft, R_ROUND);
        c.stroke(button, 1.0, R_ROUND, if hover { theme.line_2 } else { theme.line });
        let colour = match (hover, is_destructive(action)) {
            (true, true) => theme.danger,
            (true, false) => theme.label,
            (false, _) => theme.muted,
        };
        qs_symbol(&mut c, button, (panels::power_icon(action), panels::power_glyph(action)), 16, colour);
    }
    Some(c.finish())
}

const fn qs_question(action: PowerAction) -> crate::i18n::Key {
    match action {
        PowerAction::Lock => crate::i18n::Key::QsAskLock,
        PowerAction::Sleep => crate::i18n::Key::QsAskSleep,
        other => crate::power_dialog::question_key(other),
    }
}

fn draw_qs_confirm(
    painter: &crate::paint::Painter, theme: &TopbarTheme, strip: &QsConfirm,
    (action, seconds, lit): (PowerAction, Option<u64>, Option<Slot>),
    (scale, icons): (i32, Option<&mut crate::icon::IconLoader>),
) -> Option<MemoryRenderBuffer> {
    use crate::i18n::{Key, t};
    let origin = strip.rect.loc;
    let local = |rect: LocalRect| Rectangle::new(rect.loc - origin, rect.size);
    let mut c = Canvas::new(strip.rect.size.w, strip.rect.size.h, scale)?.with_icons(icons);
    let tone = if is_destructive(action) { theme.danger } else { theme.active_background };
    let full = c.full();
    c.fill(full, theme.track_soft, R2);
    c.stroke(full, 1.0, R2, theme.line);

    let badge = local(strip.badge);
    c.fill(badge, tone, R_ROUND);
    qs_symbol(&mut c, badge, (panels::power_icon(action), panels::power_glyph(action)), 14, theme.active_label);
    c.text_in(painter, local(strip.question), TextAlign::Left, t(qs_question(action)), theme.label, &QS_LABEL);
    if let (Some(note), Some(seconds)) = (strip.note, seconds) {
        let text = t(Key::QsPowerCountdown).replace("{n}", &seconds.to_string());
        c.text_in(painter, local(note), TextAlign::Left, &text, theme.faint, &QS_NOTE);
    }

    let cancel = local(strip.cancel);
    let cancel_hover = lit == Some(Slot::QsPowerCancel);
    c.fill(cancel, if cancel_hover { theme.track } else { theme.fill_2 }, R_ROUND);
    let cancel_colour = if cancel_hover { theme.label } else { theme.muted };
    c.text_in(painter, cancel, TextAlign::Center, t(Key::PowerCancel), cancel_colour, &QS_PILL_TEXT);
    let confirm = local(strip.confirm);
    c.fill(confirm, tone, R_ROUND);
    let label = t(crate::power_dialog::confirm_key(action));
    c.text_in(painter, confirm, TextAlign::Center, label, theme.active_label, &QS_PILL_TEXT);
    Some(c.finish())
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

const fn power_profile_label(profile: PowerProfile) -> crate::i18n::Key {
    match profile {
        PowerProfile::PowerSaver => crate::i18n::Key::QsProfileSaver,
        PowerProfile::Balanced => crate::i18n::Key::QsProfileBalanced,
        PowerProfile::Performance => crate::i18n::Key::QsProfilePerformance,
    }
}

const fn power_profile_icon(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::PowerSaver => "power-profile-power-saver-symbolic",
        PowerProfile::Balanced => "power-profile-balanced-symbolic",
        PowerProfile::Performance => "power-profile-performance-symbolic",
    }
}

fn wifi_subtitle(status: &crate::system_info::SystemStatus) -> &str {
    use crate::i18n::{Key, t};
    if !status.wifi_enabled {
        t(Key::QsOff)
    } else if status.wifi_ssid.is_empty() {
        t(if status.wifi_level > 0 { Key::Connected } else { Key::QsOn })
    } else {
        &status.wifi_ssid
    }
}

fn draw_qs_tiles(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, draw: &QsDraw<'_>) {
    use crate::i18n::{Key, t};
    let (status, flags) = (draw.status, draw.flags);
    let on_off = |on: bool| t(if on { Key::QsOn } else { Key::QsOff });
    let bluetooth_sub =
        if status.bluetooth_on && status.bluetooth_connected { t(Key::Connected) } else { on_off(status.bluetooth_on) };
    let moon: GlyphPainter = |b, w, h, side, _, s, c| crate::paint::draw_moon_glyph_over(b, w, h, side, s, c);
    let coffee: GlyphPainter = |b, w, h, side, stroke, s, c| {
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        crate::paint::draw_coffee_glyph_over(b, w, h, cx, cy, side, stroke, s, c, false);
    };
    for &(tile, rect) in &draw.geometry.tiles {
        let (on, title, subtitle, icon_name, glyph): (bool, &str, &str, &str, GlyphPainter) = match tile {
            QsTile::Netbird => {
                let (on, subtitle) = draw.panel.netbird.tile();
                (on, t(Key::Netbird), subtitle, "network-vpn", crate::paint::draw_lock_glyph_over)
            }
            QsTile::Wifi => (
                status.wifi_enabled,
                t(Key::WiFi),
                wifi_subtitle(status),
                wifi_icon_name(status.wifi_level),
                crate::paint::draw_wifi_waves_glyph_over,
            ),
            QsTile::Bluetooth => (
                status.bluetooth_on,
                t(Key::Bluetooth),
                bluetooth_sub,
                if status.bluetooth_on { "bluetooth-active" } else { "bluetooth-disabled" },
                crate::paint::draw_bluetooth_glyph_over,
            ),
            QsTile::NightLight => {
                let on = status.night_light_on;
                (on, t(Key::NightLight), on_off(on), "night-light", moon)
            }
            QsTile::Dnd => (flags.dnd, t(Key::DoNotDisturb), on_off(flags.dnd), "notifications-disabled", moon),
            QsTile::Caffeine => {
                let icon = if flags.caffeinated { "caffeine-cup-full" } else { "caffeine-cup-empty" };
                (flags.caffeinated, t(Key::Caffeine), on_off(flags.caffeinated), icon, coffee)
            }
            QsTile::Airplane => (
                status.airplane,
                t(Key::AirplaneMode),
                on_off(status.airplane),
                "airplane-mode",
                crate::paint::draw_wifi_waves_glyph_over,
            ),
            QsTile::Screenshot => (
                false,
                t(Key::QsScreenshot),
                t(Key::WholeScreen),
                "accessories-screenshot",
                crate::paint::draw_camera_glyph_over,
            ),
            QsTile::DarkTheme => (flags.dark, t(Key::QsDarkTheme), on_off(flags.dark), "dark-mode", moon),
        };
        draw_qs_tile(c, ctx, &QsTileSpec { tile, rect, on, title, subtitle, icon_name, glyph });
    }
}

fn draw_qs_sliders(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, draw: &QsDraw<'_>) {
    let (theme, geometry, status) = (ctx.theme, &draw.geometry, draw.status);
    let card_hover = matches!(
        ctx.hovered,
        Some(
            Slot::QsVolumeMute
                | Slot::QsVolumeMore
                | Slot::QsOutputPill
                | Slot::QsMicMute
                | Slot::QsMicMore
                | Slot::QsInputPill
                | Slot::QsDrawerItem(..)
        )
    ) || ctx.hovered.is_some_and(Slot::is_slider);
    c.fill(geometry.sliders, theme.track_soft, R3);
    c.stroke(geometry.sliders, 1.0, R3, if card_hover { theme.line_2 } else { theme.line });
    for row in geometry.mic.iter().chain(std::iter::once(&geometry.brightness)) {
        let divider = local_rect(row.track.loc.x, row.icon.loc.y - QS_CARD_PAD_Y - 1, row.track.size.w, 1);
        c.fill(divider, theme.line, 0.0);
    }
    if let Some(area) = geometry.drawer {
        draw_qs_drawer(c, ctx, area, draw.panel);
    }

    let volume = geometry.volume;
    let (muted, enabled, pct) = (status.volume_muted, status.volume_available, draw.motion.volume);
    let icon_colour = if !enabled {
        theme.faint
    } else if muted {
        theme.danger
    } else if ctx.hovered == Some(Slot::QsVolumeMute) {
        theme.label
    } else {
        theme.muted
    };
    let speaker: GlyphPainter = if muted {
        |b, w, h, side, stroke, s, c| crate::paint::draw_speaker_glyph_over(b, w, h, side, stroke, true, s, c)
    } else {
        |b, w, h, side, stroke, s, c| crate::paint::draw_speaker_glyph_over(b, w, h, side, stroke, false, s, c)
    };
    qs_symbol(c, volume.icon, (volume_icon_name(muted, pct), speaker), 15, icon_colour);
    if enabled {
        draw_qs_device_pill(c, ctx, volume.label, (draw.labels.0, draw.flags.drawer == Some(Drawer::Output)));
        draw_qs_drill(c, ctx, volume.drill, Slot::QsVolumeMore);
    }
    draw_qs_readout(c, ctx, volume.readout, enabled.then_some(pct));
    let thumb = QsThumb::of(Slot::QsVolumeTrack, ctx.hovered, draw.flags.dragging);
    draw_qs_track(c, theme, volume.track, QsTrack { pct, muted, enabled, thumb });

    if let Some(mic) = geometry.mic {
        draw_qs_mic_row(c, ctx, draw, mic);
    }

    let brightness = geometry.brightness;
    let (enabled, pct) = (status.brightness_available, draw.motion.brightness);
    let sun: GlyphPainter = crate::paint::draw_sun_glyph_over;
    qs_symbol(c, brightness.icon, ("display-brightness", sun), 15, if enabled { theme.muted } else { theme.faint });
    let label =
        Rectangle::new(Point::from((brightness.label.loc.x + SP1, brightness.label.loc.y)), brightness.label.size);
    let label_colour = if enabled { theme.label } else { theme.faint };
    c.text_in(
        ctx.painter,
        label,
        TextAlign::Left,
        crate::i18n::t(crate::i18n::Key::QsDisplay),
        label_colour,
        &QS_LABEL,
    );
    draw_qs_readout(c, ctx, brightness.readout, enabled.then_some(pct));
    let thumb = QsThumb::of(Slot::QsBrightnessTrack, ctx.hovered, draw.flags.dragging);
    draw_qs_track(c, theme, brightness.track, QsTrack { pct, muted: false, enabled, thumb });
}

fn draw_qs_mic_glyph(c: &mut Canvas<'_>, rect: LocalRect, (muted, colour): (bool, Color)) {
    let side = 15;
    let at = (rect.loc.x + (rect.size.w - side) / 2, rect.loc.y + (rect.size.h - side) / 2);
    let name = if muted { "microphone-sensitivity-muted" } else { "audio-input-microphone" };
    if c.icon(name, side, at, colour) {
        return;
    }
    let (x, y) = at;
    let centre = x + side / 2;
    c.stroke(local_rect(centre - 3, y + 1, 7, 9), 1.5, R_ROUND, colour);
    c.fill(local_rect(centre, y + 11, 1, 2), colour, 0.0);
    c.fill(local_rect(centre - 3, y + 13, 7, 1), colour, 0.0);
    if muted {
        c.fill(local_rect(x + 1, y + side / 2, side - 2, 1), colour, 0.0);
    }
}

fn draw_qs_drill(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, drill: Option<LocalRect>, slot: Slot) {
    if let Some(drill) = drill {
        draw_qs_chevron(c, ctx, drill, (false, ctx.hovered == Some(slot)));
    }
}

fn draw_qs_mic_row(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, draw: &QsDraw<'_>, row: QsSliderRow) {
    let (theme, status) = (ctx.theme, draw.status);
    let (muted, pct) = (status.mic_muted, draw.motion.mic);
    let icon_colour = if muted {
        theme.danger
    } else if ctx.hovered == Some(Slot::QsMicMute) {
        theme.label
    } else {
        theme.muted
    };
    draw_qs_mic_glyph(c, row.icon, (muted, icon_colour));
    draw_qs_device_pill(c, ctx, row.label, (draw.labels.1, draw.flags.drawer == Some(Drawer::Input)));
    draw_qs_drill(c, ctx, row.drill, Slot::QsMicMore);

    let recording = !status.privacy.microphone.is_empty();
    if muted || recording {
        let centre = (row.readout.loc.x - SP1 - QS_MIC_DOT / 2, row.readout.loc.y + row.readout.size.h / 2);
        let dot = |side: i32| local_rect(centre.0 - side / 2, centre.1 - side / 2, side, side);
        if muted {
            c.fill(dot(QS_MIC_DOT), dimmed(theme.danger, QS_MIC_MUTED_DOT), R_ROUND);
        } else {
            c.fill(dot(QS_MIC_DOT * 2), dimmed(theme.ok, QS_MIC_GLOW), R_ROUND);
            c.fill(dot(QS_MIC_DOT), theme.ok, R_ROUND);
        }
    }
    draw_qs_readout(c, ctx, row.readout, Some(pct));
    let thumb = QsThumb::of(Slot::QsMicTrack, ctx.hovered, draw.flags.dragging);
    draw_qs_track(c, theme, row.track, QsTrack { pct, muted, enabled: true, thumb });
}

fn draw_qs_battery_card(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, draw: &QsDraw<'_>) {
    let (theme, geometry, status) = (ctx.theme, &draw.geometry, draw.status);
    let Some(card) = geometry.battery_card else {
        return;
    };
    c.fill(card, theme.track_soft, R3);
    c.stroke(card, 1.0, R3, theme.line);
    if let (Some(head), Some(pct)) = (geometry.battery_head, status.battery_pct) {
        let low = !status.battery_charging && pct <= theme.battery_low_pct;
        let (top, bottom) = (head.loc.y, head.loc.y + head.size.h);
        let mut x = head.loc.x + SP2;
        let glyph_y = top + (head.size.h - QS_BATTERY_GLYPH_H) / 2;
        let glyph = local_rect(x, glyph_y, QS_BATTERY_GLYPH_W, QS_BATTERY_GLYPH_H);
        draw_qs_battery_glyph(c, glyph, pct, if low { theme.danger } else { theme.ok });
        x += QS_BATTERY_GLYPH_W + QS_BATTERY_TIP + QS_BATTERY_GAP;
        let pct_text = format!("{pct}%");
        let pct_w = text_w(ctx.painter, &pct_text, &QS_BATTERY_PCT);
        c.text(ctx.painter, (x, x + pct_w, top, bottom), TextAlign::Left, &pct_text, theme.label, &QS_BATTERY_PCT);
        x += pct_w + QS_BATTERY_GAP;
        let chevron =
            local_rect(head.loc.x + head.size.w - SP2 - QS_CHEVRON_GLYPH - 1, top, QS_CHEVRON_GLYPH + 1, head.size.h);
        let eta = panels::battery_head_line(status);
        c.text(
            ctx.painter,
            (x, chevron.loc.x - QS_BATTERY_GAP, top, bottom),
            TextAlign::Left,
            &eta,
            theme.muted,
            &TextStyle::CAP,
        );
        let hover = ctx.hovered == Some(Slot::QsBattery);
        draw_qs_chevron(c, ctx, chevron, (false, hover));
    }
    if let Some(track) = geometry.segments {
        draw_qs_segments(c, ctx, track, (status.power_profile, draw.motion.profile_position()));
    }
}

fn draw_qs_segments(
    c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, track: LocalRect, (active, position): (Option<PowerProfile>, f64),
) {
    let theme = ctx.theme;
    c.fill(track, theme.track_soft, R2);
    let segments = qs_segments(track);
    if active.is_some() {
        let (first, second) = (segments[0].1, segments[1].1);
        let step = f64::from(second.loc.x - first.loc.x);
        let x = first.loc.x + (step * position.clamp(0.0, 2.0)).round() as i32;
        c.fill(local_rect(x, first.loc.y, first.size.w, first.size.h), theme.track, R1);
    }
    for (profile, rect) in segments {
        let lit = active == Some(profile) || ctx.hovered == Some(Slot::QsProfile(profile));
        let colour = if lit { theme.label } else { theme.muted };
        let label = crate::i18n::t(power_profile_label(profile));
        let content = QS_SEGMENT_ICON + QS_SEGMENT_GAP + text_w(ctx.painter, label, &QS_LABEL);
        let x = (rect.loc.x + (rect.size.w - content) / 2).max(rect.loc.x + SP1);
        let icon = local_rect(x, rect.loc.y + (rect.size.h - QS_SEGMENT_ICON) / 2, QS_SEGMENT_ICON, QS_SEGMENT_ICON);
        qs_symbol(c, icon, (power_profile_icon(profile), crate::paint::draw_power_glyph_over), QS_SEGMENT_ICON, colour);
        let text_left = x + QS_SEGMENT_ICON + QS_SEGMENT_GAP;
        let band = (text_left, rect.loc.x + rect.size.w - SP1, rect.loc.y, rect.loc.y + rect.size.h);
        c.text(ctx.painter, band, TextAlign::Left, label, colour, &QS_LABEL);
    }
}

fn draw_quick_settings(c: &mut Canvas<'_>, ctx: &QsRenderContext<'_>, draw: &QsDraw<'_>) {
    draw_qs_header(c, ctx, &draw.geometry, &draw.status.user_name);
    draw_qs_tiles(c, ctx, draw);
    draw_qs_sliders(c, ctx, draw);
    draw_qs_battery_card(c, ctx, draw);
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
            album: "Золоті шедеври".to_string(),
            length: Some(Duration::from_secs(212)),
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
    fn the_island_sits_in_the_middle_of_the_bar_with_the_player_left_of_the_clock() {
        let silent = topbar_with(&["US", "UA"]);
        assert!(silent.slot_rect(Slot::IslandMedia).is_none());
        let clock = silent.slot_rect(Slot::Clock).expect("the clock is always there");
        let island = silent.island_target(output());
        assert_eq!(silent.island.rect(silent.rect(output())), island, "the first layout is taken without a morph");
        assert!(inside(island, clock));
        assert_eq!(island.loc.y, island::TOP);
        assert_eq!(island.loc.x + island.size.w / 2, output().size.w / 2, "the island is centred");

        let bar = playing_bar();
        let island = bar.island_target(output());
        let media = bar.slot_rect(Slot::IslandMedia).expect("a player button while something plays");
        let clock = bar.slot_rect(Slot::Clock).expect("clock");
        assert!(inside(island, media) && inside(island, clock));
        assert!(media.loc.x + media.size.w < clock.loc.x);
        assert!(island.size.w > silent.island_target(output()).size.w);
        assert_eq!(click(&bar, Slot::IslandMedia), Some(TopbarAction::ToggleMediaPlayer));
        assert_eq!(click(&bar, Slot::Clock), Some(TopbarAction::ToggleNotificationCenter));
    }

    #[test]
    fn the_player_needs_a_track_and_seeks_where_the_pointer_lands() {
        let mut silent = topbar_with(&["US", "UA"]);
        assert!(!silent.toggle_popup(OpenPopup::MediaPlayer), "silence has no player to show");
        silent.layout_slots(&painter(), output());
        assert!(silent.popup_rect().is_none());

        let bar = with_popup(playing_bar(), OpenPopup::MediaPlayer);
        let popup = bar.popup_rect().expect("popup rect");
        assert_eq!((popup.loc.y, popup.size.w), (island::TOP, island::WIDE), "the island grows in place");
        assert_eq!(popup.loc.x + popup.size.w / 2, output().size.w / 2);
        for slot in [
            Slot::PopMediaPrev,
            Slot::PopMediaPlay,
            Slot::PopMediaNext,
            Slot::PopMediaSeek,
            Slot::IslandToNotifications,
            Slot::IslandCollapse,
        ] {
            assert!(inside(popup, bar.slot_rect(slot).expect("popup slot")), "{slot:?}");
        }
        assert!(bar.slot_rect(Slot::Clock).is_none(), "the compact controls give way to the player");
        let track = bar.slot_rect(Slot::PopMediaSeek).expect("seek slot");
        let y = centre(track).y;
        let middle = f64::from(track.loc.x) + f64::from(track.size.w) / 2.0;
        assert_eq!(bar.action_at(Point::from((middle, y))), Some(TopbarAction::MediaSeek(50)));
        let start = Point::from((f64::from(track.loc.x), y));
        assert_eq!(bar.action_at(start), Some(TopbarAction::MediaSeek(0)));
        assert_eq!(click(&bar, Slot::IslandCollapse), Some(TopbarAction::ClosePopup));
        assert_eq!(click(&bar, Slot::IslandToNotifications), Some(TopbarAction::ToggleNotificationCenter));
    }

    #[test]
    fn the_island_switches_between_its_views_without_closing() {
        let mut bar = with_popup(playing_bar(), OpenPopup::MediaPlayer);
        assert!(bar.toggle_popup(OpenPopup::NotificationCenter));
        bar.layout_slots(&painter(), output());
        assert!(bar.has_open_popup());
        assert!(bar.slot_rect(Slot::IslandToMedia).is_some(), "with a player the calendar offers it");
        assert!(bar.slot_rect(Slot::PopMediaPlay).is_none());
        assert!(bar.set_media(None));
        bar.layout_slots(&painter(), output());
        assert!(bar.slot_rect(Slot::IslandToMedia).is_none());

        let mut player = with_popup(playing_bar(), OpenPopup::MediaPlayer);
        assert!(player.set_media(None));
        assert!(!player.has_open_popup(), "the player closes when the track goes away");
    }

    #[test]
    fn a_new_notification_flashes_in_the_island_and_opens_the_centre() {
        let mut bar = topbar_with(&["US", "UA"]);
        let start = std::time::Instant::now();
        let alert = IslandAlert { id: 1, heading: "Telegram".into(), summary: "Olena: hi".into(), icon: None };
        assert!(bar.show_alert(alert, start));
        bar.layout_slots(&painter(), output());
        let close = bar.slot_rect(Slot::IslandAlertClose).expect("close button");
        let below = Point::from((centre(close).x, f64::from(HEIGHT) + 6.0));
        assert!(bar.contains(output(), below), "the alert is taller than the bar and still takes clicks");
        assert_eq!(click(&bar, Slot::IslandAlertClose), Some(TopbarAction::DismissIslandAlert));
        assert_eq!(click(&bar, Slot::IslandAlert), Some(TopbarAction::ToggleNotificationCenter));
        assert!(bar.slot_rect(Slot::Clock).is_none());
        assert!(!bar.tick_island(start + island::ALERT_HOLD / 2));
        assert!(bar.tick_island(start + island::ALERT_HOLD));
        assert!(bar.alert().is_none());
        bar.layout_slots(&painter(), output());
        assert!(bar.slot_rect(Slot::Clock).is_some());

        let alert = IslandAlert { id: 2, heading: "Mail".into(), summary: String::new(), icon: None };
        bar.show_alert(alert, start);
        assert!(bar.toggle_popup(OpenPopup::NotificationCenter));
        assert!(bar.alert().is_none(), "opening the centre answers the alert");
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

        let painter = painter();
        let chip = PAD_X + app_chip_width(&painter, &bar.app_name, true);
        assert!(chip < bar.island_target(output()).loc.x, "the app chip never reaches the island");
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
        let coffee = two.slot_rect(Slot::Caffeine).expect("caffeine slot");
        assert_eq!(control.loc.x + control.size.w + ITEM_GAP, coffee.loc.x);

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
        assert_eq!(app_display_name("org.gnome.Nautilus", "Documents"), "Documents");
        assert_eq!(app_display_name("org.gnome.Nautilus", ""), "Nautilus");
        assert_eq!(app_display_name("kitty", ""), "Kitty");
        assert_eq!(app_display_name("kitty", "fish ~"), "fish ~");
        assert_eq!(app_display_name("", "Some Window"), "Some Window");
        let shown = app_display_name(&"a".repeat(80), "");
        assert!(shown.chars().count() <= APP_NAME_MAX && shown.ends_with('…'));
    }

    fn clock_box(bar: &mut Topbar) -> i32 {
        island::clock_width(bar.island_clock(&painter()))
    }

    #[test]
    fn the_clock_reserves_room_for_both_the_date_and_the_time() {
        let mut with_date = Topbar::new(TopbarTheme { clock_24h: Some(true), ..TopbarTheme::default() });
        with_date.set_now(at(21, 1, 8, 12));
        let mut without =
            Topbar::new(TopbarTheme { clock_date: false, clock_24h: Some(true), ..TopbarTheme::default() });
        without.set_now(at(21, 1, 8, 12));
        assert!(clock_box(&mut with_date) > clock_box(&mut without));
        assert_eq!(widest_digits("12:34"), "88:88");
    }

    fn at(day: u32, weekday: u32, month0: u32, hour: u32) -> calendar::LocalNow {
        calendar::LocalNow { year: 2026, month0, day, weekday, hour, minute: 5 }
    }

    #[test]
    fn the_clock_box_holds_its_width_across_days_months_and_hours() {
        let mut bar = topbar_with(&["US", "UA"]);
        let mut widths = Vec::new();
        for (day, weekday, month0) in [(24, 4, 8), (1, 3, 0), (31, 5, 4), (7, 0, 10)] {
            for hour in [0, 9, 13, 23] {
                bar.set_now(at(day, weekday, month0, hour));
                widths.push(clock_box(&mut bar));
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
        for style in [island::TIME_STYLE, status::PCT_STYLE] {
            assert!(style.tabular);
            assert!(!style.uppercase);
            assert!(style.tracking_em.abs() < f32::EPSILON, "no letter-spacing between digits");
        }
        assert!((island::TIME_STYLE.size_px - island::DATE_STYLE.size_px).abs() < f32::EPSILON);
    }

    #[test]
    fn a_battery_reading_keeps_its_width_within_a_digit_count() {
        let painter = painter();
        let reference = numeric_label_width(&painter, "88%", &status::PCT_STYLE);
        for pct in ["11%", "47%", "90%"] {
            assert_eq!(numeric_label_width(&painter, pct, &status::PCT_STYLE), reference, "{pct}");
        }
    }

    #[test]
    fn an_unread_notification_count_expands_the_clock() {
        let mut bar = topbar_with(&["US", "UA"]);
        let without_unread = clock_box(&mut bar);
        bar.set_unread(3);
        assert!(clock_box(&mut bar) > without_unread);
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
            Slot::QsDnd,
            Slot::QsDarkTheme,
            Slot::QsVolumeMute,
            Slot::QsBrightnessTrack,
            Slot::QsSettings,
            Slot::QsWifiMore,
            Slot::QsBluetoothMore,
            Slot::QsVolumeMore,
            Slot::Caffeine,
        ]
        .into_iter()
        .chain(panels::POWER_ROW.map(Slot::QsPowerButton))
        {
            assert!(bar.slot_rect(slot).is_some(), "{slot:?}");
        }
        assert!(bar.slot_rect(Slot::QsProfile(PowerProfile::Balanced)).is_none(), "no profiles daemon, no segments");
        for (slot, action) in [
            (Slot::QsWifi, TopbarAction::QsToggleWifi),
            (Slot::QsWifiMore, TopbarAction::QsPanel(PanelAction::Open(Detail::Wifi))),
            (Slot::QsScreenshot, TopbarAction::QsScreenshot),
            (Slot::QsCaffeine, TopbarAction::ToggleCaffeine),
            (Slot::QsDnd, TopbarAction::ToggleDoNotDisturb),
            (Slot::QsDarkTheme, TopbarAction::QsToggleDarkTheme),
            (Slot::QsVolumeMore, TopbarAction::QsPanel(PanelAction::Open(Detail::Sound))),
            (Slot::QsAirplane, TopbarAction::QsToggleAirplane),
            (Slot::QsSettings, TopbarAction::QsOpenSettings),
            (Slot::QsPowerButton(PowerAction::Lock), TopbarAction::QsArmPower(PowerAction::Lock)),
        ] {
            assert_eq!(click(&bar, slot), Some(action), "{slot:?}");
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
    fn a_control_the_machine_lacks_has_no_slider_to_drag() {
        let mut bar = topbar_with(&["US"]);
        bar.set_system_status(crate::system_info::SystemStatus {
            brightness_available: false,
            ..crate::system_info::SystemStatus::default()
        });
        let bar = with_popup(bar, OpenPopup::QuickSettings);
        assert!(bar.slot_rect(Slot::QsBrightnessTrack).is_none(), "a desktop without a backlight");
        assert!(bar.slot_rect(Slot::QsVolumeTrack).is_some());

        let mut bar = topbar_with(&["US"]);
        bar.set_system_status(crate::system_info::SystemStatus {
            volume_available: false,
            ..crate::system_info::SystemStatus::default()
        });
        let bar = with_popup(bar, OpenPopup::QuickSettings);
        for slot in [Slot::QsVolumeTrack, Slot::QsVolumeMute, Slot::QsVolumeMore] {
            assert!(bar.slot_rect(slot).is_none(), "{slot:?} without wpctl or pactl");
        }
    }

    #[test]
    fn the_slider_hit_area_matches_the_drawn_track() {
        for shape in [QsShape::default(), QsShape { mic: true, ..QsShape::default() }] {
            let geometry = qs_geometry(&shape);
            let card = geometry.sliders;
            let rows: Vec<QsSliderRow> =
                std::iter::once(geometry.volume).chain(geometry.mic).chain([geometry.brightness]).collect();
            assert_eq!(rows.len(), 2 + usize::from(shape.mic));
            for row in &rows {
                assert_eq!(row.track.loc.x, card.loc.x + 1 + QS_CARD_PAD_X, "0% sits at the card's content edge");
                let track_right = row.track.loc.x + row.track.size.w;
                let head_right = row.drill.map_or(row.readout.loc.x + row.readout.size.w, |drill| {
                    assert_eq!(drill.loc.x, row.readout.loc.x + row.readout.size.w + SP1);
                    drill.loc.x + drill.size.w
                });
                assert_eq!(track_right, head_right, "the head row ends where the track does");
                assert_eq!(row.track.loc.y, row.icon.loc.y + QS_SLIDER_HEAD + SP1, "the track sits under its head");
                assert!(card.contains(row.track.loc) && card.contains(row.icon.loc));
                assert!(row.label.loc.x + row.label.size.w <= row.readout.loc.x);
            }
            for pair in rows.windows(2) {
                let gap = pair[1].icon.loc.y - (pair[0].track.loc.y + QS_SLIDER_HEIGHT);
                assert_eq!(gap, QS_CARD_PAD_Y * 2 + 1, "a hairline between padded rows");
            }
            let last = rows[rows.len() - 1];
            assert_eq!(card.loc.y + card.size.h, last.track.loc.y + QS_SLIDER_HEIGHT + QS_CARD_PAD_Y + 1);
        }
    }

    #[test]
    fn the_quick_settings_popup_is_exactly_as_tall_as_its_content() {
        let pills = QsPills { output: 60, input: 70, cancel: 80, confirm: 100 };
        let plain = QsShape { pills, ..QsShape::default() };
        let battery = QsShape { battery: true, profile: true, ..plain };
        let lock = QsShape { armed: Some(PowerAction::Lock), ..battery };
        let shut_down = QsShape { armed: Some(PowerAction::ShutDown), ..battery };
        for shape in [plain, battery, lock, shut_down] {
            let geometry = qs_geometry(&shape);
            let session = geometry.confirm.map_or(geometry.power_row, |strip| strip.rect);
            assert_eq!(geometry.height, session.loc.y + session.size.h + QS_PAD, "{shape:?}");
            let rects = geometry
                .tiles
                .iter()
                .map(|(_, rect)| *rect)
                .chain(geometry.power.iter().map(|(_, rect)| *rect))
                .chain([geometry.avatar, geometry.settings, geometry.sliders, session])
                .chain(geometry.battery_card);
            for rect in rects {
                assert!(rect.loc.x >= QS_PAD && rect.loc.x + rect.size.w <= QS_WIDTH - QS_PAD, "{rect:?}");
                assert!(rect.loc.y >= QS_PAD && rect.loc.y + rect.size.h <= geometry.height - QS_PAD, "{rect:?}");
            }
        }
        let card = |shape: QsShape| qs_geometry(&shape).battery_card.map(|card| card.size.h);
        assert_eq!(card(plain), None);
        assert_eq!(card(battery), Some(2 + SP1 * 3 + QS_BATTERY_HEAD + QS_SEGMENTS_H));
        assert_eq!(card(QsShape { profile: false, ..battery }), Some(2 + SP1 * 2 + QS_BATTERY_HEAD));
        assert_eq!(card(QsShape { battery: false, ..battery }), Some(2 + SP1 * 2 + QS_SEGMENTS_H));
        let strip = |shape: QsShape| qs_geometry(&shape).confirm.map(|strip| strip.rect.size.h);
        assert!(strip(shut_down) > strip(lock), "the countdown line takes a row");
        assert!(strip(lock).is_some_and(|h| h > QS_POWER_ROW));

        let mut bar = topbar_with(&["US", "UA"]);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        let popup = bar.popup_rect().expect("popup rect");
        assert_eq!(popup.size, Size::from((QS_WIDTH, qs_geometry(&bar.qs_shape(None)).height)));
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
            tops.push((popup, bar.popup_rect().expect("popup rect").loc.y));
        }
        for (popup, top) in tops {
            let want = if Topbar::island_owns(Some(popup)) { island::TOP } else { HEIGHT + POPUP_GAP };
            assert_eq!(top, want, "{popup:?}: the island grows from where it sits, the rest hang below");
        }
    }

    #[test]
    fn a_theme_built_from_light_tokens_is_not_dark() {
        let tokens = glide_theme::Tokens::new(glide_theme::Appearance::Light, glide_theme::Accent::Blue);
        let theme = TopbarTheme::from_tokens(&tokens);
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
    fn the_battery_card_sits_under_the_sliders_and_opens_its_view() {
        let mut bar = topbar_with(&["US"]);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        assert_eq!(bar.slot_rect(Slot::QsBattery), None, "no battery, no card");

        bar.set_system_status(crate::system_info::SystemStatus { battery_pct: Some(50), ..Default::default() });
        bar.layout_slots(&painter(), output());
        let head = bar.slot_rect(Slot::QsBattery).expect("battery row");
        let brightness = bar.slot_rect(Slot::QsBrightnessTrack).expect("brightness");
        let lock = bar.slot_rect(Slot::QsPowerButton(PowerAction::Lock)).expect("lock");
        assert!(brightness.loc.y + brightness.size.h < head.loc.y && head.loc.y + head.size.h < lock.loc.y);
        assert_eq!(head.size.h, QS_BATTERY_HEAD);
        assert!(bar.popup_rect().expect("qs").contains(head.loc));
        assert_eq!(click(&bar, Slot::QsBattery), Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Battery))));

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
        let quiet = bar.slot_rect(Slot::NcDndToggle).expect("do not disturb");
        let collapse = bar.slot_rect(Slot::IslandCollapse).expect("collapse");
        assert!(popup.contains(month.loc) && popup.contains(next.loc));
        assert!(collapse.loc.y + collapse.size.h <= month.loc.y, "the header sits above the calendar");
        assert!(month.loc.y + month.size.h < quiet.loc.y);
        assert!(quiet.loc.y - month.loc.y >= calendar::BLOCK_H);
        assert!(bar.slot_rect(Slot::NcClearAll).is_none(), "nothing to clear");
        assert!(bar.slot_rect(Slot::IslandToMedia).is_none(), "nothing playing");
    }

    #[test]
    fn arrows_on_the_month_title_page_months_and_enter_returns_to_today() {
        let mut bar = topbar_with(&["US"]);
        bar.set_now(at(24, 4, 8, 12));
        bar.toggle_popup(OpenPopup::NotificationCenter);
        bar.layout_slots(&painter(), output());
        let today = bar.calendar.month();
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::IslandCollapse), "the header comes first");
        bar.set_focused(Some(Slot::CalMonth));
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
            (0, 1, Slot::QsSettings, Some(TopbarAction::QsOpenSettings)),
            (0, 1, Slot::QsBluetoothMore, Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Bluetooth)))),
            (-1, 0, Slot::QsBluetooth, Some(TopbarAction::QsToggleBluetooth)),
            (-1, 0, Slot::QsWifiMore, Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Wifi)))),
            (-1, 0, Slot::QsWifi, Some(TopbarAction::QsToggleWifi)),
        ] {
            assert!(bar.focus_nav(dx, dy));
            assert_eq!(bar.focused, Some(slot));
            assert_eq!(bar.focused_action(), action);
        }
        assert!(!bar.focus_nav(-1, 0), "nothing lies left of the first tile");
        assert!(!bar.focus_nav(0, -1), "nothing lies above the first tile but the header's far end");
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::QsNightLight));
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused_action(), Some(TopbarAction::ToggleDoNotDisturb));
        assert!(!bar.focus_nav(1, 0), "the grid has two columns");
        assert!(bar.close_popup());
        assert!(!bar.has_focus() && bar.focused_action().is_none());

        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        bar.toggle_popup(OpenPopup::QuickSettings);
        assert!(!bar.has_focus(), "toggling the popup shut drops focus");
    }

    #[test]
    fn the_netbird_tile_joins_the_grid_only_when_netbird_is_installed() {
        let mut bar = quick_settings();
        let plain = qs_geometry(&bar.qs_shape(None)).height;
        assert!(bar.slot_rect(Slot::QsNetbird).is_none() && bar.slot_rect(Slot::QsNetbirdMore).is_none());
        bar.set_netbird_installed(true);
        bar.layout_slots(&painter(), output());
        let taller = qs_geometry(&bar.qs_shape(None)).height;
        assert_eq!(taller - plain, QS_TILE_HEIGHT + QS_GRID_GAP, "a ninth tile opens a fifth row");
        assert!(bar.slot_rect(Slot::QsNetbird).is_some() && bar.slot_rect(Slot::QsNetbirdMore).is_some());
        assert_eq!(Slot::QsNetbird.action(), TopbarAction::QsToggleNetbird);
        assert_eq!(Slot::QsNetbirdMore.action(), TopbarAction::QsPanel(PanelAction::Open(Detail::Netbird)));
        let (on, subtitle) = bar.qs_panel.netbird.tile();
        assert!(!on && subtitle == crate::i18n::t(crate::i18n::Key::QsOff), "no status yet reads as off");
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
                Slot::QsSettings,
                Slot::QsBluetoothMore,
                Slot::QsDnd,
                Slot::QsAirplane,
                Slot::QsDarkTheme,
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
        bar.set_focused(Some(Slot::QsPowerButton(PowerAction::Restart)));
        assert!(bar.focus_nav(0, -1));
        assert_eq!(bar.focused, Some(Slot::QsBrightnessTrack));
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
        assert_eq!(bar.focused, Some(Slot::IslandCollapse), "the header heads the island");
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::CalNext), "Down leaves the header for the calendar");
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
        let slot = bar.slot_rect(Slot::QsSettings).expect("settings slot");
        let ring = bar.focus_ring_in(Slot::in_quick_settings).expect("ring for the focused button");
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
        assert_eq!(bar.focused_in(Slot::in_quick_settings), Some(Slot::QsSettings));
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
        let tile = bar.slot_rect(Slot::QsNightLight).expect("night light tile");
        bar.set_hover(Some(centre(tile)));
        std::thread::sleep(motion::t1() + Duration::from_millis(20));
        let jobs = bar.layer_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].layer, Layer::Hover);
        assert_eq!(jobs[0].rect, tile);
        assert!(jobs[0].in_popup);
        assert!((jobs[0].alpha - 1.0).abs() < f32::EPSILON);

        bar.set_focused(Some(Slot::QsNightLight));
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
        assert_eq!(Some(ring), bar.slot_rect(Slot::QsSettings));
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
    fn a_power_button_arms_an_inline_confirmation_in_place_of_the_row() {
        let mut bar = quick_settings();
        let restart = bar.slot_rect(Slot::QsPowerButton(PowerAction::Restart)).expect("restart");
        let height = bar.popup_rect().expect("popup").size.h;
        assert!(bar.arm_power(PowerAction::Restart));
        assert!(!bar.arm_power(PowerAction::Restart), "already armed");
        bar.layout_slots(&painter(), output());
        assert!(bar.slot_rect(Slot::QsPowerButton(PowerAction::Restart)).is_none(), "the row gives way");
        let confirm = bar.slot_rect(Slot::QsPowerConfirm(PowerAction::Restart)).expect("confirm pill");
        let cancel = bar.slot_rect(Slot::QsPowerCancel).expect("cancel pill");
        assert!(cancel.loc.x + cancel.size.w < confirm.loc.x);
        assert!(confirm.loc.y > restart.loc.y);
        assert!(bar.popup_rect().expect("popup").size.h > height);
        assert_eq!(
            click(&bar, Slot::QsPowerConfirm(PowerAction::Restart)),
            Some(TopbarAction::QsPower(PowerAction::Restart))
        );
        assert_eq!(click(&bar, Slot::QsPowerCancel), Some(TopbarAction::QsDisarmPower));
        assert!(bar.is_animating(), "the strip fades in");
        assert!(bar.qs_escape(), "Esc disarms before it closes");
        assert_eq!(bar.armed_power(), None);
        assert!(!bar.qs_escape());
        assert!(bar.has_open_popup());
    }

    #[test]
    fn only_the_session_ending_actions_count_down() {
        let mut bar = quick_settings();
        bar.arm_power(PowerAction::Lock);
        assert!(!bar.power_counting());
        let later = std::time::Instant::now() + crate::power_dialog::COUNTDOWN * 2;
        assert_eq!(bar.power_due(later), None, "lock waits for a click");
        bar.arm_power(PowerAction::ShutDown);
        let now = std::time::Instant::now();
        assert!(bar.power_counting());
        assert_eq!(bar.power_due(now), None);
        assert_eq!(bar.qs_power.and_then(|armed| armed.seconds_left(now)), Some(60));
        let due = now + crate::power_dialog::COUNTDOWN + Duration::from_millis(5);
        assert_eq!(bar.power_due(due), Some(PowerAction::ShutDown));
        assert_eq!(bar.armed_power(), None);
    }

    #[test]
    fn leaving_the_main_view_forgets_an_armed_action() {
        let mut bar = quick_settings();
        bar.set_focused(Some(Slot::QsPowerButton(PowerAction::Sleep)));
        bar.arm_power(PowerAction::Sleep);
        assert_eq!(bar.focused, Some(Slot::QsPowerConfirm(PowerAction::Sleep)), "Enter on the button lands on Confirm");
        assert!(bar.disarm_power());
        assert_eq!(bar.focused, Some(Slot::QsPowerButton(PowerAction::Sleep)));

        bar.arm_power(PowerAction::LogOut);
        assert!(bar.open_detail(Detail::Wifi));
        assert_eq!(bar.armed_power(), None);
        bar.close_detail();
        bar.arm_power(PowerAction::LogOut);
        bar.close_popup();
        assert!(!bar.power_counting(), "a closed popup never ends the session");
        bar.toggle_popup(OpenPopup::QuickSettings);
        assert_eq!(bar.armed_power(), None);
    }

    #[test]
    fn the_power_profile_is_a_segmented_control_under_the_battery() {
        let mut bar = topbar_with(&["US"]);
        bar.set_system_status(crate::system_info::SystemStatus {
            battery_pct: Some(70),
            power_profile: Some(PowerProfile::Balanced),
            ..Default::default()
        });
        assert!((bar.profile_anim.value() - 1.0).abs() < f64::EPSILON, "the first reading jumps");
        let bar = with_popup(bar, OpenPopup::QuickSettings);
        let head = bar.slot_rect(Slot::QsBattery).expect("battery row");
        let mut previous: Option<Rectangle<i32, Logical>> = None;
        for profile in QS_PROFILES {
            let rect = bar.slot_rect(Slot::QsProfile(profile)).expect("segment");
            assert!(rect.loc.y > head.loc.y + head.size.h);
            if let Some(previous) = previous {
                assert!(previous.loc.x + previous.size.w <= rect.loc.x);
                assert!((previous.size.w - rect.size.w).abs() <= 1);
            }
            previous = Some(rect);
            assert_eq!(click(&bar, Slot::QsProfile(profile)), Some(TopbarAction::QsSetPowerProfile(profile)));
        }

        let mut bar = bar;
        let mut status = bar.system_status().clone();
        status.power_profile = Some(PowerProfile::Performance);
        bar.set_system_status(status);
        assert!(bar.profile_anim.is_animating());
        assert!((bar.profile_anim.target() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_microphone_adds_its_row_between_volume_and_brightness() {
        let bar = quick_settings();
        assert!(bar.slot_rect(Slot::QsMicTrack).is_none(), "no source, no row");
        let without = bar.popup_rect().expect("popup").size.h;

        let mut bar = topbar_with(&["US"]);
        bar.set_system_status(crate::system_info::SystemStatus {
            mic_available: true,
            mic_pct: 30,
            ..Default::default()
        });
        let mut bar = with_popup(bar, OpenPopup::QuickSettings);
        let volume = bar.slot_rect(Slot::QsVolumeTrack).expect("volume");
        let mic = bar.slot_rect(Slot::QsMicTrack).expect("mic");
        let brightness = bar.slot_rect(Slot::QsBrightnessTrack).expect("brightness");
        assert!(volume.loc.y < mic.loc.y && mic.loc.y < brightness.loc.y);
        assert!(bar.popup_rect().expect("popup").size.h > without);
        assert_eq!(click(&bar, Slot::QsMicMute), Some(TopbarAction::QsMicMute));
        let quarter =
            Point::from((f64::from(mic.loc.x) + f64::from(mic.size.w) * 0.25, f64::from(mic.loc.y + mic.size.h / 2)));
        assert_eq!(bar.action_at(quarter), Some(TopbarAction::QsMicAdjust(25)));
        bar.set_focused(Some(Slot::QsMicTrack));
        assert_eq!(bar.focus_adjust(-5), Some(TopbarAction::QsMicAdjust(25)));
        assert_eq!(bar.system_status().mic_pct, 25);
        bar.set_focused(Some(Slot::QsVolumeTrack));
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::QsMicTrack));
        assert!(Slot::QsMicTrack.is_slider() && !Slot::QsMicMute.is_slider());
    }

    fn device(name: &str, active: bool, icon: &'static str) -> crate::qs_net::AudioOutput {
        crate::qs_net::AudioOutput { name: name.to_string(), description: name.to_uppercase(), icon, active }
    }

    fn with_devices() -> Topbar {
        let mut bar = topbar_with(&["US"]);
        bar.set_system_status(crate::system_info::SystemStatus { mic_available: true, ..Default::default() });
        let mut bar = with_popup(bar, OpenPopup::QuickSettings);
        bar.apply_net_event(crate::qs_net::NetEvent::Outputs(vec![
            device("speakers", true, "audio-speakers"),
            device("hdmi", false, "video-display"),
        ]));
        bar.apply_net_event(crate::qs_net::NetEvent::Inputs(vec![device("mic", true, "audio-input-microphone")]));
        bar.layout_slots(&painter(), output());
        bar
    }

    #[test]
    fn a_device_pill_unfolds_its_list_inside_the_sliders_card() {
        let mut bar = with_devices();
        assert_eq!(click(&bar, Slot::QsOutputPill), Some(TopbarAction::QsToggleDrawer(Drawer::Output)));
        assert_eq!(click(&bar, Slot::QsInputPill), Some(TopbarAction::QsToggleDrawer(Drawer::Input)));
        assert_eq!(click(&bar, Slot::QsVolumeMore), Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Sound))));
        assert_eq!(click(&bar, Slot::QsMicMore), Some(TopbarAction::QsPanel(PanelAction::Open(Detail::Sound))));
        let mic_before = bar.slot_rect(Slot::QsMicTrack).expect("mic");
        let height_before = bar.popup_rect().expect("popup").size.h;

        assert!(bar.toggle_drawer(Drawer::Output));
        assert!(bar.is_animating(), "the drawer grows over t2");
        std::thread::sleep(motion::t2() + Duration::from_millis(20));
        bar.settle_motion();
        bar.layout_slots(&painter(), output());
        let full = qs_drawer_height(2);
        assert_eq!(bar.popup_rect().expect("popup").size.h, height_before + full);
        assert_eq!(bar.slot_rect(Slot::QsMicTrack).expect("mic").loc.y, mic_before.loc.y + full);
        let volume = bar.slot_rect(Slot::QsVolumeTrack).expect("volume");
        let first = bar.slot_rect(Slot::QsDrawerItem(Drawer::Output, 0)).expect("first device");
        let second = bar.slot_rect(Slot::QsDrawerItem(Drawer::Output, 1)).expect("second device");
        assert!(volume.loc.y + volume.size.h <= first.loc.y && first.loc.y + first.size.h <= second.loc.y);
        assert!(bar.slot_rect(Slot::QsDrawerItem(Drawer::Output, 2)).is_none());
        assert_eq!(
            click(&bar, Slot::QsDrawerItem(Drawer::Output, 1)),
            Some(TopbarAction::QsUseDevice(Drawer::Output, 1))
        );
        assert_eq!(bar.drawer_device(Drawer::Output, 1).map(|device| device.name), Some("hdmi".to_string()));

        assert!(bar.toggle_drawer(Drawer::Input), "one drawer at a time");
        assert_eq!(bar.drawer(), Some(Drawer::Input));
        bar.set_focused(Some(Slot::QsDrawerItem(Drawer::Input, 0)));
        assert!(bar.qs_escape(), "Esc folds the drawer first");
        assert_eq!(bar.drawer(), None);
        assert_eq!(bar.focused, Some(Slot::QsInputPill));
        assert!(bar.has_open_popup());
    }

    #[test]
    fn a_drawer_never_outlives_the_main_view() {
        let mut bar = with_devices();
        bar.toggle_drawer(Drawer::Output);
        assert!(bar.open_detail(Detail::Wifi));
        assert_eq!(bar.drawer(), None);
        assert!(!bar.toggle_drawer(Drawer::Output), "no drawers over a detail view");
        bar.close_detail();
        bar.toggle_drawer(Drawer::Input);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.toggle_popup(OpenPopup::QuickSettings);
        assert_eq!(bar.drawer(), None, "reopening starts folded");
        assert_eq!(qs_drawer_height(0), qs_drawer_height(1), "an empty list still shows its message row");
        assert_eq!(qs_drawer_height(3) - qs_drawer_height(2), QS_DRAWER_ITEM + QS_DRAWER_GAP);
    }

    #[test]
    fn every_quick_settings_surface_draws_without_icons_at_both_scales() {
        use crate::system_info::{BatteryHealth, BatteryInfo, BatteryState};
        let mut bar = with_devices();
        let mut status = bar.system_status().clone();
        status.battery_pct = Some(4);
        status.battery = Some(BatteryInfo {
            pct: 4,
            state: BatteryState::Discharging,
            minutes: Some(75),
            health: BatteryHealth {
                wear_pct: Some(76),
                cycles: Some(300),
                draw_dw: Some(84),
                full_dwh: Some(400),
                design_dwh: Some(520),
            },
            profile: Some(PowerProfile::Balanced),
        });
        status.power_profile = Some(PowerProfile::Balanced);
        status.privacy.microphone = vec!["Firefox".into()];
        status.volume_muted = true;
        status.wifi_enabled = true;
        status.bluetooth_on = true;
        bar.set_system_status(status);
        assert!(bar.toggle_drawer(Drawer::Output));
        std::thread::sleep(motion::t2() + Duration::from_millis(20));
        bar.settle_motion();
        assert!(bar.arm_power(PowerAction::ShutDown));
        bar.layout_slots(&painter(), output());

        let painter = painter();
        let theme = bar.theme.clone();
        let (output_label, input_label) = (bar.output_label(), bar.input_label());
        let (_, flags, motion, _, shape) = bar.qs_main_key(&painter);
        let geometry = qs_geometry(&shape);
        assert!(geometry.drawer.is_some() && geometry.confirm.is_some() && geometry.mic.is_some());
        let sliver = QsDrawerArea { drawer: Drawer::Output, rect: local_rect(25, 300, 302, 1) };
        let hovers =
            [None, Some(Slot::QsVolumeTrack), Some(Slot::QsWifiMore), Some(Slot::QsDrawerItem(Drawer::Output, 1))];
        for scale in [1, 2] {
            for hovered in hovers {
                let ctx = QsRenderContext { painter: &painter, theme: &theme, hovered };
                let mut canvas = Canvas::new(QS_WIDTH, geometry.height, scale).expect("main canvas");
                let flags = QsFlags { dragging: Some(Slot::QsMicTrack), ..flags };
                let labels = (output_label.as_str(), input_label.as_str());
                let geometry = geometry.clone();
                let draw =
                    QsDraw { status: bar.system_status(), panel: &bar.qs_panel, geometry, flags, motion, labels };
                draw_quick_settings(&mut canvas, &ctx, &draw);
                draw_qs_drawer(&mut canvas, &ctx, sliver, &bar.qs_panel);
            }
            let lit = Some(Slot::QsPowerButton(PowerAction::Restart));
            assert!(draw_qs_power_row(&theme, &geometry, lit, (scale, None)).is_some());
            let strip = geometry.confirm.expect("armed strip");
            let armed = (PowerAction::ShutDown, Some(60), Some(Slot::QsPowerCancel));
            assert!(draw_qs_confirm(&painter, &theme, &strip, armed, (scale, None)).is_some());
            assert!(
                draw_qs_confirm(&painter, &theme, &strip, (PowerAction::Lock, None, None), (scale, None)).is_some()
            );
            assert!(draw_qs_background((QS_WIDTH, geometry.height), &theme, scale).is_some());
        }
    }

    #[test]
    fn picking_a_device_keeps_the_focus_on_its_row() {
        let mut bar = with_devices();
        bar.toggle_drawer(Drawer::Output);
        bar.set_focused(Some(Slot::QsDrawerItem(Drawer::Output, 1)));
        bar.begin_connect("hdmi");
        assert_eq!(bar.focused, Some(Slot::QsDrawerItem(Drawer::Output, 1)));
        bar.close_drawer();
        assert!(bar.open_detail(Detail::Sound));
        bar.set_focused(Some(Slot::QsDrawerItem(Drawer::Input, 0)));
        bar.begin_connect("mic");
        assert_eq!(bar.focused, Some(Slot::QsDrawerItem(Drawer::Input, 0)), "a microphone card in the Sound view");
        bar.set_focused(Some(Slot::QsDetailRow(1)));
        bar.begin_connect("hdmi");
        assert_eq!(bar.focused, Some(Slot::QsDetailRow(1)));
        bar.begin_connect("nothing-like-it");
        assert_eq!(bar.focused, Some(Slot::QsDetailRow(1)), "an unknown key leaves the focus where it was");
    }

    #[test]
    fn a_new_battery_estimate_repaints_the_main_view() {
        let mut bar = quick_settings();
        let battery = crate::system_info::BatteryInfo {
            pct: 60,
            state: crate::system_info::BatteryState::Discharging,
            minutes: Some(90),
            health: crate::system_info::BatteryHealth::default(),
            profile: None,
        };
        bar.set_system_status(crate::system_info::SystemStatus {
            battery_pct: Some(60),
            battery: Some(battery),
            ..Default::default()
        });
        let before = bar.qs_main_key(&painter());
        let mut status = bar.system_status().clone();
        status.battery = Some(crate::system_info::BatteryInfo { minutes: Some(80), ..battery });
        bar.set_system_status(status);
        assert_ne!(bar.qs_main_key(&painter()), before, "the estimate is drawn on the main view");
    }

    #[test]
    fn the_badge_toggles_and_the_rest_of_a_split_tile_opens_its_view() {
        let bar = quick_settings();
        for (badge, card, toggle, detail) in [
            (Slot::QsWifi, Slot::QsWifiMore, TopbarAction::QsToggleWifi, Detail::Wifi),
            (Slot::QsBluetooth, Slot::QsBluetoothMore, TopbarAction::QsToggleBluetooth, Detail::Bluetooth),
        ] {
            let badge_rect = bar.slot_rect(badge).expect("badge");
            let card_rect = bar.slot_rect(card).expect("card");
            assert!(card_rect.contains(badge_rect.loc), "the badge sits inside its card");
            assert_eq!(bar.action_at(centre(badge_rect)), Some(toggle));
            let open = Some(TopbarAction::QsPanel(PanelAction::Open(detail)));
            assert_eq!(bar.action_at(centre(card_rect)), open);
            let chevron = Point::from((
                f64::from(card_rect.loc.x + card_rect.size.w - QS_SPLIT_END - QS_CHEVRON / 2),
                f64::from(card_rect.loc.y + card_rect.size.h / 2),
            ));
            assert_eq!(bar.action_at(chevron), open);
        }
    }
}
