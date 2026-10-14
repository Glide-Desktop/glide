use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use glide_theme::{
    Accent, Appearance, Color, Revision, TextStyle, Tokens,
    motion::{self, ease_out},
    shape,
};
use glide_ui::{Reveal, Tween};
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::element::{
            Kind,
            memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        },
    },
    utils::{Logical, Point, Rectangle, Scale, Transform},
};

use super::system::{GreeterStatus, SessionEntry, UserEntry};
use crate::{
    i18n::{Key, t},
    paint::{Painter, TextAlign},
    popup_menu::text_px as text_w,
};

const PAD_X: i32 = 36;
const PAD_Y: i32 = 32;
const DATE_H: i32 = 26;
const TIME_H: i32 = 110;
const CLOCK_H: i32 = DATE_H + 2 + TIME_H;
const AVATAR: i32 = 82;
const NAME_H: i32 = 22;
const FIELD_W: i32 = 240;
const FIELD_H: i32 = 38;
const SUBMIT: i32 = 32;
const HINT_H: i32 = 24;
const AUTH_H: i32 = AVATAR + 12 + NAME_H + 12 + FIELD_H + 10 + HINT_H;
const PILL_H: i32 = 42;
const ITEM_H: i32 = 34;
const ICON: i32 = 16;
const SMALL_ICON: i32 = 14;
const STATUS_TOP: i32 = 24;
const STATUS_RIGHT: i32 = 28;
const STATUS_H: i32 = 32;
const MENU_MIN_W: i32 = 220;
const MENU_MAX_W: i32 = 360;
const MENU_ROW: i32 = 34;
const MENU_GAP: i32 = 2;
const MENU_PAD: i32 = 6;
const MAX_DOTS: usize = 24;
const MAX_PASSWORD_CHARS: usize = 512;
const ERROR_HOLD: Duration = Duration::from_millis(2500);
const SPINNER_PERIOD: Duration = Duration::from_millis(600);
const SPINNER_STEPS: u32 = 36;
const MAX_CACHED_ICONS: usize = 256;

const fn style(size_px: f32, line_height_px: f32, weight: u16, tracking_em: f32) -> TextStyle {
    TextStyle { size_px, line_height_px, weight, tracking_em, uppercase: false, tabular: false }
}

const TIME_STYLE: TextStyle = style(114.0, 110.0, 560, -0.045);
const DATE_STYLE: TextStyle = style(21.0, 26.0, 500, -0.02);
const INITIAL_STYLE: TextStyle = style(30.0, 36.0, 650, 0.0);
const NAME_STYLE: TextStyle = style(17.0, 22.0, 620, -0.015);
const FIELD_STYLE: TextStyle = style(13.5, 20.0, 400, 0.0);
const HINT_STYLE: TextStyle = style(12.0, 18.0, 450, 0.0);
const HINT_STRONG_STYLE: TextStyle = style(12.0, 18.0, 550, 0.0);
const POWER_STYLE: TextStyle = style(12.0, 16.0, 500, -0.01);
const QUESTION_STYLE: TextStyle = style(12.5, 16.0, 550, 0.0);
const CONFIRM_STYLE: TextStyle = style(12.0, 16.0, 600, 0.0);
const SESSION_STYLE: TextStyle = style(12.5, 16.0, 500, 0.0);
const MENU_STYLE: TextStyle = style(13.0, 18.0, 400, 0.0);
const MENU_ACTIVE_STYLE: TextStyle = style(13.0, 18.0, 500, 0.0);
const STATUS_STYLE: TextStyle = style(11.5, 14.0, 550, 0.0);
const KBD_STYLE: TextStyle = TextStyle { tabular: true, ..style(11.5, 14.0, 600, 0.0) };

pub(crate) fn svg(stroke_width: f32, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" \
         fill=\"none\" stroke=\"black\" stroke-width=\"{stroke_width}\" stroke-linecap=\"round\" \
         stroke-linejoin=\"round\">{body}</svg>"
    )
}

mod icon {
    pub const MOON: &str = r#"<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>"#;
    pub const RESTART: &str = r#"<polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>"#;
    pub const POWER: &str = r#"<path d="M18.36 6.64a9 9 0 1 1-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/>"#;
    pub const LAYERS: &str = r#"<polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/>"#;
    pub const CHEVRON_DOWN: &str = r#"<polyline points="6 9 12 15 18 9"/>"#;
    pub const ARROW_RIGHT: &str = r#"<line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>"#;
    pub const CHECK: &str = r#"<polyline points="20 6 9 17 4 12"/>"#;
    pub const CLOSE: &str = r#"<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>"#;
    pub const WIFI: &str = r#"<path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><line x1="12" y1="20" x2="12.01" y2="20"/>"#;
}

fn battery_svg(pct: u8) -> String {
    let level = 11.0 * f32::from(pct.min(100)) / 100.0;
    svg(
        2.0,
        &format!(
            r#"<rect x="2" y="7" width="16" height="10" rx="2"/><line x1="22" y1="11" x2="22" y2="13"/><rect x="4.5" y="9.5" width="{level:.2}" height="5" rx="0.5" fill="black" stroke="none"/>"#
        ),
    )
}

fn spinner_svg(step: u32) -> String {
    let degrees = step * 360 / SPINNER_STEPS;
    svg(
        3.0,
        &format!(
            r#"<circle cx="12" cy="12" r="9" stroke-opacity="0.35"/><path d="M12 3a9 9 0 0 1 9 9" transform="rotate({degrees} 12 12)"/>"#
        ),
    )
}

fn round_i32(value: f64) -> i32 {
    value.round() as i32
}

fn fade(color: Color, alpha: f64) -> Color {
    let alpha = alpha.clamp(0.0, 1.0) as f32;
    color.with_alpha(color.0[3] * alpha)
}

