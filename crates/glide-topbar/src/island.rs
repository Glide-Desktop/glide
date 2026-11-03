use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glide_theme::TextStyle;
use glide_theme::elevation::Level;
use glide_theme::shape::{R_ROUND, R1, R2};
use glide_theme::spacing::{SP1, SP2, SP3, SP4};
use smithay::backend::renderer::element::{
    Kind,
    memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
};
use smithay::utils::{Logical, Point, Rectangle, Scale, Size};

use super::glyphs::{Glyph, glyph};
use super::keychain_view;
use super::polkit_view;
use super::{
    Anim, BadgePop, Cache, E_IO, E_OUT, Frame, LocalRect, MEDIA_ART, MEDIA_ART_LARGE, MediaWidget, OpenPopup, Slot,
    SwitchMotion, Topbar, TopbarAction, TopbarTheme, calendar, draw_media_art, draw_switch, initial, local_rect,
    motion, numeric_label_width, push_buffer, scaled_about_centre, transport_enabled,
};
use crate::i18n::Lang;
use crate::notifications::{NcItem, Notice, NoticeReply};
use crate::paint::{Painter, TextAlign};
use crate::popup_menu::text_px as text_w;
use crate::raster::{Canvas, ShellRenderer};
use glide_theme::Color;

pub(super) const TOP: i32 = (super::HEIGHT - HAIRLINE - COMPACT_H) / 2;
pub(super) const WIDE: i32 = 356;
pub(super) const VISIBLE_ITEMS: usize = 4;
pub const ALERT_HOLD: Duration = Duration::from_millis(3500);

const HAIRLINE: i32 = 1;
const COMPACT_W: i32 = 344;
const COMPACT_H: i32 = 29;
const COMPACT_PAD: i32 = 11;
const ALERT_W: i32 = WIDE * 2;
const ALERT_H: i32 = 84;
const ALERT_PAD: i32 = SP4;
const ALERT_LINES: usize = 2;
const PAD: i32 = SP3;
const SUB_H: i32 = 25;
const SUB_PAD: i32 = 6;
const SUB_GAP: i32 = 7;
const SEP_GAP: i32 = SP2;
const SEP_H: i32 = 13;
const BARS_W: i32 = 12;
const BARS_H: i32 = 11;
const BAR_W: i32 = 3;
const BAR_PAUSED: i32 = 3;
const BARS_STILL: [i32; 3] = [4, BARS_H, 7];
const BAR_SWINGS: [(i32, i32, Duration); 3] = [
    (3, 10, Duration::from_millis(600)),
    (BARS_H, 4, Duration::from_millis(500)),
    (5, BARS_H, Duration::from_millis(700)),
];
const BARS_LIVE: Duration = Duration::from_millis(100);
const TITLE_MAX: i32 = 90;
const BADGE_H: i32 = 17;
const BADGE_PAD: i32 = 6;
const ALERT_ICON: i32 = 52;
const ALERT_CLOSE: i32 = 28;
const NOTICE_TITLE_LINES: usize = 2;
const NOTICE_BODY_LINES: usize = 4;
const NOTICE_ROW_GAP: i32 = SP3;
const BUTTON_H: i32 = 28;
const BUTTON_PAD: i32 = 11;
const FIELD_H: i32 = 34;
const FIELD_PAD: i32 = SP3;
const SEND: i32 = 26;
const CARET_H: i32 = 15;
const HEADER_H: i32 = 24;
const NC_HEADER_H: i32 = 26;
const MINI_H: i32 = 22;
const MINI_PAD: i32 = SP2;
const MINI_ICON: i32 = 12;
const MINI_GAP: i32 = 5;
const HEADER_GAP: i32 = 6;
const COLLAPSE: i32 = 22;
const DOT: i32 = 6;
const PILL_H: i32 = 18;
const TRACK_H: i32 = 5;
const THUMB: i32 = 12;
const TIMES_H: i32 = 14;
const TRANSPORT: i32 = 32;
const TRANSPORT_MAIN: i32 = 38;
const SECTION_H: i32 = 22;
const LINK_H: i32 = 19;
const TOGGLE_W: i32 = 26;
const TOGGLE_H: i32 = 15;
const MOON: i32 = 13;
const ITEM_H: i32 = 52;
const ITEM_GAP: i32 = 5;
const ITEM_ICON: i32 = 30;
const ITEM_DISMISS: i32 = 20;
const EMPTY_H: i32 = 75;
const EMPTY_ICON: i32 = 22;
const SCROLL_W: i32 = 3;
const SCRIM_ALPHA: f32 = 0.25;
const ENTER_RISE: f64 = 4.0;
const SURFACE_CACHE_LIMIT: usize = 64;
const INSET_HIGHLIGHT: f32 = 0.18;

const MEDIA_H: i32 =
    PAD + HEADER_H + PAD + MEDIA_ART_LARGE + PAD + TRACK_H + SP1 + TIMES_H + PAD + TRANSPORT_MAIN + PAD;
const CAL_TOP: i32 = PAD + NC_HEADER_H + SP2;
const DIVIDER_Y: i32 = CAL_TOP + calendar::BLOCK_H + SP2 + 2;
const SECTION_Y: i32 = DIVIDER_Y + 1 + 2 + SP2;
const STACK_Y: i32 = SECTION_Y + SECTION_H + SP2;
const MEDIA_MIN: i32 = SUB_PAD * 2 + MEDIA_ART + SUB_GAP + BARS_W;

const fn style(size_px: f32, line_height_px: f32, weight: u16, tabular: bool) -> TextStyle {
    TextStyle { size_px, line_height_px, weight, tracking_em: 0.0, uppercase: false, tabular }
}

