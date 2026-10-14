use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use glide_theme::{
    Color,
    motion::{ease_out as ease, lerp},
};
use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        utils::RescaleRenderElement,
    },
    utils::{Logical, Point, Rectangle, Scale},
};

use crate::i18n::Key;
use crate::power::PowerAction;

pub const ACTIONS: [PowerAction; 5] =
    [PowerAction::Lock, PowerAction::Sleep, PowerAction::Restart, PowerAction::ShutDown, PowerAction::LogOut];
pub const CANCEL: usize = ACTIONS.len();
pub const CONFIRM: usize = ACTIONS.len() + 1;
pub const TARGETS: usize = ACTIONS.len() + 2;
pub const COUNTDOWN: Duration = Duration::from_secs(60);
pub const COUNTDOWN_TICK: Duration = Duration::from_millis(250);

const COLUMNS: i32 = 5;
const AVATAR: i32 = 96;
const NAME_GAP: i32 = 16;
const NAME_H: i32 = 28;
const ROW_GAP: i32 = 48;
const BUTTON: i32 = 64;
const GLYPH: i32 = 28;
const SLOT: i32 = 128;
const CAPTION_GAP: i32 = 12;
const CAPTION_H: i32 = 20;
const QUESTION_GAP: i32 = 32;
const QUESTION_H: i32 = 24;
const NOTE_GAP: i32 = 4;
const NOTE_H: i32 = 24;
const BAR_GAP: i32 = 24;
const PILL_GAP: i32 = 12;
const PILL_H: i32 = 36;
const CANCEL_W: i32 = 112;
const CONFIRM_W: i32 = 164;
const RING_PAD: i32 = 4;
const RING_WIDTH: f32 = 2.0;
const CONTENT_W: i32 = 640;
const OPEN_SCALE: f64 = 0.96;
const QUESTION_RISE: f64 = 4.0;
const WELL_ALPHA: f32 = 0.14;
const SCRIM_ALPHA: f32 = 0.45;

const ROW_Y: i32 = AVATAR + NAME_GAP + NAME_H + ROW_GAP;
const ROW_H: i32 = BUTTON + CAPTION_GAP + CAPTION_H;
const QUESTION_Y: i32 = ROW_Y + ROW_H + QUESTION_GAP;
const NOTE_Y: i32 = QUESTION_Y + QUESTION_H + NOTE_GAP;
const BAR_Y: i32 = NOTE_Y + NOTE_H + BAR_GAP;
const CONTENT_H: i32 = BAR_Y + PILL_H;
const SOLO_X: i32 = (CONTENT_W - CANCEL_W) / 2;
const PAIR_X: i32 = (CONTENT_W - CANCEL_W - PILL_GAP - CONFIRM_W) / 2;

pub type PowerDialogRenderElement<R> = RescaleRenderElement<MemoryRenderBufferRenderElement<R>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Armed,
    Perform(PowerAction),
    Cancel,
}

#[must_use]
pub const fn is_destructive(action: PowerAction) -> bool {
    matches!(action, PowerAction::LogOut | PowerAction::Restart | PowerAction::ShutDown)
}

#[must_use]
pub fn index_of(action: PowerAction) -> usize {
    ACTIONS.iter().position(|candidate| *candidate == action).unwrap_or(0)
}

const fn caption_key(action: PowerAction) -> Key {
    match action {
        PowerAction::Lock => Key::PowerLock,
        PowerAction::Sleep => Key::CommandSleep,
        PowerAction::Restart => Key::CommandRestart,
        PowerAction::ShutDown => Key::CommandShutDown,
        PowerAction::LogOut => Key::CommandLogOut,
    }
}

#[must_use]
pub const fn question_key(action: PowerAction) -> Key {
    match action {
        PowerAction::Lock => Key::PowerAskLock,
        PowerAction::Sleep => Key::PowerAskSleep,
        PowerAction::Restart => Key::PowerAskRestart,
        PowerAction::ShutDown => Key::PowerAskShutDown,
        PowerAction::LogOut => Key::PowerAskLogOut,
    }
}

#[must_use]
pub const fn confirm_key(action: PowerAction) -> Key {
    match action {
        PowerAction::Lock => Key::PowerLock,
        PowerAction::Sleep => Key::PowerConfirmSleep,
        PowerAction::Restart => Key::PowerConfirmRestart,
        PowerAction::ShutDown => Key::CommandShutDown,
        PowerAction::LogOut => Key::PowerConfirmLogOut,
    }
}

const fn countdown_key(action: PowerAction) -> Option<Key> {
    match action {
        PowerAction::Lock | PowerAction::Sleep => None,
        PowerAction::Restart => Some(Key::PowerCountdownRestart),
        PowerAction::ShutDown => Some(Key::PowerCountdownShutDown),
        PowerAction::LogOut => Some(Key::PowerCountdownLogOut),
    }
}

