use std::collections::HashMap;
use std::time::Duration;

use glide_theme::motion::{self, CubicBezier, E_IO, E_OUT, E_OVER};
use glide_ui::{Reveal, Tween};
use smithay::{
    backend::renderer::{
        ImportAll, ImportMem, Renderer,
        element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Size, Transform},
};

use crate::config::Color;

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

const NC_WIDTH: i32 = 380;
const NC_PAD: i32 = glide_theme::spacing::SP3;
const NC_HEADER_HEIGHT: i32 = 24;
const NC_GAP: i32 = glide_theme::spacing::SP2;
const NC_ITEM_HEIGHT: i32 = 54;
const NC_EMPTY_HEIGHT: i32 = 60;
const NC_MAX_ITEMS: usize = 6;

const STATUS_PAD_LEFT: i32 = 8;
const STATUS_PAD_RIGHT: i32 = 7;
const STATUS_GAP: i32 = 7;
const STATUS_WIFI_WIDTH: i32 = 15;
const STATUS_SPEAKER_WIDTH: i32 = 14;
const STATUS_BATT_WIDTH: i32 = 24;
const STATUS_CHEV_WIDTH: i32 = 9;
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
const QS_ACT_GAP: i32 = 4;
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

#[derive(Debug, Clone, Copy)]
struct Anim {
    tween: Tween,
    duration: Duration,
    ease: CubicBezier,
}

impl Anim {
    fn at(value: f64) -> Self {
        Self {
            tween: Tween::at(value),
            duration: Duration::ZERO,
            ease: E_OUT,
        }
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
}

impl Default for Anim {
    fn default() -> Self {
        Self::at(0.0)
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn quantize(value: f64, steps: f64) -> u8 {
    (value.clamp(0.0, 2.0) * steps).round() as u8
}

fn lerp(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * t
}

fn blend(from: Color, to: Color, t: f64) -> Color {
    #[allow(clippy::cast_possible_truncation)]
    let t = t.clamp(0.0, 1.0) as f32;
    let mut out = [0.0_f32; 4];
    for (index, channel) in out.iter_mut().enumerate() {
        *channel = from.0[index] + (to.0[index] - from.0[index]) * t;
    }
    Color(out)
}

fn switch_thumb_x(track_x: i32, width: i32, inset: i32, thumb: i32, t: f64) -> i32 {
    let off = f64::from(track_x + inset);
    let on = f64::from(track_x + width - inset - thumb);
    #[allow(clippy::cast_possible_truncation)]
    let x = lerp(off, on, t).round() as i32;
    x.clamp(track_x, track_x + width - thumb)
}

fn segment_pill(first: (i32, i32), second: (i32, i32), t: f64) -> (i32, i32) {
    let t = t.clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation)]
    let x = lerp(f64::from(first.0), f64::from(second.0), t).round() as i32;
    #[allow(clippy::cast_possible_truncation)]
    let w = lerp(f64::from(first.1), f64::from(second.1), t).round() as i32;
    (x, w)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPopup {
    NotificationCenter,
    LanguageMenu,
    QuickSettings,
    MediaPlayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopbarAction {
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
    QsLockSession,
    QsPowerOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
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
    QsLock,
    QsPower,
}

impl Slot {
    const fn in_strip(self) -> bool {
        matches!(
            self,
            Self::Clock
                | Self::Media
                | Self::MediaPrev
                | Self::MediaPlay
                | Self::MediaNext
                | Self::LayoutCycle
                | Self::LayoutPick(_)
                | Self::LayoutDropdown
                | Self::Caffeine
                | Self::DoNotDisturb
                | Self::Status
        )
    }

    const fn in_media(self) -> bool {
        matches!(
            self,
            Self::Media | Self::MediaPrev | Self::MediaPlay | Self::MediaNext
        )
    }

    const fn in_media_popup(self) -> bool {
        matches!(
            self,
            Self::PopMediaPrev | Self::PopMediaPlay | Self::PopMediaNext | Self::PopMediaSeek
        )
    }

    const fn in_layout(self) -> bool {
        matches!(
            self,
            Self::LayoutCycle | Self::LayoutPick(_) | Self::LayoutDropdown
        )
    }

    const fn in_lang_menu(self) -> bool {
        matches!(self, Self::LangMenuItem(_) | Self::LangMenuSettings)
    }

    const fn in_notification_center(self) -> bool {
        matches!(
            self,
            Self::NcClearAll | Self::NcDndToggle | Self::NcItem(_) | Self::NcItemDismiss(_)
        )
    }

    const fn in_quick_settings(self) -> bool {
        !self.in_strip()
            && !self.in_media_popup()
            && !self.in_lang_menu()
            && !self.in_notification_center()
    }

    const fn state_layer_radius(self) -> Option<f32> {
        match self {
            Self::NcDndToggle
            | Self::QsVolumeTrack
            | Self::QsBrightnessTrack
            | Self::PopMediaSeek => None,
            Self::MediaPrev | Self::MediaPlay | Self::MediaNext => Some(glide_theme::shape::R1),
            Self::LayoutPick(_) => Some(SEG_RADIUS),
            Self::PopMediaPlay | Self::NcItemDismiss(_) => Some(glide_theme::shape::R_ROUND),
            _ => Some(glide_theme::shape::R2),
        }
    }

    const fn action(self) -> TopbarAction {
        match self {
            Self::Clock => TopbarAction::ToggleNotificationCenter,
            Self::Media => TopbarAction::ToggleMediaPlayer,
            Self::MediaPlay | Self::PopMediaPlay => TopbarAction::MediaPlayPause,
            Self::MediaPrev | Self::PopMediaPrev => TopbarAction::MediaPrevious,
            Self::MediaNext | Self::PopMediaNext => TopbarAction::MediaNext,
            Self::PopMediaSeek => TopbarAction::MediaSeek(0),
            Self::LayoutCycle => TopbarAction::CycleLayout,
            Self::LayoutPick(index) | Self::LangMenuItem(index) => {
                TopbarAction::SelectLayout(index)
            }
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
            Self::QsLock => TopbarAction::QsLockSession,
            Self::QsPower => TopbarAction::QsPowerOff,
        }
    }
}

type ScaledBuffers = std::collections::HashMap<i32, MemoryRenderBuffer>;
type QsCacheKey = (u64, bool, bool, u8, u8, u8);
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
}

#[derive(Debug)]
struct Cache<K> {
    key: Option<K>,
    buffers: ScaledBuffers,
}

impl<K> Default for Cache<K> {
    fn default() -> Self {
        Self {
            key: None,
            buffers: ScaledBuffers::new(),
        }
    }
}

impl<K: PartialEq> Cache<K> {
    fn clear(&mut self) {
        self.key = None;
        self.buffers.clear();
    }

    fn resolve(
        &mut self,
        key: K,
        scale: i32,
        draw: impl FnOnce() -> Option<MemoryRenderBuffer>,
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
    power_armed: bool,

    hover_layers: HashMap<Slot, Anim>,
    press_layers: HashMap<Slot, Anim>,
    focus_anim: Anim,
    caffeine_anim: Anim,
    dnd_anim: Anim,
    volume_anim: Anim,
    brightness_anim: Anim,
    seek_anim: Anim,
    badge_anim: Anim,
    badge_count: usize,
    badge_reserved: bool,
    segment_anim: Anim,
    slider_dragging: bool,

    rev: u64,
    layouts_rev: u64,
    media_rev: u64,
    status_rev: u64,
    nc_rev: u64,

    slots: Vec<(Slot, Rectangle<i32, Logical>)>,
    layout_key: Option<(u64, Rectangle<i32, Logical>)>,
    popup_rect: Option<Rectangle<i32, Logical>>,
    panel: std::collections::HashMap<(i32, i32, u64), MemoryRenderBuffer>,
    shadows: std::collections::HashMap<(i32, i32, i32), MemoryRenderBuffer>,
    layers: HashMap<LayerKey, MemoryRenderBuffer>,
    separator_cache: Cache<()>,
    app_cache: Cache<(String, String)>,
    clock_cache: Cache<(String, String, usize, bool, u8)>,
    media_cache: Cache<(u64, Option<u16>)>,
    media_popup_cache: Cache<(u64, Option<u16>)>,
    layout_cache: Cache<(u64, usize, bool, u8)>,
    coffee_cache: Cache<(bool, u8)>,
    dnd_cache: Cache<bool>,
    status_cache: Cache<(u64, bool)>,
    qs_cache: Cache<QsCacheKey>,
    nc_cache: Cache<(u64, bool, u8)>,
    lang_menu_cache: Cache<(u64, usize)>,
    raster_rev: u64,
}

impl Topbar {
    #[must_use]
    pub fn new(theme: TopbarTheme) -> Self {
        Self {
            theme,
            ..Self::default()
        }
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
        self.status_cache.clear();
        self.qs_cache.clear();
        self.nc_cache.clear();
        self.lang_menu_cache.clear();
    }

    const fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    #[cfg(test)]
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
        self.focus_anim
            .retarget(if slot.is_some() { 1.0 } else { 0.0 }, motion::t1(), E_OUT);
    }

