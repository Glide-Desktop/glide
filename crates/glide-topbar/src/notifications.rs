use std::time::{Duration, Instant};

use glide_notify::{CloseReason, Notification, Urgency};

use crate::popup_menu::text_px;

pub const ICON_SOURCE_PX: i32 = 96;
pub const PROMPT_CATEGORY: &str = "x-glide.prompt";
const MAX_BUTTONS: usize = 3;
const REPLY_MAX_CHARS: usize = 500;
const MAX_VISIBLE: usize = 4;
const MAX_STACKED: usize = 8;

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

#[derive(Debug)]
struct Entry {
    notification: Notification,
    updated_at: Instant,
    icon: Option<std::sync::Arc<Vec<u8>>>,
    reply_text: Option<String>,
    reply_invalid_since: Option<Instant>,
    reply_sent_at: Option<Instant>,
    stacked: Vec<Notification>,
    expiry: Expiry,
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

    fn close_composer(&mut self) -> bool {
        if self.reply_text.take().is_none() {
            return false;
        }
        self.reply_invalid_since = None;
        true
    }

    fn is_animating(&self) -> bool {
        self.reply_invalid() || self.reply_sent_at.is_some()
    }

    fn set_hovered(&mut self, hovered: bool, now: Instant) {
        if hovered {
            self.expiry.pause(now);
        } else {
            self.expiry.resume(now);
        }
    }

    fn holds_prompt(&self) -> bool {
        is_prompt(&self.notification) || self.stacked.iter().any(is_prompt)
    }

    fn rank(&self) -> u8 {
        if is_prompt(&self.notification) { 2 } else { u8::from(self.notification.urgency == Urgency::Critical) }
    }

    fn owns(&self, id: u32) -> bool {
        self.notification.id == id || self.stacked.iter().any(|notification| notification.id == id)
    }

    fn ids(&self) -> Vec<u32> {
        let mut ids = vec![self.notification.id];
        ids.extend(self.stacked.iter().map(|notification| notification.id));
        ids
    }

    fn promote(&mut self) -> bool {
        if self.stacked.is_empty() {
            return false;
        }
        self.notification = self.stacked.remove(0);
        let now = Instant::now();
        self.updated_at = now;
        self.expiry = self.expiry.restarted(now);
        self.reply_text = None;
        self.reply_invalid_since = None;
        self.reply_sent_at = None;
        true
    }
}

const MAX_ARCHIVED: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPolicy {
    pub do_not_disturb: bool,
    pub schedule: Option<(u16, u16)>,
    pub allow_critical: bool,
    pub banner_timeout: Option<Duration>,
    pub muted: Vec<String>,
    pub silent: Vec<String>,
    pub sounds: bool,
    pub system: SystemNotices,
}

#[allow(clippy::struct_excessive_bools)] // One switch per kind of notice Glide raises itself
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemNotices {
    pub drives: bool,
    pub disk_space: bool,
    pub headsets: bool,
    pub codes: bool,
    pub quiet_while_sharing: bool,
    pub private_while_sharing: bool,
}

impl Default for SystemNotices {
    fn default() -> Self {
        Self {
            drives: true,
            disk_space: true,
            headsets: true,
            codes: true,
            quiet_while_sharing: true,
            private_while_sharing: true,
        }
    }
}

#[must_use]
pub fn redacted(items: Vec<NcItem>, hide: bool) -> Vec<NcItem> {
    if !hide {
        return items;
    }
    let hidden = crate::i18n::t(crate::i18n::Key::HiddenWhileSharing);
    items.into_iter().map(|item| NcItem { body: hidden.to_string(), ..item }).collect()
}

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            do_not_disturb: false,
            schedule: None,
            allow_critical: true,
            banner_timeout: None,
            muted: Vec::new(),
            silent: Vec::new(),
            sounds: true,
            system: SystemNotices::default(),
        }
    }
}

impl NotificationPolicy {
    #[must_use]
    pub fn mutes(&self, notification: &Notification) -> bool {
        listed(&self.muted, notification)
    }

    #[must_use]
    pub fn hushes(&self, notification: &Notification) -> bool {
        listed(&self.silent, notification)
    }
}