pub(super) const DATE_STYLE: TextStyle = style(12.0, 15.0, 500, false);
pub(super) const TIME_STYLE: TextStyle = style(12.0, 15.0, 550, true);
const TITLE_STYLE: TextStyle = style(12.0, 15.0, 550, false);
const BADGE_STYLE: TextStyle = style(10.0, 13.0, 600, true);
const ALERT_TITLE_STYLE: TextStyle = style(15.0, 20.0, 650, false);
const ALERT_BODY_STYLE: TextStyle = style(13.5, 18.0, 400, false);
const PILL_STYLE: TextStyle = style(11.0, 14.0, 600, false);
const MINI_STYLE: TextStyle = style(11.0, 14.0, 550, false);
const TRACK_TITLE_STYLE: TextStyle = style(13.5, 18.0, 640, false);
const ARTIST_STYLE: TextStyle = style(12.5, 17.0, 400, false);
const ALBUM_STYLE: TextStyle = style(11.0, 14.0, 400, false);
const TIMES_STYLE: TextStyle = style(10.5, 14.0, 400, true);
const VIEW_TITLE_STYLE: TextStyle = style(12.0, 16.0, 650, false);
const LINK_STYLE: TextStyle = style(11.5, 15.0, 500, false);
const ITEM_TITLE_STYLE: TextStyle = style(12.5, 17.0, 630, false);
const ITEM_BODY_STYLE: TextStyle = style(11.5, 15.0, 400, false);
const ITEM_TIME_STYLE: TextStyle = style(10.5, 14.0, 400, true);
const COUNT_STYLE: TextStyle = style(9.5, 12.0, 600, true);
const EMPTY_STYLE: TextStyle = style(12.5, 17.0, 400, false);
const CAPTION_STYLE: TextStyle = style(12.0, 16.0, 500, false);
const BUTTON_STYLE: TextStyle = style(12.5, 16.0, 560, false);
const FIELD_STYLE: TextStyle = style(13.0, 18.0, 400, false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum View {
    #[default]
    Compact,
    Alert,
    Notice,
    Pill,
    Media,
    Calendar,
    Polkit,
    Keychain,
}

impl View {
    pub(super) const fn expanded(self) -> bool {
        matches!(self, Self::Media | Self::Calendar)
    }

    pub(super) const fn dims(self) -> bool {
        self.expanded() || matches!(self, Self::Polkit | Self::Keychain)
    }

    const fn elevation(self) -> Level {
        match self {
            Self::Compact => Level::El1,
            Self::Alert | Self::Notice | Self::Pill => Level::El2,
            Self::Media | Self::Calendar | Self::Polkit | Self::Keychain => Level::El3,
        }
    }

    const fn radius(self) -> f32 {
        match self {
            Self::Polkit => polkit_view::RADIUS,
            Self::Keychain => keychain_view::RADIUS,
            _ => R2,
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::Compact => 0,
            Self::Alert => 1,
            Self::Media => 2,
            Self::Calendar => 3,
            Self::Notice => 4,
            Self::Pill => 5,
            Self::Polkit => 6,
            Self::Keychain => 7,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IslandAlert {
    pub id: u32,
    pub heading: String,
    pub summary: String,
    pub icon: Option<Arc<Vec<u8>>>,
}

impl IslandAlert {
    #[must_use]
    pub fn from_item(item: &NcItem) -> Self {
        let (title, body) = (item.title.trim(), item.body.trim());
        let app = item.app_name.trim();
        let (heading, summary) = match (app.is_empty(), title.is_empty(), body.is_empty()) {
            (true, _, _) => (title.to_string(), body.to_string()),
            (false, true, _) => (app.to_string(), body.to_string()),
            (false, false, true) => (app.to_string(), title.to_string()),
            (false, false, false) => (app.to_string(), format!("{title}: {body}")),
        };
        Self { id: item.id, heading, summary, icon: item.icon.clone() }
    }
}

type CompactKey = (u64, String, String, usize, u8, i32, ClockWidths);
type MediaKey = (u64, Option<u16>, i32, Lang);
type NcKey = (u64, bool, (u8, u8), Option<Slot>, usize, Option<i32>, Lang);
type PillKey = (Pill, Lang);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PillDot {
    None,
    Accent,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Pill {
    asks: bool,
    dot: PillDot,
    more: usize,
}

impl Pill {
    fn label(self) -> &'static str {
        use crate::i18n::{Key, t};
        t(if self.asks { Key::NeedsAnswer } else { Key::NewNotification })
    }
}

#[derive(Debug)]
pub(super) struct Island {
    pub(super) view: View,
    previous: View,
    fade: Anim,
    from: Size<i32, Logical>,
    to: Size<i32, Logical>,
    morph: Anim,
    scrim: Anim,
    alert: Option<(IslandAlert, Instant)>,
    alert_rev: u64,
    notice: Option<Notice>,
    notice_rev: u64,
    fullscreen: bool,
    peek: Option<u32>,
    first_item: usize,
    compact_w: i32,
    backgrounds: HashMap<(i32, i32, i32, u32), MemoryRenderBuffer>,
    shadows: HashMap<(u8, i32, i32, i32), MemoryRenderBuffer>,
    scrim_buffer: Option<MemoryRenderBuffer>,
    compact: Cache<CompactKey>,
    alert_cache: Cache<u64>,
    notice_cache: Cache<u64>,
    pill_cache: Cache<PillKey>,
    media: Cache<MediaKey>,
    nc: Cache<NcKey>,
    nc_seen: Option<((u64, usize), MemoryRenderBuffer)>,
    nc_old: Option<MemoryRenderBuffer>,
    nc_fade: Anim,
    epoch: Instant,
    bars_drawn: Option<Instant>,
    bar_ink: Option<Color>,
    bars: HashMap<(i32, i32), MemoryRenderBuffer>,
    polkit: Option<crate::polkit::View>,
    polkit_motion: crate::polkit::Motion,
    polkit_rev: u64,
    polkit_peek: bool,
    polkit_layout: Option<((u64, Lang), polkit_view::Layout)>,
    polkit_cache: Cache<(u64, Lang)>,
    polkit_popover: Cache<Lang>,
    polkit_popover_shadow: Cache<(i32, i32)>,
    polkit_spinner: HashMap<(u32, i32), MemoryRenderBuffer>,
    polkit_sweep: Vec<(Color, MemoryRenderBuffer)>,
    keychain: Option<crate::keychain_prompt::View>,
    keychain_motion: crate::polkit::Motion,
    keychain_rev: u64,
    keychain_layout: Option<((u64, Lang), keychain_view::Layout)>,
    keychain_cache: Cache<(u64, Lang)>,
}

impl Default for Island {
    fn default() -> Self {
        Self {
            view: View::Compact,
            previous: View::Compact,
            fade: Anim::at(1.0),
            from: Size::default(),
            to: Size::default(),
            morph: Anim::at(1.0),
            scrim: Anim::at(0.0),
            alert: None,
            alert_rev: 0,
            notice: None,
            notice_rev: 0,
            fullscreen: false,
            peek: None,
            first_item: 0,
            compact_w: COMPACT_W,
            backgrounds: HashMap::new(),
            shadows: HashMap::new(),
            scrim_buffer: None,
            compact: Cache::default(),
            alert_cache: Cache::default(),
            notice_cache: Cache::default(),
            pill_cache: Cache::default(),
            media: Cache::default(),
            nc: Cache::default(),
            nc_seen: None,
            nc_old: None,
            nc_fade: Anim::at(1.0),
            epoch: glide_ui::clock::now(),
            bars_drawn: None,
            bar_ink: None,
            bars: HashMap::new(),
            polkit: None,
            polkit_motion: crate::polkit::Motion::default(),
            polkit_rev: 0,
            polkit_peek: false,
            polkit_layout: None,
            polkit_cache: Cache::default(),
            polkit_popover: Cache::default(),
            polkit_popover_shadow: Cache::default(),
            polkit_spinner: HashMap::new(),
            polkit_sweep: Vec::new(),
            keychain: None,
            keychain_motion: crate::polkit::Motion::default(),
            keychain_rev: 0,
            keychain_layout: None,
            keychain_cache: Cache::default(),
        }
    }
}

fn lerp_i32(from: i32, to: i32, t: f64) -> i32 {
    motion::lerp(f64::from(from), f64::from(to), t).round() as i32
}

fn placed(strip: Rectangle<i32, Logical>, size: Size<i32, Logical>) -> LocalRect {
    let centre = strip.loc.x + strip.size.w / 2;
    local_rect(centre - size.w / 2, strip.loc.y + TOP, size.w, size.h)
}

impl Island {
    pub(super) fn clear(&mut self) {
        self.backgrounds.clear();
        self.shadows.clear();
        self.scrim_buffer = None;
        self.compact.clear();
        self.alert_cache.clear();
        self.notice_cache.clear();
        self.pill_cache.clear();
        self.media.clear();
        self.nc.clear();
        self.bar_ink = None;
        self.bars.clear();
        self.polkit_layout = None;
        self.polkit_cache.clear();
        self.polkit_popover.clear();
        self.polkit_popover_shadow.clear();
        self.polkit_spinner.clear();
        self.polkit_sweep.clear();
        self.keychain_layout = None;
        self.keychain_cache.clear();
    }

    pub(super) fn is_animating(&self) -> bool {
        self.fade.is_animating()
            || self.morph.is_animating()
            || self.scrim.is_animating()
            || self.nc_fade.is_animating()
            || self.bars_live()
    }

    fn polkit_layout(&mut self, painter: &Painter) -> Option<polkit_view::Layout> {
        let view = self.polkit.as_ref()?;
        let key = (self.polkit_rev, crate::i18n::language());
        if let Some((cached, layout)) = self.polkit_layout.as_ref()
            && *cached == key
        {
            return Some(layout.clone());
        }
        let layout = polkit_view::layout(painter, view);
        self.polkit_layout = Some((key, layout.clone()));
        Some(layout)
    }

    fn keychain_layout(&mut self, painter: &Painter) -> Option<keychain_view::Layout> {
        let view = self.keychain.as_ref()?;
        let key = (self.keychain_rev, crate::i18n::language());
        if let Some((cached, layout)) = self.keychain_layout.as_ref()
            && *cached == key
        {
            return Some(layout.clone());
        }
        let layout = keychain_view::layout(painter, view);
        self.keychain_layout = Some((key, layout.clone()));
        Some(layout)
    }

    fn bars_live(&self) -> bool {
        self.bars_drawn.is_some_and(|at| at.elapsed() < BARS_LIVE)
    }

    fn size(&self) -> Size<i32, Logical> {
        let t = self.morph.value().clamp(0.0, 1.0);
        Size::from((lerp_i32(self.from.w, self.to.w, t), lerp_i32(self.from.h, self.to.h, t)))
    }

    pub(super) fn rect(&self, strip: Rectangle<i32, Logical>) -> LocalRect {
        placed(strip, self.size())
    }

    pub(super) fn target(&self, strip: Rectangle<i32, Logical>) -> LocalRect {
        placed(strip, self.to)
    }

    fn aim(&mut self, target: Size<i32, Logical>) {
        if self.to == target {
            return;
        }
        if self.to.w <= 0 || self.to.h <= 0 {
            self.from = target;
            self.to = target;
            self.morph.jump(1.0);
            return;
        }
        self.from = self.size();
        self.to = target;
        self.morph.jump(0.0);
        self.morph.retarget(1.0, motion::t3(), E_OUT);
    }

    fn show(&mut self, view: View) -> bool {
        if self.view == view {
            return false;
        }
        self.previous = self.view;
        self.view = view;
        self.fade.jump(0.0);
        self.fade.retarget(1.0, motion::t2(), E_IO);
        self.scrim.retarget(if view.dims() { 1.0 } else { 0.0 }, motion::t3(), E_IO);
        true
    }

    fn content_alpha(&self) -> f32 {
        self.fade.value().clamp(0.0, 1.0) as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) struct ClockWidths {
    date: i32,
    time: i32,
    badge: i32,
}

pub(super) const fn clock_width(widths: ClockWidths) -> i32 {
    let date = if widths.date > 0 { widths.date + SUB_GAP } else { 0 };
    let badge = if widths.badge > 0 { SUB_GAP + widths.badge } else { 0 };
    SUB_PAD * 2 + date + widths.time + badge
}

fn badge_label(count: usize) -> String {
    format!("+{count}")
}

fn badge_width(painter: &Painter, count: usize) -> i32 {
    text_w(painter, &badge_label(count), &BADGE_STYLE) + BADGE_PAD * 2
}

#[must_use]
pub(super) fn compact_width(widths: ClockWidths, media: bool) -> i32 {
    let bare = COMPACT_PAD * 2 + clock_width(widths);
    if media { COMPACT_W.max(bare + SEP_GAP * 2 + 1 + MEDIA_MIN) } else { bare }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompactLayout {
    media: Option<LocalRect>,
    separator: Option<LocalRect>,
    clock: LocalRect,
}

fn compact_layout(width: i32, widths: ClockWidths, media: bool) -> CompactLayout {
    let y = (COMPACT_H - SUB_H) / 2;
    let clock_w = clock_width(widths);
    let clock = local_rect(width - COMPACT_PAD - clock_w, y, clock_w, SUB_H);
    if !media {
        return CompactLayout { media: None, separator: None, clock };
    }
    let sep_x = clock.loc.x - SEP_GAP - 1;
    let separator = local_rect(sep_x, (COMPACT_H - SEP_H) / 2, 1, SEP_H);
    let media_w = (sep_x - SEP_GAP - COMPACT_PAD).max(0);
    CompactLayout { media: Some(local_rect(COMPACT_PAD, y, media_w, SUB_H)), separator: Some(separator), clock }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlertLayout {
    icon: LocalRect,
    text: (i32, i32),
    close: LocalRect,
}

fn alert_layout() -> AlertLayout {
    let icon = local_rect(ALERT_PAD, (ALERT_H - ALERT_ICON) / 2, ALERT_ICON, ALERT_ICON);
    let close = local_rect(ALERT_W - ALERT_PAD - ALERT_CLOSE, ALERT_PAD, ALERT_CLOSE, ALERT_CLOSE);
    AlertLayout { icon, text: (ALERT_PAD + ALERT_ICON + SP4, ALERT_W - ALERT_PAD - ALERT_CLOSE - SP3), close }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NoticeLayout {
    height: i32,
    icon: LocalRect,
    close: LocalRect,
    text: (i32, i32),
    caption_y: i32,
    badge: Option<LocalRect>,
    title: Vec<String>,
    body: Vec<String>,
    body_y: i32,
    row_y: i32,
    buttons: Vec<LocalRect>,
    field: Option<LocalRect>,
    send: Option<LocalRect>,
}

const fn line_h(style: &TextStyle) -> i32 {
    style.line_height_px as i32
}

fn notice_layout(painter: &Painter, notice: &Notice) -> NoticeLayout {
    let icon = local_rect(ALERT_PAD, ALERT_PAD, ALERT_ICON, ALERT_ICON);
    let close = local_rect(ALERT_W - ALERT_PAD - ALERT_CLOSE, ALERT_PAD, ALERT_CLOSE, ALERT_CLOSE);
    let (left, right) = (ALERT_PAD + ALERT_ICON + SP4, ALERT_W - ALERT_PAD);
    let beside_close = close.loc.x - SP3;
    let caption_y = ALERT_PAD;
    let badge = (notice.waiting > 0).then(|| {
        let x = (left + text_w(painter, &notice.app, &CAPTION_STYLE) + SP2).min(beside_close);
        let y = caption_y + (line_h(&CAPTION_STYLE) - BADGE_H) / 2;
        local_rect(x, y, badge_width(painter, notice.waiting), BADGE_H)
    });
    let title_y = caption_y + line_h(&CAPTION_STYLE);
    let title = crate::notifications::wrap_lines(
        painter,
        &notice.title,
        &ALERT_TITLE_STYLE,
        beside_close - left,
        NOTICE_TITLE_LINES,
    );
    let body_y = title_y + line_h(&ALERT_TITLE_STYLE) * title.len() as i32;
    let body = crate::notifications::wrap_paragraphs(
        painter,
        &notice.body,
        &ALERT_BODY_STYLE,
        right - left,
        NOTICE_BODY_LINES,
    );
    let text_end = body_y + line_h(&ALERT_BODY_STYLE) * body.len() as i32;
    let row_y = text_end + NOTICE_ROW_GAP;
    let (mut buttons, mut field, mut send) = (Vec::new(), None, None);
    let row_h = match &notice.reply {
        Some(NoticeReply::Composing { .. }) => {
            let rect = local_rect(left, row_y, right - left, FIELD_H);
            send = Some(local_rect(right - SP1 - SEND, row_y + (FIELD_H - SEND) / 2, SEND, SEND));
            field = Some(rect);
            FIELD_H
        }
        Some(NoticeReply::Sent) => FIELD_H,
        None if notice.buttons.is_empty() => 0,
        None => {
            let mut x = right;
            for button in &notice.buttons {
                let width = text_w(painter, button.label(), &BUTTON_STYLE) + BUTTON_PAD * 2;
                x -= width;
                buttons.push(local_rect(x, row_y, width, BUTTON_H));
                x -= SP2;
            }
            BUTTON_H
        }
    };
    let content_end = if row_h > 0 { row_y + row_h } else { text_end };
    let height = (content_end + ALERT_PAD).max(ALERT_PAD * 2 + ALERT_ICON);
    NoticeLayout {
        height,
        icon,
        close,
        text: (left, right),
        caption_y,
        badge,
        title,
        body,
        body_y,
        row_y,
        buttons,
        field,
        send,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PillLayout {
    width: i32,
    dot: Option<LocalRect>,
    text: (i32, i32),
    badge: Option<LocalRect>,
}

fn pill_layout(painter: &Painter, pill: Pill) -> PillLayout {
    let mut x = COMPACT_PAD;
    let dot = (pill.dot != PillDot::None).then(|| {
        let rect = local_rect(x, (COMPACT_H - DOT) / 2, DOT, DOT);
        x += DOT + SUB_GAP;
        rect
    });
    let text_w_px = text_w(painter, pill.label(), &TITLE_STYLE);
    let text = (x, x + text_w_px);
    x += text_w_px;
    let badge = (pill.more > 0).then(|| {
        let width = badge_width(painter, pill.more);
        let rect = local_rect(x + SUB_GAP, (COMPACT_H - BADGE_H) / 2, width, BADGE_H);
        x += SUB_GAP + width;
        rect
    });
    PillLayout { width: x + COMPACT_PAD, dot, text, badge }
}

fn mini_width(painter: &Painter, label: &str) -> i32 {
    MINI_PAD * 2 + MINI_ICON + MINI_GAP + text_w(painter, label, &MINI_STYLE)
}

fn header_controls(header_h: i32, switch_w: Option<i32>) -> (LocalRect, Option<LocalRect>) {
    let collapse = local_rect(WIDE - PAD - COLLAPSE, PAD + (header_h - COLLAPSE) / 2, COLLAPSE, COLLAPSE);
    let switch =
        switch_w.map(|w| local_rect(collapse.loc.x - HEADER_GAP - w, PAD + (header_h - MINI_H) / 2, w, MINI_H));
    (collapse, switch)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MediaLayout {
    collapse: LocalRect,
    switch: LocalRect,
    art: LocalRect,
    track: LocalRect,
    seek: LocalRect,
    times_y: i32,
    transport: [LocalRect; 3],
}

fn media_layout(switch_w: i32) -> MediaLayout {
    let (collapse, switch) = header_controls(HEADER_H, Some(switch_w));
    let art_y = PAD + HEADER_H + PAD;
    let art = local_rect(PAD, art_y, MEDIA_ART_LARGE, MEDIA_ART_LARGE);
    let track_y = art_y + MEDIA_ART_LARGE + PAD;
    let track = local_rect(PAD, track_y, WIDE - PAD * 2, TRACK_H);
    let seek = local_rect(PAD, track_y - SP2, WIDE - PAD * 2, TRACK_H + SP2 * 2);
    let times_y = track_y + TRACK_H + SP1;
    let row_y = times_y + TIMES_H + PAD;
    let row_w = TRANSPORT * 2 + TRANSPORT_MAIN + SP2 * 2;
    let mut x = (WIDE - row_w) / 2;
    let transport = [TRANSPORT, TRANSPORT_MAIN, TRANSPORT].map(|side| {
        let rect = local_rect(x, row_y + (TRANSPORT_MAIN - side) / 2, side, side);
        x += side + SP2;
        rect
    });
    MediaLayout { collapse, switch: switch.unwrap_or_default(), art, track, seek, times_y, transport }
}

const fn shown_items(count: usize) -> i32 {
    let shown = if count < VISIBLE_ITEMS { count } else { VISIBLE_ITEMS };
    shown as i32
}

const fn stack_height(count: usize) -> i32 {
    let shown = shown_items(count);
    if shown == 0 { EMPTY_H } else { shown * ITEM_H + (shown - 1) * ITEM_GAP }
}

#[must_use]
pub(super) const fn calendar_height(count: usize) -> i32 {
    STACK_Y + stack_height(count) + PAD
}

#[must_use]
pub(super) fn first_item(first: usize, count: usize) -> usize {
    first.min(count.saturating_sub(VISIBLE_ITEMS))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NcLayout {
    collapse: LocalRect,
    switch: Option<LocalRect>,
    toggle: LocalRect,
    track: LocalRect,
    moon: LocalRect,
    clear: Option<LocalRect>,
    items: Vec<(u32, LocalRect, LocalRect)>,
}

fn nc_layout(painter: &Painter, items: &[NcItem], first: usize, switch_w: Option<i32>) -> NcLayout {
    use crate::i18n::{Key, t};
    let (collapse, switch) = header_controls(NC_HEADER_H, switch_w);
    let track = local_rect(WIDE - PAD - TOGGLE_W, SECTION_Y + (SECTION_H - TOGGLE_H) / 2, TOGGLE_W, TOGGLE_H);
    let moon = local_rect(track.loc.x - MINI_GAP - MOON, SECTION_Y + (SECTION_H - MOON) / 2, MOON, MOON);
    let toggle = local_rect(moon.loc.x, SECTION_Y, WIDE - PAD - moon.loc.x, SECTION_H);
    let clear = (!items.is_empty()).then(|| {
        let w = text_w(painter, t(Key::ClearAll), &LINK_STYLE) + SUB_PAD * 2;
        local_rect(moon.loc.x - SP2 - w, SECTION_Y + (SECTION_H - LINK_H) / 2, w, LINK_H)
    });
    let first = first_item(first, items.len());
    let mut y = STACK_Y;
    let rows = items
        .iter()
        .skip(first)
        .take(VISIBLE_ITEMS)
        .map(|item| {
            let row = local_rect(PAD, y, WIDE - PAD * 2, ITEM_H);
            let dismiss = local_rect(
                row.loc.x + row.size.w - SP2 - ITEM_DISMISS,
                y + (ITEM_H - ITEM_DISMISS) / 2,
                ITEM_DISMISS,
                ITEM_DISMISS,
            );
            y += ITEM_H + ITEM_GAP;
            (item.id, row, dismiss)
        })
        .collect();
    NcLayout { collapse, switch, toggle, track, moon, clear, items: rows }
}

fn shift(rect: LocalRect, origin: Point<i32, Logical>) -> LocalRect {
    Rectangle::new(rect.loc + origin, rect.size)
}

fn clock_and_time(media: &MediaWidget) -> Option<(String, String)> {
    let length = media.length?;
    let elapsed = length.mul_f32(media.progress.unwrap_or(0.0).clamp(0.0, 1.0));
    Some((format_duration(elapsed), format_duration(length)))
}

#[must_use]
pub(super) fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes:02}:{seconds:02}") }
}

impl Topbar {
    fn wanted_island_view(&self) -> View {
        if self.island.polkit.is_some() {
            return if self.island.fullscreen && !self.island.polkit_peek { View::Pill } else { View::Polkit };
        }
        if self.island.keychain.is_some() {
            return if self.island.fullscreen && !self.island.polkit_peek { View::Pill } else { View::Keychain };
        }
        match self.open_popup {
            Some(OpenPopup::MediaPlayer) if self.media.is_some() => View::Media,
            Some(OpenPopup::NotificationCenter) => View::Calendar,
            _ if self.covered() => {
                let peeking = self.island.notice.as_ref().is_some_and(|notice| self.island.peek == Some(notice.id));
                if peeking {
                    View::Notice
                } else if self.pill().is_some() {
                    View::Pill
                } else {
                    View::Compact
                }
            }
            _ if self.island.notice.is_some() => View::Notice,
            _ if self.island.alert.is_some() => View::Alert,
            _ => View::Compact,
        }
    }

    pub(super) fn sync_island(&mut self) {
        let view = self.wanted_island_view();
        if self.island.show(view) {
            self.touch();
        }
    }

    pub(super) const fn island_owns(popup: Option<OpenPopup>) -> bool {
        matches!(popup, Some(OpenPopup::MediaPlayer | OpenPopup::NotificationCenter))
    }

    pub(super) fn island_target(&self, output: Rectangle<i32, Logical>) -> LocalRect {
        self.island.target(self.rect(output))
    }

    pub(super) fn island_content_alpha(&self) -> f32 {
        if self.island.view.expanded() { self.island.content_alpha() } else { 0.0 }
    }

    pub fn show_alert(&mut self, alert: IslandAlert, now: Instant) -> bool {
        if self.popup_reveal.is_open() && Self::island_owns(self.open_popup) {
            return false;
        }
        self.island.alert = Some((alert, now + ALERT_HOLD));
        self.island.alert_rev = self.island.alert_rev.wrapping_add(1);
        self.sync_island();
        self.touch();
        true
    }

    pub(super) const fn covered(&self) -> bool {
        self.island.fullscreen && self.open_popup.is_none()
    }

    #[must_use]
    pub fn floats(&self) -> bool {
        self.covered() && matches!(self.island.view, View::Pill | View::Notice | View::Polkit | View::Keychain)
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) -> bool {
        if self.island.fullscreen == fullscreen {
            return false;
        }
        self.island.fullscreen = fullscreen;
        self.island.peek = None;
        self.island.polkit_peek = false;
        self.sync_island();
        self.touch();
        true
    }

    pub fn set_polkit(&mut self, view: Option<crate::polkit::View>, motion: crate::polkit::Motion) -> bool {
        let moved = self.island.polkit_motion != motion;
        self.island.polkit_motion = motion;
        if self.island.polkit == view {
            return moved;
        }
        if view.is_some() && self.island.polkit.is_none() && self.open_popup.is_some() {
            self.close_popup();
        }
        if view.is_none() && self.island.keychain.is_none() {
            self.island.polkit_peek = false;
        }
        self.island.polkit = view;
        self.island.polkit_rev = self.island.polkit_rev.wrapping_add(1);
        self.sync_island();
        self.touch();
        true
    }

    pub fn set_keychain(&mut self, view: Option<crate::keychain_prompt::View>, motion: crate::polkit::Motion) -> bool {
        let moved = self.island.keychain_motion != motion;
        self.island.keychain_motion = motion;
        if self.island.keychain == view {
            return moved;
        }
        if view.is_some() && self.island.keychain.is_none() && self.open_popup.is_some() {
            self.close_popup();
        }
        if view.is_none() && self.island.polkit.is_none() {
            self.island.polkit_peek = false;
        }
        self.island.keychain = view;
        self.island.keychain_rev = self.island.keychain_rev.wrapping_add(1);
        self.sync_island();
        self.touch();
        true
    }

    #[must_use]
    pub fn polkit_shown(&self) -> bool {
        self.island.polkit.is_some() && self.island.view == View::Polkit
    }

    #[must_use]
    pub fn keychain_shown(&self) -> bool {
        self.island.keychain.is_some() && self.island.view == View::Keychain
    }

    #[must_use]
    pub fn capture_mask(&self, output: Rectangle<i32, Logical>) -> Vec<LocalRect> {
        let mut masks = Vec::new();
        if self.polkit_shown() || self.keychain_shown() {
            let island = self.island_target(output);
            let popover = self
                .island
                .polkit_layout
                .as_ref()
                .filter(|_| self.polkit_shown())
                .and_then(|(_, layout)| layout.popover)
                .map(|rect| shift(rect, island.loc));
            masks.push(island);
            masks.extend(popover);
        }
        masks.extend(self.passwords_mask());
        masks
    }

    pub fn peek_notice(&mut self) -> bool {
        if self.island.polkit.is_some() || self.island.keychain.is_some() {
            self.island.polkit_peek = true;
            self.sync_island();
            self.touch();
            return true;
        }
        let Some(id) = self.island.notice.as_ref().map(|notice| notice.id) else {
            return false;
        };
        self.island.peek = Some(id);
        self.sync_island();
        self.touch();
        true
    }

    pub(super) fn pill(&self) -> Option<Pill> {
        if self.island.polkit.is_some() || self.island.keychain.is_some() {
            return Some(Pill { asks: true, dot: PillDot::Accent, more: 0 });
        }
        if let Some(notice) = self.island.notice.as_ref() {
            let dot = if notice.asks {
                PillDot::Accent
            } else if notice.urgent {
                PillDot::Danger
            } else {
                PillDot::None
            };
            return Some(Pill { asks: notice.asks, dot, more: notice.waiting });
        }
        self.island.alert.as_ref()?;
        Some(Pill { asks: false, dot: PillDot::None, more: self.badge_shown().saturating_sub(1) })
    }

    fn pill_action(&self) -> Option<TopbarAction> {
        use crate::notifications::NotificationAction as Act;
        if self.island.polkit.is_some() || self.island.keychain.is_some() {
            return Some(TopbarAction::PeekNotice);
        }
        if let Some(notice) = self.island.notice.as_ref() {
            return Some(if notice.wants_attention() {
                TopbarAction::PeekNotice
            } else {
                TopbarAction::Notification(Act::Activate(notice.id))
            });
        }
        let pill = self.pill()?;
        let (alert, _) = self.island.alert.as_ref()?;
        Some(if pill.more > 0 {
            TopbarAction::ToggleNotificationCenter
        } else {
            TopbarAction::ActivateNotification(alert.id)
        })
    }

    pub fn set_notice(&mut self, notice: Option<Notice>) -> bool {
        if self.island.notice == notice {
            return false;
        }
        if notice.as_ref().map(|notice| notice.id) != self.island.peek {
            self.island.peek = None;
        }
        self.island.notice = notice;
        self.island.notice_rev = self.island.notice_rev.wrapping_add(1);
        self.sync_island();
        self.touch();
        true
    }

    #[must_use]
    pub fn notice(&self) -> Option<&Notice> {
        self.island.notice.as_ref()
    }

    #[must_use]
    pub fn held_notice(&self) -> Option<u32> {
        let notice = self.island.notice.as_ref()?;
        let over = self.hovered?;
        over.on_notice().then_some(notice.id)
    }

    pub(super) fn notice_action(&self, slot: Slot) -> Option<TopbarAction> {
        use crate::notifications::NotificationAction as Act;
        if slot == Slot::Pill {
            return self.pill_action();
        }
        let notice = self.island.notice.as_ref()?;
        let id = notice.id;
        let action = match slot {
            Slot::Notice => Act::Activate(id),
            Slot::NoticeClose => Act::Dismiss(id),
            Slot::NoticeField => Act::OpenReply(id),
            Slot::NoticeSend => Act::SendReply(id),
            Slot::NoticeButton(index) => notice.buttons.get(usize::from(index))?.action(id),
            _ => return None,
        };
        Some(TopbarAction::Notification(action))
    }

    pub(super) fn island_forget_alert(&mut self) {
        self.island.alert = None;
    }

    pub fn dismiss_alert(&mut self) -> bool {
        if self.island.alert.take().is_none() {
            return false;
        }
        self.sync_island();
        self.touch();
        true
    }

    pub fn tick_island(&mut self, now: Instant) -> bool {
        let Some((_, until)) = self.island.alert.as_ref() else {
            return false;
        };
        let hovered = matches!(self.hovered, Some(Slot::IslandAlert | Slot::IslandAlertClose | Slot::Pill));
        if now < *until || hovered {
            return false;
        }
        self.dismiss_alert()
    }

    #[must_use]
    pub fn alert(&self) -> Option<&IslandAlert> {
        self.island.alert.as_ref().map(|(alert, _)| alert)
    }

    pub fn scroll_notifications(&mut self, steps: i32) -> bool {
        if self.open_popup != Some(OpenPopup::NotificationCenter) || steps == 0 {
            return false;
        }
        let current = first_item(self.island.first_item, self.nc_items.len());
        let next = if steps > 0 {
            current.saturating_add(steps.unsigned_abs() as usize)
        } else {
            current.saturating_sub(steps.unsigned_abs() as usize)
        };
        let next = first_item(next, self.nc_items.len());
        if next == current {
            return false;
        }
        self.island.first_item = next;
        self.touch();
        true
    }

    pub(super) fn reset_island_scroll(&mut self) {
        self.island.first_item = 0;
    }

    pub(super) fn island_clock(&mut self, painter: &Painter) -> ClockWidths {
        let (date_w, time_w) = self.clock_metrics(painter);
        let time = time_w.max(numeric_label_width(painter, &self.clock, &TIME_STYLE));
        let date = if self.date.is_empty() { 0 } else { date_w.max(text_w(painter, &self.date, &DATE_STYLE)) };
        let count = self.badge_shown();
        let badge = if count > 0 { badge_width(painter, count) } else { 0 };
        ClockWidths { date, time, badge }
    }

    fn switch_width(&self, painter: &Painter, view: View) -> Option<i32> {
        use crate::i18n::{Key, t};
        match view {
            View::Media => Some(mini_width(painter, t(Key::Notifications))),
            View::Calendar => self.media.is_some().then(|| mini_width(painter, t(Key::Media))),
            View::Compact | View::Alert | View::Notice | View::Pill | View::Polkit | View::Keychain => None,
        }
    }

    fn island_size(&mut self, painter: &Painter, view: View) -> (i32, i32) {
        match view {
            View::Compact => {
                let widths = self.island_clock(painter);
                let width = compact_width(widths, self.media.is_some());
                self.island.compact_w = width;
                (width, COMPACT_H)
            }
            View::Alert => (ALERT_W, ALERT_H),
            View::Notice => {
                let height =
                    self.island.notice.as_ref().map_or(ALERT_H, |notice| notice_layout(painter, notice).height);
                (ALERT_W, height)
            }
            View::Pill => (self.pill().map_or(COMPACT_W, |pill| pill_layout(painter, pill).width), COMPACT_H),
            View::Media => (WIDE, MEDIA_H),
            View::Calendar => (WIDE, calendar_height(self.nc_items.len())),
            View::Polkit => {
                (polkit_view::WIDTH, self.island.polkit_layout(painter).map_or(ALERT_H, |layout| layout.height))
            }
            View::Keychain => {
                (keychain_view::WIDTH, self.island.keychain_layout(painter).map_or(ALERT_H, |layout| layout.height))
            }
        }
    }

    pub(super) fn layout_island(
        &mut self, painter: &Painter, strip: Rectangle<i32, Logical>, slots: &mut Vec<(Slot, LocalRect)>, passive: bool,
    ) -> LocalRect {
        let centre = strip.loc.x + strip.size.w / 2;
        let top = strip.loc.y + TOP;
        let view = if passive { View::Compact } else { self.island.view };
        let compact = self.island_size(painter, View::Compact);
        let (w, h) = if view == View::Compact { compact } else { self.island_size(painter, view) };
        if !passive {
            self.island.aim(Size::from((w, h)));
        }
        let target = placed(strip, Size::from((w, h)));
        let origin = target.loc;
        match view {
            View::Compact => {
                let widths = self.island_clock(painter);
                let layout = compact_layout(w, widths, self.media.is_some());
                if let Some(media) = layout.media {
                    slots.push((Slot::IslandMedia, shift(media, origin)));
                }
                slots.push((Slot::Clock, shift(layout.clock, origin)));
            }
            View::Alert => {
                let layout = alert_layout();
                slots.push((Slot::IslandAlertClose, shift(layout.close, origin)));
                slots.push((Slot::IslandAlert, target));
            }
            View::Notice => {
                if let Some(notice) = self.island.notice.as_ref() {
                    let layout = notice_layout(painter, notice);
                    slots.push((Slot::NoticeClose, shift(layout.close, origin)));
                    for (index, rect) in layout.buttons.iter().enumerate() {
                        if let Ok(index) = u8::try_from(index) {
                            slots.push((Slot::NoticeButton(index), shift(*rect, origin)));
                        }
                    }
                    if let Some(send) = layout.send {
                        slots.push((Slot::NoticeSend, shift(send, origin)));
                    }
                    if let Some(field) = layout.field {
                        slots.push((Slot::NoticeField, shift(field, origin)));
                    }
                    slots.push((Slot::Notice, target));
                }
            }
            View::Pill => slots.push((Slot::Pill, target)),
            View::Polkit => {
                if let (Some(layout), Some(view)) = (self.island.polkit_layout(painter), self.island.polkit.as_ref()) {
                    for (control, rect) in polkit_view::slots(&layout, view) {
                        slots.push((Slot::Polkit(control), shift(rect, origin)));
                    }
                }
            }
            View::Keychain => {
                if let (Some(layout), Some(view)) =
                    (self.island.keychain_layout(painter), self.island.keychain.as_ref())
                {
                    for (control, rect) in keychain_view::slots(&layout, view) {
                        slots.push((Slot::Keychain(control), shift(rect, origin)));
                    }
                }
            }
            View::Media | View::Calendar => {}
        }
        local_rect(centre - WIDE / 2, top, WIDE, COMPACT_H)
    }

    pub(super) fn layout_island_popup(
        &self, painter: &Painter, popup: OpenPopup, strip: Rectangle<i32, Logical>, slots: &mut Vec<(Slot, LocalRect)>,
    ) -> Option<LocalRect> {
        let centre = strip.loc.x + strip.size.w / 2;
        let origin = Point::from((centre - WIDE / 2, strip.loc.y + TOP));
        match popup {
            OpenPopup::MediaPlayer => {
                self.media.as_ref()?;
                let layout = media_layout(self.switch_width(painter, View::Media).unwrap_or(0));
                slots.push((Slot::IslandCollapse, shift(layout.collapse, origin)));
                slots.push((Slot::IslandToNotifications, shift(layout.switch, origin)));
                slots.push((Slot::PopMediaSeek, shift(layout.seek, origin)));
                let buttons = [Slot::PopMediaPrev, Slot::PopMediaPlay, Slot::PopMediaNext];
                for (slot, rect) in buttons.into_iter().zip(layout.transport) {
                    slots.push((slot, shift(rect, origin)));
                }
                Some(Rectangle::new(origin, Size::from((WIDE, MEDIA_H))))
            }
            OpenPopup::NotificationCenter => {
                let switch = self.switch_width(painter, View::Calendar);
                let layout = nc_layout(painter, &self.nc_items, self.island.first_item, switch);
                slots.push((Slot::IslandCollapse, shift(layout.collapse, origin)));
                if let Some(switch) = layout.switch {
                    slots.push((Slot::IslandToMedia, shift(switch, origin)));
                }
                let block = Point::from((origin.x + PAD, origin.y + CAL_TOP));
                calendar::layout_slots(painter, &self.calendar_month_title(), block, slots);
                if let Some(clear) = layout.clear {
                    slots.push((Slot::NcClearAll, shift(clear, origin)));
                }
                slots.push((Slot::NcDndToggle, shift(layout.toggle, origin)));
                for (id, row, dismiss) in layout.items {
                    slots.push((Slot::NcItemDismiss(id), shift(dismiss, origin)));
                    slots.push((Slot::NcItem(id), shift(row, origin)));
                }
                Some(Rectangle::new(origin, Size::from((WIDE, calendar_height(self.nc_items.len())))))
            }
            _ => None,
        }
    }
}

struct Ink<'a> {
    painter: &'a Painter,
    theme: &'a TopbarTheme,
}

fn draw_mini(c: &mut Canvas<'_>, ink: &Ink<'_>, rect: LocalRect, icon: Glyph, label: &str) {
    c.fill(rect, ink.theme.track_soft, R1);
    let icon_rect = local_rect(rect.loc.x + MINI_PAD, rect.loc.y, MINI_ICON, rect.size.h);
    glyph(c, icon, MINI_ICON, icon_rect, ink.theme.muted);
    let left = icon_rect.loc.x + MINI_ICON + MINI_GAP;
    let band = (left, rect.loc.x + rect.size.w - MINI_PAD, rect.loc.y, rect.loc.y + rect.size.h);
    c.text(ink.painter, band, TextAlign::Left, label, ink.theme.muted, &MINI_STYLE);
}

fn draw_collapse(c: &mut Canvas<'_>, ink: &Ink<'_>, rect: LocalRect) {
    c.fill(rect, ink.theme.track_soft, R1);
    glyph(c, Glyph::Collapse, MINI_ICON, rect, ink.theme.muted);
}

fn bars_clock(elapsed: Duration, motion_scale: f32) -> Option<Duration> {
    (motion_scale > 0.0).then(|| elapsed.div_f32(motion_scale))
}

fn bar_heights(playing: bool, clock: Option<Duration>) -> [i32; 3] {
    if !playing {
        return [BAR_PAUSED; 3];
    }
    let Some(clock) = clock else {
        return BARS_STILL;
    };
    BAR_SWINGS.map(|(from, to, period)| {
        let sweep = (clock.as_secs_f64() / period.as_secs_f64()) % 2.0;
        let t = if sweep > 1.0 { 2.0 - sweep } else { sweep };
        motion::lerp(f64::from(from), f64::from(to), E_IO.ease_f64(t)).round() as i32
    })
}

fn bar_colour(media: &MediaWidget) -> Color {
    if media.playing { media.accent } else { media.accent.with_alpha(0.5) }
}

const fn bars_x(media: LocalRect) -> i32 {
    media.loc.x + SUB_PAD + MEDIA_ART + SUB_GAP
}

fn draw_bar(height: i32, ink: Color, scale: i32) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(BAR_W, height, scale)?;
    c.fill(c.full(), ink, 1.0);
    Some(c.finish())
}

fn push_bars<R: ShellRenderer>(
    renderer: &mut R, island: &mut Island, media: &MediaWidget, (x, content, clip): (i32, LocalRect, LocalRect),
    (alpha, scale, output_scale): (f32, Scale<f64>, i32), elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) {
    let now = glide_ui::clock::now();
    let clock = bars_clock(now.saturating_duration_since(island.epoch), motion::scale());
    island.bars_drawn = (media.playing && clock.is_some()).then_some(now);
    let ink = bar_colour(media);
    if island.bar_ink != Some(ink) {
        island.bar_ink = Some(ink);
        island.bars.clear();
    }
    let bottom = i32::midpoint(COMPACT_H, BARS_H);
    let mut left = content.loc.x + x;
    for height in bar_heights(media.playing, clock) {
        let rect = local_rect(left, content.loc.y + bottom - height, BAR_W, height);
        left += BAR_W + 1;
        let key = (height, output_scale);
        if !island.bars.contains_key(&key)
            && let Some(buffer) = draw_bar(height, ink, output_scale)
        {
            island.bars.insert(key, buffer);
        }
        if let Some(buffer) = island.bars.get(&key) {
            push_clipped(renderer, buffer, (rect, clip), alpha, scale, elements);
        }
    }
}

fn draw_compact_media(c: &mut Canvas<'_>, ink: &Ink<'_>, media: &MediaWidget, rect: LocalRect) {
    let art = local_rect(rect.loc.x + SUB_PAD, (COMPACT_H - MEDIA_ART) / 2, MEDIA_ART, MEDIA_ART);
    draw_media_art(c, ink.painter, media, (media.art.as_deref(), (art.loc.x, art.loc.y, MEDIA_ART)), R1);
    c.stroke(art, 1.0, R1, Color([1.0, 1.0, 1.0, INSET_HIGHLIGHT]));
    let left = bars_x(rect) + BARS_W + SUB_GAP;
    let right = (rect.loc.x + rect.size.w - SUB_PAD).min(left + TITLE_MAX);
    if right > left {
        c.label(ink.painter, (left, right), TextAlign::Left, &media.title, ink.theme.label, &TITLE_STYLE);
    }
}

struct ClockText<'a> {
    date: &'a str,
    clock: &'a str,
    badge: BadgePop,
    widths: ClockWidths,
}

fn draw_compact_clock(c: &mut Canvas<'_>, ink: &Ink<'_>, text: &ClockText<'_>, rect: LocalRect) {
    let mut x = rect.loc.x + SUB_PAD;
    if text.widths.date > 0 {
        let span = (x, x + text.widths.date);
        c.label(ink.painter, span, TextAlign::Left, text.date, ink.theme.label, &DATE_STYLE);
        x += text.widths.date + SUB_GAP;
    }
    c.label(ink.painter, (x, x + text.widths.time), TextAlign::Left, text.clock, ink.theme.label, &TIME_STYLE);
    x += text.widths.time + SUB_GAP;
    if text.badge.visible() && text.widths.badge > 0 {
        let pill = local_rect(x, (COMPACT_H - BADGE_H) / 2, text.widths.badge, BADGE_H);
        let alpha = text.badge.pop.clamp(0.0, 1.0) as f32;
        c.fill(scaled_about_centre(pill, text.badge.scale()), ink.theme.active_background.with_alpha(alpha), R_ROUND);
        let label = badge_label(text.badge.count);
        let band = (pill.loc.x, pill.loc.x + pill.size.w, pill.loc.y, pill.loc.y + pill.size.h);
        c.text(ink.painter, band, TextAlign::Center, &label, ink.theme.active_label.with_alpha(alpha), &BADGE_STYLE);
    }
}

fn draw_compact(
    ink: &Ink<'_>, media: Option<&MediaWidget>, text: &ClockText<'_>, width: i32, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(width, COMPACT_H, scale)?.with_icons(icons);
    let layout = compact_layout(width, text.widths, media.is_some());
    if let (Some(media), Some(rect)) = (media, layout.media) {
        draw_compact_media(&mut c, ink, media, rect);
    }
    if let Some(separator) = layout.separator {
        c.fill(separator, ink.theme.line.with_alpha(ink.theme.line.0[3] * 0.8), 0.0);
    }
    draw_compact_clock(&mut c, ink, text, layout.clock);
    Some(c.finish())
}

fn draw_alert(
    ink: &Ink<'_>, alert: &IslandAlert, scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(ALERT_W, ALERT_H, scale)?.with_icons(icons);
    paint_alert(&mut c, ink, alert);
    Some(c.finish())
}

fn paint_alert(c: &mut Canvas<'_>, ink: &Ink<'_>, alert: &IslandAlert) {
    let layout = alert_layout();
    if let Some(icon) = alert.icon.as_deref() {
        let source = crate::notifications::ICON_SOURCE_PX;
        let (x, y) = (layout.icon.loc.x, layout.icon.loc.y);
        crate::paint::blit_icon_over(&mut c.buf, c.w, c.h, icon, source, ALERT_ICON, x, y, c.scale);
    } else {
        c.fill(layout.icon, ink.theme.active_background, R2);
        glyph(c, Glyph::Bell, 26, layout.icon, ink.theme.active_label);
    }
    let (left, right) = layout.text;
    let lines = alert_lines(ink.painter, alert, right - left);
    let title_h = ALERT_TITLE_STYLE.line_height_px.ceil() as i32;
    let body_h = ALERT_BODY_STYLE.line_height_px.ceil() as i32;
    let mut top = (ALERT_H - title_h - body_h * lines.len() as i32) / 2;
    let heading_right = right.min(layout.close.loc.x - SP3);
    c.text(
        ink.painter,
        (left, heading_right, top, top + title_h),
        TextAlign::Left,
        &alert.heading,
        ink.theme.label,
        &ALERT_TITLE_STYLE,
    );
    top += title_h;
    for line in &lines {
        c.text(
            ink.painter,
            (left, right, top, top + body_h),
            TextAlign::Left,
            line,
            ink.theme.muted,
            &ALERT_BODY_STYLE,
        );
        top += body_h;
    }
    c.fill(layout.close, ink.theme.fill_2, R1);
    glyph(c, Glyph::Close, 12, layout.close, ink.theme.muted);
}

fn draw_pill(ink: &Ink<'_>, pill: Pill, scale: i32) -> Option<MemoryRenderBuffer> {
    let layout = pill_layout(ink.painter, pill);
    let mut c = Canvas::new(layout.width, COMPACT_H, scale)?;
    let theme = ink.theme;
    if let Some(dot) = layout.dot {
        let colour = if pill.dot == PillDot::Danger { theme.danger } else { theme.active_background };
        c.fill(dot, colour, R_ROUND);
    }
    c.label(ink.painter, layout.text, TextAlign::Left, pill.label(), theme.label, &TITLE_STYLE);
    if let Some(badge) = layout.badge {
        c.fill(badge, theme.active_background, R_ROUND);
        let band = (badge.loc.x, badge.loc.x + badge.size.w, badge.loc.y, badge.loc.y + badge.size.h);
        c.text(ink.painter, band, TextAlign::Center, &badge_label(pill.more), theme.active_label, &BADGE_STYLE);
    }
    Some(c.finish())
}

fn draw_notice(
    ink: &Ink<'_>, notice: &Notice, scale: i32, icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let layout = notice_layout(ink.painter, notice);
    let mut c = Canvas::new(ALERT_W, layout.height, scale)?.with_icons(icons);
    paint_notice(&mut c, ink, notice, &layout);
    Some(c.finish())
}

fn paint_notice(c: &mut Canvas<'_>, ink: &Ink<'_>, notice: &Notice, layout: &NoticeLayout) {
    let theme = ink.theme;
    if let Some(icon) = notice.icon.as_deref() {
        let source = crate::notifications::ICON_SOURCE_PX;
        let (x, y) = (layout.icon.loc.x, layout.icon.loc.y);
        crate::paint::blit_icon_over(&mut c.buf, c.w, c.h, icon, source, ALERT_ICON, x, y, c.scale);
    } else {
        c.fill(layout.icon, theme.active_soft, R2);
        glyph(c, Glyph::Bell, 26, layout.icon, theme.active_soft_label);
    }
    let (left, right) = layout.text;
    let beside_close = layout.close.loc.x - SP3;
    let caption_h = line_h(&CAPTION_STYLE);
    let mut caption_left = left;
    if notice.urgent {
        let dot = local_rect(left, layout.caption_y + (caption_h - DOT) / 2, DOT, DOT);
        c.fill(dot, theme.danger, R_ROUND);
        caption_left += DOT + SP2;
    }
    let caption_right = layout.badge.map_or(beside_close, |badge| badge.loc.x - SP1);
    let band = (caption_left, caption_right, layout.caption_y, layout.caption_y + caption_h);
    c.text(ink.painter, band, TextAlign::Left, &notice.app, theme.faint, &CAPTION_STYLE);
    if let Some(badge) = layout.badge {
        c.fill(badge, theme.track_soft, R_ROUND);
        let band = (badge.loc.x, badge.loc.x + badge.size.w, badge.loc.y, badge.loc.y + badge.size.h);
        c.text(ink.painter, band, TextAlign::Center, &badge_label(notice.waiting), theme.muted, &BADGE_STYLE);
    }
    let mut y = layout.caption_y + caption_h;
    let title_h = line_h(&ALERT_TITLE_STYLE);
    for line in &layout.title {
        c.text(
            ink.painter,
            (left, beside_close, y, y + title_h),
            TextAlign::Left,
            line,
            theme.label,
            &ALERT_TITLE_STYLE,
        );
        y += title_h;
    }
    let body_h = line_h(&ALERT_BODY_STYLE);
    let mut y = layout.body_y;
    for line in &layout.body {
        c.text(ink.painter, (left, right, y, y + body_h), TextAlign::Left, line, theme.muted, &ALERT_BODY_STYLE);
        y += body_h;
    }
    c.fill(layout.close, theme.fill_2, R1);
    glyph(c, Glyph::Close, 12, layout.close, theme.muted);
    for (index, (button, rect)) in notice.buttons.iter().zip(&layout.buttons).enumerate() {
        let (fill, label) =
            if index == 0 { (theme.active_background, theme.active_label) } else { (theme.track_soft, theme.label) };
        c.fill(*rect, fill, R2);
        let band = (rect.loc.x, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + rect.size.h);
        c.text(ink.painter, band, TextAlign::Center, button.label(), label, &BUTTON_STYLE);
    }
    match &notice.reply {
        Some(NoticeReply::Composing { shown, placeholder, invalid }) => {
            paint_reply_field(c, ink, layout, (shown, placeholder, *invalid));
        }
        Some(NoticeReply::Sent) => {
            use crate::i18n::{Key, t};
            let band = (left, right, layout.row_y, layout.row_y + FIELD_H);
            c.text(ink.painter, band, TextAlign::Left, t(Key::Sent), theme.muted, &FIELD_STYLE);
        }
        None => {}
    }
}

fn paint_reply_field(
    c: &mut Canvas<'_>, ink: &Ink<'_>, layout: &NoticeLayout, (shown, placeholder, invalid): (&str, &str, bool),
) {
    let (Some(field), Some(send)) = (layout.field, layout.send) else {
        return;
    };
    let theme = ink.theme;
    c.fill(field, theme.card, R2);
    c.stroke(field, 1.0, R2, if invalid { theme.danger } else { theme.active_background });
    let text_left = field.loc.x + FIELD_PAD;
    let text_right = send.loc.x - SP2;
    let band = (text_left, text_right, field.loc.y, field.loc.y + FIELD_H);
    if shown.is_empty() {
        c.text(ink.painter, band, TextAlign::Left, placeholder, theme.faint, &FIELD_STYLE);
    } else {
        let tail = tail_that_fits(ink.painter, shown, text_right - text_left);
        c.text(ink.painter, band, TextAlign::Left, tail, theme.label, &FIELD_STYLE);
        let caret = text_left + text_w(ink.painter, tail, &FIELD_STYLE) + 1;
        if caret < text_right {
            c.fill(local_rect(caret, field.loc.y + (FIELD_H - CARET_H) / 2, 2, CARET_H), theme.active_background, 1.0);
        }
    }
    c.fill(send, theme.active_background, R1);
    glyph(c, Glyph::Send, 12, send, theme.active_label);
}

fn tail_that_fits<'a>(painter: &Painter, text: &'a str, width: i32) -> &'a str {
    let starts: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    let first = starts.partition_point(|&start| text_w(painter, &text[start..], &FIELD_STYLE) > width);
    starts.get(first).map_or("", |&start| &text[start..])
}

fn alert_lines(painter: &Painter, alert: &IslandAlert, width: i32) -> Vec<String> {
    crate::notifications::wrap_lines(painter, &alert.summary, &ALERT_BODY_STYLE, width, ALERT_LINES)
}

fn draw_media_header(c: &mut Canvas<'_>, ink: &Ink<'_>, media: &MediaWidget, layout: &MediaLayout) {
    use crate::i18n::{Key, t};
    let source = if media.source.trim().is_empty() { "—" } else { media.source.as_str() };
    let pill_w =
        (SP2 + DOT + MINI_GAP + text_w(ink.painter, source, &PILL_STYLE) + SP2).min(layout.switch.loc.x - PAD - SP2);
    let pill = local_rect(PAD, PAD + (HEADER_H - PILL_H) / 2, pill_w, PILL_H);
    c.fill(pill, ink.theme.track_soft, R_ROUND);
    c.fill(local_rect(pill.loc.x + SP2, pill.loc.y + (PILL_H - DOT) / 2, DOT, DOT), media.accent, R_ROUND);
    let text_left = pill.loc.x + SP2 + DOT + MINI_GAP;
    let band = (text_left, pill.loc.x + pill.size.w - SP2, pill.loc.y, pill.loc.y + PILL_H);
    c.text(ink.painter, band, TextAlign::Left, source, ink.theme.muted, &PILL_STYLE);
    draw_mini(c, ink, layout.switch, Glyph::Bell, t(Key::Notifications));
    draw_collapse(c, ink, layout.collapse);
}

fn draw_media_text(c: &mut Canvas<'_>, ink: &Ink<'_>, media: &MediaWidget, art: LocalRect) {
    let left = art.loc.x + art.size.w + PAD;
    let right = WIDE - PAD;
    let lines: Vec<(&str, Color, &TextStyle)> = [
        (media.title.as_str(), ink.theme.label, &TRACK_TITLE_STYLE),
        (media.artist.as_str(), ink.theme.muted, &ARTIST_STYLE),
        (media.album.as_str(), ink.theme.faint, &ALBUM_STYLE),
    ]
    .into_iter()
    .filter(|(text, _, _)| !text.trim().is_empty())
    .collect();
    let gap = 2;
    let total: i32 = lines.iter().map(|(_, _, style)| style.line_height_px.ceil() as i32).sum::<i32>()
        + gap * (lines.len() as i32 - 1).max(0);
    let mut y = art.loc.y + (art.size.h - total) / 2;
    for (text, colour, style) in lines {
        let height = style.line_height_px.ceil() as i32;
        c.text(ink.painter, (left, right, y, y + height), TextAlign::Left, text, colour, style);
        y += height + gap;
    }
}

fn draw_scrubber(c: &mut Canvas<'_>, ink: &Ink<'_>, media: &MediaWidget, layout: &MediaLayout, progress: Option<f32>) {
    let track = layout.track;
    c.fill(track, ink.theme.fill_2, R_ROUND);
    let filled = ((track.size.w as f32) * progress.unwrap_or(0.0).clamp(0.0, 1.0)).round() as i32;
    if filled > 0 {
        c.fill(local_rect(track.loc.x, track.loc.y, filled, TRACK_H), media.accent, R_ROUND);
    }
    if progress.is_some() {
        let thumb_x =
            (track.loc.x + filled - THUMB / 2).clamp(track.loc.x - THUMB / 2, track.loc.x + track.size.w - THUMB / 2);
        let thumb = local_rect(thumb_x, track.loc.y + (TRACK_H - THUMB) / 2, THUMB, THUMB);
        c.fill(thumb, ink.theme.label, R_ROUND);
    }
    if let Some((elapsed, total)) = clock_and_time(media) {
        let band = (track.loc.x, track.loc.x + track.size.w, layout.times_y, layout.times_y + TIMES_H);
        c.text(ink.painter, band, TextAlign::Left, &elapsed, ink.theme.faint, &TIMES_STYLE);
        c.text(ink.painter, band, TextAlign::Right, &total, ink.theme.faint, &TIMES_STYLE);
    }
}

fn draw_transport(c: &mut Canvas<'_>, ink: &Ink<'_>, media: &MediaWidget, layout: &MediaLayout) {
    let white = Color([1.0, 1.0, 1.0, 1.0]);
    for (index, rect) in layout.transport.into_iter().enumerate() {
        if index == 1 {
            c.fill(rect, media.accent, R_ROUND);
            let icon = if media.playing { Glyph::Pause } else { Glyph::Play };
            glyph(c, icon, 14, rect, white);
            continue;
        }
        c.fill(rect, ink.theme.track_soft, R2);
        let colour = if transport_enabled(media, index) { ink.theme.muted } else { ink.theme.ghost };
        glyph(c, if index == 0 { Glyph::Previous } else { Glyph::Next }, 13, rect, colour);
    }
}

fn draw_media(
    ink: &Ink<'_>, media: &MediaWidget, progress: Option<f32>, switch_w: i32, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(WIDE, MEDIA_H, scale)?.with_icons(icons);
    let layout = media_layout(switch_w);
    draw_media_header(&mut c, ink, media, &layout);
    let art = layout.art;
    draw_media_art(&mut c, ink.painter, media, (media.art_large.as_deref(), (art.loc.x, art.loc.y, art.size.w)), R2);
    c.stroke(art, 1.0, R2, Color([1.0, 1.0, 1.0, INSET_HIGHLIGHT * 0.8]));
    draw_media_text(&mut c, ink, media, art);
    draw_scrubber(&mut c, ink, media, &layout, progress);
    draw_transport(&mut c, ink, media, &layout);
    Some(c.finish())
}

struct NcInk<'a> {
    ink: Ink<'a>,
    dnd: bool,
    switch: SwitchMotion,
    hovered: Option<Slot>,
}

fn draw_nc_header(c: &mut Canvas<'_>, ink: &NcInk<'_>, layout: &NcLayout) {
    use crate::i18n::{Key, t};
    let theme = ink.ink.theme;
    let right = layout.switch.map_or(layout.collapse.loc.x, |switch| switch.loc.x) - SP2;
    let band = (PAD, right, PAD, PAD + NC_HEADER_H);
    c.text(ink.ink.painter, band, TextAlign::Left, t(Key::CalendarAndNotifications), theme.label, &VIEW_TITLE_STYLE);
    if let Some(switch) = layout.switch {
        draw_mini(c, &ink.ink, switch, Glyph::Play, t(Key::Media));
    }
    draw_collapse(c, &ink.ink, layout.collapse);
    c.fill(local_rect(PAD, DIVIDER_Y, WIDE - PAD * 2, 1), theme.line, 0.0);
}

fn draw_nc_section(c: &mut Canvas<'_>, ink: &NcInk<'_>, layout: &NcLayout) {
    use crate::i18n::{Key, t};
    let theme = ink.ink.theme;
    let band =
        (PAD, layout.clear.map_or(layout.moon.loc.x, |clear| clear.loc.x) - SP2, SECTION_Y, SECTION_Y + SECTION_H);
    c.text(ink.ink.painter, band, TextAlign::Left, t(Key::Notifications), theme.faint, &TextStyle::MICRO);
    if let Some(clear) = layout.clear {
        let colour = if ink.hovered == Some(Slot::NcClearAll) { theme.label } else { theme.muted };
        c.text_in(ink.ink.painter, clear, TextAlign::Center, t(Key::ClearAll), colour, &LINK_STYLE);
    }
    glyph(c, Glyph::Moon, MOON, layout.moon, if ink.dnd { theme.active_background } else { theme.faint });
    draw_switch(c, theme, layout.track, (1, TOGGLE_H - 2), ink.switch);
}

fn draw_item_icon(c: &mut Canvas<'_>, ink: &Ink<'_>, item: &NcItem, rect: LocalRect) {
    if let Some(icon) = item.icon.as_deref() {
        let source = crate::notifications::ICON_SOURCE_PX;
        crate::paint::blit_icon_over(&mut c.buf, c.w, c.h, icon, source, rect.size.w, rect.loc.x, rect.loc.y, c.scale);
        return;
    }
    c.fill(rect, ink.theme.fill_2, R2);
    let band = (rect.loc.x, rect.loc.x + rect.size.w, rect.loc.y, rect.loc.y + rect.size.h);
    c.text(ink.painter, band, TextAlign::Center, &initial(&item.app_name), ink.theme.label, &ITEM_TITLE_STYLE);
}

fn draw_item(c: &mut Canvas<'_>, ink: &Ink<'_>, item: &NcItem, row: LocalRect, dismiss: LocalRect) {
    let theme = ink.theme;
    c.fill(row, theme.track_soft, R2);
    c.stroke(row, 1.0, R2, theme.line);
    let icon = local_rect(row.loc.x + SP2, row.loc.y + (ITEM_H - ITEM_ICON) / 2, ITEM_ICON, ITEM_ICON);
    draw_item_icon(c, ink, item, icon);
    let left = icon.loc.x + ITEM_ICON + SP2;
    let right = dismiss.loc.x - SP2;
    let title_h = ITEM_TITLE_STYLE.line_height_px.ceil() as i32;
    let body_h = ITEM_BODY_STYLE.line_height_px.ceil() as i32;
    let top = row.loc.y + (ITEM_H - title_h - 1 - body_h) / 2;
    let time_w = text_w(ink.painter, &item.time, &ITEM_TIME_STYLE);
    let time_left = right - time_w;
    c.text(
        ink.painter,
        (time_left, right, top, top + title_h),
        TextAlign::Right,
        &item.time,
        theme.faint,
        &ITEM_TIME_STYLE,
    );
    let mut title_right = time_left - SP1;
    if item.count > 1 {
        let label = format!("+{}", item.count - 1);
        let badge_w = text_w(ink.painter, &label, &COUNT_STYLE) + 10;
        let title_w = text_w(ink.painter, &item.title, &ITEM_TITLE_STYLE);
        let badge_x = (left + title_w + MINI_GAP).min(title_right - badge_w);
        let badge = local_rect(badge_x, top + (title_h - 13) / 2, badge_w, 13);
        c.fill(badge, theme.fill_2, R_ROUND);
        c.text_in(ink.painter, badge, TextAlign::Center, &label, theme.muted, &COUNT_STYLE);
        title_right = badge_x - MINI_GAP;
    }
    c.text(
        ink.painter,
        (left, title_right, top, top + title_h),
        TextAlign::Left,
        &item.title,
        theme.label,
        &ITEM_TITLE_STYLE,
    );
    let body_top = top + title_h + 1;
    c.text(
        ink.painter,
        (left, right, body_top, body_top + body_h),
        TextAlign::Left,
        &item.body,
        theme.muted,
        &ITEM_BODY_STYLE,
    );
    glyph(c, Glyph::Close, 10, dismiss, theme.faint);
}

fn draw_empty(c: &mut Canvas<'_>, ink: &Ink<'_>) {
    let icon = local_rect((WIDE - EMPTY_ICON) / 2, STACK_Y + SP4, EMPTY_ICON, EMPTY_ICON);
    glyph(c, Glyph::Quiet, EMPTY_ICON, icon, ink.theme.faint.with_alpha(ink.theme.faint.0[3] * 0.6));
    let top = icon.loc.y + EMPTY_ICON + SP1;
    let band = (PAD, WIDE - PAD, top, top + EMPTY_STYLE.line_height_px.ceil() as i32);
    let label = crate::i18n::t(crate::i18n::Key::NoNewNotifications);
    c.text(ink.painter, band, TextAlign::Center, label, ink.theme.faint, &EMPTY_STYLE);
}

fn draw_scrollbar(c: &mut Canvas<'_>, theme: &TopbarTheme, first: usize, count: usize) {
    if count <= VISIBLE_ITEMS {
        return;
    }
    let track = stack_height(count);
    let thumb = (track * VISIBLE_ITEMS as i32 / count as i32).max(SP4);
    let offset = (track - thumb) * first as i32 / (count - VISIBLE_ITEMS) as i32;
    let x = WIDE - PAD + (PAD - SCROLL_W) / 2;
    c.fill(local_rect(x, STACK_Y + offset, SCROLL_W, thumb), theme.track, R_ROUND);
}

fn draw_notifications(
    ink: &NcInk<'_>, (items, first): (&[NcItem], usize), switch_w: Option<i32>, scale: i32,
    icons: Option<&mut crate::icon::IconLoader>,
) -> Option<MemoryRenderBuffer> {
    let height = calendar_height(items.len());
    let mut c = Canvas::new(WIDE, height, scale)?.with_icons(icons);
    let layout = nc_layout(ink.ink.painter, items, first, switch_w);
    draw_nc_header(&mut c, ink, &layout);
    draw_nc_section(&mut c, ink, &layout);
    if items.is_empty() {
        draw_empty(&mut c, &ink.ink);
    }
    let first = first_item(first, items.len());
    for (item, (_, row, dismiss)) in items.iter().skip(first).zip(layout.items.iter()) {
        draw_item(&mut c, &ink.ink, item, *row, *dismiss);
    }
    draw_scrollbar(&mut c, ink.ink.theme, first, items.len());
    Some(c.finish())
}

fn push_clipped<R: ShellRenderer>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, (content, clip): (LocalRect, LocalRect), alpha: f32,
    scale: Scale<f64>, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) {
    if alpha <= 0.0 {
        return;
    }
    let Some(visible) = content.intersection(clip) else {
        return;
    };
    if visible.size.w <= 0 || visible.size.h <= 0 {
        return;
    }
    let offset = Point::from((f64::from(visible.loc.x - content.loc.x), f64::from(visible.loc.y - content.loc.y)));
    let src = Rectangle::new(offset, visible.size.to_f64());
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        visible.loc.to_f64().to_physical(scale),
        buffer,
        Some(alpha.clamp(0.0, 1.0)),
        Some(src),
        Some(visible.size),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn push_stretched<R: ShellRenderer>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, (source, target): (Size<i32, Logical>, LocalRect), alpha: f32,
    scale: Scale<f64>, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) {
    if alpha <= 0.0 || target.size.w <= 0 || target.size.h <= 0 {
        return;
    }
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        target.loc.to_f64().to_physical(scale),
        buffer,
        Some(alpha.clamp(0.0, 1.0)),
        Some(Rectangle::from_size(source.to_f64())),
        Some(target.size),
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

#[derive(Debug, Clone, Copy)]
struct Spot {
    strip: Rectangle<i32, Logical>,
    rise: i32,
    clip: LocalRect,
}

impl Spot {
    fn content(self, width: i32, height: i32) -> LocalRect {
        let anchor = placed(self.strip, Size::from((width, height)));
        Rectangle::new(Point::from((anchor.loc.x, anchor.loc.y + self.rise)), anchor.size)
    }
}

fn push_view<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, view: View, alpha: f32, spot: Spot) {
    if alpha <= 0.0 {
        return;
    }
    match view {
        View::Compact => push_compact(topbar, frame, alpha, spot),
        View::Alert => push_alert(topbar, frame, alpha, spot),
        View::Notice => push_notice(topbar, frame, alpha, spot),
        View::Pill => push_pill(topbar, frame, alpha, spot),
        View::Media => push_media(topbar, frame, alpha, spot),
        View::Calendar => push_calendar(topbar, frame, alpha, spot),
        View::Polkit => push_polkit(topbar, frame, alpha, spot),
        View::Keychain => push_keychain(topbar, frame, alpha, spot),
    }
}

fn push_keychain<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    let Some(layout) = topbar.island.keychain_layout(painter) else {
        return;
    };
    let Some(view) = topbar.island.keychain.as_ref() else {
        return;
    };
    let now = glide_ui::clock::now();
    let motion = topbar.island.keychain_motion;
    let clip = spot.clip;
    let mut content = spot.content(keychain_view::WIDTH, layout.height);
    content.loc.x += shake_offset(motion.shaken, now);
    let theme = &topbar.theme;

    if view.checking {
        let rect = keychain_view::spinner_rect(&layout);
        let turn = cycle(motion.checking, now, SPINNER_PERIOD);
        let step = (turn * f64::from(polkit_view::SPINNER_STEPS)) as u32 % polkit_view::SPINNER_STEPS;
        let key = (step, output_scale);
        if !topbar.island.polkit_spinner.contains_key(&key)
            && let Some(buffer) = polkit_view::paint_spinner(step, theme.active_label, output_scale)
        {
            topbar.island.polkit_spinner.insert(key, buffer);
        }
        if let Some(buffer) = topbar.island.polkit_spinner.get(&key) {
            push_clipped(renderer, buffer, (shift(rect, content.loc), clip), alpha, scale, elements);
        }
    }

    let key = (topbar.island.keychain_rev, crate::i18n::language());
    let Some(buffer) = topbar
        .island
        .keychain_cache
        .resolve(key, output_scale, || keychain_view::paint(painter, theme, view, &layout, output_scale))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn shake_offset(shaken: Option<Instant>, now: Instant) -> i32 {
    let duration = motion::t3();
    let Some(at) = shaken else {
        return 0;
    };
    let elapsed = now.saturating_duration_since(at);
    if duration.is_zero() || elapsed >= duration {
        return 0;
    }
    let progress = elapsed.as_secs_f64() / duration.as_secs_f64();
    (-7.0 * (progress * std::f64::consts::TAU * 2.0).sin() * (1.0 - E_OUT.ease_f64(progress))).round() as i32
}

fn cycle(since: Option<Instant>, now: Instant, period: Duration) -> f64 {
    let scale = motion::scale();
    let Some(since) = since.filter(|_| scale > 0.0) else {
        return 0.0;
    };
    let period = period.as_secs_f64() * f64::from(scale);
    (now.saturating_duration_since(since).as_secs_f64() / period).fract()
}

const SPINNER_PERIOD: Duration = Duration::from_millis(800);
const SWEEP_PERIOD: Duration = Duration::from_millis(1400);
const SWEEP_SHARE: f64 = 0.34;

fn push_polkit<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let Some(layout) = topbar.island.polkit_layout(painter) else {
        return;
    };
    let Some(view) = topbar.island.polkit.as_ref() else {
        return;
    };
    let now = glide_ui::clock::now();
    let motion = topbar.island.polkit_motion;
    let clip = spot.clip;
    let mut content = spot.content(polkit_view::WIDTH, layout.height);
    content.loc.x += shake_offset(motion.shaken, now);
    let theme = &topbar.theme;
    let lang = crate::i18n::language();

    if let Some(rect) = layout.popover {
        let at = shift(rect, content.loc);
        if let Some(buffer) = topbar
            .island
            .polkit_popover
            .resolve(lang, output_scale, || polkit_view::paint_popover(painter, theme, output_scale))
        {
            push_buffer_alpha(renderer, buffer, at.loc, alpha, scale, elements);
        }
        let layers = shadow_layers(theme.dark, Level::El3);
        let margin = shadow_margin(theme.dark, Level::El3);
        let size = (rect.size.w, rect.size.h);
        if let Some(buffer) = topbar.island.polkit_popover_shadow.resolve(size, output_scale, || {
            crate::raster::shadow(size, margin, polkit_view::RADIUS, &layers, output_scale).map(|c| c.finish())
        }) {
            let corner = Point::from((at.loc.x - margin, at.loc.y - margin));
            push_buffer_alpha(renderer, buffer, corner, alpha, scale, elements);
        }
    }

    if view.stage == crate::polkit::Stage::Checking {
        if let Some(rect) = polkit_view::spinner_rect(&layout) {
            let turn = cycle(motion.checking, now, SPINNER_PERIOD);
            let step = (turn * f64::from(polkit_view::SPINNER_STEPS)) as u32 % polkit_view::SPINNER_STEPS;
            let key = (step, output_scale);
            if !topbar.island.polkit_spinner.contains_key(&key)
                && let Some(buffer) = polkit_view::paint_spinner(step, theme.active_label, output_scale)
            {
                topbar.island.polkit_spinner.insert(key, buffer);
            }
            if let Some(buffer) = topbar.island.polkit_spinner.get(&key) {
                push_clipped(renderer, buffer, (shift(rect, content.loc), clip), alpha, scale, elements);
            }
        }
        let inset = polkit_view::RADIUS as i32;
        let track = local_rect(
            clip.loc.x + inset,
            clip.loc.y + clip.size.h - polkit_view::SWEEP_H,
            (clip.size.w - inset * 2).max(0),
            polkit_view::SWEEP_H,
        );
        let width = (f64::from(track.size.w) * SWEEP_SHARE).round() as i32;
        let travel = f64::from(track.size.w + width);
        let x = track.loc.x - width + (travel * cycle(motion.checking, now, SWEEP_PERIOD)).round() as i32;
        let bar = local_rect(x, track.loc.y, width, track.size.h).intersection(track);
        let parts = [(bar, theme.focus_line), (Some(track), theme.focus_line.with_alpha(0.15))];
        for (rect, colour) in parts {
            let Some(rect) = rect else {
                continue;
            };
            if !topbar.island.polkit_sweep.iter().any(|(known, _)| *known == colour)
                && let Some(mut c) = Canvas::new(1, 1, 1)
            {
                c.fill(c.full(), colour, 0.0);
                topbar.island.polkit_sweep.push((colour, c.finish()));
            }
            if let Some((_, buffer)) = topbar.island.polkit_sweep.iter().find(|(known, _)| *known == colour) {
                push_stretched(renderer, buffer, (Size::from((1, 1)), rect), alpha, scale, elements);
            }
        }
    }

    let key = (topbar.island.polkit_rev, lang);
    let Some(buffer) = topbar
        .island
        .polkit_cache
        .resolve(key, output_scale, || polkit_view::paint(painter, theme, view, &layout, output_scale, Some(icons)))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn push_buffer_alpha<R: ShellRenderer>(
    renderer: &mut R, buffer: &MemoryRenderBuffer, at: Point<i32, Logical>, alpha: f32, scale: Scale<f64>,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) {
    if alpha <= 0.0 {
        return;
    }
    if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        at.to_f64().to_physical(scale),
        buffer,
        Some(alpha.clamp(0.0, 1.0)),
        None,
        None,
        Kind::Unspecified,
    ) {
        elements.push(element);
    }
}

fn push_compact<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let widths = topbar.island_clock(painter);
    let width = topbar.island.compact_w;
    let clip = spot.clip;
    let badge = BadgePop { count: topbar.badge_shown(), pop: topbar.badge_anim.value() };
    let key: CompactKey =
        (topbar.track_rev, topbar.date.clone(), topbar.clock.clone(), badge.count, badge.key(), width, widths);
    let content = spot.content(width, COMPACT_H);
    let media = topbar.media.as_ref();
    match (media, compact_layout(width, widths, media.is_some()).media) {
        (Some(media), Some(rect)) => {
            let place = (bars_x(rect), content, clip);
            push_bars(renderer, &mut topbar.island, media, place, (alpha, scale, output_scale), elements);
        }
        _ => topbar.island.bars_drawn = None,
    }
    let ink = Ink { painter, theme: &topbar.theme };
    let text = ClockText { date: &topbar.date, clock: &topbar.clock, badge, widths };
    let Some(buffer) = topbar
        .island
        .compact
        .resolve(key, output_scale, || draw_compact(&ink, media, &text, width, output_scale, Some(icons)))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn push_alert<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let Some((alert, _)) = topbar.island.alert.as_ref() else {
        return;
    };
    let clip = spot.clip;
    let content = spot.content(ALERT_W, ALERT_H);
    let ink = Ink { painter, theme: &topbar.theme };
    let Some(buffer) = topbar
        .island
        .alert_cache
        .resolve(topbar.island.alert_rev, output_scale, || draw_alert(&ink, alert, output_scale, Some(icons)))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn push_pill<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, _, scale, output_scale, elements) = frame.parts();
    let Some(pill) = topbar.pill() else {
        return;
    };
    let width = pill_layout(painter, pill).width;
    let clip = spot.clip;
    let content = spot.content(width, COMPACT_H);
    let ink = Ink { painter, theme: &topbar.theme };
    let key: PillKey = (pill, crate::i18n::language());
    let Some(buffer) = topbar.island.pill_cache.resolve(key, output_scale, || draw_pill(&ink, pill, output_scale))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn push_notice<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let Some(notice) = topbar.island.notice.as_ref() else {
        return;
    };
    let height = notice_layout(painter, notice).height;
    let clip = spot.clip;
    let content = spot.content(ALERT_W, height);
    let ink = Ink { painter, theme: &topbar.theme };
    let Some(buffer) = topbar
        .island
        .notice_cache
        .resolve(topbar.island.notice_rev, output_scale, || draw_notice(&ink, notice, output_scale, Some(icons)))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn push_media<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let Some(media) = topbar.media.as_ref() else {
        return;
    };
    let switch_w = topbar.switch_width(painter, View::Media).unwrap_or(0);
    let seek = topbar.seek_permille();
    let progress = seek.map(|permille| f32::from(permille) / 1000.0);
    let clip = spot.clip;
    let content = spot.content(WIDE, MEDIA_H);
    let ink = Ink { painter, theme: &topbar.theme };
    let key: MediaKey = (topbar.media_rev, seek, switch_w, crate::i18n::language());
    let Some(buffer) = topbar
        .island
        .media
        .resolve(key, output_scale, || draw_media(&ink, media, progress, switch_w, output_scale, Some(icons)))
    else {
        return;
    };
    push_clipped(renderer, buffer, (content, clip), alpha, scale, elements);
}