    #[must_use]
    pub const fn has_focus(&self) -> bool {
        self.focused.is_some()
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let popup = self.popup_reveal.is_animating(motion::t2());
        let layers = self
            .hover_layers
            .values()
            .chain(self.press_layers.values())
            .any(Anim::is_animating);
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
        popup || layers || controls
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
            self.popup_reveal.progress(duration, |t| E_OUT.ease_f64(t))
        } else {
            self.popup_reveal.progress(duration, |t| E_IO.ease_f64(t))
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
        if reserved != self.badge_reserved {
            self.badge_reserved = reserved;
            self.touch();
        }
        if let Some(rect) = self.focus_ring_rect() {
            self.focus_ring_last = Some(rect);
        }
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
        let Some(pred) = self.open_popup.map(popup_slots) else {
            return false;
        };
        let candidates: Vec<(Slot, Rectangle<i32, Logical>)> = self
            .slots
            .iter()
            .copied()
            .filter(|(slot, _)| pred(*slot))
            .collect();
        let current = self.focused.and_then(|focused| {
            candidates
                .iter()
                .find(|(slot, _)| *slot == focused)
                .map(|(_, rect)| *rect)
        });
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
        let current = glide_theme::Revision::new()
            .u64(self.theme.revision())
            .u64(crate::paint::painter_generation())
            .finish();
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

    pub fn set_caffeinated(&mut self, on: bool) {
        if self.caffeinated == on {
            return;
        }
        self.caffeinated = on;
        self.caffeine_anim
            .retarget(if on { 1.0 } else { 0.0 }, motion::t2(), E_OVER);
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
        self.dnd_anim
            .retarget(if on { 1.0 } else { 0.0 }, motion::t2(), E_OVER);
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
            self.badge_count = unread;
            self.badge_reserved = true;
            self.badge_anim.retarget(1.0, motion::t2(), E_OVER);
        } else {
            self.badge_anim.retarget(0.0, motion::t1(), E_OUT);
            self.badge_reserved = self.badge_anim.value() > 0.0;
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
        self.power_armed = false;
        self.touch();
        true
    }

    pub const fn arm_power_off(&mut self) -> bool {
        if self.power_armed {
            self.power_armed = false;
            return true;
        }
        self.power_armed = true;
        false
    }

    pub const fn disarm_power_off(&mut self) {
        self.power_armed = false;
    }

    #[must_use]
    pub const fn power_armed(&self) -> bool {
        self.power_armed
    }

    pub fn toggle_popup(&mut self, popup: OpenPopup) -> bool {
        if self.open_popup == Some(popup) {
            return self.close_popup();
        }
        self.open_popup = Some(popup);
        self.closing_popup = None;
        self.popup_reveal.open(motion::t2());
        self.set_focused(None);
        self.power_armed = false;
        self.touch();
        true
    }

    pub fn set_system_status(&mut self, status: crate::system_info::SystemStatus) -> bool {
        if self.system_status == status {
            return false;
        }
        let (volume, brightness) = (status.volume_pct, status.brightness_pct);
        self.system_status = status;
        self.slide(|bar| &mut bar.volume_anim, volume);
        self.slide(|bar| &mut bar.brightness_anim, brightness);
        self.status_rev = self.status_rev.wrapping_add(1);
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
        let progress = media
            .as_ref()
            .and_then(MediaWidget::progress_permille)
            .map_or(0.0, f64::from);
        self.media = media;
        self.seek_anim.jump(progress);
        self.media_rev = self.media_rev.wrapping_add(1);
        self.touch();
        true
    }

    fn seek_permille(&self) -> Option<u16> {
        self.media.as_ref()?.progress?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(self.seek_anim.value().clamp(0.0, 1000.0).round() as u16)
    }

    #[must_use]
    pub const fn has_media(&self) -> bool {
        self.media.is_some()
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
        if self.layouts != layouts {
            self.layouts_rev = self.layouts_rev.wrapping_add(1);
            self.segment_anim.jump(pill);
        } else {
            self.segment_anim.retarget(pill, motion::t2(), E_OUT);
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
        false
    }

    #[must_use]
    pub fn action_at(&self, pos: Point<f64, Logical>) -> Option<TopbarAction> {
        self.slots
            .iter()
            .find(|(_, rect)| rect.to_f64().contains(pos))
            .map(|(slot, rect)| match slot {
                Slot::QsVolumeTrack => {
                    let ratio =
                        ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // Ratio is clamped to 0.0..=1.0
                    let pct = (ratio * 100.0).round() as u8;
                    TopbarAction::QsVolumeAdjust(pct)
                }
                Slot::QsBrightnessTrack => {
                    let ratio =
                        ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // Ratio is clamped to 0.0..=1.0
                    let pct = (ratio * 100.0).round() as u8;
                    TopbarAction::QsBrightnessAdjust(pct)
                }
                Slot::PopMediaSeek => {
                    let ratio =
                        ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // Ratio is clamped to 0.0..=1.0
                    let pct = (ratio * 100.0).round() as u8;
                    TopbarAction::MediaSeek(pct)
                }
                other => other.action(),
            })
    }

    #[must_use]
    pub fn slot_at(&self, pos: Point<f64, Logical>) -> Option<Slot> {
        self.slots
            .iter()
            .find(|(_, rect)| rect.to_f64().contains(pos))
            .map(|(slot, _)| *slot)
    }

    pub fn set_hover(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        let hovered = pos.and_then(|pos| {
            self.slots
                .iter()
                .find(|(_, rect)| rect.to_f64().contains(pos))
                .map(|(slot, _)| *slot)
        });
        if self.hovered == hovered {
            return false;
        }
        if let Some(previous) = self.hovered.take()
            && let Some(layer) = self.hover_layers.get_mut(&previous)
        {
            layer.retarget(0.0, motion::t1(), E_IO);
        }
        if let Some(slot) = hovered.filter(|slot| slot.state_layer_radius().is_some()) {
            self.hover_layers
                .entry(slot)
                .or_default()
                .retarget(1.0, motion::t1(), E_IO);
        }
        self.hovered = hovered;
        true
    }

    fn segmented(&self) -> bool {
        self.layouts.len() == 2
    }

    fn layout_control_width(&self, painter: &crate::paint::Painter) -> i32 {
        if self.segmented() {
            let track: i32 = self
                .layouts
                .iter()
                .map(|name| segment_width(painter, name))
                .sum();
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
        self.layouts
            .get(self.layout_index)
            .map_or("", String::as_str)
    }

    fn status_button_width(&self, painter: &crate::paint::Painter) -> i32 {
        let mut width = STATUS_PAD_LEFT + STATUS_WIFI_WIDTH + STATUS_GAP + STATUS_SPEAKER_WIDTH;
        if let Some(pct) = self.system_status.battery_pct {
            let count_str = format!("{pct}%");
            #[allow(clippy::cast_possible_truncation)]
            let text_w = painter
                .text_width_styled(&count_str, &glide_theme::TextStyle::MICRO)
                .ceil() as i32;
            width += STATUS_GAP + STATUS_BATT_WIDTH + 5 + text_w;
        }
        width += STATUS_GAP + STATUS_CHEV_WIDTH + STATUS_PAD_RIGHT;
        width.max(BUTTON_MIN_WIDTH)
    }

    fn caffeine_width() -> i32 {
        #[allow(clippy::cast_possible_truncation)]
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

        let status_w = self.status_button_width(painter);
        right -= status_w;
        slots.push((
            Slot::Status,
            Rectangle::new(
                Point::from((right, y)),
                Size::from((status_w, BUTTON_HEIGHT)),
            ),
        ));

        right -= STATUS_SEP_WIDTH;

        let coffee_width = Self::caffeine_width();
        right -= coffee_width;
        slots.push((
            Slot::Caffeine,
            Rectangle::new(
                Point::from((right, y)),
                Size::from((coffee_width, BUTTON_HEIGHT)),
            ),
        ));

        let mut lang_rect = None;
        if !self.layouts.is_empty() {
            let width = self.layout_control_width(painter);
            right -= ITEM_GAP + width;
            let control_rect =
                Rectangle::new(Point::from((right, y)), Size::from((width, BUTTON_HEIGHT)));
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
            Rectangle::new(
                Point::from((right, y)),
                Size::from((DND_WIDTH, BUTTON_HEIGHT)),
            ),
        ));

        let clock_w = clock_width(painter, &self.date, &self.clock, self.badge_shown());
        let clock_x = strip.loc.x + (strip.size.w - clock_w) / 2;
        slots.push((
            Slot::Clock,
            Rectangle::new(
                Point::from((clock_x, y)),
                Size::from((clock_w, BUTTON_HEIGHT)),
            ),
        ));

        let media_rect = self
            .media
            .as_ref()
            .map(|media| layout_media_slots(painter, media, clock_x, y, &mut slots));

        let mut popup_slots = Vec::new();
        let popup_rect = match self.shown_popup() {
            Some(OpenPopup::LanguageMenu) => lang_rect.map(|btn| {
                layout_lang_menu_slots(&self.layouts, btn, strip.loc.y, &mut popup_slots)
            }),
            Some(OpenPopup::NotificationCenter) => Some(layout_nc_slots(
                &self.nc_items,
                output,
                strip.loc.y,
                &mut popup_slots,
            )),
            Some(OpenPopup::QuickSettings) => {
                Some(layout_qs_slots(output, strip.loc.y, &mut popup_slots))
            }
            Some(OpenPopup::MediaPlayer) => media_rect.map(|anchor| {
                layout_media_popup_slots(anchor, output, strip.loc.y, &mut popup_slots)
            }),
            None => None,
        };
        if self.popup_reveal.is_open() {
            slots.append(&mut popup_slots);
        }

        self.slots = slots;
        self.popup_rect = popup_rect;
    }

    fn badge_shown(&self) -> usize {
        if self.unread > 0 {
            self.unread
        } else if self.badge_reserved {
            self.badge_count
        } else {
            0
        }
    }

    #[must_use]
    pub fn slot_rect(&self, slot: Slot) -> Option<Rectangle<i32, Logical>> {
        self.slots
            .iter()
            .find(|(candidate, _)| *candidate == slot)
            .map(|(_, rect)| *rect)
    }

    fn layout_control_rect(&self) -> Option<Rectangle<i32, Logical>> {
        if self.segmented() {
            let first = self.slot_rect(Slot::LayoutPick(0))?;
            let last = self.slot_rect(Slot::LayoutPick(self.layouts.len() - 1))?;
            let x = first.loc.x - SEG_PAD_X - SEG_INSET;
            let width = (last.loc.x + last.size.w + SEG_INSET + SEG_PAD_X) - x;
            Some(Rectangle::new(
                Point::from((
                    x,
                    first.loc.y - SEG_INSET - (BUTTON_HEIGHT - SEG_HEIGHT) / 2,
                )),
                Size::from((width, BUTTON_HEIGHT)),
            ))
        } else if self.layouts.len() >= 3 {
            self.slot_rect(Slot::LayoutDropdown)
        } else {
            self.slot_rect(Slot::LayoutCycle)
        }
    }
}

fn layout_lang_menu_slots(
    layouts: &[String],
    btn: Rectangle<i32, Logical>,
    strip_y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let menu_w = LANG_MENU_WIDTH;
    let menu_x = btn.loc.x + btn.size.w - menu_w;
    let menu_y = strip_y + HEIGHT + POPUP_GAP;
    let count = i32::try_from(layouts.len()).unwrap_or(0);
    let menu_h = LANG_MENU_PAD * 2
        + count * LANG_MENU_ITEM_HEIGHT
        + LANG_MENU_DIVIDER
        + LANG_MENU_ITEM_HEIGHT;
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
    items: &[crate::notifications::NcItem],
    output: Rectangle<i32, Logical>,
    strip_y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let nc_w = NC_WIDTH;
    let nc_x = output.loc.x + (output.size.w - nc_w) / 2;
    let nc_y = strip_y + HEIGHT + POPUP_GAP;
    let count = items.len().min(NC_MAX_ITEMS);
    let content_h = if count == 0 {
        NC_EMPTY_HEIGHT
    } else {
        i32::try_from(count).unwrap_or(0) * NC_ITEM_HEIGHT
    };
    let nc_h = NC_PAD + NC_HEADER_HEIGHT + NC_GAP + content_h + NC_PAD;
    let rect = Rectangle::new(Point::from((nc_x, nc_y)), Size::from((nc_w, nc_h)));

    let head_y = nc_y + NC_PAD;
    let sw_x = nc_x + nc_w - NC_PAD - SWITCH_WIDTH;
    slots.push((
        Slot::NcDndToggle,
        Rectangle::new(
            Point::from((sw_x, head_y)),
            Size::from((SWITCH_WIDTH, NC_HEADER_HEIGHT)),
        ),
    ));

    let moon_x = sw_x - 8 - 14;
    let clear_w = 96;
    let clear_x = moon_x - 10 - clear_w;
    slots.push((
        Slot::NcClearAll,
        Rectangle::new(
            Point::from((clear_x, head_y)),
            Size::from((clear_w, NC_HEADER_HEIGHT)),
        ),
    ));

    let mut item_y = nc_y + NC_PAD + NC_HEADER_HEIGHT + NC_GAP;
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
            Rectangle::new(
                Point::from((nc_x + NC_PAD, item_y)),
                Size::from((nc_w - NC_PAD * 2, NC_ITEM_HEIGHT)),
            ),
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
    actions: [(Slot, LocalRect); 3],
    height: i32,
}

fn local_rect(x: i32, y: i32, w: i32, h: i32) -> LocalRect {
    Rectangle::new(Point::from((x, y)), Size::from((w, h)))
}

fn qs_geometry() -> QsGeometry {
    let row_w = QS_WIDTH - QS_PAD * 2;
    let col_w = (row_w - QS_GRID_GAP) / 2;
    let col2_x = QS_PAD + col_w + QS_GRID_GAP;
    let row1_y = QS_PAD;
    let row2_y = row1_y + QS_TILE_HEIGHT + QS_GRID_GAP;
    let tiles = [
        (
            Slot::QsWifi,
            local_rect(QS_PAD, row1_y, col_w, QS_TILE_HEIGHT),
        ),
        (
            Slot::QsBluetooth,
            local_rect(col2_x, row1_y, col_w, QS_TILE_HEIGHT),
        ),
        (
            Slot::QsNightLight,
            local_rect(QS_PAD, row2_y, col_w, QS_TILE_HEIGHT),
        ),
        (
            Slot::QsScreenshot,
            local_rect(col2_x, row2_y, col_w, QS_TILE_HEIGHT),
        ),
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
    let acts_w = QS_ACT_BTN * 3 + QS_ACT_GAP * 2;
    let acts_x = QS_PAD + row_w - acts_w;
    let action = |index: i32| {
        local_rect(
            acts_x + index * (QS_ACT_BTN + QS_ACT_GAP),
            footer_y,
            QS_ACT_BTN,
            QS_ACT_BTN,
        )
    };

    QsGeometry {
        tiles,
        caffeine,
        volume_icon: local_rect(QS_PAD, volume_y, QS_SLIDER_ICON, QS_SLIDER_HEIGHT),
        volume_track: local_rect(track_x, volume_y, track_w, QS_SLIDER_HEIGHT),
        brightness_icon: local_rect(QS_PAD, brightness_y, QS_SLIDER_ICON, QS_SLIDER_HEIGHT),
        brightness_track: local_rect(track_x, brightness_y, track_w, QS_SLIDER_HEIGHT),
        divider_y,
        footer,
        actions: [
            (Slot::QsSettings, action(0)),
            (Slot::QsLock, action(1)),
            (Slot::QsPower, action(2)),
        ],
        height: footer_y + QS_ACT_BTN + QS_PAD,
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
    }
}

const FOCUS_RING: i32 = 2;
const FOCUS_RING_STROKE: f32 = 2.0;

fn draw_focus_ring(
    buf: &mut [u8],
    width: i32,
    height: i32,
    ring: Option<LocalRect>,
    theme: &TopbarTheme,
    scale: i32,
) {
    let Some(rect) = ring else {
        return;
    };
    crate::paint::stroke_rounded_rect_over(
        buf,
        width,
        height,
        rect.loc.x - FOCUS_RING,
        rect.loc.y - FOCUS_RING,
        rect.size.w + FOCUS_RING * 2,
        rect.size.h + FOCUS_RING * 2,
        scale,
        FOCUS_RING_STROKE,
        glide_theme::shape::R2 + FOCUS_RING_STROKE,
        theme.active_background,
    );
}

fn layout_qs_slots(
    output: Rectangle<i32, Logical>,
    strip_y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let geometry = qs_geometry();
    let origin = Point::from((
        output.loc.x + output.size.w - PAD_X - QS_WIDTH,
        strip_y + HEIGHT + POPUP_GAP,
    ));

    for (slot, rect) in geometry.tiles {
        slots.push((slot, offset_rect(rect, origin)));
    }
    slots.push((Slot::QsCaffeine, offset_rect(geometry.caffeine, origin)));
    slots.push((
        Slot::QsVolumeMute,
        offset_rect(geometry.volume_icon, origin),
    ));
    slots.push((
        Slot::QsVolumeTrack,
        offset_rect(geometry.volume_track, origin),
    ));
    slots.push((
        Slot::QsBrightnessTrack,
        offset_rect(geometry.brightness_track, origin),
    ));
    for (slot, rect) in geometry.actions {
        slots.push((slot, offset_rect(rect, origin)));
    }

    Rectangle::new(origin, Size::from((QS_WIDTH, geometry.height)))
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
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(label, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    (text + 29).max(BUTTON_MIN_WIDTH)
}

#[must_use]
pub fn short_layout_name(name: &str) -> String {
    let inner = name
        .rsplit_once('(')
        .and_then(|(_, tail)| tail.split_once(')'))
        .map(|(inside, _)| inside.trim());

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
        Some(first) if first.is_lowercase() => {
            first.to_uppercase().collect::<String>() + chars.as_str()
        }
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
    let written = unsafe {
        libc::strftime(
            out.as_mut_ptr().cast(),
            out.len(),
            format.as_ptr(),
            &raw const tm,
        )
    };
    if written == 0 {
        return None;
    }
    std::str::from_utf8(&out[..written])
        .ok()
        .map(collapse_spaces)
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

#[must_use]
pub fn local_date() -> Option<String> {
    format_local(c"%a %e %b")
}

fn button_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(label, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    (text + BUTTON_PAD_X * 2).max(BUTTON_MIN_WIDTH)
}

fn segment_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(label, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    (text + glide_theme::spacing::SP1 * 2).max(SEG_MIN_WIDTH)
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    progress.map(|value| (value.clamp(0.0, 1.0) * 1000.0).round() as u16)
}

const fn transport_width() -> i32 {
    MEDIA_BTN * 3 + MEDIA_BTN_GAP * 2
}

fn clipped_width(painter: &crate::paint::Painter, text: &str, cap: i32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let width = painter
        .text_width_styled(text, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    width.min(cap)
}

fn media_width(painter: &crate::paint::Painter, media: &MediaWidget) -> i32 {
    let title = clipped_width(painter, &media.title, MEDIA_TITLE_MAX);
    let artist = clipped_width(painter, &media.artist, MEDIA_ARTIST_MAX);
    let tags = if artist > 0 {
        title + MEDIA_TAG_GAP + artist
    } else {
        title
    };
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
    anchor: Rectangle<i32, Logical>,
    output: Rectangle<i32, Logical>,
    strip_y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let centred = anchor.loc.x + (anchor.size.w - MP_WIDTH) / 2;
    let max_x = output.loc.x + output.size.w - PAD_X - MP_WIDTH;
    let x = centred.clamp(output.loc.x + PAD_X, max_x.max(output.loc.x + PAD_X));
    let y = strip_y + HEIGHT + POPUP_GAP;

    let track_y = y + MP_PAD + MP_ART + MP_PAD;
    slots.push((
        Slot::PopMediaSeek,
        Rectangle::new(
            Point::from((x + MP_PAD, track_y - 6)),
            Size::from((MP_WIDTH - MP_PAD * 2, MP_TRACK_H + 12)),
        ),
    ));

    let row_y = y + MP_HEIGHT - MP_PAD - MP_BTN_MAIN;
    let row_w = MP_BTN * 2 + MP_BTN_MAIN + MP_BTN_GAP * 2;
    let mut bx = x + (MP_WIDTH - row_w) / 2;
    for slot in [Slot::PopMediaPrev, Slot::PopMediaPlay, Slot::PopMediaNext] {
        let side = if slot == Slot::PopMediaPlay {
            MP_BTN_MAIN
        } else {
            MP_BTN
        };
        let top = row_y + (MP_BTN_MAIN - side) / 2;
        slots.push((
            slot,
            Rectangle::new(Point::from((bx, top)), Size::from((side, side))),
        ));
        bx += side + MP_BTN_GAP;
    }

    Rectangle::new(Point::from((x, y)), Size::from((MP_WIDTH, MP_HEIGHT)))
}

fn layout_media_slots(
    painter: &crate::paint::Painter,
    media: &MediaWidget,
    clock_x: i32,
    y: i32,
    slots: &mut Vec<(Slot, Rectangle<i32, Logical>)>,
) -> Rectangle<i32, Logical> {
    let width = media_width(painter, media);
    let left = clock_x - MEDIA_SEP - width;
    let mut x = left + width - MEDIA_PAD - transport_width();
    for slot in [Slot::MediaPrev, Slot::MediaPlay, Slot::MediaNext] {
        slots.push((
            slot,
            Rectangle::new(
                Point::from((x, y + (BUTTON_HEIGHT - MEDIA_BTN) / 2)),
                Size::from((MEDIA_BTN, MEDIA_BTN)),
            ),
        ));
        x += MEDIA_BTN + MEDIA_BTN_GAP;
    }
    slots.push((
        Slot::Media,
        Rectangle::new(
            Point::from((left, y)),
            Size::from((width - transport_width() - MEDIA_PAD, BUTTON_HEIGHT)),
        ),
    ));
    Rectangle::new(Point::from((left, y)), Size::from((width, BUTTON_HEIGHT)))
}

fn push_media<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
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

    let buffer = topbar
        .media_cache
        .resolve((topbar.media_rev, seek), output_scale, || {
            draw_media(painter, theme, media, progress, width, output_scale)
        });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_media(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    media: &MediaWidget,
    progress: Option<f32>,
    width: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let [r, g, b, _] = media.accent.0;
    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        scale,
        Color([r, g, b, MEDIA_TINT]),
        BUTTON_RADIUS,
        [true; 4],
    );

    let art_top = (BUTTON_HEIGHT - MEDIA_ART) / 2;
    if let Some(art) = media.art.as_deref() {
        crate::paint::blit_square_over(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            art,
            MEDIA_ART,
            MEDIA_PAD,
            art_top,
            scale,
        );
    } else {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            MEDIA_PAD,
            art_top,
            MEDIA_ART,
            MEDIA_ART,
            scale,
            media.accent,
            glide_theme::shape::R1,
        );
    }

    let mut x = MEDIA_PAD + MEDIA_ART + MEDIA_GAP;
    let title = clipped_width(painter, &media.title, MEDIA_TITLE_MAX);
    painter.draw_label_over_styled(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        x,
        x + title,
        scale,
        crate::paint::TextAlign::Left,
        &media.title,
        theme.label.to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );
    x += title;

    let artist = clipped_width(painter, &media.artist, MEDIA_ARTIST_MAX);
    if artist > 0 {
        x += MEDIA_TAG_GAP;
        painter.draw_label_over_styled(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            x,
            x + artist,
            scale,
            crate::paint::TextAlign::Left,
            &media.artist,
            theme.faint.to_buffer_bytes(),
            &glide_theme::TextStyle::TOPBAR,
        );
    }

    draw_media_transport(theme, media, &mut buf, width, scale);

    if let Some(progress) = progress {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let filled = (((width - MEDIA_PAD * 2) as f32) * progress.clamp(0.0, 1.0)).round() as i32;
        if filled > 0 {
            crate::paint::Painter::fill_rounded_rect_over(
                &mut buf,
                width,
                BUTTON_HEIGHT,
                MEDIA_PAD,
                BUTTON_HEIGHT - MEDIA_LINE - 1,
                filled,
                MEDIA_LINE,
                scale,
                media.accent,
                glide_theme::shape::R_ROUND,
            );
        }
    }

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_media_popup<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
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

    let buffer = topbar
        .media_popup_cache
        .resolve((topbar.media_rev, seek), output_scale, || {
            draw_media_popup(painter, theme, media, progress, output_scale)
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

fn popup_placement(
    popup: Rectangle<i32, Logical>,
    element: Rectangle<i32, Logical>,
    progress: f64,
) -> PopupPlacement {
    let p = progress.clamp(0.0, 1.0);
    let zoom = POPUP_SCALE_FROM + (1.0 - POPUP_SCALE_FROM) * p;
    let rise = -POPUP_RISE * (1.0 - p);
    let anchor_x = f64::from(popup.loc.x) + f64::from(popup.size.w) / 2.0;
    let anchor_y = f64::from(popup.loc.y);
    let x = anchor_x + (f64::from(element.loc.x) - anchor_x) * zoom;
    let y = anchor_y + (f64::from(element.loc.y) - anchor_y) * zoom + rise;
    #[allow(clippy::cast_possible_truncation)]
    let size = Size::from((
        (f64::from(element.size.w) * zoom).round() as i32,
        (f64::from(element.size.h) * zoom).round() as i32,
    ));
    PopupPlacement {
        position: Point::from((x, y)),
        size,
    }
}

fn push_popup_buffer<R>(
    renderer: &mut R,
    buffer: &MemoryRenderBuffer,
    popup: Rectangle<i32, Logical>,
    element: Rectangle<i32, Logical>,
    progress: f64,
    scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if progress <= 0.0 {
        return;
    }
    if progress >= 1.0 {
        push_buffer(renderer, buffer, element.loc, scale, elements);
        return;
    }
    let placement = popup_placement(popup, element, progress);
    #[allow(clippy::cast_possible_truncation)]
    let alpha = progress.clamp(0.0, 1.0) as f32;
    let src = Rectangle::from_size(element.size.to_f64());
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        placement.position.to_physical(scale),
        buffer,
        Some(alpha),
        Some(src),
        Some(placement.size),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

#[allow(clippy::too_many_arguments)]
fn push_popup_shadow<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    rect: Rectangle<i32, Logical>,
    progress: f64,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
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
    let appearance = if dark {
        glide_theme::Appearance::Dark
    } else {
        glide_theme::Appearance::Light
    };
    glide_theme::elevation::layers(glide_theme::elevation::Level::El3, appearance)
}

fn popup_shadow_margin(dark: bool) -> i32 {
    let reach = popup_shadow_layers(dark)
        .iter()
        .map(|layer| layer.blur + layer.dy)
        .fold(0.0_f32, f32::max);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        reach.ceil() as i32 + glide_theme::spacing::SP1
    }
}

fn draw_popup_shadow(
    dark: bool,
    size: Size<i32, Logical>,
    margin: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (size.w + margin * 2, size.h + margin * 2);
    let mut buf = alloc_pixel_buffer(w, h, scale)?;
    let (pw, ph) = (w * scale, h * scale);

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    let layers: Vec<_> = popup_shadow_layers(dark)
        .into_iter()
        .map(|layer| crate::paint::ShadowLayerSpec {
            dy: layer.dy * scale_f,
            blur: layer.blur * scale_f,
            color: layer.color,
        })
        .collect();

    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_outset_shadow_and_border(
        &mut buf,
        pw,
        ph,
        size.w as f32 * scale_f,
        size.h as f32 * scale_f,
        &layers,
        0.0,
        Color([0.0, 0.0, 0.0, 0.0]),
        POPUP_RADIUS * scale_f,
        Some(POPUP_RADIUS * scale_f),
    );

    Some(buffer_from(&buf, w, h, scale))
}

fn draw_media_popup(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    media: &MediaWidget,
    progress: Option<f32>,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(MP_WIDTH * scale * MP_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        MP_WIDTH,
        MP_HEIGHT,
        scale,
        theme.popover_bg,
        glide_theme::shape::R3,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        MP_WIDTH,
        MP_HEIGHT,
        0,
        0,
        MP_WIDTH,
        MP_HEIGHT,
        scale,
        1.0,
        glide_theme::shape::R3,
        theme.border,
    );

    if let Some(art) = media.art_large.as_deref() {
        crate::paint::blit_square_over(
            &mut buf, MP_WIDTH, MP_HEIGHT, art, MP_ART, MP_PAD, MP_PAD, scale,
        );
    } else {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            MP_WIDTH,
            MP_HEIGHT,
            MP_PAD,
            MP_PAD,
            MP_ART,
            MP_ART,
            scale,
            media.accent,
            glide_theme::shape::R2,
        );
    }

    let text_left = MP_PAD + MP_ART + MP_PAD;
    let text_right = MP_WIDTH - MP_PAD;
    painter.draw_label_over_styled_in_band(
        &mut buf,
        MP_WIDTH,
        MP_HEIGHT,
        text_left,
        text_right,
        MP_PAD + 6,
        MP_PAD + 26,
        scale,
        crate::paint::TextAlign::Left,
        &media.title,
        theme.label.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );
    painter.draw_label_over_styled_in_band(
        &mut buf,
        MP_WIDTH,
        MP_HEIGHT,
        text_left,
        text_right,
        MP_PAD + 26,
        MP_PAD + 44,
        scale,
        crate::paint::TextAlign::Left,
        &media.artist,
        theme.muted.to_buffer_bytes(),
        &glide_theme::TextStyle::CALL,
    );
    painter.draw_label_over_styled_in_band(
        &mut buf,
        MP_WIDTH,
        MP_HEIGHT,
        text_left,
        text_right,
        MP_PAD + 44,
        MP_PAD + 58,
        scale,
        crate::paint::TextAlign::Left,
        &media.source,
        theme.faint.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );

    draw_media_popup_track(theme, media, progress, &mut buf, scale);
    draw_media_popup_transport(theme, media, &mut buf, scale);

    Some(buffer_from(&buf, MP_WIDTH, MP_HEIGHT, scale))
}

fn draw_media_popup_track(
    theme: &TopbarTheme,
    media: &MediaWidget,
    progress: Option<f32>,
    buf: &mut [u8],
    scale: i32,
) {
    let y = MP_PAD + MP_ART + MP_PAD;
    let width = MP_WIDTH - MP_PAD * 2;
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        MP_WIDTH,
        MP_HEIGHT,
        MP_PAD,
        y,
        width,
        MP_TRACK_H,
        scale,
        theme.track_soft,
        glide_theme::shape::R_ROUND,
    );
    let Some(progress) = progress else {
        return;
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let filled = ((width as f32) * progress.clamp(0.0, 1.0)).round() as i32;
    if filled > 0 {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            MP_WIDTH,
            MP_HEIGHT,
            MP_PAD,
            y,
            filled,
            MP_TRACK_H,
            scale,
            media.accent,
            glide_theme::shape::R_ROUND,
        );
    }
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        MP_WIDTH,
        MP_HEIGHT,
        (MP_PAD + filled - 5).clamp(MP_PAD, MP_PAD + width - 10),
        y - 3,
        10,
        10,
        scale,
        theme.thumb,
        glide_theme::shape::R_ROUND,
    );
}

fn draw_media_popup_transport(theme: &TopbarTheme, media: &MediaWidget, buf: &mut [u8], scale: i32) {
    let row_y = MP_HEIGHT - MP_PAD - MP_BTN_MAIN;
    let row_w = MP_BTN * 2 + MP_BTN_MAIN + MP_BTN_GAP * 2;
    let mut x = (MP_WIDTH - row_w) / 2;

    for slot in [Slot::PopMediaPrev, Slot::PopMediaPlay, Slot::PopMediaNext] {
        let main = slot == Slot::PopMediaPlay;
        let side = if main { MP_BTN_MAIN } else { MP_BTN };
        let top = row_y + (MP_BTN_MAIN - side) / 2;
        let enabled = match slot {
            Slot::PopMediaPrev => media.can_previous,
            Slot::PopMediaNext => media.can_next,
            _ => true,
        };

        let background = if main { media.accent } else { theme.track_soft };
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            MP_WIDTH,
            MP_HEIGHT,
            x,
            top,
            side,
            side,
            scale,
            background,
            if main {
                glide_theme::shape::R_ROUND
            } else {
                glide_theme::shape::R2
            },
        );

        let colour = if main {
            theme.popover_bg
        } else if enabled {
            theme.label
        } else {
            theme.faint
        };
        crate::paint::draw_media_glyph_over(
            buf,
            MP_WIDTH,
            MP_HEIGHT,
            x,
            top,
            side,
            scale,
            colour,
            match slot {
                Slot::PopMediaPrev => crate::paint::MediaGlyph::Previous,
                Slot::PopMediaNext => crate::paint::MediaGlyph::Next,
                _ if media.playing => crate::paint::MediaGlyph::Pause,
                _ => crate::paint::MediaGlyph::Play,
            },
        );
        x += side + MP_BTN_GAP;
    }
}

fn draw_media_transport(
    theme: &TopbarTheme,
    media: &MediaWidget,
    buf: &mut [u8],
    width: i32,
    scale: i32,
) {
    let mut btn_x = width - MEDIA_PAD - transport_width();
    let btn_y = (BUTTON_HEIGHT - MEDIA_BTN) / 2;
    for slot in [Slot::MediaPrev, Slot::MediaPlay, Slot::MediaNext] {
        let enabled = match slot {
            Slot::MediaPrev => media.can_previous,
            Slot::MediaNext => media.can_next,
            _ => true,
        };
        let colour = if enabled {
            if slot == Slot::MediaPlay {
                media.accent
            } else {
                theme.muted
            }
        } else {
            theme.faint
        };
        crate::paint::draw_media_glyph_over(
            buf,
            width,
            BUTTON_HEIGHT,
            btn_x,
            btn_y,
            MEDIA_BTN,
            scale,
            colour,
            match slot {
                Slot::MediaPrev => crate::paint::MediaGlyph::Previous,
                Slot::MediaNext => crate::paint::MediaGlyph::Next,
                _ if media.playing => crate::paint::MediaGlyph::Pause,
                _ => crate::paint::MediaGlyph::Play,
            },
        );
        btn_x += MEDIA_BTN + MEDIA_BTN_GAP;
    }
}

fn clock_width(painter: &crate::paint::Painter, date: &str, clock: &str, unread: usize) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let date_w = painter
        .text_width_styled(date, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    #[allow(clippy::cast_possible_truncation)]
    let time_w = painter
        .text_width_styled(clock, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    let gap = if date.is_empty() { 0 } else { CLOCK_GAP };
    let mut width = date_w + gap + time_w + BUTTON_PAD_X * 2;
    if unread > 0 {
        let count_str = unread.to_string();
        #[allow(clippy::cast_possible_truncation)]
        let text_w = painter
            .text_width_styled(&count_str, &glide_theme::TextStyle::MICRO)
            .ceil() as i32;
        let badge_w = (text_w + CLOCK_BADGE_PAD_X * 2).max(CLOCK_BADGE_MIN_WIDTH);
        width += CLOCK_GAP + badge_w;
    }
    width
}

fn app_chip_width(painter: &crate::paint::Painter, name: &str, has_icon: bool) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(name, &glide_theme::TextStyle::HEAD)
        .ceil() as i32;
    let icon = if has_icon { APP_ICON + APP_GAP } else { 0 };
    APP_PAD_LEFT + icon + text + BUTTON_PAD_X
}

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
}

impl TopbarTheme {
    #[must_use]
    pub fn from_tokens(t: &glide_theme::Tokens, enabled: bool) -> Self {
        let tint = |alpha: f32| Color([t.shell_fg.0[0], t.shell_fg.0[1], t.shell_fg.0[2], alpha]);
        let dark = t.appearance == glide_theme::Appearance::Dark;
        Self {
            enabled,
            dark,
            background: t.shell_bg,
            popover_bg: t.popover_bg,
            border: t.shell_line,
            label: t.shell_fg,
            muted: tint(0.68),
            faint: tint(0.46),
            button_hover: tint(glide_theme::motion::ST_HOVER),
            track: tint(0.18),
            track_soft: tint(0.07),
            segment_active: tint(0.22),
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
            .finish()
    }
}

impl Default for TopbarTheme {
    fn default() -> Self {
        let tokens =
            glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        Self::from_tokens(&tokens, true)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    icons: &mut crate::icon::IconLoader,
    output: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    backdrop: Option<&crate::dock::Backdrop<'_>>,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if !topbar.is_enabled() {
        return Vec::new();
    }