fn listed(list: &[String], notification: &Notification) -> bool {
    let names = [notification.desktop_entry.trim(), notification.app_name.trim()];
    list.iter().any(|muted| {
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

impl NotificationPolicy {
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
}

#[derive(Debug, Clone)]
struct Archived {
    notification: Notification,
    received_at: Instant,
    icon: Option<std::sync::Arc<Vec<u8>>>,
    read: bool,
}

pub const LOCAL_ID_FLOOR: u32 = u32::MAX - 4096;

const MAX_GLANCES: usize = 8;

#[must_use]
pub const fn is_local(id: u32) -> bool {
    id >= LOCAL_ID_FLOOR
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glance {
    pub item: NcItem,
    pub replaces: bool,
}

#[must_use]
pub fn glances(notification: &Notification) -> bool {
    visible_buttons(notification).is_empty()
        && notification.reply.is_none()
        && !notification.resident
        && notification.urgency != Urgency::Critical
}

fn nc_item(notification: &Notification, age: Duration, icon: Option<&std::sync::Arc<Vec<u8>>>, count: usize) -> NcItem {
    NcItem {
        id: notification.id,
        app_name: notification.app_name.clone(),
        title: notification.summary.clone(),
        body: notification.body.clone(),
        time: crate::i18n::elapsed_label(age.as_secs()),
        icon: icon.cloned(),
        count,
    }
}

fn is_prompt(notification: &Notification) -> bool {
    is_local(notification.id) && notification.category == PROMPT_CATEGORY
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeButton {
    Reply(String),
    Action(usize, String),
    Settings(String),
}

impl NoticeButton {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Reply(label) | Self::Action(_, label) | Self::Settings(label) => label,
        }
    }

    #[must_use]
    pub const fn action(&self, id: u32) -> NotificationAction {
        match self {
            Self::Reply(_) => NotificationAction::OpenReply(id),
            Self::Action(index, _) => NotificationAction::Button(id, *index),
            Self::Settings(_) => NotificationAction::Configure(id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeReply {
    Composing { shown: String, placeholder: String, invalid: bool },
    Sent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub id: u32,
    pub app: String,
    pub title: String,
    pub body: String,
    pub icon: Option<std::sync::Arc<Vec<u8>>>,
    pub buttons: Vec<NoticeButton>,
    pub reply: Option<NoticeReply>,
    pub urgent: bool,
    pub asks: bool,
    pub waiting: usize,
}

impl Notice {
    #[must_use]
    pub fn wants_attention(&self) -> bool {
        !self.buttons.is_empty() || self.reply.is_some() || self.waiting > 0
    }
}

impl Notice {
    fn of(entry: &Entry, waiting: usize) -> Self {
        let notification = &entry.notification;
        let reply_button = notification.reply.as_ref().map(|reply| NoticeButton::Reply(reply.label.clone()));
        let actions = visible_buttons(notification)
            .iter()
            .enumerate()
            .map(|(index, action)| NoticeButton::Action(index, action.label.clone()));
        let settings = notification
            .has_settings_action
            .then(|| NoticeButton::Settings(crate::i18n::t(crate::i18n::Key::Configure).to_string()));
        let reply = match entry.reply_phase() {
            ReplyPhase::Closed => None,
            ReplyPhase::Sent => Some(NoticeReply::Sent),
            phase @ (ReplyPhase::Composing | ReplyPhase::Invalid) => Some(NoticeReply::Composing {
                shown: visible_reply(entry, entry.reply_text.as_deref().unwrap_or_default()),
                placeholder: reply_placeholder(entry),
                invalid: phase == ReplyPhase::Invalid,
            }),
        };
        Self {
            id: notification.id,
            app: notification.display_app().to_string(),
            title: notification.summary.clone(),
            body: notification.body.clone(),
            icon: entry.icon.clone(),
            buttons: reply_button.into_iter().chain(actions).chain(settings).collect(),
            reply,
            urgent: notification.urgency == Urgency::Critical && !is_prompt(notification),
            asks: is_prompt(notification),
            waiting,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalEvent {
    Action { id: u32, key: String },
    Reply { id: u32, text: String },
    Closed { id: u32 },
}

#[derive(Debug, Default)]
pub struct Notifications {
    handle: Option<glide_notify::NotifyHandle>,
    local: Vec<LocalEvent>,
    live: Vec<Entry>,
    archive: Vec<Archived>,
    dnd: bool,
    inhibited: bool,
    policy: NotificationPolicy,
    policy_applied: bool,
    scheduled_dnd: Option<bool>,
    held: Option<u32>,
    locked_at: Option<Instant>,
    banners: Banners,
    pending_glances: Vec<Glance>,
    senders: SenderLog,
}

#[derive(Debug, Default)]
struct SenderLog {
    list: glide_config::senders::Senders,
    unsaved: bool,
    saved_at: Option<Instant>,
}

const SENDERS_SAVE_EVERY: Duration = Duration::from_secs(30);

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Banners {
    #[default]
    Island,
    #[cfg(test)]
    Toasts,
}

impl Notifications {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_senders(&mut self, senders: glide_config::senders::Senders) {
        self.senders = SenderLog { list: senders, ..SenderLog::default() };
    }

    fn note_sender(&mut self, notification: &Notification) {
        if is_local(notification.id) {
            return;
        }
        if let Some(key) = glide_config::senders::key(&notification.desktop_entry, &notification.app_name) {
            self.senders.list.note(&key, &notification.app_name, unix_now());
            self.senders.unsaved = true;
        }
    }

    pub fn senders_to_save(&mut self, now: Instant) -> Option<glide_config::senders::Senders> {
        let log = &mut self.senders;
        let due = log.saved_at.is_none_or(|saved| now.duration_since(saved) >= SENDERS_SAVE_EVERY);
        if !log.unsaved || !due {
            return None;
        }
        log.unsaved = false;
        log.saved_at = Some(now);
        Some(log.list.clone())
    }

    #[cfg(test)]
    pub const fn set_glances(&mut self, on: bool) {
        self.banners = if on { Banners::Island } else { Banners::Toasts };
    }

    pub fn take_glances(&mut self) -> Vec<Glance> {
        std::mem::take(&mut self.pending_glances)
    }

    pub fn attach(&mut self, handle: glide_notify::NotifyHandle) {
        self.handle = Some(handle);
    }

    pub fn take_local_events(&mut self) -> Vec<LocalEvent> {
        std::mem::take(&mut self.local)
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
        let wanted =
            self.policy.sounds && notification.sound != glide_notify::Sound::None && !self.policy.hushes(notification);
        (wanted && !self.silences(notification)).then(|| notification.sound.clone())
    }

    #[must_use]
    pub fn silences(&self, notification: &Notification) -> bool {
        let critical = notification.urgency == Urgency::Critical;
        if critical && is_local(notification.id) {
            return false;
        }
        let breaks_through = critical && self.policy.allow_critical;
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
        self.live.iter()
    }

    fn active_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
        self.live.iter_mut()
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
        let age = |at: Instant| now.duration_since(at);
        self.active()
            .map(|entry| nc_item(&entry.notification, age(entry.updated_at), entry.icon.as_ref(), entry.group_size()))
            .chain(
                self.archive
                    .iter()
                    .map(|kept| nc_item(&kept.notification, age(kept.received_at), kept.icon.as_ref(), 1)),
            )
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

    #[must_use]
    pub fn notice(&self) -> Option<Notice> {
        let top = self.active().map(Entry::rank).max()?;
        let head = self.active().find(|entry| entry.rank() == top)?;
        let total: usize = self.active().map(Entry::group_size).sum();
        Some(Notice::of(head, total - 1))
    }

    pub fn hold(&mut self, id: Option<u32>) {
        self.held = id;
        let now = Instant::now();
        let locked = self.locked_at.is_some();
        for entry in self.active_mut() {
            let held = id == Some(entry.notification.id);
            entry.set_hovered(held || locked, now);
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
                changed = true;
            } else if !wants {
                changed |= entry.close_composer();
            }
        }
        changed
    }

    pub fn cancel_reply(&mut self) -> bool {
        let mut changed = false;
        for entry in self.active_mut() {
            changed |= entry.close_composer();
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
        true
    }

    pub fn backspace_reply(&mut self) -> bool {
        let Some(text) = self.reply_buffer() else {
            return false;
        };
        text.pop().is_some()
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

    pub fn settle(&mut self) -> bool {
        self.retire_sent();
        self.is_animating()
    }

    fn leave(&mut self, index: usize) {
        self.live.remove(index);
    }

    fn leave_by_id(&mut self, id: u32) -> bool {
        let Some(index) = self.live.iter().position(|entry| entry.notification.id == id) else {
            return false;
        };
        self.leave(index);
        true
    }

    fn evict_overflow(&mut self) {
        while self.live.len() > MAX_VISIBLE {
            let Some(last) = (0..self.live.len())
                .rev()
                .filter(|index| !self.live[*index].holds_prompt())
                .min_by_key(|index| self.live[*index].rank())
            else {
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
        self.live.iter().any(Entry::is_animating)
    }

    fn glance(&mut self, icons: &mut crate::icon::IconLoader, notification: Notification, replaces: bool) {
        let icon = load_icon(icons, &notification);
        self.remove(notification.id, CloseReason::Expired, false);
        let item = nc_item(&notification, Duration::ZERO, icon.as_ref(), 1);
        self.pending_glances.push(Glance { item, replaces });
        if self.pending_glances.len() > MAX_GLANCES {
            self.pending_glances.remove(0);
        }
        self.keep(notification, Instant::now(), icon);
    }

    pub fn push(&mut self, icons: &mut crate::icon::IconLoader, notification: Notification, replaces: bool) -> bool {
        let notification = self.policy.shape(notification);
        if !replaces {
            self.note_sender(&notification);
        }
        self.forget_kept(notification.id);
        if self.silences(&notification) {
            let icon = load_icon(icons, &notification);
            self.keep(notification, Instant::now(), icon);
            return true;
        }
        if self.banners == Banners::Island && glances(&notification) {
            self.glance(icons, notification, replaces);
            return true;
        }

        if !replaces && self.active().any(|entry| entry.notification.is_duplicate_of(&notification)) {
            self.notify_closed(notification.id, CloseReason::Expired);
            return false;
        }

        let id = notification.id;
        let group = if is_prompt(&notification) { String::new() } else { notification.group_key() };
        let icon = load_icon(icons, &notification);
        let now = Instant::now();
        let mut entry = Entry {
            notification,
            updated_at: now,
            icon,
            reply_text: None,
            reply_invalid_since: None,
            reply_sent_at: None,
            stacked: Vec::new(),
            expiry: Expiry::new(now),
        };
        if self.locked_at.is_some() {
            entry.expiry.pause(now);
        }

        if replaces && let Some(slot) = self.live.iter_mut().find(|slot| slot.owns(id)) {
            if slot.notification.id == id {
                entry.stacked = std::mem::take(&mut slot.stacked);
                entry.expiry = slot.expiry.restarted(now);
                if entry.notification.reply.is_some() {
                    entry.reply_text = slot.reply_text.take();
                    entry.reply_invalid_since = slot.reply_invalid_since;
                    entry.reply_sent_at = slot.reply_sent_at;
                }
                *slot = entry;
            } else {
                for stacked in &mut slot.stacked {
                    if stacked.id == id {
                        *stacked = entry.notification.clone();
                    }
                }
            }
            return true;
        }

        self.live.retain(|slot| !slot.owns(id));

        if let Some(index) = self.live.iter().position(|slot| {
            !group.is_empty() && !is_prompt(&slot.notification) && slot.notification.group_key() == group
        }) {
            let previous = self.live.remove(index);
            entry.stacked = std::iter::once(previous.notification)
                .chain(previous.stacked)
                .filter(|notification| notification.id != id)
                .collect();
            entry.expiry = if previous.expiry.is_paused() { previous.expiry.restarted(now) } else { entry.expiry };
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
        let Some(index) = self.live.iter().position(|entry| entry.owns(id)) else {
            return false;
        };

        let head = self.live[index].notification.id;
        if head != id {
            self.live[index].stacked.retain(|notification| notification.id != id);
        } else if !self.live[index].promote() {
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

    pub fn withdraw(&mut self, id: u32, reason: CloseReason) -> bool {
        let removed = self.remove(id, reason, true);
        let forgotten = self.forget_kept(id);
        removed || forgotten
    }

    pub fn tick(&mut self) -> bool {
        let scheduled = self.policy.schedule.is_some() && self.follow_schedule(crate::local_minute_of_day());
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
        changed || sent
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

    pub fn activate(&mut self, id: u32, token: Option<&str>) -> Option<String> {
        let Some(entry) = self.active().find(|entry| entry.notification.id == id) else {
            return self.activate_kept(id, token);
        };
        let has_default = entry.notification.has_default_action;
        let resident = entry.notification.resident;
        let app = app_to_raise(&entry.notification);
        if let Some(handle) = self.handle.as_ref()
            && has_default
        {
            handle.invoked(id, glide_notify::ACTION_DEFAULT, token);
        }
        if !resident {
            self.remove(id, CloseReason::Dismissed, true);
        }
        app
    }

    fn activate_kept(&mut self, id: u32, token: Option<&str>) -> Option<String> {
        let kept = self.archive.iter().find(|kept| kept.notification.id == id)?;
        let app = app_to_raise(&kept.notification);
        if let Some(handle) = self.handle.as_ref()
            && kept.notification.has_default_action
        {
            handle.invoked(id, glide_notify::ACTION_DEFAULT, token);
        }
        self.forget_kept(id);
        app
    }

    pub fn set_locked(&mut self, locked: bool) {
        if locked == self.locked_at.is_some() {
            return;
        }
        let now = Instant::now();
        self.locked_at = locked.then_some(now);
        let held = self.held;
        for entry in &mut self.live {
            if locked {
                entry.expiry.pause(now);
            } else if held != Some(entry.notification.id) {
                entry.expiry.resume(now);
            }
        }
    }
}

fn app_to_raise(notification: &Notification) -> Option<String> {
    if notification.has_default_action || is_local(notification.id) {
        return None;
    }
    [&notification.desktop_entry, &notification.app_name]
        .into_iter()
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
        .map(str::to_string)
}

fn visible_buttons(notification: &Notification) -> &[glide_notify::Action] {
    let count = notification.actions.len().min(MAX_BUTTONS);
    &notification.actions[..count]
}

fn load_icon(icons: &mut crate::icon::IconLoader, notification: &Notification) -> Option<std::sync::Arc<Vec<u8>>> {
    load_icon_bytes(icons, notification).map(std::sync::Arc::new)
}

fn load_icon_bytes(icons: &mut crate::icon::IconLoader, notification: &Notification) -> Option<Vec<u8>> {
    if let Some(bytes) = &notification.image_data {
        if bytes.len() == (ICON_SOURCE_PX as usize) * (ICON_SOURCE_PX as usize) * 4 {
            return Some(bytes.clone());
        }
        if let Some(rendered) = glide_paint::render_png_rounded(bytes, ICON_SOURCE_PX, 8.0) {
            return Some(rendered);
        }
    }
    if !notification.app_icon.trim().is_empty()
        && let Some(bytes) = icons.load_named(&notification.app_icon, ICON_SOURCE_PX)
    {
        return Some(bytes.to_vec());
    }
    if !notification.desktop_entry.trim().is_empty()
        && let Some(bytes) = icons.load(&notification.desktop_entry, ICON_SOURCE_PX)
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
        let last = lines.len() - 1;
        if used < remaining && !lines[last].ends_with('…') {
            lines[last] = ellipsise(&lines[last], &fits);
        }
    }

    lines
}

#[must_use]
pub fn wrap_paragraphs(
    painter: &crate::paint::Painter, text: &str, style: &glide_theme::TextStyle, max_width: i32, max_lines: usize,
) -> Vec<String> {
    let paragraphs: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    let fits = |candidate: &str| text_px(painter, candidate, style) <= max_width;
    let mut lines: Vec<String> = Vec::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let room = max_lines.saturating_sub(lines.len());
        let mut wrapped = wrap_lines(painter, paragraph, style, max_width, room);
        let cut = wrapped.len() == room && index + 1 < paragraphs.len();
        lines.append(&mut wrapped);
        if cut {
            if let Some(last) = lines.last_mut()
                && !last.ends_with('…')
            {
                *last = ellipsise(last, &fits);
            }
            break;
        }
    }
    lines
}

fn clip_word(word: &str, fits: &impl Fn(&str) -> bool) -> String {
    if fits(word) { word.to_string() } else { ellipsise(word, fits) }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut store = Notifications::new();
        store.set_glances(false);
        (store, crate::icon::IconLoader::new(None))
    }

    #[test]
    fn plain_notifications_glance_and_the_interactive_ones_wait_in_the_live_queue() {
        let (mut store, mut icons) = store();
        store.set_glances(true);
        assert!(store.push(&mut icons, from_app(1, "Telegram"), false));
        assert!(store.live.is_empty(), "no toast for a plain notification");
        assert_eq!(store.unread(), 1, "it waits in the centre");
        assert_eq!(store.nc_items().first().map(|item| item.id), Some(1));
        let glances = store.take_glances();
        assert_eq!(glances.len(), 1);
        assert_eq!((glances[0].item.id, glances[0].replaces), (1, false));
        assert!(store.take_glances().is_empty(), "each glance is handed over once");

        let buttons = Notification {
            actions: vec![glide_notify::Action { key: "open".into(), label: "Open".into() }],
            ..notification(2)
        };
        let critical = Notification { urgency: Urgency::Critical, ..notification(3) };
        let resident = Notification { resident: true, ..notification(4) };
        for interactive in [repliable(5), buttons, critical, resident] {
            let id = interactive.id;
            store.push(&mut icons, interactive, false);
            assert!(store.live.iter().any(|entry| entry.owns(id)), "{id} keeps its toast");
        }
        assert!(store.take_glances().is_empty());
    }

    #[test]
    fn image_data_in_notification_becomes_entry_icon() {
        let (mut store, mut icons) = store();
        let pixel_bytes = vec![0x80; (ICON_SOURCE_PX as usize) * (ICON_SOURCE_PX as usize) * 4];
        let mut note = Notification {
            actions: vec![glide_notify::Action { key: "open".into(), label: "Open".into() }],
            ..notification(42)
        };
        note.image_data = Some(pixel_bytes.clone());
        assert!(store.push(&mut icons, note, false));
        let entry = store.live.iter().find(|e| e.notification.id == 42).expect("stored");
        assert_eq!(entry.icon.as_deref().map(Vec::as_slice), Some(pixel_bytes.as_slice()));
    }

    #[test]
    fn the_centre_hides_bodies_only_while_asked_to() {
        let item = NcItem {
            id: 1,
            app_name: "Telegram".into(),
            title: "Olena".into(),
            body: "The door code is 4821".into(),
            time: "now".into(),
            icon: None,
            count: 1,
        };
        assert_eq!(redacted(vec![item.clone()], false)[0].body, item.body);
        let hidden = redacted(vec![item.clone()], true);
        assert_ne!(hidden[0].body, item.body);
        assert_eq!((hidden[0].title.as_str(), hidden[0].app_name.as_str()), ("Olena", "Telegram"));
    }

    #[test]
    fn a_silent_app_keeps_its_banner_but_plays_no_sound() {
        let (mut store, _) = store();
        store.set_policy(NotificationPolicy { silent: vec!["Telegram".into()], ..NotificationPolicy::default() });
        let loud = |app: &str| Notification {
            sound: glide_notify::Sound::Named("message-new-instant".into()),
            ..from_app(1, app)
        };
        assert_eq!(store.audible(&loud("Telegram")), None);
        assert!(!store.silences(&loud("Telegram")), "its banner still shows");
        assert!(store.audible(&loud("Firefox")).is_some());
    }

    #[test]
    fn senders_are_noted_once_per_new_notification_and_saved_at_most_every_half_minute() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        store.push(&mut icons, Notification { body: "edit".into(), ..from_app(1, "Telegram") }, true);
        store.push(&mut icons, Notification { id: LOCAL_ID_FLOOR, ..from_app(0, "Glide") }, false);
        let start = Instant::now();
        let saved = store.senders_to_save(start).expect("a new sender is due");
        assert_eq!(saved.apps.len(), 1, "local notices are not senders");
        assert_eq!(saved.apps["Telegram"].count, 1, "a replacement is not a new notification");
        store.push(&mut icons, from_app(2, "Telegram"), false);
        assert!(store.senders_to_save(start + Duration::from_secs(5)).is_none());
        assert!(store.senders_to_save(start + SENDERS_SAVE_EVERY).is_some());
    }

    #[test]
    fn a_glance_follows_replacements_silence_and_withdrawal() {
        let (mut store, mut icons) = store();
        store.set_glances(true);
        store.push(&mut icons, notification(1), false);
        store.push(&mut icons, Notification { body: "newer".into(), ..notification(1) }, true);
        let glances = store.take_glances();
        assert_eq!(glances.iter().map(|glance| glance.replaces).collect::<Vec<_>>(), [false, true]);
        assert_eq!(store.nc_items().len(), 1, "a replacement updates the kept one");
        assert_eq!(store.nc_items()[0].body, "newer");

        store.set_dnd(true);
        store.push(&mut icons, notification(2), false);
        assert!(store.take_glances().is_empty(), "Do Not Disturb keeps it quiet");
        assert_eq!(store.nc_items().len(), 2);

        assert!(store.withdraw(1, CloseReason::Revoked));
        assert!(store.nc_items().iter().all(|item| item.id != 1), "an app that closes it takes it back");
        assert!(!store.withdraw(1, CloseReason::Revoked));
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
        assert!(store.push(&mut icons, critical(LOCAL_ID_FLOOR + 1), false));
        assert_eq!(store.active().count(), 1, "the shell's own critical toasts, like a pairing prompt, always show");

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
        assert!(store.is_empty());
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
    fn toasts_wait_out_the_lock_screen_instead_of_expiring_unseen() {
        let (mut store, mut icons) = store();
        let quick = Notification { timeout: Some(Duration::from_millis(20)), ..notification(1) };
        store.push(&mut icons, quick, false);
        store.set_locked(true);
        let during = Notification { timeout: Some(Duration::from_millis(20)), ..notification(2) };
        store.push(&mut icons, during, false);
        store.hold(None);
        std::thread::sleep(Duration::from_millis(40));
        store.tick();
        assert_eq!(store.live.len(), 2, "nothing times out while locked, before or after the lock began");
        store.set_locked(false);
        std::thread::sleep(Duration::from_millis(40));
        store.tick();
        assert!(store.active().next().is_none(), "the clock runs again after unlocking");
    }

    #[test]
    fn evicting_moves_the_oldest_to_the_centre_and_clearing_empties_everything() {
        let (mut store, mut icons) = store();
        assert!(!store.clear_all(), "nothing to clear");
        let overflow = u32::try_from(MAX_VISIBLE).expect("the budget fits a u32") + 1;
        for id in 1..=overflow {
            store.push(&mut icons, notification(id), false);
        }
        assert_eq!(store.live.len(), MAX_VISIBLE);
        assert!(store.live.iter().all(|entry| entry.notification.id != 1));
        assert_eq!(store.archive.first().map(|kept| kept.notification.id), Some(1));
        assert!(store.clear_all() && store.is_empty());
    }

    const WIDTH: i32 = 300;
    const LINES: usize = 3;

    #[test]
    fn wrapping_fits_the_width_the_line_budget_and_clips_what_cannot_break() {
        let painter = painter();
        let style = glide_theme::TextStyle::CALL;
        let wrap = |body: &str, max| wrap_lines(&painter, body, &style, WIDTH, max);
        let long = wrap(&"lorem ipsum dolor sit amet ".repeat(40), LINES);
        assert_eq!(long.len(), LINES);
        assert!(long.last().is_some_and(|line| line.ends_with('…')));
        for line in wrap("the quick brown fox jumps over the lazy dog again and again", LINES) {
            assert!(text_px(&painter, &line, &style) <= WIDTH, "{line:?}");
        }
        assert_eq!(wrap("short", LINES), vec!["short".to_string()]);
        let word = wrap(&"x".repeat(400), 1);
        assert_eq!(word.len(), 1);
        assert!(text_px(&painter, &word[0], &style) <= WIDTH);
        assert!(word[0].ends_with('…'), "a clipped word says so");
        let middle = wrap(&format!("see {} for details", "y".repeat(400)), LINES);
        assert!(middle.iter().any(|line| line.starts_with('y') && line.ends_with('…')), "{middle:?}");
        assert!(wrap("   ", LINES).is_empty());
    }

    #[test]
    fn body_paragraphs_keep_their_line_breaks_and_the_cut_one_ends_in_an_ellipsis() {
        let painter = painter();
        let wrap = |text: &str, max| wrap_paragraphs(&painter, text, &glide_theme::TextStyle::CALL, WIDTH, max);
        let body = "Allow it only if you trust it.\n/usr/lib/signal-desktop/signal-desktop";
        assert_eq!(wrap(body, LINES), ["Allow it only if you trust it.", "/usr/lib/signal-desktop/signal-desktop"]);
        let cut = wrap("first paragraph\nsecond paragraph", 1);
        assert_eq!(cut.len(), 1);
        assert!(cut[0].ends_with('…'), "{cut:?}");
        assert!(wrap("\n\n  \n", LINES).is_empty());
    }

    #[test]
    fn the_island_notice_is_the_head_toast_with_its_buttons_reply_state_and_queue() {
        let (mut store, mut icons) = store();
        assert_eq!(store.notice(), None);
        let ask = LOCAL_ID_FLOOR + 1;
        let question = Notification {
            category: PROMPT_CATEGORY.into(),
            urgency: Urgency::Critical,
            actions: vec![
                glide_notify::Action { key: "accept".into(), label: "Allow".into() },
                glide_notify::Action { key: "decline".into(), label: "Cancel".into() },
            ],
            timeout: None,
            ..from_app(ask, "Glide")
        };
        assert!(store.push(&mut icons, question, false));
        let notice = store.notice().expect("notice");
        assert_eq!(notice.id, ask);
        assert_eq!(notice.buttons, [NoticeButton::Action(0, "Allow".into()), NoticeButton::Action(1, "Cancel".into())]);
        assert!(!notice.urgent, "a question is not an alarm");
        assert_eq!(notice.waiting, 0);

        assert!(store.push(&mut icons, repliable(2), false));
        assert_eq!(store.notice().map(|notice| (notice.id, notice.waiting)), Some((ask, 1)), "the question leads");
        assert!(store.remove(ask, CloseReason::Dismissed, true));
        assert!(store.push(&mut icons, repliable(3), false));
        let notice = store.notice().expect("notice");
        assert_eq!((notice.id, notice.waiting), (3, 1), "otherwise the newest leads and the other waits");
        assert_eq!(notice.buttons.first(), Some(&NoticeButton::Reply("Reply".into())));
        assert!(store.open_reply(3) && store.type_reply('h'));
        assert!(matches!(
            store.notice().and_then(|notice| notice.reply),
            Some(NoticeReply::Composing { shown, .. }) if shown == "h"
        ));
    }

    #[test]
    fn questions_lead_the_queue_never_stack_and_are_never_evicted() {
        let (mut store, mut icons) = store();
        let (first, second) = (LOCAL_ID_FLOOR + 1, LOCAL_ID_FLOOR + 2);
        let question = |id: u32| Notification {
            category: PROMPT_CATEGORY.into(),
            urgency: Urgency::Critical,
            timeout: None,
            ..from_app(id, "Glide")
        };
        assert!(store.push(&mut icons, question(first), false));
        assert!(store.push(&mut icons, question(second), false));
        let battery = Notification { urgency: Urgency::Critical, timeout: None, ..from_app(3, "Glide") };
        assert!(store.push(&mut icons, battery, false));
        assert_eq!(
            store.live.iter().map(|entry| entry.stacked.len()).sum::<usize>(),
            0,
            "nothing stacks on a question"
        );
        assert_eq!(store.notice().map(|notice| (notice.id, notice.waiting)), Some((second, 2)));

        for id in 10..20 {
            assert!(store.push(
                &mut icons,
                Notification { timeout: None, ..from_app(id, &format!("App {id}")) },
                false
            ));
        }
        let kept: Vec<u32> = store.active().map(|entry| entry.notification.id).collect();
        assert!(kept.contains(&first) && kept.contains(&second), "{kept:?}");
        assert_eq!(store.notice().map(|notice| notice.id), Some(second), "newer chatter never jumps a question");
        assert!(
            store.remove(second, CloseReason::Dismissed, true) && store.remove(first, CloseReason::Dismissed, true)
        );
        assert_eq!(store.notice().map(|notice| notice.id), Some(3), "then the critical notice");
    }

    #[test]
    fn an_app_cannot_dress_its_notification_up_as_a_shell_question() {
        let (mut store, mut icons) = store();
        let spoof = |id: u32| Notification {
            category: PROMPT_CATEGORY.into(),
            app_name: format!("Wi-Fi {id}"),
            ..repliable(id)
        };
        for id in 1..=8 {
            assert!(store.push(&mut icons, spoof(id), false));
        }
        assert_eq!(store.live.len(), MAX_VISIBLE, "a remote notification is evicted like any other");
        assert!(store.notice().is_some_and(|notice| !notice.asks), "and never shown as a question");
    }

    #[test]
    fn holding_the_island_notice_freezes_its_timeout_and_a_sent_reply_retires_without_toasts() {
        let (mut store, mut icons) = store();
        assert!(store.push(&mut icons, notification(1), false));
        store.hold(Some(1));
        std::thread::sleep(Duration::from_millis(80));
        store.tick();
        assert!(store.notice().is_some(), "held past its 50 ms timeout");
        store.hold(None);
        std::thread::sleep(Duration::from_millis(80));
        store.tick();
        assert!(store.notice().is_none());

        assert!(store.push(&mut icons, repliable(2), false));
        assert!(store.open_reply(2) && store.type_reply('k') && store.send_reply());
        if let Some(entry) = store.live.iter_mut().find(|entry| entry.notification.id == 2) {
            entry.reply_sent_at = Instant::now().checked_sub(glide_theme::motion::t4());
        }
        store.settle();
        assert!(store.notice().is_none(), "settle retires the sent reply now that render may not run");
    }

    #[test]
    fn a_click_without_a_default_action_raises_the_app_instead() {
        let (mut store, mut icons) = store();
        store.set_glances(true);
        let plain = Notification { desktop_entry: "org.telegram.desktop".into(), ..from_app(1, "Telegram") };
        assert!(store.push(&mut icons, plain, false));
        assert_eq!(store.activate(1, None).as_deref(), Some("org.telegram.desktop"));
        assert!(store.nc_items().is_empty(), "the click reads it");

        let handled = Notification { has_default_action: true, ..from_app(2, "Telegram") };
        assert!(store.push(&mut icons, handled, false));
        assert_eq!(store.activate(2, None), None, "the app opens the chat itself");

        assert!(store.push(&mut icons, from_app(3, "Mail"), false));
        assert_eq!(store.activate(3, None).as_deref(), Some("Mail"), "the name stands in for a missing entry");
        assert_eq!(store.activate(99, None), None);
        assert_eq!(app_to_raise(&Notification { id: LOCAL_ID_FLOOR + 5, ..from_app(5, "Glide") }), None);
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
        assert_eq!(store.notice().and_then(|notice| notice.reply), Some(NoticeReply::Sent));
        assert!(!store.open_reply(1), "a sent notice does not reopen its composer");

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
        store.open_reply(1);
        store.send_reply();
        assert!(store.live[0].reply_invalid());
        assert!(store.is_animating(), "the flag alone keeps the pump running");

        store.live[0].reply_invalid_since = Instant::now().checked_sub(glide_theme::motion::t3());
        assert!(!store.live[0].reply_invalid());
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Composing);
        assert!(!store.is_animating());
        assert_eq!(store.reply_target(), Some(1), "expiry reverts the border, not the draft");
    }

    #[test]
    fn cancelling_clears_the_invalid_flag_with_the_draft() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.send_reply();
        assert!(store.is_animating());

        assert!(store.cancel_reply());
        assert_eq!(store.live[0].reply_phase(), ReplyPhase::Closed);
        assert!(!store.is_animating());
    }

    #[test]
    fn the_reply_button_leads_and_a_configure_button_comes_last_only_when_offered() {
        let (mut store, mut icons) = store();
        let everything = Notification {
            has_settings_action: true,
            actions: vec![glide_notify::Action { key: "archive".to_string(), label: "Archive".to_string() }],
            ..repliable(1)
        };
        store.push(&mut icons, everything, false);
        let buttons = store.notice().map(|notice| notice.buttons).unwrap_or_default();
        assert!(matches!(
            buttons.as_slice(),
            [NoticeButton::Reply(_), NoticeButton::Action(0, archive), NoticeButton::Settings(_)] if archive == "Archive"
        ));
        assert_eq!(buttons[2].action(1), NotificationAction::Configure(1));
        assert!(store.remove(1, CloseReason::Dismissed, false));
        store.push(&mut icons, notification(2), false);
        assert!(store.notice().is_some_and(|notice| notice.buttons.is_empty()));
    }

    #[test]
    fn one_app_collapses_into_one_counted_stack_within_its_budget() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        assert_eq!(store.notice().map(|notice| notice.waiting), Some(0), "a lone notice counts nothing");
        let second =
            Notification { desktop_entry: "org.telegram.desktop".to_string(), ..from_app(2, "Telegram Desktop") };
        let first = Notification { desktop_entry: "org.telegram.desktop".to_string(), ..from_app(1, "Telegram") };
        let (mut grouped, mut gicons) = self::store();
        grouped.push(&mut gicons, first, false);
        grouped.push(&mut gicons, second, false);
        assert_eq!(grouped.live.len(), 1, "grouping prefers the desktop entry");

        store.push(&mut icons, from_app(2, "Telegram"), false);
        assert_eq!(store.notice().map(|notice| notice.waiting), Some(1));
        store.push(&mut icons, from_app(3, "Telegram"), false);
        assert_eq!(store.notice().map(|notice| notice.waiting), Some(2));
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
    }

    #[test]
    fn closing_the_top_of_a_stack_reveals_the_one_beneath_timed_afresh() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, from_app(1, "Telegram"), false);
        std::thread::sleep(Duration::from_millis(40));
        store.push(&mut icons, from_app(2, "Telegram"), false);
        store.push(&mut icons, from_app(3, "Telegram"), false);

        assert!(store.remove(3, CloseReason::Dismissed, false));
        assert_eq!(store.notice().map(|notice| notice.id), Some(2), "the previous message shows");
        assert_eq!(store.unread(), 2);
        assert!(store.live[0].updated_at.elapsed() < Duration::from_millis(20));
        assert!(store.remove(2, CloseReason::Dismissed, false));
        assert!(store.remove(1, CloseReason::Dismissed, false));
        assert!(store.is_empty(), "only the last one empties the queue");
    }

    #[test]
    fn a_replacement_keeps_the_typed_reply_but_restarts_expiry() {
        let (mut store, mut icons) = store();
        store.push(&mut icons, repliable(1), false);
        store.open_reply(1);
        store.type_reply('h');
        let first = store.live[0].updated_at;
        std::thread::sleep(Duration::from_millis(5));

        let mut update = repliable(1);
        update.body = "An edited body".to_string();
        assert!(store.push(&mut icons, update, true));

        let entry = &store.live[0];
        assert_eq!(entry.reply_text.as_deref(), Some("h"));
        assert!(entry.updated_at > first, "expiry restarts on replacement");
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
}