const fn icon_names(action: PowerAction) -> &'static [&'static str] {
    match action {
        PowerAction::Lock => &["system-lock-screen-symbolic", "changes-prevent-symbolic"],
        PowerAction::Sleep => &["system-suspend-symbolic", "weather-clear-night-symbolic"],
        PowerAction::Restart => &["system-reboot-symbolic", "view-refresh-symbolic"],
        PowerAction::ShutDown => &["system-shutdown-symbolic"],
        PowerAction::LogOut => &["system-log-out-symbolic", "application-exit-symbolic"],
    }
}

#[must_use]
pub fn countdown_text(action: PowerAction, seconds: u64) -> Option<String> {
    countdown_key(action).map(|key| crate::i18n::t(key).replace("{n}", &seconds.to_string()))
}

#[must_use]
pub fn scrim_color() -> [f32; 4] {
    let [r, g, b, _] = Color::from_rgb8(16, 15, 14, 1.0).0;
    [r * SCRIM_ALPHA, g * SCRIM_ALPHA, b * SCRIM_ALPHA, SCRIM_ALPHA]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub content: Rectangle<i32, Logical>,
}

impl Geometry {
    #[must_use]
    pub fn new(area: Rectangle<i32, Logical>) -> Self {
        let x = area.loc.x + (area.size.w - CONTENT_W) / 2;
        let y = area.loc.y + (area.size.h - CONTENT_H) / 2;
        Self { content: rect(x, y, CONTENT_W, CONTENT_H) }
    }

    fn slot_x(index: usize) -> i32 {
        let column = i32::try_from(index).unwrap_or(0);
        (CONTENT_W - SLOT * COLUMNS) / 2 + SLOT * column
    }

    fn cancel_x(arm: f64) -> i32 {
        lerp(f64::from(SOLO_X), f64::from(PAIR_X), arm.clamp(0.0, 1.0)).round() as i32
    }

    fn pill(index: usize, arm: f64) -> Rectangle<i32, Logical> {
        if index == CANCEL {
            rect(Self::cancel_x(arm), BAR_Y, CANCEL_W, PILL_H)
        } else {
            rect(PAIR_X + CANCEL_W + PILL_GAP, BAR_Y, CONFIRM_W, PILL_H)
        }
    }

    #[must_use]
    pub fn well(index: usize, armed: bool) -> Rectangle<i32, Logical> {
        if index < CANCEL {
            rect(Self::slot_x(index) + (SLOT - BUTTON) / 2, ROW_Y, BUTTON, BUTTON)
        } else {
            Self::pill(index, if armed { 1.0 } else { 0.0 })
        }
    }

    fn hit_rect(index: usize, armed: bool) -> Rectangle<i32, Logical> {
        if index < CANCEL { rect(Self::slot_x(index), ROW_Y, SLOT, ROW_H) } else { Self::well(index, armed) }
    }

    #[must_use]
    pub fn contains(&self, pos: Point<f64, Logical>) -> bool {
        self.content.to_f64().contains(pos)
    }

    #[must_use]
    pub fn target_at(&self, pos: Point<f64, Logical>, armed: bool) -> Option<usize> {
        let local = pos - self.content.loc.to_f64();
        (0..target_count(armed)).find(|index| Self::hit_rect(*index, armed).to_f64().contains(local))
    }
}

const fn target_count(armed: bool) -> usize {
    if armed { TARGETS } else { CONFIRM }
}

#[derive(Debug, Clone, Copy)]
struct Countdown {
    action: PowerAction,
    deadline: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub danger: Color,
    pub ring: Color,
}

impl Palette {
    #[must_use]
    pub fn from_tokens(tokens: &glide_theme::Tokens) -> Self {
        Self {
            text: tokens.fg_1,
            muted: tokens.fg_2,
            accent: tokens.acc_bg,
            accent_text: tokens.acc_fg,
            danger: tokens.danger,
            ring: tokens.acc_ring,
        }
    }

    fn revision(&self) -> glide_theme::Revision {
        glide_theme::Revision::new()
            .color(self.text)
            .color(self.muted)
            .color(self.accent)
            .color(self.accent_text)
            .color(self.danger)
            .color(self.ring)
    }

    const fn confirm_fill(&self, action: PowerAction) -> Color {
        if is_destructive(action) { self.danger } else { self.accent }
    }
}

struct Raster {
    key: u64,
    content: MemoryRenderBuffer,
    cancel: MemoryRenderBuffer,
    confirm: Vec<MemoryRenderBuffer>,
    question: Vec<MemoryRenderBuffer>,
    hover: Vec<MemoryRenderBuffer>,
    ring: Vec<MemoryRenderBuffer>,
    note: Option<(String, MemoryRenderBuffer)>,
}

pub struct PowerDialog {
    user: String,
    focus: Option<usize>,
    hovered: Option<usize>,
    armed: Option<PowerAction>,
    shown: Option<PowerAction>,
    countdown: Option<Countdown>,
    hover: [glide_ui::Tween; TARGETS],
    ring: [glide_ui::Tween; TARGETS],
    arm: glide_ui::Tween,
    ask: glide_ui::Tween,
    reveal: glide_ui::Reveal,
    closing: bool,
    rasters: HashMap<i32, Raster>,
}