    topbar.invalidate_stale_buffers();
    topbar.layout_slots(painter, output);

    let strip = topbar.rect(output);
    let mut elements = Vec::new();

    push_quick_settings(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_notification_center(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_lang_menu(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_app_chip(
        topbar,
        renderer,
        painter,
        icons,
        strip,
        scale,
        output_scale,
        &mut elements,
    );
    push_clock(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_media(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_media_popup(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_dnd(topbar, renderer, scale, output_scale, &mut elements);
    push_layout(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_caffeine(topbar, renderer, scale, output_scale, &mut elements);
    push_status(
        topbar,
        renderer,
        painter,
        scale,
        output_scale,
        &mut elements,
    );
    push_panel(
        topbar,
        renderer,
        strip,
        scale,
        output_scale,
        backdrop,
        &mut elements,
    );

    elements
}

fn push_buffer<R>(
    renderer: &mut R,
    buffer: &MemoryRenderBuffer,
    position: Point<i32, Logical>,
    scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
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

#[allow(clippy::too_many_arguments)]
fn push_panel<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    strip: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    backdrop: Option<&crate::dock::Backdrop<'_>>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let key = (
        strip.size.w,
        output_scale,
        backdrop.map_or(0, |source| source.generation),
    );
    if !topbar.panel.contains_key(&key) {
        topbar.panel.clear();
        if let Some(buffer) = draw_panel(&topbar.theme, strip, output_scale, backdrop) {
            topbar.panel.insert(key, buffer);
        }
    }
    let Some(buffer) = topbar.panel.get(&key) else {
        return;
    };
    push_buffer(renderer, buffer, strip.loc, scale, elements);
}

fn draw_panel(
    theme: &TopbarTheme,
    strip: Rectangle<i32, Logical>,
    scale: i32,
    backdrop: Option<&crate::dock::Backdrop<'_>>,
) -> Option<MemoryRenderBuffer> {
    let (w, h) = (strip.size.w, strip.size.h);
    let pixels = usize::try_from(w * scale * h * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let has_backdrop =
        backdrop.is_some_and(|source| source.blit(&mut buf, w, h, 0.0, strip, scale));
    if has_backdrop {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            w,
            h,
            0,
            0,
            w,
            h,
            scale,
            theme.background,
            0.0,
        );
    } else {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            w,
            h,
            scale,
            theme.background,
            0.0,
            [false; 4],
        );
    }

    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        w,
        h,
        0,
        h - 1,
        w,
        1,
        scale,
        theme.border,
        0.0,
    );

    Some(buffer_from(&buf, w, h, scale))
}

#[allow(clippy::too_many_arguments)]
fn push_app_chip<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    icons: &mut crate::icon::IconLoader,
    strip: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if topbar.app_name.is_empty() {
        return;
    }

    let theme = &topbar.theme;
    let name = &topbar.app_name;
    let app_id = &topbar.app_id;
    let buffer = topbar
        .app_cache
        .resolve((app_id.clone(), name.clone()), output_scale, || {
            let icon = icons.load(app_id, APP_ICON * scale_or_one(output_scale));
            let width = app_chip_width(painter, name, icon.is_some());
            draw_app_chip(painter, theme, name, icon, width, output_scale)
        });
    let Some(buffer) = buffer else {
        return;
    };

    let position = Point::from((
        strip.loc.x + PAD_X,
        strip.loc.y + (HEIGHT - BUTTON_HEIGHT) / 2,
    ));
    push_buffer(renderer, buffer, position, scale, elements);
}

fn draw_app_chip(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    name: &str,
    icon: Option<&[u8]>,
    width: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        scale,
        theme.track_soft,
        BUTTON_RADIUS,
        [true; 4],
    );

    let mut text_left = APP_PAD_LEFT;
    if let Some(icon) = icon {
        let top = (BUTTON_HEIGHT - APP_ICON) / 2;
        crate::paint::blit_square_over(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            icon,
            APP_ICON,
            APP_PAD_LEFT,
            top,
            scale,
        );
        text_left += APP_ICON + APP_GAP;
    }

    painter.draw_label_over_styled(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        text_left,
        width - BUTTON_PAD_X,
        scale,
        crate::paint::TextAlign::Left,
        name,
        theme.label.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_clock<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
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
    let unread = topbar.unread;
    let hovered = topbar.hovered == Some(Slot::Clock);
    let open = topbar.open_popup == Some(OpenPopup::NotificationCenter);
    let buffer = topbar.clock_cache.resolve(
        (date.clone(), clock.clone(), unread, hovered, open),
        output_scale,
        || {
            draw_clock(
                painter,
                theme,
                date,
                clock,
                unread,
                hovered,
                open,
                width,
                output_scale,
            )
        },
    );
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

#[allow(clippy::too_many_arguments)]
fn draw_clock(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    date: &str,
    clock: &str,
    unread: usize,
    hovered: bool,
    open: bool,
    width: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    if open || hovered {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.button_hover,
            BUTTON_RADIUS,
            [true; 4],
        );
    }

    #[allow(clippy::cast_possible_truncation)]
    let date_w = painter
        .text_width_styled(date, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;

    if !date.is_empty() {
        painter.draw_label_over_styled(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            BUTTON_PAD_X,
            BUTTON_PAD_X + date_w,
            scale,
            crate::paint::TextAlign::Left,
            date,
            theme.muted.to_buffer_bytes(),
            &glide_theme::TextStyle::TOPBAR,
        );
    }

    let time_left = if date.is_empty() {
        BUTTON_PAD_X
    } else {
        BUTTON_PAD_X + date_w + CLOCK_GAP
    };
    #[allow(clippy::cast_possible_truncation)]
    let time_w = painter
        .text_width_styled(clock, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;
    painter.draw_label_over_styled(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        time_left,
        time_left + time_w,
        scale,
        crate::paint::TextAlign::Left,
        clock,
        theme.label.to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );

    if unread > 0 {
        let count_str = unread.to_string();
        #[allow(clippy::cast_possible_truncation)]
        let text_w = painter
            .text_width_styled(&count_str, &glide_theme::TextStyle::MICRO)
            .ceil() as i32;
        let badge_w = (text_w + CLOCK_BADGE_PAD_X * 2).max(CLOCK_BADGE_MIN_WIDTH);
        let badge_x = time_left + time_w + CLOCK_GAP;
        let badge_y = (BUTTON_HEIGHT - CLOCK_BADGE_HEIGHT) / 2;

        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            badge_x,
            badge_y,
            badge_w,
            CLOCK_BADGE_HEIGHT,
            scale,
            theme.active_background,
            glide_theme::shape::R_ROUND,
        );

        painter.draw_label_over_styled(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            badge_x,
            badge_x + badge_w,
            scale,
            crate::paint::TextAlign::Center,
            &count_str,
            theme.active_label.to_buffer_bytes(),
            &glide_theme::TextStyle::MICRO,
        );
    }

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_layout<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
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

    let buffer = topbar.layout_cache.resolve(
        (topbar.layouts_rev, active, hovered, open),
        output_scale,
        || {
            let label = layouts.get(active).map_or("", String::as_str);
            if segmented {
                draw_segmented(
                    painter,
                    theme,
                    layouts,
                    active,
                    hovered,
                    width,
                    output_scale,
                )
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
                draw_button(
                    painter,
                    theme,
                    label,
                    width,
                    hovered == Some(Slot::LayoutCycle),
                    output_scale,
                )
            }
        },
    );
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_lang_dropdown_button(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    text: &str,
    width: i32,
    hovered: bool,
    open: bool,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    if open || hovered {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.button_hover,
            BUTTON_RADIUS,
            [true; 4],
        );
    }

    #[allow(clippy::cast_possible_truncation)]
    let text_w = painter
        .text_width_styled(text, &glide_theme::TextStyle::TOPBAR)
        .ceil() as i32;

    painter.draw_label_over_styled(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        9,
        9 + text_w,
        scale,
        crate::paint::TextAlign::Left,
        text,
        if open || hovered {
            theme.label
        } else {
            theme.muted
        }
        .to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );

    let chev_side = 10;
    let chev_pixels = usize::try_from(chev_side * scale * chev_side * scale * 4).ok()?;
    let mut chev_buf = vec![0u8; chev_pixels];
    crate::paint::draw_chevron_glyph_over(
        &mut chev_buf,
        chev_side,
        chev_side,
        9.0,
        1.5,
        open,
        scale,
        theme.faint,
    );
    let chev_x = width - 6 - chev_side;
    let chev_y = (BUTTON_HEIGHT - chev_side) / 2;
    crate::paint::blit_square_over(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        &chev_buf,
        chev_side,
        chev_x,
        chev_y,
        scale,
    );

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_lang_menu<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if topbar.open_popup != Some(OpenPopup::LanguageMenu) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let theme = &topbar.theme;
    let layouts = &topbar.layouts;
    let active = topbar.layout_index;
    let hovered = topbar.hovered_in(Slot::in_lang_menu);
    let focused = topbar.focused_in(Slot::in_lang_menu);
    let focus = topbar.focus_ring_in(Slot::in_lang_menu);
    let width = rect.size.w;
    let height = rect.size.h;

    let buffer = topbar.lang_menu_cache.resolve(
        (topbar.layouts_rev, active, hovered, focused),
        output_scale,
        || {
            draw_lang_menu(
                painter,
                theme,
                layouts,
                active,
                hovered,
                focus,
                width,
                height,
                output_scale,
            )
        },
    );
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
    push_popup_shadow(topbar, renderer, rect, scale, output_scale, elements);
}

struct LangMenuRenderContext<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    width: i32,
    height: i32,
    scale: i32,
    active: usize,
    hovered: Option<Slot>,
}

fn draw_lang_menu_item(
    buf: &mut [u8],
    ctx: &LangMenuRenderContext<'_>,
    index: usize,
    code: &str,
    y: i32,
) {
    let is_active = index == ctx.active;
    let is_hovered = ctx.hovered == Some(Slot::LangMenuItem(index));

    if is_hovered {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            LANG_MENU_PAD,
            y,
            ctx.width - LANG_MENU_PAD * 2,
            LANG_MENU_ITEM_HEIGHT,
            ctx.scale,
            ctx.theme.button_hover,
            glide_theme::shape::R2,
        );
    }

    if is_active {
        let tick_side = 14;
        if let Ok(tick_pixels) = usize::try_from(tick_side * ctx.scale * tick_side * ctx.scale * 4)
        {
            let mut tick_buf = vec![0u8; tick_pixels];
            crate::paint::draw_tick_glyph_over(
                &mut tick_buf,
                tick_side,
                tick_side,
                13.0,
                1.8,
                ctx.scale,
                ctx.theme.active_background,
            );
            let tick_y = y + (LANG_MENU_ITEM_HEIGHT - tick_side) / 2;
            crate::paint::blit_square_over(
                buf,
                ctx.width,
                ctx.height,
                &tick_buf,
                tick_side,
                LANG_MENU_PAD + 6,
                tick_y,
                ctx.scale,
            );
        }
    }

    let text_left = LANG_MENU_PAD + 6 + 14 + 6;
    let color = if is_active || is_hovered {
        ctx.theme.label
    } else {
        ctx.theme.muted
    };
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        text_left,
        text_left + 40,
        y,
        y + LANG_MENU_ITEM_HEIGHT,
        ctx.scale,
        crate::paint::TextAlign::Left,
        code,
        color.to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );

    let native = native_layout_name(code);
    if !native.is_empty() {
        ctx.painter.draw_label_over_styled_in_band(
            buf,
            ctx.width,
            ctx.height,
            ctx.width - 120,
            ctx.width - LANG_MENU_PAD - 8,
            y,
            y + LANG_MENU_ITEM_HEIGHT,
            ctx.scale,
            crate::paint::TextAlign::Right,
            native,
            ctx.theme.faint.to_buffer_bytes(),
            &glide_theme::TextStyle::TOPBAR,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_lang_menu(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    layouts: &[String],
    active: usize,
    hovered: Option<Slot>,
    focus: Option<LocalRect>,
    width: i32,
    height: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        height,
        scale,
        theme.popover_bg,
        glide_theme::shape::R3,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        width,
        height,
        0,
        0,
        width,
        height,
        scale,
        1.0,
        glide_theme::shape::R3,
        theme.border,
    );

    let ctx = LangMenuRenderContext {
        painter,
        theme,
        width,
        height,
        scale,
        active,
        hovered,
    };

    let mut y = LANG_MENU_PAD;
    for (index, code) in layouts.iter().enumerate() {
        draw_lang_menu_item(&mut buf, &ctx, index, code, y);
        y += LANG_MENU_ITEM_HEIGHT;
    }

    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        width,
        height,
        LANG_MENU_PAD + 4,
        y + 4,
        width - (LANG_MENU_PAD + 4) * 2,
        1,
        scale,
        theme.border,
        0.0,
    );
    y += LANG_MENU_DIVIDER;

    let settings_hovered = hovered == Some(Slot::LangMenuSettings);
    if settings_hovered {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            width,
            height,
            LANG_MENU_PAD,
            y,
            width - LANG_MENU_PAD * 2,
            LANG_MENU_ITEM_HEIGHT,
            scale,
            theme.button_hover,
            glide_theme::shape::R2,
        );
    }
    let settings_left = LANG_MENU_PAD + 6 + 14 + 6;
    let settings_color = if settings_hovered {
        theme.label
    } else {
        theme.muted
    };
    painter.draw_label_over_styled_in_band(
        &mut buf,
        width,
        height,
        settings_left,
        width - LANG_MENU_PAD - 8,
        y,
        y + LANG_MENU_ITEM_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        crate::i18n::t(crate::i18n::Key::InputSettings),
        settings_color.to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );
    draw_focus_ring(&mut buf, width, height, focus, theme, scale);

    Some(buffer_from(&buf, width, height, scale))
}

fn push_notification_center<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if topbar.open_popup != Some(OpenPopup::NotificationCenter) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let theme = &topbar.theme;
    let dnd = topbar.dnd;
    let hovered = topbar.hovered_in(Slot::in_notification_center);
    let focused = topbar.focused_in(Slot::in_notification_center);
    let focus = topbar.focus_ring_in(Slot::in_notification_center);
    let width = rect.size.w;
    let height = rect.size.h;
    let nc_rev = topbar.nc_rev;
    let items = &topbar.nc_items;

