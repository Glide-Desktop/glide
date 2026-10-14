use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use glide_theme::{
    Color, Revision, TextStyle,
    motion::{self, ease_out},
};
use smithay::{
    backend::renderer::element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
    },
    utils::{Logical, Point, Rectangle, Scale, Size},
};

use crate::{
    greeter::ui::{Canvas, IconCache, Rect, solid_pixel, svg},
    i18n::{Key, t},
    paint::{Painter, TextAlign},
};

pub const ENV: &str = "GLIDE_HANDOFF";
pub const MAX_WAIT: Duration = Duration::from_secs(4);
const SPINNER: i32 = 36;
const LABEL_GAP: i32 = 16;
const LABEL_H: i32 = 22;
const CONTENT_W: i32 = 360;
const SPINNER_PERIOD: Duration = Duration::from_millis(800);
const SPINNER_STEPS: u32 = 48;
const RING: &str = r#"<circle cx="12" cy="12" r="10"/>"#;
const LABEL_STYLE: TextStyle =
    TextStyle { size_px: 15.0, line_height_px: 22.0, weight: 550, tracking_em: 0.0, uppercase: false, tabular: false };
const BACKDROP: [u8; 4] = [0x16, 0x13, 0x14, 0xff];
const KDSETMODE: libc::Ioctl = 0x4B3A;
const KD_TEXT: libc::c_int = 0;

#[must_use]
pub fn console_sequence() -> String {
    let [b, g, r, _] = BACKDROP;
    format!("\x1b]P0{r:02x}{g:02x}{b:02x}\x1b[0;40m\x1b[2J\x1b[H\x1b[?25l")
}

fn open_console() -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let vt: u32 = std::env::var("XDG_VTNR").ok()?.trim().parse().ok()?;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
        .open(format!("/dev/tty{vt}"))
        .inspect_err(|err| tracing::debug!(?err, vt, "cannot open the session VT"))
        .ok()
}

pub fn paint_console() {
    use std::{io::Write, os::fd::AsRawFd};
    let Some(mut console) = open_console() else {
        return;
    };
    if let Err(err) = console.write_all(console_sequence().as_bytes()) {
        tracing::debug!(?err, "cannot recolour the session VT");
        return;
    }
    // SAFETY: `console` is an open VT descriptor for the duration of the call
    // and KDSETMODE takes its mode argument by value.
    let switched = unsafe { libc::ioctl(console.as_raw_fd(), KDSETMODE, KD_TEXT) };
    if switched == 0 {
        tracing::info!("session VT recoloured and switched to text mode for the handoff");
    } else {
        tracing::debug!(
            err = %std::io::Error::last_os_error(),
            "VT recoloured; leaving the mode switch to the seat manager"
        );
    }
}

