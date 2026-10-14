use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use glide_notify::{CloseReason, Notification, Urgency};
use glide_theme::motion::{ease_io, ease_out};
use glide_ui::{Reveal, Tween};

use crate::{
    config::Color,
    paint::TextAlign,
    popup_menu::text_px,
    raster::{Canvas, Rect, ShellRenderer, rect},
};
use glide_theme::TextStyle;

pub const WIDTH: i32 = 360;
const RAIL_GAP: i32 = glide_theme::spacing::SP2;
const RAIL_INSET: i32 = glide_theme::spacing::SP3;
const PADDING: i32 = glide_theme::spacing::SP3;
const RADIUS: f32 = glide_theme::shape::R3;

const ICON: i32 = 32;
pub const ICON_SOURCE_PX: i32 = ICON * 3;
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
    pub icon: Option<std::sync::Arc<Vec<u8>>>,
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
    Glyph,
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

const fn narrow(value: f64) -> f32 {
    value as f32
}

#[derive(Debug)]
struct Entry {
    notification: Notification,
    shown_at: Instant,
    updated_at: Instant,
    drawn_when: String,
    icon: Option<std::sync::Arc<Vec<u8>>>,
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
    expiry: Expiry,
    hover: Tween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Expiry {
    started: Instant,
    paused_at: Option<Instant>,
    paused: Duration,
}

impl Expiry {
    const fn new(now: Instant) -> Self {
        Self { started: now, paused_at: None, paused: Duration::ZERO }
    }

    fn restarted(self, now: Instant) -> Self {
        let mut next = Self::new(now);
        if self.is_paused() {
            next.pause(now);
        }
        next
    }

    const fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    fn pause(&mut self, now: Instant) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    fn resume(&mut self, now: Instant) {
        if let Some(at) = self.paused_at.take() {
            self.paused += now.saturating_duration_since(at);
        }
    }

    fn elapsed(&self, now: Instant) -> Duration {
        self.paused_at.unwrap_or(now).saturating_duration_since(self.started).saturating_sub(self.paused)
    }

    fn expired(&self, timeout: Duration, now: Instant) -> bool {
        !self.is_paused() && self.elapsed(now) >= timeout
    }
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
        if self.reply_invalid() { ReplyPhase::Invalid } else { ReplyPhase::Composing }
    }

    fn reply_invalid(&self) -> bool {
        self.reply_invalid_since.is_some_and(|since| since.elapsed() < glide_theme::motion::t3())
    }

    fn reply_confirmation_elapsed(&self) -> bool {
        self.reply_sent_at.is_some_and(|sent| sent.elapsed() >= glide_theme::motion::t4())
    }

    fn reply_row_open(&self) -> bool {
        self.reply_text.is_some() || self.reply_sent_at.is_some()
    }

    fn leaving(&self) -> bool {
        !self.reveal.is_open()
    }

    fn leave_progress(&self) -> f32 {
        narrow(self.reveal.progress(glide_theme::motion::t2(), ease_io).clamp(0.0, 1.0))
    }

    fn leave_scale(&self) -> f64 {
        LEAVE_SCALE + (1.0 - LEAVE_SCALE) * f64::from(self.leave_progress())
    }

    fn drawn_offset(&self, target: i32) -> i32 {
        self.y.map_or(target, |tween| tween.value(glide_theme::motion::t3(), ease_out).round() as i32)
    }

    fn lay_out_at(&mut self, target: i32) -> i32 {
        let wanted = f64::from(target);
        match self.y.as_mut() {
            None => {
                self.y = Some(Tween::at(wanted));
            }
            Some(tween) => {
                tween.retarget(wanted, glide_theme::motion::t3(), ease_out);
            }
        }
        self.drawn_offset(target)
    }

    fn ring_alpha(&self) -> f32 {
        narrow(self.ring.value(glide_theme::motion::t1(), ease_io))
    }

    fn close_composer(&mut self) -> bool {
        if self.reply_text.take().is_none() {
            return false;
        }
        self.reply_invalid_since = None;
        self.set_ring(false);
        true
    }

    fn set_ring(&mut self, on: bool) {
        if on {
            self.ring.retarget(1.0, glide_theme::motion::t1(), ease_io);
        } else {
            self.ring.jump(0.0);
        }
    }

    fn sync_peeks(&mut self) {
        let duration = glide_theme::motion::t2();
        let wanted = peek_layers(self);
        while self.peeks.len() < wanted {
            let mut reveal = Reveal::closed();
            reveal.open(duration);
            self.peeks.push(reveal);
        }
        for (index, reveal) in self.peeks.iter_mut().enumerate() {
            if index < wanted {
                reveal.open(duration);
            } else {
                reveal.close(duration);
            }
            reveal.settle(duration);
        }
        self.peeks.retain(|reveal| reveal.is_open() || reveal.is_visible(duration));
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
            || self.hover.is_animating(t2)
    }

    fn hover_alpha(&self) -> f32 {
        narrow(self.hover.value(glide_theme::motion::t2(), ease_io).clamp(0.0, 1.0))
    }

    fn set_hovered(&mut self, hovered: bool, now: Instant) {
        let target = if hovered { 1.0 } else { 0.0 };
        if (self.hover.target() - target).abs() > f64::EPSILON {
            self.hover.retarget(target, glide_theme::motion::t2(), ease_io);
        }
        if hovered {
            self.expiry.pause(now);
        } else {
            self.expiry.resume(now);
        }
    }

    fn owns(&self, id: u32) -> bool {
        self.notification.id == id || self.stacked.iter().any(|notification| notification.id == id)
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
        self.updated_at = self.shown_at;
        self.expiry = self.expiry.restarted(self.shown_at);
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
    pub appearance: glide_theme::Appearance,
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
            .bool(self.appearance == glide_theme::Appearance::Dark)
            .finish()
    }
}

impl Default for NotificationTheme {
    fn default() -> Self {
        Self::from_tokens(&glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue))
    }
}

impl NotificationTheme {
    #[must_use]
    pub fn from_tokens(t: &glide_theme::Tokens) -> Self {
        Self {
            enabled: true,
            background: t.popover_bg,
            border: t.line,
            title: t.fg_1,
            body: t.fg_2,
            meta: t.fg_3,
            fill: t.fill_1,
            accent: t.acc_bg,
            accent_label: t.acc_fg,
            accent_ring: t.acc_ring,
            field: t.view_bg,
            danger: t.danger,
            reply_invalid_border: t.danger,
            hover: t.state.with_alpha(glide_theme::motion::ST_HOVER),
            appearance: t.appearance,
        }
    }
}

const MAX_ARCHIVED: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BannerPosition {
    #[default]
    Right,
    Center,
    Left,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPolicy {
    pub do_not_disturb: bool,
    pub schedule: Option<(u16, u16)>,
    pub allow_critical: bool,
    pub banner_timeout: Option<Duration>,
    pub position: BannerPosition,
    pub muted: Vec<String>,
    pub sounds: bool,
}

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            do_not_disturb: false,
            schedule: None,
            allow_critical: true,
            banner_timeout: None,
            position: BannerPosition::default(),
            muted: Vec::new(),
            sounds: true,
        }
    }
}

impl NotificationPolicy {
    #[must_use]
    pub fn mutes(&self, notification: &Notification) -> bool {
        let names = [notification.desktop_entry.trim(), notification.app_name.trim()];
        self.muted.iter().any(|muted| {
            let muted = muted.trim();
            !muted.is_empty()
                && names.iter().any(|name| {
                    !name.is_empty()
                        && (name.eq_ignore_ascii_case(muted)
                            || name.strip_suffix(".desktop").is_some_and(|stem| stem.eq_ignore_ascii_case(muted))
                            || muted.strip_suffix(".desktop").is_some_and(|stem| stem.eq_ignore_ascii_case(name)))
                })
        })
    }

    #[must_use]
    pub fn wants_dnd(&self, minute_of_day: u16) -> Option<bool> {
        let (from, to) = self.schedule?;
        Some(if from <= to { (from..to).contains(&minute_of_day) } else { minute_of_day >= from || minute_of_day < to })
    }

    fn shape(&self, mut notification: Notification) -> Notification {
        if notification.urgency != Urgency::Critical
            && notification.timeout.is_some()
            && let Some(timeout) = self.banner_timeout
        {
            notification.timeout = Some(timeout);
        }
        notification
    }

    const fn slide_direction(&self) -> i32 {
        match self.position {
            BannerPosition::Right => 1,
            BannerPosition::Center => 0,
            BannerPosition::Left => -1,
        }
    }
}

#[derive(Debug, Clone)]
struct Archived {
    notification: Notification,
    received_at: Instant,
    icon: Option<std::sync::Arc<Vec<u8>>>,
    read: bool,
}

pub const LOCAL_ID_FLOOR: u32 = u32::MAX - 4096;

