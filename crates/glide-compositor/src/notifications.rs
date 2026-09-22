use std::time::{Duration, Instant};

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

use glide_notify::{CloseReason, Notification, Urgency};
use glide_ui::{Reveal, Tween};

use crate::config::Color;

pub const WIDTH: i32 = 340;
const RAIL_GAP: i32 = glide_theme::spacing::SP2;
const RAIL_INSET: i32 = glide_theme::spacing::SP3;
const PADDING: i32 = glide_theme::spacing::SP3;
const RADIUS: f32 = glide_theme::shape::R3;

const ICON: i32 = 32;
const ICON_RADIUS: f32 = glide_theme::shape::R2;
const ICON_GAP: i32 = glide_theme::spacing::SP3;

const META_HEIGHT: i32 = 15;
const TITLE_HEIGHT: i32 = 18;
const BODY_LINE: i32 = 17;
const BODY_MAX_LINES: usize = 3;
const PROGRESS_HEIGHT: i32 = 6;
const PROGRESS_GAP: i32 = glide_theme::spacing::SP2;

const DISMISS_SIDE: i32 = 22;
const DISMISS_INSET: i32 = glide_theme::spacing::SP2;
const DISMISS_GLYPH: f32 = 9.0;
const DISMISS_STROKE: f32 = 1.8;

const BUTTON_HEIGHT: i32 = 28;
const BUTTON_PAD_X: i32 = glide_theme::spacing::SP3;
const BUTTON_GAP: i32 = glide_theme::spacing::SP2;
const BUTTON_RADIUS: f32 = glide_theme::shape::R2;
const BUTTON_ROW_GAP: i32 = glide_theme::spacing::SP2;
const MAX_BUTTONS: usize = 3;

const REPLY_HEIGHT: i32 = 34;
const REPLY_PAD_LEFT: i32 = glide_theme::spacing::SP3;
const REPLY_SEND: i32 = 26;
const REPLY_SEND_INSET: i32 = glide_theme::spacing::SP1;
const REPLY_RING: i32 = 3;
const REPLY_MAX_CHARS: usize = 500;

const ENTER_SHIFT: i32 = 24;
const LEAVE_SCALE: f64 = 0.96;

const MAX_VISIBLE: usize = 4;
const MAX_STACKED: usize = 8;
const PEEK_REVEAL: i32 = 5;
const PEEK_BODY: i32 = 16;
const PEEK_INSET: i32 = glide_theme::spacing::SP2;
const PEEK_MAX: usize = 2;