    let buffer = topbar
        .nc_cache
        .resolve((nc_rev, dnd, hovered, focused), output_scale, || {
            draw_notification_center(
                painter,
                theme,
                items,
                dnd,
                hovered,
                focus,
                width,
                height,
                output_scale,
            )
        });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
    push_popup_shadow(topbar, renderer, rect, scale, output_scale, elements);
}

struct NcRenderContext<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    width: i32,
    height: i32,
    scale: i32,
    dnd: bool,
    hovered: Option<Slot>,
}

fn draw_nc_dnd_switch(buf: &mut [u8], ctx: &NcRenderContext<'_>, head_y: i32) -> i32 {
    let sw_x = ctx.width - NC_PAD - SWITCH_WIDTH;
    let sw_y = head_y + (NC_HEADER_HEIGHT - SWITCH_HEIGHT) / 2;
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        sw_x,
        sw_y,
        SWITCH_WIDTH,
        SWITCH_HEIGHT,
        ctx.scale,
        if ctx.dnd {
            ctx.theme.active_background
        } else {
            ctx.theme.track
        },
        glide_theme::shape::R_ROUND,
    );
    let thumb_x = if ctx.dnd {
        sw_x + SWITCH_WIDTH - SWITCH_INSET - SWITCH_THUMB
    } else {
        sw_x + SWITCH_INSET
    };
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        thumb_x,
        sw_y + SWITCH_INSET,
        SWITCH_THUMB,
        SWITCH_THUMB,
        ctx.scale,
        ctx.theme.thumb,
        glide_theme::shape::R_ROUND,
    );

    let moon_side = 14;
    let moon_x = sw_x - 8 - moon_side;
    let moon_y = head_y + (NC_HEADER_HEIGHT - moon_side) / 2;
    if let Ok(moon_pixels) = usize::try_from(moon_side * ctx.scale * moon_side * ctx.scale * 4) {
        let mut moon_buf = vec![0u8; moon_pixels];
        let moon_color = if ctx.dnd {
            ctx.theme.active_label
        } else {
            ctx.theme.muted
        };
        crate::paint::draw_moon_glyph_over(
            &mut moon_buf,
            moon_side,
            moon_side,
            14.0,
            ctx.scale,
            moon_color,
        );
        crate::paint::blit_square_over(
            buf, ctx.width, ctx.height, &moon_buf, moon_side, moon_x, moon_y, ctx.scale,
        );
    }
    moon_x
}

fn draw_nc_header(buf: &mut [u8], ctx: &NcRenderContext<'_>) {
    let head_y = NC_PAD;
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        NC_PAD + 2,
        NC_PAD + 120,
        head_y,
        head_y + NC_HEADER_HEIGHT,
        ctx.scale,
        crate::paint::TextAlign::Left,
        crate::i18n::t(crate::i18n::Key::Notifications),
        ctx.theme.muted.to_buffer_bytes(),
        &glide_theme::TextStyle::MICRO,
    );

    let moon_x = draw_nc_dnd_switch(buf, ctx, head_y);

    let clear_w = 96;
    let clear_x = moon_x - 10 - clear_w;
    let clear_hover = ctx.hovered == Some(Slot::NcClearAll);
    if clear_hover {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            clear_x,
            head_y,
            clear_w,
            NC_HEADER_HEIGHT,
            ctx.scale,
            ctx.theme.button_hover,
            glide_theme::shape::R2,
        );
    }
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        clear_x,
        clear_x + clear_w,
        head_y,
        head_y + NC_HEADER_HEIGHT,
        ctx.scale,
        crate::paint::TextAlign::Center,
        crate::i18n::t(crate::i18n::Key::ClearAll),
        if clear_hover {
            ctx.theme.label
        } else {
            ctx.theme.muted
        }
        .to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );
}

fn draw_nc_item_icon(
    buf: &mut [u8],
    ctx: &NcRenderContext<'_>,
    item: &crate::notifications::NcItem,
    item_y: i32,
) {
    let icon_side = 32;
    let icon_x = NC_PAD + 4;
    let icon_y = item_y + (NC_ITEM_HEIGHT - icon_side) / 2;
    if let Some(icon) = item.icon.as_deref() {
        crate::paint::blit_square_over(
            buf, ctx.width, ctx.height, icon, icon_side, icon_x, icon_y, ctx.scale,
        );
    } else {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            icon_x,
            icon_y,
            icon_side,
            icon_side,
            ctx.scale,
            ctx.theme.track_soft,
            glide_theme::shape::R2,
        );
        let initial = item
            .app_name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        ctx.painter.draw_label_over_styled_in_band(
            buf,
            ctx.width,
            ctx.height,
            icon_x,
            icon_x + icon_side,
            icon_y,
            icon_y + icon_side,
            ctx.scale,
            crate::paint::TextAlign::Center,
            &initial,
            ctx.theme.label.to_buffer_bytes(),
            &glide_theme::TextStyle::HEAD,
        );
    }
}

fn nc_count_badge_width(ctx: &NcRenderContext<'_>, item: &crate::notifications::NcItem) -> i32 {
    let label = format!("+{}", item.count.saturating_sub(1));
    #[allow(clippy::cast_possible_truncation)]
    let text = ctx
        .painter
        .text_width_styled(&label, &glide_theme::TextStyle::MICRO)
        .ceil() as i32;
    text + glide_theme::spacing::SP2
}

fn nc_count_badge_left(ctx: &NcRenderContext<'_>, item: &crate::notifications::NcItem) -> i32 {
    let time_left = ctx.width - NC_PAD - 60;
    if item.count < 2 {
        return time_left;
    }
    time_left - nc_count_badge_width(ctx, item) - glide_theme::spacing::SP1
}

fn draw_nc_item_count(
    buf: &mut [u8],
    ctx: &NcRenderContext<'_>,
    item: &crate::notifications::NcItem,
    item_y: i32,
) {
    if item.count < 2 {
        return;
    }
    let label = format!("+{}", item.count - 1);
    let width = nc_count_badge_width(ctx, item);
    let height = 15;
    let left = nc_count_badge_left(ctx, item);
    let top = item_y + 8;

    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        left,
        top,
        width,
        height,
        ctx.scale,
        ctx.theme.track_soft,
        glide_theme::shape::R_ROUND,
    );
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        left,
        left + width,
        top,
        top + height,
        ctx.scale,
        crate::paint::TextAlign::Center,
        &label,
        ctx.theme.muted.to_buffer_bytes(),
        &glide_theme::TextStyle::MICRO,
    );
}

fn draw_nc_item_dismiss(buf: &mut [u8], ctx: &NcRenderContext<'_>, id: u32, item_y: i32) {
    let x_side = 22;
    let x_left = ctx.width - NC_PAD - x_side;
    let x_top = item_y + 24;
    let x_hover = ctx.hovered == Some(Slot::NcItemDismiss(id));
    if x_hover {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            x_left,
            x_top,
            x_side,
            x_side,
            ctx.scale,
            ctx.theme.button_hover,
            glide_theme::shape::R_ROUND,
        );
    }
    if let Ok(x_pixels) = usize::try_from(x_side * ctx.scale * x_side * ctx.scale * 4) {
        let mut close_buf = vec![0u8; x_pixels];
        crate::paint::draw_close_glyph_over(
            &mut close_buf,
            x_side,
            x_side,
            9.0,
            1.8,
            ctx.scale,
            if x_hover {
                ctx.theme.label
            } else {
                ctx.theme.muted
            },
        );
        crate::paint::blit_square_over(
            buf, ctx.width, ctx.height, &close_buf, x_side, x_left, x_top, ctx.scale,
        );
    }
}