fn lerp(from: i32, to: i32, t: f64) -> i32 {
    round_i32(f64::from(from) + f64::from(to - from) * t)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && x < f64::from(self.x + self.w)
            && y >= f64::from(self.y)
            && y < f64::from(self.y + self.h)
    }

    #[must_use]
    pub const fn right(self) -> i32 {
        self.x + self.w
    }

    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.y + self.h
    }

    #[must_use]
    pub const fn inflate(self, by: i32) -> Self {
        Self::new(self.x - by, self.y - by, self.w + by * 2, self.h + by * 2)
    }

    #[must_use]
    pub const fn local_to(self, frame: Self) -> Self {
        Self::new(self.x - frame.x, self.y - frame.y, self.w, self.h)
    }

    #[must_use]
    pub const fn centre_square(self, side: i32) -> (i32, i32) {
        (self.x + (self.w - side) / 2, self.y + (self.h - side) / 2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerAction {
    Sleep,
    Restart,
    ShutDown,
}

impl PowerAction {
    pub const ALL: [Self; 3] = [Self::Sleep, Self::Restart, Self::ShutDown];

    const fn icon(self) -> &'static str {
        match self {
            Self::Sleep => icon::MOON,
            Self::Restart => icon::RESTART,
            Self::ShutDown => icon::POWER,
        }
    }

    fn label(self) -> &'static str {
        t(match self {
            Self::Sleep => Key::GreeterSleep,
            Self::Restart => Key::GreeterRestart,
            Self::ShutDown => Key::CommandShutDown,
        })
    }

    fn question(self) -> &'static str {
        t(match self {
            Self::Sleep => Key::GreeterAskSleep,
            Self::Restart => Key::GreeterAskRestart,
            Self::ShutDown => Key::GreeterAskShutDown,
        })
    }

    fn confirm(self) -> &'static str {
        t(match self {
            Self::Sleep => Key::GreeterConfirmSleep,
            Self::Restart => Key::GreeterRestart,
            Self::ShutDown => Key::CommandShutDown,
        })
    }

    const fn danger(self) -> bool {
        !matches!(self, Self::Sleep)
    }

    const fn index(self) -> usize {
        match self {
            Self::Sleep => 0,
            Self::Restart => 1,
            Self::ShutDown => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    Idle,
    Authenticating,
    Error,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GreeterHit {
    None,
    UserAvatar,
    PasswordField,
    Submit,
    Power(PowerAction),
    PowerConfirm,
    PowerCancel,
    PowerCapsule,
    Session,
    SessionMenu,
    SessionItem(usize),
    KeyboardLayout,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submit {
    Nothing,
    Send(String),
    Queued,
    CreateSession(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Prompt {
    secret: bool,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Part {
    Clock,
    Status,
    Avatar,
    Name,
    Field,
    Hint,
    Power,
    Session,
    Menu,
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    fg_1: Color,
    fg_2: Color,
    fg_3: Color,
    white: Color,
    glass: Color,
    glass_border: Color,
    glass_card: Color,
    popover: Color,
    line: Color,
    line_2: Color,
    item_hover: Color,
    acc_bg: Color,
    acc_bg_hover: Color,
    acc_ring: Color,
    avatar_from: Color,
    avatar_to: Color,
    danger: Color,
    danger_soft: Color,
    revision: u64,
}

impl Palette {
    fn new(accent: Accent) -> Self {
        let tokens = Tokens::new(Appearance::Dark, accent);
        let white = Color([1.0, 1.0, 1.0, 1.0]);
        let mut palette = Self {
            fg_1: Color::from_rgb8(242, 240, 237, 1.0),
            fg_2: Color::from_rgb8(242, 240, 237, 0.72),
            fg_3: Color::from_rgb8(242, 240, 237, 0.48),
            white,
            glass: Color::from_rgb8(30, 29, 34, 0.45),
            glass_border: white.with_alpha(0.14),
            glass_card: Color::from_rgb8(26, 25, 30, 0.65),
            popover: Color::from_rgb8(37, 36, 41, 1.0),
            line: white.with_alpha(0.10),
            line_2: white.with_alpha(0.18),
            item_hover: white.with_alpha(0.12),
            acc_bg: tokens.acc_bg,
            acc_bg_hover: tokens.acc_bg_hover,
            acc_ring: tokens.ramp[1].with_alpha(0.45),
            avatar_from: tokens.ramp[2],
            avatar_to: tokens.ramp[4],
            danger: tokens.danger,
            danger_soft: tokens.danger.with_alpha(0.20),
            revision: 0,
        };
        palette.revision = Revision::new()
            .color(palette.acc_bg)
            .color(palette.acc_bg_hover)
            .color(palette.acc_ring)
            .color(palette.avatar_from)
            .color(palette.avatar_to)
            .color(palette.danger)
            .finish();
        palette
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub origin: (i32, i32),
    pub clock: Rect,
    pub avatar: Rect,
    pub name: Rect,
    pub field: Rect,
    pub submit: Rect,
    pub hint: Rect,
    pub power: Rect,
    pub power_frame: Rect,
    pub power_items: [Rect; 3],
    pub dividers: [Rect; 2],
    pub question: Rect,
    pub confirm: Rect,
    pub cancel: Rect,
    pub arm: f64,
    pub session: Rect,
    pub menu: Rect,
    pub menu_items: Vec<Rect>,
    pub status: Rect,
    pub kbd: Rect,
    pub wifi: Option<Rect>,
    pub battery: Option<Rect>,
}

struct Layer {
    key: u64,
    buffer: MemoryRenderBuffer,
}

pub(crate) type IconCache = HashMap<u64, Vec<u8>>;

#[allow(clippy::struct_excessive_bools)] // Independent UI flags, not a state machine
pub struct GreeterUi {
    users: Vec<UserEntry>,
    active_user: usize,
    sessions: Vec<SessionEntry>,
    active_session: usize,
    password: String,
    pending_dots: usize,
    auth: AuthState,
    prompt: Option<Prompt>,
    queued: Option<String>,
    needs_session: bool,
    message: Option<(String, Tone)>,
    error_at: Option<Instant>,
    unavailable: bool,
    arm: Option<PowerAction>,
    arm_shown: PowerAction,
    arm_reveal: Reveal,
    menu_reveal: Reveal,
    hover: GreeterHit,
    power_hover: [Tween; 3],
    session_lift: Tween,
    kbd_label: String,
    caps_lock: bool,
    status: GreeterStatus,
    time: String,
    date: String,
    leaving: Option<Instant>,
    spinner_epoch: Instant,
    palette: Palette,
    layout: Option<Layout>,
    layers: HashMap<(Part, i32), Layer>,
    icons: IconCache,
    scrim: MemoryRenderBuffer,
    dirty: bool,
    mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Login,
    Lock,
}

pub(crate) fn solid_pixel(bgra: [u8; 4]) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(&bgra, Fourcc::Argb8888, (1, 1), 1, Transform::Normal, None)
}

fn is_generic_password_prompt(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.trim().is_empty() || lower.contains("password") || lower.contains("пароль")
}

impl GreeterUi {
    #[must_use]
    pub fn new(users: Vec<UserEntry>, sessions: Vec<SessionEntry>, accent: Accent) -> Self {
        let mut ui = Self {
            users,
            active_user: 0,
            sessions,
            active_session: 0,
            password: String::new(),
            pending_dots: 0,
            auth: AuthState::Idle,
            prompt: None,
            queued: None,
            needs_session: false,
            message: None,
            error_at: None,
            unavailable: false,
            arm: None,
            arm_shown: PowerAction::ShutDown,
            arm_reveal: Reveal::closed().with_ease(ease_out),
            menu_reveal: Reveal::closed().with_ease(ease_out),
            hover: GreeterHit::None,
            power_hover: [Tween::at(0.0); 3],
            session_lift: Tween::at(0.0),
            kbd_label: String::new(),
            caps_lock: false,
            status: GreeterStatus::default(),
            time: String::new(),
            date: String::new(),
            leaving: None,
            spinner_epoch: Instant::now(),
            palette: Palette::new(accent),
            layout: None,
            layers: HashMap::new(),
            icons: HashMap::new(),
            scrim: solid_pixel([5, 4, 4, 102]),
            dirty: true,
            mode: Mode::Login,
        };
        ui.refresh_clock();
        ui
    }

    pub fn restore(&mut self, last: &super::LastLogin) {
        if let Some(index) = self.users.iter().position(|user| user.username == last.user) {
            self.active_user = index;
        }
        if let Some(index) = self.sessions.iter().position(|session| session.id == last.session) {
            self.active_session = index;
        }
        self.dirty = true;
    }

    #[must_use]
    pub fn last_login(&self) -> Option<super::LastLogin> {
        let user = self.active_user()?.username.clone();
        let session = self.active_session().map(|session| session.id.clone()).unwrap_or_default();
        Some(super::LastLogin { user, session })
    }

    #[must_use]
    pub fn lock(user: UserEntry, accent: Accent) -> Self {
        Self { mode: Mode::Lock, ..Self::new(vec![user], Vec::new(), accent) }
    }

    #[must_use]
    pub const fn is_lock(&self) -> bool {
        matches!(self.mode, Mode::Lock)
    }

    #[must_use]
    pub const fn is_leaving(&self) -> bool {
        self.leaving.is_some()
    }

    #[must_use]
    pub fn cover_alpha(&self) -> f64 {
        if self.is_lock() { 1.0 - self.leave_progress() } else { 1.0 }
    }

    #[must_use]
    pub fn active_user(&self) -> Option<&UserEntry> {
        self.users.get(self.active_user)
    }

    #[must_use]
    pub fn active_session(&self) -> Option<&SessionEntry> {
        self.sessions.get(self.active_session)
    }

    #[must_use]
    pub const fn armed(&self) -> Option<PowerAction> {
        self.arm
    }

    #[must_use]
    pub fn confirmable(&self) -> Option<PowerAction> {
        self.arm.filter(|_| self.password.is_empty() && self.editable())
    }

    #[must_use]
    pub const fn is_unavailable(&self) -> bool {
        self.unavailable
    }

    const fn editable(&self) -> bool {
        matches!(self.auth, AuthState::Idle | AuthState::Error) && self.leaving.is_none()
    }

    fn clear_error(&mut self) {
        if self.auth == AuthState::Error {
            self.auth = AuthState::Idle;
        }
        if matches!(self.message, Some((_, Tone::Error))) {
            self.message = None;
        }
        self.error_at = None;
    }

    pub fn type_char(&mut self, ch: char) -> bool {
        if !self.editable() || self.password.chars().count() >= MAX_PASSWORD_CHARS {
            return false;
        }
        self.clear_error();
        self.password.push(ch);
        self.dirty = true;
        true
    }

    pub fn backspace(&mut self) -> bool {
        if !self.editable() {
            return false;
        }
        self.clear_error();
        let popped = self.password.pop().is_some();
        self.dirty = true;
        popped
    }

    pub fn clear_password(&mut self) -> bool {
        if !self.editable() || self.password.is_empty() {
            return false;
        }
        self.password.clear();
        self.dirty = true;
        true
    }

    pub fn cancel(&mut self) -> bool {
        self.cancel_power_arm() || self.close_session_menu() || self.clear_password()
    }

    pub fn cycle_user(&mut self) -> Option<String> {
        if self.users.len() < 2 || !self.editable() {
            return None;
        }
        self.active_user = (self.active_user + 1) % self.users.len();
        self.password.clear();
        self.prompt = None;
        self.queued = None;
        self.needs_session = false;
        self.clear_error();
        self.message = None;
        self.dirty = true;
        self.active_user().map(|user| user.username.clone())
    }

    pub fn toggle_session_menu(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        if self.menu_reveal.is_open() {
            self.close_session_menu();
        } else {
            self.cancel_power_arm();
            self.menu_reveal.open(motion::t2());
            self.dirty = true;
        }
    }

    pub fn close_session_menu(&mut self) -> bool {
        let closed = self.menu_reveal.close(motion::t2());
        self.dirty |= closed;
        closed
    }

    pub fn select_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_session = index;
            self.close_session_menu();
            self.dirty = true;
        }
    }

    pub fn arm_power(&mut self, action: PowerAction) {
        self.close_session_menu();
        self.arm = Some(action);
        self.arm_shown = action;
        self.arm_reveal.open(motion::t3());
        self.retarget_hover();
        self.dirty = true;
    }

    pub fn cancel_power_arm(&mut self) -> bool {
        if self.arm.take().is_none() {
            return false;
        }
        self.arm_reveal.close(motion::t3());
        self.retarget_hover();
        self.dirty = true;
        true
    }

    pub fn set_keyboard_layout(&mut self, label: String) {
        if self.kbd_label != label {
            self.kbd_label = label;
            self.dirty = true;
        }
    }

    pub fn set_caps_lock(&mut self, on: bool) -> bool {
        let changed = std::mem::replace(&mut self.caps_lock, on) != on;
        self.dirty |= changed;
        changed
    }

    pub fn set_status(&mut self, status: GreeterStatus) {
        if self.status != status {
            self.status = status;
            self.dirty = true;
        }
    }

    pub fn submit(&mut self) -> Submit {
        if !self.editable() || self.password.is_empty() {
            return Submit::Nothing;
        }
        self.clear_error();
        self.message = None;
        self.pending_dots = self.password.chars().count();
        let password = std::mem::take(&mut self.password);
        self.auth = AuthState::Authenticating;
        self.spinner_epoch = Instant::now();
        self.dirty = true;
        if self.prompt.take().is_some() {
            return Submit::Send(password);
        }
        self.queued = Some(password);
        if std::mem::take(&mut self.needs_session)
            && let Some(user) = self.active_user()
        {
            return Submit::CreateSession(user.username.clone());
        }
        Submit::Queued
    }

    pub fn take_queued(&mut self) -> Option<String> {
        self.queued.take()
    }

    pub fn on_question(&mut self, secret: bool, text: String) -> Option<String> {
        if let Some(password) = self.queued.take() {
            return Some(password);
        }
        if self.auth == AuthState::Authenticating {
            self.auth = AuthState::Idle;
            self.pending_dots = 0;
        }
        self.prompt = Some(Prompt { secret, text });
        self.dirty = true;
        None
    }

    pub fn on_notice(&mut self, text: String, error: bool) {
        let tone = if error { Tone::Error } else { Tone::Info };
        self.message = Some((text, tone));
        self.dirty = true;
    }

    pub fn set_auth_error(&mut self, message: String, recreate_later: bool) {
        self.auth = AuthState::Error;
        self.error_at = Some(Instant::now());
        self.password.clear();
        self.pending_dots = 0;
        self.queued = None;
        self.prompt = None;
        self.needs_session = recreate_later;
        self.message = Some((message, Tone::Error));
        self.dirty = true;
    }

    pub fn set_auth_success(&mut self) {
        self.auth = AuthState::Success;
        self.message = None;
        self.dirty = true;
    }

    pub fn set_unavailable(&mut self) {
        self.unavailable = true;
        self.set_auth_error(t(Key::GreeterUnavailable).to_string(), false);
    }

    pub fn leave(&mut self) {
        if self.leaving.is_none() {
            self.leaving = Some(Instant::now());
            self.cancel_power_arm();
            self.close_session_menu();
            self.dirty = true;
        }
    }

    #[must_use]
    pub fn finished_leaving(&self) -> bool {
        self.leaving.is_some_and(|since| since.elapsed() >= motion::t4())
    }

    fn leave_progress(&self) -> f64 {
        let Some(since) = self.leaving else {
            return 0.0;
        };
        let duration = motion::t4();
        if duration.is_zero() {
            return 1.0;
        }
        ease_out((since.elapsed().as_secs_f64() / duration.as_secs_f64()).min(1.0))
    }

    fn shake_offset(&self) -> f64 {
        let Some(since) = self.error_at else {
            return 0.0;
        };
        let duration = motion::t3();
        let elapsed = since.elapsed();
        if duration.is_zero() || elapsed >= duration {
            return 0.0;
        }
        let progress = elapsed.as_secs_f64() / duration.as_secs_f64();
        -7.0 * (progress * std::f64::consts::TAU * 2.0).sin() * (1.0 - ease_out(progress))
    }

    fn spinner_step(&self) -> u32 {
        let period = SPINNER_PERIOD.as_secs_f64();
        let turns = self.spinner_epoch.elapsed().as_secs_f64() / period;
        let step = (turns.fract() * f64::from(SPINNER_STEPS)) as u32;
        step % SPINNER_STEPS
    }

    fn refresh_clock(&mut self) {
        let Some(now) = crate::topbar::calendar::LocalNow::now() else {
            return;
        };
        let h24 = crate::topbar::calendar::default_24h(crate::topbar::calendar::env_locale_is_us());
        let full = crate::topbar::calendar::format_time(now.hour, now.minute, h24);
        let time = full.split(' ').next().unwrap_or_default().to_string();
        let date = crate::i18n::long_date(crate::i18n::language(), now.weekday, now.day, now.month0);
        if time != self.time || date != self.date {
            self.time = time;
            self.date = date;
            self.dirty = true;
        }
    }

    pub fn update_hover(&mut self, hit: GreeterHit) -> bool {
        if self.hover == hit {
            return false;
        }
        self.hover = hit;
        self.retarget_hover();
        self.dirty = true;
        true
    }

    fn retarget_hover(&mut self) {
        let duration = motion::t2();
        for action in PowerAction::ALL {
            let on = self.arm.is_none() && self.hover == GreeterHit::Power(action);
            self.power_hover[action.index()].retarget(f64::from(u8::from(on)), duration, ease_out);
        }
        let lifted = self.hover == GreeterHit::Session;
        self.session_lift.retarget(f64::from(u8::from(lifted)), duration, ease_out);
    }

    fn is_animating(&self) -> bool {
        let (t2, t3) = (motion::t2(), motion::t3());
        self.error_at.is_some_and(|since| since.elapsed() < t3)
            || self.arm_reveal.is_animating(t3)
            || self.menu_reveal.is_animating(t2)
            || self.power_hover.iter().any(|tween| tween.is_animating(t2))
            || self.session_lift.is_animating(t2)
            || matches!(self.auth, AuthState::Authenticating | AuthState::Success)
            || self.leaving.is_some()
    }

    pub fn tick(&mut self) -> bool {
        self.refresh_clock();
        if self.error_at.is_some_and(|since| since.elapsed() >= ERROR_HOLD) && !self.unavailable {
            self.clear_error();
            self.dirty = true;
        }
        self.arm_reveal.settle(motion::t3());
        self.menu_reveal.settle(motion::t2());
        self.dirty || self.is_animating()
    }

    #[must_use]
    pub fn hit_test(&self, pos: Point<f64, Logical>) -> GreeterHit {
        let Some(layout) = self.layout.as_ref() else {
            return GreeterHit::None;
        };
        if self.leaving.is_some() {
            return GreeterHit::None;
        }
        let x = pos.x - f64::from(layout.origin.0);
        let y = pos.y - f64::from(layout.origin.1);
        if self.menu_reveal.is_open() {
            if let Some(index) = layout.menu_items.iter().position(|r| r.contains(x, y)) {
                return GreeterHit::SessionItem(index);
            }
            if layout.menu.contains(x, y) {
                return GreeterHit::SessionMenu;
            }
        }
        if layout.arm >= 0.5 {
            if layout.confirm.contains(x, y) {
                return GreeterHit::PowerConfirm;
            }
            if layout.cancel.contains(x, y) {
                return GreeterHit::PowerCancel;
            }
        } else if let Some(index) = layout.power_items.iter().position(|r| r.contains(x, y)) {
            return GreeterHit::Power(PowerAction::ALL[index]);
        }
        if layout.power.contains(x, y) {
            return GreeterHit::PowerCapsule;
        }
        if layout.session.contains(x, y) {
            return GreeterHit::Session;
        }
        if layout.kbd.contains(x, y) {
            return GreeterHit::KeyboardLayout;
        }
        if layout.status.contains(x, y) {
            return GreeterHit::Status;
        }
        if layout.avatar.contains(x, y) {
            return GreeterHit::UserAvatar;
        }
        if layout.submit.contains(x, y) {
            return GreeterHit::Submit;
        }
        if layout.field.contains(x, y) {
            return GreeterHit::PasswordField;
        }
        GreeterHit::None
    }

    fn session_label(&self) -> &str {
        self.active_session().map_or("Glide", |s| s.name.as_str())
    }

    fn battery_label(&self) -> Option<String> {
        self.status.battery.map(|(pct, _)| format!("{pct}%"))
    }

    #[must_use]
    pub fn compute_layout(&self, painter: &Painter, area: Rectangle<i32, Logical>) -> Layout {
        let (w, h) = (area.size.w, area.size.h);
        let clock = Rect::new(PAD_X, PAD_Y + 4, (w - PAD_X * 2).max(0), CLOCK_H);
        let bottom_y = h - PAD_Y - PILL_H;
        let free = bottom_y - clock.bottom() - AUTH_H;
        let top = (clock.bottom() + free / 2 - 10).max(clock.bottom() + 8);

        let avatar = Rect::new((w - AVATAR) / 2, top, AVATAR, AVATAR);
        let name = Rect::new((w - 320) / 2, avatar.bottom() + 12, 320, NAME_H);
        let field = Rect::new((w - FIELD_W) / 2, name.bottom() + 12, FIELD_W, FIELD_H);
        let submit = Rect::new(field.right() - 3 - SUBMIT, field.y + 3, SUBMIT, SUBMIT);
        let hint = Rect::new((w - 420) / 2, field.bottom() + 10, 420, HINT_H);

        let mut layout =
            Layout { origin: (area.loc.x, area.loc.y), clock, avatar, name, field, submit, hint, ..Layout::default() };
        self.layout_power(painter, &mut layout, bottom_y);
        if !self.is_lock() {
            self.layout_session(painter, &mut layout, w, bottom_y);
        }
        self.layout_status(painter, &mut layout, w);
        layout
    }

    fn layout_power(&self, painter: &Painter, layout: &mut Layout, bottom_y: i32) {
        let arm = self.arm_reveal.value(motion::t3());
        let labels = PowerAction::ALL.map(|action| text_w(painter, action.label(), &POWER_STYLE));
        let reveal = self.power_hover.map(|tween| tween.value(motion::t2(), ease_out));
        let button_w = |index: usize, t: f64| 10 + ICON + round_i32(f64::from(6 + labels[index]) * t) + 10;
        let idle_w = |t: [f64; 3]| 2 + 4 + (0..3).map(|i| button_w(i, t[i])).sum::<i32>() + 10 + 4;
        let armed_w = |action: PowerAction| {
            2 + 12
                + text_w(painter, action.question(), &QUESTION_STYLE)
                + 8
                + 24
                + text_w(painter, action.confirm(), &CONFIRM_STYLE)
                + 8
                + 28
                + 4
        };
        let power_x = PAD_X;
        let current_w = lerp(idle_w(reveal), armed_w(self.arm_shown), arm);
        let power = Rect::new(power_x, bottom_y, current_w, PILL_H);
        let widest = PowerAction::ALL.into_iter().map(armed_w).max().unwrap_or(0).max(idle_w([1.0; 3]));
        let power_frame = Rect::new(power_x, bottom_y, widest, PILL_H);

        let mut power_items = [Rect::default(); 3];
        let mut dividers = [Rect::default(); 2];
        let mut x = power.x + 1 + 4;
        for index in 0..3 {
            let bw = button_w(index, reveal[index]);
            power_items[index] = Rect::new(x, power.y + 4, bw, ITEM_H);
            x += bw + 2;
            if index < 2 {
                dividers[index] = Rect::new(x, power.y + (PILL_H - 16) / 2, 1, 16);
                x += 1 + 2;
            }
        }
        let question_w = text_w(painter, self.arm_shown.question(), &QUESTION_STYLE);
        let question = Rect::new(power.x + 1 + 12, power.y, question_w, PILL_H);
        let confirm_w = 24 + text_w(painter, self.arm_shown.confirm(), &CONFIRM_STYLE);
        let confirm = Rect::new(question.right() + 8, power.y + 6, confirm_w, 30);
        let cancel = Rect::new(confirm.right() + 8, power.y + 7, 28, 28);
        layout.power = power;
        layout.power_frame = power_frame;
        layout.power_items = power_items;
        layout.dividers = dividers;
        layout.question = question;
        layout.confirm = confirm;
        layout.cancel = cancel;
        layout.arm = arm;
    }

    fn layout_session(&self, painter: &Painter, layout: &mut Layout, w: i32, bottom_y: i32) {
        let label_w = text_w(painter, self.session_label(), &SESSION_STYLE).min(240);
        let session_w = 2 + 16 + SMALL_ICON + 8 + label_w + 8 + 11 + 16;
        let session = Rect::new(w - PAD_X - session_w, bottom_y, session_w, PILL_H);

        let widest_item = self.sessions.iter().map(|s| text_w(painter, &s.name, &MENU_ACTIVE_STYLE)).max().unwrap_or(0);
        let menu_w = (widest_item + MENU_PAD * 2 + 24 + 12 + 8).clamp(MENU_MIN_W, MENU_MAX_W);
        let rows = i32::try_from(self.sessions.len()).unwrap_or(0);
        let menu_h = MENU_PAD * 2 + rows * MENU_ROW + (rows - 1).max(0) * MENU_GAP;
        let menu = Rect::new(session.right() - menu_w, session.y - 8 - menu_h, menu_w, menu_h);
        let menu_items = (0..rows)
            .map(|row| {
                Rect::new(
                    menu.x + MENU_PAD,
                    menu.y + MENU_PAD + row * (MENU_ROW + MENU_GAP),
                    menu.w - MENU_PAD * 2,
                    MENU_ROW,
                )
            })
            .collect();
        layout.session = session;
        layout.menu = menu;
        layout.menu_items = menu_items;
    }

    fn layout_status(&self, painter: &Painter, layout: &mut Layout, w: i32) {
        let mut right = w - STATUS_RIGHT;
        let battery = self.battery_label().map(|label| {
            let bw = 2 + 10 + SMALL_ICON + 6 + text_w(painter, &label, &STATUS_STYLE) + 10;
            right -= bw;
            let rect = Rect::new(right, STATUS_TOP, bw, STATUS_H);
            right -= 8;
            rect
        });
        let wifi = self.status.wifi.map(|_| {
            let ww = 2 + 10 + SMALL_ICON + 10;
            right -= ww;
            let rect = Rect::new(right, STATUS_TOP, ww, STATUS_H);
            right -= 8;
            rect
        });
        let kbd_w = (2 + 18 + text_w(painter, &self.kbd_label, &KBD_STYLE)).max(STATUS_H);
        let kbd = if self.kbd_label.is_empty() {
            Rect::new(right, STATUS_TOP, 0, STATUS_H)
        } else {
            Rect::new(right - kbd_w, STATUS_TOP, kbd_w, STATUS_H)
        };
        let status_left = if kbd.w > 0 { kbd.x } else { right + 8 };
        let status = Rect::new(status_left, STATUS_TOP, (w - STATUS_RIGHT - status_left).max(0), STATUS_H);
        layout.status = status;
        layout.kbd = kbd;
        layout.wifi = wifi;
        layout.battery = battery;
    }

    fn field_text(&self) -> (String, bool) {
        let visible = self.prompt.as_ref().is_some_and(|prompt| !prompt.secret);
        if self.auth == AuthState::Authenticating || self.auth == AuthState::Success {
            return ("•".repeat(self.pending_dots.min(MAX_DOTS)), false);
        }
        if self.password.is_empty() {
            let placeholder =
                self.prompt.as_ref().filter(|prompt| !is_generic_password_prompt(&prompt.text)).map_or_else(
                    || t(Key::GreeterPassword).to_string(),
                    |prompt| prompt.text.trim().trim_end_matches(':').to_string(),
                );
            return (placeholder, true);
        }
        if visible {
            return (self.password.clone(), false);
        }
        let count = self.password.chars().count().min(MAX_DOTS);
        ("•".repeat(count), false)
    }

    fn hint(&self) -> (String, Tone, bool) {
        if let Some((text, tone)) = &self.message {
            return (text.clone(), *tone, *tone == Tone::Error);
        }
        if self.caps_lock && self.editable() {
            return (t(Key::CapsLockOn).to_string(), Tone::Info, true);
        }
        let key = match (self.auth, self.is_lock()) {
            (AuthState::Authenticating, _) => Key::GreeterChecking,
            (AuthState::Success, false) => Key::GreeterLoggingIn,
            (AuthState::Success, true) => Key::LockUnlocking,
            (AuthState::Idle | AuthState::Error, false) => Key::GreeterHint,
            (AuthState::Idle | AuthState::Error, true) => Key::LockHint,
        };
        (t(key).to_string(), Tone::Info, false)
    }

    fn part_key(&self, part: Part, frame: Rect, painter_generation: u64) -> u64 {
        let base = Revision::new().u64(self.palette.revision).u64(painter_generation).i32(frame.w).i32(frame.h);
        let hover = |hit: GreeterHit| self.hover == hit;
        match part {
            Part::Clock => base.str(&self.date).str(&self.time),
            Part::Status => base
                .str(&self.kbd_label)
                .bool(hover(GreeterHit::KeyboardLayout))
                .u64(u64::from(self.status.battery.map_or(255, |(pct, _)| pct)))
                .bool(self.status.wifi.is_some()),
            Part::Avatar => {
                base.str(&self.active_user().map_or('?', |u| u.initial).to_string()).bool(hover(GreeterHit::UserAvatar))
            }
            Part::Name => base.str(self.active_user().map_or("", |u| u.display_name.as_str())),
            Part::Field => {
                let (text, placeholder) = self.field_text();
                let busy = matches!(self.auth, AuthState::Authenticating | AuthState::Success);
                base.str(&text)
                    .bool(placeholder)
                    .bool(self.auth == AuthState::Error)
                    .bool(self.editable())
                    .bool(hover(GreeterHit::Submit))
                    .u64(if busy { u64::from(self.spinner_step()) + 1 } else { 0 })
            }
            Part::Hint => {
                let (text, tone, strong) = self.hint();
                base.str(&text).bool(tone == Tone::Error).bool(strong)
            }
            Part::Power => {
                let layout = self.layout.as_ref();
                base.f64(layout.map_or(0.0, |l| l.arm))
                    .i32(layout.map_or(0, |l| l.power.w))
                    .u64(self.arm_shown.index() as u64)
                    .u64(match self.hover {
                        GreeterHit::Power(action) => action.index() as u64 + 1,
                        GreeterHit::PowerConfirm => 10,
                        GreeterHit::PowerCancel => 11,
                        GreeterHit::PowerCapsule => 12,
                        _ => 0,
                    })
                    .f64(self.power_hover.iter().map(|tween| tween.value(motion::t2(), ease_out)).sum())
            }
            Part::Session => {
                base.str(self.session_label()).bool(hover(GreeterHit::Session) || self.menu_reveal.is_open())
            }
            Part::Menu => {
                let mut key = base.u64(self.active_session as u64).u64(match self.hover {
                    GreeterHit::SessionItem(index) => index as u64 + 1,
                    _ => 0,
                });
                for session in &self.sessions {
                    key = key.str(&session.name);
                }
                key
            }
        }
        .finish()
    }

    fn frame_of(part: Part, layout: &Layout) -> Rect {
        match part {
            Part::Clock => layout.clock,
            Part::Status => layout.status.inflate(12),
            Part::Avatar => layout.avatar.inflate(40),
            Part::Name => layout.name,
            Part::Field => layout.field.inflate(4),
            Part::Hint => layout.hint,
            Part::Power => layout.power_frame.inflate(12),
            Part::Session => layout.session.inflate(12),
            Part::Menu => layout.menu.inflate(48),
        }
    }

    fn draw_part(&self, part: Part, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let local = |rect: Rect| rect.local_to(frame);
        match part {
            Part::Clock => {
                let full = Rect::new(0, 0, frame.w, DATE_H);
                canvas.text(full, &self.date, p.fg_2, &DATE_STYLE, TextAlign::Center);
                let time = Rect::new(0, DATE_H + 2, frame.w, TIME_H);
                canvas.text(time, &self.time, p.fg_1, &TIME_STYLE, TextAlign::Center);
            }
            Part::Status => self.draw_status(layout, frame, canvas),
            Part::Avatar => {
                let avatar = local(layout.avatar);
                let radius = shape::R_ROUND;
                canvas.shadow(avatar, radius, 8, 24.0, Color([0.0, 0.0, 0.0, 0.30]));
                if self.hover == GreeterHit::UserAvatar {
                    canvas.stroke(avatar.inflate(3), 3.0, radius, p.acc_ring);
                } else {
                    canvas.stroke(avatar.inflate(2), 2.0, radius, p.glass_border);
                }
                canvas.gradient(avatar, radius, p.avatar_from, p.avatar_to);
                let initial = self.active_user().map_or('?', |u| u.initial).to_string();
                canvas.text(avatar, &initial, p.white, &INITIAL_STYLE, TextAlign::Center);
            }
            Part::Name => {
                let name = self.active_user().map_or("", |u| u.display_name.as_str());
                let rect = Rect::new(0, 0, frame.w, frame.h);
                canvas.text(rect, name, p.fg_1, &NAME_STYLE, TextAlign::Center);
            }
            Part::Field => self.draw_field(layout, frame, canvas),
            Part::Hint => {
                let (text, tone, strong) = self.hint();
                let color = match tone {
                    Tone::Error => p.danger,
                    Tone::Info if self.message.is_some() => p.fg_2,
                    Tone::Info => p.fg_3,
                };
                let style = if strong { &HINT_STRONG_STYLE } else { &HINT_STYLE };
                let rect = Rect::new(0, 0, frame.w, frame.h);
                canvas.text(rect, &text, color, style, TextAlign::Center);
            }
            Part::Power => self.draw_power(layout, frame, canvas),
            Part::Session => self.draw_session(layout, frame, canvas),
            Part::Menu => self.draw_menu(layout, frame, canvas),
        }
    }

    fn draw_session(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let pill = layout.session.local_to(frame);
        let hovered = self.hover == GreeterHit::Session || self.menu_reveal.is_open();
        canvas.shadow(pill, shape::R_ROUND, 2, 6.0, Color([0.0, 0.0, 0.0, 0.35]));
        let (fill, border, fg) =
            if hovered { (p.glass_card, p.line_2, p.fg_1) } else { (p.glass, p.glass_border, p.fg_2) };
        canvas.fill(pill, shape::R_ROUND, fill);
        canvas.stroke(pill, 1.0, shape::R_ROUND, border);
        let icon_x = pill.x + 1 + 16;
        let icon_y = pill.y + (PILL_H - SMALL_ICON) / 2;
        canvas.icon(&svg(2.0, icon::LAYERS), icon_x, icon_y, SMALL_ICON, fg);
        let label_x = icon_x + SMALL_ICON + 8;
        let label_right = pill.right() - 1 - 16 - 11 - 8;
        let label = Rect::new(label_x, pill.y, label_right - label_x, PILL_H);
        canvas.text(label, self.session_label(), fg, &SESSION_STYLE, TextAlign::Left);
        let chevron_x = pill.right() - 1 - 16 - 11;
        canvas.icon(&svg(2.5, icon::CHEVRON_DOWN), chevron_x, pill.y + (PILL_H - 11) / 2, 11, fg);
    }

    fn draw_menu(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let local = |rect: Rect| rect.local_to(frame);
        let menu = local(layout.menu);
        canvas.shadow(menu, shape::R3, 16, 48.0, Color([0.0, 0.0, 0.0, 0.55]));
        canvas.fill(menu, shape::R3, p.popover);
        canvas.stroke(menu, 1.0, shape::R3, p.line_2);
        for (index, (session, row)) in self.sessions.iter().zip(&layout.menu_items).enumerate() {
            let row = local(*row);
            let active = index == self.active_session;
            let (fg, style) = if active {
                canvas.fill(row, shape::R2, p.acc_bg);
                (p.white, &MENU_ACTIVE_STYLE)
            } else {
                if self.hover == GreeterHit::SessionItem(index) {
                    canvas.fill(row, shape::R2, p.white.with_alpha(0.08));
                }
                (p.fg_1, &MENU_STYLE)
            };
            let text = Rect::new(row.x + 12, row.y, row.w - 24 - 12 - 8, row.h);
            canvas.text(text, &session.name, fg, style, TextAlign::Left);
            if active {
                let (_, y) = row.centre_square(12);
                canvas.icon(&svg(3.0, icon::CHECK), row.right() - 12 - 12, y, 12, fg);
            }
        }
    }

    fn draw_status(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let pill = |canvas: &mut Canvas<'_>, rect: Rect, hovered: bool| {
            canvas.shadow(rect, shape::R_ROUND, 2, 6.0, Color([0.0, 0.0, 0.0, 0.35]));
            let (fill, border) = if hovered { (p.glass_card, p.line_2) } else { (p.glass, p.glass_border) };
            canvas.fill(rect, shape::R_ROUND, fill);
            canvas.stroke(rect, 1.0, shape::R_ROUND, border);
        };
        if layout.kbd.w > 0 {
            let kbd = layout.kbd.local_to(frame);
            let hovered = self.hover == GreeterHit::KeyboardLayout;
            pill(canvas, kbd, hovered);
            let fg = if hovered { p.fg_1 } else { p.fg_2 };
            canvas.text(kbd, &self.kbd_label, fg, &KBD_STYLE, TextAlign::Center);
        }
        if let Some(wifi) = layout.wifi {
            let wifi = wifi.local_to(frame);
            pill(canvas, wifi, false);
            let (x, y) = wifi.centre_square(SMALL_ICON);
            canvas.icon(&svg(2.2, icon::WIFI), x, y, SMALL_ICON, p.fg_2);
        }
        if let (Some(battery), Some((pct, _))) = (layout.battery, self.status.battery) {
            let battery = battery.local_to(frame);
            pill(canvas, battery, false);
            let icon_x = battery.x + 1 + 10;
            let icon_y = battery.y + (STATUS_H - SMALL_ICON) / 2;
            canvas.icon(&battery_svg(pct), icon_x, icon_y, SMALL_ICON, p.fg_2);
            let text_x = icon_x + SMALL_ICON + 6;
            let text = Rect::new(text_x, battery.y, battery.right() - 11 - text_x, STATUS_H);
            canvas.text(text, &format!("{pct}%"), p.fg_2, &STATUS_STYLE, TextAlign::Left);
        }
    }

    fn draw_field(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let field = layout.field.local_to(frame);
        let submit = layout.submit.local_to(frame);
        canvas.fill(field, shape::R2, p.glass_card);

        let (text, placeholder) = self.field_text();
        let text_x = field.x + 14;
        let text_rect = Rect::new(text_x, field.y, submit.x - 6 - text_x, field.h);
        let colour = if placeholder { p.fg_3 } else { p.fg_1 };
        canvas.text(text_rect, &text, colour, &FIELD_STYLE, TextAlign::Left);
        if self.editable() {
            let typed_w = if placeholder { 0 } else { text_w(canvas.painter, &text, &FIELD_STYLE) + 1 };
            let caret_x = (text_x + typed_w).min(text_rect.right());
            canvas.fill(Rect::new(caret_x, field.y + (field.h - 16) / 2, 1, 16), 0.0, p.acc_bg_hover);
        }

        let fill = if self.hover == GreeterHit::Submit { p.acc_bg_hover } else { p.acc_bg };
        canvas.fill(submit, shape::R1, fill);
        let (x, y) = submit.centre_square(15);
        if matches!(self.auth, AuthState::Authenticating | AuthState::Success) {
            canvas.icon(&spinner_svg(self.spinner_step()), x, y, 15, p.white);
        } else {
            canvas.icon(&svg(2.5, icon::ARROW_RIGHT), x, y, 15, p.white);
        }
    }

    fn draw_power(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let capsule = layout.power.local_to(frame);
        let hovered = matches!(
            self.hover,
            GreeterHit::Power(_) | GreeterHit::PowerCapsule | GreeterHit::PowerConfirm | GreeterHit::PowerCancel
        );
        let (fill, border) = if hovered { (p.glass_card, p.line_2) } else { (p.glass, p.glass_border) };
        canvas.shadow(capsule, shape::R_ROUND, 2, 6.0, Color([0.0, 0.0, 0.0, 0.35]));
        canvas.fill(capsule, shape::R_ROUND, fill);
        canvas.stroke(capsule, 1.0, shape::R_ROUND, border);

        let arm = layout.arm;
        canvas.clipped(capsule.x + 1, capsule.right() - 1, |canvas| {
            if arm < 0.5 {
                self.draw_power_idle(layout, frame, canvas, 1.0 - arm * 2.0);
            } else {
                self.draw_power_armed(layout, frame, canvas, arm * 2.0 - 1.0);
            }
        });
    }

    fn draw_power_idle(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>, alpha: f64) {
        let p = &self.palette;
        for action in PowerAction::ALL {
            let item = layout.power_items[action.index()].local_to(frame);
            let reveal = self.power_hover[action.index()].value(motion::t2(), ease_out);
            let active = self.hover == GreeterHit::Power(action);
            let fg = match (active, action.danger()) {
                (true, true) => p.danger,
                (true, false) => p.fg_1,
                (false, _) => p.fg_2,
            };
            if active {
                let bg = if action.danger() { p.danger_soft } else { p.item_hover };
                canvas.fill(item, shape::R_ROUND, fade(bg, alpha));
            }
            let icon_y = item.y + (ITEM_H - ICON) / 2;
            canvas.icon(&svg(2.0, action.icon()), item.x + 10, icon_y, ICON, fade(fg, alpha));
            if reveal > 0.01 {
                let label_x = item.x + 10 + ICON + 6;
                canvas.clipped(item.x, item.right() - 10, |canvas| {
                    let label = Rect::new(label_x, item.y, 120, ITEM_H);
                    canvas.text(label, action.label(), fade(fg, alpha * reveal), &POWER_STYLE, TextAlign::Left);
                });
            }
        }
        for divider in layout.dividers {
            canvas.fill(divider.local_to(frame), 0.0, fade(p.line, alpha));
        }
    }

    fn draw_power_armed(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>, alpha: f64) {
        let p = &self.palette;
        let action = self.arm_shown;
        let question = layout.question.local_to(frame);
        canvas.text(question, action.question(), fade(p.fg_1, alpha), &QUESTION_STYLE, TextAlign::Left);
        let confirm = layout.confirm.local_to(frame);
        let base = if action.danger() { p.danger } else { p.acc_bg };
        canvas.fill(confirm, shape::R_ROUND, fade(base, alpha));
        if self.hover == GreeterHit::PowerConfirm {
            canvas.fill(confirm, shape::R_ROUND, fade(p.white.with_alpha(motion::ST_HOVER), alpha));
        }
        canvas.text(confirm, action.confirm(), fade(p.white, alpha), &CONFIRM_STYLE, TextAlign::Center);
        let cancel = layout.cancel.local_to(frame);
        let (bg, fg) = if self.hover == GreeterHit::PowerCancel {
            (p.white.with_alpha(0.16), p.fg_1)
        } else {
            (p.white.with_alpha(0.08), p.fg_2)
        };
        canvas.fill(cancel, shape::R_ROUND, fade(bg, alpha));
        let (x, y) = cancel.centre_square(12);
        canvas.icon(&svg(2.5, icon::CLOSE), x, y, 12, fade(fg, alpha));
    }

    #[allow(clippy::too_many_lines)]
    pub fn render<R>(
        &mut self, renderer: &mut R, painter: &Painter, area: Rectangle<i32, Logical>, scale: Scale<f64>,
        buffer_scale: i32, primary: bool,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: crate::raster::ShellRenderer,
    {
        let mut out = Vec::new();
        let size = area.size;
        let leave = self.leave_progress();
        let full = |buffer: &MemoryRenderBuffer, alpha: f64, renderer: &mut R| {
            let alpha = alpha.clamp(0.0, 1.0) as f32;
            MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                (0.0, 0.0),
                buffer,
                Some(alpha),
                Some(Rectangle::from_size((1.0, 1.0).into())),
                Some(size),
                Kind::Unspecified,
            )
            .ok()
        };

        if primary && size.w > 0 && size.h > 0 && buffer_scale > 0 {
            let layout = self.compute_layout(painter, area);
            self.layout = Some(layout.clone());
            let generation = crate::paint::painter_generation();
            let menu_t = self.menu_reveal.value(motion::t2());
            let lift = self.session_lift.value(motion::t2(), ease_out);
            let shake = self.shake_offset();
            let ui_alpha = 1.0 - leave;

            let mut parts = Vec::with_capacity(9);
            if menu_t > 0.0 && !self.sessions.is_empty() {
                parts.push((Part::Menu, (0.0, 6.0 * (1.0 - menu_t) - 2.0 * lift), menu_t * ui_alpha));
            }
            parts.push((Part::Status, (0.0, 0.0), ui_alpha));
            if !self.is_lock() {
                parts.push((Part::Session, (0.0, 20.0 * leave - 2.0 * lift), ui_alpha));
            }
            parts.push((Part::Power, (0.0, 20.0 * leave), ui_alpha));
            parts.push((Part::Hint, (0.0, 0.0), ui_alpha));
            parts.push((Part::Field, (shake, 0.0), ui_alpha));
            parts.push((Part::Name, (0.0, 0.0), ui_alpha));
            parts.push((Part::Avatar, (0.0, 0.0), ui_alpha));
            parts.push((Part::Clock, (0.0, -20.0 * leave), ui_alpha));

            let mut icons = std::mem::take(&mut self.icons);
            for (part, (dx, dy), alpha) in parts {
                if alpha <= 0.0 {
                    continue;
                }
                let frame = Self::frame_of(part, &layout);
                if frame.w <= 0 || frame.h <= 0 {
                    continue;
                }
                let key = self.part_key(part, frame, generation);
                let slot = (part, buffer_scale);
                if self.layers.get(&slot).is_none_or(|layer| layer.key != key) {
                    let mut canvas = Canvas::new(frame.w, frame.h, buffer_scale, painter, &mut icons);
                    self.draw_part(part, &layout, frame, &mut canvas);
                    let buffer = canvas.finish();
                    self.layers.insert(slot, Layer { key, buffer });
                }
                let Some(layer) = self.layers.get(&slot) else {
                    continue;
                };
                let location =
                    Point::<f64, Logical>::from((f64::from(frame.x) + dx, f64::from(frame.y) + dy)).to_physical(scale);
                let location = Point::from((location.x.round(), location.y.round()));
                let alpha = alpha.clamp(0.0, 1.0) as f32;
                if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    location,
                    &layer.buffer,
                    Some(alpha),
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    out.push(element);
                }
            }
            self.icons = icons;
            self.dirty = false;
        }

        if let Some(element) = full(&self.scrim, self.cover_alpha(), renderer) {
            out.push(element);
        }
        out
    }
}

pub(crate) struct Canvas<'a> {
    buf: Vec<u8>,
    w: i32,
    h: i32,
    scale: i32,
    painter: &'a Painter,
    icons: &'a mut IconCache,
}

fn composite_over(dst: &mut [u8], src: &[u8], columns: Option<(usize, usize, usize)>) {
    for (index, (d, s)) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)).enumerate() {
        if let Some((stride, from, to)) = columns {
            let column = index % stride;
            if column < from || column >= to {
                continue;
            }
        }
        let alpha = u16::from(s[3]);
        if alpha == 0 {
            continue;
        }
        let keep = 255 - alpha;
        for (dc, sc) in d.iter_mut().zip(s) {
            let mixed = u16::from(*sc) + (u16::from(*dc) * keep + 127) / 255;
            *dc = u8::try_from(mixed).unwrap_or(u8::MAX);
        }
    }
}