const COUNT_HEIGHT: i32 = 15;
const COUNT_PAD_X: i32 = 5;
const COUNT_RADIUS: f32 = glide_theme::shape::R_ROUND;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAction {
    Activate(u32),
    Dismiss(u32),
    Button(u32, usize),
    OpenReply(u32),
    SendReply(u32),
    Configure(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcItem {
    pub id: u32,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub time: String,
    pub icon: Option<Vec<u8>>,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hit {
    Body(u32),
    Dismiss(u32),
    Button(u32, usize),
    Reply(u32),
    Send(u32),
    Settings(u32),
}

impl Hit {
    const fn action(self) -> NotificationAction {
        match self {
            Self::Body(id) => NotificationAction::Activate(id),
            Self::Dismiss(id) => NotificationAction::Dismiss(id),
            Self::Button(id, index) => NotificationAction::Button(id, index),
            Self::Reply(id) => NotificationAction::OpenReply(id),
            Self::Send(id) => NotificationAction::SendReply(id),
            Self::Settings(id) => NotificationAction::Configure(id),
        }
    }

    const fn id(self) -> u32 {
        match self {
            Self::Body(id)
            | Self::Dismiss(id)
            | Self::Button(id, _)
            | Self::Reply(id)
            | Self::Send(id)
            | Self::Settings(id) => id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LayerShape {
    Chip(i32),
    Dismiss,
    Send,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LayerKind {
    Hover,
    Press,
}

impl LayerShape {
    const fn size(self) -> (i32, i32) {
        match self {
            Self::Chip(width) => (width, BUTTON_HEIGHT),
            Self::Dismiss => (DISMISS_SIDE, DISMISS_SIDE),
            Self::Send => (REPLY_SEND, REPLY_SEND),
        }
    }

    const fn radius(self) -> f32 {
        match self {
            Self::Chip(_) | Self::Send => BUTTON_RADIUS,
            Self::Dismiss => glide_theme::shape::R_ROUND,
        }
    }
}

fn ease_out(t: f64) -> f64 {
    glide_theme::motion::E_OUT.ease_f64(t)
}

fn ease_io(t: f64) -> f64 {
    glide_theme::motion::E_IO.ease_f64(t)
}

#[derive(Debug)]
struct Entry {
    notification: Notification,
    shown_at: Instant,
    icon: Option<Vec<u8>>,
    height: i32,
    body_lines: Option<Vec<String>>,
    buttons: Vec<i32>,
    reply_button: Option<i32>,
    settings_button: Option<i32>,
    reply_text: Option<String>,
    reply_invalid_since: Option<Instant>,
    reply_sent_at: Option<Instant>,
    stacked: Vec<Notification>,
    reveal: Reveal,
    y: Option<Tween>,
    peeks: Vec<Reveal>,
    ring: Tween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ReplyPhase {
    Closed,
    Composing,
    Invalid,
    Sent,
}

impl Entry {
    fn group_size(&self) -> usize {
        self.stacked.len() + 1
    }

    fn reply_phase(&self) -> ReplyPhase {
        if self.reply_sent_at.is_some() {
            return ReplyPhase::Sent;
        }
        if self.reply_text.is_none() {
            return ReplyPhase::Closed;
        }
        if self.reply_invalid() {
            ReplyPhase::Invalid
        } else {
            ReplyPhase::Composing
        }
    }

    fn reply_invalid(&self) -> bool {
        self.reply_invalid_since
            .is_some_and(|since| since.elapsed() < glide_theme::motion::t3())
    }

    fn reply_confirmation_elapsed(&self) -> bool {
        self.reply_sent_at
            .is_some_and(|sent| sent.elapsed() >= glide_theme::motion::t4())
    }

    fn reply_row_open(&self) -> bool {
        self.reply_text.is_some() || self.reply_sent_at.is_some()
    }

    fn leaving(&self) -> bool {
        !self.reveal.is_open()
    }

    fn leave_progress(&self) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let progress = self
            .reveal
            .progress(glide_theme::motion::t2(), ease_io)
            .clamp(0.0, 1.0) as f32;
        progress
    }

    fn drawn_offset(&self, target: i32) -> i32 {
        #[allow(clippy::cast_possible_truncation)]
        self.y.map_or(target, |tween| {
            tween
                .value(glide_theme::motion::t3(), ease_out)
                .round() as i32
        })
    }

    fn ring_alpha(&self) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let alpha = self.ring.value(glide_theme::motion::t1(), ease_io) as f32;
        alpha
    }

    fn is_animating(&self) -> bool {
        let t1 = glide_theme::motion::t1();
        let t2 = glide_theme::motion::t2();
        let t3 = glide_theme::motion::t3();
        enter_progress(self) < 1.0
            || self.reply_invalid()
            || self.reply_sent_at.is_some()
            || self.leaving()
            || self.y.is_some_and(|tween| tween.is_animating(t3))
            || self.peeks.iter().any(|reveal| reveal.is_animating(t2))
            || self.ring.is_animating(t1)
    }

    fn owns(&self, id: u32) -> bool {
        self.notification.id == id
            || self
                .stacked
                .iter()
                .any(|notification| notification.id == id)
    }

    fn ids(&self) -> Vec<u32> {
        let mut ids = vec![self.notification.id];
        ids.extend(self.stacked.iter().map(|notification| notification.id));
        ids
    }

    fn remeasure(&mut self) {
        self.body_lines = None;
        self.buttons.clear();
        self.reply_button = None;
        self.settings_button = None;
        self.reply_text = None;
        self.reply_invalid_since = None;
        self.reply_sent_at = None;
    }

    fn promote(&mut self) -> bool {
        if self.stacked.is_empty() {
            return false;
        }
        self.notification = self.stacked.remove(0);
        self.shown_at = Instant::now();
        self.remeasure();
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationTheme {
    pub enabled: bool,
    pub background: Color,
    pub border: Color,
    pub title: Color,
    pub body: Color,
    pub meta: Color,
    pub fill: Color,
    pub accent: Color,
    pub accent_label: Color,
    pub accent_ring: Color,
    pub field: Color,
    pub danger: Color,
    pub reply_invalid_border: Color,
    pub hover: Color,
}

impl NotificationTheme {
    #[must_use]
    pub fn revision(&self) -> u64 {
        glide_theme::Revision::new()
            .bool(self.enabled)
            .color(self.background)
            .color(self.border)
            .color(self.title)
            .color(self.body)
            .color(self.meta)
            .color(self.fill)
            .color(self.accent)
            .color(self.accent_label)
            .color(self.accent_ring)
            .color(self.field)
            .color(self.danger)
            .color(self.reply_invalid_border)
            .color(self.hover)
            .finish()
    }
}

impl Default for NotificationTheme {
    fn default() -> Self {
        let t = glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue);
        let tint = |alpha: f32| Color([t.fg_1.0[0], t.fg_1.0[1], t.fg_1.0[2], alpha]);
        Self {
            enabled: true,
            background: t.popover_bg,
            border: t.line,
            title: t.fg_1,
            body: tint(0.68),
            meta: tint(0.46),
            fill: tint(0.07),
            accent: t.acc_bg,
            accent_label: t.acc_fg,
            accent_ring: t.acc_ring,
            field: t.view_bg,
            danger: t.danger,
            reply_invalid_border: t.danger,
            hover: tint(glide_theme::motion::ST_HOVER),
        }
    }
}

#[derive(Debug, Default)]
pub struct Notifications {
    theme: NotificationTheme,
    handle: Option<glide_notify::NotifyHandle>,
    live: Vec<Entry>,
    dnd: bool,
    inhibited: bool,
    hovered: Option<Hit>,
    pressed: Option<Hit>,
    hover_layers: std::collections::HashMap<Hit, Tween>,
    press_layers: std::collections::HashMap<Hit, Tween>,
    slots: Vec<(Hit, Rectangle<i32, Logical>)>,
    layer_slots: Vec<(Hit, Rectangle<i32, Logical>, LayerShape)>,
    buffers: std::collections::HashMap<(u32, ReplyPhase, i32), MemoryRenderBuffer>,
    shadows: std::collections::HashMap<(u32, i32, i32), MemoryRenderBuffer>,
    peeks: std::collections::HashMap<(u32, i32, i32), MemoryRenderBuffer>,
    layers: std::collections::HashMap<(LayerShape, LayerKind, i32), MemoryRenderBuffer>,
    rings: std::collections::HashMap<(bool, i32), MemoryRenderBuffer>,
    raster_rev: u64,
}

impl Notifications {
    #[must_use]
    pub fn new(theme: NotificationTheme) -> Self {
        Self {
            theme,
            ..Self::default()
        }
    }

    pub fn attach(&mut self, handle: glide_notify::NotifyHandle) {
        self.handle = Some(handle);
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.theme.enabled
    }

    #[must_use]
    pub const fn dnd(&self) -> bool {
        self.dnd
    }

    #[must_use]
    pub const fn suppressed(&self) -> bool {
        self.dnd || self.inhibited
    }

    pub const fn set_dnd(&mut self, on: bool) {
        self.dnd = on;
    }

    pub const fn set_inhibited(&mut self, on: bool) {
        self.inhibited = on;
    }

    fn active(&self) -> impl Iterator<Item = &Entry> {
        self.live.iter().filter(|entry| !entry.leaving())
    }

    fn active_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
        self.live.iter_mut().filter(|entry| !entry.leaving())
    }

    #[must_use]
    pub fn unread(&self) -> usize {
        self.active().map(Entry::group_size).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active().next().is_none()
    }

    #[must_use]
    pub fn nc_items(&self) -> Vec<NcItem> {
        let now = Instant::now();
        self.active()
            .map(|entry| {
                let elapsed = now.duration_since(entry.shown_at).as_secs();
                let time = crate::i18n::elapsed_label(elapsed / 60);
                NcItem {
                    id: entry.notification.id,
                    app_name: entry.notification.app_name.clone(),
                    title: entry.notification.summary.clone(),
                    body: entry.notification.body.clone(),
                    time,
                    icon: entry.icon.clone(),
                    count: entry.group_size(),
                }
            })
            .collect()
    }

    fn invalidate(&mut self) {
        self.buffers.clear();
        self.shadows.clear();
        self.peeks.clear();
        self.layers.clear();
        self.rings.clear();
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

    pub fn measure(&mut self, painter: &crate::paint::Painter) {
        for entry in &mut self.live {
            if entry.body_lines.is_none() {
                let lines = wrap_lines(
                    painter,
                    &entry.notification.body,
                    &glide_theme::TextStyle::CALL,
                    body_width(),
                    BODY_MAX_LINES,
                );
                entry.body_lines = Some(lines);
                entry.buttons = button_widths(painter, &entry.notification);
                entry.reply_button = entry
                    .notification
                    .reply
                    .as_ref()
                    .map(|reply| chip_width(painter, &reply.label));
                entry.settings_button = entry
                    .notification
                    .has_settings_action
                    .then(|| chip_width(painter, crate::i18n::t(crate::i18n::Key::Configure)));
            }
            let lines = entry.body_lines.as_deref().unwrap_or(&[]);
            entry.height = entry_height(&entry.notification, lines, entry.reply_row_open());
        }
    }

    #[must_use]
    pub fn reply_target(&self) -> Option<u32> {
        self.active()
            .find(|entry| entry.reply_text.is_some())
            .map(|entry| entry.notification.id)
    }

    pub fn open_reply(&mut self, id: u32) -> bool {
        let mut changed = false;
        for entry in self.active_mut() {
            let wants = entry.notification.id == id
                && entry.notification.reply.is_some()
                && entry.reply_sent_at.is_none();
            let open = entry.reply_text.is_some();
            if wants && !open {
                entry.reply_text = Some(String::new());
                changed = true;
            } else if !wants && open {
                entry.reply_text = None;
                entry.reply_invalid_since = None;
                changed = true;
            }
        }
        if changed {
            self.buffers.clear();
        }
        changed
    }

    pub fn cancel_reply(&mut self) -> bool {
        let mut changed = false;
        for entry in self.active_mut() {
            if entry.reply_text.take().is_some() {
                entry.reply_invalid_since = None;
                changed = true;
            }
        }
        if changed {
            self.buffers.clear();
        }
        changed
    }

    pub fn type_reply(&mut self, ch: char) -> bool {
        let Some(text) = self.reply_buffer() else {
            return false;
        };
        if text.chars().count() >= REPLY_MAX_CHARS {
            return false;
        }
        text.push(ch);
        self.buffers.clear();
        true
    }

    pub fn backspace_reply(&mut self) -> bool {
        let Some(text) = self.reply_buffer() else {
            return false;
        };
        if text.pop().is_none() {
            return false;
        }
        self.buffers.clear();
        true
    }

    fn reply_buffer(&mut self) -> Option<&mut String> {
        self.active_mut()
            .find_map(|entry| entry.reply_text.as_mut())
    }

    pub fn send_reply(&mut self) -> bool {
        let Some((id, sender, text)) = self.active().find_map(|entry| {
            entry.reply_text.as_ref().map(|text| {
                (
                    entry.notification.id,
                    entry.notification.sender.clone(),
                    text.clone(),
                )
            })
        }) else {
            return false;
        };

        if text.trim().is_empty() {
            for entry in self.active_mut() {
                if entry.notification.id == id {
                    entry.reply_invalid_since = Some(Instant::now());
                }
            }
            return true;
        }
        if let Some(handle) = self.handle.as_ref() {
            handle.replied(id, &sender, &text);
        }
        if glide_theme::motion::t4().is_zero() {
            self.remove(id, CloseReason::Dismissed, true);
            return true;
        }
        for entry in self.active_mut() {
            if entry.notification.id == id {
                entry.reply_text = None;
                entry.reply_invalid_since = None;
                entry.reply_sent_at = Some(Instant::now());
            }
        }
        self.forget_pointer_state(id);
        true
    }

    fn retire_sent(&mut self) -> bool {
        let done: Vec<u32> = self
            .active()
            .filter(|entry| entry.reply_confirmation_elapsed())
            .map(|entry| entry.notification.id)
            .collect();
        let mut changed = false;
        for id in done {
            changed |= self.remove(id, CloseReason::Dismissed, true);
        }
        changed
    }

    fn retire_left(&mut self) -> bool {
        let duration = glide_theme::motion::t2();
        let before = self.live.len();
        self.live
            .retain(|entry| !entry.leaving() || entry.reveal.is_visible(duration));
        if self.live.len() == before {
            return false;
        }
        let ids: std::collections::HashSet<u32> =
            self.live.iter().map(|entry| entry.notification.id).collect();
        self.buffers.retain(|(id, _, _), _| ids.contains(id));
        self.shadows.retain(|(id, _, _), _| ids.contains(id));
        true
    }

    fn forget_pointer_state(&mut self, id: u32) {
        if self.hovered.is_some_and(|hit| hit.id() == id) {
            self.hovered = None;
        }
        if self.pressed.is_some_and(|hit| hit.id() == id) {
            self.pressed = None;
        }
        self.hover_layers.retain(|hit, _| hit.id() != id);
        self.press_layers.retain(|hit, _| hit.id() != id);
    }

    fn leave(&mut self, index: usize) {
        let id = self.live[index].notification.id;
        self.live[index].reveal.close(glide_theme::motion::t2());
        self.forget_pointer_state(id);
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let t1 = glide_theme::motion::t1();
        self.live.iter().any(Entry::is_animating)
            || self
                .hover_layers
                .values()
                .chain(self.press_layers.values())
                .any(|tween| tween.is_animating(t1))
    }

    pub fn push(
        &mut self,
        icons: &mut crate::icon::IconLoader,
        notification: Notification,
        replaces: bool,
    ) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if self.suppressed() && notification.urgency != Urgency::Critical {
            return false;
        }

        if !replaces
            && self
                .live
                .iter()
                .any(|entry| entry.notification.is_duplicate_of(&notification))
        {
            self.notify_closed(notification.id, CloseReason::Expired);
            return false;
        }

        let id = notification.id;
        let group = notification.group_key();
        let icon = load_icon(icons, &notification);
        let mut entry = Entry {
            notification,
            shown_at: Instant::now(),
            icon,
            height: 0,
            body_lines: None,
            buttons: Vec::new(),
            reply_button: None,
            settings_button: None,
            reply_text: None,
            reply_invalid_since: None,
            reply_sent_at: None,
            stacked: Vec::new(),
        };

        if replaces && let Some(slot) = self.live.iter_mut().find(|slot| slot.owns(id)) {
            if slot.notification.id == id {
                entry.stacked = std::mem::take(&mut slot.stacked);
                *slot = entry;
            } else {
                for stacked in &mut slot.stacked {
                    if stacked.id == id {
                        *stacked = entry.notification.clone();
                    }
                }
            }
            self.buffers.retain(|(key, _, _, _), _| *key != id);
            return true;
        }

        self.buffers.retain(|(key, _, _, _), _| *key != id);

        if let Some(index) = self
            .live
            .iter()
            .position(|slot| !group.is_empty() && slot.notification.group_key() == group)
        {
            let previous = self.live.remove(index);
            self.buffers
                .retain(|(key, _, _, _), _| *key != previous.notification.id);
            entry.stacked = std::iter::once(previous.notification)
                .chain(previous.stacked)
                .filter(|notification| notification.id != id)
                .collect();
            while entry.stacked.len() > MAX_STACKED {
                if let Some(dropped) = entry.stacked.pop() {
                    self.notify_closed(dropped.id, CloseReason::Expired);
                }
            }
            self.live.insert(0, entry);
            return true;
        }

        self.live.retain(|slot| !slot.owns(id));
        self.live.insert(0, entry);
        while self.live.len() > MAX_VISIBLE {
            if let Some(dropped) = self.live.pop() {
                for id in dropped.ids() {
                    self.notify_closed(id, CloseReason::Expired);
                }
            }
        }
        true
    }

    pub fn remove(&mut self, id: u32, reason: CloseReason, tell_sender: bool) -> bool {
        let Some(index) = self.live.iter().position(|entry| entry.owns(id)) else {
            return false;
        };

        let is_head = self.live[index].notification.id == id;
        if !is_head {
            self.live[index]
                .stacked
                .retain(|notification| notification.id != id);
            self.buffers
                .retain(|(key, _, _, _), _| *key != self.live[index].notification.id);
            if tell_sender {
                self.notify_closed(id, reason);
            }
            return true;
        }

        self.buffers.retain(|(key, _, _, _), _| *key != id);
        if self.hovered.is_some_and(|hit| hit.id() == id) {
            self.hovered = None;
        }

        if self.live[index].promote() {
            if tell_sender {
                self.notify_closed(id, reason);
            }
            return true;
        }

        self.live.remove(index);
        if tell_sender {
            self.notify_closed(id, reason);
        }
        true
    }

    pub fn clear_all(&mut self) -> bool {
        if self.live.is_empty() {
            return false;
        }
        let ids: Vec<u32> = self.live.iter().flat_map(Entry::ids).collect();
        for id in ids {
            self.notify_closed(id, CloseReason::Dismissed);
        }
        self.live.clear();
        self.buffers.clear();
        self.hovered = None;
        true
    }

    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let expired: Vec<u32> = self
            .live
            .iter()
            .filter(|entry| {
                entry
                    .notification
                    .timeout
                    .is_some_and(|timeout| now.duration_since(entry.shown_at) >= timeout)
            })
            .map(|entry| entry.notification.id)
            .collect();

        let mut changed = false;
        for id in expired {
            changed |= self.remove(id, CloseReason::Expired, true);
        }
        let retired = self.retire_sent();
        changed || retired
    }

    #[must_use]
    pub fn next_deadline(&self) -> Option<Duration> {
        let now = Instant::now();
        let confirmation = glide_theme::motion::t4();
        self.live
            .iter()
            .filter_map(|entry| {
                let expiry = entry
                    .notification
                    .timeout
                    .map(|timeout| timeout.saturating_sub(now.duration_since(entry.shown_at)));
                let close = entry
                    .reply_sent_at
                    .map(|sent| confirmation.saturating_sub(now.duration_since(sent)));
                match (expiry, close) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                }
            })
            .min()
    }

    fn notify_closed(&self, id: u32, reason: CloseReason) {
        if let Some(handle) = self.handle.as_ref() {
            handle.closed(id, reason);
        }
    }

    pub fn press_button(&mut self, id: u32, index: usize, token: Option<&str>) {
        let Some(entry) = self.live.iter().find(|entry| entry.notification.id == id) else {
            return;
        };
        let Some(action) = visible_buttons(&entry.notification).get(index) else {
            return;
        };
        let key = action.key.clone();
        let resident = entry.notification.resident;
        if let Some(handle) = self.handle.as_ref() {
            handle.invoked(id, &key, token);
        }
        if !resident {
            self.remove(id, CloseReason::Dismissed, true);
        }
    }

    pub fn configure(&mut self, id: u32, token: Option<&str>) {
        let Some(entry) = self.live.iter().find(|entry| entry.owns(id)) else {
            return;
        };
        if !entry.notification.has_settings_action {
            return;
        }
        let head = entry.notification.id;
        if let Some(handle) = self.handle.as_ref() {
            handle.invoked(head, glide_notify::ACTION_SETTINGS, token);
        }
        self.remove(head, CloseReason::Dismissed, true);
    }

    pub fn activate(&mut self, id: u32, token: Option<&str>) {
        let Some(entry) = self.live.iter().find(|entry| entry.notification.id == id) else {
            return;
        };
        let has_default = entry.notification.has_default_action;
        let resident = entry.notification.resident;
        if let Some(handle) = self.handle.as_ref()
            && has_default
        {
            handle.invoked(id, glide_notify::ACTION_DEFAULT, token);
        }
        if !resident {
            self.remove(id, CloseReason::Dismissed, true);
        }
    }

    #[must_use]
    pub fn rect(&self, output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let total: i32 = self
            .live
            .iter()
            .map(|entry| occupied_height(entry) + RAIL_GAP)
            .sum::<i32>()
            .saturating_sub(RAIL_GAP)
            .max(0);
        Rectangle::new(
            Point::from((
                output.loc.x + output.size.w - RAIL_INSET - WIDTH,
                output.loc.y + crate::topbar::HEIGHT + RAIL_GAP,
            )),
            Size::from((WIDTH, total)),
        )
    }

    #[must_use]
    pub fn contains(&self, output: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        self.is_enabled() && !self.live.is_empty() && self.rect(output).to_f64().contains(pos)
    }

    #[must_use]
    pub fn action_at(&self, pos: Point<f64, Logical>) -> Option<NotificationAction> {
        self.slots
            .iter()
            .find(|(_, rect)| rect.to_f64().contains(pos))
            .map(|(hit, _)| hit.action())
    }

    pub fn set_hover(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        let hovered = pos.and_then(|pos| {
            self.slots
                .iter()
                .find(|(_, rect)| rect.to_f64().contains(pos))
                .map(|(hit, _)| *hit)
        });
        if self.hovered == hovered {
            return false;
        }
        self.hovered = hovered;
        true
    }

    fn layout_slots(&mut self, output: Rectangle<i32, Logical>) {
        let origin = self.rect(output).loc;
        let mut y = origin.y;
        let mut slots = Vec::new();

        for entry in &self.live {
            let id = entry.notification.id;
            slots.push((
                Hit::Dismiss(id),
                Rectangle::new(
                    Point::from((
                        origin.x + WIDTH - DISMISS_INSET - DISMISS_SIDE,
                        y + DISMISS_INSET,
                    )),
                    Size::from((DISMISS_SIDE, DISMISS_SIDE)),
                ),
            ));

            if entry.reply_text.is_some() {
                let top = y + entry.height - PADDING - REPLY_HEIGHT;
                let left = origin.x + PADDING + ICON + ICON_GAP;
                slots.push((
                    Hit::Send(id),
                    Rectangle::new(
                        Point::from((
                            origin.x + WIDTH - PADDING - REPLY_SEND_INSET - REPLY_SEND,
                            top + (REPLY_HEIGHT - REPLY_SEND) / 2,
                        )),
                        Size::from((REPLY_SEND, REPLY_SEND)),
                    ),
                ));
                slots.push((
                    Hit::Reply(id),
                    Rectangle::new(
                        Point::from((left, top)),
                        Size::from((WIDTH - PADDING - left + origin.x, REPLY_HEIGHT)),
                    ),
                ));
            } else if entry.reply_sent_at.is_none() {
                let mut x = origin.x + PADDING + ICON + ICON_GAP;
                let top = y + entry.height - PADDING - BUTTON_HEIGHT;
                if let Some(width) = entry.reply_button {
                    slots.push((
                        Hit::Reply(id),
                        Rectangle::new(Point::from((x, top)), Size::from((width, BUTTON_HEIGHT))),
                    ));
                    x += width + BUTTON_GAP;
                }
                for (index, width) in entry.buttons.iter().enumerate() {
                    slots.push((
                        Hit::Button(id, index),
                        Rectangle::new(Point::from((x, top)), Size::from((*width, BUTTON_HEIGHT))),
                    ));
                    x += width + BUTTON_GAP;
                }
                if let Some(width) = entry.settings_button {
                    slots.push((
                        Hit::Settings(id),
                        Rectangle::new(Point::from((x, top)), Size::from((width, BUTTON_HEIGHT))),
                    ));
                }
            }

            slots.push((
                Hit::Body(id),
                Rectangle::new(
                    Point::from((origin.x, y)),
                    Size::from((WIDTH, entry.height)),
                ),
            ));
            y += occupied_height(entry) + RAIL_GAP;
        }

        self.slots = slots;
    }
}

const fn body_width() -> i32 {
    WIDTH - PADDING * 2 - ICON - ICON_GAP
}

fn peek_layers(entry: &Entry) -> usize {
    entry.stacked.len().min(PEEK_MAX)
}

fn occupied_height(entry: &Entry) -> i32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let layers = peek_layers(entry) as i32;
    entry.height + PEEK_REVEAL * layers
}

fn entry_height(notification: &Notification, body_lines: &[String], replying: bool) -> i32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let lines = body_lines.len() as i32;
    let mut height = PADDING * 2 + META_HEIGHT + TITLE_HEIGHT + lines * BODY_LINE;
    if notification.progress.is_some() {
        height += PROGRESS_GAP + PROGRESS_HEIGHT;
    }
    if replying {
        height += BUTTON_ROW_GAP + REPLY_HEIGHT;
    } else if !visible_buttons(notification).is_empty()
        || notification.reply.is_some()
        || notification.has_settings_action
    {
        height += BUTTON_ROW_GAP + BUTTON_HEIGHT;
    }
    height.max(PADDING * 2 + ICON)
}

fn visible_buttons(notification: &Notification) -> &[glide_notify::Action] {
    let count = notification.actions.len().min(MAX_BUTTONS);
    &notification.actions[..count]
}

fn chip_width(painter: &crate::paint::Painter, label: &str) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let text = painter
        .text_width_styled(label, &glide_theme::TextStyle::LAUNCHER_CHIP)
        .ceil() as i32;
    text + BUTTON_PAD_X * 2
}

fn button_widths(painter: &crate::paint::Painter, notification: &Notification) -> Vec<i32> {
    visible_buttons(notification)
        .iter()
        .map(|action| chip_width(painter, &action.label))
        .collect()
}

fn shadow_margin() -> i32 {
    let reach = shadow_layers()
        .iter()
        .map(|layer| layer.blur + layer.dy)
        .fold(0.0_f32, f32::max);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        reach.ceil() as i32 + glide_theme::spacing::SP1
    }
}