fn push_calendar<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, alpha: f32, spot: Spot) {
    let (renderer, painter, icons, scale, output_scale, elements) = frame.parts();
    let clip = spot.clip;
    let switch_w = topbar.switch_width(painter, View::Calendar);
    let height = calendar_height(topbar.nc_items.len());
    let content = spot.content(WIDE, height);
    if let Some(block) = calendar::block(topbar, painter, icons, output_scale) {
        let (left, top) = (content.loc.x + PAD, content.loc.y + CAL_TOP);
        let header = local_rect(left, top, WIDE - PAD * 2, calendar::GRID_TOP);
        push_clipped(renderer, &block.header, (header, clip), alpha, scale, elements);
        let grid_area =
            local_rect(left, top + calendar::GRID_TOP, WIDE - PAD * 2, calendar::BLOCK_H - calendar::GRID_TOP);
        if let Some(grid_clip) = grid_area.intersection(clip) {
            for (buffer, offset) in &block.months {
                let month = Rectangle::new(Point::from((left + offset, grid_area.loc.y)), grid_area.size);
                push_clipped(renderer, buffer, (month, grid_clip), alpha, scale, elements);
            }
        }
    }
    let first = first_item(topbar.island.first_item, topbar.nc_items.len());
    let hovered = topbar.hovered.filter(|slot| matches!(slot, Slot::NcClearAll));
    let key: NcKey =
        (topbar.nc_rev, topbar.dnd, topbar.dnd_anim.switch().key(), hovered, first, switch_w, crate::i18n::language());
    let ink = NcInk {
        ink: Ink { painter, theme: &topbar.theme },
        dnd: topbar.dnd,
        switch: topbar.dnd_anim.switch(),
        hovered,
    };
    let items = &topbar.nc_items;
    let Some(buffer) = topbar
        .island
        .nc
        .resolve(key, output_scale, || draw_notifications(&ink, (items, first), switch_w, output_scale, Some(icons)))
    else {
        return;
    };
    let buffer = buffer.clone();
    let island = &mut topbar.island;
    let marker = (topbar.nc_rev, first);
    if let Some((seen, old)) = island.nc_seen.take()
        && seen != marker
    {
        island.nc_old = Some(old);
        island.nc_fade = Anim::at(0.0);
        island.nc_fade.retarget(1.0, motion::t2(), E_OUT);
    }
    island.nc_seen = Some((marker, buffer.clone()));
    let fade = island.nc_fade.value().clamp(0.0, 1.0) as f32;
    if fade < 1.0
        && let Some(old) = &island.nc_old
    {
        push_clipped(renderer, old, (content, clip), alpha * (1.0 - fade), scale, elements);
    }
    push_clipped(renderer, &buffer, (content, clip), alpha, scale, elements);
}

