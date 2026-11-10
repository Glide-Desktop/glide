use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use glide_theme::{
    Accent, Appearance, Color, Revision, TextStyle, Tokens,
    motion::{self, ease_out},
    shape,
};
use glide_ui::{Reveal, clock};
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
use glide_paint::{Painter, TextAlign};
use glide_shell::{
    i18n::{Key, t},
    popup_menu::text_px as text_w,
};

const NAV_RIGHT: i32 = 20;
const NAV_TOP: i32 = 16;
const NAV_H: i32 = 34;
const NAV_ITEM: i32 = 32;
const NAV_GAP: i32 = 2;
const SEP_MARGIN: i32 = 6;
const DATE_H: i32 = 16;
const CLOCK_GAP: i32 = 6;
const TIME_LOGIN: i32 = 128;
const TIME_LOCK: i32 = 112;
const AVATAR_LOGIN: i32 = 92;
const AVATAR_LOCK: i32 = 72;
const NAME_LOGIN: i32 = 28;
const NAME_LOCK: i32 = 25;
const BLOCK_W: i32 = 600;
const FIELD_W: i32 = 320;
const FIELD_H: i32 = 46;
const GO: i32 = 36;
const HINT_H: i32 = 18;
const CHIP_H: i32 = 34;
const CHIP_AVATAR: i32 = 26;
const CHIP_GAP: i32 = 10;
const MENU_GAP: i32 = 8;
const MENU_PAD: i32 = 6;
const MENU_NOTE_H: i32 = 26;
const MENU_SHADOW_DY: i32 = 22;
const MENU_SHADOW_BLUR: i32 = 52;
const SESSION_MENU_W: i32 = 240;
const POWER_MENU_W: i32 = 260;
const SESSION_ROW: i32 = 34;
const POWER_ROW: i32 = 38;
const CARD_W: i32 = 360;
const MEDIA_H: i32 = 60;
const NOTICES_H: i32 = 44;
const CARD_GAP: i32 = 8;
const ARM_HOLD: Duration = Duration::from_secs(4);
const TOAST_HOLD: Duration = Duration::from_millis(1800);
const TOAST_H: i32 = 36;
const MAX_DOTS: usize = 24;
const MAX_PASSWORD_CHARS: usize = 512;
const PASSWORD_CAPACITY: usize = MAX_PASSWORD_CHARS * 4;
const ERROR_HOLD: Duration = Duration::from_millis(2500);
const SPINNER_PERIOD: Duration = Duration::from_millis(800);
const SPINNER_STEPS: u32 = 36;
const SUBMIT_SPINNER: i32 = 16;
const BUSY_SPINNER: i32 = 18;
const BUSY_GAP: i32 = 10;
const TOAST_SPINNER: i32 = 14;
const MAX_SPINNER_BUFFERS: usize = SPINNER_STEPS as usize * 6;
const MAX_CACHED_ICONS: usize = 256;
const SCRIM_ROWS: i32 = 64;
const FAILLOCK_CONF: &str = "/etc/security/faillock.conf";
const MAX_CONVERSATION_NOTICES: usize = 8;
const CARET_LEAD: i32 = 3;
const SCENE_TRAVEL: f64 = 24.0;
const MENU_TRAVEL: f64 = 6.0;

const fn style(size_px: f32, line_height_px: f32, weight: u16, tracking_em: f32) -> TextStyle {
    TextStyle { size_px, line_height_px, weight, tracking_em, uppercase: false, tabular: false }
}

const DATE_STYLE: TextStyle = style(11.5, 16.0, 600, 0.16);
const TIME_LOGIN_STYLE: TextStyle = TextStyle { tabular: true, ..style(128.0, 128.0, 300, -0.04) };
const TIME_LOCK_STYLE: TextStyle = TextStyle { tabular: true, ..style(112.0, 112.0, 300, -0.04) };
const INITIAL_LOGIN: TextStyle = style(36.0, 40.0, 680, 0.0);
const INITIAL_LOCK: TextStyle = style(28.0, 32.0, 680, 0.0);
const INITIAL_CHIP: TextStyle = style(11.5, 14.0, 680, 0.0);
const NAME_LOGIN_STYLE: TextStyle = style(22.0, 28.0, 650, -0.015);
const NAME_LOCK_STYLE: TextStyle = style(19.0, 25.0, 650, -0.015);
const FIELD_STYLE: TextStyle = style(14.5, 20.0, 400, 0.02);
const DOTS_STYLE: TextStyle = style(14.5, 20.0, 400, 0.14);
const HINT_STYLE: TextStyle = style(12.5, 18.0, 400, 0.0);
const KBD_STYLE: TextStyle = style(12.0, 16.0, 600, 0.04);
const NAV_STYLE: TextStyle = style(12.0, 16.0, 550, 0.0);
const MENU_STYLE: TextStyle = style(13.0, 18.0, 400, 0.0);
const MENU_ARMED_STYLE: TextStyle = style(13.0, 18.0, 600, 0.0);
const MENU_NOTE_STYLE: TextStyle = style(11.5, 16.0, 400, 0.0);
const CHIP_STYLE: TextStyle = style(12.5, 16.0, 550, 0.0);
const PILL_STYLE: TextStyle = style(13.5, 18.0, 550, 0.0);
const PILL_COUNT_STYLE: TextStyle = TextStyle { tabular: true, ..style(13.5, 18.0, 650, 0.0) };
const BUSY_STYLE: TextStyle = style(14.0, 20.0, 550, 0.0);
const MEDIA_TITLE_STYLE: TextStyle = style(13.5, 18.0, 640, 0.0);
const MEDIA_SUB_STYLE: TextStyle = style(12.0, 16.0, 400, 0.0);
const NOTICE_STYLE: TextStyle = style(12.5, 16.0, 550, 0.0);
const NOTICE_NOTE_STYLE: TextStyle = style(11.5, 16.0, 400, 0.0);
const BUBBLE_STYLE: TextStyle = style(11.0, 14.0, 700, 0.0);

pub(crate) fn svg(stroke_width: f32, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" \
         fill=\"none\" stroke=\"black\" stroke-width=\"{stroke_width}\" stroke-linecap=\"round\" \
         stroke-linejoin=\"round\">{body}</svg>"
    )
}

fn filled(body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"black\">{body}</svg>"
    )
}

mod icon {
    pub const MOON: &str = r#"<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>"#;
    pub const RESTART: &str =
        r#"<path d="M3 12a9 9 0 0 1 15.4-6.4L21 8M21 3v5h-5M21 12a9 9 0 0 1-15.4 6.4L3 16M3 21v-5h5"/>"#;
    pub const POWER: &str = r#"<path d="M18.36 6.64a9 9 0 1 1-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/>"#;
    pub const LAYERS: &str = r#"<path d="m12 3 9 5-9 5-9-5z"/><path d="m3 13 9 5 9-5"/>"#;
    pub const CHEVRON_DOWN: &str = r#"<path d="m6 9 6 6 6-6"/>"#;
    pub const ARROW_RIGHT: &str = r#"<path d="M5 12h14M13 6l6 6-6 6"/>"#;
    pub const CHECK: &str = r#"<path d="m5 12.5 4.5 4.5L19 7.5"/>"#;
    pub const WIFI: &str = r#"<path d="M2 8.8a15 15 0 0 1 20 0"/><path d="M5 12.6a10 10 0 0 1 14 0"/><path d="M8.5 16.4a5 5 0 0 1 7 0"/><circle cx="12" cy="20" r="1"/>"#;
    pub const KEYBOARD: &str = r#"<rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M7.5 14h9"/>"#;
    pub const CLOCK: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>"#;
    pub const NOTE: &str =
        r#"<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>"#;
    pub const PLAY: &str = r#"<path d="M4.5 3v10l8-5z"/>"#;
    pub const PAUSE: &str = r#"<path d="M5 3h2v10H5zM9 3h2v10H9z"/>"#;
    pub const PREVIOUS: &str = r#"<path d="M5 3h1.5v10H5zM12 3v10l-6-5z"/>"#;
    pub const NEXT: &str = r#"<path d="M11 3h-1.5v10H11zM4 3v10l6-5z"/>"#;
}

fn battery_svg(pct: u8) -> String {
    let level = 13.0 * f32::from(pct.min(100)) / 100.0;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"28\" height=\"28\" viewBox=\"0 0 28 28\" fill=\"none\" stroke=\"black\" stroke-width=\"1.6\"><rect x=\"1\" y=\"8\" width=\"22\" height=\"12\" rx=\"3\"/><rect x=\"4\" y=\"11\" width=\"{level:.2}\" height=\"6\" rx=\"1.4\" fill=\"black\" stroke=\"none\"/><path d=\"M25 12v4\" stroke-linecap=\"round\"/></svg>"
    )
}

fn spinner_svg(step: u32) -> String {
    let degrees = step * 360 / SPINNER_STEPS;
    svg(
        3.0,
        &format!(
            r#"<circle cx="12" cy="12" r="9" stroke-opacity="0.3"/><path d="M12 3a9 9 0 0 1 9 9" transform="rotate({degrees} 12 12)"/>"#
        ),
    )
}