fn shadow_layers() -> Vec<glide_theme::elevation::ShadowLayer> {
    glide_theme::elevation::layers(
        glide_theme::elevation::Level::El3,
        glide_theme::Appearance::Dark,
    )
}

fn enter_progress(entry: &Entry) -> f32 {
    let duration = glide_theme::motion::t4();
    if duration.is_zero() {
        return 1.0;
    }
    let elapsed = entry.shown_at.elapsed();
    if elapsed >= duration {
        return 1.0;
    }
    let raw = elapsed.as_secs_f32() / duration.as_secs_f32();
    glide_theme::motion::E_OUT.ease(raw.clamp(0.0, 1.0))
}

fn load_icon(icons: &mut crate::icon::IconLoader, notification: &Notification) -> Option<Vec<u8>> {
    if !notification.desktop_entry.trim().is_empty()
        && let Some(bytes) = icons.load(&notification.desktop_entry, ICON)
    {
        return Some(bytes.to_vec());
    }
    if !notification.app_icon.trim().is_empty()
        && let Some(bytes) = icons.load_named(&notification.app_icon, ICON)
    {
        return Some(bytes.to_vec());
    }
    if !notification.app_name.trim().is_empty()
        && let Some(bytes) = icons.load(&notification.app_name, ICON)
    {
        return Some(bytes.to_vec());
    }
    None
}