fn push_surface<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, rect: LocalRect, radius: f32) {
    let (renderer, _, _, scale, output_scale, elements) = frame.parts();
    if rect.size.w <= 0 || rect.size.h <= 0 {
        return;
    }
    let corner = radius.ceil() as i32 + 1;
    let sliced = rect.size.w >= corner * 2 && rect.size.h >= corner * 2;
    let size = if sliced { (corner * 2 + 1, corner * 2 + 1) } else { (rect.size.w, rect.size.h) };
    let key = (size.0, size.1, output_scale, radius.to_bits());
    let island = &mut topbar.island;
    if island.backgrounds.len() >= SURFACE_CACHE_LIMIT && !island.backgrounds.contains_key(&key) {
        island.backgrounds.clear();
    }
    if !island.backgrounds.contains_key(&key)
        && let Some(canvas) =
            Canvas::popover(size, output_scale, radius, topbar.theme.island_bg, topbar.theme.island_line)
    {
        island.backgrounds.insert(key, canvas.finish());
    }
    let Some(buffer) = island.backgrounds.get(&key) else {
        return;
    };
    if !sliced {
        push_buffer(renderer, buffer, rect.loc, scale, elements);
        return;
    }
    for (src, dst) in nine_slices(rect, corner) {
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            dst.loc.to_f64().to_physical(scale),
            buffer,
            None,
            Some(src),
            Some(dst.size),
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }
}