fn draw_nc_item(
    buf: &mut [u8],
    ctx: &NcRenderContext<'_>,
    item: &crate::notifications::NcItem,
    item_y: i32,
    is_first: bool,
) {
    if !is_first {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            NC_PAD,
            item_y,
            ctx.width - NC_PAD * 2,
            1,
            ctx.scale,
            ctx.theme.border,
            0.0,
        );
    }
    if ctx.hovered == Some(Slot::NcItem(item.id)) {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            NC_PAD,
            item_y,
            ctx.width - NC_PAD * 2,
            NC_ITEM_HEIGHT,
            ctx.scale,
            ctx.theme.button_hover,
            glide_theme::shape::R2,
        );
    }

    draw_nc_item_icon(buf, ctx, item, item_y);

    let text_left = NC_PAD + 4 + 32 + 10;
    let text_right = ctx.width - NC_PAD - 48;
    let title_right = text_right.min(nc_count_badge_left(ctx, item) - glide_theme::spacing::SP2);
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        text_left,
        title_right,
        item_y + 8,
        item_y + 26,
        ctx.scale,
        crate::paint::TextAlign::Left,
        &item.title,
        ctx.theme.label.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        text_left,
        text_right,
        item_y + 28,
        item_y + 46,
        ctx.scale,
        crate::paint::TextAlign::Left,
        &item.body,
        ctx.theme.muted.to_buffer_bytes(),
        &glide_theme::TextStyle::CALL,
    );

    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        ctx.width - NC_PAD - 60,
        ctx.width - NC_PAD - 4,
        item_y + 8,
        item_y + 24,
        ctx.scale,
        crate::paint::TextAlign::Right,
        &item.time,
        ctx.theme.faint.to_buffer_bytes(),
        &glide_theme::TextStyle::MICRO,
    );

    draw_nc_item_count(buf, ctx, item, item_y);
    draw_nc_item_dismiss(buf, ctx, item.id, item_y);
}

#[allow(clippy::too_many_arguments)]
fn draw_notification_center(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    items: &[crate::notifications::NcItem],
    dnd: bool,
    hovered: Option<Slot>,
    focus: Option<LocalRect>,
    width: i32,
    height: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        height,
        scale,
        theme.popover_bg,
        glide_theme::shape::R3,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        width,
        height,
        0,
        0,
        width,
        height,
        scale,
        1.0,
        glide_theme::shape::R3,
        theme.border,
    );

    let ctx = NcRenderContext {
        painter,
        theme,
        width,
        height,
        scale,
        dnd,
        hovered,
    };

    draw_nc_header(&mut buf, &ctx);

    let mut item_y = NC_PAD + NC_HEADER_HEIGHT + NC_GAP;
    if items.is_empty() {
        painter.draw_label_over_styled_in_band(
            &mut buf,
            width,
            height,
            NC_PAD,
            width - NC_PAD,
            item_y,
            item_y + NC_EMPTY_HEIGHT,
            scale,
            crate::paint::TextAlign::Center,
            crate::i18n::t(crate::i18n::Key::NoNewNotifications),
            theme.muted.to_buffer_bytes(),
            &glide_theme::TextStyle::TOPBAR,
        );
    } else {
        for (i, item) in items.iter().take(NC_MAX_ITEMS).enumerate() {
            draw_nc_item(&mut buf, &ctx, item, item_y, i == 0);
            item_y += NC_ITEM_HEIGHT;
        }
    }
    draw_focus_ring(&mut buf, width, height, focus, theme, scale);

    Some(buffer_from(&buf, width, height, scale))
}

fn draw_button(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    text: &str,
    width: i32,
    hovered: bool,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    if hovered {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.button_hover,
            BUTTON_RADIUS,
            [true; 4],
        );
    }
    painter.draw_label_over_styled(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        BUTTON_PAD_X,
        width - BUTTON_PAD_X,
        scale,
        crate::paint::TextAlign::Center,
        text,
        if hovered { theme.label } else { theme.muted }.to_buffer_bytes(),
        &glide_theme::TextStyle::TOPBAR,
    );
    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

#[allow(clippy::too_many_arguments)]
fn draw_segmented(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    layouts: &[String],
    active: usize,
    hovered: Option<Slot>,
    width: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    let track_top = (BUTTON_HEIGHT - SEG_HEIGHT) / 2;
    let track_width = width - SEG_PAD_X * 2;
    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        SEG_PAD_X,
        track_top,
        track_width,
        SEG_HEIGHT,
        scale,
        theme.track_soft,
        BUTTON_RADIUS,
    );

    let mut x = SEG_PAD_X + SEG_INSET;
    let top = track_top + SEG_INSET;
    let height = SEG_HEIGHT - SEG_INSET * 2;
    for (index, name) in layouts.iter().enumerate() {
        let seg = segment_width(painter, name);
        let is_active = index == active;
        let is_hovered = hovered == Some(Slot::LayoutPick(index));

        if is_active {
            crate::paint::Painter::fill_rounded_rect_over(
                &mut buf,
                width,
                BUTTON_HEIGHT,
                x,
                top,
                seg,
                height,
                scale,
                theme.segment_active,
                SEG_RADIUS,
            );
        } else if is_hovered {
            crate::paint::Painter::fill_rounded_rect_over(
                &mut buf,
                width,
                BUTTON_HEIGHT,
                x,
                top,
                seg,
                height,
                scale,
                theme.button_hover,
                SEG_RADIUS,
            );
        }

        let color = if is_active {
            theme.label
        } else if is_hovered {
            theme.muted
        } else {
            theme.faint
        };
        painter.draw_label_over_styled(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            x,
            x + seg,
            scale,
            crate::paint::TextAlign::Center,
            name,
            color.to_buffer_bytes(),
            &glide_theme::TextStyle::TOPBAR,
        );

        x += seg + SEG_GAP;
    }

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_caffeine<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(rect) = topbar.slot_rect(Slot::Caffeine) else {
        return;
    };
    let hovered = topbar.hovered == Some(Slot::Caffeine);
    let on = topbar.caffeinated;
    let theme = &topbar.theme;
    let width = rect.size.w;

    let buffer = topbar
        .coffee_cache
        .resolve((on, hovered), output_scale, || {
            draw_caffeine(theme, width, hovered, on, output_scale)
        });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_caffeine(
    theme: &TopbarTheme,
    width: i32,
    hovered: bool,
    on: bool,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    if hovered {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.button_hover,
            BUTTON_RADIUS,
            [true; 4],
        );
    }

    let glyph_color = if on || hovered {
        theme.label
    } else {
        theme.muted
    };
    #[allow(clippy::cast_precision_loss)]
    let glyph_centre = glide_theme::spacing::SP1 as f32 + COFFEE_GLYPH / 2.0;
    #[allow(clippy::cast_precision_loss)]
    crate::paint::draw_coffee_glyph_over(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        glyph_centre,
        BUTTON_HEIGHT as f32 / 2.0,
        COFFEE_GLYPH,
        COFFEE_STROKE,
        scale,
        glyph_color,
        on,
    );

    let track_x = width - glide_theme::spacing::SP1 - SWITCH_WIDTH;
    let track_y = (BUTTON_HEIGHT - SWITCH_HEIGHT) / 2;
    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        track_x,
        track_y,
        SWITCH_WIDTH,
        SWITCH_HEIGHT,
        scale,
        if on {
            theme.active_background
        } else {
            theme.track
        },
        glide_theme::shape::R_ROUND,
    );

    let thumb_x = if on {
        track_x + SWITCH_WIDTH - SWITCH_INSET - SWITCH_THUMB
    } else {
        track_x + SWITCH_INSET
    };
    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        thumb_x,
        track_y + SWITCH_INSET,
        SWITCH_THUMB,
        SWITCH_THUMB,
        scale,
        theme.thumb,
        glide_theme::shape::R_ROUND,
    );

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_dnd<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(rect) = topbar.slot_rect(Slot::DoNotDisturb) else {
        return;
    };
    let hovered = topbar.hovered == Some(Slot::DoNotDisturb);
    let on = topbar.dnd;
    let theme = &topbar.theme;
    let width = rect.size.w;

    let buffer = topbar.dnd_cache.resolve((on, hovered), output_scale, || {
        draw_dnd(theme, width, hovered, on, output_scale)
    });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_dnd(
    theme: &TopbarTheme,
    width: i32,
    hovered: bool,
    on: bool,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let pixels = usize::try_from(width * scale * BUTTON_HEIGHT * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    if on {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.active_background,
            BUTTON_RADIUS,
            [true; 4],
        );
    } else if hovered {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.button_hover,
            BUTTON_RADIUS,
            [true; 4],
        );
    }

    let glyph_color = if on {
        theme.active_label
    } else if hovered {
        theme.label
    } else {
        theme.muted
    };

    crate::paint::draw_moon_glyph_over(
        &mut buf,
        width,
        BUTTON_HEIGHT,
        DND_GLYPH,
        scale,
        glyph_color,
    );

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn alloc_pixel_buffer(w: i32, h: i32, scale: i32) -> Option<Vec<u8>> {
    let count = (w * scale).checked_mul(h * scale)?.checked_mul(4)?;
    let pixels = usize::try_from(count).ok()?;
    Some(vec![0u8; pixels])
}

fn push_status<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    let Some(rect) = topbar.slot_rect(Slot::Status) else {
        return;
    };
    let hovered = topbar.hovered == Some(Slot::Status);
    let is_open = topbar.open_popup == Some(OpenPopup::QuickSettings);
    let theme = &topbar.theme;
    let width = rect.size.w;

    let sep_pos = Point::from((rect.loc.x - STATUS_SEP_WIDTH, rect.loc.y));
    let separator = topbar
        .separator_cache
        .resolve((), output_scale, || draw_separator(theme, output_scale));
    if let Some(buffer) = separator {
        push_buffer(renderer, buffer, sep_pos, scale, elements);
    }

    let status_rev = topbar.status_rev;
    let status = &topbar.system_status;
    let buffer = topbar
        .status_cache
        .resolve((status_rev, is_open, hovered), output_scale, || {
            draw_status_button(
                theme,
                status,
                painter,
                width,
                is_open,
                hovered,
                output_scale,
            )
        });
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
}

fn draw_separator(theme: &TopbarTheme, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut buf = alloc_pixel_buffer(STATUS_SEP_WIDTH, BUTTON_HEIGHT, scale)?;
    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        STATUS_SEP_WIDTH,
        BUTTON_HEIGHT,
        6,
        5,
        1,
        14,
        scale,
        theme.border,
        0.0,
    );
    Some(buffer_from(&buf, STATUS_SEP_WIDTH, BUTTON_HEIGHT, scale))
}

fn draw_status_wifi(
    buf: &mut [u8],
    width: i32,
    cur_x: i32,
    level: u8,
    fg: Color,
    faint_fg: Color,
    scale: i32,
) {
    let Some(mut wifi_buf) = alloc_pixel_buffer(STATUS_WIFI_WIDTH, BUTTON_HEIGHT, scale) else {
        return;
    };
    crate::paint::draw_wifi_bars_over(
        &mut wifi_buf,
        STATUS_WIFI_WIDTH,
        BUTTON_HEIGHT,
        level,
        scale,
        fg,
        faint_fg,
    );
    crate::paint::blit_rect_over(
        buf,
        width,
        BUTTON_HEIGHT,
        &wifi_buf,
        STATUS_WIFI_WIDTH,
        BUTTON_HEIGHT,
        cur_x,
        0,
        scale,
    );
}

fn draw_status_speaker(buf: &mut [u8], width: i32, cur_x: i32, muted: bool, fg: Color, scale: i32) {
    let Some(mut spk_buf) = alloc_pixel_buffer(STATUS_SPEAKER_WIDTH, BUTTON_HEIGHT, scale) else {
        return;
    };
    crate::paint::draw_speaker_glyph_over(
        &mut spk_buf,
        STATUS_SPEAKER_WIDTH,
        BUTTON_HEIGHT,
        14.0,
        1.4,
        muted,
        scale,
        fg,
    );
    crate::paint::blit_rect_over(
        buf,
        width,
        BUTTON_HEIGHT,
        &spk_buf,
        STATUS_SPEAKER_WIDTH,
        BUTTON_HEIGHT,
        cur_x,
        0,
        scale,
    );
}

const BATTERY_LOW_PCT: u8 = 20;

#[allow(clippy::too_many_arguments)]
fn draw_status_battery(
    buf: &mut [u8],
    width: i32,
    mut cur_x: i32,
    pct: u8,
    charging: bool,
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    fg: Color,
    scale: i32,
) -> i32 {
    let Some(mut bat_buf) = alloc_pixel_buffer(STATUS_BATT_WIDTH, BUTTON_HEIGHT, scale) else {
        return cur_x;
    };
    let bat_args = crate::paint::BatteryRenderArgs {
        canvas_width: STATUS_BATT_WIDTH,
        canvas_height: BUTTON_HEIGHT,
        pct,
        charging,
        scale,
        fg,
        ok_color: theme.ok,
        danger_color: theme.danger,
    };
    crate::paint::draw_battery_glyph_over(&mut bat_buf, &bat_args);
    crate::paint::blit_rect_over(
        buf,
        width,
        BUTTON_HEIGHT,
        &bat_buf,
        STATUS_BATT_WIDTH,
        BUTTON_HEIGHT,
        cur_x,
        0,
        scale,
    );
    cur_x += STATUS_BATT_WIDTH + 5;

    let pct_str = format!("{pct}%");
    let pct_color = if !charging && pct <= theme.battery_low_pct {
        theme.danger
    } else {
        fg
    };
    #[allow(clippy::cast_possible_truncation)]
    let pct_w = painter
        .text_width_styled(&pct_str, &glide_theme::TextStyle::MICRO)
        .ceil() as i32;
    painter.draw_label_over_styled_in_band(
        buf,
        width,
        BUTTON_HEIGHT,
        cur_x,
        cur_x + pct_w + 2,
        0,
        BUTTON_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        &pct_str,
        pct_color.to_buffer_bytes(),
        &glide_theme::TextStyle::MICRO,
    );
    cur_x + pct_w
}

fn draw_status_chevron(
    buf: &mut [u8],
    width: i32,
    cur_x: i32,
    is_open: bool,
    faint_fg: Color,
    scale: i32,
) {
    let Some(mut chev_buf) = alloc_pixel_buffer(STATUS_CHEV_WIDTH, BUTTON_HEIGHT, scale) else {
        return;
    };
    crate::paint::draw_chevron_glyph_over(
        &mut chev_buf,
        STATUS_CHEV_WIDTH,
        BUTTON_HEIGHT,
        9.0,
        1.5,
        is_open,
        scale,
        faint_fg,
    );
    crate::paint::blit_rect_over(
        buf,
        width,
        BUTTON_HEIGHT,
        &chev_buf,
        STATUS_CHEV_WIDTH,
        BUTTON_HEIGHT,
        cur_x,
        0,
        scale,
    );
}

fn draw_status_button(
    theme: &TopbarTheme,
    status: &crate::system_info::SystemStatus,
    painter: &crate::paint::Painter,
    width: i32,
    is_open: bool,
    hovered: bool,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut buf = alloc_pixel_buffer(width, BUTTON_HEIGHT, scale)?;

    if is_open {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.segment_active,
            BUTTON_RADIUS,
            [true; 4],
        );
    } else if hovered {
        crate::paint::Painter::fill_panel_background(
            &mut buf,
            width,
            BUTTON_HEIGHT,
            scale,
            theme.button_hover,
            BUTTON_RADIUS,
            [true; 4],
        );
    }

    let fg = if is_open || hovered {
        theme.label
    } else {
        theme.muted
    };
    let faint_fg = theme.faint;

    let mut cur_x = STATUS_PAD_LEFT;
    draw_status_wifi(
        &mut buf,
        width,
        cur_x,
        status.wifi_level,
        fg,
        faint_fg,
        scale,
    );
    cur_x += STATUS_WIFI_WIDTH + STATUS_GAP;

    draw_status_speaker(&mut buf, width, cur_x, status.volume_muted, fg, scale);
    cur_x += STATUS_SPEAKER_WIDTH;

    if let Some(pct) = status.battery_pct {
        cur_x += STATUS_GAP;
        cur_x = draw_status_battery(
            &mut buf,
            width,
            cur_x,
            pct,
            status.battery_charging,
            painter,
            theme,
            fg,
            scale,
        );
    }

    cur_x += STATUS_GAP;
    draw_status_chevron(&mut buf, width, cur_x, is_open, faint_fg, scale);

    Some(buffer_from(&buf, width, BUTTON_HEIGHT, scale))
}