#[must_use]
pub fn wrap_lines(
    painter: &crate::paint::Painter,
    text: &str,
    style: &glide_theme::TextStyle,
    max_width: i32,
    max_lines: usize,
) -> Vec<String> {
    if text.trim().is_empty() || max_lines == 0 || max_width <= 0 {
        return Vec::new();
    }

    let fits = |candidate: &str| {
        #[allow(clippy::cast_possible_truncation)]
        let width = painter.text_width_styled(candidate, style).ceil() as i32;
        width <= max_width
    };

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };

        if fits(&candidate) {
            current = candidate;
            continue;
        }

        if current.is_empty() {
            current = clip_word(word, &fits);
        } else {
            lines.push(std::mem::take(&mut current));
            if lines.len() == max_lines {
                break;
            }
            current = if fits(word) {
                word.to_string()
            } else {
                clip_word(word, &fits)
            };
        }
    }

    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }

    if lines.len() == max_lines {
        let remaining = text.split_whitespace().count();
        let used: usize = lines
            .iter()
            .map(|line| line.split_whitespace().count())
            .sum();
        if used < remaining {
            let last = lines.len() - 1;
            lines[last] = ellipsise(&lines[last], &fits);
        }
    }

    lines
}

fn clip_word(word: &str, fits: &impl Fn(&str) -> bool) -> String {
    let mut kept = String::new();
    for ch in word.chars() {
        let mut candidate = kept.clone();
        candidate.push(ch);
        if !fits(&candidate) {
            break;
        }
        kept = candidate;
    }
    if kept.is_empty() {
        word.chars().take(1).collect()
    } else {
        kept
    }
}

fn ellipsise(line: &str, fits: &impl Fn(&str) -> bool) -> String {
    let mut chars: Vec<char> = line.chars().collect();
    loop {
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if fits(&candidate) || chars.len() <= 1 {
            return candidate;
        }
        chars.pop();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    notifications: &mut Notifications,
    renderer: &mut R,
    painter: &crate::paint::Painter,
    output: Rectangle<i32, Logical>,
    scale: Scale<f64>,
    output_scale: i32,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    if !notifications.is_enabled() || notifications.live.is_empty() {
        return Vec::new();
    }

    notifications.invalidate_stale_buffers();
    notifications.retire_sent();
    if notifications.live.is_empty() {
        return Vec::new();
    }
    notifications.measure(painter);
    notifications.layout_slots(output);

    let origin = notifications.rect(output).loc;
    let mut elements = Vec::new();
    let mut y = origin.y;

    let theme = notifications.theme.clone();
    let hovered = notifications.hovered;

    let margin = shadow_margin();
    let mut shadows = Vec::new();

    for index in 0..notifications.live.len() {
        let (id, height, advance, hover, progress, peeks, phase) = {
            let entry = &notifications.live[index];
            let id = entry.notification.id;
            let hover = hovered.filter(|hit| hit.id() == id);
            (
                id,
                entry.height,
                occupied_height(entry),
                hover,
                enter_progress(entry),
                peek_layers(entry),
                entry.reply_phase(),
            )
        };

        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let shift = ((1.0 - progress) * ENTER_SHIFT as f32).round() as i32;
        let position = Point::from((origin.x + shift, y));
        let alpha = progress;

        let shadow_key = (id, height, output_scale);
        if let std::collections::hash_map::Entry::Vacant(slot) =
            notifications.shadows.entry(shadow_key)
            && let Some(buffer) = draw_shadow(height, margin, output_scale)
        {
            slot.insert(buffer);
        }
        if let Some(buffer) = notifications.shadows.get(&shadow_key) {
            let at = Point::from((position.x - margin, position.y - margin));
            if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                at.to_f64().to_physical(scale),
                buffer,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            ) {
                shadows.push(element);
            }
        }

        push_peeks(
            notifications,
            renderer,
            &theme,
            PeekRun {
                id,
                peeks,
                position,
                height,
                alpha,
            },
            scale,
            output_scale,
            &mut shadows,
        );

        let key = (id, hover, phase, output_scale);
        if !notifications.buffers.contains_key(&key) {
            let entry = &notifications.live[index];
            if let Some(buffer) = draw_toast(painter, &theme, entry, hover, phase, output_scale) {
                notifications.buffers.insert(key, buffer);
            }
        }

        if let Some(buffer) = notifications.buffers.get(&key)
            && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                position.to_f64().to_physical(scale),
                buffer,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            )
        {
            elements.push(element);
        }

        y += advance + RAIL_GAP;
    }

    elements.append(&mut shadows);
    elements
}

#[derive(Clone, Copy)]
struct PeekRun {
    id: u32,
    peeks: usize,
    position: Point<i32, Logical>,
    height: i32,
    alpha: f32,
}

#[allow(clippy::too_many_arguments)]
fn push_peeks<R>(
    notifications: &mut Notifications,
    renderer: &mut R,
    theme: &NotificationTheme,
    run: PeekRun,
    scale: Scale<f64>,
    output_scale: i32,
    elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: Send + Clone + 'static,
{
    for layer in (1..=run.peeks).rev() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let step = layer as i32;
        let key = (run.id, step, output_scale);
        if let std::collections::hash_map::Entry::Vacant(slot) = notifications.peeks.entry(key)
            && let Some(buffer) = draw_peek(theme, step, output_scale)
        {
            slot.insert(buffer);
        }
        let Some(buffer) = notifications.peeks.get(&key) else {
            continue;
        };
        let at = Point::from((
            run.position.x + PEEK_INSET * step,
            run.position.y + run.height - PEEK_BODY + PEEK_REVEAL * step,
        ));
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            at.to_f64().to_physical(scale),
            buffer,
            Some(run.alpha),
            None,
            None,
            Kind::Unspecified,
        ) {
            elements.push(element);
        }
    }
}

fn draw_peek(theme: &NotificationTheme, step: i32, scale: i32) -> Option<MemoryRenderBuffer> {
    let width = WIDTH - PEEK_INSET * step * 2;
    let height = PEEK_BODY;
    let pixels = usize::try_from(width * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        width,
        height,
        scale,
        theme.background,
        RADIUS,
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
        RADIUS,
        theme.border,
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (width * scale, height * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

fn draw_shadow(height: i32, margin: i32, scale: i32) -> Option<MemoryRenderBuffer> {
    let (w, h) = (WIDTH + margin * 2, height + margin * 2);
    let (pw, ph) = (w * scale, h * scale);
    let mut buf = vec![0u8; usize::try_from(pw * ph * 4).ok()?];

    #[allow(clippy::cast_precision_loss)]
    let scale_f = scale as f32;
    let layers: Vec<_> = shadow_layers()
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
        WIDTH as f32 * scale_f,
        height as f32 * scale_f,
        &layers,
        0.0,
        Color([0.0, 0.0, 0.0, 0.0]),
        RADIUS * scale_f,
        Some(RADIUS * scale_f),
    );

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (pw, ph),
        scale,
        Transform::Normal,
        None,
    ))
}

fn draw_toast(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    entry: &Entry,
    hovered: Option<Hit>,
    phase: ReplyPhase,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    let id = entry.notification.id;
    let dismiss_hover = hovered == Some(Hit::Dismiss(id));
    let height = entry.height;
    let pixels = usize::try_from(WIDTH * scale * height * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];

    crate::paint::Painter::fill_panel_background(
        &mut buf,
        WIDTH,
        height,
        scale,
        theme.background,
        RADIUS,
        [true; 4],
    );
    crate::paint::stroke_rounded_rect_over(
        &mut buf,
        WIDTH,
        height,
        0,
        0,
        WIDTH,
        height,
        scale,
        1.0,
        RADIUS,
        theme.border,
    );

    if entry.notification.urgency == Urgency::Critical {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            WIDTH,
            height,
            0,
            0,
            3,
            height,
            scale,
            theme.danger,
            0.0,
        );
    }

    let icon_top = PADDING;
    if let Some(icon) = entry.icon.as_deref() {
        crate::paint::blit_square_over(
            &mut buf, WIDTH, height, icon, ICON, PADDING, icon_top, scale,
        );
    } else {
        crate::paint::Painter::fill_rounded_rect_over(
            &mut buf,
            WIDTH,
            height,
            PADDING,
            icon_top,
            ICON,
            ICON,
            scale,
            theme.fill,
            ICON_RADIUS,
        );
    }

    draw_toast_text(painter, theme, entry, &mut buf, height, scale);
    draw_toast_buttons(
        painter, theme, entry, &mut buf, height, hovered, phase, scale,
    );
    draw_toast_dismiss(theme, &mut buf, height, dismiss_hover, scale);

    Some(MemoryRenderBuffer::from_slice(
        &buf,
        smithay::backend::allocator::Fourcc::Argb8888,
        (WIDTH * scale, height * scale),
        scale,
        Transform::Normal,
        None,
    ))
}

fn draw_toast_text(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    entry: &Entry,
    buf: &mut [u8],
    height: i32,
    scale: i32,
) {
    let text_left = PADDING + ICON + ICON_GAP;
    let text_right = WIDTH - PADDING;
    let mut y = PADDING;

    draw_meta_row(
        painter, theme, entry, buf, height, text_left, text_right, y, scale,
    );
    y += META_HEIGHT;

    let title_color = if entry.notification.urgency == Urgency::Critical {
        theme.danger
    } else {
        theme.title
    };
    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        text_left,
        text_right,
        y,
        y + TITLE_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        &entry.notification.summary,
        title_color.to_buffer_bytes(),
        &glide_theme::TextStyle::HEAD,
    );
    y += TITLE_HEIGHT;

    for line in entry.body_lines.iter().flatten() {
        painter.draw_label_over_styled_in_band(
            buf,
            WIDTH,
            height,
            text_left,
            text_right,
            y,
            y + BODY_LINE,
            scale,
            crate::paint::TextAlign::Left,
            line,
            theme.body.to_buffer_bytes(),
            &glide_theme::TextStyle::CALL,
        );
        y += BODY_LINE;
    }

    if let Some(progress) = entry.notification.progress {
        draw_toast_progress(theme, buf, height, y + PROGRESS_GAP, progress, scale);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_meta_row(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    entry: &Entry,
    buf: &mut [u8],
    height: i32,
    text_left: i32,
    text_right: i32,
    y: i32,
    scale: i32,
) {
    let meta = meta_line(entry);
    let badge = count_badge(entry);
    let measure = |text: &str| {
        #[allow(clippy::cast_possible_truncation)]
        let width = painter
            .text_width_styled(text, &glide_theme::TextStyle::CAP)
            .ceil() as i32;
        width
    };
    let badge_width = badge
        .as_ref()
        .map_or(0, |text| measure(text) + COUNT_PAD_X * 2);
    let meta_right = text_right - DISMISS_SIDE - badge_width;

    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        text_left,
        meta_right,
        y,
        y + META_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        &meta,
        theme.meta.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );

    let Some(text) = badge.as_ref() else {
        return;
    };
    let badge_x = (text_left + measure(&meta) + glide_theme::spacing::SP2).min(meta_right);
    let badge_y = y + (META_HEIGHT - COUNT_HEIGHT) / 2;
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        WIDTH,
        height,
        badge_x,
        badge_y,
        badge_width,
        COUNT_HEIGHT,
        scale,
        theme.fill,
        COUNT_RADIUS,
    );
    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        badge_x,
        badge_x + badge_width,
        badge_y,
        badge_y + COUNT_HEIGHT,
        scale,
        crate::paint::TextAlign::Center,
        text,
        theme.body.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );
}