#[must_use]
pub const fn is_local(id: u32) -> bool {
    id >= LOCAL_ID_FLOOR
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalEvent {
    Action { id: u32, key: String },
    Reply { id: u32, text: String },
    Closed { id: u32 },
}

#[derive(Debug, Default)]
pub struct Notifications {
    theme: NotificationTheme,
    handle: Option<glide_notify::NotifyHandle>,
    local: Vec<LocalEvent>,
    live: Vec<Entry>,
    archive: Vec<Archived>,
    dnd: bool,
    inhibited: bool,
    policy: NotificationPolicy,
    policy_applied: bool,
    scheduled_dnd: Option<bool>,
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
        Self { theme, ..Self::default() }
    }

    pub fn set_theme(&mut self, theme: &NotificationTheme) {
        let next = NotificationTheme { enabled: self.theme.enabled, ..theme.clone() };
        if self.theme != next {
            self.theme = next;
            self.invalidate();
        }
    }

    pub fn attach(&mut self, handle: glide_notify::NotifyHandle) {
        self.handle = Some(handle);
    }

    pub fn take_local_events(&mut self) -> Vec<LocalEvent> {
        std::mem::take(&mut self.local)
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

    #[must_use]
    pub const fn policy(&self) -> &NotificationPolicy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: NotificationPolicy) {
        if !self.policy_applied || policy.do_not_disturb != self.policy.do_not_disturb {
            self.dnd = policy.do_not_disturb;
        }
        if policy.schedule != self.policy.schedule {
            self.scheduled_dnd = None;
        }
        self.policy_applied = true;
        self.policy = policy;
    }

    #[must_use]
    pub fn audible(&self, notification: &Notification) -> Option<glide_notify::Sound> {
        let wanted = self.is_enabled() && self.policy.sounds && notification.sound != glide_notify::Sound::None;
        (wanted && !self.silences(notification)).then(|| notification.sound.clone())
    }

    #[must_use]
    pub fn silences(&self, notification: &Notification) -> bool {
        let breaks_through = notification.urgency == Urgency::Critical && self.policy.allow_critical;
        (self.suppressed() && !breaks_through) || self.policy.mutes(notification)
    }

    pub fn follow_schedule(&mut self, minute_of_day: Option<u16>) -> bool {
        let wanted = minute_of_day.and_then(|minute| self.policy.wants_dnd(minute));
        if wanted == self.scheduled_dnd {
            return false;
        }
        self.scheduled_dnd = wanted;
        match wanted {
            Some(on) if self.dnd != on => {
                self.dnd = on;
                true
            }
            _ => false,
        }
    }

    fn active(&self) -> impl Iterator<Item = &Entry> {
        self.live.iter().filter(|entry| !entry.leaving())
    }

    fn active_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
        self.live.iter_mut().filter(|entry| !entry.leaving())
    }

    #[must_use]
    pub fn unread(&self) -> usize {
        let live: usize = self.active().map(Entry::group_size).sum();
        let archived = self.archive.iter().filter(|item| !item.read).count();
        live + archived
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active().next().is_none() && self.archive.is_empty()
    }

    #[must_use]
    pub fn nc_items(&self) -> Vec<NcItem> {
        let now = Instant::now();
        let item = |notification: &Notification, at: Instant, icon: &Option<std::sync::Arc<Vec<u8>>>, count| {
            let elapsed = now.duration_since(at).as_secs();
            NcItem {
                id: notification.id,
                app_name: notification.app_name.clone(),
                title: notification.summary.clone(),
                body: notification.body.clone(),
                time: crate::i18n::elapsed_label(elapsed),
                icon: icon.clone(),
                count,
            }
        };
        self.active()
            .map(|entry| item(&entry.notification, entry.updated_at, &entry.icon, entry.group_size()))
            .chain(self.archive.iter().map(|kept| item(&kept.notification, kept.received_at, &kept.icon, 1)))
            .collect()
    }

    pub fn mark_read(&mut self) -> bool {
        let mut changed = false;
        for item in &mut self.archive {
            changed |= !item.read;
            item.read = true;
        }
        changed
    }

    fn keep(&mut self, notification: Notification, received_at: Instant, icon: Option<std::sync::Arc<Vec<u8>>>) {
        if notification.transient {
            return;
        }
        self.archive.retain(|kept| kept.notification.id != notification.id);
        self.archive.insert(0, Archived { notification, received_at, icon, read: false });
        self.archive.truncate(MAX_ARCHIVED);
    }

    fn keep_entry(&mut self, index: usize) {
        let entry = &self.live[index];
        let received_at = entry.updated_at;
        let icon = entry.icon.clone();
        let notifications: Vec<Notification> =
            std::iter::once(entry.notification.clone()).chain(entry.stacked.iter().cloned()).collect();
        for notification in notifications.into_iter().rev() {
            self.keep(notification, received_at, icon.clone());
        }
    }

    fn forget_kept(&mut self, id: u32) -> bool {
        let before = self.archive.len();
        self.archive.retain(|kept| kept.notification.id != id);
        self.archive.len() != before
    }

    fn invalidate(&mut self) {
        self.buffers.clear();
        self.shadows.clear();
        self.peeks.clear();
        self.layers.clear();
        self.rings.clear();
    }

    fn invalidate_stale_buffers(&mut self) {
        let current = crate::paint::raster_revision(self.theme.revision());
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
                entry.reply_button = entry.notification.reply.as_ref().map(|reply| chip_width(painter, &reply.label));
                entry.settings_button = entry
                    .notification
                    .has_settings_action
                    .then(|| chip_width(painter, crate::i18n::t(crate::i18n::Key::Configure)));
            }
            let lines = entry.body_lines.as_deref().unwrap_or(&[]);
            entry.height = entry_height(&entry.notification, lines, entry.reply_row_open());
            entry.sync_peeks();
        }
    }

    #[must_use]
    pub fn reply_target(&self) -> Option<u32> {
        self.active().find(|entry| entry.reply_text.is_some()).map(|entry| entry.notification.id)
    }

    pub fn open_reply(&mut self, id: u32) -> bool {
        let mut changed = false;
        for entry in self.active_mut() {
            let wants =
                entry.notification.id == id && entry.notification.reply.is_some() && entry.reply_sent_at.is_none();
            let open = entry.reply_text.is_some();
            if wants && !open {
                entry.reply_text = Some(String::new());
                entry.set_ring(true);
                changed = true;
            } else if !wants {
                changed |= entry.close_composer();
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
            changed |= entry.close_composer();
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
        self.active_mut().find_map(|entry| entry.reply_text.as_mut())
    }

    pub fn send_reply(&mut self) -> bool {
        let Some((id, sender, text)) = self.active().find_map(|entry| {
            entry
                .reply_text
                .as_ref()
                .map(|text| (entry.notification.id, entry.notification.sender.clone(), text.clone()))
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
        if is_local(id) {
            self.local.push(LocalEvent::Reply { id, text });
        } else if let Some(handle) = self.handle.as_ref() {
            handle.replied(id, &sender, &text);
        }
        if glide_theme::motion::t4().is_zero() {
            self.remove(id, CloseReason::Dismissed, true);
            return true;
        }
        for entry in self.active_mut() {
            if entry.notification.id == id {
                entry.close_composer();
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
        self.live.retain(|entry| !entry.leaving() || entry.reveal.is_visible(duration));
        if self.live.len() == before {
            return false;
        }
        let ids: std::collections::HashSet<u32> = self.live.iter().map(|entry| entry.notification.id).collect();
        self.buffers.retain(|(id, _, _), _| ids.contains(id));
        self.shadows.retain(|(id, _, _), _| ids.contains(id));
        self.peeks.retain(|(id, _, _), _| ids.contains(id));
        true
    }

    fn prune_layers(&mut self) {
        let duration = glide_theme::motion::t1();
        let settled = |tween: &mut Tween| tween.target() > 0.0 || tween.is_animating(duration);
        self.hover_layers.retain(|_, tween| settled(tween));
        self.press_layers.retain(|_, tween| settled(tween));
    }

    pub fn settle(&mut self) -> bool {
        self.retire_left();
        self.prune_layers();
        self.is_animating()
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
        self.retire_left();
    }

    fn leave_by_id(&mut self, id: u32) -> bool {
        let Some(index) = self.live.iter().position(|entry| !entry.leaving() && entry.notification.id == id) else {
            return false;
        };
        self.leave(index);
        true
    }

    fn evict_overflow(&mut self) {
        loop {
            let active: Vec<usize> =
                self.live.iter().enumerate().filter(|(_, entry)| !entry.leaving()).map(|(index, _)| index).collect();
            if active.len() <= MAX_VISIBLE {
                return;
            }
            let Some(&last) = active.last() else {
                return;
            };
            for id in self.live[last].ids() {
                self.notify_closed(id, CloseReason::Expired);
            }
            self.keep_entry(last);
            self.leave(last);
        }
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let t1 = glide_theme::motion::t1();
        self.live.iter().any(Entry::is_animating)
            || self.hover_layers.values().chain(self.press_layers.values()).any(|tween| tween.is_animating(t1))
    }

    pub fn push(&mut self, icons: &mut crate::icon::IconLoader, notification: Notification, replaces: bool) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let notification = self.policy.shape(notification);
        self.forget_kept(notification.id);
        if self.silences(&notification) {
            let icon = load_icon(icons, &notification);
            self.keep(notification, Instant::now(), icon);
            return true;
        }

        if !replaces && self.active().any(|entry| entry.notification.is_duplicate_of(&notification)) {
            self.notify_closed(notification.id, CloseReason::Expired);
            return false;
        }

        let id = notification.id;
        let group = notification.group_key();
        let icon = load_icon(icons, &notification);
        let now = Instant::now();
        let mut entry = Entry {
            notification,
            shown_at: now,
            updated_at: now,
            drawn_when: String::new(),
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
            reveal: Reveal::open_now(),
            y: None,
            peeks: Vec::new(),
            ring: Tween::at(0.0),
            expiry: Expiry::new(now),
            hover: Tween::at(0.0),
        };

        if replaces && let Some(slot) = self.live.iter_mut().find(|slot| !slot.leaving() && slot.owns(id)) {
            if slot.notification.id == id {
                entry.stacked = std::mem::take(&mut slot.stacked);
                entry.y = slot.y;
                entry.peeks = std::mem::take(&mut slot.peeks);
                entry.shown_at = slot.shown_at;
                entry.reveal = slot.reveal;
                entry.expiry = slot.expiry.restarted(now);
                entry.hover = slot.hover;
                if entry.notification.reply.is_some() {
                    entry.reply_text = slot.reply_text.take();
                    entry.reply_invalid_since = slot.reply_invalid_since;
                    entry.reply_sent_at = slot.reply_sent_at;
                    entry.ring = slot.ring;
                }
                *slot = entry;
            } else {
                for stacked in &mut slot.stacked {
                    if stacked.id == id {
                        *stacked = entry.notification.clone();
                    }
                }
            }
            self.buffers.retain(|(key, _, _), _| *key != id);
            return true;
        }

        self.live.retain(|slot| !slot.owns(id));
        self.buffers.retain(|(key, _, _), _| *key != id);

        if let Some(index) = self
            .live
            .iter()
            .position(|slot| !slot.leaving() && !group.is_empty() && slot.notification.group_key() == group)
        {
            let previous = self.live.remove(index);
            self.buffers.retain(|(key, _, _), _| *key != previous.notification.id);
            entry.stacked = std::iter::once(previous.notification)
                .chain(previous.stacked)
                .filter(|notification| notification.id != id)
                .collect();
            entry.y = previous.y;
            entry.peeks = previous.peeks;
            while entry.stacked.len() > MAX_STACKED {
                if let Some(dropped) = entry.stacked.pop() {
                    self.notify_closed(dropped.id, CloseReason::Expired);
                }
            }
            self.live.insert(0, entry);
            return true;
        }

        self.live.insert(0, entry);
        self.evict_overflow();
        true
    }

    pub fn remove(&mut self, id: u32, reason: CloseReason, tell_sender: bool) -> bool {
        let Some(index) = self.live.iter().position(|entry| !entry.leaving() && entry.owns(id)) else {
            return false;
        };

        let head = self.live[index].notification.id;
        if head != id {
            self.live[index].stacked.retain(|notification| notification.id != id);
            self.buffers.retain(|(key, _, _), _| *key != head);
        } else if self.live[index].promote() {
            self.forget_pointer_state(id);
            self.buffers.retain(|(key, _, _), _| *key != id);
        } else {
            self.leave(index);
        }
        if tell_sender {
            self.notify_closed(id, reason);
        }
        true
    }

    pub fn clear_all(&mut self) -> bool {
        if self.is_empty() {
            return false;
        }
        let ids: Vec<u32> = self.active().flat_map(Entry::ids).collect();
        for id in ids {
            self.notify_closed(id, CloseReason::Dismissed);
        }
        let heads: Vec<u32> = self.active().map(|entry| entry.notification.id).collect();
        for id in heads {
            self.leave_by_id(id);
        }
        self.archive.clear();
        true
    }

    pub fn dismiss(&mut self, id: u32) -> bool {
        let removed = self.remove(id, CloseReason::Dismissed, true);
        let forgotten = self.forget_kept(id);
        removed || forgotten
    }

    pub fn tick(&mut self) -> bool {
        let scheduled = self.policy.schedule.is_some() && self.follow_schedule(crate::topbar::local_minute_of_day());
        self.tick_toasts() || scheduled
    }

    fn tick_toasts(&mut self) -> bool {
        let now = Instant::now();
        let expired: Vec<u32> = self
            .active()
            .filter(|entry| entry.notification.timeout.is_some_and(|timeout| entry.expiry.expired(timeout, now)))
            .map(|entry| entry.notification.id)
            .collect();

        let mut changed = false;
        for id in expired {
            let kept = self
                .active()
                .find(|entry| entry.notification.id == id)
                .map(|entry| (entry.notification.clone(), entry.updated_at, entry.icon.clone()));
            if let Some((notification, at, icon)) = kept {
                self.keep(notification, at, icon);
            }
            changed |= self.remove(id, CloseReason::Expired, true);
        }
        let sent = self.retire_sent();
        let left = self.retire_left();
        changed || sent || left
    }

    fn notify_closed(&mut self, id: u32, reason: CloseReason) {
        if is_local(id) {
            self.local.push(LocalEvent::Closed { id });
        } else if let Some(handle) = self.handle.as_ref() {
            handle.closed(id, reason);
        }
    }

    pub fn press_button(&mut self, id: u32, index: usize, token: Option<&str>) {
        let Some(entry) = self.active().find(|entry| entry.notification.id == id) else {
            return;
        };
        let Some(action) = visible_buttons(&entry.notification).get(index) else {
            return;
        };
        let key = action.key.clone();
        let resident = entry.notification.resident;
        if is_local(id) {
            self.local.push(LocalEvent::Action { id, key });
        } else if let Some(handle) = self.handle.as_ref() {
            handle.invoked(id, &key, token);
        }
        if !resident {
            self.remove(id, CloseReason::Dismissed, true);
        }
    }

    pub fn configure(&mut self, id: u32, token: Option<&str>) {
        let Some(entry) = self.active().find(|entry| entry.owns(id)) else {
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
        let Some(entry) = self.active().find(|entry| entry.notification.id == id) else {
            self.activate_kept(id, token);
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

    fn activate_kept(&mut self, id: u32, token: Option<&str>) {
        let Some(kept) = self.archive.iter().find(|kept| kept.notification.id == id) else {
            return;
        };
        if let Some(handle) = self.handle.as_ref()
            && kept.notification.has_default_action
        {
            handle.invoked(id, glide_notify::ACTION_DEFAULT, token);
        }
        self.forget_kept(id);
    }

    #[must_use]
    pub fn rect(&self, output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        let total: i32 =
            self.active().map(|entry| occupied_height(entry) + RAIL_GAP).sum::<i32>().saturating_sub(RAIL_GAP).max(0);
        let x = match self.policy.position {
            BannerPosition::Right => output.loc.x + output.size.w - RAIL_INSET - WIDTH,
            BannerPosition::Center => output.loc.x + (output.size.w - WIDTH) / 2,
            BannerPosition::Left => output.loc.x + RAIL_INSET,
        };
        Rectangle::new(Point::from((x, output.loc.y + crate::topbar::HEIGHT + RAIL_GAP)), Size::from((WIDTH, total)))
    }

    #[must_use]
    pub fn contains(&self, output: Rectangle<i32, Logical>, pos: Point<f64, Logical>) -> bool {
        self.is_enabled() && !self.is_empty() && self.rect(output).to_f64().contains(pos)
    }

    #[must_use]
    pub fn action_at(&self, pos: Point<f64, Logical>) -> Option<NotificationAction> {
        self.hit_at(pos).map(Hit::action)
    }

    fn hit_at(&self, pos: Point<f64, Logical>) -> Option<Hit> {
        self.slots.iter().find(|(_, rect)| rect.to_f64().contains(pos)).map(|(hit, _)| *hit)
    }

    fn has_layer(&self, hit: Hit) -> bool {
        self.layer_slots.iter().any(|(slot, _, _)| *slot == hit)
    }

    pub fn set_hover(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        let hovered = pos.and_then(|pos| self.hit_at(pos));
        if self.hovered == hovered {
            return false;
        }
        self.hovered = hovered;
        self.retarget_hover_layer(hovered);
        self.sync_toast_hover(Instant::now());
        true
    }

    fn sync_toast_hover(&mut self, now: Instant) {
        let over = self.hovered.map(Hit::id);
        for entry in self.live.iter_mut().filter(|entry| !entry.leaving()) {
            let hovered = over.is_some_and(|id| entry.notification.id == id);
            entry.set_hovered(hovered, now);
        }
    }

    fn retarget_hover_layer(&mut self, target: Option<Hit>) {
        let target = target.filter(|hit| self.has_layer(*hit));
        let duration = glide_theme::motion::t1();
        for (hit, tween) in &mut self.hover_layers {
            if Some(*hit) != target {
                tween.retarget(0.0, duration, ease_io);
            }
        }
        if let Some(hit) = target {
            self.hover_layers.entry(hit).or_insert_with(|| Tween::at(0.0)).retarget(1.0, duration, ease_io);
        }
    }

    pub fn set_pressed(&mut self, pos: Option<Point<f64, Logical>>) -> bool {
        let pressed = pos.and_then(|pos| self.hit_at(pos)).filter(|hit| self.has_layer(*hit));
        if self.pressed == pressed {
            return false;
        }
        self.pressed = pressed;
        let duration = glide_theme::motion::t1();
        for tween in self.press_layers.values_mut() {
            tween.retarget(0.0, duration, ease_io);
        }
        if let Some(hit) = pressed {
            self.press_layers.entry(hit).or_insert_with(|| Tween::at(0.0)).jump(1.0);
        }
        true
    }

    fn hover_layer_alpha(&self, hit: Hit) -> f32 {
        layer_alpha(&self.hover_layers, hit)
    }

    fn press_layer_alpha(&self, hit: Hit) -> f32 {
        layer_alpha(&self.press_layers, hit)
    }

    fn layout_slots(&mut self, output: Rectangle<i32, Logical>) {
        let origin = self.rect(output).loc;
        let mut offset = 0;
        let mut slots = Vec::new();
        let mut layer_slots = Vec::new();

        for entry in &mut self.live {
            if entry.leaving() {
                if entry.y.is_none() {
                    entry.y = Some(Tween::at(f64::from(offset)));
                }
                continue;
            }
            let y = origin.y + entry.lay_out_at(offset);
            let id = entry.notification.id;
            let dismiss = Rectangle::new(
                Point::from((origin.x + WIDTH - DISMISS_INSET - DISMISS_SIDE, y + DISMISS_INSET)),
                Size::from((DISMISS_SIDE, DISMISS_SIDE)),
            );
            slots.push((Hit::Dismiss(id), dismiss));
            layer_slots.push((Hit::Dismiss(id), dismiss, LayerShape::Dismiss));

            if entry.reply_text.is_some() {
                let top = y + entry.height - PADDING - REPLY_HEIGHT;
                let left = origin.x + PADDING + ICON + ICON_GAP;
                let send = Rectangle::new(
                    Point::from((
                        origin.x + WIDTH - PADDING - REPLY_SEND_INSET - REPLY_SEND,
                        top + (REPLY_HEIGHT - REPLY_SEND) / 2,
                    )),
                    Size::from((REPLY_SEND, REPLY_SEND)),
                );
                slots.push((Hit::Send(id), send));
                layer_slots.push((Hit::Send(id), send, LayerShape::Send));
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
                let mut chips: Vec<(Hit, i32)> = Vec::new();
                if let Some(width) = entry.reply_button {
                    chips.push((Hit::Reply(id), width));
                }
                for (index, width) in entry.buttons.iter().enumerate() {
                    chips.push((Hit::Button(id, index), *width));
                }
                if let Some(width) = entry.settings_button {
                    chips.push((Hit::Settings(id), width));
                }
                for (hit, width) in chips {
                    let rect = Rectangle::new(Point::from((x, top)), Size::from((width, BUTTON_HEIGHT)));
                    slots.push((hit, rect));
                    layer_slots.push((hit, rect, LayerShape::Chip(width)));
                    x += width + BUTTON_GAP;
                }
            }

            slots.push((Hit::Body(id), Rectangle::new(Point::from((origin.x, y)), Size::from((WIDTH, entry.height)))));
            offset += occupied_height(entry) + RAIL_GAP;
        }

        self.slots = slots;
        self.layer_slots = layer_slots;
    }
}

fn layer_alpha(layers: &std::collections::HashMap<Hit, Tween>, hit: Hit) -> f32 {
    narrow(layers.get(&hit).map_or(0.0, |tween| tween.value(glide_theme::motion::t1(), ease_io)))
}

const fn body_width() -> i32 {
    WIDTH - PADDING * 2 - ICON - ICON_GAP
}

fn peek_layers(entry: &Entry) -> usize {
    entry.stacked.len().min(PEEK_MAX)
}

fn occupied_height(entry: &Entry) -> i32 {
    let layers = peek_layers(entry) as i32;
    entry.height + PEEK_REVEAL * layers
}

fn entry_height(notification: &Notification, body_lines: &[String], replying: bool) -> i32 {
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
    text_px(painter, label, &glide_theme::TextStyle::LAUNCHER_CHIP) + BUTTON_PAD_X * 2
}

fn button_widths(painter: &crate::paint::Painter, notification: &Notification) -> Vec<i32> {
    visible_buttons(notification).iter().map(|action| chip_width(painter, &action.label)).collect()
}

fn shadow_margin() -> i32 {
    let reach = [glide_theme::Appearance::Light, glide_theme::Appearance::Dark]
        .into_iter()
        .flat_map(shadow_layers)
        .map(|layer| layer.blur + layer.dy)
        .fold(0.0_f32, f32::max);
    reach.ceil() as i32 + glide_theme::spacing::SP1
}

fn shadow_layers(appearance: glide_theme::Appearance) -> Vec<glide_theme::elevation::ShadowLayer> {
    glide_theme::elevation::layers(glide_theme::elevation::Level::El3, appearance)
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

fn load_icon(icons: &mut crate::icon::IconLoader, notification: &Notification) -> Option<std::sync::Arc<Vec<u8>>> {
    load_icon_bytes(icons, notification).map(std::sync::Arc::new)
}

fn load_icon_bytes(icons: &mut crate::icon::IconLoader, notification: &Notification) -> Option<Vec<u8>> {
    if !notification.desktop_entry.trim().is_empty()
        && let Some(bytes) = icons.load(&notification.desktop_entry, ICON_SOURCE_PX)
    {
        return Some(bytes.to_vec());
    }
    if !notification.app_icon.trim().is_empty()
        && let Some(bytes) = icons.load_named(&notification.app_icon, ICON_SOURCE_PX)
    {
        return Some(bytes.to_vec());
    }
    if !notification.app_name.trim().is_empty()
        && let Some(bytes) = icons.load(&notification.app_name, ICON_SOURCE_PX)
    {
        return Some(bytes.to_vec());
    }
    None
}

#[must_use]
pub fn wrap_lines(
    painter: &crate::paint::Painter, text: &str, style: &glide_theme::TextStyle, max_width: i32, max_lines: usize,
) -> Vec<String> {
    if text.trim().is_empty() || max_lines == 0 || max_width <= 0 {
        return Vec::new();
    }

    let fits = |candidate: &str| text_px(painter, candidate, style) <= max_width;

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };

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
            current = if fits(word) { word.to_string() } else { clip_word(word, &fits) };
        }
    }

    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }

    if lines.len() == max_lines {
        let remaining = text.split_whitespace().count();
        let used: usize = lines.iter().map(|line| line.split_whitespace().count()).sum();
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
    if kept.is_empty() { word.chars().take(1).collect() } else { kept }
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

pub fn render<R>(
    notifications: &mut Notifications, renderer: &mut R, painter: &crate::paint::Painter,
    output: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
) -> Vec<MemoryRenderBufferRenderElement<R>>
where
    R: ShellRenderer,
{
    if !notifications.is_enabled() || notifications.live.is_empty() {
        return Vec::new();
    }

    notifications.invalidate_stale_buffers();
    notifications.retire_sent();
    notifications.retire_left();
    notifications.prune_layers();
    if notifications.live.is_empty() {
        return Vec::new();
    }
    notifications.measure(painter);
    notifications.layout_slots(output);

    let origin = notifications.rect(output).loc;
    let direction = notifications.policy.slide_direction();
    let mut elements = Vec::new();

    let theme = notifications.theme.clone();
    let margin = shadow_margin();
    let mut shadows = Vec::new();

    for index in 0..notifications.live.len() {
        let (id, height, phase, run) = {
            let entry = &notifications.live[index];
            let enter = enter_progress(entry);
            let shift = ((1.0 - enter) * ENTER_SHIFT as f32).round() as i32;
            let position = Point::from((origin.x + shift * direction, origin.y + entry.drawn_offset(0)));
            (
                entry.notification.id,
                entry.height,
                entry.reply_phase(),
                Placement {
                    position,
                    height: entry.height,
                    factor: entry.leave_scale(),
                    alpha: enter * entry.leave_progress(),
                    scale,
                },
            )
        };

        let shadow_key = (id, height, output_scale);
        if let std::collections::hash_map::Entry::Vacant(slot) = notifications.shadows.entry(shadow_key)
            && let Some(buffer) = draw_shadow(height, margin, output_scale, theme.appearance)
        {
            slot.insert(buffer);
        }
        if let Some(buffer) = notifications.shadows.get(&shadow_key)
            && let Some(element) = run.element(
                renderer,
                buffer,
                Point::from((-margin, -margin)),
                (WIDTH + margin * 2, height + margin * 2),
                1.0,
            )
        {
            shadows.push(element);
        }

        push_peeks(notifications, renderer, &theme, index, run, output_scale, &mut shadows);

        push_layers(notifications, renderer, &theme, index, run, output_scale, &mut elements);

        let key = (id, phase, output_scale);
        let when = elapsed_label(&notifications.live[index]);
        if notifications.live[index].drawn_when != when {
            notifications.buffers.retain(|(key, _, _), _| *key != id);
            notifications.live[index].drawn_when = when;
        }
        if !notifications.buffers.contains_key(&key) {
            let entry = &notifications.live[index];
            if let Some(buffer) = draw_toast(painter, &theme, entry, phase, output_scale) {
                notifications.buffers.insert(key, buffer);
            }
        }

        if let Some(buffer) = notifications.buffers.get(&key)
            && let Some(element) = run.element(renderer, buffer, Point::from((0, 0)), (WIDTH, height), 1.0)
        {
            elements.push(element);
        }
    }

    elements.append(&mut shadows);
    elements
}

#[derive(Clone, Copy)]
struct Placement {
    position: Point<i32, Logical>,
    height: i32,
    factor: f64,
    alpha: f32,
    scale: Scale<f64>,
}

impl Placement {
    fn element<R>(
        &self, renderer: &mut R, buffer: &MemoryRenderBuffer, local: Point<i32, Logical>, size: (i32, i32), alpha: f32,
    ) -> Option<MemoryRenderBufferRenderElement<R>>
    where
        R: ShellRenderer,
    {
        let alpha = Some(self.alpha * alpha);
        let at = Point::from((self.position.x + local.x, self.position.y + local.y)).to_f64();
        if (self.factor - 1.0).abs() < f64::EPSILON {
            return MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                at.to_physical(self.scale),
                buffer,
                alpha,
                None,
                None,
                Kind::Unspecified,
            )
            .ok();
        }
        let centre = Point::<f64, Logical>::from((
            f64::from(self.position.x) + f64::from(WIDTH) / 2.0,
            f64::from(self.position.y) + f64::from(self.height) / 2.0,
        ));
        let scaled = Point::<f64, Logical>::from((
            centre.x - (centre.x - at.x) * self.factor,
            centre.y - (centre.y - at.y) * self.factor,
        ));
        let dst = Size::<i32, Logical>::from((
            (f64::from(size.0) * self.factor).round() as i32,
            (f64::from(size.1) * self.factor).round() as i32,
        ));
        let src =
            Rectangle::<f64, Logical>::new(Point::from((0.0, 0.0)), Size::from((f64::from(size.0), f64::from(size.1))));
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            scaled.to_physical(self.scale),
            buffer,
            alpha,
            Some(src),
            Some(dst),
            Kind::Unspecified,
        )
        .ok()
    }
}

fn push_peeks<R>(
    notifications: &mut Notifications, renderer: &mut R, theme: &NotificationTheme, index: usize, run: Placement,
    output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let duration = glide_theme::motion::t2();
    let (id, fades): (u32, Vec<f32>) = {
        let entry = &notifications.live[index];
        let fades =
            entry.peeks.iter().map(|reveal| narrow(reveal.progress(duration, ease_out).clamp(0.0, 1.0))).collect();
        (entry.notification.id, fades)
    };
    for (layer, fade) in fades.into_iter().enumerate() {
        let step = layer as i32 + 1;
        let key = (id, step, output_scale);
        if let std::collections::hash_map::Entry::Vacant(slot) = notifications.peeks.entry(key)
            && let Some(buffer) = draw_peek(theme, step, output_scale)
        {
            slot.insert(buffer);
        }
        let Some(buffer) = notifications.peeks.get(&key) else {
            continue;
        };
        let local = Point::from((PEEK_INSET * step, run.height - PEEK_BODY + PEEK_REVEAL * step));
        let size = (WIDTH - PEEK_INSET * step * 2, PEEK_BODY);
        if let Some(element) = run.element(renderer, buffer, local, size, fade) {
            elements.push(element);
        }
    }
}

fn push_layers<R>(
    notifications: &mut Notifications, renderer: &mut R, theme: &NotificationTheme, index: usize, run: Placement,
    output_scale: i32, elements: &mut Vec<MemoryRenderBufferRenderElement<R>>,
) where
    R: ShellRenderer,
{
    let entry = &notifications.live[index];
    let id = entry.notification.id;
    let composing = entry.reply_text.is_some();
    let invalid = entry.reply_phase() == ReplyPhase::Invalid;
    let ring = entry.ring_alpha();
    let toast_hover = entry.hover_alpha();
    let Some(body) = notifications.slots.iter().find(|(hit, _)| *hit == Hit::Body(id)).map(|(_, rect)| rect.loc) else {
        return;
    };
    let layers: Vec<(Point<i32, Logical>, LayerShape, [f32; 2])> = notifications
        .layer_slots
        .iter()
        .filter(|(hit, _, _)| hit.id() == id)
        .map(|(hit, rect, shape)| {
            (
                Point::from((rect.loc.x - body.x, rect.loc.y - body.y)),
                *shape,
                [notifications.hover_layer_alpha(*hit), notifications.press_layer_alpha(*hit)],
            )
        })
        .collect();

    for (local, shape, [hover, press]) in layers {
        let glyph = if shape == LayerShape::Dismiss { toast_hover } else { 0.0 };
        for (kind, alpha) in [(LayerKind::Glyph, glyph), (LayerKind::Hover, hover), (LayerKind::Press, press)] {
            if alpha <= 0.0 {
                continue;
            }
            let key = (shape, kind, output_scale);
            if let std::collections::hash_map::Entry::Vacant(slot) = notifications.layers.entry(key)
                && let Some(buffer) = draw_layer(theme, shape, kind, output_scale)
            {
                slot.insert(buffer);
            }
            if let Some(buffer) = notifications.layers.get(&key)
                && let Some(element) = run.element(renderer, buffer, local, shape.size(), alpha)
            {
                elements.push(element);
            }
        }
    }

    if !composing || ring <= 0.0 {
        return;
    }
    let key = (invalid, output_scale);
    if let std::collections::hash_map::Entry::Vacant(slot) = notifications.rings.entry(key)
        && let Some(buffer) = draw_ring(theme, invalid, output_scale)
    {
        slot.insert(buffer);
    }
    let local = Point::from((PADDING + ICON + ICON_GAP - REPLY_RING, run.height - PADDING - REPLY_HEIGHT - REPLY_RING));
    if let Some(buffer) = notifications.rings.get(&key)
        && let Some(element) = run.element(renderer, buffer, local, ring_size(), ring)
    {
        elements.push(element);
    }
}

fn draw_layer(theme: &NotificationTheme, shape: LayerShape, kind: LayerKind, scale: i32) -> Option<MemoryRenderBuffer> {
    let (width, height) = shape.size();
    let mut c = Canvas::new(width, height, scale)?;
    match kind {
        LayerKind::Hover => c.panel(theme.hover, shape.radius()),
        LayerKind::Press => c.panel(theme.hover.with_alpha(glide_theme::motion::ST_PRESS), shape.radius()),
        LayerKind::Glyph => {
            crate::paint::draw_close_glyph_over(
                &mut c.buf,
                width,
                height,
                DISMISS_GLYPH,
                DISMISS_STROKE,
                scale,
                theme.meta,
            );
        }
    }
    Some(c.finish())
}

const fn ring_size() -> (i32, i32) {
    (WIDTH - PADDING - (PADDING + ICON + ICON_GAP) + REPLY_RING * 2, REPLY_HEIGHT + REPLY_RING * 2)
}

fn ring_width() -> f32 {
    f32::from(u8::try_from(REPLY_RING).unwrap_or_default())
}

fn draw_ring(theme: &NotificationTheme, invalid: bool, scale: i32) -> Option<MemoryRenderBuffer> {
    let (width, height) = ring_size();
    let mut c = Canvas::new(width, height, scale)?;
    let color = if invalid { theme.reply_invalid_border.with_alpha(theme.accent_ring.0[3]) } else { theme.accent_ring };
    c.stroke(c.full(), ring_width(), BUTTON_RADIUS + ring_width(), color);
    Some(c.finish())
}

fn draw_peek(theme: &NotificationTheme, step: i32, scale: i32) -> Option<MemoryRenderBuffer> {
    let size = (WIDTH - PEEK_INSET * step * 2, PEEK_BODY);
    Some(Canvas::popover(size, scale, RADIUS, theme.background, theme.border)?.finish())
}

fn draw_shadow(
    height: i32, margin: i32, scale: i32, appearance: glide_theme::Appearance,
) -> Option<MemoryRenderBuffer> {
    Some(crate::raster::shadow((WIDTH, height), margin, RADIUS, &shadow_layers(appearance), scale)?.finish())
}

fn draw_toast(
    painter: &crate::paint::Painter, theme: &NotificationTheme, entry: &Entry, phase: ReplyPhase, scale: i32,
) -> Option<MemoryRenderBuffer> {
    let mut c = Canvas::new(WIDTH, entry.height, scale)?;
    c.panel(theme.background, RADIUS);
    if entry.notification.urgency == Urgency::Critical {
        draw_critical_stripe(&mut c, theme.danger);
    }
    c.stroke(c.full(), 1.0, RADIUS, theme.border);
    if let Some(icon) = entry.icon.as_deref() {
        crate::paint::blit_icon_over(&mut c.buf, WIDTH, c.h, icon, ICON_SOURCE_PX, ICON, PADDING, PADDING, scale);
    } else {
        c.fill(rect(PADDING, PADDING, ICON, ICON), theme.fill, ICON_RADIUS);
    }
    draw_toast_text(painter, theme, entry, &mut c);
    draw_toast_buttons(painter, theme, entry, &mut c, phase);
    Some(c.finish())
}

const CRITICAL_STRIPE: i32 = 3;

fn draw_critical_stripe(c: &mut Canvas<'_>, color: Color) {
    let (Ok(row), Ok(keep), Some(mut stripe)) = (
        usize::try_from(c.w * c.scale * 4),
        usize::try_from(CRITICAL_STRIPE * c.scale * 4),
        Canvas::new(c.w, c.h, c.scale),
    ) else {
        return;
    };
    stripe.panel(color, RADIUS);
    for line in stripe.buf.chunks_exact_mut(row) {
        line[keep.min(row)..].fill(0);
    }
    c.blit(&stripe.buf, c.full());
}

fn draw_toast_text(painter: &crate::paint::Painter, theme: &NotificationTheme, entry: &Entry, c: &mut Canvas<'_>) {
    let (left, right) = (PADDING + ICON + ICON_GAP, WIDTH - PADDING);
    let mut y = PADDING;
    draw_meta_row(painter, theme, entry, c, (left, right), y);
    y += META_HEIGHT;

    let title_color = if entry.notification.urgency == Urgency::Critical { theme.danger } else { theme.title };
    let band = (left, right, y, y + TITLE_HEIGHT);
    c.text(painter, band, TextAlign::Left, &entry.notification.summary, title_color, &TextStyle::HEAD);
    y += TITLE_HEIGHT;

    for line in entry.body_lines.iter().flatten() {
        c.text(painter, (left, right, y, y + BODY_LINE), TextAlign::Left, line, theme.body, &TextStyle::CALL);
        y += BODY_LINE;
    }

    if let Some(progress) = entry.notification.progress {
        let track = rect(left, y + PROGRESS_GAP, right - left, PROGRESS_HEIGHT);
        c.fill(track, theme.fill, glide_theme::shape::R_ROUND);
        let filled = ((track.size.w as f32) * progress.clamp(0.0, 1.0)).round() as i32;
        if filled > 0 {
            c.fill(Rect { size: (filled, PROGRESS_HEIGHT).into(), ..track }, theme.accent, glide_theme::shape::R_ROUND);
        }
    }
}

fn draw_meta_row(
    painter: &crate::paint::Painter, theme: &NotificationTheme, entry: &Entry, c: &mut Canvas<'_>,
    (text_left, text_right): (i32, i32), y: i32,
) {
    let meta = meta_line(entry);
    let badge = count_badge(entry);
    let measure = |text: &str| text_px(painter, text, &TextStyle::CAP);
    let badge_width = badge.as_ref().map_or(0, |text| measure(text) + COUNT_PAD_X * 2);
    let meta_right = text_right - DISMISS_SIDE - badge_width;
    c.text(painter, (text_left, meta_right, y, y + META_HEIGHT), TextAlign::Left, &meta, theme.meta, &TextStyle::CAP);

    let Some(text) = badge.as_ref() else {
        return;
    };
    let badge_x = (text_left + measure(&meta) + glide_theme::spacing::SP2).min(meta_right);
    let badge = rect(badge_x, y + (META_HEIGHT - COUNT_HEIGHT) / 2, badge_width, COUNT_HEIGHT);
    c.fill(badge, theme.fill, COUNT_RADIUS);
    c.text_in(painter, badge, TextAlign::Center, text, theme.body, &TextStyle::CAP);
}

fn draw_toast_buttons(
    painter: &crate::paint::Painter, theme: &NotificationTheme, entry: &Entry, c: &mut Canvas<'_>, phase: ReplyPhase,
) {
    match phase {
        ReplyPhase::Sent => {
            let top = c.h - PADDING - REPLY_HEIGHT;
            let band = (PADDING + ICON + ICON_GAP, WIDTH - PADDING, top, top + REPLY_HEIGHT);
            let sent = crate::i18n::t(crate::i18n::Key::Sent);
            c.text(painter, band, TextAlign::Left, sent, theme.meta, &TextStyle::CAP);
            return;
        }
        ReplyPhase::Composing | ReplyPhase::Invalid => {
            draw_reply_composer(painter, theme, entry, c, phase);
            return;
        }
        ReplyPhase::Closed => {}
    }
    let top = c.h - PADDING - BUTTON_HEIGHT;
    let mut x = PADDING + ICON + ICON_GAP;
    let accent = (theme.accent, theme.accent_label);
    let plain = (theme.fill, theme.title);
    let reply = entry
        .reply_button
        .zip(entry.notification.reply.as_ref())
        .map(|(width, reply)| (width, reply.label.as_str(), accent));
    let buttons =
        entry.buttons.iter().zip(visible_buttons(&entry.notification)).enumerate().map(|(index, (width, action))| {
            let primary = index == 0 && entry.reply_button.is_none();
            (*width, action.label.as_str(), if primary { accent } else { plain })
        });
    let settings = entry.settings_button.map(|width| (width, crate::i18n::t(crate::i18n::Key::Configure), plain));
    for (width, label, colors) in reply.into_iter().chain(buttons).chain(settings) {
        draw_chip(painter, c, rect(x, top, width, BUTTON_HEIGHT), label, colors);
        x += width + BUTTON_GAP;
    }
}

fn draw_chip(
    painter: &crate::paint::Painter, c: &mut Canvas<'_>, chip: Rect, label: &str,
    (background, foreground): (Color, Color),
) {
    c.fill(chip, background, BUTTON_RADIUS);
    c.text_in(painter, chip, TextAlign::Center, label, foreground, &TextStyle::LAUNCHER_CHIP);
}

fn reply_placeholder(entry: &Entry) -> String {
    entry
        .notification
        .reply
        .as_ref()
        .map(|reply| reply.placeholder.clone())
        .filter(|placeholder| !placeholder.trim().is_empty())
        .unwrap_or_else(|| crate::i18n::t(crate::i18n::Key::Reply).to_string())
}

fn visible_reply(entry: &Entry, text: &str) -> String {
    let secret = entry.notification.reply.as_ref().is_some_and(|reply| reply.secret);
    if secret { "\u{2022}".repeat(text.chars().count()) } else { text.to_string() }
}

fn draw_reply_composer(
    painter: &crate::paint::Painter, theme: &NotificationTheme, entry: &Entry, c: &mut Canvas<'_>, phase: ReplyPhase,
) {
    let Some(text) = entry.reply_text.as_ref() else {
        return;
    };
    let left = PADDING + ICON + ICON_GAP;
    let right = WIDTH - PADDING;
    let top = c.h - PADDING - REPLY_HEIGHT;
    let border = if phase == ReplyPhase::Invalid { theme.reply_invalid_border } else { theme.accent };
    let field = rect(left, top, right - left, REPLY_HEIGHT);
    c.fill(field, theme.field, BUTTON_RADIUS);
    c.stroke(field, 1.0, BUTTON_RADIUS, border);

    let send_x = right - REPLY_SEND_INSET - REPLY_SEND;
    let text_left = left + REPLY_PAD_LEFT;
    let text_right = send_x - glide_theme::spacing::SP2;

    let showing_placeholder = text.is_empty();
    let (shown, colour) = if showing_placeholder {
        (reply_placeholder(entry), theme.meta)
    } else {
        (clip_tail(painter, &visible_reply(entry, text), text_right - text_left), theme.title)
    };
    let style = &TextStyle::LAUNCHER_CHIP;
    c.text(painter, (text_left, text_right, top, top + REPLY_HEIGHT), TextAlign::Left, &shown, colour, style);

    if !showing_placeholder {
        let caret = text_left + text_px(painter, &shown, style) + 1;
        if caret < text_right {
            c.fill(rect(caret, top + (REPLY_HEIGHT - 15) / 2, 2, 15), theme.accent, 1.0);
        }
    }

    let send = rect(send_x, top + (REPLY_HEIGHT - REPLY_SEND) / 2, REPLY_SEND, REPLY_SEND);
    draw_chip(painter, c, send, "\u{2191}", (theme.accent, theme.accent_label));
}

fn clip_tail(painter: &crate::paint::Painter, text: &str, max_width: i32) -> String {
    let starts: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    let fits = |start: usize| text_px(painter, &text[start..], &TextStyle::LAUNCHER_CHIP) <= max_width;
    let first = starts.partition_point(|&start| !fits(start));
    starts.get(first).map_or_else(String::new, |&start| text[start..].to_string())
}

fn meta_line(entry: &Entry) -> String {
    let app = entry.notification.display_app();
    let when = elapsed_label(entry);
    let origin = entry.notification.origin.trim();
    if origin.is_empty() { format!("{app} · {when}") } else { format!("{app} · {origin} · {when}") }
}

fn elapsed_label(entry: &Entry) -> String {
    crate::i18n::elapsed_label(entry.updated_at.elapsed().as_secs())
}

fn count_badge(entry: &Entry) -> Option<String> {
    (!entry.stacked.is_empty()).then(|| format!("+{}", entry.stacked.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::buffer_from;

    fn painter() -> crate::paint::Painter {
        crate::paint::Painter::new(14.0, None)
    }

    #[test]
    fn hovering_a_toast_freezes_its_timeout_and_leaving_resumes_the_rest() {
        let start = Instant::now();
        let secs = Duration::from_secs;
        let timeout = secs(5);
        let mut expiry = Expiry::new(start);

        assert_eq!(expiry.elapsed(start + secs(2)), secs(2));
        expiry.pause(start + secs(2));
        assert!(!expiry.expired(timeout, start + secs(30)));

        expiry.resume(start + secs(30));
        assert_eq!(expiry.elapsed(start + secs(30)), secs(2));
        assert!(!expiry.expired(timeout, start + secs(32)));
        assert!(expiry.expired(timeout, start + secs(33)));

        let mut nested = Expiry::new(start);
        for (at, pause) in [(1, true), (4, true), (6, false), (9, false), (10, true), (12, false)] {
            if pause {
                nested.pause(start + secs(at));
            } else {
                nested.resume(start + secs(at));
            }
        }
        assert_eq!(nested.elapsed(start + secs(14)), secs(7));

        let mut hovered = Expiry::new(start);
        hovered.pause(start + secs(3));
        let next = hovered.restarted(start + secs(8));
        assert!(next.is_paused(), "a replacement under the pointer stays paused");
        assert_eq!(next.elapsed(start + secs(20)), Duration::ZERO);
        assert!(!Expiry::new(start).restarted(start + secs(1)).is_paused());
    }

    #[test]
    fn the_critical_stripe_stays_inside_the_rounded_card() {
        let (height, scale) = (80, 2);
        let mut c = Canvas::new(WIDTH, height, scale).unwrap();
        draw_critical_stripe(&mut c, Color([1.0, 0.0, 0.0, 1.0]));
        let alpha = |x: i32, y: i32| c.buf[usize::try_from((y * WIDTH * scale + x) * 4 + 3).unwrap()];
        assert_eq!(alpha(0, 0), 0, "top-left corner is outside the card radius");
        assert_eq!(alpha(0, height * scale - 1), 0, "bottom-left corner too");
        assert!(alpha(1, height * scale / 2) > 0, "the stripe shows mid-card");
        assert_eq!(alpha(CRITICAL_STRIPE * scale, height * scale / 2), 0);
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
        entry.stacked.iter().map(|notification| notification.id).collect()
    }

    fn from_app(id: u32, app: &str) -> Notification {
        Notification { app_name: app.to_string(), ..notification(id) }
    }

    fn repliable(id: u32) -> Notification {
        Notification {
            sender: ":1.42".to_string(),
            reply: Some(glide_notify::Reply {
                key: glide_notify::ACTION_INLINE_REPLY.to_string(),
                label: "Reply".to_string(),
                placeholder: "Message…".to_string(),
                secret: false,
            }),
            timeout: None,
            ..notification(id)
        }
    }

    fn store() -> (Notifications, crate::icon::IconLoader) {
        (Notifications::new(NotificationTheme::default()), crate::icon::IconLoader::new(None))
    }

    fn settle_entry(store: &mut Notifications) {
        if let Some(settled) = Instant::now().checked_sub(glide_theme::motion::t4()) {
            store.live[0].shown_at = settled;
        }
        assert!(!store.is_animating(), "a settled toast has nothing left to animate");
    }

    #[test]
    fn a_secret_reply_is_drawn_as_bullets_and_local_toasts_report_back_to_the_shell() {
        let (mut store, mut icons) = store();
        let mut prompt = repliable(LOCAL_ID_FLOOR + 1);
        if let Some(reply) = prompt.reply.as_mut() {
            reply.secret = true;
        }
        store.push(&mut icons, prompt, false);
        assert_eq!(visible_reply(&store.live[0], "pässword"), "\u{2022}".repeat(8));
        assert!(store.open_reply(LOCAL_ID_FLOOR + 1));
        for ch in "hunter2".chars() {
            store.type_reply(ch);
        }
        assert!(store.send_reply());
        assert_eq!(
            store.take_local_events(),
            [LocalEvent::Reply { id: LOCAL_ID_FLOOR + 1, text: "hunter2".into() }],
            "a local reply never reaches D-Bus"
        );
        store.push(&mut icons, repliable(7), false);
        let ordinary = store.live.iter().find(|entry| entry.notification.id == 7).expect("pushed");
        assert_eq!(visible_reply(ordinary, "hi"), "hi", "ordinary replies stay readable");
    }

    #[test]
    fn the_rail_hugs_the_configured_corner_below_the_topbar() {
        let (mut store, mut icons) = store();
        assert!(!store.contains(output(), Point::from((1800.0, 60.0))), "empty");
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        let rect = store.rect(output());
        assert_eq!(rect.loc.x + rect.size.w, 1920 - RAIL_INSET);
        assert_eq!((rect.loc.y, rect.size.w), (crate::topbar::HEIGHT + RAIL_GAP, WIDTH));
        for (position, x) in [(BannerPosition::Left, RAIL_INSET), (BannerPosition::Center, (1920 - WIDTH) / 2)] {
            store.set_policy(NotificationPolicy { position, ..NotificationPolicy::default() });
            assert_eq!(store.rect(output()).loc.x, x);
        }
    }

    #[test]
    fn a_muted_app_files_straight_into_the_center_without_a_toast() {
        let (mut store, mut icons) = store();
        store.set_policy(NotificationPolicy { muted: vec!["telegram".to_string()], ..NotificationPolicy::default() });
        assert!(store.push(&mut icons, from_app(1, "Telegram"), false));
        assert!(store.active().next().is_none());
        assert_eq!(store.archive.len(), 1);
        assert!(store.push(&mut icons, from_app(2, "Firefox"), false));
        assert_eq!(store.active().count(), 1);

        let policy = NotificationPolicy {
            muted: vec!["org.telegram.desktop.desktop".to_string()],
            ..NotificationPolicy::default()
        };
        let sent = Notification { desktop_entry: "org.telegram.desktop".to_string(), ..notification(1) };
        assert!(policy.mutes(&sent), "desktop entries match with or without .desktop");
        assert!(!policy.mutes(&notification(2)));
    }

    #[test]
    fn the_banner_duration_overrides_expiring_toasts_but_not_sticky_ones() {
        let (mut store, mut icons) = store();
        store.set_policy(NotificationPolicy {
            banner_timeout: Some(Duration::from_secs(20)),
            ..NotificationPolicy::default()
        });
        store.push(&mut icons, notification(1), false);
        let mut sticky = notification(2);
        sticky.timeout = None;
        store.push(&mut icons, sticky, false);
        let timeout = |id: u32| {
            store.active().find(|entry| entry.notification.id == id).and_then(|entry| entry.notification.timeout)
        };
        assert_eq!(timeout(1), Some(Duration::from_secs(20)));
        assert_eq!(timeout(2), None);
    }

    fn critical(id: u32) -> Notification {
        Notification { urgency: Urgency::Critical, timeout: None, ..notification(id) }
    }

    #[test]
    fn do_not_disturb_and_inhibitors_file_everything_but_critical_into_the_center() {
        let (mut store, mut icons) = store();
        store.set_dnd(true);
        assert!(store.push(&mut icons, notification(1), false));
        assert!(store.active().next().is_none());
        assert_eq!(store.nc_items()[0].id, 1);
        assert!(store.push(&mut icons, critical(2), false));
        assert_eq!(store.active().count(), 1);
        assert_eq!(store.unread(), 2);

        let (mut store, mut icons) = self::store();
        store.set_policy(NotificationPolicy {
            do_not_disturb: true,
            allow_critical: false,
            ..NotificationPolicy::default()
        });
        assert!(store.push(&mut icons, critical(1), false));
        assert!(store.active().next().is_none(), "critical can be held back too");

        let (mut store, mut icons) = self::store();
        store.set_inhibited(true);
        assert!(store.suppressed());
        assert!(store.push(&mut icons, notification(1), false));
        assert!(store.active().next().is_none());
        assert_eq!(store.archive.len(), 1);
    }

    #[test]
    fn configured_and_scheduled_do_not_disturb_apply_only_at_their_edges() {
        let (mut store, _) = store();
        store.set_policy(NotificationPolicy::default());
        assert!(!store.dnd());
        store.set_dnd(true);
        store.set_policy(NotificationPolicy::default());
        assert!(store.dnd(), "an unrelated reload keeps the top bar toggle");
        store.set_policy(NotificationPolicy { do_not_disturb: true, ..NotificationPolicy::default() });
        store.set_policy(NotificationPolicy::default());
        assert!(!store.dnd());

        store.set_policy(NotificationPolicy { schedule: Some((22 * 60, 7 * 60)), ..NotificationPolicy::default() });
        assert!(store.follow_schedule(Some(23 * 60)));
        assert!(store.dnd());
        store.set_dnd(false);
        assert!(!store.follow_schedule(Some(23 * 60 + 5)));
        assert!(!store.dnd(), "a manual toggle holds until the next edge");
        assert!(!store.follow_schedule(Some(8 * 60)));
        store.set_dnd(true);
        assert!(!store.follow_schedule(Some(9 * 60)));
        assert!(store.dnd());
        store.set_dnd(false);
        assert!(store.follow_schedule(Some(22 * 60)));
        assert!(store.dnd());
        assert!(store.follow_schedule(Some(7 * 60)));
        assert!(!store.dnd());
    }

    #[test]
    fn the_newest_sits_on_top_replacements_keep_their_place_and_the_rail_is_capped() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);
        let ids =
            |store: &Notifications| -> Vec<u32> { store.live.iter().map(|entry| entry.notification.id).collect() };
        assert_eq!(ids(&store), [2, 1]);
        let updated = Notification { summary: "Edited".to_string(), ..notification(1) };
        store.push(&mut icons, updated, true);
        assert_eq!(ids(&store), [2, 1]);
        assert_eq!(store.live[1].notification.summary, "Edited");
        assert_eq!(store.unread(), 2);

        let (mut store, mut icons) = self::store();
        let overflow = u32::try_from(MAX_VISIBLE).expect("the budget fits a u32") + 3;
        for id in 1..=overflow {
            store.push(&mut icons, notification(id), false);
        }
        assert_eq!(store.active().count(), MAX_VISIBLE);
        assert_eq!(store.unread(), MAX_VISIBLE + 3);
    }

    #[test]
    fn an_expired_toast_stays_in_the_center_unread_until_it_is_seen_or_dismissed() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        assert!(!store.tick());
        std::thread::sleep(Duration::from_millis(80));
        assert!(store.tick());
        assert!(store.active().next().is_none());
        assert_eq!(store.archive.len(), 1);
        assert_eq!(store.unread(), 1);
        assert_eq!(store.nc_items().len(), 1);
        assert!(!store.is_empty());

        assert!(store.mark_read());
        assert!(!store.mark_read());
        assert_eq!(store.unread(), 0);
        assert_eq!(store.nc_items().len(), 1, "seen is not gone");

        assert!(store.dismiss(1));
        assert!(!store.dismiss(1));
        assert!(store.nc_items().is_empty());
        assert!(store.is_empty());
    }

    #[test]
    fn dismissed_closed_and_transient_toasts_leave_no_trace() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);
        assert!(store.remove(1, CloseReason::Dismissed, true));
        assert!(store.remove(2, CloseReason::Revoked, false));
        assert!(!store.remove(1, CloseReason::Dismissed, false));
        assert!(store.is_empty() && store.buffers.is_empty());
        let fleeting = Notification { transient: true, ..notification(3) };
        store.push(&mut icons, fleeting, false);
        std::thread::sleep(Duration::from_millis(80));
        assert!(store.tick());
        assert_eq!(store.archive.len(), 0);
    }

    #[test]
    fn an_evicted_toast_is_kept_with_its_stack_and_the_center_lists_them_newest_first() {
        let (mut store, mut icons) = store();
        let overflow = u32::try_from(MAX_VISIBLE).expect("fits") + 1;
        for id in 1..=overflow {
            let mut sticky = notification(id);
            sticky.timeout = None;
            store.push(&mut icons, sticky, false);
        }
        assert_eq!(store.archive.len(), 1);
        let ids: Vec<u32> = store.nc_items().iter().map(|item| item.id).collect();
        assert_eq!(ids.last(), Some(&1), "the oldest was evicted and follows the live ones");
        assert_eq!(ids.len(), usize::try_from(overflow).expect("fits"));
    }

    #[test]
    fn a_replacement_with_a_kept_id_supersedes_the_kept_copy() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        std::thread::sleep(Duration::from_millis(80));
        assert!(store.tick());
        assert_eq!(store.archive.len(), 1);
        store.push(&mut icons, notification(1), true);
        assert_eq!(store.archive.len(), 0);
        assert_eq!(store.nc_items().len(), 1);
    }

    #[test]
    fn the_archive_is_capped_at_its_budget() {
        let (mut store, mut icons) = store();
        store.set_dnd(true);
        let budget = u32::try_from(MAX_ARCHIVED).expect("fits");
        for id in 1..=budget + 5 {
            store.push(&mut icons, notification(id), false);
        }
        assert_eq!(store.archive.len(), MAX_ARCHIVED);
        assert_eq!(store.nc_items()[0].id, budget + 5);
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
    }

    #[test]
    fn entry_animation_state_machine_reveal_open_now_and_retire_left() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);

        assert_eq!(store.live.len(), 2);
        for entry in &store.live {
            assert!(entry.reveal.is_open());
            assert!(!entry.leaving());
        }

        assert!(!store.retire_left());
        assert_eq!(store.live.len(), 2);

        store.leave(0);
        assert!(store.live[0].leaving());
        assert!(!store.live[1].leaving());

        assert!(!store.retire_left());
        assert_eq!(store.live.len(), 2);

        std::thread::sleep(glide_theme::motion::t2() + Duration::from_millis(10));

        assert!(store.retire_left());
        assert_eq!(store.live.len(), 1);
        assert_eq!(store.live[0].notification.id, 1);
        assert!(!store.live[0].leaving());

        assert!(!store.retire_left());
        assert_eq!(store.live.len(), 1);
    }

    fn pixel() -> MemoryRenderBuffer {
        buffer_from(&[0u8; 4], 1, 1, 1)
    }

    fn centre(rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        Point::from((f64::from(rect.loc.x + rect.size.w / 2), f64::from(rect.loc.y + rect.size.h / 2)))
    }

    fn slot(store: &Notifications, hit: Hit) -> Rectangle<i32, Logical> {
        store
            .slots
            .iter()
            .find(|(slot, _)| *slot == hit)
            .map_or_else(|| panic!("{hit:?} has a slot"), |(_, rect)| *rect)
    }

    #[test]
    fn buffers_3tuple_key_retention_isolates_toast_invalidation() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);

        let buf = pixel();

        let k1_default = (1, ReplyPhase::Closed, 1);
        let k1_reply = (1, ReplyPhase::Composing, 1);
        let k1_scale2 = (1, ReplyPhase::Closed, 2);

        let k2_default = (2, ReplyPhase::Closed, 1);
        let k2_sent = (2, ReplyPhase::Sent, 1);
        let k2_scale2 = (2, ReplyPhase::Closed, 2);

        store.buffers.insert(k1_default, buf.clone());
        store.buffers.insert(k1_reply, buf.clone());
        store.buffers.insert(k1_scale2, buf.clone());

        store.buffers.insert(k2_default, buf.clone());
        store.buffers.insert(k2_sent, buf.clone());
        store.buffers.insert(k2_scale2, buf.clone());

        assert_eq!(store.buffers.len(), 6);

        let mut edited = notification(1);
        edited.summary = "Updated Summary".to_string();
        store.push(&mut icons, edited, true);

        assert!(!store.buffers.contains_key(&k1_default));
        assert!(!store.buffers.contains_key(&k1_reply));
        assert!(!store.buffers.contains_key(&k1_scale2));

        assert!(store.buffers.contains_key(&k2_default));
        assert!(store.buffers.contains_key(&k2_sent));
        assert!(store.buffers.contains_key(&k2_scale2));
        assert_eq!(store.buffers.len(), 3);

        store.buffers.insert(k1_default, buf);
        assert_eq!(store.buffers.len(), 4);

        assert!(store.remove(1, CloseReason::Dismissed, false));
        assert!(
            store.buffers.contains_key(&k1_default),
            "a leaving toast keeps its raster so the fade-out has pixels to draw"
        );
        assert_eq!(store.buffers.len(), 4);

        store.leave(0);
        std::thread::sleep(glide_theme::motion::t2() + Duration::from_millis(10));
        assert!(store.retire_left());
        assert!(store.buffers.is_empty());
    }

    #[test]
    fn a_dismissed_toast_is_not_hit_testable_but_stays_rendered_until_its_leave_ends() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());
        let body = slot(&store, Hit::Body(1));
        store.buffers.insert((1, ReplyPhase::Closed, 1), pixel());
        assert!(store.set_hover(Some(centre(slot(&store, Hit::Dismiss(1))))));

        assert!(store.remove(1, CloseReason::Dismissed, false));
        assert_eq!(store.live.len(), 1, "the entry lingers for its fade-out");
        assert!(store.live[0].leaving());
        assert!(store.is_empty(), "but it no longer counts as a live toast");
        assert_eq!(store.unread(), 0);
        assert_eq!(store.hovered, None);
        assert!(store.hover_layers.is_empty());

        store.layout_slots(output());
        assert!(store.slots.is_empty(), "a leaving toast offers no targets");
        assert!(store.layer_slots.is_empty());
        assert_eq!(store.hit_at(centre(body)), None);
        assert!(!store.contains(output(), centre(body)));
        assert!(!store.set_hover(Some(centre(body))));

        let progress = store.live[0].leave_progress();
        assert!(progress > 0.0 && progress <= 1.0, "fading, not gone: {progress}");
        assert!(store.live[0].leave_scale() <= 1.0);
        assert!(store.live[0].leave_scale() >= LEAVE_SCALE);
        assert!(
            store.buffers.contains_key(&(1, ReplyPhase::Closed, 1)),
            "the raster survives until the entry is retired"
        );
        assert!(store.is_animating());
        assert!(store.settle(), "the pump keeps running while it fades");

        std::thread::sleep(glide_theme::motion::t2() + Duration::from_millis(10));
        assert!(!store.settle(), "a finished leave is retired and stops the pump");
        assert!(store.live.is_empty());
        assert!(store.buffers.is_empty());
        assert!(!store.remove(1, CloseReason::Dismissed, false));
    }

    #[test]
    fn a_toast_below_a_dismissed_one_slides_up_from_its_drawn_offset() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, notification(2), false);
        store.measure(&painter());
        store.layout_slots(output());

        let below = store.live.iter().position(|entry| entry.notification.id == 1).expect("toast 1 is laid out");
        let rested = occupied_height(&store.live[0]) + RAIL_GAP;
        assert_eq!(store.live[below].drawn_offset(0), rested);
        assert!(!store.live[below].y.is_some_and(|y| y.is_animating(glide_theme::motion::t3())));

        assert!(store.remove(2, CloseReason::Dismissed, false));
        store.layout_slots(output());

        let tween = store.live[below].y.expect("laid out twice");
        assert!((tween.target() - 0.0).abs() < f64::EPSILON, "the freed slot is the new target");
        assert!(
            (store.live[below].drawn_offset(0) - rested).abs() <= 1,
            "the slide starts from where the toast was drawn, not from the target"
        );
        assert!(tween.is_animating(glide_theme::motion::t3()));
        assert!(store.is_animating());

        let origin = store.rect(output()).loc;
        let body = slot(&store, Hit::Body(1));
        assert_eq!(body.loc.y, origin.y + store.live[below].drawn_offset(0), "hit boxes follow the drawn toast");
        assert_eq!(
            store.rect(output()).size.h,
            store.live[below].height,
            "the rail already excludes the leaving toast"
        );
    }

    #[test]
    fn a_new_toast_on_top_pushes_the_rest_down_from_their_drawn_offset() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());
        assert_eq!(store.live[0].drawn_offset(0), 0);

        store.push(&mut icons, notification(2), false);
        store.measure(&painter());
        store.layout_slots(output());

        let pushed_down = occupied_height(&store.live[0]) + RAIL_GAP;
        let older = store.live[1].y.expect("laid out");
        assert!((older.target() - f64::from(pushed_down)).abs() < f64::EPSILON);
        assert_eq!(store.live[1].drawn_offset(0), 0, "the older toast begins its slide where it was");
        assert_eq!(store.live[0].drawn_offset(0), 0, "the newcomer appears in its slot instead of sliding into it");
        assert!(older.is_animating(glide_theme::motion::t3()));
    }

    #[test]
    fn the_hover_layer_crossfades_from_its_current_value() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());
        let dismiss = slot(&store, Hit::Dismiss(1));
        let body = slot(&store, Hit::Body(1));

        assert!(store.set_hover(Some(centre(dismiss))));
        let tween = store.hover_layers[&Hit::Dismiss(1)];
        assert!((tween.target() - 1.0).abs() < f64::EPSILON);
        assert!(store.is_animating(), "the layer fade keeps the pump alive");

        std::thread::sleep(Duration::from_millis(40));
        let midway = store.hover_layer_alpha(Hit::Dismiss(1));
        assert!(midway > 0.0 && midway < 1.0, "midway was {midway}");

        assert!(
            store.set_hover(Some(Point::from((
                f64::from(body.loc.x + PADDING),
                f64::from(body.loc.y + body.size.h - 2),
            ))))
        );
        let reversed = store.hover_layer_alpha(Hit::Dismiss(1));
        assert!((reversed - midway).abs() < 0.1, "no pop on reversal: {midway} vs {reversed}");
        assert!((store.hover_layers[&Hit::Dismiss(1)].target() - 0.0).abs() < f64::EPSILON);
        assert!(!store.hover_layers.contains_key(&Hit::Body(1)), "the body has no state layer");
    }

    #[test]
    fn a_press_layer_lands_at_once_and_fades_out_on_release() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());
        let dismiss = slot(&store, Hit::Dismiss(1));

        assert!(store.set_pressed(Some(centre(dismiss))));
        assert!((store.press_layer_alpha(Hit::Dismiss(1)) - 1.0).abs() < f32::EPSILON);
        assert!(!store.set_pressed(Some(centre(dismiss))));

        assert!(store.set_pressed(None));
        let tween = store.press_layers[&Hit::Dismiss(1)];
        assert!((tween.target() - 0.0).abs() < f64::EPSILON);
        assert!(tween.is_animating(glide_theme::motion::t1()));
        assert!(store.is_animating());
        assert!(!store.set_pressed(None));
    }

    #[test]
    fn the_composer_ring_fades_in_over_t1_and_drops_with_the_composer() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        assert!((store.live[0].ring_alpha() - 0.0).abs() < f32::EPSILON);

        store.open_reply(1);
        assert!((store.live[0].ring.target() - 1.0).abs() < f64::EPSILON);
        assert!(store.live[0].ring.is_animating(glide_theme::motion::t1()));

        store.cancel_reply();
        assert!((store.live[0].ring_alpha() - 0.0).abs() < f32::EPSILON);
        assert!(!store.live[0].ring.is_animating(glide_theme::motion::t1()));
    }

    #[test]
    fn peeks_reveal_as_a_stack_grows_and_fade_as_it_shrinks() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.measure(&painter());
        assert!(store.live[0].peeks.is_empty());

        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.measure(&painter());
        assert_eq!(store.live[0].peeks.len(), 1);
        assert!(store.live[0].peeks[0].is_open());
        assert!(store.live[0].peeks[0].is_animating(glide_theme::motion::t2()));

        assert!(store.remove(2, CloseReason::Dismissed, false));
        store.measure(&painter());
        assert_eq!(peek_layers(&store.live[0]), 0);
        assert_eq!(store.live[0].peeks.len(), 1, "the peek fades rather than vanishing");
        assert!(!store.live[0].peeks[0].is_open());
        assert!(store.is_animating());
    }

    #[test]
    fn evicting_the_oldest_toast_fades_it_out_instead_of_dropping_it() {
        let (mut store, mut icons) = store();
        let overflow = u32::try_from(MAX_VISIBLE).expect("the budget fits a u32") + 1;
        for id in 1..=overflow {
            store.push(&mut icons, notification(id), false);
        }
        assert_eq!(store.active().count(), MAX_VISIBLE);
        assert_eq!(store.live.len(), MAX_VISIBLE + 1);
        let evicted = store.live.iter().find(|entry| entry.leaving()).expect("the oldest toast is leaving");
        assert_eq!(evicted.notification.id, 1);
    }

    #[test]
    fn the_dismiss_target_sits_inside_its_toast_and_wins_over_the_body() {
        let (mut store, mut icons) = store();
        assert!(!store.clear_all(), "nothing to clear");
        store.push(&mut icons, notification(1), false);
        store.measure(&painter());
        store.layout_slots(output());
        let (body, dismiss) = (slot(&store, Hit::Body(1)), slot(&store, Hit::Dismiss(1)));
        assert!(dismiss.loc.x >= body.loc.x && dismiss.loc.y >= body.loc.y);
        assert!(dismiss.loc.x + dismiss.size.w <= body.loc.x + body.size.w);
        assert!(dismiss.loc.y + dismiss.size.h <= body.loc.y + body.size.h);
        assert_eq!(store.action_at(centre(dismiss)), Some(NotificationAction::Dismiss(1)));
        let corner = Point::from((f64::from(body.loc.x + PADDING), f64::from(body.loc.y + body.size.h - 2)));
        assert_eq!(store.action_at(corner), Some(NotificationAction::Activate(1)));

        let target = centre(dismiss);
        assert!(store.set_hover(Some(target)) && !store.set_hover(Some(target)));
        assert!(store.set_hover(None) && !store.set_hover(None));
        assert!(store.clear_all() && store.is_empty());
    }

    #[test]
    fn toasts_grow_with_their_body_and_progress_but_never_below_the_icon() {
        let painter = painter();
        let wrap = |body: &str| wrap_lines(&painter, body, &glide_theme::TextStyle::CALL, body_width(), BODY_MAX_LINES);
        let short = notification(1);
        let long = Notification { body: "word ".repeat(60), ..notification(2) };
        let (short_lines, long_lines) = (wrap(&short.body), wrap(&long.body));
        assert!(long_lines.len() > short_lines.len());
        assert!(entry_height(&long, &long_lines, false) > entry_height(&short, &short_lines, false));

        let with_bar = Notification { progress: Some(0.5), ..notification(2) };
        let one = vec!["one".to_string()];
        assert!(entry_height(&with_bar, &one, false) > entry_height(&short, &one, false));
        let bare = Notification { body: String::new(), ..notification(1) };
        assert!(entry_height(&bare, &[], false) >= PADDING * 2 + ICON);
    }

    #[test]
    fn wrapping_fits_the_width_the_line_budget_and_clips_what_cannot_break() {
        let painter = painter();
        let style = glide_theme::TextStyle::CALL;
        let wrap = |body: &str, max| wrap_lines(&painter, body, &style, body_width(), max);
        let long = wrap(&"lorem ipsum dolor sit amet ".repeat(40), BODY_MAX_LINES);
        assert_eq!(long.len(), BODY_MAX_LINES);
        assert!(long.last().is_some_and(|line| line.ends_with('…')));
        for line in wrap("the quick brown fox jumps over the lazy dog again and again", BODY_MAX_LINES) {
            assert!(text_px(&painter, &line, &style) <= body_width(), "{line:?}");
        }
        assert_eq!(wrap("short", BODY_MAX_LINES), vec!["short".to_string()]);
        let word = wrap(&"x".repeat(400), 1);
        assert_eq!(word.len(), 1);
        assert!(text_px(&painter, &word[0], &style) <= body_width());
        assert!(wrap("   ", BODY_MAX_LINES).is_empty());
    }

    #[test]
    fn the_meta_line_names_the_app_the_web_origin_and_the_age() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        let meta = meta_line(&store.live[0]);
        assert!(meta.starts_with("Mail"));
        assert!(meta.ends_with(&elapsed_label(&store.live[0])));
        let web = Notification { origin: "example.com".to_string(), ..notification(2) };
        store.push(&mut icons, web, false);
        assert!(meta_line(&store.live[0]).contains("example.com"));
    }

    #[test]
    fn a_disabled_rail_accepts_nothing() {
        let mut store = Notifications::new(NotificationTheme { enabled: false, ..NotificationTheme::default() });
        let mut icons = crate::icon::IconLoader::new(None);
        assert!(!store.push(&mut icons, notification(1), false));
        assert!(store.is_empty());
    }

    #[test]
    fn a_composer_opens_only_for_replies_and_edits_a_bounded_draft() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, notification(1), false);
        assert!(!store.open_reply(1) && store.reply_target().is_none());
        store.push(&mut icons, repliable(2), false);
        store.push(&mut icons, repliable(3), false);
        assert!(!store.type_reply('a'), "no composer, no text");
        assert!(store.open_reply(2));
        assert!(store.open_reply(3));
        assert_eq!(store.reply_target(), Some(3), "one composer at a time");

        assert!(store.type_reply('h') && store.type_reply('i'));
        assert_eq!(store.live[0].reply_text.as_deref(), Some("hi"));
        assert!(store.backspace_reply() && store.backspace_reply());
        assert!(!store.backspace_reply(), "an empty draft cannot shrink");
        for _ in 0..(REPLY_MAX_CHARS + 20) {
            store.type_reply('z');
        }
        let draft = store.live[0].reply_text.as_ref().map_or(0, |t| t.chars().count());
        assert_eq!(draft, REPLY_MAX_CHARS);

        assert!(store.cancel_reply() && !store.cancel_reply());
        assert_eq!(store.reply_target(), None);
        assert_eq!(store.unread(), 3, "cancelling is not dismissing");
        store.open_reply(3);
        store.remove(3, CloseReason::Dismissed, false);
        assert_eq!(store.reply_target(), None, "dismissing clears the composer");
    }

    #[test]
    fn a_successful_send_schedules_the_close_behind_a_confirmation() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.type_reply('o');
        store.type_reply('k');

        assert!(store.send_reply());
        assert_eq!(store.unread(), 1, "the toast lingers to show the confirmation");
        assert_eq!(store.reply_target(), None, "the keyboard is released at once");
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Sent);
        assert!(store.is_animating(), "the pump must run until the close fires");
        assert!(!store.tick(), "the close does not fire early");

        store.measure(&painter());
        store.layout_slots(output());
        assert!(
            !store
                .slots
                .iter()
                .any(|(hit, _)| matches!(hit, Hit::Reply(1) | Hit::Send(1) | Hit::Button(1, _) | Hit::Settings(1))),
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
        assert!(store.is_animating(), "the pump must run while the field is flagged");

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
        assert!(store.is_animating(), "the flag alone keeps the pump running");

        store.live[0].reply_invalid_since = Instant::now().checked_sub(glide_theme::motion::t3());
        assert!(!store.live[0].reply_invalid());
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Composing);
        store.live[0].shown_at = Instant::now()
            .checked_sub(glide_theme::motion::T4)
            .expect("the monotonic clock has run for longer than one t4");
        store.live[0].ring.jump(1.0);
        assert!(!store.is_animating());
        assert_eq!(store.reply_target(), Some(1), "expiry reverts the border, not the draft");
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
        assert!(!store.open_reply(1), "a sent toast does not reopen its composer");
    }

    #[test]
    fn the_invalid_border_takes_the_danger_colour_and_changes_the_revision() {
        let theme = NotificationTheme::default();
        assert_eq!(theme.reply_invalid_border, theme.danger);
        let recoloured =
            NotificationTheme { reply_invalid_border: Color([1.0, 0.0, 0.0, 1.0]), ..NotificationTheme::default() };
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

        assert_eq!(store.action_at(centre(slot(&store, Hit::Send(1)))), Some(NotificationAction::SendReply(1)));
    }

    #[test]
    fn a_repliable_notification_shows_a_reply_target_before_its_other_buttons() {
        let (mut store, mut icons) = store();
        let mut with_both = repliable(1);
        with_both.actions = vec![glide_notify::Action { key: "archive".to_string(), label: "Archive".to_string() }];
        store.push(&mut icons, with_both, false);
        store.measure(&painter());
        store.layout_slots(output());

        let reply = slot(&store, Hit::Reply(1));
        let archive = slot(&store, Hit::Button(1, 0));
        assert!(reply.loc.x + reply.size.w <= archive.loc.x);
    }

    #[test]
    fn one_app_collapses_into_one_counted_stack_within_its_budget() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        assert_eq!(count_badge(&store.live[0]), None, "a lone toast counts nothing");
        let second =
            Notification { desktop_entry: "org.telegram.desktop".to_string(), ..from_app(2, "Telegram Desktop") };
        let first = Notification { desktop_entry: "org.telegram.desktop".to_string(), ..from_app(1, "Telegram") };
        let (mut grouped, mut gicons) = self::store();
        grouped.push(&mut gicons, first, false);
        grouped.push(&mut gicons, second, false);
        assert_eq!(grouped.live.len(), 1, "grouping prefers the desktop entry");

        store.push(&mut icons, from_app(2, "Telegram"), false);
        assert_eq!(count_badge(&store.live[0]).as_deref(), Some("+1"));
        store.push(&mut icons, from_app(3, "Telegram"), false);
        assert_eq!(count_badge(&store.live[0]).as_deref(), Some("+2"));
        assert_eq!(store.live.len(), 1, "one toast per application");
        assert_eq!(store.live[0].notification.id, 3, "the newest leads");
        assert_eq!(stacked_ids(&store.live[0]), vec![2, 1]);
        assert_eq!(store.unread(), 3, "the badge still counts every message");

        store.push(&mut icons, from_app(4, "Mail"), false);
        assert_eq!(store.live.len(), 2, "different apps keep their own toasts");
        let items = store.nc_items();
        let count = |app: &str| items.iter().find(|item| item.app_name == app).map(|i| i.count);
        assert_eq!((items.len(), count("Telegram"), count("Mail")), (2, Some(3), Some(1)));

        let (mut store, mut icons) = self::store();
        let overflow = u32::try_from(MAX_STACKED).expect("the budget fits a u32") + 4;
        for id in 1..=overflow {
            store.push(&mut icons, from_app(id, "Telegram"), false);
        }
        assert_eq!(store.live.len(), 1);
        assert_eq!(store.live[0].stacked.len(), MAX_STACKED);
        store.measure(&painter());
        assert_eq!(peek_layers(&store.live[0]), PEEK_MAX);
        assert_eq!(occupied_height(&store.live[0]), store.live[0].height + PEEK_REVEAL * 2);
    }

    #[test]
    fn toasts_and_their_peeks_never_share_a_row_and_stay_inside_the_rail() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.measure(&painter());
        assert_eq!(occupied_height(&store.live[0]), store.live[0].height);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.measure(&painter());
        assert_eq!(occupied_height(&store.live[0]), store.live[0].height + PEEK_REVEAL);

        for (id, app) in [(3, "Mail"), (4, "Files"), (5, "Calendar")] {
            let item = Notification {
                body: "a body long enough to wrap onto more than one line ".repeat(id as usize),
                ..from_app(id, app)
            };
            store.push(&mut icons, item, false);
        }
        store.measure(&painter());
        store.layout_slots(output());
        let rail = store.rect(output());
        let mut rows: Vec<(Rectangle<i32, Logical>, i32)> = store
            .live
            .iter()
            .map(|entry| {
                let body = slot(&store, Hit::Body(entry.notification.id));
                assert_eq!(body.size.h, entry.height, "hit box and buffer agree");
                assert!(body.loc.y >= rail.loc.y);
                assert!(body.loc.y + body.size.h <= rail.loc.y + rail.size.h);
                (body, PEEK_REVEAL * peek_layers(entry) as i32)
            })
            .collect();
        assert_eq!(rows.len(), 4);
        rows.sort_by_key(|(rect, _)| rect.loc.y);
        for pair in rows.windows(2) {
            let ((upper, reveal), (lower, _)) = (pair[0], pair[1]);
            assert!(upper.loc.y + upper.size.h + reveal <= lower.loc.y, "{pair:?}");
        }
    }

    #[test]
    fn closing_the_top_of_a_stack_reveals_the_one_beneath_measured_and_timed_afresh() {
        let (mut store, mut icons) = store();
        let tall = Notification {
            body: "a body long enough to wrap over several lines ".repeat(4),
            ..from_app(1, "Telegram")
        };
        store.push(&mut icons, tall, false);
        std::thread::sleep(Duration::from_millis(40));
        store.push(&mut icons, from_app(2, "Telegram"), false);
        let short = Notification { body: "short".to_string(), ..from_app(3, "Telegram") };
        store.push(&mut icons, short, false);
        store.measure(&painter());
        let short_height = store.live[0].height;

        assert!(store.remove(3, CloseReason::Dismissed, false));
        assert_eq!(store.live[0].notification.id, 2, "the previous message shows");
        assert_eq!(store.unread(), 2);
        assert!(store.live[0].shown_at.elapsed() < Duration::from_millis(20));
        assert!(store.remove(2, CloseReason::Dismissed, false));
        store.measure(&painter());
        assert!(store.live[0].height > short_height, "measured afresh");
        assert!(store.remove(1, CloseReason::Dismissed, false));
        assert!(store.is_empty(), "only the last one empties the rail");
    }

    #[test]
    fn a_replacement_keeps_the_entrance_and_the_typed_reply_but_restarts_expiry() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        settle_entry(&mut store);
        store.open_reply(1);
        store.type_reply('h');
        let shown_at = store.live[0].shown_at;
        std::thread::sleep(Duration::from_millis(5));

        let mut update = repliable(1);
        update.body = "An edited body".to_string();
        assert!(store.push(&mut icons, update, true));

        let entry = &store.live[0];
        assert_eq!(entry.shown_at, shown_at, "the toast must not replay its entrance");
        assert_eq!(entry.reply_text.as_deref(), Some("h"));
        assert!(entry.updated_at > shown_at, "expiry restarts on replacement");
        assert_eq!(entry.notification.body, "An edited body");
    }

    #[test]
    fn expiry_walks_down_the_stack_rather_than_dropping_it() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);

        std::thread::sleep(Duration::from_millis(80));
        assert!(store.tick());
        assert_eq!(store.live[0].notification.id, 1);
        assert_eq!(store.active().count(), 1);
        assert_eq!(store.archive.len(), 1, "the expired head waits in the center");
        assert_eq!(store.unread(), 2);
    }

    #[test]
    fn buried_messages_close_without_touching_the_visible_one_and_clear_with_the_rail() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, from_app(2, "Telegram"), false);
        assert!(store.remove(1, CloseReason::Revoked, false));
        assert_eq!(store.live[0].notification.id, 2);
        assert_eq!(store.unread(), 1);
        assert!(store.live[0].stacked.is_empty());

        store.push(&mut icons, from_app(3, "Telegram"), false);
        store.push(&mut icons, from_app(4, "Mail"), false);
        assert!(store.clear_all());
        assert!(store.is_empty() && store.unread() == 0);
    }

    #[test]
    fn a_repeat_of_the_same_message_is_swallowed() {
        let (mut store, mut icons) = store();
        assert!(store.push(&mut icons, from_app(1, "Backup"), false));

        let mut repeat = from_app(2, "Backup");
        repeat.summary = store.live[0].notification.summary.clone();
        assert!(!store.push(&mut icons, repeat, false), "an identical body and summary is not news");
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
    fn a_configure_chip_comes_last_and_only_when_offered() {
        let (mut store, mut icons) = store();
        let configurable = Notification {
            has_settings_action: true,
            actions: vec![glide_notify::Action { key: "open".to_string(), label: "Open".to_string() }],
            ..notification(1)
        };
        store.push(&mut icons, configurable, false);
        store.push(&mut icons, notification(2), false);
        store.measure(&painter());
        store.layout_slots(output());
        let open = slot(&store, Hit::Button(1, 0));
        assert!(open.loc.x + open.size.w <= slot(&store, Hit::Settings(1)).loc.x);
        assert!(!store.slots.iter().any(|(hit, _)| *hit == Hit::Settings(2)));
    }
}