fn push_quick_settings<R>(
    topbar: &mut Topbar,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if topbar.open_popup != Some(OpenPopup::QuickSettings) {
        return;
    }
    let Some(rect) = topbar.popup_rect else {
        return;
    };
    let theme = &topbar.theme;
    let hovered = topbar.hovered_in(Slot::in_quick_settings);
    let focused = topbar.focused_in(Slot::in_quick_settings);
    let focus = topbar.focus_ring_in(Slot::in_quick_settings);
    let width = rect.size.w;
    let height = rect.size.h;
    let status_rev = topbar.status_rev;
    let caffeinated = topbar.caffeinated;
    let power_armed = topbar.power_armed;
    let status = &topbar.system_status;

    let buffer = topbar.qs_cache.resolve(
        (status_rev, caffeinated, power_armed, hovered, focused),
        output_scale,
        || {
            draw_quick_settings(
                painter,
                theme,
                status,
                QsFlags {
                    caffeinated,
                    power_armed,
                },
                hovered,
                focus,
                width,
                height,
                output_scale,
            )
        },
    );
    let Some(buffer) = buffer else {
        return;
    };
    push_buffer(renderer, buffer, rect.loc, scale, elements);
    push_popup_shadow(topbar, renderer, rect, scale, output_scale, elements);
}

struct QsRenderContext<'a> {
    painter: &'a crate::paint::Painter,
    theme: &'a TopbarTheme,
    width: i32,
    height: i32,
    scale: i32,
    hovered: Option<Slot>,
    power_armed: bool,
}

#[derive(Debug, Clone, Copy)]
struct QsFlags {
    caffeinated: bool,
    power_armed: bool,
}

type GlyphPainter = fn(&mut [u8], i32, i32, f32, f32, i32, Color);

struct QsTileSpec<'a> {
    rect: LocalRect,
    slot: Slot,
    on: bool,
    title: &'a str,
    subtitle: &'a str,
    glyph: GlyphPainter,
}

fn draw_qs_icon_badge(
    buf: &mut [u8],
    ctx: &QsRenderContext<'_>,
    x: i32,
    y: i32,
    on: bool,
    glyph: GlyphPainter,
) {
    let badge_bg = if on {
        ctx.theme.active_background
    } else {
        ctx.theme.track
    };
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        x,
        y,
        QS_ICON_BADGE,
        QS_ICON_BADGE,
        ctx.scale,
        badge_bg,
        glide_theme::shape::R_ROUND,
    );
    let glyph_fg = if on {
        ctx.theme.active_label
    } else {
        ctx.theme.muted
    };
    if let Some(mut glyph_buf) = alloc_pixel_buffer(QS_ICON_BADGE, QS_ICON_BADGE, ctx.scale) {
        glyph(
            &mut glyph_buf,
            QS_ICON_BADGE,
            QS_ICON_BADGE,
            15.0,
            1.5,
            ctx.scale,
            glyph_fg,
        );
        crate::paint::blit_rect_over(
            buf,
            ctx.width,
            ctx.height,
            &glyph_buf,
            QS_ICON_BADGE,
            QS_ICON_BADGE,
            x,
            y,
            ctx.scale,
        );
    }
}

fn draw_qs_tile(buf: &mut [u8], ctx: &QsRenderContext<'_>, spec: &QsTileSpec<'_>) {
    let rect = spec.rect;
    let is_hovered = ctx.hovered == Some(spec.slot);
    let bg = if spec.on {
        ctx.theme.active_soft
    } else {
        ctx.theme.track_soft
    };
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        rect.loc.x,
        rect.loc.y,
        rect.size.w,
        rect.size.h,
        ctx.scale,
        bg,
        glide_theme::shape::R2,
    );
    if is_hovered {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            rect.loc.x,
            rect.loc.y,
            rect.size.w,
            rect.size.h,
            ctx.scale,
            ctx.theme.button_hover,
            glide_theme::shape::R2,
        );
    }

    let ico_x = rect.loc.x + 10;
    let ico_y = rect.loc.y + (rect.size.h - QS_ICON_BADGE) / 2;
    draw_qs_icon_badge(buf, ctx, ico_x, ico_y, spec.on, spec.glyph);

    let txt_x = ico_x + QS_ICON_BADGE + glide_theme::spacing::SP2;
    let txt_right = rect.loc.x + rect.size.w - 6;
    let title_color = if spec.on {
        ctx.theme.active_soft_label
    } else {
        ctx.theme.label
    };
    let sub_color = if spec.on {
        ctx.theme.active_soft_label
    } else {
        ctx.theme.muted
    };
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        txt_x,
        txt_right,
        rect.loc.y + 10,
        rect.loc.y + 26,
        ctx.scale,
        crate::paint::TextAlign::Left,
        spec.title,
        title_color.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        txt_x,
        txt_right,
        rect.loc.y + 28,
        rect.loc.y + 44,
        ctx.scale,
        crate::paint::TextAlign::Left,
        spec.subtitle,
        sub_color.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );
}

fn draw_qs_switch(buf: &mut [u8], ctx: &QsRenderContext<'_>, sw_x: i32, sw_y: i32, on: bool) {
    let track_color = if on {
        ctx.theme.active_background
    } else {
        ctx.theme.track
    };
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        sw_x,
        sw_y,
        QS_SWITCH_W,
        QS_SWITCH_H,
        ctx.scale,
        track_color,
        glide_theme::shape::R_ROUND,
    );
    let thumb_x = if on {
        sw_x + QS_SWITCH_W - QS_SWITCH_INSET - QS_SWITCH_THUMB
    } else {
        sw_x + QS_SWITCH_INSET
    };
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        thumb_x,
        sw_y + QS_SWITCH_INSET,
        QS_SWITCH_THUMB,
        QS_SWITCH_THUMB,
        ctx.scale,
        ctx.theme.thumb,
        glide_theme::shape::R_ROUND,
    );
}

fn draw_qs_caffeine_row(buf: &mut [u8], ctx: &QsRenderContext<'_>, rect: LocalRect, on: bool) {
    let is_hovered = ctx.hovered == Some(Slot::QsCaffeine);
    let bg = if is_hovered {
        ctx.theme.button_hover
    } else {
        ctx.theme.track_soft
    };
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        rect.loc.x,
        rect.loc.y,
        rect.size.w,
        rect.size.h,
        ctx.scale,
        bg,
        glide_theme::shape::R2,
    );

    let ico_x = rect.loc.x + 10;
    let ico_y = rect.loc.y + (rect.size.h - QS_ICON_BADGE) / 2;
    draw_qs_icon_badge(buf, ctx, ico_x, ico_y, on, |b, w, h, side, stroke, s, c| {
        #[allow(clippy::cast_precision_loss)]
        crate::paint::draw_coffee_glyph_over(
            b,
            w,
            h,
            w as f32 / 2.0,
            h as f32 / 2.0,
            side,
            stroke,
            s,
            c,
            false,
        );
    });

    let txt_x = ico_x + QS_ICON_BADGE + glide_theme::spacing::SP2;
    let txt_right = rect.loc.x + rect.size.w - 10 - QS_SWITCH_W - glide_theme::spacing::SP2;
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        txt_x,
        txt_right,
        rect.loc.y + 6,
        rect.loc.y + 22,
        ctx.scale,
        crate::paint::TextAlign::Left,
        crate::i18n::t(crate::i18n::Key::Caffeine),
        ctx.theme.label.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );
    let subtitle = if on {
        crate::i18n::t(crate::i18n::Key::CaffeineOn)
    } else {
        crate::i18n::t(crate::i18n::Key::CaffeineOff)
    };
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        txt_x,
        txt_right,
        rect.loc.y + 22,
        rect.loc.y + 38,
        ctx.scale,
        crate::paint::TextAlign::Left,
        subtitle,
        ctx.theme.muted.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );

    let sw_x = rect.loc.x + rect.size.w - 10 - QS_SWITCH_W;
    let sw_y = rect.loc.y + (rect.size.h - QS_SWITCH_H) / 2;
    draw_qs_switch(buf, ctx, sw_x, sw_y, on);
}

struct QsSliderSpec {
    icon: LocalRect,
    track: LocalRect,
    pct: u8,
    muted: bool,
}

fn draw_qs_slider(
    buf: &mut [u8],
    ctx: &QsRenderContext<'_>,
    spec: &QsSliderSpec,
    draw_icon: impl FnOnce(&mut [u8], i32, i32, i32, Color),
) {
    let icon = spec.icon;
    if let Some(mut ico_buf) = alloc_pixel_buffer(icon.size.w, icon.size.h, ctx.scale) {
        draw_icon(
            &mut ico_buf,
            icon.size.w,
            icon.size.h,
            ctx.scale,
            ctx.theme.muted,
        );
        crate::paint::blit_rect_over(
            buf,
            ctx.width,
            ctx.height,
            &ico_buf,
            icon.size.w,
            icon.size.h,
            icon.loc.x,
            icon.loc.y,
            ctx.scale,
        );
    }

    let track = spec.track;
    let tr_y = track.loc.y + (track.size.h - QS_SLIDER_TRACK_H) / 2;
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        track.loc.x,
        tr_y,
        track.size.w,
        QS_SLIDER_TRACK_H,
        ctx.scale,
        ctx.theme.track,
        glide_theme::shape::R_ROUND,
    );

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let fill_w = (((track.size.w as f32) * f32::from(spec.pct.min(100))) / 100.0).round() as i32;
    if fill_w > 0 {
        let fill_color = if spec.muted {
            ctx.theme.track_soft
        } else {
            ctx.theme.active_background
        };
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            ctx.width,
            ctx.height,
            track.loc.x,
            tr_y,
            fill_w,
            QS_SLIDER_TRACK_H,
            ctx.scale,
            fill_color,
            glide_theme::shape::R_ROUND,
        );
    }

    let thumb_x = (track.loc.x + fill_w - QS_SLIDER_THUMB / 2)
        .clamp(track.loc.x, track.loc.x + track.size.w - QS_SLIDER_THUMB);
    let thumb_y = track.loc.y + (track.size.h - QS_SLIDER_THUMB) / 2;
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        ctx.width,
        ctx.height,
        thumb_x,
        thumb_y,
        QS_SLIDER_THUMB,
        QS_SLIDER_THUMB,
        ctx.scale,
        ctx.theme.thumb,
        glide_theme::shape::R_ROUND,
    );
}

fn draw_qs_avatar(buf: &mut [u8], ctx: &QsRenderContext<'_>, x: i32, y: i32, user_name: &str) {
    crate::paint::fill_rounded_rect_gradient_over(
        buf,
        ctx.width,
        ctx.height,
        x,
        y,
        QS_AVATAR,
        QS_AVATAR,
        ctx.scale,
        Color::from_hex("#a077d8").expect("literal is valid"),
        Color::from_hex("#7a4dbb").expect("literal is valid"),
        glide_theme::shape::R_ROUND,
    );
    let initial = user_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        x,
        x + QS_AVATAR,
        y,
        y + QS_AVATAR,
        ctx.scale,
        crate::paint::TextAlign::Center,
        &initial,
        Color([1.0, 1.0, 1.0, 1.0]).to_buffer_bytes(),
        &glide_theme::TextStyle::OV_LABEL,
    );
}

fn draw_qs_footer(
    buf: &mut [u8],
    ctx: &QsRenderContext<'_>,
    geometry: &QsGeometry,
    user_name: &str,
) {
    let footer = geometry.footer;
    let (x, y) = (footer.loc.x, footer.loc.y);
    let avatar_y = y + (footer.size.h - QS_AVATAR) / 2;
    draw_qs_avatar(buf, ctx, x, avatar_y, user_name);

    let acts_left = geometry.actions[0].1.loc.x;
    let (footer_text, footer_color) = if ctx.power_armed {
        (
            crate::i18n::t(crate::i18n::Key::PowerOffConfirm),
            ctx.theme.danger,
        )
    } else {
        (user_name, ctx.theme.label)
    };
    ctx.painter.draw_label_over_styled_in_band(
        buf,
        ctx.width,
        ctx.height,
        x + QS_AVATAR + glide_theme::spacing::SP2,
        acts_left - glide_theme::spacing::SP2,
        y,
        y + footer.size.h,
        ctx.scale,
        crate::paint::TextAlign::Left,
        footer_text,
        footer_color.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );

    let glyphs: [GlyphPainter; 3] = [
        crate::paint::draw_gear_glyph_over,
        crate::paint::draw_lock_glyph_over,
        crate::paint::draw_power_glyph_over,
    ];

    for ((slot, rect), draw_glyph) in geometry.actions.iter().zip(glyphs) {
        let is_hovered = ctx.hovered == Some(*slot);
        let armed = ctx.power_armed && *slot == Slot::QsPower;
        let background = if armed {
            Some(ctx.theme.danger)
        } else if is_hovered {
            Some(ctx.theme.button_hover)
        } else {
            None
        };
        if let Some(background) = background {
            crate::paint::Painter::fill_rounded_rect_over(
                buf,
                ctx.width,
                ctx.height,
                rect.loc.x,
                rect.loc.y,
                rect.size.w,
                rect.size.h,
                ctx.scale,
                background,
                glide_theme::shape::R2,
            );
        }
        let fg = if armed {
            ctx.theme.active_label
        } else if is_hovered {
            ctx.theme.label
        } else {
            ctx.theme.muted
        };
        if let Some(mut glyph_buf) = alloc_pixel_buffer(rect.size.w, rect.size.h, ctx.scale) {
            draw_glyph(
                &mut glyph_buf,
                rect.size.w,
                rect.size.h,
                15.0,
                1.4,
                ctx.scale,
                fg,
            );
            crate::paint::blit_rect_over(
                buf,
                ctx.width,
                ctx.height,
                &glyph_buf,
                rect.size.w,
                rect.size.h,
                rect.loc.x,
                rect.loc.y,
                ctx.scale,
            );
        }
    }
}

fn draw_qs_tiles(
    buf: &mut [u8],
    ctx: &QsRenderContext<'_>,
    geometry: &QsGeometry,
    status: &crate::system_info::SystemStatus,
) {
    use crate::i18n::{Key, t};
    let wifi_on = status.wifi_level > 0;
    let wifi_sub = if wifi_on && status.wifi_ssid.is_empty() {
        t(Key::Connected)
    } else {
        &status.wifi_ssid
    };
    let on_off = |on: bool| if on { t(Key::On) } else { t(Key::Off) };

    let specs = [
        QsTileSpec {
            rect: geometry.tiles[0].1,
            slot: Slot::QsWifi,
            on: wifi_on,
            title: "Wi-Fi",
            subtitle: wifi_sub,
            glyph: crate::paint::draw_wifi_waves_glyph_over,
        },
        QsTileSpec {
            rect: geometry.tiles[1].1,
            slot: Slot::QsBluetooth,
            on: status.bluetooth_on,
            title: "Bluetooth",
            subtitle: on_off(status.bluetooth_on),
            glyph: crate::paint::draw_bluetooth_glyph_over,
        },
        QsTileSpec {
            rect: geometry.tiles[2].1,
            slot: Slot::QsNightLight,
            on: status.night_light_on,
            title: t(Key::NightLight),
            subtitle: if status.night_light_on {
                t(Key::On)
            } else {
                t(Key::LessBlue)
            },
            glyph: |b, w, h, side, _, s, c| crate::paint::draw_moon_glyph_over(b, w, h, side, s, c),
        },
        QsTileSpec {
            rect: geometry.tiles[3].1,
            slot: Slot::QsScreenshot,
            on: false,
            title: t(Key::Screenshot),
            subtitle: t(Key::WholeScreen),
            glyph: crate::paint::draw_camera_glyph_over,
        },
    ];
    for spec in &specs {
        draw_qs_tile(buf, ctx, spec);
    }
}