fn draw_toast_progress(
    theme: &NotificationTheme,
    buf: &mut [u8],
    height: i32,
    y: i32,
    progress: f32,
    scale: i32,
) {
    let left = PADDING + ICON + ICON_GAP;
    let track = WIDTH - PADDING - left;
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        WIDTH,
        height,
        left,
        y,
        track,
        PROGRESS_HEIGHT,
        scale,
        theme.fill,
        glide_theme::shape::R_ROUND,
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let filled = ((track as f32) * progress.clamp(0.0, 1.0)).round() as i32;
    if filled > 0 {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            WIDTH,
            height,
            left,
            y,
            filled,
            PROGRESS_HEIGHT,
            scale,
            theme.accent,
            glide_theme::shape::R_ROUND,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_toast_buttons(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    entry: &Entry,
    buf: &mut [u8],
    height: i32,
    hovered: Option<Hit>,
    phase: ReplyPhase,
    scale: i32,
) {
    match phase {
        ReplyPhase::Sent => {
            draw_reply_confirmation(painter, theme, buf, height, scale);
            return;
        }
        ReplyPhase::Composing | ReplyPhase::Invalid => {
            draw_reply_composer(painter, theme, entry, buf, height, hovered, phase, scale);
            return;
        }
        ReplyPhase::Closed => {}
    }
    if entry.buttons.is_empty() && entry.reply_button.is_none() && entry.settings_button.is_none() {
        return;
    }
    let id = entry.notification.id;
    let labels = visible_buttons(&entry.notification);
    let top = height - PADDING - BUTTON_HEIGHT;
    let mut x = PADDING + ICON + ICON_GAP;

    if let (Some(width), Some(reply)) = (entry.reply_button, entry.notification.reply.as_ref()) {
        draw_chip(
            painter,
            buf,
            height,
            x,
            top,
            width,
            &reply.label,
            theme.accent,
            theme.accent_label,
            hovered == Some(Hit::Reply(id)),
            theme.hover,
            scale,
        );
        x += width + BUTTON_GAP;
    }

    for (index, width) in entry.buttons.iter().enumerate() {
        let Some(action) = labels.get(index) else {
            break;
        };
        let is_primary = index == 0 && entry.reply_button.is_none();
        let is_hovered = hovered == Some(Hit::Button(id, index));

        let (background, label) = if is_primary {
            (theme.accent, theme.accent_label)
        } else if is_hovered {
            (theme.hover, theme.title)
        } else {
            (theme.fill, theme.title)
        };

        draw_chip(
            painter,
            buf,
            height,
            x,
            top,
            *width,
            &action.label,
            background,
            label,
            is_primary && is_hovered,
            theme.hover,
            scale,
        );

        x += width + BUTTON_GAP;
    }

    if let Some(width) = entry.settings_button {
        draw_chip(
            painter,
            buf,
            height,
            x,
            top,
            width,
            crate::i18n::t(crate::i18n::Key::Configure),
            theme.fill,
            theme.title,
            hovered == Some(Hit::Settings(id)),
            theme.hover,
            scale,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_chip(
    painter: &crate::paint::Painter,
    buf: &mut [u8],
    height: i32,
    x: i32,
    top: i32,
    width: i32,
    label: &str,
    background: Color,
    foreground: Color,
    overlay: bool,
    overlay_color: Color,
    scale: i32,
) {
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        WIDTH,
        height,
        x,
        top,
        width,
        BUTTON_HEIGHT,
        scale,
        background,
        BUTTON_RADIUS,
    );
    if overlay {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            WIDTH,
            height,
            x,
            top,
            width,
            BUTTON_HEIGHT,
            scale,
            overlay_color,
            BUTTON_RADIUS,
        );
    }
    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        x,
        x + width,
        top,
        top + BUTTON_HEIGHT,
        scale,
        crate::paint::TextAlign::Center,
        label,
        foreground.to_buffer_bytes(),
        &glide_theme::TextStyle::LAUNCHER_CHIP,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_reply_composer(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    entry: &Entry,
    buf: &mut [u8],
    height: i32,
    hovered: Option<Hit>,
    phase: ReplyPhase,
    scale: i32,
) {
    let Some(text) = entry.reply_text.as_ref() else {
        return;
    };
    let id = entry.notification.id;
    let left = PADDING + ICON + ICON_GAP;
    let right = WIDTH - PADDING;
    let top = height - PADDING - REPLY_HEIGHT;
    let field_width = right - left;

    let (ring, border) = if phase == ReplyPhase::Invalid {
        (
            theme
                .reply_invalid_border
                .with_alpha(theme.accent_ring.0[3]),
            theme.reply_invalid_border,
        )
    } else {
        (theme.accent_ring, theme.accent)
    };
    draw_field_frame(
        theme,
        buf,
        height,
        FieldFrame {
            left,
            top,
            width: field_width,
            ring,
            border,
        },
        scale,
    );

    let send_x = right - REPLY_SEND_INSET - REPLY_SEND;
    let text_left = left + REPLY_PAD_LEFT;
    let text_right = send_x - glide_theme::spacing::SP2;

    let showing_placeholder = text.is_empty();
    let shown = if showing_placeholder {
        entry
            .notification
            .reply
            .as_ref()
            .map(|reply| reply.placeholder.clone())
            .filter(|placeholder| !placeholder.trim().is_empty())
            .unwrap_or_else(|| "Reply".to_string())
    } else {
        clip_tail(painter, text, text_right - text_left)
    };
    let colour = if showing_placeholder {
        theme.meta
    } else {
        theme.title
    };
    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        text_left,
        text_right,
        top,
        top + REPLY_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        &shown,
        colour.to_buffer_bytes(),
        &glide_theme::TextStyle::LAUNCHER_CHIP,
    );

    if !showing_placeholder {
        #[allow(clippy::cast_possible_truncation)]
        let caret = text_left
            + painter
                .text_width_styled(&shown, &glide_theme::TextStyle::LAUNCHER_CHIP)
                .ceil() as i32
            + 1;
        if caret < text_right {
            crate::paint::Painter::fill_rounded_rect_over(
                buf,
                WIDTH,
                height,
                caret,
                top + (REPLY_HEIGHT - 15) / 2,
                2,
                15,
                scale,
                theme.accent,
                1.0,
            );
        }
    }

    draw_send_button(
        painter,
        theme,
        buf,
        height,
        send_x,
        top + (REPLY_HEIGHT - REPLY_SEND) / 2,
        hovered == Some(Hit::Send(id)),
        scale,
    );
}

#[derive(Clone, Copy)]
struct FieldFrame {
    left: i32,
    top: i32,
    width: i32,
    ring: Color,
    border: Color,
}

fn draw_field_frame(
    theme: &NotificationTheme,
    buf: &mut [u8],
    height: i32,
    frame: FieldFrame,
    scale: i32,
) {
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        WIDTH,
        height,
        frame.left - REPLY_RING,
        frame.top - REPLY_RING,
        frame.width + REPLY_RING * 2,
        REPLY_HEIGHT + REPLY_RING * 2,
        scale,
        frame.ring,
        BUTTON_RADIUS + f32::from(u8::try_from(REPLY_RING).unwrap_or_default()),
    );
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        WIDTH,
        height,
        frame.left,
        frame.top,
        frame.width,
        REPLY_HEIGHT,
        scale,
        theme.field,
        BUTTON_RADIUS,
    );
    crate::paint::stroke_rounded_rect_over(
        buf,
        WIDTH,
        height,
        frame.left,
        frame.top,
        frame.width,
        REPLY_HEIGHT,
        scale,
        1.0,
        BUTTON_RADIUS,
        frame.border,
    );
}

fn draw_reply_confirmation(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    buf: &mut [u8],
    height: i32,
    scale: i32,
) {
    let left = PADDING + ICON + ICON_GAP;
    let right = WIDTH - PADDING;
    let top = height - PADDING - REPLY_HEIGHT;
    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        left,
        right,
        top,
        top + REPLY_HEIGHT,
        scale,
        crate::paint::TextAlign::Left,
        crate::i18n::t(crate::i18n::Key::Sent),
        theme.meta.to_buffer_bytes(),
        &glide_theme::TextStyle::CAP,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_send_button(
    painter: &crate::paint::Painter,
    theme: &NotificationTheme,
    buf: &mut [u8],
    height: i32,
    x: i32,
    top: i32,
    hovered: bool,
    scale: i32,
) {
    crate::paint::Painter::fill_rounded_rect_over(
        buf,
        WIDTH,
        height,
        x,
        top,
        REPLY_SEND,
        REPLY_SEND,
        scale,
        theme.accent,
        BUTTON_RADIUS,
    );
    if hovered {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            WIDTH,
            height,
            x,
            top,
            REPLY_SEND,
            REPLY_SEND,
            scale,
            theme.hover,
            BUTTON_RADIUS,
        );
    }
    painter.draw_label_over_styled_in_band(
        buf,
        WIDTH,
        height,
        x,
        x + REPLY_SEND,
        top,
        top + REPLY_SEND,
        scale,
        crate::paint::TextAlign::Center,
        "\u{2191}",
        theme.accent_label.to_buffer_bytes(),
        &glide_theme::TextStyle::LAUNCHER_CHIP,
    );
}

fn clip_tail(painter: &crate::paint::Painter, text: &str, max_width: i32) -> String {
    let style = glide_theme::TextStyle::LAUNCHER_CHIP;
    #[allow(clippy::cast_possible_truncation)]
    let fits =
        |candidate: &str| painter.text_width_styled(candidate, &style).ceil() as i32 <= max_width;
    if fits(text) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let candidate: String = chars[start..].iter().collect();
        if fits(&candidate) {
            return candidate;
        }
        start += 1;
    }
    String::new()
}