pub(super) fn nine_slices(rect: LocalRect, corner: i32) -> Vec<(Rectangle<f64, Logical>, LocalRect)> {
    let spans = |at: i32, length: i32| {
        [
            (0, corner, at, corner),
            (corner, 1, at + corner, length - corner * 2),
            (corner + 1, corner, at + length - corner, corner),
        ]
    };
    let mut out = Vec::with_capacity(9);
    for (src_y, src_h, y, h) in spans(rect.loc.y, rect.size.h) {
        for (src_x, src_w, x, w) in spans(rect.loc.x, rect.size.w) {
            if w <= 0 || h <= 0 {
                continue;
            }
            let src = Rectangle::new(
                Point::from((f64::from(src_x), f64::from(src_y))),
                Size::from((f64::from(src_w), f64::from(src_h))),
            );
            out.push((src, local_rect(x, y, w, h)));
        }
    }
    out
}

fn shadow_layers(dark: bool, level: Level) -> Vec<glide_theme::elevation::ShadowLayer> {
    let appearance = if dark { glide_theme::Appearance::Dark } else { glide_theme::Appearance::Light };
    glide_theme::elevation::layers(level, appearance)
}

fn shadow_margin(dark: bool, level: Level) -> i32 {
    let reach = shadow_layers(dark, level).iter().map(|layer| layer.blur + layer.dy).fold(0.0_f32, f32::max);
    reach.ceil() as i32 + SP1
}