fn arc(step: u32) -> String {
    let degrees = step * 360 / SPINNER_STEPS;
    format!(r#"<path d="M12 2a10 10 0 0 1 10 10" transform="rotate({degrees} 12 12)"/>"#)
}

fn content_rect(size: Size<i32, Logical>) -> Rect {
    let h = SPINNER + LABEL_GAP + LABEL_H;
    Rect::new((size.w - CONTENT_W) / 2, (size.h - h) / 2, CONTENT_W, h)
}

pub struct Cover {
    backdrop: MemoryRenderBuffer,
    content: HashMap<i32, (u64, MemoryRenderBuffer)>,
    icons: IconCache,
    epoch: Instant,
}

impl Default for Cover {
    fn default() -> Self {
        Self::new()
    }
}

impl Cover {
    #[must_use]
    pub fn new() -> Self {
        Self { backdrop: solid_pixel(BACKDROP), content: HashMap::new(), icons: HashMap::new(), epoch: Instant::now() }
    }

    fn spinner_step(&self) -> u32 {
        let turns = self.epoch.elapsed().as_secs_f64() / SPINNER_PERIOD.as_secs_f64();
        let step = (turns.fract() * f64::from(SPINNER_STEPS)) as u32;
        step % SPINNER_STEPS
    }

    fn content_buffer(&mut self, painter: &Painter, buffer_scale: i32) -> &MemoryRenderBuffer {
        let step = self.spinner_step();
        let label = t(Key::GreeterLoggingIn);
        let key = Revision::new().u64(u64::from(step)).u64(crate::paint::painter_generation()).str(label).finish();
        let stale = self.content.get(&buffer_scale).is_none_or(|(cached, _)| *cached != key);
        if stale {
            let frame_h = SPINNER + LABEL_GAP + LABEL_H;
            let mut canvas = Canvas::new(CONTENT_W, frame_h, buffer_scale, painter, &mut self.icons);
            let x = (CONTENT_W - SPINNER) / 2;
            let white = Color([1.0, 1.0, 1.0, 1.0]);
            canvas.icon(&svg(2.0, RING), x, 0, SPINNER, white.with_alpha(0.22));
            canvas.icon(&svg(2.0, &arc(step)), x, 0, SPINNER, white);
            let label_rect = Rect::new(0, SPINNER + LABEL_GAP, CONTENT_W, LABEL_H);
            let fg = Color::from_rgb8(242, 240, 237, 0.72);
            canvas.text(label_rect, label, fg, &LABEL_STYLE, TextAlign::Center);
            self.content.insert(buffer_scale, (key, canvas.finish()));
        }
        &self.content[&buffer_scale].1
    }

    pub fn render<R>(
        &mut self, renderer: &mut R, painter: &Painter, size: Size<i32, Logical>, scale: Scale<f64>, buffer_scale: i32,
        alpha: f64,
    ) -> Vec<MemoryRenderBufferRenderElement<R>>
    where
        R: crate::raster::ShellRenderer,
    {
        let mut out = Vec::new();
        if alpha <= 0.0 || size.w <= 0 || size.h <= 0 || buffer_scale <= 0 {
            return out;
        }
        let alpha = alpha.clamp(0.0, 1.0) as f32;
        let content = content_rect(size);
        let location = Point::<f64, Logical>::from((f64::from(content.x), f64::from(content.y))).to_physical(scale);
        let location = Point::from((location.x.round(), location.y.round()));
        let buffer = self.content_buffer(painter, buffer_scale);
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            buffer,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        ) {
            out.push(element);
        }
        if let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            (0.0, 0.0),
            &self.backdrop,
            Some(alpha),
            Some(Rectangle::from_size((1.0, 1.0).into())),
            Some(size),
            Kind::Unspecified,
        ) {
            out.push(element);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Arriving,
    Waiting,
    Revealing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Readiness {
    pub status: bool,
    pub apps: bool,
    pub frames: u32,
}

impl Readiness {
    #[must_use]
    pub const fn complete(self) -> bool {
        self.status && self.apps && self.frames >= 2
    }
}

pub struct Handoff {
    phase: Phase,
    since: Instant,
    created: Instant,
    readiness: Readiness,
    opaque_frames: u32,
    pub cover: Cover,
}

impl Handoff {
    #[must_use]
    pub fn arriving() -> Self {
        Self::with_phase(Phase::Arriving)
    }

    #[must_use]
    pub fn waiting() -> Self {
        Self::with_phase(Phase::Waiting)
    }

    fn with_phase(phase: Phase) -> Self {
        let now = Instant::now();
        Self { phase, since: now, created: now, readiness: Readiness::default(), opaque_frames: 0, cover: Cover::new() }
    }

    #[must_use]
    pub fn from_environment() -> Option<Self> {
        let requested = std::env::var_os(ENV).is_some_and(|value| value == "1");
        // SAFETY: called once on the main thread at startup, next to the
        // existing session `set_var` calls and before any session worker spawns.
        unsafe { std::env::remove_var(ENV) };
        requested.then(|| {
            tracing::info!("arrived from the greeter; holding the handoff cover until ready");
            Self::waiting()
        })
    }

    fn progress(&self) -> f64 {
        let duration = motion::t4();
        if duration.is_zero() {
            return 1.0;
        }
        ease_out((self.since.elapsed().as_secs_f64() / duration.as_secs_f64()).min(1.0))
    }

    #[must_use]
    pub fn alpha(&self) -> f64 {
        match self.phase {
            Phase::Arriving => self.progress(),
            Phase::Waiting => 1.0,
            Phase::Revealing => 1.0 - self.progress(),
        }
    }

    #[must_use]
    pub const fn blocks_input(&self) -> bool {
        !matches!(self.phase, Phase::Revealing)
    }

    pub fn mark_status(&mut self) {
        self.readiness.status = true;
    }

    pub fn mark_apps(&mut self) {
        self.readiness.apps = true;
    }

    pub fn mark_frame(&mut self) {
        self.readiness.frames = self.readiness.frames.saturating_add(1);
        if self.alpha() >= 1.0 {
            self.opaque_frames = self.opaque_frames.saturating_add(1);
        }
    }

    #[must_use]
    pub const fn fully_covered(&self) -> bool {
        matches!(self.phase, Phase::Arriving) && self.opaque_frames > 0
    }

    #[must_use]
    pub fn should_reveal(&self) -> bool {
        self.readiness.complete() || self.created.elapsed() >= MAX_WAIT
    }

    pub fn tick(&mut self) -> bool {
        if self.phase == Phase::Waiting && self.should_reveal() {
            tracing::info!(
                waited_ms = self.created.elapsed().as_millis(),
                readiness = ?self.readiness,
                "session ready; revealing the desktop"
            );
            self.phase = Phase::Revealing;
            self.since = Instant::now();
        }
        !(self.phase == Phase::Revealing && self.since.elapsed() >= motion::t4())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_needs_status_apps_and_two_frames() {
        let mut ready = Readiness::default();
        assert!(!ready.complete());
        ready.status = true;
        ready.apps = true;
        ready.frames = 1;
        assert!(!ready.complete(), "the first frame may still be settling");
        ready.frames = 2;
        assert!(ready.complete());
    }

    #[test]
    fn a_waiting_cover_stays_opaque_and_blocks_input_until_ready() {
        let mut handoff = Handoff::waiting();
        assert!(handoff.tick());
        assert!((handoff.alpha() - 1.0).abs() < f64::EPSILON);
        assert!(handoff.blocks_input());
        handoff.mark_status();
        handoff.mark_apps();
        handoff.mark_frame();
        handoff.mark_frame();
        handoff.tick();
        assert!(!handoff.blocks_input(), "input flows once the reveal starts");
    }

    #[test]
    fn a_stuck_session_is_revealed_after_the_maximum_wait() {
        let mut handoff = Handoff::waiting();
        handoff.created = Instant::now().checked_sub(MAX_WAIT).unwrap();
        assert!(handoff.should_reveal());
        handoff.tick();
        assert!(!handoff.blocks_input());
    }

    #[test]
    fn an_arriving_cover_is_only_fully_covered_after_an_opaque_frame() {
        let mut handoff = Handoff::arriving();
        assert!(!handoff.fully_covered());
        handoff.since = Instant::now().checked_sub(Duration::from_secs(5)).unwrap();
        assert!((handoff.alpha() - 1.0).abs() < f64::EPSILON);
        assert!(!handoff.fully_covered(), "must be presented, not just computed");
        handoff.mark_frame();
        assert!(handoff.fully_covered());
    }

    #[test]
    fn the_console_is_repainted_in_the_cover_colour_with_the_cursor_hidden() {
        let sequence = console_sequence();
        assert!(sequence.starts_with("\x1b]P0141316"), "palette slot 0 must match the cover backdrop #141316");
        assert!(sequence.contains("\x1b[2J"), "the screen is cleared");
        assert!(sequence.ends_with("\x1b[?25l"), "the text cursor is hidden");
    }

    #[test]
    fn the_content_is_centred_on_the_output() {
        let rect = content_rect((1920, 1080).into());
        assert_eq!(rect.x * 2 + rect.w, 1920);
        assert!((rect.y * 2 + rect.h - 1080).abs() <= 1);
    }
}