fn light_one(tweens: &mut [glide_ui::Tween], lit: Option<usize>) {
    for (index, tween) in tweens.iter_mut().enumerate() {
        let to = if Some(index) == lit { 1.0 } else { 0.0 };
        tween.retarget(to, glide_theme::motion::t1(), ease);
    }
}

impl PowerDialog {
    #[must_use]
    pub fn new(user: String, focus: Option<PowerAction>, now: Instant) -> Self {
        let mut reveal = glide_ui::Reveal::closed().with_ease(ease);
        reveal.open(glide_theme::motion::t3());
        let mut dialog = Self {
            user,
            focus: None,
            hovered: None,
            armed: None,
            shown: None,
            countdown: None,
            hover: [glide_ui::Tween::at(0.0); TARGETS],
            ring: [glide_ui::Tween::at(0.0); TARGETS],
            arm: glide_ui::Tween::at(0.0),
            ask: glide_ui::Tween::at(0.0),
            reveal,
            closing: false,
            rasters: HashMap::new(),
        };
        dialog.refocus(focus, now);
        dialog
    }

    pub fn refocus(&mut self, focus: Option<PowerAction>, now: Instant) {
        match focus {
            Some(action) => self.arm(action, now),
            None => self.disarm(),
        }
        self.set_focus(focus.map(index_of));
    }

    fn arm(&mut self, action: PowerAction, now: Instant) {
        if self.armed != Some(action) {
            self.ask.jump(0.0);
            self.ask.retarget(1.0, glide_theme::motion::t1(), ease);
        }
        self.armed = Some(action);
        self.shown = Some(action);
        self.arm.retarget(1.0, glide_theme::motion::t2(), ease);
        self.countdown = is_destructive(action).then(|| Countdown { action, deadline: now + COUNTDOWN });
    }

    fn disarm(&mut self) {
        self.armed = None;
        self.countdown = None;
        self.arm.retarget(0.0, glide_theme::motion::t2(), ease);
        if self.focus == Some(CONFIRM) {
            self.set_focus(None);
        }
    }

    #[must_use]
    pub const fn focus(&self) -> Option<usize> {
        self.focus
    }

    #[must_use]
    pub const fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    #[must_use]
    pub const fn armed(&self) -> Option<PowerAction> {
        self.armed
    }

    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }

    fn set_focus(&mut self, focus: Option<usize>) -> bool {
        if let Some(countdown) = self.countdown
            && focus != Some(index_of(countdown.action))
            && focus != Some(CONFIRM)
        {
            self.countdown = None;
        }
        if focus == self.focus {
            return false;
        }
        light_one(&mut self.ring, focus);
        self.focus = focus;
        true
    }

    pub fn step(&mut self, delta: i32) -> bool {
        let len = i32::try_from(target_count(self.is_armed())).unwrap_or(1);
        let next = match self.focus {
            Some(current) => (i32::try_from(current).unwrap_or(0) + delta).rem_euclid(len),
            None if delta < 0 => len - 1,
            None => 0,
        };
        self.set_focus(usize::try_from(next).ok())
    }

    fn choose(&mut self, index: usize, now: Instant) -> Outcome {
        match index {
            CANCEL => Outcome::Cancel,
            CONFIRM => self.armed.map_or(Outcome::Nothing, Outcome::Perform),
            _ => match ACTIONS.get(index).copied() {
                Some(action) if self.armed == Some(action) => Outcome::Perform(action),
                Some(action) => {
                    self.arm(action, now);
                    Outcome::Armed
                }
                None => Outcome::Nothing,
            },
        }
    }

    pub fn activate(&mut self, now: Instant) -> Outcome {
        match self.focus {
            Some(index) => self.choose(index, now),
            None => Outcome::Nothing,
        }
    }

    pub fn press(&mut self, geometry: Geometry, pos: Point<f64, Logical>, now: Instant) -> Outcome {
        match geometry.target_at(pos, self.is_armed()) {
            Some(index) => self.choose(index, now),
            None if geometry.contains(pos) => Outcome::Nothing,
            None => Outcome::Cancel,
        }
    }

    pub fn hover(&mut self, target: Option<usize>) -> bool {
        if target == self.hovered {
            return false;
        }
        light_one(&mut self.hover, target);
        self.hovered = target;
        true
    }

    #[must_use]
    pub fn countdown_action(&self) -> Option<PowerAction> {
        self.countdown.map(|countdown| countdown.action)
    }

    #[must_use]
    pub fn remaining(&self, now: Instant) -> Option<u64> {
        self.countdown.map(|countdown| {
            let left = countdown.deadline.saturating_duration_since(now);
            left.as_secs() + u64::from(left.subsec_nanos() > 0)
        })
    }

    #[must_use]
    pub fn due(&self, now: Instant) -> Option<PowerAction> {
        self.countdown.filter(|countdown| now >= countdown.deadline).map(|countdown| countdown.action)
    }

    #[must_use]
    pub const fn counting(&self) -> bool {
        self.countdown.is_some() && !self.closing
    }

    #[must_use]
    pub const fn is_closing(&self) -> bool {
        self.closing
    }

    pub fn begin_close(&mut self) {
        let opening = self.duration();
        self.reveal.close(opening);
        self.closing = true;
        self.countdown = None;
    }

    fn duration(&self) -> Duration {
        if self.closing { glide_theme::motion::t2() } else { glide_theme::motion::t3() }
    }

    pub fn tick(&mut self) -> bool {
        let duration = self.duration();
        self.reveal.settle(duration);
        !self.closing || self.reveal.is_visible(duration)
    }

    #[must_use]
    pub fn is_animating(&self) -> bool {
        let layers = glide_theme::motion::t1();
        self.reveal.is_animating(self.duration())
            || self.arm.is_animating(glide_theme::motion::t2())
            || self.ask.is_animating(layers)
            || self.hover.iter().chain(self.ring.iter()).any(|tween| tween.is_animating(layers))
    }

    #[must_use]
    pub fn openness(&self) -> f64 {
        self.reveal.progress(self.duration(), ease).clamp(0.0, 1.0)
    }

    fn arm_level(&self) -> f64 {
        self.arm.value(glide_theme::motion::t2(), ease).clamp(0.0, 1.0)
    }

    fn shown_prompt(&self) -> Option<usize> {
        self.armed.or(self.shown).map(index_of)
    }

    fn ask_level(&self) -> f64 {
        self.ask.value(glide_theme::motion::t1(), ease).clamp(0.0, 1.0)
    }

    fn raster_key(&self, theme_revision: u64, palette: &Palette) -> u64 {
        let mut revision = palette
            .revision()
            .u64(theme_revision)
            .u64(crate::paint::painter_generation())
            .str(&self.user)
            .str(crate::i18n::t(Key::PowerCancel));
        for action in ACTIONS {
            revision = revision
                .str(crate::i18n::t(caption_key(action)))
                .str(crate::i18n::t(question_key(action)))
                .str(crate::i18n::t(confirm_key(action)));
        }
        revision.finish()
    }
}