impl<'a> Canvas<'a> {
    pub(crate) fn new(w: i32, h: i32, scale: i32, painter: &'a Painter, icons: &'a mut IconCache) -> Self {
        let len = usize::try_from(w * scale * h * scale * 4).unwrap_or(0);
        Self { buf: vec![0; len], w, h, scale, painter, icons }
    }

    fn view(&mut self) -> crate::raster::Canvas<'static, &mut [u8]> {
        crate::raster::Canvas::over(&mut self.buf, self.w, self.h, self.scale)
    }

    fn fill(&mut self, r: Rect, radius: f32, color: Color) {
        if color.0[3] > 0.0 && r.w > 0 && r.h > 0 {
            self.view().fill(crate::raster::rect(r.x, r.y, r.w, r.h), color, radius);
        }
    }

    fn gradient(&mut self, r: Rect, radius: f32, from: Color, to: Color) {
        self.view().gradient(crate::raster::rect(r.x, r.y, r.w, r.h), radius, from, to);
    }

    fn stroke(&mut self, r: Rect, width: f32, radius: f32, color: Color) {
        if color.0[3] > 0.0 {
            self.view().stroke(crate::raster::rect(r.x, r.y, r.w, r.h), width, radius, color);
        }
    }

    pub(crate) fn text(&mut self, r: Rect, text: &str, color: Color, style: &TextStyle, align: TextAlign) {
        if !text.is_empty() && color.0[3] > 0.0 && r.w > 0 {
            let painter = self.painter;
            self.view().text(painter, (r.x, r.right(), r.y, r.bottom()), align, text, color, style);
        }
    }

    pub(crate) fn icon(&mut self, svg: &str, x: i32, y: i32, side: i32, color: Color) {
        if color.0[3] <= 0.0 {
            return;
        }
        let px = side * self.scale;
        let key = Revision::new().str(svg).i32(px).color(color).finish();
        if self.icons.len() > MAX_CACHED_ICONS && !self.icons.contains_key(&key) {
            self.icons.clear();
        }
        let bytes = self
            .icons
            .entry(key)
            .or_insert_with(|| crate::paint::render_svg_tinted(svg.as_bytes(), px, color).unwrap_or_default());
        if bytes.is_empty() {
            return;
        }
        crate::paint::blit_square_over(&mut self.buf, self.w, self.h, bytes, side, x, y, self.scale);
    }

    fn shadow(&mut self, r: Rect, radius: f32, dy: i32, blur: f32, color: Color) {
        let mut layer = vec![0u8; self.buf.len()];
        Painter::fill_rounded_rect_over(&mut layer, self.w, self.h, r.x, r.y + dy, r.w, r.h, self.scale, color, radius);
        let radius_px = crate::paint::css_blur_box_radius(blur) * self.scale;
        crate::paint::box_blur_rgba(&mut layer, self.w * self.scale, self.h * self.scale, radius_px);
        composite_over(&mut self.buf, &layer, None);
    }

    fn clipped(&mut self, from_x: i32, to_x: i32, draw: impl FnOnce(&mut Self)) {
        let len = self.buf.len();
        let saved = std::mem::replace(&mut self.buf, vec![0; len]);
        draw(self);
        let layer = std::mem::replace(&mut self.buf, saved);
        let stride = usize::try_from(self.w * self.scale).unwrap_or(0);
        let from = usize::try_from((from_x * self.scale).max(0)).unwrap_or(0);
        let to = usize::try_from((to_x * self.scale).max(0)).unwrap_or(0);
        if stride > 0 {
            composite_over(&mut self.buf, &layer, Some((stride, from, to)));
        }
    }

    pub(crate) fn finish(self) -> MemoryRenderBuffer {
        crate::raster::buffer_from(&self.buf, self.w, self.h, self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users() -> Vec<UserEntry> {
        vec![
            UserEntry { username: "artem".into(), display_name: "Artem".into(), uid: 1000, initial: 'A' },
            UserEntry { username: "guest".into(), display_name: "Guest User".into(), uid: 1001, initial: 'G' },
        ]
    }

    fn sessions() -> Vec<SessionEntry> {
        ["glide", "sway"]
            .into_iter()
            .map(|id| SessionEntry {
                id: id.into(),
                name: id.into(),
                exec: vec![id.into()],
                desktop_names: String::new(),
            })
            .collect()
    }

    fn ui() -> GreeterUi {
        GreeterUi::new(users(), sessions(), Accent::Blue)
    }

    fn laid_out(ui: &mut GreeterUi) -> Layout {
        let painter = Painter::new(14.0, None);
        let layout = ui.compute_layout(&painter, Rectangle::from_size((1920, 1080).into()));
        ui.layout = Some(layout.clone());
        layout
    }

    fn centre(rect: Rect) -> Point<f64, Logical> {
        Point::from((f64::from(rect.x) + f64::from(rect.w) / 2.0, f64::from(rect.y) + f64::from(rect.h) / 2.0))
    }

    #[test]
    fn a_new_greeter_starts_idle_on_the_first_user_and_session() {
        let ui = ui();
        assert_eq!(ui.active_user().unwrap().username, "artem");
        assert_eq!(ui.active_session().unwrap().id, "glide");
        assert_eq!(ui.auth, AuthState::Idle);
        assert!(ui.armed().is_none());
        assert!(!ui.menu_reveal.is_open());
    }

    #[test]
    fn multibyte_passwords_show_one_dot_per_character() {
        let mut ui = ui();
        for ch in "пароль".chars() {
            ui.type_char(ch);
        }
        assert_eq!(ui.field_text(), ("••••••".to_string(), false));
        ui.backspace();
        assert_eq!(ui.field_text().0.chars().count(), 5);
    }

    #[test]
    fn a_password_typed_before_the_prompt_is_queued_then_sent_on_the_prompt() {
        let mut ui = ui();
        ui.type_char('x');
        assert_eq!(ui.submit(), Submit::Queued);
        assert_eq!(ui.auth, AuthState::Authenticating);
        assert!(ui.password.is_empty(), "the secret leaves the UI state once submitted");
        assert_eq!(ui.on_question(true, "Password:".into()), Some("x".into()));
    }

    #[test]
    fn a_password_typed_after_the_prompt_is_sent_directly() {
        let mut ui = ui();
        assert_eq!(ui.on_question(true, "Password:".into()), None);
        ui.type_char('y');
        assert_eq!(ui.submit(), Submit::Send("y".into()));
    }

    #[test]
    fn a_second_factor_prompt_reopens_the_field_with_its_own_placeholder() {
        let mut ui = ui();
        ui.on_question(true, "Password:".into());
        ui.type_char('y');
        ui.submit();
        assert_eq!(ui.on_question(false, "Verification code:".into()), None);
        assert_eq!(ui.auth, AuthState::Idle);
        assert_eq!(ui.field_text(), ("Verification code".to_string(), true));
        ui.type_char('4');
        assert_eq!(ui.field_text(), ("4".to_string(), false));
    }

    #[test]
    fn typing_clears_an_auth_error_and_a_failed_session_is_recreated_on_submit() {
        let mut ui = ui();
        ui.set_auth_error("boom".into(), true);
        assert_eq!(ui.auth, AuthState::Error);
        ui.type_char('a');
        assert_eq!(ui.auth, AuthState::Idle);
        assert!(ui.message.is_none());
        assert_eq!(ui.submit(), Submit::CreateSession("artem".into()));
        assert_eq!(ui.take_queued(), Some("a".into()));
    }

    #[test]
    fn input_is_ignored_while_authenticating() {
        let mut ui = ui();
        ui.type_char('a');
        ui.submit();
        assert!(!ui.type_char('b'));
        assert!(!ui.backspace());
        assert_eq!(ui.cycle_user(), None);
        assert_eq!(ui.submit(), Submit::Nothing);
    }

    #[test]
    fn cycling_users_wraps_and_resets_the_password() {
        let mut ui = ui();
        ui.type_char('a');
        assert_eq!(ui.cycle_user(), Some("guest".into()));
        assert!(ui.password.is_empty());
        assert_eq!(ui.cycle_user(), Some("artem".into()));
    }

    #[test]
    fn escape_unwinds_arm_then_menu_then_password() {
        let mut ui = ui();
        ui.type_char('a');
        ui.toggle_session_menu();
        assert!(ui.menu_reveal.is_open());
        ui.arm_power(PowerAction::Restart);
        assert!(!ui.menu_reveal.is_open(), "arming closes the menu");
        assert!(ui.cancel());
        assert!(ui.armed().is_none());
        ui.toggle_session_menu();
        assert!(ui.cancel());
        assert!(!ui.menu_reveal.is_open());
        assert!(ui.cancel());
        assert!(ui.password.is_empty());
        assert!(!ui.cancel());
    }

    #[test]
    fn enter_confirms_an_armed_power_action_only_with_an_empty_idle_field() {
        let mut ui = ui();
        assert_eq!(ui.confirmable(), None);
        ui.arm_power(PowerAction::ShutDown);
        assert_eq!(ui.confirmable(), Some(PowerAction::ShutDown));
        ui.type_char('a');
        assert_eq!(ui.confirmable(), None, "a typed password is submitted instead");
        ui.submit();
        assert_eq!(ui.confirmable(), None, "nothing is confirmed while a password is being checked");
    }

    #[test]
    fn selecting_a_session_closes_the_menu() {
        let mut ui = ui();
        ui.toggle_session_menu();
        ui.select_session(1);
        assert_eq!(ui.active_session().unwrap().id, "sway");
        assert!(!ui.menu_reveal.is_open());
    }

    #[test]
    fn hit_testing_follows_the_layout_the_render_path_stored() {
        let mut ui = ui();
        ui.set_keyboard_layout("US".into());
        let layout = laid_out(&mut ui);

        assert_eq!(ui.hit_test(centre(layout.submit)), GreeterHit::Submit);
        assert_eq!(ui.hit_test(centre(layout.field)), GreeterHit::PasswordField);
        assert_eq!(ui.hit_test(centre(layout.avatar)), GreeterHit::UserAvatar);
        assert_eq!(ui.hit_test(centre(layout.session)), GreeterHit::Session);
        assert_eq!(ui.hit_test(centre(layout.kbd)), GreeterHit::KeyboardLayout);
        for action in PowerAction::ALL {
            assert_eq!(ui.hit_test(centre(layout.power_items[action.index()])), GreeterHit::Power(action));
        }
        assert_eq!(ui.hit_test(Point::from((5.0, 5.0))), GreeterHit::None);
    }

    #[test]
    fn hit_testing_accounts_for_the_output_origin() {
        let mut ui = ui();
        let painter = Painter::new(14.0, None);
        let area = Rectangle::new((1920, 0).into(), (1920, 1080).into());
        let layout = ui.compute_layout(&painter, area);
        ui.layout = Some(layout.clone());
        let mut submit = centre(layout.submit);
        submit.x += 1920.0;
        assert_eq!(ui.hit_test(submit), GreeterHit::Submit);
    }

    #[test]
    fn the_layout_keeps_every_block_on_screen_and_apart() {
        let mut ui = ui();
        let layout = laid_out(&mut ui);
        assert!(layout.clock.bottom() < layout.avatar.y);
        assert!(layout.hint.bottom() < layout.power.y);
        assert!(layout.power.right() < layout.session.x);
        assert_eq!(layout.power.y, layout.session.y);
        assert!(layout.menu.bottom() < layout.session.y);
        assert_eq!(layout.menu.right(), layout.session.right());
        assert_eq!(layout.field.w, FIELD_W);
        assert_eq!(layout.submit.right(), layout.field.right() - 3);
    }

    #[test]
    fn the_lock_screen_has_no_session_picker_and_its_own_hints() {
        let mut ui = GreeterUi::lock(users().remove(0), Accent::Blue);
        assert!(ui.is_lock());
        let layout = laid_out(&mut ui);
        assert_eq!(layout.session, Rect::default());
        assert_eq!(ui.hit_test(centre(layout.submit)), GreeterHit::Submit);
        ui.toggle_session_menu();
        assert!(!ui.menu_reveal.is_open());
        assert_eq!(ui.cycle_user(), None);
        assert_eq!(ui.hint().0, t(Key::LockHint));
        ui.set_auth_success();
        assert_eq!(ui.hint().0, t(Key::LockUnlocking));
    }

    #[test]
    fn the_last_user_and_session_are_preselected_when_they_still_exist() {
        use crate::greeter::LastLogin;
        let mut ui = ui();
        ui.restore(&LastLogin { user: "guest".into(), session: "sway".into() });
        assert_eq!(ui.active_user().unwrap().username, "guest");
        assert_eq!(ui.active_session().unwrap().id, "sway");
        assert_eq!(ui.last_login(), Some(LastLogin { user: "guest".into(), session: "sway".into() }));
        ui.restore(&LastLogin { user: "deleted".into(), session: "removed".into() });
        assert_eq!(ui.active_user().unwrap().username, "guest", "an unknown name changes nothing");
        assert_eq!(ui.active_session().unwrap().id, "sway");
        let saved = LastLogin::parse("session = sway\nuser=guest\nnoise\n");
        assert_eq!(saved, LastLogin { user: "guest".into(), session: "sway".into() });
        assert_eq!(LastLogin::parse(&saved.serialize()), saved);
        assert_eq!(LastLogin::parse(""), LastLogin::default());
    }

    #[test]
    fn caps_lock_replaces_the_hint_while_typing_but_never_an_error() {
        let mut ui = ui();
        assert!(!ui.set_caps_lock(false), "an unchanged state does not redraw");
        assert_eq!(ui.hint().0, t(Key::GreeterHint));
        assert!(ui.set_caps_lock(true));
        assert_eq!(ui.hint().0, t(Key::CapsLockOn));
        ui.set_auth_error("boom".into(), false);
        assert_eq!(ui.hint().0, "boom");
        ui.type_char('A');
        assert_eq!(ui.hint().0, t(Key::CapsLockOn));
        ui.submit();
        assert_eq!(ui.hint().0, t(Key::GreeterChecking));
    }

    #[test]
    fn only_the_lock_screen_fades_its_cover_while_leaving() {
        let mut lock = GreeterUi::lock(users().remove(0), Accent::Blue);
        let mut greeter = ui();
        assert!((lock.cover_alpha() - 1.0).abs() < f64::EPSILON);
        lock.leave();
        greeter.leave();
        assert!(lock.is_leaving());
        assert!((greeter.cover_alpha() - 1.0).abs() < f64::EPSILON);
        assert!(lock.cover_alpha() <= 1.0);
    }

    #[test]
    fn every_icon_rasterises() {
        let side = 32;
        for body in [
            icon::MOON,
            icon::RESTART,
            icon::POWER,
            icon::LAYERS,
            icon::CHEVRON_DOWN,
            icon::ARROW_RIGHT,
            icon::CHECK,
            icon::CLOSE,
            icon::WIFI,
        ] {
            assert!(crate::paint::render_svg(svg(2.0, body).as_bytes(), side).is_some(), "{body}");
        }
        assert!(crate::paint::render_svg(battery_svg(50).as_bytes(), side).is_some());
        assert!(crate::paint::render_svg(spinner_svg(7).as_bytes(), side).is_some());
    }

    #[test]
    fn the_power_icon_stroke_starts_at_the_top_of_the_ring() {
        let side = 48;
        let bytes = crate::paint::render_svg(svg(2.0, icon::POWER).as_bytes(), side).unwrap();
        let alpha_at = |x: i32, y: i32| bytes[usize::try_from((y * side + x) * 4 + 3).unwrap()];
        assert!(alpha_at(side / 2, side / 8) > 0, "stem reaches the top");
        assert_eq!(alpha_at(side / 2, side * 5 / 8), 0, "no stem below the centre");
    }
}