fn draw_toast_dismiss(
    theme: &NotificationTheme,
    buf: &mut [u8],
    height: i32,
    hovered: bool,
    scale: i32,
) {
    let dismiss_x = WIDTH - DISMISS_INSET - DISMISS_SIDE;
    if hovered {
        crate::paint::Painter::fill_rounded_rect_over(
            buf,
            WIDTH,
            height,
            dismiss_x,
            DISMISS_INSET,
            DISMISS_SIDE,
            DISMISS_SIDE,
            scale,
            theme.hover,
            glide_theme::shape::R_ROUND,
        );
    }
    if let Some(glyph) = draw_dismiss_glyph(theme, scale) {
        crate::paint::blit_square_over(
            buf,
            WIDTH,
            height,
            &glyph,
            DISMISS_SIDE,
            dismiss_x,
            DISMISS_INSET,
            scale,
        );
    }
}

fn draw_dismiss_glyph(theme: &NotificationTheme, scale: i32) -> Option<Vec<u8>> {
    let pixels = usize::try_from(DISMISS_SIDE * scale * DISMISS_SIDE * scale * 4).ok()?;
    let mut buf = vec![0u8; pixels];
    crate::paint::draw_close_glyph_over(
        &mut buf,
        DISMISS_SIDE,
        DISMISS_SIDE,
        DISMISS_GLYPH,
        DISMISS_STROKE,
        scale,
        theme.meta,
    );
    Some(buf)
}

fn meta_line(entry: &Entry) -> String {
    let app = entry.notification.display_app();
    let age = entry.shown_at.elapsed();
    let when = if age < Duration::from_secs(60) {
        "now".to_string()
    } else {
        format!("{} min", age.as_secs() / 60)
    };
    let origin = entry.notification.origin.trim();
    if origin.is_empty() {
        format!("{app} · {when}")
    } else {
        format!("{app} · {origin} · {when}")
    }
}