pub(crate) use crate::raster::rect;

pub(crate) struct Sheet<'a> {
    canvas: crate::raster::Canvas<'static>,
    painter: &'a crate::paint::Painter,
}

impl std::ops::Deref for Sheet<'_> {
    type Target = crate::raster::Canvas<'static>;
    fn deref(&self) -> &Self::Target {
        &self.canvas
    }
}

impl std::ops::DerefMut for Sheet<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.canvas
    }
}

impl Sheet<'_> {
    pub fn text(&mut self, band: Rectangle<i32, Logical>, text: &str, color: Color, style: &glide_theme::TextStyle) {
        self.canvas.text_in(self.painter, band, crate::paint::TextAlign::Center, text, color, style);
    }

    fn well(&mut self, rect: Rectangle<i32, Logical>, color: Color) {
        self.canvas.fill(rect, color, round_radius(rect));
    }
}

pub(crate) fn paint(
    (w, h): (i32, i32), scale: i32, painter: &crate::paint::Painter, draw: impl FnOnce(&mut Sheet<'_>),
) -> Option<MemoryRenderBuffer> {
    let mut sheet = Sheet { canvas: crate::raster::Canvas::new(w, h, scale)?, painter };
    draw(&mut sheet);
    Some(sheet.finish())
}

fn round_radius(rect: Rectangle<i32, Logical>) -> f32 {
    let half = rect.size.w.min(rect.size.h) as f32 / 2.0;
    glide_theme::shape::R_ROUND.min(half)
}

fn initial_of(text: &str) -> String {
    text.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

fn draw_content(
    user: &str, palette: &Palette, icons: &mut crate::icon::IconLoader, painter: &crate::paint::Painter, scale: i32,
) -> Option<MemoryRenderBuffer> {
    paint((CONTENT_W, ROW_Y + ROW_H), scale, painter, |sheet| {
        draw_header(sheet, user, palette);
        draw_buttons(sheet, palette, icons);
    })
}

fn draw_header(sheet: &mut Sheet<'_>, user: &str, palette: &Palette) {
    let avatar = rect((sheet.w - AVATAR) / 2, 0, AVATAR, AVATAR);
    sheet.well(avatar, palette.accent);
    let initial = initial_of(user);
    sheet.text(avatar, &initial, palette.accent_text, &glide_theme::TextStyle::DISPLAY);
    let name = rect(0, AVATAR + NAME_GAP, sheet.w, NAME_H);
    sheet.text(name, user, palette.text, &glide_theme::TextStyle::T2);
}

fn draw_buttons(sheet: &mut Sheet<'_>, palette: &Palette, icons: &mut crate::icon::IconLoader) {
    let well_color = palette.text.with_alpha(WELL_ALPHA);
    for (index, action) in ACTIONS.into_iter().enumerate() {
        let well = Geometry::well(index, false);
        sheet.well(well, well_color);
        let caption = crate::i18n::t(caption_key(action));
        let glyph_x = well.loc.x + (BUTTON - GLYPH) / 2;
        let glyph_y = well.loc.y + (BUTTON - GLYPH) / 2;
        let drawn = icon_names(action).iter().any(|name| {
            icons.load_symbolic(name, GLYPH * sheet.scale, palette.text).is_some_and(|pixels| {
                sheet.blit_square(pixels, GLYPH, (glyph_x, glyph_y));
                true
            })
        });
        if !drawn {
            let initial = initial_of(caption);
            sheet.text(well, &initial, palette.text, &glide_theme::TextStyle::T2);
        }
        let band = rect(Geometry::slot_x(index), ROW_Y + BUTTON + CAPTION_GAP, SLOT, CAPTION_H);
        sheet.text(band, caption, palette.text, &glide_theme::TextStyle::CALL);
    }
}

fn draw_pill(
    width: i32, label: &str, (fill, text): (Color, Color), painter: &crate::paint::Painter, scale: i32,
) -> Option<MemoryRenderBuffer> {
    paint((width, PILL_H), scale, painter, |sheet| {
        sheet.well(sheet.full(), fill);
        sheet.text(sheet.full(), label, text, &glide_theme::TextStyle::HEAD);
    })
}

fn draw_line(
    text: &str, (color, style, height): (Color, &glide_theme::TextStyle, i32), painter: &crate::paint::Painter,
    scale: i32,
) -> Option<MemoryRenderBuffer> {
    paint((CONTENT_W, height), scale, painter, |sheet| {
        sheet.text(sheet.full(), text, color, style);
    })
}

fn draw_layer(index: usize, color: Color, painter: &crate::paint::Painter, scale: i32) -> Option<MemoryRenderBuffer> {
    let size = Geometry::well(index, true).size;
    paint((size.w, size.h), scale, painter, |sheet| {
        sheet.well(sheet.full(), color);
    })
}

fn draw_ring(index: usize, color: Color, painter: &crate::paint::Painter, scale: i32) -> Option<MemoryRenderBuffer> {
    let well = Geometry::well(index, true).size;
    let (w, h) = (well.w + RING_PAD * 2, well.h + RING_PAD * 2);
    paint((w, h), scale, painter, |sheet| {
        let radius = round_radius(sheet.full());
        sheet.stroke(rect(0, 0, w, h), RING_WIDTH, radius, color);
    })
}

fn draw_raster(
    dialog: &PowerDialog, key: u64, palette: &Palette, icons: &mut crate::icon::IconLoader,
    painter: &crate::paint::Painter, scale: i32,
) -> Option<Raster> {
    let content = draw_content(&dialog.user, palette, icons, painter, scale)?;
    let cancel = draw_pill(
        CANCEL_W,
        crate::i18n::t(Key::PowerCancel),
        (palette.text.with_alpha(WELL_ALPHA), palette.text),
        painter,
        scale,
    )?;
    let confirm = ACTIONS
        .into_iter()
        .map(|action| {
            draw_pill(
                CONFIRM_W,
                crate::i18n::t(confirm_key(action)),
                (palette.confirm_fill(action), palette.accent_text),
                painter,
                scale,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let question = ACTIONS
        .into_iter()
        .map(|action| {
            draw_line(
                crate::i18n::t(question_key(action)),
                (palette.text, &glide_theme::TextStyle::T3, QUESTION_H),
                painter,
                scale,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let hover = (0..TARGETS)
        .map(|index| {
            let base = if index == CONFIRM { palette.accent_text } else { palette.text };
            let color = base.with_alpha(glide_theme::motion::ST_HOVER);
            draw_layer(index, color, painter, scale)
        })
        .collect::<Option<Vec<_>>>()?;
    let ring = (0..TARGETS).map(|index| draw_ring(index, palette.ring, painter, scale)).collect::<Option<Vec<_>>>()?;
    Some(Raster { key, content, cancel, confirm, question, hover, ring, note: None })
}

type Layer<'a> = (&'a MemoryRenderBuffer, Point<i32, Logical>, f64);

fn place_layers<'a>(
    raster: &'a Raster, armed: Option<usize>, (arm, ask): (f64, f64),
    (hovers, rings): (&[f64; TARGETS], &[f64; TARGETS]),
) -> Vec<Layer<'a>> {
    let reveal = arm.min(ask);
    let rect_of = |index: usize| {
        if index < CANCEL { Geometry::well(index, false) } else { Geometry::pill(index, arm) }
    };
    let pill_alpha = |index: usize| if index == CONFIRM { reveal } else { 1.0 };
    let mut layers: Vec<Layer<'a>> = Vec::new();
    for (index, alpha) in rings.iter().enumerate() {
        let alpha = alpha * pill_alpha(index);
        if alpha > 0.0 {
            let loc = rect_of(index).loc - Point::from((RING_PAD, RING_PAD));
            layers.push((&raster.ring[index], loc, alpha));
        }
    }
    if let Some((_, note)) = raster.note.as_ref() {
        layers.push((note, Point::from((0, NOTE_Y)), arm));
    }
    if let Some(index) = armed
        && reveal > 0.0
    {
        let rise = (QUESTION_RISE * (1.0 - ask)).round() as i32;
        if let Some(question) = raster.question.get(index) {
            layers.push((question, Point::from((0, QUESTION_Y + rise)), reveal));
        }
    }
    for (index, alpha) in hovers.iter().enumerate() {
        let alpha = alpha * pill_alpha(index);
        if alpha > 0.0 {
            layers.push((&raster.hover[index], rect_of(index).loc, alpha));
        }
    }
    if let Some(index) = armed
        && reveal > 0.0
        && let Some(confirm) = raster.confirm.get(index)
    {
        layers.push((confirm, rect_of(CONFIRM).loc, reveal));
    }
    layers.push((&raster.cancel, rect_of(CANCEL).loc, 1.0));
    layers.push((&raster.content, Point::from((0, 0)), 1.0));
    layers
}

#[allow(clippy::too_many_arguments)]
pub fn render<R>(
    dialog: &mut PowerDialog, renderer: &mut R, icons: &mut crate::icon::IconLoader, painter: &crate::paint::Painter,
    theme_revision: u64, palette: &Palette, area: Rectangle<i32, Logical>, scale: Scale<f64>, output_scale: i32,
) -> Vec<PowerDialogRenderElement<R>>
where
    R: crate::raster::ShellRenderer,
{
    let openness = dialog.openness();
    if openness <= 0.0 {
        return Vec::new();
    }
    let geometry = Geometry::new(area);
    let key = dialog.raster_key(theme_revision, palette);
    if dialog.rasters.get(&output_scale).is_none_or(|raster| raster.key != key) {
        dialog.rasters.remove(&output_scale);
        if let Some(raster) = draw_raster(dialog, key, palette, icons, painter, output_scale) {
            dialog.rasters.insert(output_scale, raster);
        }
    }

    let layers_duration = glide_theme::motion::t1();
    let hovers: [f64; TARGETS] = std::array::from_fn(|index| dialog.hover[index].value(layers_duration, ease));
    let rings: [f64; TARGETS] = std::array::from_fn(|index| dialog.ring[index].value(layers_duration, ease));
    let levels = (dialog.arm_level(), dialog.ask_level());
    let armed = dialog.shown_prompt();
    let note_text = dialog
        .countdown_action()
        .zip(dialog.remaining(Instant::now()))
        .and_then(|(action, seconds)| countdown_text(action, seconds));

    let Some(raster) = dialog.rasters.get_mut(&output_scale) else {
        return Vec::new();
    };
    match note_text {
        Some(text) => {
            if raster.note.as_ref().is_none_or(|(drawn, _)| *drawn != text) {
                raster.note =
                    draw_line(&text, (palette.muted, &glide_theme::TextStyle::BODY, NOTE_H), painter, output_scale)
                        .map(|buffer| (text, buffer));
            }
        }
        None => raster.note = None,
    }

    let layers = place_layers(raster, armed, levels, (&hovers, &rings));

    let grow = OPEN_SCALE + (1.0 - OPEN_SCALE) * openness;
    let centre = Point::<i32, Logical>::from((
        geometry.content.loc.x + geometry.content.size.w / 2,
        geometry.content.loc.y + geometry.content.size.h / 2,
    ))
    .to_f64()
    .to_physical(scale)
    .to_i32_round();

    layers
        .into_iter()
        .filter_map(|(buffer, local, alpha)| {
            let loc = (geometry.content.loc + local).to_f64().to_physical(scale);
            let alpha = (openness * alpha).clamp(0.0, 1.0) as f32;
            MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                loc,
                buffer,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            )
            .ok()
        })
        .map(|element| RescaleRenderElement::from_element(element, centre, grow))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rectangle<i32, Logical> {
        rect(0, 0, 1920, 1080)
    }

    fn dialog(focus: Option<PowerAction>, now: Instant) -> PowerDialog {
        PowerDialog::new("Artem".to_string(), focus, now)
    }

    fn centre_of(geometry: Geometry, local: Rectangle<i32, Logical>) -> Point<f64, Logical> {
        let loc = geometry.content.loc + local.loc;
        Point::from((
            f64::from(loc.x) + f64::from(local.size.w) / 2.0,
            f64::from(loc.y) + f64::from(local.size.h) / 2.0,
        ))
    }

    #[test]
    fn the_column_count_matches_the_action_list() {
        assert_eq!(usize::try_from(COLUMNS).ok(), Some(ACTIONS.len()));
        let row = Geometry::slot_x(ACTIONS.len() - 1) + SLOT - Geometry::slot_x(0);
        assert!(Geometry::slot_x(0) >= 0 && row <= CONTENT_W, "the button row fits the content");
    }

    #[test]
    fn the_layout_sits_on_the_4px_grid() {
        for value in [
            AVATAR,
            NAME_GAP,
            NAME_H,
            ROW_GAP,
            BUTTON,
            GLYPH,
            SLOT,
            CAPTION_GAP,
            CAPTION_H,
            QUESTION_GAP,
            QUESTION_H,
            NOTE_GAP,
            NOTE_H,
            BAR_GAP,
            PILL_GAP,
            PILL_H,
            CANCEL_W,
            CONFIRM_W,
            RING_PAD,
            CONTENT_W,
            CONTENT_H,
            SOLO_X,
            PAIR_X,
        ] {
            assert_eq!(value % 4, 0, "{value} is off the 4px grid");
        }
    }

    #[test]
    fn cancel_and_confirm_sit_side_by_side_centred_under_the_question() {
        let cancel = Geometry::well(CANCEL, true);
        let confirm = Geometry::well(CONFIRM, true);
        assert_eq!(cancel.loc.x + cancel.size.w + PILL_GAP, confirm.loc.x);
        assert_eq!(cancel.loc.x, CONTENT_W - (confirm.loc.x + confirm.size.w));
        let solo = Geometry::well(CANCEL, false);
        assert_eq!(solo.loc.x * 2 + solo.size.w, CONTENT_W);
        const { assert!(QUESTION_Y > ROW_Y + ROW_H && NOTE_Y > QUESTION_Y && BAR_Y > NOTE_Y) };
    }

    #[test]
    fn focus_wraps_across_every_button_cancel_and_confirm() {
        let now = Instant::now();
        let mut dialog = dialog(Some(PowerAction::Lock), now);
        assert_eq!(dialog.focus(), Some(0));
        for (step, focus) in [(-1, CONFIRM), (-1, CANCEL), (2, 0)] {
            assert!(dialog.step(step));
            assert_eq!(dialog.focus(), Some(focus));
        }
        for _ in 0..TARGETS {
            dialog.step(1);
        }
        assert_eq!(dialog.focus(), Some(0));
        for (step, focus) in [(1, 0), (-1, CANCEL)] {
            let mut unfocused = self::dialog(None, now);
            assert_eq!(unfocused.focus(), None);
            assert!(unfocused.step(step));
            assert_eq!(unfocused.focus(), Some(focus), "a hidden confirm is skipped");
        }
    }

    #[test]
    fn destructive_actions_count_down_until_focus_leaves_or_the_dialog_closes() {
        let now = Instant::now();
        for action in ACTIONS {
            assert_eq!(dialog(Some(action), now).counting(), is_destructive(action), "{action:?}");
            assert_eq!(countdown_text(action, 5).is_some(), is_destructive(action));
        }
        assert!(!dialog(None, now).counting());

        let shutdown = dialog(Some(PowerAction::ShutDown), now);
        assert_eq!(shutdown.remaining(now), Some(60));
        assert_eq!(shutdown.remaining(now + Duration::from_millis(59_500)), Some(1));
        assert_eq!(shutdown.due(now + Duration::from_secs(59)), None);
        assert_eq!(shutdown.due(now + COUNTDOWN), Some(PowerAction::ShutDown));
        assert_eq!(shutdown.remaining(now + COUNTDOWN), Some(0));

        let mut restart = dialog(Some(PowerAction::Restart), now);
        restart.step(1);
        assert!(!restart.counting() && restart.due(now + COUNTDOWN).is_none());

        let mut logout = dialog(Some(PowerAction::LogOut), now);
        assert_eq!(logout.focus(), Some(index_of(PowerAction::LogOut)));
        logout.step(2);
        assert_eq!(logout.focus(), Some(CONFIRM));
        assert!(logout.counting(), "focus on the confirm pill keeps the countdown");
        assert_eq!(logout.activate(now), Outcome::Perform(PowerAction::LogOut));
        logout.begin_close();
        assert!(logout.is_closing() && !logout.counting());
        assert_eq!(logout.due(now + COUNTDOWN), None);
    }

    #[test]
    fn closing_mid_open_starts_from_the_visible_openness() {
        let now = Instant::now();
        let mut dialog = dialog(None, now);
        let half = glide_theme::motion::t3() / 2;
        if half.is_zero() {
            return;
        }
        std::thread::sleep(half);
        let before = dialog.openness();
        dialog.begin_close();
        let after = dialog.openness();
        assert!(before > 0.0 && before < 1.0);
        assert!((before - after).abs() < 0.05, "closing jumped from {before} to {after}");
    }

    #[test]
    fn disarming_keeps_the_question_until_it_has_faded() {
        let now = Instant::now();
        let mut dialog = dialog(Some(PowerAction::Restart), now);
        dialog.refocus(None, now);
        assert_eq!(dialog.armed(), None);
        assert_eq!(dialog.shown_prompt(), Some(index_of(PowerAction::Restart)));
    }

    #[test]
    fn opening_focused_on_an_action_already_asks_about_it() {
        let now = Instant::now();
        for action in ACTIONS {
            let mut dialog = dialog(Some(action), now);
            assert_eq!(dialog.armed(), Some(action));
            assert_eq!(dialog.activate(now), Outcome::Perform(action));
        }
        assert_eq!(dialog(None, now).armed(), None);
    }

    #[test]
    fn the_first_activation_arms_and_the_second_performs() {
        let now = Instant::now();
        let mut dialog = dialog(None, now);
        assert_eq!(dialog.activate(now), Outcome::Nothing);
        dialog.step(1);
        dialog.step(3);
        assert_eq!(dialog.focus(), Some(index_of(PowerAction::ShutDown)));
        assert_eq!(dialog.activate(now), Outcome::Armed);
        assert_eq!(dialog.armed(), Some(PowerAction::ShutDown));
        assert!(dialog.counting(), "arming a destructive action counts down");
        assert_eq!(dialog.activate(now), Outcome::Perform(PowerAction::ShutDown));
    }

    #[test]
    fn arming_another_action_replaces_the_question() {
        let now = Instant::now();
        let mut dialog = dialog(Some(PowerAction::Restart), now);
        dialog.step(-1);
        assert_eq!(dialog.activate(now), Outcome::Armed);
        assert_eq!(dialog.armed(), Some(PowerAction::Sleep));
        assert!(!dialog.counting(), "sleep never counts down");
        dialog.step(-1);
        assert_eq!(dialog.activate(now), Outcome::Armed);
        assert_eq!(dialog.armed(), Some(PowerAction::Lock));
    }

    #[test]
    fn cancel_always_cancels_and_confirm_needs_an_armed_action() {
        let now = Instant::now();
        let mut dialog = dialog(Some(PowerAction::Sleep), now);
        dialog.step(4);
        assert_eq!(dialog.focus(), Some(CANCEL));
        assert_eq!(dialog.activate(now), Outcome::Cancel);
        assert_eq!(dialog.choose(CONFIRM, now), Outcome::Perform(PowerAction::Sleep));
        dialog.refocus(None, now);
        assert_eq!(dialog.choose(CONFIRM, now), Outcome::Nothing);
    }

    #[test]
    fn a_press_arms_then_confirms_and_an_outside_click_cancels() {
        let geometry = Geometry::new(area());
        let now = Instant::now();
        for (index, action) in ACTIONS.into_iter().enumerate() {
            let mut dialog = dialog(None, now);
            let centre = centre_of(geometry, Geometry::well(index, false));
            assert_eq!(geometry.target_at(centre, false), Some(index));
            assert_eq!(dialog.press(geometry, centre, now), Outcome::Armed);
            let confirm = centre_of(geometry, Geometry::well(CONFIRM, true));
            assert_eq!(geometry.target_at(confirm, true), Some(CONFIRM));
            assert_eq!(dialog.press(geometry, confirm, now), Outcome::Perform(action));
            assert_eq!(dialog.press(geometry, centre, now), Outcome::Perform(action));
        }
        let mut idle = dialog(None, now);
        let confirm = centre_of(geometry, Geometry::well(CONFIRM, true));
        assert_ne!(geometry.target_at(confirm, false), Some(CONFIRM));
        let cancel = centre_of(geometry, Geometry::well(CANCEL, false));
        assert_eq!(idle.press(geometry, cancel, now), Outcome::Cancel);
        let avatar = centre_of(geometry, rect(0, 0, CONTENT_W, AVATAR));
        assert_eq!(idle.press(geometry, avatar, now), Outcome::Nothing);
        assert_eq!(idle.press(geometry, Point::from((4.0, 4.0)), now), Outcome::Cancel);
    }

    #[test]
    fn every_action_asks_its_own_question_and_names_its_own_button() {
        for (index, action) in ACTIONS.into_iter().enumerate() {
            let question = crate::i18n::t(question_key(action));
            assert!(question.ends_with('?'), "{question}");
            assert!(!crate::i18n::t(confirm_key(action)).is_empty());
            for other in ACTIONS.into_iter().skip(index + 1) {
                assert_ne!(question_key(action), question_key(other));
                assert_ne!(confirm_key(action), confirm_key(other));
            }
        }
    }

    #[test]
    fn destructive_confirmation_is_a_danger_pill() {
        let palette =
            Palette::from_tokens(&glide_theme::Tokens::new(glide_theme::Appearance::Dark, glide_theme::Accent::Blue));
        for action in ACTIONS {
            let expected = if is_destructive(action) { palette.danger } else { palette.accent };
            assert_eq!(palette.confirm_fill(action), expected);
        }
    }

    #[test]
    fn the_content_is_centred_over_a_premultiplied_scrim_and_hover_tracks_one_target() {
        let content = Geometry::new(area()).content;
        assert_eq!(content.loc.x * 2 + content.size.w, area().size.w);
        assert_eq!(content.loc.y * 2 + content.size.h, area().size.h);
        let [r, g, b, a] = scrim_color();
        assert!(r <= a && g <= a && b <= a);
        assert!((a - SCRIM_ALPHA).abs() < f32::EPSILON);
        let mut dialog = dialog(None, Instant::now());
        assert!(dialog.hover(Some(2)) && !dialog.hover(Some(2)));
        assert_eq!(dialog.hovered(), Some(2));
        assert!(dialog.hover(None));
    }
}