fn round_i32(value: f64) -> i32 {
    value.round() as i32
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
        self.w > 0
            && self.h > 0
            && x >= f64::from(self.x)
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
    const LOCK: [Self; 2] = [Self::Restart, Self::ShutDown];

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

    fn again(self) -> &'static str {
        t(match self {
            Self::Sleep => Key::GreeterAgainSleep,
            Self::Restart => Key::GreeterAgainRestart,
            Self::ShutDown => Key::GreeterAgainShutDown,
        })
    }

    const fn danger(self) -> bool {
        !matches!(self, Self::Sleep)
    }

    const fn index(self) -> u64 {
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
pub enum MediaButton {
    Previous,
    PlayPause,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GreeterHit {
    None,
    UserAvatar,
    User(usize),
    PasswordField,
    Submit,
    KeyboardLayout,
    Session,
    SessionItem(usize),
    PowerButton,
    Power(PowerAction),
    Menu,
    Status,
    Media(MediaButton),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submit {
    Nothing,
    Send(String),
    Queued,
    CreateSession(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LockMedia {
    pub title: String,
    pub subtitle: String,
    pub playing: bool,
    pub accent: Color,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockNotices {
    pub count: usize,
    pub apps: Vec<String>,
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
enum MenuKind {
    Session,
    Power,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Part {
    Clock,
    Nav,
    Avatar,
    Name,
    Field,
    Hint,
    Users,
    Menu,
    Media,
    Notices,
    Toast,
}

impl Part {
    fn owning(hit: GreeterHit) -> Option<Self> {
        Some(match hit {
            GreeterHit::None => return None,
            GreeterHit::UserAvatar => Self::Avatar,
            GreeterHit::User(_) => Self::Users,
            GreeterHit::PasswordField | GreeterHit::Submit => Self::Field,
            GreeterHit::KeyboardLayout | GreeterHit::Session | GreeterHit::PowerButton | GreeterHit::Status => {
                Self::Nav
            }
            GreeterHit::SessionItem(_) | GreeterHit::Power(_) | GreeterHit::Menu => Self::Menu,
            GreeterHit::Media(_) => Self::Media,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    text: Color,
    muted: Color,
    faint: Color,
    date: Color,
    white: Color,
    glass: Color,
    glass_line: Color,
    highlight: Color,
    hover: Color,
    open: Color,
    separator: Color,
    popover: Color,
    popover_line: Color,
    card: Color,
    card_soft: Color,
    card_line: Color,
    acc_bg: Color,
    acc_bg_hover: Color,
    go_idle: Color,
    danger: Color,
    danger_fill: Color,
    danger_soft: Color,
    danger_line: Color,
    wrong_line: Color,
    wrong_text: Color,
    caps_text: Color,
    revision: u64,
}

impl Palette {
    fn new(accent: Accent) -> Self {
        let tokens = Tokens::new(Appearance::Dark, accent);
        let white = Color([1.0, 1.0, 1.0, 1.0]);
        let ink = tokens.fg_1;
        let mut palette = Self {
            text: ink,
            muted: ink.with_alpha(0.62),
            faint: ink.with_alpha(0.5),
            date: ink.with_alpha(0.7),
            white,
            glass: white.with_alpha(0.10),
            glass_line: white.with_alpha(0.16),
            highlight: white.with_alpha(0.08),
            hover: white.with_alpha(0.10),
            open: white.with_alpha(0.14),
            separator: white.with_alpha(0.18),
            popover: Color::from_rgb8(47, 46, 52, 0.94),
            popover_line: white.with_alpha(0.09),
            card: Color::from_rgb8(30, 29, 35, 0.5),
            card_soft: Color::from_rgb8(30, 29, 35, 0.4),
            card_line: white.with_alpha(0.08),
            acc_bg: tokens.acc_bg,
            acc_bg_hover: tokens.acc_bg_hover,
            go_idle: white.with_alpha(0.12),
            danger: tokens.danger,
            danger_fill: Color::from_rgb8(224, 101, 91, 1.0),
            danger_soft: tokens.danger.with_alpha(0.16),
            danger_line: tokens.danger.with_alpha(0.32),
            wrong_line: Color::from_rgb8(255, 140, 128, 0.7),
            wrong_text: Color::from_rgb8(255, 154, 144, 1.0),
            caps_text: Color::from_rgb8(240, 199, 106, 1.0),
            revision: 0,
        };
        palette.revision =
            Revision::new().color(palette.text).color(palette.acc_bg).color(palette.acc_bg_hover).finish();
        palette
    }
}

fn user_colors(seed: u32) -> (Color, Color) {
    const PAIRS: [(Accent, Accent); 4] = [
        (Accent::Blue, Accent::Purple),
        (Accent::Amber, Accent::Red),
        (Accent::Green, Accent::Teal),
        (Accent::Orange, Accent::Purple),
    ];
    let (from, to) = PAIRS[seed as usize % PAIRS.len()];
    (from.ramp()[2], to.ramp()[2])
}

fn bubble_colour(name: &str) -> Color {
    let hash = name.bytes().fold(0_u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(u32::from(byte)));
    Accent::ALL[hash as usize % Accent::ALL.len()].ramp()[3]
}

struct NavRow {
    right: i32,
    separators: Vec<Rect>,
}

impl NavRow {
    fn take(&mut self, width: i32) -> Rect {
        let rect = Rect::new(self.right - width, NAV_TOP + (NAV_H - NAV_ITEM) / 2, width, NAV_ITEM);
        self.right = rect.x - NAV_GAP;
        rect
    }

    fn separate(&mut self) {
        let x = self.right + NAV_GAP - SEP_MARGIN - 1;
        self.separators.push(Rect::new(x, NAV_TOP + (NAV_H - 16) / 2, 1, 16));
        self.right = x - SEP_MARGIN - NAV_GAP;
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub origin: (i32, i32),
    pub clock: Rect,
    pub nav: Rect,
    pub kbd: Rect,
    pub session: Rect,
    pub status: Rect,
    pub separators: Vec<Rect>,
    pub power: Rect,
    pub avatar: Rect,
    pub name: Rect,
    pub field: Rect,
    pub field_frame: Rect,
    pub submit: Rect,
    pub hint: Rect,
    pub users: Vec<(usize, Rect)>,
    pub users_frame: Rect,
    pub menu: Rect,
    pub menu_items: Vec<Rect>,
    pub media: Rect,
    pub media_buttons: [Rect; 3],
    pub notices: Rect,
    pub toast: Rect,
    pub busy_spinner: Rect,
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
    answered: bool,
    message: Option<(String, Tone)>,
    conversation_notices: Vec<String>,
    error_at: Option<Instant>,
    error_held: bool,
    unavailable: bool,
    menu: Option<MenuKind>,
    menu_shown: MenuKind,
    menu_reveal: Reveal,
    arm: Option<PowerAction>,
    armed_at: Option<Instant>,
    failures: u32,
    deny: Option<u32>,
    locked_until: Option<Instant>,
    hover: GreeterHit,
    hover_from: GreeterHit,
    hover_at: Instant,
    toast: Option<(PowerAction, Instant)>,
    toast_reveal: Reveal,
    kbd_label: String,
    caps_lock: bool,
    status: GreeterStatus,
    media: Option<LockMedia>,
    notices: LockNotices,
    time: String,
    date: String,
    leaving: Option<Instant>,
    arriving: Option<Instant>,
    spinner_epoch: Instant,
    palette: Palette,
    layout: Option<Layout>,
    layers: HashMap<(Part, i32), Layer>,
    spinners: HashMap<(i32, i32, u32), MemoryRenderBuffer>,
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

fn scrim_buffer() -> MemoryRenderBuffer {
    let mut pixels = Vec::with_capacity(usize::try_from(SCRIM_ROWS * 4).unwrap_or(0));
    for row in 0..SCRIM_ROWS {
        let t = f64::from(row) / f64::from(SCRIM_ROWS - 1);
        let alpha = 0.1 + (0.45 - 0.1) * t;
        let channel = |value: f64| (value * alpha * 255.0).round() as u8;
        pixels.extend_from_slice(&[channel(14.0 / 255.0), channel(10.0 / 255.0), channel(10.0 / 255.0)]);
        pixels.push((alpha * 255.0).round() as u8);
    }
    MemoryRenderBuffer::from_slice(&pixels, Fourcc::Argb8888, (1, SCRIM_ROWS), 1, Transform::Normal, None)
}

fn is_generic_password_prompt(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.trim().is_empty() || lower.contains("password") || lower.contains("пароль")
}

#[must_use]
pub fn faillock_deny(conf: &str) -> u32 {
    conf.lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "deny")
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(3)
}

#[must_use]
pub fn lockout_left(text: &str) -> Option<Duration> {
    let lower = text.to_lowercase();
    if !(lower.contains("lock") || lower.contains("блок")) {
        return None;
    }
    let words: Vec<&str> = lower.split(|ch: char| !ch.is_alphanumeric()).filter(|word| !word.is_empty()).collect();
    words.windows(2).find_map(|pair| {
        let count: u64 = pair[0].parse().ok()?;
        let unit = pair[1];
        if unit.starts_with("min") || unit.starts_with("хв") {
            Some(Duration::from_secs(count * 60))
        } else if unit.starts_with("sec") || unit.starts_with("сек") {
            Some(Duration::from_secs(count))
        } else {
            None
        }
    })
}

fn countdown(left: Duration) -> String {
    let secs = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn plural(lang: glide_shell::i18n::Lang, count: u64, keys: [Key; 3]) -> String {
    let key = match lang.plural(count) {
        glide_app::i18n::Plural::One => keys[0],
        glide_app::i18n::Plural::Few => keys[1],
        glide_app::i18n::Plural::Many => keys[2],
    };
    t(key).replace("{n}", &count.to_string())
}

impl GreeterUi {
    #[must_use]
    pub fn new(users: Vec<UserEntry>, sessions: Vec<SessionEntry>, accent: Accent) -> Self {
        let deny = std::fs::read_to_string(FAILLOCK_CONF).ok().map(|conf| faillock_deny(&conf)).filter(|n| *n > 0);
        let mut ui = Self {
            users,
            active_user: 0,
            sessions,
            active_session: 0,
            password: String::with_capacity(PASSWORD_CAPACITY),
            pending_dots: 0,
            auth: AuthState::Idle,
            prompt: None,
            queued: None,
            needs_session: false,
            answered: false,
            message: None,
            conversation_notices: Vec::new(),
            error_at: None,
            error_held: false,
            unavailable: false,
            menu: None,
            menu_shown: MenuKind::Power,
            menu_reveal: Reveal::closed().with_ease(ease_out),
            arm: None,
            armed_at: None,
            failures: 0,
            deny,
            locked_until: None,
            hover: GreeterHit::None,
            hover_from: GreeterHit::None,
            hover_at: clock::now(),
            toast: None,
            toast_reveal: Reveal::closed().with_ease(ease_out),
            kbd_label: String::new(),
            caps_lock: false,
            status: GreeterStatus::default(),
            media: None,
            notices: LockNotices::default(),
            time: String::new(),
            date: String::new(),
            leaving: None,
            arriving: None,
            spinner_epoch: clock::now(),
            palette: Palette::new(accent),
            layout: None,
            layers: HashMap::new(),
            spinners: HashMap::new(),
            icons: HashMap::new(),
            scrim: scrim_buffer(),
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
        if self.is_lock() { 1.0 - self.fade() } else { 1.0 }
    }

    pub fn arrive(&mut self) {
        if self.is_lock() && !motion::t3().is_zero() {
            self.arriving = Some(clock::now());
            self.dirty = true;
        }
    }

    #[must_use]
    pub fn is_translucent(&self) -> bool {
        self.leaving.is_some() || self.arrive_progress() < 1.0
    }

    fn arrive_progress(&self) -> f64 {
        let (Some(since), duration) = (self.arriving, motion::t3()) else {
            return 1.0;
        };
        if duration.is_zero() {
            return 1.0;
        }
        ease_out((clock::since(since).as_secs_f64() / duration.as_secs_f64()).min(1.0))
    }

    fn fade(&self) -> f64 {
        if self.leaving.is_some() { self.leave_progress() } else { 1.0 - self.arrive_progress() }
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

    fn locked_out(&self) -> Option<Duration> {
        let until = self.locked_until?;
        let left = until.saturating_duration_since(clock::now());
        (!left.is_zero()).then_some(left)
    }

    fn editable(&self) -> bool {
        matches!(self.auth, AuthState::Idle | AuthState::Error) && self.leaving.is_none() && self.locked_out().is_none()
    }

    fn power_actions(&self) -> &'static [PowerAction] {
        if self.is_lock() { &PowerAction::LOCK } else { &PowerAction::ALL }
    }

    fn clear_error(&mut self) {
        if self.auth == AuthState::Error {
            self.auth = AuthState::Idle;
        }
        if matches!(self.message, Some((_, Tone::Error))) {
            self.message = None;
        }
        self.error_at = None;
        self.error_held = false;
    }

    fn forget_queued(&mut self) {
        if let Some(mut password) = self.queued.take() {
            wipe(&mut password);
        }
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
        let popped = self.password.pop();
        if let Some(ch) = popped {
            let len = self.password.len();
            self.password.extend(std::iter::repeat_n('\0', ch.len_utf8()));
            self.password.truncate(len);
        }
        self.dirty = true;
        popped.is_some()
    }

    pub fn clear_password(&mut self) -> bool {
        if !self.editable() || self.password.is_empty() {
            return false;
        }
        wipe(&mut self.password);
        self.dirty = true;
        true
    }

    pub fn cancel(&mut self) -> bool {
        self.cancel_power_arm() || self.close_menus() || self.clear_password()
    }

    fn reset_for_user(&mut self) {
        wipe(&mut self.password);
        self.prompt = None;
        self.forget_queued();
        self.needs_session = false;
        self.clear_error();
        self.message = None;
        self.conversation_notices.clear();
        self.failures = 0;
        self.locked_until = None;
        self.close_menus();
        self.dirty = true;
    }

    fn can_switch_user(&self) -> bool {
        self.users.len() > 1
            && matches!(self.auth, AuthState::Idle | AuthState::Error)
            && self.leaving.is_none()
            && !self.is_lock()
    }

    pub fn cycle_user(&mut self) -> Option<String> {
        if !self.can_switch_user() {
            return None;
        }
        self.active_user = (self.active_user + 1) % self.users.len();
        self.reset_for_user();
        self.active_user().map(|user| user.username.clone())
    }

    pub fn select_user(&mut self, index: usize) -> Option<String> {
        if !self.can_switch_user() || index >= self.users.len() || index == self.active_user {
            return None;
        }
        self.active_user = index;
        self.reset_for_user();
        self.active_user().map(|user| user.username.clone())
    }

    fn open_menu(&mut self, kind: MenuKind) {
        if self.menu == Some(kind) {
            self.close_menus();
            return;
        }
        if kind != MenuKind::Power {
            self.arm = None;
            self.armed_at = None;
        }
        self.menu = Some(kind);
        self.menu_shown = kind;
        self.menu_reveal.open(motion::t2());
        self.dirty = true;
    }

    pub fn toggle_session_menu(&mut self) {
        if self.sessions.is_empty() || self.is_lock() {
            return;
        }
        self.open_menu(MenuKind::Session);
    }

    pub fn toggle_power_menu(&mut self) {
        self.open_menu(MenuKind::Power);
    }

    pub fn close_menus(&mut self) -> bool {
        if self.menu.take().is_none() {
            return false;
        }
        self.arm = None;
        self.armed_at = None;
        self.menu_reveal.close(motion::t2());
        self.dirty = true;
        true
    }

    pub fn close_session_menu(&mut self) -> bool {
        if self.menu == Some(MenuKind::Session) { self.close_menus() } else { false }
    }

    pub fn select_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_session = index;
            self.close_menus();
            self.dirty = true;
        }
    }

    pub fn arm_power(&mut self, action: PowerAction) {
        if !self.power_actions().contains(&action) {
            return;
        }
        if self.menu != Some(MenuKind::Power) {
            self.menu = Some(MenuKind::Power);
            self.menu_shown = MenuKind::Power;
            self.menu_reveal.open(motion::t2());
        }
        self.arm = Some(action);
        self.armed_at = Some(clock::now());
        self.dirty = true;
    }

    pub fn cancel_power_arm(&mut self) -> bool {
        if self.arm.take().is_none() {
            return false;
        }
        self.armed_at = None;
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

    pub fn set_media(&mut self, media: Option<LockMedia>) {
        if self.media != media {
            self.media = media;
            self.dirty = true;
        }
    }

    #[must_use]
    pub const fn notice_count(&self) -> usize {
        self.notices.count
    }

    pub fn set_notices(&mut self, notices: LockNotices) {
        if self.notices != notices {
            self.notices = notices;
            self.dirty = true;
        }
    }

    pub fn submit(&mut self) -> Submit {
        if !self.editable() || self.password.is_empty() {
            return Submit::Nothing;
        }
        self.clear_error();
        self.message = None;
        self.close_menus();
        self.pending_dots = self.password.chars().count();
        let password = std::mem::replace(&mut self.password, String::with_capacity(PASSWORD_CAPACITY));
        self.auth = AuthState::Authenticating;
        self.spinner_epoch = clock::now();
        self.dirty = true;
        if self.prompt.take().is_some() {
            self.answered = true;
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

    pub fn take_answered(&mut self) -> bool {
        std::mem::take(&mut self.answered)
    }

    pub fn refused_before_answer(&mut self, message: String) {
        let notice = self.message.take().map(|(text, _)| text).filter(|text| !text.trim().is_empty());
        self.set_auth_error(notice.unwrap_or(message), true);
    }

    pub fn take_queued(&mut self) -> Option<String> {
        self.queued.take()
    }

    pub fn on_question(&mut self, secret: bool, text: String) -> Option<String> {
        if matches!(self.message, Some((_, Tone::Info))) {
            self.message = None;
            self.dirty = true;
        }
        if let Some(password) = self.queued.take() {
            self.answered = true;
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
        let kept = text.trim();
        if !kept.is_empty() {
            if self.conversation_notices.len() >= MAX_CONVERSATION_NOTICES {
                self.conversation_notices.remove(0);
            }
            self.conversation_notices.push(kept.to_string());
        }
        self.message = Some((text, tone));
        self.dirty = true;
    }

    pub fn set_auth_error(&mut self, message: String, recreate_later: bool) {
        self.auth = AuthState::Error;
        self.error_at = Some(clock::now());
        wipe(&mut self.password);
        self.pending_dots = 0;
        self.forget_queued();
        self.prompt = None;
        self.needs_session = recreate_later;
        self.conversation_notices.clear();
        self.message = Some((message, Tone::Error));
        self.error_held = false;
        self.dirty = true;
    }

    pub fn set_auth_denied(&mut self, reason: Option<String>, fallback: &str) {
        let conversation = std::mem::take(&mut self.conversation_notices).join(" ");
        let current = self.message.as_ref().map(|(text, _)| text.clone());
        let lockout = [reason.as_deref(), current.as_deref(), Some(conversation.as_str())]
            .into_iter()
            .flatten()
            .find_map(|text| lockout_left(text).map(|left| (text.to_string(), left)));
        if let Some((text, left)) = lockout {
            self.set_auth_error(text, false);
            self.locked_until = Some(clock::now() + left);
            self.error_held = true;
            return;
        }
        if let Some(reason) = reason {
            self.set_auth_error(reason, false);
            self.error_held = true;
            return;
        }
        self.failures = self.failures.saturating_add(1);
        let text = match self.deny.map(|deny| deny.saturating_sub(self.failures)) {
            Some(left) if left > 0 => format!(
                "{} {}",
                t(Key::GreeterWrongShort),
                plural(
                    glide_shell::i18n::language(),
                    u64::from(left),
                    [Key::PkLeftOne, Key::PkLeftFew, Key::PkLeftMany]
                )
            ),
            _ => fallback.to_string(),
        };
        self.set_auth_error(text, false);
    }

    pub fn set_auth_success(&mut self) {
        self.auth = AuthState::Success;
        self.message = None;
        self.conversation_notices.clear();
        self.failures = 0;
        self.close_menus();
        self.spinner_epoch = clock::now();
        self.dirty = true;
    }

    pub fn set_unavailable(&mut self) {
        self.unavailable = true;
        self.set_auth_error(t(Key::GreeterUnavailable).to_string(), false);
    }

    pub fn leave(&mut self) {
        if self.leaving.is_none() {
            self.leaving = Some(clock::now());
            self.close_menus();
            self.dirty = true;
        }
    }

    #[must_use]
    pub const fn unlocking(&self) -> bool {
        self.is_lock() && self.leaving.is_some()
    }

    #[must_use]
    pub fn finished_leaving(&self) -> bool {
        self.leaving.is_some_and(|since| clock::since(since) >= motion::t4())
    }

    fn leave_progress(&self) -> f64 {
        let Some(since) = self.leaving else {
            return 0.0;
        };
        let duration = motion::t4();
        if duration.is_zero() {
            return 1.0;
        }
        ease_out((clock::since(since).as_secs_f64() / duration.as_secs_f64()).min(1.0))
    }

    fn shake_offset(&self) -> f64 {
        let Some(since) = self.error_at else {
            return 0.0;
        };
        let duration = motion::t3();
        let elapsed = clock::since(since);
        if duration.is_zero() || elapsed >= duration {
            return 0.0;
        }
        let progress = elapsed.as_secs_f64() / duration.as_secs_f64();
        -8.0 * (progress * std::f64::consts::TAU * 2.0).sin() * (1.0 - ease_out(progress))
    }

    fn spinner_step(&self) -> u32 {
        let period = SPINNER_PERIOD.as_secs_f64();
        let turns = clock::since(self.spinner_epoch).as_secs_f64() / period;
        let step = (turns.fract() * f64::from(SPINNER_STEPS)) as u32;
        step % SPINNER_STEPS
    }

    fn refresh_clock(&mut self) {
        let Some(now) = glide_shell::clock::LocalNow::now() else {
            return;
        };
        let h24 = glide_shell::clock::default_24h(glide_shell::clock::env_prefers_12h());
        let full = glide_shell::clock::format_time(now.hour, now.minute, h24);
        let time = full.split(' ').next().unwrap_or_default().to_string();
        let date = glide_shell::i18n::long_date(glide_shell::i18n::language(), now.weekday, now.day, now.month0)
            .replace(", ", " · ")
            .to_uppercase();
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
        self.hover_from = self.hover;
        self.hover = hit;
        self.hover_at = clock::now();
        self.dirty = true;
        true
    }

    fn hover_level(&self) -> f64 {
        let duration = motion::t1();
        if duration.is_zero() {
            return 1.0;
        }
        ease_out((clock::since(self.hover_at).as_secs_f64() / duration.as_secs_f64()).min(1.0))
    }

    fn hover_amount(&self, hit: GreeterHit) -> f32 {
        let level = (self.hover_level() * 8.0).round() / 8.0;
        let amount = if self.hover == hit {
            level
        } else if self.hover_from == hit {
            1.0 - level
        } else {
            0.0
        };
        amount as f32
    }

    fn hover_signature(&self, part: Part) -> String {
        let mine = |hit: GreeterHit| if Part::owning(hit) == Some(part) { hit } else { GreeterHit::None };
        let (now, before) = (mine(self.hover), mine(self.hover_from));
        if now == GreeterHit::None && before == GreeterHit::None {
            return String::new();
        }
        format!("{now:?}{before:?}{}", (self.hover_level() * 8.0).round())
    }

    pub fn power_started(&mut self, action: PowerAction) {
        self.toast = Some((action, clock::now()));
        self.toast_reveal.open(motion::t3());
        self.dirty = true;
    }

    pub fn keep_toast(&mut self) {
        if let Some((_, at)) = &mut self.toast {
            *at = clock::now();
            self.toast_reveal.open(motion::t3());
        }
    }

    fn toast_label(&self) -> Option<&'static str> {
        let (action, _) = self.toast?;
        Some(t(match action {
            PowerAction::Sleep => Key::GreeterToastSleep,
            PowerAction::Restart => Key::GreeterToastRestart,
            PowerAction::ShutDown => Key::GreeterToastShutDown,
        }))
    }

    fn busy(&self) -> bool {
        matches!(self.auth, AuthState::Authenticating | AuthState::Success)
    }

    fn is_animating(&self) -> bool {
        let (t2, t3) = (motion::t2(), motion::t3());
        self.error_at.is_some_and(|since| clock::since(since) < t3)
            || self.menu_reveal.is_animating(t2)
            || self.hover_level() < 1.0
            || self.toast.is_some()
            || self.busy()
            || self.leaving.is_some()
            || self.arrive_progress() < 1.0
    }

    pub fn tick(&mut self) -> bool {
        self.refresh_clock();
        if self.error_at.is_some_and(|since| clock::since(since) >= ERROR_HOLD) && !self.unavailable && !self.error_held
        {
            self.clear_error();
            self.dirty = true;
        }
        if self.armed_at.is_some_and(|at| clock::since(at) >= ARM_HOLD) {
            self.cancel_power_arm();
        }
        if self.toast.is_some_and(|(_, at)| clock::since(at) >= TOAST_HOLD) && self.toast_reveal.close(motion::t2()) {
            self.dirty = true;
        }
        if self.toast.is_some() && !self.toast_reveal.is_open() && !self.toast_reveal.is_visible(motion::t2()) {
            self.toast = None;
            self.dirty = true;
        }
        if let Some(until) = self.locked_until {
            if clock::now() >= until {
                self.locked_until = None;
                self.failures = 0;
                self.clear_error();
            }
            self.dirty = true;
        }
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
        if self.menu.is_some() {
            if let Some(index) = layout.menu_items.iter().position(|r| r.contains(x, y)) {
                return match self.menu_shown {
                    MenuKind::Session => GreeterHit::SessionItem(index),
                    MenuKind::Power => {
                        self.power_actions().get(index).copied().map_or(GreeterHit::Menu, GreeterHit::Power)
                    }
                };
            }
            if layout.menu.contains(x, y) {
                return GreeterHit::Menu;
            }
        }
        let targets = [
            (layout.power, GreeterHit::PowerButton),
            (layout.session, GreeterHit::Session),
            (layout.kbd, GreeterHit::KeyboardLayout),
            (layout.status, GreeterHit::Status),
            (layout.media_buttons[0], GreeterHit::Media(MediaButton::Previous)),
            (layout.media_buttons[1], GreeterHit::Media(MediaButton::PlayPause)),
            (layout.media_buttons[2], GreeterHit::Media(MediaButton::Next)),
        ];
        if let Some((_, hit)) = targets.iter().find(|(rect, _)| rect.contains(x, y)) {
            return *hit;
        }
        if let Some((index, _)) = layout.users.iter().find(|(_, rect)| rect.contains(x, y)) {
            return GreeterHit::User(*index);
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

    fn status_width(&self, painter: &Painter) -> i32 {
        let wifi = self.status.wifi.map_or(0, |_| 15);
        let battery = self.battery_label().map_or(0, |label| 19 + 6 + text_w(painter, &label, &NAV_STYLE));
        let inner = wifi + battery + if wifi > 0 && battery > 0 { 12 } else { 0 };
        if inner == 0 { 0 } else { inner + 12 }
    }

    fn layout_nav(&self, painter: &Painter, layout: &mut Layout, w: i32) {
        let mut row = NavRow { right: w - NAV_RIGHT, separators: Vec::new() };
        layout.power = row.take(NAV_ITEM);
        row.separate();
        let status_w = self.status_width(painter);
        if status_w > 0 {
            layout.status = row.take(status_w);
            row.separate();
        }
        if !self.is_lock() && !self.sessions.is_empty() {
            let label = text_w(painter, self.session_label(), &NAV_STYLE).min(220);
            layout.session = row.take(10 + 15 + 7 + label + 7 + 11 + 10);
        }
        if !self.kbd_label.is_empty() {
            layout.kbd = row.take(10 + 16 + 7 + text_w(painter, &self.kbd_label, &KBD_STYLE) + 10);
        }
        let left = [layout.kbd, layout.session, layout.status, layout.power]
            .iter()
            .filter(|rect| rect.w > 0)
            .map(|rect| rect.x)
            .min()
            .unwrap_or(layout.power.x);
        layout.nav = Rect::new(left, NAV_TOP, w - NAV_RIGHT - left, NAV_H);
        layout.separators = row.separators;
    }

    fn layout_menu(&self, painter: &Painter, layout: &mut Layout) {
        let (anchor, width, rows, row_h, note) = match self.menu_shown {
            MenuKind::Session => {
                let widest =
                    self.sessions.iter().map(|s| text_w(painter, &s.name, &MENU_STYLE)).max().unwrap_or(0) + 60;
                (layout.session, widest.max(SESSION_MENU_W), self.sessions.len(), SESSION_ROW, 0)
            }
            MenuKind::Power => (layout.power, POWER_MENU_W, self.power_actions().len(), POWER_ROW, MENU_NOTE_H),
        };
        let rows = i32::try_from(rows).unwrap_or(0);
        let height = MENU_PAD * 2 + rows * row_h + note;
        let right = if anchor.w > 0 { anchor.right() } else { layout.power.right() };
        let menu = Rect::new(right - width, NAV_TOP + NAV_H + MENU_GAP, width, height);
        layout.menu_items = (0..rows)
            .map(|row| Rect::new(menu.x + MENU_PAD, menu.y + MENU_PAD + row * row_h, width - MENU_PAD * 2, row_h))
            .collect();
        layout.menu = menu;
    }

    fn block_height(&self, others: bool) -> i32 {
        let (avatar, name, gap_name, gap_field) =
            if self.is_lock() { (AVATAR_LOCK, NAME_LOCK, 12, 18) } else { (AVATAR_LOGIN, NAME_LOGIN, 16, 22) };
        let users = if others { 22 + CHIP_H } else { 0 };
        avatar + gap_name + name + gap_field + FIELD_H + 12 + HINT_H + users
    }

    fn other_users(&self) -> Vec<usize> {
        if self.is_lock() {
            return Vec::new();
        }
        (0..self.users.len()).filter(|index| *index != self.active_user).collect()
    }

    #[must_use]
    pub fn compute_layout(&self, painter: &Painter, area: Rectangle<i32, Logical>) -> Layout {
        let (w, h) = (area.size.w, area.size.h);
        let mut layout = Layout { origin: (area.loc.x, area.loc.y), ..Layout::default() };
        self.layout_nav(painter, &mut layout, w);

        let lock = self.is_lock();
        let time_h = if lock { TIME_LOCK } else { TIME_LOGIN };
        let date_y = round_i32(f64::from(h) * if lock { 0.09 } else { 0.115 }).max(NAV_TOP + NAV_H + 8);
        layout.clock = Rect::new(0, date_y, w, DATE_H + CLOCK_GAP + time_h);

        let mut cards_bottom = layout.clock.bottom();
        if lock {
            let x = (w - CARD_W) / 2;
            let mut y = layout.clock.bottom() + 26;
            if self.media.is_some() {
                layout.media = Rect::new(x, y, CARD_W, MEDIA_H);
                let next = Rect::new(layout.media.right() - 8 - 32, y + (MEDIA_H - 32) / 2, 32, 32);
                let play = Rect::new(next.x - 12 - GO, y + (MEDIA_H - GO) / 2, GO, GO);
                let previous = Rect::new(play.x - 12 - 32, y + (MEDIA_H - 32) / 2, 32, 32);
                layout.media_buttons = [previous, play, next];
                y += MEDIA_H + CARD_GAP;
            }
            if self.notices.count > 0 {
                layout.notices = Rect::new(x, y, CARD_W, NOTICES_H);
                y += NOTICES_H + CARD_GAP;
            }
            cards_bottom = y - CARD_GAP;
        }

        let others = self.other_users();
        let block_h = self.block_height(!others.is_empty());
        let free = (h - layout.clock.bottom() - block_h) / 2 - 60;
        let mut top = layout.clock.bottom() + free.max(40);
        if lock && cards_bottom > layout.clock.bottom() {
            top = top.max(cards_bottom + 60);
        }
        let (avatar_side, name_h, gap_name, gap_field) =
            if lock { (AVATAR_LOCK, NAME_LOCK, 12, 18) } else { (AVATAR_LOGIN, NAME_LOGIN, 16, 22) };
        let centre = w / 2;
        layout.avatar = Rect::new(centre - avatar_side / 2, top, avatar_side, avatar_side);
        layout.name = Rect::new(centre - BLOCK_W / 2, layout.avatar.bottom() + gap_name, BLOCK_W, name_h);
        layout.field = Rect::new(centre - FIELD_W / 2, layout.name.bottom() + gap_field, FIELD_W, FIELD_H);
        layout.field_frame = Rect::new(centre - BLOCK_W / 2, layout.field.y - 8, BLOCK_W, FIELD_H + 16);
        layout.submit = Rect::new(layout.field.right() - 5 - GO, layout.field.y + (FIELD_H - GO) / 2, GO, GO);
        layout.hint = Rect::new(centre - BLOCK_W / 2, layout.field.bottom() + 12, BLOCK_W, HINT_H);
        if self.auth == AuthState::Success {
            let total = BUSY_SPINNER + BUSY_GAP + text_w(painter, &self.busy_label(), &BUSY_STYLE);
            let field = layout.field;
            let (x, y) = (field.x + (field.w - total) / 2, field.y + (FIELD_H - BUSY_SPINNER) / 2);
            layout.busy_spinner = Rect::new(x, y, BUSY_SPINNER, BUSY_SPINNER);
        }

        if !others.is_empty() {
            let widths: Vec<i32> = others
                .iter()
                .map(|index| 4 + CHIP_AVATAR + 8 + text_w(painter, &self.users[*index].display_name, &CHIP_STYLE) + 12)
                .collect();
            let total = widths.iter().sum::<i32>() + CHIP_GAP * (i32::try_from(widths.len()).unwrap_or(1) - 1);
            let mut x = centre - total / 2;
            let y = layout.hint.bottom() + 22;
            layout.users = others
                .iter()
                .zip(widths)
                .map(|(index, width)| {
                    let rect = Rect::new(x, y, width, CHIP_H);
                    x += width + CHIP_GAP;
                    (*index, rect)
                })
                .collect();
            layout.users_frame = Rect::new(centre - total / 2 - 4, y - 4, total + 8, CHIP_H + 8);
        }

        self.layout_menu(painter, &mut layout);
        if let Some(label) = self.toast_label() {
            let width = 16 + 14 + 8 + text_w(painter, label, &MENU_STYLE) + 16;
            layout.toast = Rect::new(centre - width / 2, h - 32 - TOAST_H, width, TOAST_H);
        }
        layout
    }

    fn field_text(&self) -> (String, bool) {
        let visible = self.prompt.as_ref().is_some_and(|prompt| !prompt.secret);
        if self.busy() {
            return ("•".repeat(self.pending_dots.min(MAX_DOTS)), false);
        }
        if self.password.is_empty() {
            let placeholder =
                self.prompt.as_ref().filter(|prompt| !is_generic_password_prompt(&prompt.text)).map_or_else(
                    || t(Key::PkPassword).to_string(),
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

    fn hint(&self) -> (String, Tone) {
        if self.locked_out().is_some() {
            let key = if self.is_lock() { Key::GreeterLockedHintLock } else { Key::GreeterLockedHint };
            return (t(key).to_string(), Tone::Info);
        }
        if let Some((text, tone)) = &self.message {
            return (text.lines().next().unwrap_or_default().to_string(), *tone);
        }
        if self.caps_lock && self.editable() {
            return (t(Key::CapsLockOn).to_string(), Tone::Info);
        }
        let key = match (self.auth, self.is_lock()) {
            (AuthState::Authenticating, _) => Key::GreeterChecking,
            (AuthState::Success, _) => return (String::new(), Tone::Info),
            (AuthState::Idle | AuthState::Error, false) => Key::GreeterHint,
            (AuthState::Idle | AuthState::Error, true) => Key::LockHint,
        };
        (t(key).to_string(), Tone::Info)
    }

    fn busy_label(&self) -> String {
        if self.is_lock() {
            t(Key::LockUnlocking).to_string()
        } else {
            t(Key::GreeterEntering).replace("{session}", self.session_label())
        }
    }

    fn part_key(&self, part: Part, frame: Rect, painter_generation: u64) -> u64 {
        let base = Revision::new().u64(self.palette.revision).u64(painter_generation).i32(frame.w).i32(frame.h);
        let hovered = base.str(&self.hover_signature(part));
        match part {
            Part::Clock => base.str(&self.date).str(&self.time).bool(self.is_lock()),
            Part::Nav => hovered
                .str(&self.kbd_label)
                .str(self.session_label())
                .u64(u64::from(self.status.battery.map_or(255, |(pct, _)| pct)))
                .bool(self.status.wifi.is_some())
                .u64(self.menu.map_or(0, |kind| kind as u64 + 1)),
            Part::Avatar => hovered
                .str(&self.active_user().map_or('?', |u| u.initial).to_string())
                .u64(u64::from(self.active_user().map_or(0, |u| u.uid)))
                .bool(self.can_switch_user()),
            Part::Name => base.str(self.active_user().map_or("", |u| u.display_name.as_str())),
            Part::Field => {
                let (text, placeholder) = self.field_text();
                let locked = self.locked_out().map(countdown);
                hovered
                    .str(&text)
                    .bool(placeholder)
                    .bool(self.auth == AuthState::Error)
                    .bool(self.editable())
                    .bool(self.busy())
                    .bool(self.auth == AuthState::Success)
                    .str(locked.as_deref().unwrap_or_default())
                    .str(&self.busy_label())
            }
            Part::Hint => {
                let (text, tone) = self.hint();
                base.str(&text).bool(tone == Tone::Error).bool(self.caps_lock)
            }
            Part::Users => {
                let mut key = hovered;
                for (index, _) in self.layout.as_ref().map(|l| l.users.as_slice()).unwrap_or_default() {
                    key = key.str(&self.users[*index].display_name);
                }
                key
            }
            Part::Menu => {
                let mut key = hovered
                    .u64(self.menu_shown as u64)
                    .u64(self.active_session as u64)
                    .u64(self.arm.map_or(9, PowerAction::index));
                for session in &self.sessions {
                    key = key.str(&session.name);
                }
                key
            }
            Part::Media => {
                let media = self.media.as_ref();
                hovered
                    .str(media.map_or("", |m| m.title.as_str()))
                    .str(media.map_or("", |m| m.subtitle.as_str()))
                    .bool(media.is_some_and(|m| m.playing))
                    .color(media.map_or(self.palette.acc_bg, |m| m.accent))
            }
            Part::Notices => {
                let mut key = base.u64(self.notices.count as u64);
                for app in &self.notices.apps {
                    key = key.str(app);
                }
                key
            }
            Part::Toast => base.str(self.toast_label().unwrap_or_default()),
        }
        .finish()
    }

    fn frame_of(part: Part, layout: &Layout) -> Rect {
        match part {
            Part::Clock => layout.clock,
            Part::Nav => layout.nav.inflate(4),
            Part::Avatar => layout.avatar.inflate(40),
            Part::Name => layout.name.inflate(8),
            Part::Field => layout.field_frame,
            Part::Hint => layout.hint.inflate(4),
            Part::Users => layout.users_frame,
            Part::Menu => {
                let reach = MENU_SHADOW_BLUR * 3 / 2;
                let frame = layout.menu.inflate(reach);
                Rect::new(frame.x, frame.y, frame.w, frame.h + MENU_SHADOW_DY)
            }
            Part::Media => layout.media.inflate(24),
            Part::Notices => layout.notices.inflate(24),
            Part::Toast => layout.toast.inflate(24),
        }
    }

    fn draw_part(&self, part: Part, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let local = |rect: Rect| rect.local_to(frame);
        match part {
            Part::Clock => {
                let date = Rect::new(0, 0, frame.w, DATE_H);
                canvas.text(date, &self.date, p.date, &DATE_STYLE, TextAlign::Center);
                let (style, height) =
                    if self.is_lock() { (&TIME_LOCK_STYLE, TIME_LOCK) } else { (&TIME_LOGIN_STYLE, TIME_LOGIN) };
                let time = Rect::new(0, DATE_H + CLOCK_GAP, frame.w, height);
                canvas.text(time, &self.time, p.white.with_alpha(0.96), style, TextAlign::Center);
            }
            Part::Nav => self.draw_nav(layout, frame, canvas),
            Part::Avatar => {
                let avatar = local(layout.avatar);
                let radius = shape::R_ROUND;
                canvas.shadow(avatar, radius, 12, 32.0, Color([0.0, 0.0, 0.0, 0.35]));
                let user = self.active_user();
                let (from, to) = user_colors(user.map_or(0, |u| u.uid));
                canvas.gradient(avatar, radius, from, to);
                let lift = if self.can_switch_user() { self.hover_amount(GreeterHit::UserAvatar) } else { 0.0 };
                let ring = p.white.with_alpha(0.18 + 0.22 * lift);
                canvas.stroke(avatar, 1.0, radius, ring);
                let initial = user.map_or('?', |u| u.initial).to_string();
                let style = if self.is_lock() { &INITIAL_LOCK } else { &INITIAL_LOGIN };
                canvas.text(avatar, &initial, p.white, style, TextAlign::Center);
            }
            Part::Name => {
                let name = self.active_user().map_or("", |u| u.display_name.as_str());
                let style = if self.is_lock() { &NAME_LOCK_STYLE } else { &NAME_LOGIN_STYLE };
                canvas.text(local(layout.name), name, p.text, style, TextAlign::Center);
            }
            Part::Field => self.draw_field(layout, frame, canvas),
            Part::Hint => {
                let (text, tone) = self.hint();
                let colour = match tone {
                    Tone::Error => p.wrong_text,
                    Tone::Info if self.caps_lock && self.editable() && self.message.is_none() => p.caps_text,
                    Tone::Info if self.auth == AuthState::Authenticating => p.text.with_alpha(0.75),
                    Tone::Info => p.muted,
                };
                canvas.text(local(layout.hint), &text, colour, &HINT_STYLE, TextAlign::Center);
            }
            Part::Users => self.draw_users(layout, frame, canvas),
            Part::Menu => self.draw_menu(layout, frame, canvas),
            Part::Media => self.draw_media(layout, frame, canvas),
            Part::Notices => self.draw_notices(layout, frame, canvas),
            Part::Toast => self.draw_toast(layout, frame, canvas),
        }
    }

    fn draw_toast(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let Some(label) = self.toast_label() else {
            return;
        };
        let pill = layout.toast.local_to(frame);
        canvas.fill(pill, shape::R_ROUND, Color::from_rgb8(30, 29, 35, 0.8));
        canvas.stroke(pill, 1.0, shape::R_ROUND, p.white.with_alpha(0.1));
        let text = Rect::new(pill.x + 16 + TOAST_SPINNER + 8, pill.y, pill.w - 16 - TOAST_SPINNER - 8 - 12, TOAST_H);
        canvas.text(text, label, p.text, &MENU_STYLE, TextAlign::Left);
    }

    fn layered(&self, canvas: &mut Canvas<'_>, rect: Rect, radius: f32, colour: Color, hit: GreeterHit) {
        let amount = self.hover_amount(hit);
        if amount > 0.0 {
            canvas.fill(rect, radius, colour.with_alpha(colour.0[3] * amount));
        }
    }

    fn nav_button(&self, canvas: &mut Canvas<'_>, rect: Rect, hit: GreeterHit, open: bool) {
        let p = &self.palette;
        if open {
            canvas.fill(rect, shape::R_ROUND, p.open);
        } else {
            self.layered(canvas, rect, shape::R_ROUND, p.hover, hit);
        }
    }

    fn draw_nav(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let ink = p.text.with_alpha(0.88);
        let local = |rect: Rect| rect.local_to(frame);
        if layout.kbd.w > 0 {
            let kbd = local(layout.kbd);
            self.nav_button(canvas, kbd, GreeterHit::KeyboardLayout, false);
            canvas.icon(&svg(1.9, icon::KEYBOARD), kbd.x + 10, kbd.y + (NAV_ITEM - 16) / 2, 16, ink);
            let label = Rect::new(kbd.x + 10 + 16 + 7, kbd.y, kbd.w - 10 - 16 - 7, kbd.h);
            canvas.text(label, &self.kbd_label, ink, &KBD_STYLE, TextAlign::Left);
        }
        if layout.session.w > 0 {
            let session = local(layout.session);
            self.nav_button(canvas, session, GreeterHit::Session, self.menu == Some(MenuKind::Session));
            canvas.icon(&svg(2.0, icon::LAYERS), session.x + 10, session.y + (NAV_ITEM - 15) / 2, 15, ink);
            let label_x = session.x + 10 + 15 + 7;
            let label = Rect::new(label_x, session.y, session.right() - 10 - 11 - 7 - label_x, session.h);
            canvas.text(label, self.session_label(), ink, &NAV_STYLE, TextAlign::Left);
            let chevron = svg(2.6, icon::CHEVRON_DOWN);
            canvas.icon(&chevron, session.right() - 10 - 11, session.y + (NAV_ITEM - 11) / 2, 11, ink);
        }
        for separator in &layout.separators {
            canvas.fill(local(*separator), 0.0, p.separator);
        }
        if layout.status.w > 0 {
            let status = local(layout.status);
            let mut x = status.x + 6;
            if self.status.wifi.is_some() {
                canvas.icon(&svg(2.0, icon::WIFI), x, status.y + (NAV_ITEM - 15) / 2, 15, ink);
                x += 15 + 12;
            }
            if let (Some((pct, _)), Some(label)) = (self.status.battery, self.battery_label()) {
                canvas.icon(&battery_svg(pct), x, status.y + (NAV_ITEM - 19) / 2, 19, ink);
                let text = Rect::new(x + 19 + 6, status.y, status.right() - x - 19 - 6, status.h);
                canvas.text(text, &label, ink, &NAV_STYLE, TextAlign::Left);
            }
        }
        let power = local(layout.power);
        self.nav_button(canvas, power, GreeterHit::PowerButton, self.menu == Some(MenuKind::Power));
        let (x, y) = power.centre_square(16);
        canvas.icon(&svg(2.1, icon::POWER), x, y, 16, ink);
    }

    fn draw_menu(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let local = |rect: Rect| rect.local_to(frame);
        let menu = local(layout.menu);
        canvas.shadow(menu, shape::R3, MENU_SHADOW_DY, MENU_SHADOW_BLUR as f32, Color([0.0, 0.0, 0.0, 0.5]));
        canvas.fill(menu, shape::R3, p.popover);
        canvas.stroke(menu, 1.0, shape::R3, p.popover_line);
        canvas.fill(Rect::new(menu.x + 8, menu.y + 1, menu.w - 16, 1), 0.0, p.white.with_alpha(0.07));
        match self.menu_shown {
            MenuKind::Session => {
                for (index, (session, row)) in self.sessions.iter().zip(&layout.menu_items).enumerate() {
                    let row = local(*row);
                    self.layered(canvas, row, shape::R2, p.highlight, GreeterHit::SessionItem(index));
                    let text = Rect::new(row.x + 10, row.y, row.w - 10 - 34, row.h);
                    canvas.text(text, &session.name, p.text, &MENU_STYLE, TextAlign::Left);
                    if index == self.active_session {
                        let (_, y) = row.centre_square(14);
                        canvas.icon(&svg(2.6, icon::CHECK), row.right() - 10 - 14, y, 14, p.acc_bg_hover);
                    }
                }
            }
            MenuKind::Power => {
                for (action, row) in self.power_actions().iter().zip(&layout.menu_items) {
                    let row = local(*row);
                    let armed = self.arm == Some(*action);
                    if armed {
                        canvas.fill(row, shape::R2, if action.danger() { p.danger_fill } else { p.acc_bg });
                    } else {
                        self.layered(canvas, row, shape::R2, p.highlight, GreeterHit::Power(*action));
                    }
                    let ink = if armed { p.white } else { p.text };
                    let (_, y) = row.centre_square(16);
                    canvas.icon(&svg(2.0, action.icon()), row.x + 12, y, 16, ink);
                    let text = Rect::new(row.x + 12 + 16 + 10, row.y, row.w - 12 - 16 - 10 - 8, row.h);
                    let (label, style) =
                        if armed { (action.again(), &MENU_ARMED_STYLE) } else { (action.label(), &MENU_STYLE) };
                    canvas.text(text, label, ink, style, TextAlign::Left);
                }
                let last = layout.menu_items.last().copied().map_or(menu, local);
                let note = Rect::new(menu.x + MENU_PAD + 10, last.bottom() + 6, menu.w - MENU_PAD * 2 - 20, 16);
                canvas.text(note, t(Key::GreeterPowerNote), p.faint, &MENU_NOTE_STYLE, TextAlign::Left);
            }
        }
    }

    fn draw_field(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let field = layout.field.local_to(frame);
        if self.auth == AuthState::Success {
            let label = self.busy_label();
            let text_w = text_w(canvas.painter, &label, &BUSY_STYLE);
            let spinner = layout.busy_spinner.local_to(frame);
            let rect = Rect::new(spinner.right() + BUSY_GAP, field.y, text_w + 2, FIELD_H);
            canvas.text(rect, &label, p.text, &BUSY_STYLE, TextAlign::Left);
            return;
        }
        if let Some(left) = self.locked_out() {
            let label = t(Key::GreeterLockedFor);
            let count = countdown(left);
            let label_w = text_w(canvas.painter, label, &PILL_STYLE);
            let count_w = text_w(canvas.painter, &count, &PILL_COUNT_STYLE);
            let width = 18 + 17 + 10 + label_w + 10 + count_w + 18;
            let pill = Rect::new(field.x + (field.w - width) / 2, field.y, width, FIELD_H);
            canvas.fill(pill, shape::R_ROUND, p.danger_soft);
            canvas.stroke(pill, 1.0, shape::R_ROUND, p.danger_line);
            canvas.icon(&svg(2.0, icon::CLOCK), pill.x + 18, pill.y + (FIELD_H - 17) / 2, 17, p.danger);
            let text_x = pill.x + 18 + 17 + 10;
            canvas.text(Rect::new(text_x, pill.y, label_w + 2, FIELD_H), label, p.text, &PILL_STYLE, TextAlign::Left);
            let count_rect = Rect::new(text_x + label_w + 10, pill.y, count_w + 2, FIELD_H);
            canvas.text(count_rect, &count, p.danger, &PILL_COUNT_STYLE, TextAlign::Left);
            return;
        }
        let wrong = self.auth == AuthState::Error;
        if wrong {
            canvas.fill(field.inflate(4), shape::R_ROUND, p.danger.with_alpha(0.18));
        }
        canvas.fill(field, shape::R_ROUND, p.glass);
        canvas.stroke(field, 1.0, shape::R_ROUND, if wrong { p.wrong_line } else { p.glass_line });
        canvas.fill(Rect::new(field.x + 20, field.y + 1, field.w - 40, 1), 0.0, p.highlight);
        let submit = layout.submit.local_to(frame);
        let (text, placeholder) = self.field_text();
        let text_x = field.x + 20;
        let text_rect = Rect::new(text_x, field.y, submit.x - 8 - text_x, field.h);
        let dots = !placeholder && self.prompt.as_ref().is_none_or(|prompt| prompt.secret);
        let style = if dots { &DOTS_STYLE } else { &FIELD_STYLE };
        let colour = if placeholder { p.faint } else { p.white };
        let busy = self.busy();
        canvas.text(text_rect, &text, if busy { colour.with_alpha(0.8) } else { colour }, style, TextAlign::Left);
        if self.editable() {
            let typed_w = if placeholder { -CARET_LEAD } else { text_w(canvas.painter, &text, style) + 1 };
            let caret_x = (text_x + typed_w).min(text_rect.right());
            canvas.fill(Rect::new(caret_x, field.y + (field.h - 18) / 2, 1, 18), 0.0, p.white.with_alpha(0.9));
        }
        let ready = !self.password.is_empty() && self.editable();
        canvas.fill(submit, shape::R_ROUND, if busy || ready { p.acc_bg } else { p.go_idle });
        if ready {
            self.layered(canvas, submit, shape::R_ROUND, p.acc_bg_hover, GreeterHit::Submit);
        }
        if !busy {
            let (x, y) = submit.centre_square(SUBMIT_SPINNER);
            canvas.icon(&svg(2.4, icon::ARROW_RIGHT), x, y, SUBMIT_SPINNER, p.white);
        }
    }

    fn spinner_spot(&self, part: Part, layout: &Layout) -> Option<Rect> {
        match part {
            Part::Toast if self.toast_label().is_some() => {
                let pill = layout.toast;
                Some(Rect::new(pill.x + 16, pill.y + (TOAST_H - TOAST_SPINNER) / 2, TOAST_SPINNER, TOAST_SPINNER))
            }
            Part::Field if self.auth == AuthState::Success => Some(layout.busy_spinner),
            Part::Field if self.locked_out().is_none() && self.busy() => {
                let (x, y) = layout.submit.centre_square(SUBMIT_SPINNER);
                Some(Rect::new(x, y, SUBMIT_SPINNER, SUBMIT_SPINNER))
            }
            _ => None,
        }
    }

    fn spinner_buffer(&mut self, side: i32, buffer_scale: i32, painter: &Painter) -> Option<&MemoryRenderBuffer> {
        let key = (side, buffer_scale, self.spinner_step());
        if !self.spinners.contains_key(&key) {
            if self.spinners.len() >= MAX_SPINNER_BUFFERS {
                self.spinners.clear();
            }
            let mut icons = std::mem::take(&mut self.icons);
            let mut canvas = Canvas::new(side, side, buffer_scale, painter, &mut icons);
            canvas.icon(&spinner_svg(key.2), 0, 0, side, self.palette.white);
            let buffer = canvas.finish();
            self.icons = icons;
            self.spinners.insert(key, buffer);
        }
        self.spinners.get(&key)
    }

    fn draw_users(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        for (index, rect) in &layout.users {
            let chip = rect.local_to(frame);
            canvas.fill(chip, shape::R_ROUND, p.white.with_alpha(0.06));
            self.layered(canvas, chip, shape::R_ROUND, p.white.with_alpha(0.05), GreeterHit::User(*index));
            let user = &self.users[*index];
            let avatar = Rect::new(chip.x + 4, chip.y + (CHIP_H - CHIP_AVATAR) / 2, CHIP_AVATAR, CHIP_AVATAR);
            let (from, to) = user_colors(user.uid);
            canvas.gradient(avatar, shape::R_ROUND, from, to);
            canvas.text(avatar, &user.initial.to_string(), p.white, &INITIAL_CHIP, TextAlign::Center);
            let label = Rect::new(avatar.right() + 8, chip.y, chip.right() - avatar.right() - 8 - 8, chip.h);
            canvas.text(label, &user.display_name, p.text.with_alpha(0.86), &CHIP_STYLE, TextAlign::Left);
        }
    }

    fn draw_media(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let Some(media) = self.media.as_ref() else {
            return;
        };
        let card = layout.media.local_to(frame);
        canvas.fill(card, shape::R3, p.card);
        canvas.stroke(card, 1.0, shape::R3, p.card_line);
        canvas.fill(Rect::new(card.x + 10, card.y + 1, card.w - 20, 1), 0.0, p.white.with_alpha(0.07));
        let tile = Rect::new(card.x + 8, card.y + 8, 44, 44);
        canvas.fill(tile, shape::R2, media.accent);
        canvas.stroke(tile, 1.0, shape::R2, p.white.with_alpha(0.16));
        let (x, y) = tile.centre_square(20);
        canvas.icon(&svg(1.8, icon::NOTE), x, y, 20, p.white.with_alpha(0.85));
        let [previous, play, next] = layout.media_buttons.map(|rect| rect.local_to(frame));
        let text_x = tile.right() + 12;
        let text_right = previous.x - 12;
        let heading = Rect::new(text_x, card.y + 11, text_right - text_x, 18);
        canvas.text(heading, &media.title, p.text, &MEDIA_TITLE_STYLE, TextAlign::Left);
        let subtitle = Rect::new(text_x, heading.bottom() + 2, text_right - text_x, 16);
        canvas.text(subtitle, &media.subtitle, p.muted, &MEDIA_SUB_STYLE, TextAlign::Left);
        for (rect, button, glyph) in
            [(previous, MediaButton::Previous, icon::PREVIOUS), (next, MediaButton::Next, icon::NEXT)]
        {
            self.layered(canvas, rect, shape::R_ROUND, p.hover, GreeterHit::Media(button));
            let (x, y) = rect.centre_square(14);
            canvas.icon(&filled(glyph), x, y, 14, p.text);
        }
        canvas.fill(play, shape::R_ROUND, p.acc_bg);
        self.layered(canvas, play, shape::R_ROUND, p.acc_bg_hover, GreeterHit::Media(MediaButton::PlayPause));
        let (x, y) = play.centre_square(14);
        canvas.icon(&filled(if media.playing { icon::PAUSE } else { icon::PLAY }), x, y, 14, p.white);
    }

    fn draw_notices(&self, layout: &Layout, frame: Rect, canvas: &mut Canvas<'_>) {
        let p = &self.palette;
        let card = layout.notices.local_to(frame);
        canvas.fill(card, shape::R3, p.card_soft);
        canvas.stroke(card, 1.0, shape::R3, p.card_line);
        let mut x = card.x + 10;
        let y = card.y + (NOTICES_H - 24) / 2;
        let shown = self.notices.apps.iter().take(3).count();
        for (index, app) in self.notices.apps.iter().take(3).enumerate() {
            let bubble = Rect::new(x, y, 24, 24);
            canvas.fill(bubble.inflate(2), shape::R_ROUND, Color::from_rgb8(36, 35, 42, 1.0));
            canvas.fill(bubble, shape::R_ROUND, bubble_colour(app));
            let initial = app.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_default();
            canvas.text(bubble, &initial, p.white, &BUBBLE_STYLE, TextAlign::Center);
            if index + 1 < shown {
                x += 10;
            }
        }
        let text_x = if shown == 0 { card.x + 14 } else { x + 24 + 10 };
        let note = t(Key::LockNoticesHidden);
        let note_w = text_w(canvas.painter, note, &NOTICE_NOTE_STYLE);
        let note_rect = Rect::new(card.right() - 14 - note_w, card.y, note_w + 2, NOTICES_H);
        canvas.text(note_rect, note, p.faint, &NOTICE_NOTE_STYLE, TextAlign::Left);
        let label = plural(
            glide_shell::i18n::language(),
            self.notices.count as u64,
            [Key::LockNoticesOne, Key::LockNoticesFew, Key::LockNoticesMany],
        );
        let label_rect = Rect::new(text_x, card.y, note_rect.x - 8 - text_x, NOTICES_H);
        canvas.text(label_rect, &label, p.text, &NOTICE_STYLE, TextAlign::Left);
    }

    fn parts(&self, leave: f64) -> Vec<(Part, (f64, f64), f64)> {
        let menu_t = self.menu_reveal.value(motion::t2());
        let ui = motion::assist(1.0 - leave);
        let (up, down) = (-SCENE_TRAVEL * leave, SCENE_TRAVEL * leave);
        let mut parts = Vec::with_capacity(11);
        if self.toast_label().is_some() {
            let duration = if self.toast_reveal.is_open() { motion::t3() } else { motion::t2() };
            let shown = self.toast_reveal.value(duration);
            parts.push((Part::Toast, (0.0, SCENE_TRAVEL * (1.0 - shown)), motion::assist(shown)));
        }
        if menu_t > 0.0 {
            parts.push((Part::Menu, (0.0, up - MENU_TRAVEL * (1.0 - menu_t)), motion::assist(menu_t) * ui));
        }
        parts.push((Part::Nav, (0.0, up), ui));
        parts.push((Part::Users, (0.0, down), if self.busy() { 0.0 } else { ui }));
        parts.push((Part::Hint, (0.0, down), ui));
        parts.push((Part::Field, (self.shake_offset(), down), ui));
        parts.push((Part::Name, (0.0, down), ui));
        parts.push((Part::Avatar, (0.0, down), ui));
        if self.notices.count > 0 {
            parts.push((Part::Notices, (0.0, up), ui));
        }
        if self.media.is_some() {
            parts.push((Part::Media, (0.0, up), ui));
        }
        parts.push((Part::Clock, (0.0, up), ui));
        parts
    }

    fn layer(&mut self, part: Part, layout: &Layout, painter: &Painter, buffer_scale: i32) -> Option<Rect> {
        let frame = Self::frame_of(part, layout);
        if frame.w <= 0 || frame.h <= 0 {
            return None;
        }
        let key = self.part_key(part, frame, glide_paint::painter_generation());
        let slot = (part, buffer_scale);
        if self.layers.get(&slot).is_none_or(|layer| layer.key != key) {
            let mut icons = std::mem::take(&mut self.icons);
            let mut canvas = Canvas::new(frame.w, frame.h, buffer_scale, painter, &mut icons);
            self.draw_part(part, layout, frame, &mut canvas);
            let buffer = canvas.finish();
            self.icons = icons;
            self.layers.insert(slot, Layer { key, buffer });
        }
        Some(frame)
    }

    pub fn render<R>(
        &mut self, renderer: &mut R, painter: &Painter, area: Rectangle<i32, Logical>, scale: Scale<f64>,
        buffer_scale: i32, primary: bool,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: glide_shell::raster::ShellRenderer,
    {
        let mut out = Vec::new();
        let size = area.size;
        let leave = self.fade();

        if primary && size.w > 0 && size.h > 0 && buffer_scale > 0 {
            let layout = self.compute_layout(painter, area);
            self.layout = Some(layout.clone());
            for (part, (dx, dy), alpha) in self.parts(leave) {
                if alpha <= 0.0 {
                    continue;
                }
                let placed = |at: Rect| {
                    let location = Point::<f64, Logical>::from((f64::from(at.x) + dx, f64::from(at.y) + dy));
                    let location = location.to_physical(scale);
                    Point::from((location.x.round(), location.y.round()))
                };
                let alpha = Some(alpha.clamp(0.0, 1.0) as f32);
                if let Some(spot) = self.spinner_spot(part, &layout)
                    && let Some(buffer) = self.spinner_buffer(spot.w, buffer_scale, painter)
                    && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                        renderer,
                        placed(spot),
                        buffer,
                        alpha,
                        None,
                        None,
                        Kind::Unspecified,
                    )
                {
                    out.push(element);
                }
                let Some(frame) = self.layer(part, &layout, painter, buffer_scale) else {
                    continue;
                };
                let Some(layer) = self.layers.get(&(part, buffer_scale)) else {
                    continue;
                };
                if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                    renderer,
                    placed(frame),
                    &layer.buffer,
                    alpha,
                    None,
                    None,
                    Kind::Unspecified,
                ) {
                    out.push(element);
                }
            }
            self.dirty = false;
        }

        let alpha = self.cover_alpha().clamp(0.0, 1.0) as f32;
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0.0, 0.0),
            &self.scrim,
            Some(alpha),
            Some(Rectangle::from_size((1.0, f64::from(SCRIM_ROWS)).into())),
            Some(size),
            Kind::Unspecified,
        ) {
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

fn composite_over(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
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

    fn view(&mut self) -> glide_shell::raster::Canvas<'static, &mut [u8]> {
        glide_shell::raster::Canvas::over(&mut self.buf, self.w, self.h, self.scale)
    }

    fn fill(&mut self, r: Rect, radius: f32, color: Color) {
        if color.0[3] > 0.0 && r.w > 0 && r.h > 0 {
            self.view().fill(glide_shell::raster::rect(r.x, r.y, r.w, r.h), color, radius);
        }
    }

    fn gradient(&mut self, r: Rect, radius: f32, from: Color, to: Color) {
        self.view().gradient(glide_shell::raster::rect(r.x, r.y, r.w, r.h), radius, from, to);
    }

    fn stroke(&mut self, r: Rect, width: f32, radius: f32, color: Color) {
        if color.0[3] > 0.0 {
            self.view().stroke(glide_shell::raster::rect(r.x, r.y, r.w, r.h), width, radius, color);
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
            .or_insert_with(|| glide_paint::render_svg_tinted(svg.as_bytes(), px, color).unwrap_or_default());
        if bytes.is_empty() {
            return;
        }
        glide_paint::blit_square_over(&mut self.buf, self.w, self.h, bytes, side, x, y, self.scale);
    }

    fn shadow(&mut self, r: Rect, radius: f32, dy: i32, blur: f32, color: Color) {
        let (w, h, scale, len) = (self.w, self.h, self.scale, self.buf.len());
        let mut key = Revision::new().str("shadow").color(color);
        for value in [w, h, scale, r.x, r.y, r.w, r.h, dy] {
            key = key.i32(value);
        }
        let key = key.u64(u64::from(radius.to_bits())).u64(u64::from(blur.to_bits())).finish();
        if self.icons.len() > MAX_CACHED_ICONS && !self.icons.contains_key(&key) {
            self.icons.clear();
        }
        let layer = self.icons.entry(key).or_insert_with(|| {
            let mut layer = vec![0u8; len];
            Painter::fill_rounded_rect_over(&mut layer, w, h, r.x, r.y + dy, r.w, r.h, scale, color, radius);
            glide_paint::box_blur_rgba(
                &mut layer,
                w * scale,
                h * scale,
                glide_paint::css_blur_box_radius(blur) * scale,
            );
            layer
        });
        composite_over(&mut self.buf, layer);
    }

    pub(crate) fn finish(self) -> MemoryRenderBuffer {
        glide_shell::raster::buffer_from(&self.buf, self.w, self.h, self.scale)
    }
}

pub(crate) fn wipe(secret: &mut String) {
    let len = secret.len();
    secret.clear();
    secret.extend(std::iter::repeat_n('\0', len));
    secret.clear();
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
    fn a_refusal_before_any_answer_waits_for_the_user_and_costs_no_attempt() {
        let mut ui = ui();
        ui.on_notice("System is going down".into(), true);
        assert!(!ui.take_answered());
        ui.refused_before_answer("authentication error".into());
        assert_eq!(ui.auth, AuthState::Error);
        assert_eq!(ui.failures, 0, "nobody typed a wrong password");
        assert_eq!(ui.message.as_ref().map(|(text, _)| text.as_str()), Some("System is going down"));
        assert!(ui.type_char('x'));
        assert_eq!(ui.submit(), Submit::CreateSession("artem".into()), "the session is created again on submit");
        assert_eq!(ui.on_question(true, "Password:".into()).as_deref(), Some("x"));
        assert!(ui.take_answered());
        assert!(!ui.take_answered());
    }

    #[test]
    fn only_a_lock_screen_that_is_fading_out_counts_as_unlocking() {
        let mut lock = GreeterUi::lock(users().remove(0), Accent::Blue);
        assert!(!lock.unlocking());
        lock.leave();
        assert!(lock.unlocking(), "a new lock request must replace a lock screen that is already letting go");
        let mut greeter = ui();
        greeter.leave();
        assert!(!greeter.unlocking(), "the login greeter leaving is a session start, not an unlock");
    }

    #[test]
    fn a_new_greeter_starts_idle_on_the_first_user_and_session() {
        let ui = ui();
        assert_eq!(ui.active_user().unwrap().username, "artem");
        assert_eq!(ui.active_session().unwrap().id, "glide");
        assert_eq!(ui.auth, AuthState::Idle);
        assert!(ui.armed().is_none());
        assert!(ui.menu.is_none());
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
    fn a_reason_from_pam_stays_until_the_next_key_but_a_wrong_password_fades() {
        let aged = clock::now().checked_sub(ERROR_HOLD * 2).expect("the clock is past the hold");
        let mut ui = GreeterUi::lock(users().remove(0), Accent::Blue);
        ui.deny = None;
        ui.set_auth_denied(Some("Authentication token is no longer valid.".into()), "Wrong password");
        ui.error_at = Some(aged);
        ui.tick();
        assert_eq!(ui.hint().0, "Authentication token is no longer valid.");
        ui.type_char('a');
        assert!(ui.message.is_none(), "typing dismisses it");

        ui.set_auth_denied(None, "Wrong password");
        assert_eq!(ui.hint().0, "Wrong password");
        ui.error_at = Some(aged);
        ui.tick();
        assert!(ui.message.is_none());
    }

    #[test]
    fn wrong_passwords_count_down_the_faillock_attempts() {
        let mut ui = ui();
        ui.deny = Some(3);
        ui.set_auth_denied(None, "Wrong password");
        let first = ui.hint().0;
        assert!(first.starts_with(t(Key::GreeterWrongShort)) && first.contains('2'), "{first}");
        ui.set_auth_denied(None, "Wrong password");
        let second = ui.hint().0;
        assert!(second.starts_with(t(Key::GreeterWrongShort)) && second.contains('1'), "{second}");
        ui.set_auth_denied(None, "Wrong password");
        assert_eq!(ui.hint().0, "Wrong password", "no count once the attempts are spent");
        ui.select_user(1);
        assert_eq!(ui.failures, 0, "another user starts afresh");
    }

    #[test]
    fn a_notice_from_an_earlier_conversation_never_locks_a_later_attempt() {
        let mut ui = ui();
        ui.deny = None;
        ui.on_notice("The account is locked due to 3 failed logins. (1 minutes left to unlock)".into(), false);
        assert_eq!(ui.on_question(true, "Password:".into()), None);
        ui.type_char('a');
        assert!(matches!(ui.submit(), Submit::Send(_)));
        ui.set_auth_denied(None, "Wrong password");
        assert!(ui.locked_out().is_some(), "the prompt belongs to the conversation the notice came from");
        ui.locked_until = None;
        assert_eq!(ui.on_question(true, "Password:".into()), None);
        ui.type_char('b');
        assert!(matches!(ui.submit(), Submit::Send(_)));
        ui.set_auth_denied(None, "Wrong password");
        assert!(ui.locked_out().is_none(), "a denial consumed the notice, so the next conversation starts clean");
        assert_eq!(ui.hint().0, "Wrong password");
    }

    #[test]
    fn a_faillock_notice_locks_the_field_with_a_countdown() {
        let mut ui = ui();
        ui.on_notice("The account is locked due to 3 failed logins.\n(10 minutes left to unlock)".into(), false);
        ui.set_auth_denied(None, "Wrong password");
        let left = ui.locked_out().expect("locked");
        assert!(left > Duration::from_mins(9) && left <= Duration::from_mins(10));
        assert!(!ui.type_char('a'), "nothing can be typed while locked out");
        assert_eq!(ui.hint().0, t(Key::GreeterLockedHint));
        ui.locked_until = clock::now().checked_sub(Duration::from_secs(1));
        ui.tick();
        assert!(ui.locked_out().is_none());
        assert!(ui.type_char('a'));
    }

    #[test]
    fn a_lockout_notice_survives_the_password_prompt_that_follows_it() {
        let mut ui = ui();
        ui.on_notice("The account is locked due to 3 failed logins.".into(), false);
        ui.on_notice("(10 minutes left to unlock)".into(), false);
        assert_eq!(ui.on_question(true, "Password:".into()), None);
        assert!(ui.message.is_none(), "the prompt still clears the shown notice");
        ui.type_char('a');
        assert!(matches!(ui.submit(), Submit::Send(_)));
        ui.set_auth_denied(None, "Wrong password");
        let left = ui.locked_out().expect("the notices that came before the prompt still lock the field");
        assert!(left > Duration::from_mins(9) && left <= Duration::from_mins(10));
        assert!(!ui.type_char('b'));
        assert_eq!(ui.hint().0, t(Key::GreeterLockedHint));
    }

    #[test]
    fn notices_do_not_leak_from_one_conversation_into_the_next_denial() {
        let mut ui = ui();
        ui.on_notice("The account is locked due to 3 failed logins. (10 minutes left to unlock)".into(), false);
        ui.set_auth_success();
        ui.on_notice("Welcome back".into(), false);
        ui.on_question(true, "Password:".into());
        ui.set_auth_denied(None, "Wrong password");
        assert!(ui.locked_out().is_none(), "a finished conversation leaves no lockout behind");

        let mut ui = self::ui();
        ui.on_notice("The account is locked due to 3 failed logins. (10 minutes left to unlock)".into(), false);
        ui.set_auth_error("greetd said no".into(), true);
        ui.on_question(true, "Password:".into());
        ui.set_auth_denied(None, "Wrong password");
        assert!(ui.locked_out().is_none(), "an error ends the conversation and its notices");

        let mut ui = self::ui();
        ui.on_notice("The account is locked due to 3 failed logins. (10 minutes left to unlock)".into(), false);
        ui.reset_for_user();
        ui.set_auth_denied(None, "Wrong password");
        assert!(ui.locked_out().is_none(), "switching user starts a new conversation");
    }

    #[test]
    fn lockout_messages_parse_in_both_languages_and_unrelated_ones_do_not() {
        assert_eq!(lockout_left("Wait 10 minutes"), None, "a lock word is required");
        assert_eq!(
            lockout_left("The account is locked due to 3 failed logins. (10 minutes left to unlock)"),
            Some(Duration::from_mins(10))
        );
        assert_eq!(lockout_left("Обліковий запис заблоковано (5 хв до розблокування)"), Some(Duration::from_mins(5)));
        assert_eq!(lockout_left("Last login: Mon 10 minutes ago"), None);
        assert_eq!(countdown(Duration::from_millis(581_200)), "9:42");
        assert_eq!(faillock_deny("# deny = 9\n deny= 5 # local\n"), 5);
        assert_eq!(faillock_deny("unlock_time = 600"), 3);
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
    fn cycling_and_picking_users_reset_the_password() {
        let mut ui = ui();
        ui.type_char('a');
        assert_eq!(ui.cycle_user(), Some("guest".into()));
        assert!(ui.password.is_empty());
        assert_eq!(ui.select_user(1), None, "picking the active user does nothing");
        assert_eq!(ui.select_user(0), Some("artem".into()));
    }

    #[test]
    fn escape_unwinds_arm_then_menu_then_password() {
        let mut ui = ui();
        ui.type_char('a');
        ui.toggle_session_menu();
        assert_eq!(ui.menu, Some(MenuKind::Session));
        ui.arm_power(PowerAction::Restart);
        assert_eq!(ui.menu, Some(MenuKind::Power), "arming opens the power menu");
        assert!(ui.cancel());
        assert!(ui.armed().is_none());
        assert!(ui.cancel());
        assert!(ui.menu.is_none());
        assert!(ui.cancel());
        assert!(ui.password.is_empty());
        assert!(!ui.cancel());
    }

    #[test]
    fn an_armed_action_expires_and_enter_confirms_only_with_an_empty_field() {
        let mut ui = ui();
        assert_eq!(ui.confirmable(), None);
        ui.arm_power(PowerAction::ShutDown);
        assert_eq!(ui.confirmable(), Some(PowerAction::ShutDown));
        ui.type_char('a');
        assert_eq!(ui.confirmable(), None, "a typed password is submitted instead");
        ui.armed_at = clock::now().checked_sub(ARM_HOLD * 2);
        ui.tick();
        assert_eq!(ui.armed(), None, "the second press has to come soon");
        assert_eq!(ui.menu, Some(MenuKind::Power), "the menu stays open");
    }

    #[test]
    fn the_lock_screen_offers_only_restart_and_shut_down() {
        let mut ui = GreeterUi::lock(users().remove(0), Accent::Blue);
        ui.arm_power(PowerAction::Sleep);
        assert_eq!(ui.armed(), None);
        ui.arm_power(PowerAction::Restart);
        assert_eq!(ui.armed(), Some(PowerAction::Restart));
        let layout = laid_out(&mut ui);
        assert_eq!(layout.menu_items.len(), 2);
        assert_eq!(ui.hit_test(centre(layout.menu_items[1])), GreeterHit::Power(PowerAction::ShutDown));
    }

    #[test]
    fn a_started_power_action_shows_a_centred_toast_that_leaves_on_its_own() {
        let mut ui = ui();
        assert!(ui.parts(0.0).iter().all(|(part, ..)| *part != Part::Toast));
        ui.power_started(PowerAction::ShutDown);
        assert_eq!(ui.toast_label(), Some(t(Key::GreeterToastShutDown)));
        let layout = laid_out(&mut ui);
        assert_eq!(layout.toast.x + layout.toast.w / 2, 960);
        assert_eq!(layout.toast.bottom(), 1080 - 32);
        assert_eq!(ui.parts(0.0)[0].0, Part::Toast, "the toast is drawn above everything");
        let stale = |ui: &mut GreeterUi| {
            ui.toast = ui.toast.map(|(action, _)| (action, clock::now().checked_sub(TOAST_HOLD * 2).unwrap()));
        };
        stale(&mut ui);
        ui.keep_toast();
        ui.tick();
        assert!(ui.toast_label().is_some(), "a session end still waiting for windows keeps the toast");
        stale(&mut ui);
        ui.tick();
        assert!(!ui.toast_reveal.is_open(), "the toast sinks away before it is dropped");
        ui.toast_reveal = Reveal::closed();
        ui.tick();
        assert_eq!(ui.toast_label(), None);
    }

    #[test]
    fn the_scene_leaves_by_travelling_and_its_opacity_is_the_shorter_track() {
        let mut ui = ui();
        ui.power_started(PowerAction::ShutDown);
        ui.toast_reveal = Reveal::open_now();
        let offset = |parts: &[(Part, (f64, f64), f64)], wanted: Part| {
            parts.iter().find(|(part, ..)| *part == wanted).map(|(_, (_, dy), alpha)| (*dy, *alpha)).unwrap()
        };
        let near = |(dy, alpha): (f64, f64), wanted: (f64, f64)| {
            (dy - wanted.0).abs() < 1e-9 && (alpha - wanted.1).abs() < 1e-9
        };
        let rest = ui.parts(0.0);
        for part in [Part::Clock, Part::Nav, Part::Avatar, Part::Field, Part::Toast] {
            assert!(near(offset(&rest, part), (0.0, 1.0)), "{part:?} rests in place");
        }
        let halfway = ui.parts(0.5);
        assert!(near(offset(&halfway, Part::Clock), (-SCENE_TRAVEL / 2.0, 1.0)), "opaque at half of the travel");
        assert!(near(offset(&halfway, Part::Field), (SCENE_TRAVEL / 2.0, 1.0)));
        let late = ui.parts(0.8);
        assert!(offset(&late, Part::Avatar).1 < 1.0, "opacity only drops in the last 40 % of the exit");
    }

    #[test]
    fn hover_crossfades_between_the_old_and_the_new_target() {
        let mut ui = ui();
        ui.update_hover(GreeterHit::Session);
        ui.update_hover(GreeterHit::PowerButton);
        ui.hover_at = clock::now() + Duration::from_mins(1);
        if !motion::t1().is_zero() {
            assert!(ui.hover_amount(GreeterHit::PowerButton) < 1.0, "the new target fades in");
            assert!(ui.hover_amount(GreeterHit::Session) > 0.0, "the old one fades out");
            assert!(ui.is_animating());
        }
        ui.hover_at = clock::now().checked_sub(Duration::from_secs(5)).unwrap();
        assert!((ui.hover_amount(GreeterHit::PowerButton) - 1.0).abs() < f32::EPSILON);
        assert!(ui.hover_amount(GreeterHit::Session).abs() < f32::EPSILON);
        assert_eq!(ui.hover_signature(Part::Field), "", "parts without the hovered control keep their raster");
        assert_ne!(ui.hover_signature(Part::Nav), "");
    }

    #[test]
    fn the_menu_layer_holds_its_whole_shadow() {
        let mut ui = ui();
        ui.toggle_power_menu();
        let painter = Painter::new(14.0, None);
        let layout = laid_out(&mut ui);
        let frame = GreeterUi::frame_of(Part::Menu, &layout);
        let mut icons = IconCache::new();
        let mut canvas = Canvas::new(frame.w, frame.h, 1, &painter, &mut icons);
        ui.draw_part(Part::Menu, &layout, frame, &mut canvas);
        let (w, h) = (frame.w as usize, frame.h as usize);
        let alpha = |x: usize, y: usize| canvas.buf[(y * w + x) * 4 + 3];
        let edge = (0..w)
            .flat_map(|x| [alpha(x, 0), alpha(x, h - 1)])
            .chain((0..h).flat_map(|y| [alpha(0, y), alpha(w - 1, y)]));
        assert!(edge.max().unwrap_or(0) <= 2, "the shadow is cut off at the layer edge");
    }

    #[test]
    fn selecting_a_session_closes_the_menu() {
        let mut ui = ui();
        ui.toggle_session_menu();
        ui.select_session(1);
        assert_eq!(ui.active_session().unwrap().id, "sway");
        assert!(ui.menu.is_none());
    }

    #[test]
    fn the_top_right_row_runs_layout_session_status_power() {
        let mut ui = ui();
        ui.set_keyboard_layout("UK".into());
        ui.set_status(GreeterStatus { battery: Some((87, false)), wifi: Some(3) });
        let layout = laid_out(&mut ui);
        assert_eq!(layout.nav.x, layout.kbd.x);
        let order = [layout.kbd, layout.session, layout.status, layout.power];
        for pair in order.windows(2) {
            assert!(pair[0].right() < pair[1].x, "{pair:?}");
        }
        assert_eq!(layout.power.right(), 1920 - NAV_RIGHT);
        assert_eq!(layout.separators.len(), 2);
        assert_eq!(ui.hit_test(centre(layout.kbd)), GreeterHit::KeyboardLayout);
        assert_eq!(ui.hit_test(centre(layout.session)), GreeterHit::Session);
        assert_eq!(ui.hit_test(centre(layout.power)), GreeterHit::PowerButton);
        ui.toggle_power_menu();
        let layout = laid_out(&mut ui);
        assert_eq!(layout.menu.right(), layout.power.right(), "the menu hangs under its button");
        assert!(layout.menu.y > layout.power.bottom());
        assert_eq!(ui.hit_test(centre(layout.menu_items[0])), GreeterHit::Power(PowerAction::Sleep));
    }

    #[test]
    fn the_auth_block_stacks_avatar_name_field_hint_and_other_users() {
        let mut ui = ui();
        let layout = laid_out(&mut ui);
        assert!(layout.clock.bottom() < layout.avatar.y);
        assert!(layout.avatar.bottom() < layout.name.y);
        assert!(layout.name.bottom() < layout.field.y);
        assert!(layout.field.bottom() < layout.hint.y);
        assert_eq!((layout.field.w, layout.field.h), (FIELD_W, FIELD_H));
        assert_eq!(layout.submit.right(), layout.field.right() - 5);
        let [(index, chip)] = layout.users[..] else { panic!("one other user") };
        assert_eq!(index, 1);
        assert!(chip.y > layout.hint.bottom());
        assert_eq!(ui.hit_test(centre(chip)), GreeterHit::User(1));
        assert_eq!(ui.hit_test(centre(layout.submit)), GreeterHit::Submit);
        assert_eq!(ui.hit_test(centre(layout.field)), GreeterHit::PasswordField);
        assert_eq!(ui.hit_test(centre(layout.avatar)), GreeterHit::UserAvatar);
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
    fn the_lock_screen_has_media_and_notices_above_the_field_and_no_session() {
        let mut ui = GreeterUi::lock(users().remove(0), Accent::Blue);
        ui.set_media(Some(LockMedia {
            title: "Nightcall".into(),
            subtitle: "Kavinsky · Spotify".into(),
            playing: true,
            accent: Color::from_rgb8(107, 63, 143, 1.0),
        }));
        ui.set_notices(LockNotices { count: 3, apps: vec!["Telegram".into(), "Пошта".into()] });
        let layout = laid_out(&mut ui);
        assert_eq!(layout.session, Rect::default());
        assert!(layout.users.is_empty());
        assert!(layout.clock.bottom() < layout.media.y);
        assert!(layout.media.bottom() < layout.notices.y);
        assert!(layout.notices.bottom() + 60 <= layout.avatar.y);
        assert_eq!(ui.hit_test(centre(layout.media_buttons[1])), GreeterHit::Media(MediaButton::PlayPause));
        ui.toggle_session_menu();
        assert!(ui.menu.is_none());
        assert_eq!(ui.cycle_user(), None);
        assert_eq!(ui.hint().0, t(Key::LockHint));
        ui.set_auth_success();
        assert_eq!(ui.busy_label(), t(Key::LockUnlocking));
    }

    #[test]
    fn the_last_user_and_session_are_preselected_when_they_still_exist() {
        use crate::LastLogin;
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
            icon::WIFI,
            icon::KEYBOARD,
            icon::CLOCK,
            icon::NOTE,
        ] {
            assert!(glide_paint::render_svg(svg(2.0, body).as_bytes(), side).is_some(), "{body}");
        }
        for body in [icon::PLAY, icon::PAUSE, icon::PREVIOUS, icon::NEXT] {
            assert!(glide_paint::render_svg(filled(body).as_bytes(), side).is_some(), "{body}");
        }
        assert!(glide_paint::render_svg(battery_svg(50).as_bytes(), side).is_some());
        assert!(glide_paint::render_svg(spinner_svg(7).as_bytes(), side).is_some());
    }

    #[test]
    fn the_power_icon_stroke_starts_at_the_top_of_the_ring() {
        let side = 48;
        let bytes = glide_paint::render_svg(svg(2.0, icon::POWER).as_bytes(), side).unwrap();
        let alpha_at = |x: i32, y: i32| bytes[usize::try_from((y * side + x) * 4 + 3).unwrap()];
        assert!(alpha_at(side / 2, side / 8) > 0, "stem reaches the top");
        assert_eq!(alpha_at(side / 2, side * 5 / 8), 0, "no stem below the centre");
    }

    fn compose(ui: &mut GreeterUi, painter: &Painter, (w, h): (i32, i32), scale: i32) -> Vec<u8> {
        let layout = ui.compute_layout(painter, Rectangle::from_size((w, h).into()));
        ui.layout = Some(layout.clone());
        let mut image = glide_shell::raster::Canvas::new(w, h, scale).expect("canvas");
        let full = image.full();
        image.gradient(full, 0.0, Color::from_rgb8(31, 42, 70, 1.0), Color::from_rgb8(40, 30, 64, 1.0));
        for row in 0..h {
            let alpha = 0.1 + 0.35 * f64::from(row) / f64::from(h);
            image.fill(glide_shell::raster::rect(0, row, w, 1), Color::from_rgb8(10, 10, 14, alpha as f32), 0.0);
        }
        let mut parts = ui.parts(0.0);
        parts.reverse();
        for (part, _, alpha) in parts {
            if alpha <= 0.0 {
                continue;
            }
            let frame = GreeterUi::frame_of(part, &layout);
            if frame.w <= 0 || frame.h <= 0 {
                continue;
            }
            let mut icons = IconCache::new();
            let mut canvas = Canvas::new(frame.w, frame.h, scale, painter, &mut icons);
            ui.draw_part(part, &layout, frame, &mut canvas);
            image.blit(&canvas.buf, glide_shell::raster::rect(frame.x, frame.y, frame.w, frame.h));
            if let Some(spot) = ui.spinner_spot(part, &layout) {
                let mut canvas = Canvas::new(spot.w, spot.h, scale, painter, &mut icons);
                canvas.icon(&spinner_svg(ui.spinner_step()), 0, 0, spot.w, ui.palette.white);
                image.blit(&canvas.buf, glide_shell::raster::rect(spot.x, spot.y, spot.w, spot.h));
            }
        }
        image.buf
    }

    #[test]
    fn a_turning_spinner_never_redraws_the_field_or_the_pill_it_sits_on() {
        let painter = Painter::new(14.0, None);
        let area = Rectangle::from_size((1280, 800).into());
        let mut ui = GreeterUi::new(users(), Vec::new(), Accent::Blue);
        ui.auth = AuthState::Authenticating;
        ui.power_started(PowerAction::Restart);
        let layout = ui.compute_layout(&painter, area);
        let keys = |ui: &GreeterUi| {
            [Part::Field, Part::Toast].map(|part| ui.part_key(part, GreeterUi::frame_of(part, &layout), 0))
        };
        let before = keys(&ui);
        ui.spinner_epoch = clock::now().checked_sub(SPINNER_PERIOD / 3).expect("a past instant");
        assert_eq!(keys(&ui), before, "the step is not part of either raster");

        let submit = ui.spinner_spot(Part::Field, &layout).expect("the go button spins");
        assert_eq!((submit.w, submit.h), (SUBMIT_SPINNER, SUBMIT_SPINNER));
        assert_eq!(ui.spinner_spot(Part::Toast, &layout).map(|spot| spot.w), Some(TOAST_SPINNER));
        assert_eq!(ui.spinner_spot(Part::Clock, &layout), None);

        ui.auth = AuthState::Success;
        let layout = ui.compute_layout(&painter, area);
        assert_eq!(ui.spinner_spot(Part::Field, &layout), Some(layout.busy_spinner));
        assert_eq!(layout.busy_spinner.w, BUSY_SPINNER);
        assert_ne!(ui.part_key(Part::Field, GreeterUi::frame_of(Part::Field, &layout), 0), before[0]);

        ui.auth = AuthState::Idle;
        assert_eq!(ui.spinner_spot(Part::Field, &layout), None, "an idle field has an arrow, not a spinner");
    }

    #[test]
    #[ignore = "writes preview PNGs to $GLIDE_GREETER_PREVIEW"]
    fn preview() {
        let Some(dir) = std::env::var_os("GLIDE_GREETER_PREVIEW") else {
            return;
        };
        let painter = Painter::new(14.0, None);
        let size = (1280, 800);
        let users = vec![
            UserEntry { username: "artem".into(), display_name: "Артем".into(), uid: 1000, initial: 'А' },
            UserEntry { username: "olena".into(), display_name: "Олена".into(), uid: 1001, initial: 'О' },
            UserEntry { username: "guest".into(), display_name: "Гість".into(), uid: 1002, initial: 'Г' },
        ];
        let sessions = ["Glide", "GNOME", "Plasma (Wayland)"]
            .into_iter()
            .map(|name| SessionEntry {
                id: name.to_lowercase(),
                name: name.into(),
                exec: vec![name.into()],
                desktop_names: String::new(),
            })
            .collect::<Vec<_>>();
        let status = GreeterStatus { battery: Some((87, false)), wifi: Some(3) };
        let login = || {
            let mut ui = GreeterUi::new(users.clone(), sessions.clone(), Accent::Blue);
            ui.set_keyboard_layout("UK".into());
            ui.set_status(status);
            ui.time = "16:40".into();
            ui.date = "ПʼЯТНИЦЯ · 24 ЖОВТНЯ".into();
            ui.deny = Some(3);
            ui
        };
        let lock = || {
            let mut ui = GreeterUi::lock(users[0].clone(), Accent::Blue);
            ui.set_keyboard_layout("UK".into());
            ui.set_status(status);
            ui.time = "16:40".into();
            ui.date = "ПʼЯТНИЦЯ · 24 ЖОВТНЯ".into();
            ui.set_media(Some(LockMedia {
                title: "Nightcall".into(),
                subtitle: "Kavinsky · Spotify".into(),
                playing: true,
                accent: Color::from_rgb8(107, 63, 143, 1.0),
            }));
            ui.set_notices(LockNotices {
                count: 3, apps: vec!["Telegram".into(), "Пошта".into(), "Glide".into()]
            });
            ui
        };
        let mut cases: Vec<(&str, GreeterUi)> = Vec::new();
        cases.push(("login", login()));
        let mut typed = login();
        "secret".chars().for_each(|ch| {
            typed.type_char(ch);
        });
        cases.push(("login-typed", typed));
        let mut wrong = login();
        wrong.set_auth_denied(None, "Невірний пароль");
        wrong.error_at = None;
        cases.push(("login-wrong", wrong));
        let mut locked = login();
        locked.on_notice("The account is locked due to 3 failed logins. (10 minutes left to unlock)".into(), false);
        locked.set_auth_denied(None, "x");
        locked.error_at = None;
        cases.push(("login-lockout", locked));
        let mut power = login();
        power.arm_power(PowerAction::ShutDown);
        power.menu_reveal = Reveal::open_now();
        cases.push(("login-power", power));
        let mut session = login();
        session.toggle_session_menu();
        session.menu_reveal = Reveal::open_now();
        cases.push(("login-session", session));
        let mut toast = lock();
        toast.power_started(PowerAction::ShutDown);
        cases.push(("lock-toast", toast));
        let mut entering = login();
        entering.set_auth_success();
        cases.push(("login-entering", entering));
        cases.push(("lock", lock()));
        let mut caps = lock();
        caps.set_caps_lock(true);
        cases.push(("lock-caps", caps));
        for (name, mut ui) in cases {
            let pixels = compose(&mut ui, &painter, size, 1);
            let image = glide_shell::image::Image::from_bgra(1280, 800, pixels).expect("image");
            let png = image.png().expect("png");
            std::fs::write(std::path::Path::new(&dir).join(format!("greeter-{name}.png")), png).expect("write");
        }
    }
}