fn push_shadow<R: ShellRenderer>(
    topbar: &mut Topbar, frame: &mut Frame<'_, R>, rect: LocalRect, (view, size): (View, Size<i32, Logical>),
    alpha: f32,
) {
    let (renderer, _, _, scale, output_scale, elements) = frame.parts();
    if alpha <= 0.0 || size.w <= 0 || size.h <= 0 {
        return;
    }
    let dark = topbar.theme.dark;
    let level = view.elevation();
    let margin = shadow_margin(dark, level);
    let key = (view.tag(), size.w, size.h, output_scale);
    let island = &mut topbar.island;
    if island.shadows.len() >= SURFACE_CACHE_LIMIT && !island.shadows.contains_key(&key) {
        island.shadows.clear();
    }
    if !island.shadows.contains_key(&key)
        && let Some(canvas) =
            crate::raster::shadow((size.w, size.h), margin, view.radius(), &shadow_layers(dark, level), output_scale)
    {
        island.shadows.insert(key, canvas.finish());
    }
    let Some(buffer) = island.shadows.get(&key) else {
        return;
    };
    let source = Size::from((size.w + margin * 2, size.h + margin * 2));
    let target =
        local_rect(rect.loc.x - margin, rect.loc.y - margin, rect.size.w + margin * 2, rect.size.h + margin * 2);
    push_stretched(renderer, buffer, (source, target), alpha, scale, elements);
}