fn draw_qs_sliders(
    buf: &mut [u8],
    ctx: &QsRenderContext<'_>,
    geometry: &QsGeometry,
    status: &crate::system_info::SystemStatus,
) {
    draw_qs_slider(
        buf,
        ctx,
        &QsSliderSpec {
            icon: geometry.volume_icon,
            track: geometry.volume_track,
            pct: status.volume_pct,
            muted: status.volume_muted,
        },
        |b, w, h, s, c| {
            crate::paint::draw_speaker_glyph_over(b, w, h, 15.0, 1.4, status.volume_muted, s, c);
        },
    );
    draw_qs_slider(
        buf,
        ctx,
        &QsSliderSpec {
            icon: geometry.brightness_icon,
            track: geometry.brightness_track,
            pct: status.brightness_pct,
            muted: false,
        },
        |b, w, h, s, c| crate::paint::draw_sun_glyph_over(b, w, h, 15.0, 1.4, s, c),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_quick_settings(
    painter: &crate::paint::Painter,
    theme: &TopbarTheme,
    status: &crate::system_info::SystemStatus,
    flags: QsFlags,
    hovered: Option<Slot>,
    focus: Option<LocalRect>,
    width: i32,
    height: i32,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let caffeinated = flags.caffeinated;
    let mut buf = alloc_pixel_buffer(width, height, scale)?;

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        height,
        scale,
        theme.popover_bg,
        POPUP_RADIUS,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        width,
        height,
        0,
        0,
        width,
        height,
        scale,
        1.0,
        POPUP_RADIUS,
        theme.border,
    );

    let ctx = QsRenderContext {
        painter,
        theme,
        width,
        height,
        scale,
        hovered,
        power_armed: flags.power_armed,
    };
    let geometry = qs_geometry();

    draw_qs_tiles(&mut buf, &ctx, &geometry, status);
    draw_qs_caffeine_row(&mut buf, &ctx, geometry.caffeine, caffeinated);
    draw_qs_sliders(&mut buf, &ctx, &geometry, status);

    crate::paint::Painter::fill_rounded_rect_over(
        &mut buf,
        width,
        height,
        QS_PAD,
        geometry.divider_y,
        width - QS_PAD * 2,
        1,
        scale,
        theme.border,
        0.0,
    );

    draw_qs_footer(&mut buf, &ctx, &geometry, &status.user_name);
    draw_focus_ring(&mut buf, width, height, focus, theme, scale);

    Some(buffer_from(&buf, width, height, scale))
}

const fn scale_or_one(scale: i32) -> i32 {
    if scale < 1 { 1 } else { scale }
}

fn buffer_from(buf: &[u8], width: i32, height: i32, scale: i32) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (width * scale, height * scale),
        scale,
        Transform::Normal,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn playing() -> MediaWidget {
        MediaWidget {
            title: "Пісня про рушник".to_string(),
            artist: "Хор".to_string(),
            source: "Rhythmbox".to_string(),
            art: None,
            art_large: None,
            accent: Color([0.9, 0.63, 0.29, 1.0]),
            playing: true,
            can_next: true,
            can_previous: true,
            progress: Some(0.38),
        }
    }

    #[test]
    fn silence_leaves_the_bar_without_a_media_widget() {
        let bar = topbar_with(&["US", "UA"]);
        assert!(!bar.has_media());
        assert!(bar.slot_rect(Slot::Media).is_none());
        assert!(bar.slot_rect(Slot::MediaPlay).is_none());
    }

    #[test]
    fn the_clock_does_not_move_when_a_track_starts() {
        let mut bar = topbar_with(&["US", "UA"]);
        let silent = bar.slot_rect(Slot::Clock).expect("clock slot");

        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());
        let sounding = bar.slot_rect(Slot::Clock).expect("clock slot");

        assert_eq!(
            silent, sounding,
            "the clock is the anchor; media must grow away from it"
        );
    }

    #[test]
    fn media_grows_to_the_left_of_the_clock_without_touching_it() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());

        let clock = bar.slot_rect(Slot::Clock).expect("clock slot");
        let media = bar.slot_rect(Slot::Media).expect("media slot");
        let next = bar.slot_rect(Slot::MediaNext).expect("next slot");

        assert!(media.loc.x < clock.loc.x);
        assert!(next.loc.x + next.size.w <= clock.loc.x);
    }

    #[test]
    fn the_three_transport_targets_sit_in_order_and_do_not_overlap() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());

        let previous = bar.slot_rect(Slot::MediaPrev).expect("previous slot");
        let play = bar.slot_rect(Slot::MediaPlay).expect("play slot");
        let next = bar.slot_rect(Slot::MediaNext).expect("next slot");

        assert!(previous.loc.x + previous.size.w <= play.loc.x);
        assert!(play.loc.x + play.size.w <= next.loc.x);
    }

    #[test]
    fn each_transport_target_reports_its_own_command() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());

        for (slot, want) in [
            (Slot::MediaPrev, TopbarAction::MediaPrevious),
            (Slot::MediaPlay, TopbarAction::MediaPlayPause),
            (Slot::MediaNext, TopbarAction::MediaNext),
        ] {
            let rect = bar.slot_rect(slot).expect("transport slot");
            let centre = Point::from((
                f64::from(rect.loc.x + rect.size.w / 2),
                f64::from(rect.loc.y + rect.size.h / 2),
            ));
            assert_eq!(bar.action_at(centre), Some(want));
        }
    }

    #[test]
    fn a_longer_title_widens_the_widget_but_is_capped() {
        let painter = painter();
        let short = media_width(
            &painter,
            &MediaWidget {
                title: "Hi".to_string(),
                ..playing()
            },
        );
        let long = media_width(
            &painter,
            &MediaWidget {
                title: "x".repeat(400),
                ..playing()
            },
        );
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
    }

    #[test]
    fn a_track_without_an_artist_makes_a_narrower_widget() {
        let painter = painter();
        let with_artist = media_width(&painter, &playing());
        let without = media_width(
            &painter,
            &MediaWidget {
                artist: String::new(),
                ..playing()
            },
        );
        assert!(without < with_artist);
    }

    #[test]
    fn clicking_the_track_name_opens_the_player_rather_than_pausing() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());

        let media = bar.slot_rect(Slot::Media).expect("media slot");
        let centre = Point::from((
            f64::from(media.loc.x + 8),
            f64::from(media.loc.y + media.size.h / 2),
        ));
        assert_eq!(bar.action_at(centre), Some(TopbarAction::ToggleMediaPlayer));
    }

    #[test]
    fn the_player_popup_only_opens_while_something_is_playing() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.toggle_popup(OpenPopup::MediaPlayer);
        bar.layout_slots(&painter(), output());
        assert!(bar.popup_rect().is_none(), "silence has no player to show");

        bar.set_media(Some(playing()));
        bar.layout_slots(&painter(), output());
        assert!(bar.popup_rect().is_some());
    }

    #[test]
    fn the_popup_transport_sits_inside_the_popup() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.toggle_popup(OpenPopup::MediaPlayer);
        bar.layout_slots(&painter(), output());

        let popup = bar.popup_rect().expect("popup rect");
        for slot in [
            Slot::PopMediaPrev,
            Slot::PopMediaPlay,
            Slot::PopMediaNext,
            Slot::PopMediaSeek,
        ] {
            let rect = bar.slot_rect(slot).expect("popup slot");
            assert!(rect.loc.x >= popup.loc.x);
            assert!(rect.loc.x + rect.size.w <= popup.loc.x + popup.size.w);
            assert!(rect.loc.y >= popup.loc.y);
            assert!(rect.loc.y + rect.size.h <= popup.loc.y + popup.size.h);
        }
    }

    #[test]
    fn seeking_reports_the_fraction_the_pointer_landed_on() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.toggle_popup(OpenPopup::MediaPlayer);
        bar.layout_slots(&painter(), output());

        let track = bar.slot_rect(Slot::PopMediaSeek).expect("seek slot");
        let middle = Point::from((
            f64::from(track.loc.x) + f64::from(track.size.w) / 2.0,
            f64::from(track.loc.y + track.size.h / 2),
        ));
        assert_eq!(bar.action_at(middle), Some(TopbarAction::MediaSeek(50)));

        let start = Point::from((
            f64::from(track.loc.x),
            f64::from(track.loc.y + track.size.h / 2),
        ));
        assert_eq!(bar.action_at(start), Some(TopbarAction::MediaSeek(0)));
    }

    #[test]
    fn a_progress_tick_only_repaints_when_the_thousandth_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(
            !bar.set_media_progress(Some(0.5)),
            "there is nothing playing to attach progress to"
        );

        bar.set_media(Some(playing()));
        assert!(bar.set_media_progress(Some(0.5)));
        assert!(!bar.set_media_progress(Some(0.5)));
        assert!(bar.set_media_progress(None));
    }

    #[test]
    fn the_media_widget_only_dirties_the_bar_when_it_actually_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(bar.set_media(Some(playing())));
        assert!(!bar.set_media(Some(playing())));
        assert!(bar.set_media(Some(MediaWidget {
            playing: false,
            ..playing()
        })));
        assert!(bar.set_media(None));
        assert!(!bar.set_media(None));
    }

    #[test]
    fn the_strip_spans_the_output_and_owns_its_own_height() {
        let bar = topbar_with(&["US", "UA"]);
        let strip = bar.rect(output());
        assert_eq!(strip.size.w, 1920);
        assert_eq!(strip.size.h, HEIGHT);
        assert_eq!(strip.loc, output().loc);
    }

    #[test]
    fn the_controls_sit_inside_the_strip() {
        let bar = topbar_with(&["US", "UA"]);
        let strip = bar.rect(output());
        for (_, rect) in &bar.slots {
            assert!(rect.loc.y >= strip.loc.y);
            assert!(rect.loc.y + rect.size.h <= strip.loc.y + strip.size.h);
            assert!(rect.loc.x + rect.size.w <= strip.loc.x + strip.size.w - PAD_X + 1);
        }
    }

    #[test]
    fn two_layouts_render_as_a_segmented_control_with_one_slot_each() {
        let bar = topbar_with(&["US", "UA"]);
        assert!(bar.segmented());
        assert!(bar.slot_rect(Slot::LayoutPick(0)).is_some());
        assert!(bar.slot_rect(Slot::LayoutPick(1)).is_some());
        assert!(bar.slot_rect(Slot::LayoutCycle).is_none());
    }

    #[test]
    fn three_layouts_render_as_a_dropdown_button() {
        let bar = topbar_with(&["US", "UA", "PL"]);
        assert!(!bar.segmented());
        assert!(bar.slot_rect(Slot::LayoutDropdown).is_some());
        assert!(bar.slot_rect(Slot::LayoutPick(0)).is_none());
    }

    #[test]
    fn language_menu_opens_with_correct_slots_and_can_be_closed() {
        let mut bar = topbar_with(&["US", "UA", "PL"]);
        assert!(!bar.has_open_popup());
        assert!(bar.toggle_popup(OpenPopup::LanguageMenu));
        bar.layout_slots(&painter(), output());
        assert!(bar.has_open_popup());
        assert_eq!(bar.open_popup(), Some(OpenPopup::LanguageMenu));
        assert!(bar.slot_rect(Slot::LangMenuItem(0)).is_some());
        assert!(bar.slot_rect(Slot::LangMenuItem(1)).is_some());
        assert!(bar.slot_rect(Slot::LangMenuItem(2)).is_some());
        assert!(bar.slot_rect(Slot::LangMenuSettings).is_some());

        let settings_slot = bar
            .slot_rect(Slot::LangMenuSettings)
            .expect("settings slot");
        let centre = Point::from((
            f64::from(settings_slot.loc.x + settings_slot.size.w / 2),
            f64::from(settings_slot.loc.y + settings_slot.size.h / 2),
        ));
        assert_eq!(bar.action_at(centre), Some(TopbarAction::OpenInputSettings));

        assert!(bar.contains(output(), centre));
        assert!(bar.close_popup());
        bar.layout_slots(&painter(), output());
        assert!(!bar.has_open_popup());
        assert!(bar.slot_rect(Slot::LangMenuItem(0)).is_none());
    }

    #[test]
    fn notification_center_opens_with_items_and_clear_action() {
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
        assert!(bar.toggle_popup(OpenPopup::NotificationCenter));
        bar.layout_slots(&painter(), output());
        assert!(bar.has_open_popup());
        assert_eq!(bar.open_popup(), Some(OpenPopup::NotificationCenter));

        assert!(bar.slot_rect(Slot::NcClearAll).is_some());
        assert!(bar.slot_rect(Slot::NcDndToggle).is_some());
        assert!(bar.slot_rect(Slot::NcItem(42)).is_some());
        assert!(bar.slot_rect(Slot::NcItemDismiss(42)).is_some());

        let item_slot = bar.slot_rect(Slot::NcItem(42)).expect("nc item slot");
        let centre = Point::from((
            f64::from(item_slot.loc.x + item_slot.size.w / 2),
            f64::from(item_slot.loc.y + item_slot.size.h / 2),
        ));
        assert_eq!(
            bar.action_at(centre),
            Some(TopbarAction::ActivateNotification(42))
        );

        let dismiss_slot = bar
            .slot_rect(Slot::NcItemDismiss(42))
            .expect("dismiss slot");
        let dismiss_centre = Point::from((
            f64::from(dismiss_slot.loc.x + dismiss_slot.size.w / 2),
            f64::from(dismiss_slot.loc.y + dismiss_slot.size.h / 2),
        ));
        assert_eq!(
            bar.action_at(dismiss_centre),
            Some(TopbarAction::DismissNotification(42))
        );
    }

    #[test]
    fn a_single_layout_still_draws_but_never_overlaps_the_caffeine_switch() {
        let bar = topbar_with(&["US"]);
        let coffee = bar.slot_rect(Slot::Caffeine).expect("caffeine slot");
        let cycle = bar.slot_rect(Slot::LayoutCycle).expect("layout slot");
        assert!(cycle.loc.x + cycle.size.w <= coffee.loc.x);
    }

    #[test]
    fn no_layouts_means_no_layout_control_at_all() {
        let mut bar = Topbar::new(TopbarTheme::default());
        bar.layout_slots(&painter(), output());
        assert!(bar.slot_rect(Slot::LayoutCycle).is_none());
        assert!(bar.slot_rect(Slot::LayoutPick(0)).is_none());
        assert!(bar.slot_rect(Slot::Caffeine).is_some());
    }

    #[test]
    fn the_segments_do_not_overlap_each_other() {
        let bar = topbar_with(&["US", "UA"]);
        let first = bar.slot_rect(Slot::LayoutPick(0)).expect("first segment");
        let second = bar.slot_rect(Slot::LayoutPick(1)).expect("second segment");
        assert!(first.loc.x + first.size.w <= second.loc.x);
    }

    #[test]
    fn the_segmented_control_box_encloses_every_segment() {
        let bar = topbar_with(&["US", "UA"]);
        let control = bar.layout_control_rect().expect("control rect");
        for index in 0..2 {
            let seg = bar.slot_rect(Slot::LayoutPick(index)).expect("segment");
            assert!(seg.loc.x >= control.loc.x);
            assert!(seg.loc.x + seg.size.w <= control.loc.x + control.size.w);
            assert!(seg.loc.y >= control.loc.y);
            assert!(seg.loc.y + seg.size.h <= control.loc.y + control.size.h);
        }
    }

    #[test]
    fn clicking_a_segment_selects_that_layout_rather_than_cycling() {
        let bar = topbar_with(&["US", "UA"]);
        let second = bar.slot_rect(Slot::LayoutPick(1)).expect("second segment");
        let centre = Point::from((
            f64::from(second.loc.x + second.size.w / 2),
            f64::from(second.loc.y + second.size.h / 2),
        ));
        assert_eq!(bar.action_at(centre), Some(TopbarAction::SelectLayout(1)));
    }

    #[test]
    fn clicking_a_control_reports_its_action() {
        let bar = topbar_with(&["US", "UA"]);
        for (slot, rect) in &bar.slots {
            let centre = Point::from((
                f64::from(rect.loc.x + rect.size.w / 2),
                f64::from(rect.loc.y + rect.size.h / 2),
            ));
            assert_eq!(bar.action_at(centre), Some(slot.action()));
        }
    }

    #[test]
    fn empty_strip_space_is_not_a_control() {
        let bar = topbar_with(&["US", "UA"]);
        assert_eq!(bar.action_at(Point::from((600.0, 17.0))), None);
    }

    #[test]
    fn the_app_chip_never_reaches_the_centred_clock() {
        let bar = topbar_with(&["US", "UA"]);
        let painter = painter();
        let chip = PAD_X + app_chip_width(&painter, &bar.app_name, true);
        let clock = clock_width(&painter, &bar.date, &bar.clock, bar.unread);
        let clock_left = (output().size.w - clock) / 2;
        assert!(chip < clock_left);
    }

    #[test]
    fn a_point_below_the_strip_is_not_the_topbar() {
        let bar = topbar_with(&["US", "UA"]);
        assert!(!bar.contains(output(), Point::from((100.0, f64::from(HEIGHT) + 1.0))));
        assert!(bar.contains(output(), Point::from((100.0, 4.0))));
    }

    #[test]
    fn hover_only_reports_a_change_when_the_control_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        let coffee = bar.slot_rect(Slot::Caffeine).expect("caffeine slot");
        let centre = Point::from((
            f64::from(coffee.loc.x + coffee.size.w / 2),
            f64::from(coffee.loc.y + coffee.size.h / 2),
        ));

        assert!(bar.set_hover(Some(centre)));
        assert!(!bar.set_hover(Some(centre)), "same control, no repaint");
        assert!(bar.set_hover(None));
        assert!(!bar.set_hover(None));
    }

    #[test]
    fn the_clock_only_dirties_the_strip_when_it_actually_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.set_clock("Mon 21 Sep".to_string(), "12:34".to_string()));
        assert!(bar.set_clock("Mon 21 Sep".to_string(), "12:35".to_string()));
        assert!(bar.set_clock("Tue 22 Sep".to_string(), "12:35".to_string()));
    }

    #[test]
    fn the_layout_set_only_dirties_the_strip_when_it_actually_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.set_layouts(vec!["US".to_string(), "UA".to_string()], 0));
        assert!(bar.set_layouts(vec!["US".to_string(), "UA".to_string()], 1));
        assert!(bar.set_layouts(vec!["US".to_string()], 0));
    }

    #[test]
    fn the_app_chip_only_dirties_the_strip_when_it_actually_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.set_app("org.gnome.Nautilus".to_string(), "Nautilus".to_string()));
        assert!(bar.set_app("kitty".to_string(), "Kitty".to_string()));
    }

    #[test]
    fn a_wider_layout_name_widens_its_segment() {
        let painter = painter();
        let narrow = segment_width(&painter, "US");
        let wide = segment_width(&painter, "UKR");
        assert!(wide >= narrow);
        assert!(narrow >= SEG_MIN_WIDTH);
    }

    #[test]
    fn a_disabled_topbar_claims_no_input_and_draws_nothing() {
        let bar = Topbar::new(TopbarTheme {
            enabled: false,
            ..TopbarTheme::default()
        });
        assert!(!bar.contains(output(), Point::from((100.0, 4.0))));
    }

    #[test]
    fn short_layout_name_prefers_the_parenthesised_variant() {
        assert_eq!(short_layout_name("Ukrainian"), "UKR");
        assert_eq!(short_layout_name("English (US)"), "US");
        assert_eq!(short_layout_name("Polish (qwertz)"), "QWE");
    }

    #[test]
    fn the_app_name_comes_from_the_last_segment_of_a_reverse_dns_id() {
        assert_eq!(app_display_name("org.gnome.Nautilus", ""), "Nautilus");
        assert_eq!(app_display_name("kitty", ""), "Kitty");
        assert_eq!(app_display_name("", "Some Window"), "Some Window");
    }

    #[test]
    fn a_very_long_app_name_is_truncated_rather_than_pushing_into_the_clock() {
        let long = "a".repeat(80);
        let shown = app_display_name(&long, "");
        assert!(shown.chars().count() <= APP_NAME_MAX);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn the_caffeine_switch_is_wide_enough_for_its_glyph_and_track() {
        let width = Topbar::caffeine_width();
        assert!(width > SWITCH_WIDTH);
        assert!(width >= BUTTON_MIN_WIDTH);
    }

    #[test]
    fn the_clock_reserves_room_for_both_the_date_and_the_time() {
        let painter = painter();
        let with_date = clock_width(&painter, "Mon 21 Sep", "12:34", 0);
        let without = clock_width(&painter, "", "12:34", 0);
        assert!(with_date > without);
    }

    #[test]
    fn an_unread_notification_count_expands_the_clock() {
        let painter = painter();
        let without_unread = clock_width(&painter, "Mon 21 Sep", "12:34", 0);
        let with_unread = clock_width(&painter, "Mon 21 Sep", "12:34", 3);
        assert!(with_unread > without_unread);
    }

    #[test]
    fn the_do_not_disturb_button_sits_to_the_left_of_layouts() {
        let bar = topbar_with(&["US", "UA"]);
        let dnd = bar.slot_rect(Slot::DoNotDisturb).expect("dnd slot");
        let first_layout = bar.slot_rect(Slot::LayoutPick(0)).expect("first layout");
        assert!(dnd.loc.x + dnd.size.w <= first_layout.loc.x);
    }

    #[test]
    fn toggling_dnd_dirties_the_topbar_only_when_it_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.set_dnd(false));
        assert!(bar.set_dnd(true));
        assert!(!bar.set_dnd(true));
        assert!(bar.set_dnd(false));
    }

    #[test]
    fn toggling_unread_dirties_the_topbar_only_when_it_changes() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.set_unread(0));
        assert!(bar.set_unread(1));
        assert!(!bar.set_unread(1));
        assert!(bar.set_unread(0));
    }

    #[test]
    fn local_clock_reads_as_a_wall_clock() {
        let clock = local_clock().expect("the system clock must be readable");
        assert_eq!(clock.len(), 5);
        assert_eq!(clock.as_bytes()[2], b':');
    }

    #[test]
    fn local_date_is_never_padded_with_runs_of_spaces() {
        let date = local_date().expect("the system clock must be readable");
        assert!(!date.contains("  "));
        assert!(!date.starts_with(' '));
    }

    #[test]
    fn quick_settings_opens_with_correct_slots_and_can_be_closed() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(bar.slot_rect(Slot::Status).is_some());
        assert!(!bar.has_open_popup());
        assert!(bar.toggle_popup(OpenPopup::QuickSettings));
        bar.layout_slots(&painter(), output());
        assert!(bar.has_open_popup());
        assert_eq!(bar.open_popup(), Some(OpenPopup::QuickSettings));

        assert!(bar.slot_rect(Slot::QsWifi).is_some());
        assert!(bar.slot_rect(Slot::QsBluetooth).is_some());
        assert!(bar.slot_rect(Slot::QsNightLight).is_some());
        assert!(bar.slot_rect(Slot::QsScreenshot).is_some());
        assert!(bar.slot_rect(Slot::QsCaffeine).is_some());
        assert!(bar.slot_rect(Slot::QsVolumeMute).is_some());
        assert!(bar.slot_rect(Slot::QsVolumeTrack).is_some());
        assert!(bar.slot_rect(Slot::QsBrightnessTrack).is_some());
        assert!(bar.slot_rect(Slot::QsSettings).is_some());
        assert!(bar.slot_rect(Slot::QsLock).is_some());
        assert!(bar.slot_rect(Slot::QsPower).is_some());
        assert!(bar.slot_rect(Slot::Caffeine).is_some());

        let wifi_slot = bar.slot_rect(Slot::QsWifi).expect("wifi slot");
        let centre = Point::from((
            f64::from(wifi_slot.loc.x + wifi_slot.size.w / 2),
            f64::from(wifi_slot.loc.y + wifi_slot.size.h / 2),
        ));
        assert_eq!(bar.action_at(centre), Some(TopbarAction::QsToggleWifi));

        let vol_slot = bar.slot_rect(Slot::QsVolumeTrack).expect("vol slot");
        let quarter = Point::from((
            f64::from(vol_slot.loc.x) + f64::from(vol_slot.size.w) * 0.25,
            f64::from(vol_slot.loc.y + vol_slot.size.h / 2),
        ));
        assert_eq!(
            bar.action_at(quarter),
            Some(TopbarAction::QsVolumeAdjust(25))
        );

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
        let footer_bottom = geometry.footer.loc.y + geometry.footer.size.h;
        assert_eq!(geometry.height, footer_bottom + QS_PAD);
        for (_, rect) in geometry.tiles.iter().chain(geometry.actions.iter()) {
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
        for popup in [
            OpenPopup::QuickSettings,
            OpenPopup::NotificationCenter,
            OpenPopup::LanguageMenu,
            OpenPopup::MediaPlayer,
        ] {
            bar.close_popup();
            bar.toggle_popup(popup);
            bar.layout_slots(&painter(), output());
            tops.push(bar.popup_rect().expect("popup rect").loc.y);
        }
        assert!(
            tops.iter().all(|top| *top == HEIGHT + POPUP_GAP),
            "{tops:?}"
        );
    }

    #[test]
    fn the_player_popup_is_centred_under_the_media_widget() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.set_media(Some(playing()));
        bar.toggle_popup(OpenPopup::MediaPlayer);
        bar.layout_slots(&painter(), output());

        let popup = bar.popup_rect().expect("popup rect");
        let media = bar.slot_rect(Slot::Media).expect("media slot");
        let next = bar.slot_rect(Slot::MediaNext).expect("next slot");
        let widget_right = next.loc.x + next.size.w + MEDIA_PAD;
        let widget_centre = i32::midpoint(media.loc.x, widget_right);
        let popup_centre = popup.loc.x + popup.size.w / 2;
        assert!(
            (widget_centre - popup_centre).abs() <= 1,
            "widget centre {widget_centre}, popup centre {popup_centre}"
        );
    }

    #[test]
    fn hovering_does_not_invalidate_the_layout() {
        let mut bar = topbar_with(&["US", "UA"]);
        let before = bar.layout_key;
        let coffee = bar.slot_rect(Slot::Caffeine).expect("caffeine slot");
        let centre = Point::from((
            f64::from(coffee.loc.x + coffee.size.w / 2),
            f64::from(coffee.loc.y + coffee.size.h / 2),
        ));
        assert!(bar.set_hover(Some(centre)));
        bar.layout_slots(&painter(), output());
        assert_eq!(bar.layout_key, before, "hover is paint state, not geometry");

        assert!(bar.set_unread(4));
        bar.layout_slots(&painter(), output());
        assert_ne!(bar.layout_key, before, "a badge changes the clock width");
    }

    #[test]
    fn a_widget_only_sees_hover_on_its_own_controls() {
        let mut bar = topbar_with(&["US", "UA"]);
        let coffee = bar.slot_rect(Slot::Caffeine).expect("caffeine slot");
        let centre = Point::from((
            f64::from(coffee.loc.x + coffee.size.w / 2),
            f64::from(coffee.loc.y + coffee.size.h / 2),
        ));
        bar.set_hover(Some(centre));
        assert_eq!(bar.hovered_in(Slot::in_layout), None);
        assert_eq!(bar.hovered_in(Slot::in_media), None);
        assert_eq!(bar.hovered_in(Slot::in_quick_settings), None);
        assert_eq!(bar.hovered_in(Slot::in_strip), Some(Slot::Caffeine));
    }

    #[test]
    fn the_screenshot_tile_reports_a_screenshot() {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        let tile = bar.slot_rect(Slot::QsScreenshot).expect("screenshot tile");
        let centre = Point::from((
            f64::from(tile.loc.x + tile.size.w / 2),
            f64::from(tile.loc.y + tile.size.h / 2),
        ));
        assert_eq!(bar.action_at(centre), Some(TopbarAction::QsScreenshot));
        let row = bar.slot_rect(Slot::QsCaffeine).expect("caffeine row");
        let centre = Point::from((
            f64::from(row.loc.x + row.size.w / 2),
            f64::from(row.loc.y + row.size.h / 2),
        ));
        assert_eq!(bar.action_at(centre), Some(TopbarAction::ToggleCaffeine));
    }

    #[test]
    fn a_theme_built_from_light_tokens_is_not_dark() {
        let tokens =
            glide_theme::Tokens::new(glide_theme::Appearance::Light, glide_theme::Accent::Blue);
        let theme = TopbarTheme::from_tokens(&tokens, true);
        assert!(!theme.dark);
        assert_ne!(theme.revision(), TopbarTheme::default().revision());
    }

    #[test]
    fn status_button_reserves_room_for_battery_when_present() {
        let painter = painter();
        let mut bar = Topbar::new(TopbarTheme::default());
        let without_batt = bar.status_button_width(&painter);

        let status = crate::system_info::SystemStatus {
            battery_pct: Some(82),
            ..Default::default()
        };
        bar.set_system_status(status);
        let with_batt = bar.status_button_width(&painter);

        assert!(with_batt > without_batt);
    }

    fn quick_settings() -> Topbar {
        let mut bar = topbar_with(&["US", "UA"]);
        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        bar
    }

    #[test]
    fn the_first_arrow_press_lands_on_the_top_left_tile() {
        let mut bar = quick_settings();
        assert!(!bar.has_focus());
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused, Some(Slot::QsWifi));
        assert!(bar.has_focus());
    }

    #[test]
    fn arrows_walk_the_quick_settings_tiles_left_right_and_down() {
        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused, Some(Slot::QsBluetooth));
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::QsScreenshot));
        assert!(bar.focus_nav(-1, 0));
        assert_eq!(bar.focused, Some(Slot::QsNightLight));
        assert!(bar.focus_nav(0, -1));
        assert_eq!(bar.focused, Some(Slot::QsWifi));
        assert!(!bar.focus_nav(-1, 0), "nothing lies left of the first tile");
        assert!(!bar.focus_nav(0, -1), "nothing lies above the first row");
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
                Slot::QsSettings,
            ]
        );
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused, Some(Slot::QsLock));
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused, Some(Slot::QsPower));
    }

    #[test]
    fn enter_on_a_focused_tile_reports_that_tiles_action() {
        let mut bar = quick_settings();
        assert_eq!(bar.focused_action(), None);
        bar.focus_nav(1, 0);
        assert_eq!(bar.focused_action(), Some(TopbarAction::QsToggleWifi));
        bar.focus_nav(1, 0);
        assert_eq!(bar.focused_action(), Some(TopbarAction::QsToggleBluetooth));
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
        assert_eq!(
            bar.focus_adjust(-5),
            Some(TopbarAction::QsBrightnessAdjust(0))
        );
        assert_eq!(
            bar.focus_adjust(5),
            Some(TopbarAction::QsBrightnessAdjust(5))
        );

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
    fn closing_a_popup_drops_keyboard_focus() {
        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        assert!(bar.has_focus());
        assert!(bar.close_popup());
        assert!(!bar.has_focus());
        assert_eq!(bar.focused_action(), None);

        bar.toggle_popup(OpenPopup::QuickSettings);
        bar.layout_slots(&painter(), output());
        bar.focus_nav(1, 0);
        assert!(bar.has_focus());
        bar.toggle_popup(OpenPopup::QuickSettings);
        assert!(!bar.has_focus());
    }

    #[test]
    fn focus_goes_nowhere_without_an_open_popup() {
        let mut bar = topbar_with(&["US", "UA"]);
        assert!(!bar.focus_nav(1, 0));
        assert!(!bar.has_focus());
        assert_eq!(bar.focus_adjust(5), None);
    }

    #[test]
    fn a_zero_step_never_moves_focus() {
        let mut bar = quick_settings();
        assert!(!bar.focus_nav(0, 0));
        assert!(!bar.has_focus());
    }

    #[test]
    fn arrows_walk_the_language_menu_top_to_bottom() {
        let mut bar = topbar_with(&["US", "UA", "PL"]);
        bar.toggle_popup(OpenPopup::LanguageMenu);
        bar.layout_slots(&painter(), output());
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
        bar.toggle_popup(OpenPopup::NotificationCenter);
        bar.layout_slots(&painter(), output());

        bar.focus_nav(0, 1);
        assert_eq!(bar.focused, Some(Slot::NcClearAll));
        assert!(bar.focus_nav(0, 1));
        assert_eq!(
            bar.focused,
            Some(Slot::NcItem(1)),
            "the dismiss button shares no column with clear-all"
        );
        assert!(bar.focus_nav(1, 0));
        assert_eq!(bar.focused, Some(Slot::NcItemDismiss(1)));
        assert!(bar.focus_nav(0, 1));
        assert_eq!(bar.focused, Some(Slot::NcItemDismiss(2)));
        assert!(bar.focus_nav(-1, 0));
        assert_eq!(bar.focused, Some(Slot::NcItem(2)));
        assert_eq!(
            bar.focused_action(),
            Some(TopbarAction::ActivateNotification(2))
        );
    }

    #[test]
    fn the_focus_ring_is_reported_in_popup_local_coordinates() {
        let mut bar = quick_settings();
        bar.focus_nav(1, 0);
        let popup = bar.popup_rect().expect("popup rect");
        let slot = bar.slot_rect(Slot::QsWifi).expect("wifi slot");
        let ring = bar
            .focus_ring_in(Slot::in_quick_settings)
            .expect("ring for the focused tile");
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
        let tile = bar.slot_rect(Slot::QsBluetooth).expect("bluetooth tile");
        let centre = Point::from((
            f64::from(tile.loc.x + tile.size.w / 2),
            f64::from(tile.loc.y + tile.size.h / 2),
        ));
        assert!(bar.set_hover(Some(centre)));
        assert_eq!(
            bar.hovered_in(Slot::in_quick_settings),
            Some(Slot::QsBluetooth)
        );
        assert_eq!(bar.focused_in(Slot::in_quick_settings), Some(Slot::QsWifi));
    }

    #[test]
    fn focus_ring_stroke_matches_its_inset() {
        assert_eq!(FOCUS_RING, 2);
        assert!((FOCUS_RING_STROKE - 2.0).abs() < f32::EPSILON);
    }
}