fn count_badge(entry: &Entry) -> Option<String> {
    (!entry.stacked.is_empty()).then(|| format!("+{}", entry.stacked.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painter() -> crate::paint::Painter {
        crate::paint::Painter::new(14.0, None)
    }

    fn output() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), Size::from((1920, 1080)))
    }

    fn notification(id: u32) -> Notification {
        Notification {
            id,
            app_name: format!("Mail {id}"),
            summary: format!("New message {id}"),
            body: "A short body".to_string(),
            timeout: Some(Duration::from_millis(50)),
            ..Notification::default()
        }
    }

    fn stacked_ids(entry: &Entry) -> Vec<u32> {
        entry
            .stacked
            .iter()
            .map(|notification| notification.id)
            .collect()
    }

    fn from_app(id: u32, app: &str) -> Notification {
        Notification {
            app_name: app.to_string(),
            ..notification(id)
        }
    }

    fn repliable(id: u32) -> Notification {
        Notification {
            sender: ":1.42".to_string(),
            reply: Some(glide_notify::Reply {
                key: glide_notify::ACTION_INLINE_REPLY.to_string(),
                label: "Reply".to_string(),
                placeholder: "Message…".to_string(),
            }),
            timeout: None,
            ..notification(id)
        }
    }

    fn store() -> (Notifications, crate::icon::IconLoader) {
        (
            Notifications::new(NotificationTheme::default()),
            crate::icon::IconLoader::new(None),
        )
    }

    fn settle_entry(store: &mut Notifications) {
        if let Some(settled) = Instant::now().checked_sub(glide_theme::motion::t4()) {
            store.live[0].shown_at = settled;
        }
        assert!(
            !store.is_animating(),
            "a settled toast has nothing left to animate"
        );
    }

    #[test]
    fn the_rail_hugs_the_top_right_corner_below_the_topbar() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        let rect = store.rect(output());
        assert_eq!(rect.loc.x + rect.size.w, 1920 - RAIL_INSET);
        assert_eq!(rect.loc.y, crate::topbar::HEIGHT + RAIL_GAP);
        assert_eq!(rect.size.w, WIDTH);
    }

    #[test]
    fn an_empty_rail_claims_no_input() {
        let (store, _) = store();
        assert!(!store.contains(output(), Point::from((1800.0, 60.0))));
    }

    #[test]
    fn the_newest_notification_sits_at_the_top_of_the_rail() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);
        assert_eq!(store.live[0].notification.id, 2);
        assert_eq!(store.live[1].notification.id, 1);
    }

    #[test]
    fn the_rail_never_grows_past_its_visible_budget() {
        let (mut store, mut icons) = store();
        let overflow = u32::try_from(MAX_VISIBLE).expect("the budget fits a u32") + 3;
        for id in 1..=overflow {
            store.push(&mut icons, notification(id), false);
        }
        assert_eq!(store.unread(), MAX_VISIBLE);
    }

    #[test]
    fn replacing_a_notification_keeps_its_place_in_the_rail() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);

        let mut updated = notification(1);
        updated.summary = "Edited".to_string();
        store.push(&mut icons, updated, true);

        assert_eq!(store.live[1].notification.id, 1);
        assert_eq!(store.live[1].notification.summary, "Edited");
        assert_eq!(store.unread(), 2);
    }

    #[test]
    fn do_not_disturb_drops_everything_except_critical_notifications() {
        let (mut store, mut icons) = store();
        store.set_dnd(true);

        assert!(!store.push(&mut icons, notification(1), false));

        let mut urgent = notification(2);
        urgent.urgency = Urgency::Critical;
        urgent.timeout = None;
        assert!(store.push(&mut icons, urgent, false));
        assert_eq!(store.unread(), 1);
    }

    #[test]
    fn an_application_inhibitor_suppresses_exactly_like_do_not_disturb() {
        let (mut store, mut icons) = store();
        store.set_inhibited(true);
        assert!(store.suppressed());
        assert!(!store.push(&mut icons, notification(1), false));
    }

    #[test]
    fn a_notification_expires_once_its_timeout_elapses() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        assert!(!store.tick());
        std::thread::sleep(Duration::from_millis(80));
        assert!(store.tick());
        assert!(store.is_empty());
    }

    #[test]
    fn a_notification_without_a_timeout_never_expires_on_its_own() {
        let (mut store, mut icons) = store();
        let mut sticky = notification(1);
        sticky.timeout = None;
        store.push(&mut icons, sticky, false);
        std::thread::sleep(Duration::from_millis(30));
        assert!(!store.tick());
        assert_eq!(store.unread(), 1);
        assert_eq!(store.next_deadline(), None);
    }

    #[test]
    fn the_next_deadline_is_the_soonest_one_still_pending() {
        let (mut store, mut icons) = store();
        let mut slow = notification(1);
        slow.timeout = Some(Duration::from_secs(30));
        store.push(&mut icons, slow, false);
        store.push(&mut icons, notification(2), false);

        let deadline = store.next_deadline().expect("a deadline is pending");
        assert!(deadline <= Duration::from_millis(50));
    }

    #[test]
    fn dismissing_a_notification_removes_it_and_its_cached_pixels() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        assert!(store.remove(1, CloseReason::Dismissed, false));
        assert!(store.is_empty());
        assert!(store.buffers.is_empty());
        assert!(!store.remove(1, CloseReason::Dismissed, false));
    }

    #[test]
    fn clearing_the_rail_reports_whether_anything_was_there() {
        let (mut store, mut icons) = store();
        assert!(!store.clear_all());
        store.push(&mut icons, notification(1), false);
        assert!(store.clear_all());
        assert!(store.is_empty());
    }

    #[test]
    fn the_dismiss_target_sits_inside_the_toast_it_belongs_to() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());

        let body = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Body(1))
            .map(|(_, rect)| *rect)
            .expect("body slot");
        let dismiss = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Dismiss(1))
            .map(|(_, rect)| *rect)
            .expect("dismiss slot");

        assert!(dismiss.loc.x >= body.loc.x);
        assert!(dismiss.loc.x + dismiss.size.w <= body.loc.x + body.size.w);
        assert!(dismiss.loc.y >= body.loc.y);
        assert!(dismiss.loc.y + dismiss.size.h <= body.loc.y + body.size.h);
    }

    #[test]
    fn the_dismiss_target_wins_over_the_body_underneath_it() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());

        let dismiss = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Dismiss(1))
            .map(|(_, rect)| *rect)
            .expect("dismiss slot");
        let centre = Point::from((
            f64::from(dismiss.loc.x + dismiss.size.w / 2),
            f64::from(dismiss.loc.y + dismiss.size.h / 2),
        ));
        assert_eq!(
            store.action_at(centre),
            Some(NotificationAction::Dismiss(1))
        );
    }

    #[test]
    fn clicking_the_body_activates_rather_than_dismisses() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());

        let body = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Body(1))
            .map(|(_, rect)| *rect)
            .expect("body slot");
        let point = Point::from((
            f64::from(body.loc.x + PADDING),
            f64::from(body.loc.y + body.size.h - 2),
        ));
        assert_eq!(
            store.action_at(point),
            Some(NotificationAction::Activate(1))
        );
    }

    #[test]
    fn hover_only_reports_a_change_when_the_target_changes() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());

        let dismiss = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Dismiss(1))
            .map(|(_, rect)| *rect)
            .expect("dismiss slot");
        let centre = Point::from((
            f64::from(dismiss.loc.x + dismiss.size.w / 2),
            f64::from(dismiss.loc.y + dismiss.size.h / 2),
        ));

        assert!(store.set_hover(Some(centre)));
        assert!(!store.set_hover(Some(centre)));
        assert!(store.set_hover(None));
        assert!(!store.set_hover(None));
    }

    #[test]
    fn a_taller_body_makes_a_taller_toast() {
        let painter = painter();
        let short = notification(1);
        let short_lines: Vec<String> = wrap_lines(
            &painter,
            &short.body,
            &glide_theme::TextStyle::CALL,
            body_width(),
            BODY_MAX_LINES,
        );

        let mut long = notification(2);
        long.body = "word ".repeat(60);
        let long_lines = wrap_lines(
            &painter,
            &long.body,
            &glide_theme::TextStyle::CALL,
            body_width(),
            BODY_MAX_LINES,
        );

        assert!(long_lines.len() > short_lines.len());
        assert!(
            entry_height(&long, &long_lines, false) > entry_height(&short, &short_lines, false)
        );
    }

    #[test]
    fn a_progress_notification_reserves_room_for_its_bar() {
        let plain = notification(1);
        let mut with_bar = notification(2);
        with_bar.progress = Some(0.5);
        let lines = vec!["one".to_string()];
        assert!(entry_height(&with_bar, &lines, false) > entry_height(&plain, &lines, false));
    }

    #[test]
    fn a_toast_is_never_shorter_than_its_icon() {
        let mut bare = notification(1);
        bare.body = String::new();
        assert!(entry_height(&bare, &[], false) >= PADDING * 2 + ICON);
    }

    #[test]
    fn wrapping_never_exceeds_the_line_budget() {
        let painter = painter();
        let lines = wrap_lines(
            &painter,
            &"lorem ipsum dolor sit amet ".repeat(40),
            &glide_theme::TextStyle::CALL,
            body_width(),
            BODY_MAX_LINES,
        );
        assert_eq!(lines.len(), BODY_MAX_LINES);
        assert!(lines.last().is_some_and(|line| line.ends_with('…')));
    }

    #[test]
    fn wrapping_keeps_every_line_inside_the_measured_width() {
        let painter = painter();
        let lines = wrap_lines(
            &painter,
            "the quick brown fox jumps over the lazy dog again and again",
            &glide_theme::TextStyle::CALL,
            body_width(),
            BODY_MAX_LINES,
        );
        for line in &lines {
            #[allow(clippy::cast_possible_truncation)]
            let width = painter
                .text_width_styled(line, &glide_theme::TextStyle::CALL)
                .ceil() as i32;
            assert!(width <= body_width(), "{line:?} overflows the toast");
        }
    }

    #[test]
    fn a_body_that_fits_is_left_on_one_line_without_an_ellipsis() {
        let painter = painter();
        let lines = wrap_lines(
            &painter,
            "short",
            &glide_theme::TextStyle::CALL,
            body_width(),
            BODY_MAX_LINES,
        );
        assert_eq!(lines, vec!["short".to_string()]);
    }

    #[test]
    fn an_unbreakable_word_is_clipped_instead_of_overflowing() {
        let painter = painter();
        let lines = wrap_lines(
            &painter,
            &"x".repeat(400),
            &glide_theme::TextStyle::CALL,
            body_width(),
            1,
        );
        assert_eq!(lines.len(), 1);
        #[allow(clippy::cast_possible_truncation)]
        let width = painter
            .text_width_styled(&lines[0], &glide_theme::TextStyle::CALL)
            .ceil() as i32;
        assert!(width <= body_width());
    }

    #[test]
    fn an_empty_body_produces_no_lines_at_all() {
        let painter = painter();
        assert!(
            wrap_lines(
                &painter,
                "   ",
                &glide_theme::TextStyle::CALL,
                body_width(),
                BODY_MAX_LINES
            )
            .is_empty()
        );
    }

    #[test]
    fn the_meta_line_names_the_application_and_how_long_ago_it_spoke() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        let meta = meta_line(&store.live[0]);
        assert!(meta.starts_with("Mail"));
        assert!(meta.ends_with("now"));
    }

    #[test]
    fn a_web_notification_shows_the_origin_between_app_and_time() {
        let (mut store, mut icons) = store();
        let mut web = notification(1);
        web.origin = "example.com".to_string();
        store.push(&mut icons, web, false);
        assert!(meta_line(&store.live[0]).contains("example.com"));
    }

    #[test]
    fn a_disabled_rail_accepts_nothing() {
        let mut store = Notifications::new(NotificationTheme {
            enabled: false,
            ..NotificationTheme::default()
        });
        let mut icons = crate::icon::IconLoader::new(None);
        assert!(!store.push(&mut icons, notification(1), false));
        assert!(store.is_empty());
    }

    #[test]
    fn only_a_notification_that_offers_a_reply_opens_a_composer() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        assert!(!store.open_reply(1));
        assert_eq!(store.reply_target(), None);

        store.push(&mut icons, repliable(2), false);
        assert!(store.open_reply(2));
        assert_eq!(store.reply_target(), Some(2));
    }

    #[test]
    fn opening_one_composer_closes_any_other() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.push(&mut icons, repliable(2), false);

        store.open_reply(1);
        assert_eq!(store.reply_target(), Some(1));
        store.open_reply(2);
        assert_eq!(store.reply_target(), Some(2));
    }

    #[test]
    fn typing_only_lands_while_a_composer_is_open() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        assert!(!store.type_reply('a'), "no composer, no text");

        store.open_reply(1);
        assert!(store.type_reply('h'));
        assert!(store.type_reply('i'));
        assert_eq!(store.live[0].reply_text.as_deref(), Some("hi"));
    }

    #[test]
    fn backspace_walks_the_draft_back_and_stops_at_empty() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.type_reply('x');

        assert!(store.backspace_reply());
        assert_eq!(store.live[0].reply_text.as_deref(), Some(""));
        assert!(!store.backspace_reply(), "an empty draft cannot shrink");
    }

    #[test]
    fn a_draft_cannot_grow_without_bound() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        for _ in 0..(REPLY_MAX_CHARS + 20) {
            store.type_reply('z');
        }
        let length = store.live[0]
            .reply_text
            .as_ref()
            .map_or(0, |text| text.chars().count());
        assert_eq!(length, REPLY_MAX_CHARS);
    }

    #[test]
    fn escape_abandons_the_draft_but_keeps_the_notification() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.type_reply('a');

        assert!(store.cancel_reply());
        assert_eq!(store.reply_target(), None);
        assert_eq!(store.unread(), 1, "cancelling is not dismissing");
        assert!(!store.cancel_reply());
    }

    #[test]
    fn a_successful_send_schedules_the_close_behind_a_confirmation() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.type_reply('o');
        store.type_reply('k');

        assert!(store.send_reply());
        assert_eq!(
            store.unread(),
            1,
            "the toast lingers to show the confirmation"
        );
        assert_eq!(
            store.reply_target(),
            None,
            "the keyboard is released at once"
        );
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Sent);
        assert!(
            store.is_animating(),
            "the pump must run until the close fires"
        );
        assert!(
            store
                .next_deadline()
                .is_some_and(|deadline| deadline <= glide_theme::motion::t4()),
            "the close is due no later than one t4 after sending"
        );
        assert!(!store.tick(), "the close does not fire early");

        store.measure(&painter());
        store.layout_slots(output());
        assert!(
            !store.slots.iter().any(|(hit, _)| matches!(
                hit,
                Hit::Reply(1) | Hit::Send(1) | Hit::Button(1, _) | Hit::Settings(1)
            )),
            "no reply or action target is offered while the confirmation shows"
        );

        store.live[0].reply_sent_at = Instant::now().checked_sub(glide_theme::motion::t4());
        assert!(store.tick());
        assert!(store.is_empty());
    }

    #[test]
    fn sending_an_empty_draft_keeps_the_composer_open_and_flags_it_invalid() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);

        assert!(store.send_reply());
        assert_eq!(store.unread(), 1, "nothing was said, nothing was sent");
        assert_eq!(store.reply_target(), Some(1), "the composer stays open");
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Invalid);
        assert!(store.live[0].reply_invalid_since.is_some());
        assert!(
            store.is_animating(),
            "the pump must run while the field is flagged"
        );

        store.type_reply(' ');
        assert!(store.send_reply(), "whitespace is still an empty reply");
        assert_eq!(store.reply_target(), Some(1));
    }

    #[test]
    fn the_invalid_flag_expires_after_one_t3() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        settle_entry(&mut store);
        store.open_reply(1);
        store.send_reply();
        assert!(store.live[0].reply_invalid());
        assert!(
            store.is_animating(),
            "the flag alone keeps the pump running"
        );

        store.live[0].reply_invalid_since = Instant::now().checked_sub(glide_theme::motion::t3());
        assert!(!store.live[0].reply_invalid());
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Composing);
        store.live[0].shown_at = Instant::now()
            .checked_sub(glide_theme::motion::T4)
            .expect("the monotonic clock has run for longer than one t4");
        assert!(!store.is_animating());
        assert_eq!(
            store.reply_target(),
            Some(1),
            "expiry reverts the border, not the draft"
        );
    }

    #[test]
    fn cancelling_clears_the_invalid_flag_with_the_draft() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        settle_entry(&mut store);
        store.open_reply(1);
        store.send_reply();
        assert!(store.is_animating());

        assert!(store.cancel_reply());
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Closed);
        store.live[0].shown_at = Instant::now()
            .checked_sub(glide_theme::motion::T4)
            .expect("the monotonic clock has run for longer than one t4");
        assert!(!store.is_animating());
    }

    #[test]
    fn the_confirmation_keeps_the_composer_row_height_until_it_closes() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.measure(&painter());
        let composing = store.live[0].height;

        store.type_reply('x');
        store.send_reply();
        store.measure(&painter());
        assert_eq!(store.live[0].height, composing);
        assert!(
            !store.open_reply(1),
            "a sent toast does not reopen its composer"
        );
    }

    #[test]
    fn the_invalid_border_takes_the_danger_colour_and_changes_the_revision() {
        let theme = NotificationTheme::default();
        assert_eq!(theme.reply_invalid_border, theme.danger);
        let recoloured = NotificationTheme {
            reply_invalid_border: Color([1.0, 0.0, 0.0, 1.0]),
            ..NotificationTheme::default()
        };
        assert_ne!(theme.revision(), recoloured.revision());
    }

    #[test]
    fn an_open_composer_makes_the_toast_taller_than_its_button_row() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.measure(&painter());
        let closed = store.live[0].height;

        store.open_reply(1);
        store.measure(&painter());
        assert!(store.live[0].height > closed);
    }

    #[test]
    fn the_composer_offers_a_send_target_of_its_own() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.measure(&painter());
        store.layout_slots(output());

        let send = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Send(1))
            .map(|(_, rect)| *rect)
            .expect("send slot");
        let centre = Point::from((
            f64::from(send.loc.x + send.size.w / 2),
            f64::from(send.loc.y + send.size.h / 2),
        ));
        assert_eq!(
            store.action_at(centre),
            Some(NotificationAction::SendReply(1))
        );
    }

    #[test]
    fn a_repliable_notification_shows_a_reply_target_before_its_other_buttons() {
        let (mut store, mut icons) = store();
        let mut with_both = repliable(1);
        with_both.actions = vec![glide_notify::Action {
            key: "archive".to_string(),
            label: "Archive".to_string(),
        }];
        store.push(&mut icons, with_both, false);
        store.measure(&painter());
        store.layout_slots(output());

        let reply = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Reply(1))
            .map(|(_, rect)| *rect)
            .expect("reply chip");
        let archive = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Button(1, 0))
            .map(|(_, rect)| *rect)
            .expect("action chip");
        assert!(reply.loc.x + reply.size.w <= archive.loc.x);
    }

    #[test]
    fn a_chatty_application_collapses_into_one_stacked_toast() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.push(&mut icons, from_app(3, "Telegram"), false);

        assert_eq!(store.live.len(), 1, "one toast per application");
        assert_eq!(store.live[0].notification.id, 3, "the newest leads");
        assert_eq!(stacked_ids(&store.live[0]), vec![2, 1]);
        assert_eq!(store.unread(), 3, "the badge still counts every message");
    }

    #[test]
    fn different_applications_keep_their_own_toasts() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Mail"), false);
        assert_eq!(store.live.len(), 2);
    }

    #[test]
    fn grouping_prefers_the_desktop_entry_over_the_display_name() {
        let (mut store, mut icons) = store();
        let mut first = from_app(1, "Telegram");
        first.desktop_entry = "org.telegram.desktop".to_string();
        let mut second = from_app(2, "Telegram Desktop");
        second.desktop_entry = "org.telegram.desktop".to_string();

        store.push(&mut icons, first, false);
        store.push(&mut icons, second, false);
        assert_eq!(store.live.len(), 1);
    }

    #[test]
    fn a_stack_tells_the_reader_how_many_are_hidden() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        assert_eq!(
            count_badge(&store.live[0]),
            None,
            "a lone toast counts nothing"
        );

        store.push(&mut icons, from_app(2, "Telegram"), false);
        assert_eq!(count_badge(&store.live[0]).as_deref(), Some("+1"));

        store.push(&mut icons, from_app(3, "Telegram"), false);
        assert_eq!(count_badge(&store.live[0]).as_deref(), Some("+2"));
    }

    #[test]
    fn a_stack_reserves_the_room_its_peeks_need() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.measure(&painter());
        let lone = occupied_height(&store.live[0]);
        assert_eq!(
            lone, store.live[0].height,
            "nothing peeks out from one toast"
        );

        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.measure(&painter());
        assert_eq!(
            occupied_height(&store.live[0]),
            store.live[0].height + PEEK_REVEAL
        );
    }

    #[test]
    fn peeks_never_grow_past_the_two_that_are_drawn() {
        let (mut store, mut icons) = store();
        for id in 1..=6 {
            store.push(&mut icons, from_app(id, "Telegram"), false);
        }
        store.measure(&painter());
        assert_eq!(peek_layers(&store.live[0]), PEEK_MAX);
        assert_eq!(
            occupied_height(&store.live[0]),
            store.live[0].height + PEEK_REVEAL * 2
        );
    }

    #[test]
    fn a_stack_never_overlaps_the_toast_beneath_it() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.push(&mut icons, from_app(3, "Mail"), false);
        store.measure(&painter());
        store.layout_slots(output());

        let first = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Body(3))
            .map(|(_, rect)| *rect)
            .expect("mail toast");
        let second = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Body(2))
            .map(|(_, rect)| *rect)
            .expect("telegram toast");

        let stacked_first = store
            .live
            .iter()
            .find(|entry| entry.notification.id == 3)
            .map(peek_layers)
            .unwrap_or_default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let reveal = PEEK_REVEAL * stacked_first as i32;
        assert!(
            first.loc.y + first.size.h + reveal <= second.loc.y,
            "the peek of one toast must not reach the next"
        );
    }

    #[test]
    fn no_two_toasts_ever_share_a_pixel_row() {
        let (mut store, mut icons) = store();
        for (id, app) in [(1, "Mail"), (2, "Telegram"), (3, "Files"), (4, "Calendar")] {
            let mut item = from_app(id, app);
            item.body = "a body long enough to wrap onto more than one line ".repeat(id as usize);
            store.push(&mut icons, item, false);
        }
        store.measure(&painter());
        store.layout_slots(output());

        let mut bodies: Vec<Rectangle<i32, Logical>> = store
            .slots
            .iter()
            .filter(|(hit, _)| matches!(hit, Hit::Body(_)))
            .map(|(_, rect)| *rect)
            .collect();
        bodies.sort_by_key(|rect| rect.loc.y);

        assert_eq!(bodies.len(), 4, "four applications, four toasts");
        for pair in bodies.windows(2) {
            assert!(
                pair[0].loc.y + pair[0].size.h <= pair[1].loc.y,
                "toast at y={} h={} runs into the one at y={}",
                pair[0].loc.y,
                pair[0].size.h,
                pair[1].loc.y
            );
        }
    }

    #[test]
    fn the_rail_rect_encloses_every_toast_it_lays_out() {
        let (mut store, mut icons) = store();
        for (id, app) in [(1, "Mail"), (2, "Telegram"), (3, "Files")] {
            store.push(&mut icons, from_app(id, app), false);
        }
        store.measure(&painter());
        store.layout_slots(output());

        let rail = store.rect(output());
        for (hit, rect) in &store.slots {
            if !matches!(hit, Hit::Body(_)) {
                continue;
            }
            assert!(rect.loc.y >= rail.loc.y);
            assert!(
                rect.loc.y + rect.size.h <= rail.loc.y + rail.size.h,
                "a toast falls outside the rail the pointer is tested against"
            );
        }
    }

    #[test]
    fn a_toast_buffer_is_exactly_as_tall_as_the_row_reserved_for_it() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Mail"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.measure(&painter());
        store.layout_slots(output());

        for entry in &store.live {
            let body = store
                .slots
                .iter()
                .find(|(hit, _)| *hit == Hit::Body(entry.notification.id))
                .map(|(_, rect)| *rect)
                .expect("body slot");
            assert_eq!(
                body.size.h, entry.height,
                "the hit box and the drawn buffer must be the same height"
            );
        }
    }

    #[test]
    fn the_centre_reports_a_group_as_one_row_carrying_its_count() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.push(&mut icons, from_app(3, "Mail"), false);

        let items = store.nc_items();
        assert_eq!(items.len(), 2, "one row per application");
        let telegram = items
            .iter()
            .find(|item| item.app_name == "Telegram")
            .expect("telegram row");
        assert_eq!(telegram.count, 2);
        let mail = items
            .iter()
            .find(|item| item.app_name == "Mail")
            .expect("mail row");
        assert_eq!(mail.count, 1);
    }

    #[test]
    fn a_stack_never_retains_more_than_its_budget() {
        let (mut store, mut icons) = store();
        let overflow = u32::try_from(MAX_STACKED).expect("the budget fits a u32") + 4;
        for id in 1..=overflow {
            store.push(&mut icons, from_app(id, "Telegram"), false);
        }
        assert_eq!(store.live.len(), 1);
        assert_eq!(store.live[0].stacked.len(), MAX_STACKED);
    }

    #[test]
    fn closing_the_top_of_a_stack_reveals_the_one_beneath_it() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.push(&mut icons, from_app(3, "Telegram"), false);
        assert_eq!(store.live[0].notification.id, 3);

        assert!(store.remove(3, CloseReason::Dismissed, false));
        assert_eq!(
            store.live[0].notification.id, 2,
            "the previous message must become readable, not vanish"
        );
        assert_eq!(store.unread(), 2);

        assert!(store.remove(2, CloseReason::Dismissed, false));
        assert_eq!(store.live[0].notification.id, 1);
        assert_eq!(store.unread(), 1);

        assert!(store.remove(1, CloseReason::Dismissed, false));
        assert!(store.is_empty(), "only the last one empties the rail");
    }

    #[test]
    fn a_promoted_message_is_measured_afresh() {
        let (mut store, mut icons) = store();
        let mut first = from_app(1, "Telegram");
        first.body = "a body long enough to wrap over several lines ".repeat(4);
        store.push(&mut icons, first, false);

        let mut second = from_app(2, "Telegram");
        second.body = "short".to_string();
        store.push(&mut icons, second, false);

        store.measure(&painter());
        let short_height = store.live[0].height;

        store.remove(2, CloseReason::Dismissed, false);
        store.measure(&painter());
        assert!(
            store.live[0].height > short_height,
            "the taller message must get its own height back"
        );
    }

    #[test]
    fn a_promoted_message_gets_its_own_time_to_be_read() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        std::thread::sleep(Duration::from_millis(40));
        store.push(&mut icons, from_app(2, "Telegram"), false);

        store.remove(2, CloseReason::Dismissed, false);
        assert!(
            store.live[0].shown_at.elapsed() < Duration::from_millis(20),
            "the revealed message must not inherit the elapsed time of the one above it"
        );
    }

    #[test]
    fn expiry_walks_down_the_stack_rather_than_dropping_it() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);

        std::thread::sleep(Duration::from_millis(80));
        assert!(store.tick());
        assert_eq!(store.live[0].notification.id, 1);
        assert_eq!(store.unread(), 1);
    }

    #[test]
    fn closing_a_buried_message_leaves_the_visible_one_alone() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);

        assert!(store.remove(1, CloseReason::Revoked, false));
        assert_eq!(store.live[0].notification.id, 2);
        assert_eq!(store.unread(), 1);
        assert!(store.live[0].stacked.is_empty());
    }

    #[test]
    fn clearing_the_rail_still_closes_every_buried_message() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.push(&mut icons, from_app(3, "Mail"), false);

        assert!(store.clear_all());
        assert!(store.is_empty());
        assert_eq!(store.unread(), 0);
    }

    #[test]
    fn a_repeat_of_the_same_message_is_swallowed() {
        let (mut store, mut icons) = store();
        assert!(store.push(&mut icons, from_app(1, "Backup"), false));

        let mut repeat = from_app(2, "Backup");
        repeat.summary = store.live[0].notification.summary.clone();
        assert!(
            !store.push(&mut icons, repeat, false),
            "an identical body and summary is not news"
        );
        assert_eq!(store.unread(), 1);
    }

    #[test]
    fn replacing_a_message_keeps_the_rest_of_its_stack() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);

        let mut edited = from_app(2, "Telegram");
        edited.summary = "Edited".to_string();
        store.push(&mut icons, edited, true);

        assert_eq!(store.live.len(), 1);
        assert_eq!(store.live[0].notification.summary, "Edited");
        assert_eq!(stacked_ids(&store.live[0]), vec![1]);
    }

    #[test]
    fn a_configurable_notification_offers_a_configure_target_last() {
        let (mut store, mut icons) = store();
        let mut configurable = notification(1);
        configurable.has_settings_action = true;
        configurable.actions = vec![glide_notify::Action {
            key: "open".to_string(),
            label: "Open".to_string(),
        }];
        store.push(&mut icons, configurable, false);
        store.measure(&painter());
        store.layout_slots(output());

        let open = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Button(1, 0))
            .map(|(_, rect)| *rect)
            .expect("action chip");
        let settings = store
            .slots
            .iter()
            .find(|(hit, _)| *hit == Hit::Settings(1))
            .map(|(_, rect)| *rect)
            .expect("configure chip");
        assert!(open.loc.x + open.size.w <= settings.loc.x);
    }

    #[test]
    fn a_notification_without_a_settings_action_shows_no_configure_chip() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());
        assert!(!store.slots.iter().any(|(hit, _)| *hit == Hit::Settings(1)));
    }

    #[test]
    fn dismissing_the_notification_being_replied_to_clears_the_composer() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.remove(1, CloseReason::Dismissed, false);
        assert_eq!(store.reply_target(), None);
    }
}