fn push_scrim<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, output: Rectangle<i32, Logical>) {
    let (renderer, _, _, scale, _, elements) = frame.parts();
    let level = topbar.island.scrim.value().clamp(0.0, 1.0) as f32 * SCRIM_ALPHA;
    if level <= 0.0 {
        return;
    }
    if topbar.island.scrim_buffer.is_none()
        && let Some(mut canvas) = Canvas::new(1, 1, 1)
    {
        canvas.fill(canvas.full(), Color([0.0, 0.0, 0.0, 1.0]), 0.0);
        topbar.island.scrim_buffer = Some(canvas.finish());
    }
    if let Some(buffer) = topbar.island.scrim_buffer.as_ref() {
        push_stretched(renderer, buffer, (Size::from((1, 1)), output), level, scale, elements);
    }
}

pub(super) fn push<R: ShellRenderer>(topbar: &mut Topbar, frame: &mut Frame<'_, R>, output: Rectangle<i32, Logical>) {
    let strip = topbar.rect(output);
    let rect = topbar.island.rect(strip);
    if rect.size.w <= 0 || rect.size.h <= 0 {
        return;
    }
    let fade = topbar.island.fade.value().clamp(0.0, 1.0);
    let rise = (ENTER_RISE * (1.0 - topbar.island.fade.value_with(E_OUT).clamp(0.0, 1.0))).round() as i32;
    let (current, previous) = (topbar.island.view, topbar.island.previous);
    let settled = fade >= 1.0 || previous == current;
    let entering = Spot { strip, rise: if settled { 0 } else { rise }, clip: rect };
    push_view(topbar, frame, current, if settled { 1.0 } else { fade as f32 }, entering);
    if !settled {
        push_view(topbar, frame, previous, (1.0 - fade) as f32, Spot { strip, rise: 0, clip: rect });
    }
    push_surface(topbar, frame, rect, current.radius());
    let (from, to) = (topbar.island.from, topbar.island.to);
    push_shadow(topbar, frame, rect, (current, to), if settled { 1.0 } else { fade as f32 });
    if !settled {
        push_shadow(topbar, frame, rect, (previous, from), (1.0 - fade) as f32);
    }
    push_scrim(topbar, frame, output);
}

pub(super) fn push_compact_only<R: ShellRenderer>(
    topbar: &mut Topbar, frame: &mut Frame<'_, R>, output: Rectangle<i32, Logical>,
) {
    let strip = topbar.rect(output);
    let size = Size::from((topbar.island.compact_w, COMPACT_H));
    let rect = placed(strip, size);
    if rect.size.w <= 0 || rect.size.h <= 0 {
        return;
    }
    push_compact(topbar, frame, 1.0, Spot { strip, rise: 0, clip: rect });
    push_surface(topbar, frame, rect, View::Compact.radius());
    push_shadow(topbar, frame, rect, (View::Compact, size), 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_morph_reuses_corner_slices_across_sizes() {
        let corner = 9;
        for (w, h) in [(344, 29), (500, 120), (712, 84), (356, 201), (18, 18)] {
            let rect = local_rect(10, 2, w, h);
            let slices = nine_slices(rect, corner);
            let area: i32 = slices.iter().map(|(_, dst)| dst.size.w * dst.size.h).sum();
            assert_eq!(area, w * h, "{w}x{h}: the slices tile the island exactly");
            for (src, dst) in &slices {
                assert!(src.loc.x + src.size.w <= f64::from(corner * 2 + 1));
                assert!(src.loc.y + src.size.h <= f64::from(corner * 2 + 1));
                assert!(rect.contains_rect(*dst));
                if src.size.w > 1.0 && src.size.h > 1.0 {
                    assert_eq!(dst.size, Size::from((corner, corner)), "corners are never stretched");
                }
            }
        }
    }

    fn widths() -> ClockWidths {
        ClockWidths { date: 60, time: 34, badge: 0 }
    }

    fn prompt(lang: Lang) -> Notice {
        use crate::notifications::NoticeButton;
        let (title, body, allow, cancel) = match lang {
            Lang::En => (
                "Signal wants to type and read keys in other apps",
                "Allow it only if you trust this program, then start it again.\n/usr/lib/signal-desktop/signal-desktop",
                "Allow",
                "Cancel",
            ),
            Lang::Uk => (
                "Signal хоче вводити текст і читати клавіші в інших програмах",
                "Дозвольте, лише якщо довіряєте цій програмі, і запустіть її знову.\n/usr/lib/signal-desktop/signal-desktop",
                "Дозволити",
                "Скасувати",
            ),
        };
        Notice {
            id: 7,
            app: "Glide".into(),
            title: title.into(),
            body: body.into(),
            icon: None,
            buttons: vec![NoticeButton::Action(0, allow.into()), NoticeButton::Action(1, cancel.into())],
            reply: None,
            urgent: false,
            asks: true,
            waiting: 0,
        }
    }

    fn painter() -> Painter {
        Painter::new(14.0, None)
    }

    #[test]
    fn a_prompt_keeps_its_whole_question_and_puts_the_primary_button_on_the_right() {
        let painter = painter();
        for lang in [Lang::En, Lang::Uk] {
            let notice = prompt(lang);
            let layout = notice_layout(&painter, &notice);
            assert!(!layout.title.is_empty() && layout.title.len() <= NOTICE_TITLE_LINES);
            assert!(layout.title.iter().all(|line| !line.ends_with('…')), "{lang:?}: {:?}", layout.title);
            assert_eq!(layout.body.last().map(String::as_str), Some("/usr/lib/signal-desktop/signal-desktop"));
            let [allow, cancel] = layout.buttons[..] else { panic!("two buttons") };
            assert_eq!(allow.loc.x + allow.size.w, ALERT_W - ALERT_PAD, "the first button is the primary one");
            assert!(cancel.loc.x + cancel.size.w < allow.loc.x);
            assert_eq!(allow.loc.y + BUTTON_H + ALERT_PAD, layout.height);
            assert!(layout.field.is_none() && layout.badge.is_none());
        }
    }

    #[test]
    fn a_reply_swaps_the_buttons_for_a_field_and_waiting_notices_show_a_count() {
        let painter = painter();
        let bare = notice_layout(&painter, &Notice { buttons: Vec::new(), body: String::new(), ..prompt(Lang::En) });
        assert_eq!(bare.height, ALERT_PAD * 2 + ALERT_ICON, "never shorter than the icon");
        let composing = Notice {
            reply: Some(NoticeReply::Composing { shown: "hi".into(), placeholder: "Message".into(), invalid: false }),
            waiting: 2,
            ..prompt(Lang::En)
        };
        let layout = notice_layout(&painter, &composing);
        let (field, send) = (layout.field.expect("field"), layout.send.expect("send"));
        assert!(layout.buttons.is_empty());
        assert_eq!(field.size.h, FIELD_H);
        assert!(field.contains(send.loc) && field.loc.x + field.size.w > send.loc.x + send.size.w);
        assert!(layout.badge.is_some_and(|badge| badge.loc.x < layout.close.loc.x));
        let tail = tail_that_fits(&painter, "a very long reply indeed", 40);
        assert!(tail.ends_with("indeed") || "indeed".ends_with(tail), "the caret end stays visible: {tail:?}");
        assert!(text_w(&painter, tail, &FIELD_STYLE) <= 40);
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_ISLAND_PREVIEW"]
    fn notice_preview() {
        let Some(dir) = std::env::var_os("GLIDE_ISLAND_PREVIEW") else {
            return;
        };
        let painter = painter();
        let scale = 2;
        for appearance in [glide_theme::Appearance::Dark, glide_theme::Appearance::Light] {
            let theme = TopbarTheme::from_tokens(&glide_theme::Tokens::new(appearance, glide_theme::Accent::Blue));
            let ink = Ink { painter: &painter, theme: &theme };
            let cases = [
                ("en", prompt(Lang::En)),
                ("uk", Notice { waiting: 2, ..prompt(Lang::Uk) }),
                (
                    "many",
                    Notice {
                        title: "Signal просить доступ".into(),
                        body:
                            "Запис екрана · буфер обміну · клавіші в інших програмах\nДозвольте, лише якщо довіряєте \
                               цій програмі, і запустіть її знову.\n/usr/lib/signal-desktop/signal-desktop"
                                .into(),
                        ..prompt(Lang::Uk)
                    },
                ),
                (
                    "reply",
                    Notice {
                        app: "Telegram".into(),
                        title: "Olena".into(),
                        body: "Are we still on for tonight?".into(),
                        buttons: Vec::new(),
                        reply: Some(NoticeReply::Composing {
                            shown: "Yes, at seven".into(),
                            placeholder: "Message…".into(),
                            invalid: false,
                        }),
                        ..prompt(Lang::En)
                    },
                ),
                (
                    "urgent",
                    Notice {
                        title: "Battery is almost empty".into(),
                        body: "5 % left. Plug in the charger.".into(),
                        buttons: Vec::new(),
                        urgent: true,
                        ..prompt(Lang::En)
                    },
                ),
            ];
            for (name, notice) in cases {
                let layout = notice_layout(&painter, &notice);
                let mut c = Canvas::popover((ALERT_W, layout.height), scale, R2, theme.island_bg, theme.island_line)
                    .expect("surface");
                paint_notice(&mut c, &ink, &notice, &layout);
                let (w, h) = ((ALERT_W * scale) as u32, (layout.height * scale) as u32);
                let mut image = glide_shell::image::Image::new(w + 64, h + 64).expect("image");
                let shade = if appearance == glide_theme::Appearance::Dark { 30 } else { 230 };
                let backdrop = vec![shade; ((w + 64) * (h + 64) * 4) as usize];
                image.draw(&glide_shell::image::Image::from_bgra(w + 64, h + 64, backdrop).expect("backdrop"), 0, 0);
                image.draw(&glide_shell::image::Image::from_bgra(w, h, c.buf.clone()).expect("notice"), 32, 32);
                let png = image.png().expect("png");
                let file = format!("notice-{name}-{appearance:?}.png").to_lowercase();
                std::fs::write(std::path::Path::new(&dir).join(file), png).expect("write");
            }
        }
    }

    #[test]
    fn the_compact_island_keeps_its_concept_width_with_media_and_hugs_the_clock_without() {
        assert_eq!(compact_width(widths(), true), COMPACT_W);
        let bare = compact_width(widths(), false);
        assert_eq!(bare, COMPACT_PAD * 2 + SUB_PAD * 2 + 60 + SUB_GAP + 34);
        assert!(bare < COMPACT_W);
        let badge = ClockWidths { badge: 24, ..widths() };
        assert_eq!(compact_width(badge, false), bare + SUB_GAP + 24, "the badge widens a clock-only island");
        assert_eq!(compact_width(badge, true), COMPACT_W, "with media the clock takes the room from the title");
    }

    #[test]
    fn the_compact_island_is_centred_between_the_top_edge_and_the_bars_hairline() {
        assert_eq!(TOP * 2 + COMPACT_H, super::super::HEIGHT - HAIRLINE);
        const { assert!(COMPACT_W <= WIDE, "the bar reserves WIDE for the island") };
        for inner in [SUB_H, SEP_H, BADGE_H, MEDIA_ART] {
            assert_eq!((COMPACT_H - inner) % 2, 0, "a {inner} px part centres on whole pixels");
        }
    }

    #[test]
    fn compact_controls_sit_inside_the_island_with_the_clock_on_the_right() {
        let layout = compact_layout(COMPACT_W, widths(), true);
        let media = layout.media.expect("a media button when something plays");
        let separator = layout.separator.expect("a separator between the two halves");
        assert_eq!(media.loc.x, COMPACT_PAD);
        assert_eq!(layout.clock.loc.x + layout.clock.size.w, COMPACT_W - COMPACT_PAD);
        assert!(media.loc.x + media.size.w + SEP_GAP <= separator.loc.x);
        assert!(separator.loc.x + 1 + SEP_GAP <= layout.clock.loc.x);
        for rect in [media, layout.clock] {
            assert!(rect.loc.y >= 0 && rect.loc.y + rect.size.h <= COMPACT_H);
        }
        let bare = compact_layout(compact_width(widths(), false), widths(), false);
        assert!(bare.media.is_none() && bare.separator.is_none());
        assert_eq!(bare.clock.loc.x, COMPACT_PAD);
    }

    #[test]
    fn expanded_heights_match_the_concept_and_grow_with_notifications() {
        assert_eq!(MEDIA_H, 201);
        assert_eq!(calendar_height(0), STACK_Y + EMPTY_H + PAD);
        assert_eq!(calendar_height(1), STACK_Y + ITEM_H + PAD);
        assert_eq!(calendar_height(4), calendar_height(9), "more than four notifications scroll");
        assert!(calendar_height(4) <= 564, "never taller than the concept's 564 px");
        assert_eq!(stack_height(4), 4 * ITEM_H + 3 * ITEM_GAP);
    }

    #[test]
    fn the_notification_list_scrolls_by_whole_items_and_stops_at_both_ends() {
        assert_eq!(first_item(0, 9), 0);
        assert_eq!(first_item(3, 9), 3);
        assert_eq!(first_item(7, 9), 5, "the last page stays full");
        assert_eq!(first_item(4, 3), 0, "a short list never scrolls");
    }

    #[test]
    fn the_media_view_stacks_header_art_scrubber_and_transport() {
        let layout = media_layout(90);
        assert!(layout.switch.loc.x + layout.switch.size.w + HEADER_GAP <= layout.collapse.loc.x);
        assert_eq!(layout.collapse.loc.x + COLLAPSE, WIDE - PAD);
        assert!(layout.art.loc.y + layout.art.size.h < layout.track.loc.y);
        assert!(layout.seek.loc.y < layout.track.loc.y);
        assert!(layout.seek.loc.y + layout.seek.size.h > layout.track.loc.y + TRACK_H);
        let [prev, play, next] = layout.transport;
        assert_eq!(play.size.w, TRANSPORT_MAIN);
        assert_eq!(prev.loc.y + prev.size.h / 2, play.loc.y + play.size.h / 2);
        assert_eq!(play.loc.x + play.size.w / 2, WIDE / 2, "play sits in the middle");
        assert!(next.loc.x > play.loc.x);
        assert_eq!(play.loc.y + play.size.h + PAD, MEDIA_H);
    }

    #[test]
    fn the_alert_is_twice_the_size_of_the_old_banner_and_keeps_its_parts_apart() {
        assert_eq!((ALERT_W, ALERT_H), (2 * 356, 2 * 42));
        let layout = alert_layout();
        assert_eq!(layout.icon.loc.x, ALERT_PAD);
        assert_eq!(layout.icon.loc.y * 2 + ALERT_ICON, ALERT_H);
        assert_eq!(layout.close.loc.x + ALERT_CLOSE, ALERT_W - ALERT_PAD);
        assert!(layout.text.0 >= layout.icon.loc.x + ALERT_ICON && layout.text.1 <= layout.close.loc.x);
        let title = ALERT_TITLE_STYLE.line_height_px.ceil() as i32;
        let body = ALERT_BODY_STYLE.line_height_px.ceil() as i32;
        assert!(title + body * ALERT_LINES as i32 + 2 * SP2 <= ALERT_H, "the heading and two body lines fit");
    }

    #[test]
    fn alert_bodies_wrap_onto_two_lines_and_never_keep_their_newlines() {
        let painter = crate::paint::Painter::new(13.0, None);
        let alert = IslandAlert {
            id: 1,
            heading: String::from("Mail"),
            summary: String::from("first\nsecond line of a body long enough to need wrapping across the alert ")
                + &"word ".repeat(80),
            icon: None,
        };
        let lines = alert_lines(&painter, &alert, 400);
        assert_eq!(lines.len(), ALERT_LINES);
        assert!(lines.iter().all(|line| !line.contains('\n')));
        assert!(lines[0].starts_with("first second"));
        assert!(lines[1].ends_with('…'));
    }

    #[test]
    fn a_view_change_crossfades_and_a_new_size_morphs_from_where_the_island_is() {
        let strip = local_rect(0, 0, 1920, 34);
        let mut island = Island::default();
        island.aim(Size::from((COMPACT_W, COMPACT_H)));
        let first = local_rect(960 - COMPACT_W / 2, TOP, COMPACT_W, COMPACT_H);
        assert_eq!(island.rect(strip), first, "the first size is taken at once");
        assert!(island.show(View::Calendar));
        assert!(!island.show(View::Calendar));
        assert_eq!(island.previous, View::Compact);
        island.aim(Size::from((WIDE, 500)));
        let now = island.rect(strip);
        assert!(now.size.w >= COMPACT_W && now.size.w <= WIDE);
        assert_eq!(island.target(strip), local_rect(782, TOP, WIDE, 500));
        assert!(!island.morph.is_animating() || now.size.h < 500, "the morph starts from the compact size");
    }

    #[test]
    fn every_output_draws_the_same_morph_centred_on_its_own_bar() {
        let (left, right) = (local_rect(0, 0, 1920, 34), local_rect(1920, 0, 2560, 34));
        let mut island = Island::default();
        island.aim(Size::from((COMPACT_W, COMPACT_H)));
        island.aim(Size::from((ALERT_W, ALERT_H)));
        let animating = island.morph.is_animating();
        island.aim(Size::from((ALERT_W, ALERT_H)));
        assert_eq!(island.morph.is_animating(), animating, "laying out for another output does not restart it");
        let (a, b) = (island.rect(left), island.rect(right));
        assert_eq!(a.size, b.size);
        assert_eq!(a.loc.x + a.size.w / 2, 960);
        assert_eq!(b.loc.x + b.size.w / 2, 1920 + 1280);
    }

    #[test]
    fn alerts_read_like_the_concept() {
        let item = NcItem {
            id: 7,
            app_name: String::from("Telegram"),
            title: String::from("Olena"),
            body: String::from("Call at 15:00?"),
            time: String::new(),
            icon: None,
            count: 1,
        };
        let alert = IslandAlert::from_item(&item);
        assert_eq!((alert.heading.as_str(), alert.summary.as_str()), ("Telegram", "Olena: Call at 15:00?"));
        let bare = IslandAlert::from_item(&NcItem { app_name: String::new(), ..item.clone() });
        assert_eq!((bare.heading.as_str(), bare.summary.as_str()), ("Olena", "Call at 15:00?"));
        let quiet = IslandAlert::from_item(&NcItem { body: String::new(), ..item });
        assert_eq!(quiet.summary, "Olena");
    }

    #[test]
    fn the_equaliser_holds_still_without_motion_and_rests_low_when_paused() {
        assert_eq!(bar_heights(true, None), BARS_STILL);
        assert_eq!(bar_heights(false, Some(Duration::from_millis(250))), [BAR_PAUSED; 3]);
        assert_eq!(bars_clock(Duration::from_secs(3), 0.0), None, "animations off stop the bars");
        assert_eq!(bars_clock(Duration::from_secs(3), 2.0), Some(Duration::from_millis(1500)), "speed slows them");
    }

    #[test]
    fn equaliser_bars_swing_between_the_concept_keyframes_inside_their_box() {
        assert_eq!(bar_heights(true, Some(Duration::ZERO)), [3, BARS_H, 5]);
        for (index, (from, to, period)) in BAR_SWINGS.into_iter().enumerate() {
            assert_eq!(bar_heights(true, Some(period))[index], to, "bar {index} reaches its far keyframe");
            assert_eq!(bar_heights(true, Some(period * 2))[index], from, "bar {index} swings back");
        }
        let mut seen = std::collections::HashSet::new();
        for ms in (0..1400).step_by(10) {
            let heights = bar_heights(true, Some(Duration::from_millis(ms)));
            assert!(heights.iter().all(|h| (BAR_PAUSED..=BARS_H).contains(h)), "{heights:?} at {ms} ms");
            seen.insert(heights);
        }
        assert!(seen.len() > 20, "the bars move while playing");
        const { assert!(3 * (BAR_W + 1) - 1 <= BARS_W, "the bars fit the room the title leaves them") };
    }

    #[test]
    fn the_equaliser_pumps_frames_only_while_its_bars_are_being_drawn() {
        let mut island = Island::default();
        assert!(!island.is_animating());
        island.bars_drawn = Some(glide_ui::clock::now());
        assert!(island.is_animating());
        island.bars_drawn = glide_ui::clock::now().checked_sub(BARS_LIVE * 2);
        assert!(!island.is_animating(), "a hidden or locked bar stops the pump");
    }

    #[test]
    fn durations_print_like_a_player() {
        assert_eq!(format_duration(Duration::from_secs(81)), "01:21");
        assert_eq!(format_duration(Duration::from_secs(212)), "03:32");
        assert_eq!(format_duration(Duration::from_secs(3725)), "1:02:05");
    }
}
