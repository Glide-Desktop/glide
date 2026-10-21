pub mod compose;
pub mod model;
pub mod render;

use std::os::unix::net::UnixStream;
use std::sync::Arc;

use smithay::input::keyboard::{Keysym, ModifiersState};
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::{state::GlideState, window::GlideWindow};
use compose::{Frame, Image, Shadow};
use model::{Area, Mode, Outcome, Shot, Tool};

const LEFT: u32 = 0x110;

pub struct Session {
    pub output: Output,
    pub origin: Point<i32, Logical>,
    pub bounds: Size<i32, Logical>,
    pub scale: f64,
    pub shot: Option<Shot>,
    pub frozen: Option<Arc<Image>>,
    pub windows: Vec<Candidate>,
    pub window_image: Option<Arc<Image>>,
    pub window_generation: usize,
    pub waiters: Vec<UnixStream>,
    pub cache: render::Cache,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub window: GlideWindow,
    pub frame: Area,
    pub clip: f32,
    pub radius: f32,
}

pub struct WindowCapture {
    pub window: GlideWindow,
    pub output: Output,
    pub clip: f32,
    pub radius: f32,
    pub direct: bool,
}

impl Session {
    #[must_use]
    pub fn awaiting_freeze(&self, output: &Output) -> bool {
        self.output == *output && self.frozen.is_none()
    }

    #[must_use]
    pub fn shows_on(&self, output: &Output) -> bool {
        self.output == *output && self.shot.is_some()
    }

    #[must_use]
    pub fn scene(&self) -> Option<render::Scene<'_>> {
        let shot = self.shot.as_ref()?;
        let frozen = self.frozen.as_deref()?;
        let chosen = shot.window.and_then(|index| self.windows.get(index));
        let window = self.window_image.as_deref().zip(chosen.map(|candidate| candidate.frame));
        Some(render::Scene {
            shot,
            sources: render::Sources { frozen, window, scale: self.scale },
            window_generation: self.window_generation,
            radius: shot.hovered.and_then(|index| self.windows.get(index)).map_or(0.0, |candidate| candidate.radius),
        })
    }

    fn local(&self, pos: Point<f64, Logical>) -> model::Pos {
        pos - self.origin.to_f64()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShotKey {
    Cancel,
    Confirm,
    Copy,
    Undo,
    Redo,
    Mode(Mode),
    CycleMode(i8),
    ToggleShadow,
    Tool(Tool),
    Swatch(usize),
    Nudge { dx: i8, dy: i8, big: bool, grow: bool },
    Type(String),
    Backspace,
    CommitText,
    None,
}

#[must_use]
pub fn shortcut(
    modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym, typing: bool, editing: bool,
) -> ShotKey {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let ctrl = modifiers.ctrl;
    if matches(Keysym::Escape) {
        return if typing { ShotKey::CommitText } else { ShotKey::Cancel };
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return if typing { ShotKey::CommitText } else { ShotKey::Confirm };
    }
    if ctrl && !modifiers.alt {
        if matches(Keysym::c) || matches(Keysym::Insert) {
            return ShotKey::Copy;
        }
        if matches(Keysym::s) {
            return ShotKey::Confirm;
        }
        if matches(Keysym::z) {
            return if modifiers.shift { ShotKey::Redo } else { ShotKey::Undo };
        }
        if matches(Keysym::y) {
            return ShotKey::Redo;
        }
        return ShotKey::None;
    }
    if typing {
        if matches(Keysym::BackSpace) {
            return ShotKey::Backspace;
        }
        return modified_sym
            .key_char()
            .filter(|c| !c.is_control())
            .map_or(ShotKey::None, |c| ShotKey::Type(c.to_string()));
    }
    if modifiers.alt || modifiers.logo {
        return ShotKey::None;
    }
    let arrows = [(Keysym::Left, -1, 0), (Keysym::Right, 1, 0), (Keysym::Up, 0, -1), (Keysym::Down, 0, 1)];
    if editing {
        if let Some((_, dx, dy)) = arrows.iter().find(|(sym, ..)| matches(*sym)) {
            return ShotKey::Nudge { dx: *dx, dy: *dy, big: modifiers.shift, grow: false };
        }
        if matches(Keysym::BackSpace) || matches(Keysym::Delete) {
            return ShotKey::Undo;
        }
        let tools = [
            (Keysym::m, Tool::Move),
            (Keysym::p, Tool::Pen),
            (Keysym::a, Tool::Arrow),
            (Keysym::r, Tool::Rect),
            (Keysym::h, Tool::Highlight),
            (Keysym::b, Tool::Blur),
            (Keysym::t, Tool::Text),
            (Keysym::n, Tool::Step),
        ];
        if let Some((_, tool)) = tools.iter().find(|(sym, _)| matches(*sym)) {
            return ShotKey::Tool(*tool);
        }
        let digits = [Keysym::_1, Keysym::_2, Keysym::_3, Keysym::_4, Keysym::_5, Keysym::_6];
        if let Some(index) = digits.iter().position(|sym| matches(*sym)) {
            return ShotKey::Swatch(index);
        }
        return ShotKey::None;
    }
    if matches(Keysym::space) {
        return ShotKey::Confirm;
    }
    if matches(Keysym::ISO_Left_Tab) || (matches(Keysym::Tab) && modifiers.shift) {
        return ShotKey::CycleMode(-1);
    }
    if matches(Keysym::Tab) {
        return ShotKey::CycleMode(1);
    }
    let modes = [(Keysym::a, Mode::Area), (Keysym::w, Mode::Window), (Keysym::s, Mode::Screen)];
    if let Some((_, mode)) = modes.iter().find(|(sym, _)| matches(*sym)) {
        return ShotKey::Mode(*mode);
    }
    if matches(Keysym::d) {
        return ShotKey::ToggleShadow;
    }
    ShotKey::None
}

fn refuse(waiter: &UnixStream, message: &str) {
    crate::ipc::reply(waiter, &glide_ipc::Response::Error { message: message.to_string() });
}

fn frame_of(window: &GlideWindow, location: Point<i32, Logical>) -> Rectangle<i32, Logical> {
    let geometry = window.geometry();
    let header = window.header_height();
    Rectangle::new(
        location + geometry.loc - Point::from((0, header)),
        Size::from((geometry.size.w, geometry.size.h + header)),
    )
}

#[must_use]
pub fn crop_region(target: Area, scale: f64, size: (u32, u32)) -> Option<(i32, i32, i32, i32)> {
    crate::screen_picker::physical_crop(target, scale, (size.0 as i32, size.1 as i32))
}

impl GlideState {
    #[must_use]
    pub fn shot_active(&self) -> bool {
        self.session.shot.is_some()
    }

    #[must_use]
    pub fn shot_typing(&self) -> bool {
        self.session.shot.as_ref().and_then(|session| session.shot.as_ref()).is_some_and(|shot| shot.typing)
    }

    #[must_use]
    pub fn shot_editing(&self) -> bool {
        self.session.shot.as_ref().and_then(|session| session.shot.as_ref()).is_some_and(Shot::editing)
    }

    fn capture_radius(&self, window: &GlideWindow) -> (f32, f32) {
        if self.is_size_locked(window) {
            return (0.0, 0.0);
        }
        match window.try_decoration_state() {
            Some(state) if state.header.is_framed() && !state.header.maximized => {
                (state.header.theme.corner_radius, state.header.theme.corner_radius)
            }
            Some(state) if state.header.maximized => (0.0, 0.0),
            _ => (0.0, glide_theme::shape::R3),
        }
    }

    fn pointer_output(&self) -> Option<(Output, Rectangle<i32, Logical>)> {
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let outputs: Vec<_> = self
            .space
            .outputs()
            .filter_map(|output| Some((output.clone(), self.space.output_geometry(output)?)))
            .collect();
        let output = crate::screenshot_ui::target_output(&outputs, pointer)?;
        outputs.into_iter().find(|(candidate, _)| *candidate == output)
    }

    pub fn open_shot(&mut self, waiter: Option<UnixStream>) {
        let blocked =
            self.screen_locked() || self.greeter.is_some() || self.picker_active() || self.handoff_blocks_input();
        if blocked {
            if let Some(waiter) = waiter {
                refuse(&waiter, "the screen cannot be captured right now");
            }
            return;
        }
        if let Some(session) = self.session.shot.as_mut() {
            session.waiters.extend(waiter);
            return;
        }
        let Some((output, geometry)) = self.pointer_output() else {
            if let Some(waiter) = waiter {
                refuse(&waiter, "no output to capture");
            }
            return;
        };
        let windows: Vec<Candidate> = self
            .space
            .elements()
            .rev()
            .filter_map(|window| {
                let frame = frame_of(window, self.space.element_location(window)?);
                frame.overlaps(geometry).then_some((window.clone(), frame))
            })
            .map(|(window, frame)| {
                let (clip, radius) = self.capture_radius(&window);
                let local = Rectangle::new(frame.loc - geometry.loc, frame.size).to_f64();
                Candidate { window, frame: local, clip, radius }
            })
            .collect();
        tracing::info!(output = %output.name(), windows = windows.len(), "screenshot tool opened");
        self.session.shot = Some(Session {
            scale: output.current_scale().fractional_scale(),
            output: output.clone(),
            origin: geometry.loc,
            bounds: geometry.size,
            shot: None,
            frozen: None,
            windows,
            window_image: None,
            window_generation: 0,
            waiters: waiter.into_iter().collect(),
            cache: render::Cache::default(),
        });
        self.pending_screenshot = Some(output);
        self.queue_redraw();
    }

    pub fn tick_shot(&mut self) -> bool {
        let Some(session) = self.session.shot.as_ref() else {
            return false;
        };
        if self.space.output_geometry(&session.output).is_none() || self.screen_locked() {
            tracing::info!("the screenshot tool lost its output or the screen locked");
            self.cancel_shot();
            return false;
        }
        session.cache.is_animating()
    }

    pub fn open_shot_after_menus(&mut self) {
        let settle = glide_theme::motion::t2() + std::time::Duration::from_millis(40);
        let timer = smithay::reexports::calloop::timer::Timer::from_duration(settle);
        let inserted = self.loop_handle.insert_source(timer, |_, (), state| {
            state.open_shot(None);
            smithay::reexports::calloop::timer::TimeoutAction::Drop
        });
        if let Err(err) = inserted {
            tracing::warn!(?err, "could not wait for the menus to close");
            self.open_shot(None);
        }
    }

    pub fn shot_frozen(&mut self, output: &Output, pixels: Vec<u8>, size: (i32, i32), scale: f64) {
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let memory = self.session.shot_memory;
        let Some(session) = self.session.shot.as_mut().filter(|session| session.awaiting_freeze(output)) else {
            return;
        };
        let Some(image) = Image::from_bgra(size.0.max(0) as u32, size.1.max(0) as u32, pixels) else {
            tracing::error!(?size, "the frozen frame was unusable");
            self.cancel_shot();
            return;
        };
        session.scale = scale;
        session.frozen = Some(Arc::new(image));
        let bounds = session.bounds.to_f64();
        let mut shot = Shot::new(bounds, session.windows.iter().map(|candidate| candidate.frame).collect(), &memory);
        let local = session.local(pointer);
        shot.motion(local, None);
        session.shot = Some(shot);
        session.cache.reveal();
        self.update_shot_cursor(pointer);
        self.queue_redraw();
    }

    fn update_shot_cursor(&mut self, pos: Point<f64, Logical>) {
        let Some(session) = self.session.shot.as_ref() else {
            return;
        };
        let Some(shot) = session.shot.as_ref() else {
            return;
        };
        let local = session.local(pos);
        let control = shot.control_at(local);
        self.shell_cursor = Some(render::cursor_for(shot, control, local));
    }

    pub fn shot_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(session) = self.session.shot.as_mut() else {
            return false;
        };
        let local = session.local(pos);
        if let Some(shot) = session.shot.as_mut() {
            let control = shot.control_at(local);
            let moved = shot.motion(local, control);
            let hover = session.cache.set_hover(if shot.bar_visible() { control } else { None });
            if moved || hover || shot.drawing() || shot.selecting() {
                self.queue_redraw();
            }
        }
        self.update_shot_cursor(pos);
        true
    }

    pub fn shot_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        let Some(session) = self.session.shot.as_mut() else {
            return false;
        };
        let local = session.local(pos);
        let Some(shot) = session.shot.as_mut() else {
            return true;
        };
        if button != LEFT {
            return true;
        }
        let control = shot.control_at(local);
        let outcome = shot.press(local, control);
        self.shot_outcome(outcome);
        self.update_shot_cursor(pos);
        true
    }

    pub fn shot_release(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(session) = self.session.shot.as_mut() else {
            return false;
        };
        let local = session.local(pos);
        if let Some(shot) = session.shot.as_mut() {
            let outcome = shot.release(local);
            self.shot_outcome(outcome);
        }
        self.update_shot_cursor(pos);
        true
    }

    pub fn shot_key(&mut self, key: ShotKey) {
        let Some(shot) = self.session.shot.as_mut().and_then(|session| session.shot.as_mut()) else {
            if key == ShotKey::Cancel {
                self.cancel_shot();
            }
            return;
        };
        let outcome = match key {
            ShotKey::Cancel => Outcome::Close,
            ShotKey::Confirm => {
                if shot.editing() {
                    shot.finish(Outcome::Save)
                } else {
                    shot.capture()
                }
            }
            ShotKey::Copy => shot.finish(Outcome::Copy),
            ShotKey::Undo => {
                shot.undo();
                Outcome::Redraw
            }
            ShotKey::Redo => {
                shot.redo();
                Outcome::Redraw
            }
            ShotKey::Mode(mode) => {
                shot.set_mode(mode);
                Outcome::Redraw
            }
            ShotKey::CycleMode(step) => {
                let index = model::MODES.iter().position(|mode| *mode == shot.mode).unwrap_or(0) as i32;
                let next = (index + i32::from(step)).rem_euclid(model::MODES.len() as i32) as usize;
                shot.set_mode(model::MODES[next]);
                Outcome::Redraw
            }
            ShotKey::ToggleShadow => {
                shot.toggle_shadow();
                Outcome::Redraw
            }
            ShotKey::Tool(tool) => {
                shot.set_tool(tool);
                Outcome::Redraw
            }
            ShotKey::Swatch(index) => {
                shot.set_swatch(index);
                Outcome::Redraw
            }
            ShotKey::Nudge { dx, dy, big, grow } => {
                let step = if big { 10.0 } else { 1.0 };
                shot.nudge(f64::from(dx) * step, f64::from(dy) * step, grow);
                Outcome::Redraw
            }
            ShotKey::Type(text) => {
                shot.type_text(&text);
                Outcome::Redraw
            }
            ShotKey::Backspace => {
                shot.backspace();
                Outcome::Redraw
            }
            ShotKey::CommitText => {
                shot.commit_text();
                Outcome::Redraw
            }
            ShotKey::None => Outcome::Nothing,
        };
        self.shot_outcome(outcome);
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        self.update_shot_cursor(pointer);
    }

    fn shot_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Redraw => self.queue_redraw(),
            Outcome::Close => self.cancel_shot(),
            Outcome::Copy => self.finish_shot(true),
            Outcome::Save => self.finish_shot(false),
            Outcome::Window(index) => {
                if let Some(session) = self.session.shot.as_ref()
                    && let Some(candidate) = session.windows.get(index)
                {
                    self.session.window_capture = Some(WindowCapture {
                        window: candidate.window.clone(),
                        output: session.output.clone(),
                        clip: candidate.clip,
                        radius: candidate.radius,
                        direct: false,
                    });
                }
                self.queue_redraw();
            }
        }
        self.refresh_shot_sprites();
    }

    fn refresh_shot_sprites(&mut self) {
        let Some(session) = self.session.shot.as_mut() else {
            return;
        };
        let scale = session.scale;
        let Some(shot) = session.shot.as_mut() else {
            return;
        };
        let painter = crate::paint::current();
        let mut drew = false;
        for mark in shot.marks.iter_mut().filter(|mark| mark.shape.needs_sprite() && mark.sprite.is_none()) {
            mark.sprite = render::sprite_for(mark, &painter, scale);
            drew |= mark.sprite.is_some();
        }
        if let Some(draft) = shot.draft.as_mut().filter(|draft| draft.shape.needs_sprite() && draft.sprite.is_none()) {
            draft.sprite = render::sprite_for(draft, &painter, scale);
        }
        if drew {
            shot.revision += 1;
        }
    }

    pub fn cancel_shot(&mut self) {
        if let Some(session) = self.session.shot.take() {
            if let Some(shot) = session.shot.as_ref() {
                self.session.shot_memory = shot.memory();
            }
            for waiter in &session.waiters {
                refuse(waiter, "cancelled");
            }
            if self.pending_screenshot.as_ref() == Some(&session.output) {
                self.pending_screenshot = None;
            }
        }
        self.session.window_capture.take_if(|capture| !capture.direct);
        self.shell_cursor = None;
        self.queue_redraw();
    }

    fn finish_shot(&mut self, copy: bool) {
        let Some(session) = self.session.shot.take() else {
            return;
        };
        self.session.window_capture.take_if(|capture| !capture.direct);
        self.shell_cursor = None;
        self.queue_redraw();
        let (Some(shot), Some(frozen)) = (session.shot.as_ref(), session.frozen.as_ref()) else {
            return;
        };
        self.session.shot_memory = shot.memory();
        let Some(target) = shot.target() else {
            return;
        };
        let scale = session.scale;
        let chosen = shot.window.and_then(|index| session.windows.get(index));
        let radius = match shot.mode {
            Mode::Window => chosen.map_or(0.0, |candidate| candidate.radius),
            _ => 0.0,
        };
        let job = Job {
            frozen: Arc::clone(frozen),
            window: session.window_image.clone().filter(|_| shot.mode == Mode::Window),
            target,
            scale,
            marks: shot.marks.clone(),
            shadow: shot.shadow_applies().then_some(Shadow { radius: radius * scale as f32, scale }),
            round: if shot.mode == Mode::Window {
                chosen.map_or(0.0, |candidate| candidate.clip) * scale as f32
            } else {
                0.0
            },
            write: !copy || !session.waiters.is_empty(),
        };
        self.session.screenshot_waiters.extend(session.waiters);
        tracing::info!(mode = ?shot.mode, marks = shot.marks.len(), copy, "screenshot taken");
        crate::screenshot_ui::run_in_background(&self.loop_handle, move || job.run());
    }

    pub fn screenshot_window_now(&mut self) {
        if self.screen_locked() || self.session.shot.is_some() {
            return;
        }
        let window = self
            .focus_history
            .iter()
            .filter_map(|id| self.window_map.get(id))
            .find(|window| self.space.element_location(window).is_some())
            .cloned();
        let Some(window) = window else {
            self.request_screenshot();
            return;
        };
        let Some(location) = self.space.element_location(&window) else {
            return;
        };
        let frame = frame_of(&window, location);
        let centre = frame.loc.to_f64() + Point::from((f64::from(frame.size.w) / 2.0, f64::from(frame.size.h) / 2.0));
        let outputs: Vec<_> = self
            .space
            .outputs()
            .filter_map(|output| Some((output.clone(), self.space.output_geometry(output)?)))
            .collect();
        let Some(output) = crate::screenshot_ui::target_output(&outputs, centre) else {
            return;
        };
        let (clip, radius) = self.capture_radius(&window);
        self.session.window_capture = Some(WindowCapture { window, output, clip, radius, direct: true });
        self.queue_redraw();
    }

    pub fn window_captured(&mut self, capture: &WindowCapture, pixels: Vec<u8>, size: (i32, i32), scale: f64) {
        let Some(image) = Image::from_bgra(size.0.max(0) as u32, size.1.max(0) as u32, pixels) else {
            tracing::warn!(?size, "the window capture was empty");
            return;
        };
        if capture.direct {
            let shadow =
                self.session.shot_memory.shadow.then_some(Shadow { radius: capture.radius * scale as f32, scale });
            let target =
                Rectangle::from_size(Size::from((f64::from(image.width) / scale, f64::from(image.height) / scale)));
            let job = Job {
                frozen: Arc::new(image.clone()),
                window: Some(Arc::new(image)),
                target,
                scale,
                marks: Vec::new(),
                shadow,
                round: capture.clip * scale as f32,
                write: true,
            };
            crate::screenshot_ui::run_in_background(&self.loop_handle, move || job.run());
            return;
        }
        let Some(session) = self.session.shot.as_mut().filter(|session| session.output == capture.output) else {
            return;
        };
        session.window_image = Some(Arc::new(image));
        session.window_generation += 1;
        session.cache.drop_window();
        self.queue_redraw();
    }
}

struct Job {
    frozen: Arc<Image>,
    window: Option<Arc<Image>>,
    target: Area,
    scale: f64,
    marks: Vec<model::Mark>,
    shadow: Option<Shadow>,
    round: f32,
    write: bool,
}

impl Job {
    fn compose(&self) -> Option<Image> {
        let (mut image, frame) = if let Some(window) = self.window.as_deref() {
            let frame = Frame {
                x: (self.target.loc.x * self.scale).round(),
                y: (self.target.loc.y * self.scale).round(),
                scale: self.scale,
            };
            (window.clone(), frame)
        } else {
            let region = crop_region(self.target, self.scale, (self.frozen.width, self.frozen.height))?;
            let frame = Frame { x: f64::from(region.0), y: f64::from(region.1), scale: self.scale };
            let cropped = self.frozen.crop(region)?;
            (compose::rounded_corners(cropped, self.round), frame)
        };
        compose::apply_marks(&mut image, &self.marks, &frame);
        if self.window.is_some() && self.round > 0.0 {
            image = compose::rounded_corners(image, self.round);
        }
        match self.shadow {
            Some(shadow) => compose::with_shadow(&image, shadow),
            None => Some(image),
        }
    }

    fn run(self) -> Result<crate::backend::screenshot::Saved, String> {
        let image = self.compose().ok_or_else(|| "nothing to capture".to_string())?;
        let (width, height) = (image.width, image.height);
        crate::backend::screenshot::save_rgba(&image.into_straight(), width, height, self.write)
            .map_err(|err| format!("{err:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> ModifiersState {
        ModifiersState::default()
    }

    #[test]
    fn keys_choose_a_mode_before_and_a_tool_after_the_capture() {
        assert_eq!(shortcut(&none(), &[Keysym::w], Keysym::w, false, false), ShotKey::Mode(Mode::Window));
        assert_eq!(shortcut(&none(), &[Keysym::a], Keysym::a, false, true), ShotKey::Tool(Tool::Arrow));
        assert_eq!(shortcut(&none(), &[Keysym::_3], Keysym::_3, false, true), ShotKey::Swatch(2));
        assert_eq!(shortcut(&none(), &[Keysym::Escape], Keysym::Escape, false, true), ShotKey::Cancel);
        assert_eq!(shortcut(&none(), &[Keysym::Return], Keysym::Return, false, false), ShotKey::Confirm);
    }

    #[test]
    fn typing_takes_every_printable_key_and_escape_only_ends_the_text() {
        let shift = ModifiersState { shift: true, ..none() };
        assert_eq!(shortcut(&shift, &[Keysym::a], Keysym::A, true, true), ShotKey::Type("A".into()));
        assert_eq!(
            shortcut(&none(), &[Keysym::Cyrillic_ze], Keysym::Cyrillic_ze, true, true),
            ShotKey::Type("з".into())
        );
        assert_eq!(shortcut(&none(), &[Keysym::Escape], Keysym::Escape, true, true), ShotKey::CommitText);
        assert_eq!(shortcut(&none(), &[Keysym::BackSpace], Keysym::BackSpace, true, true), ShotKey::Backspace);
        let ctrl = ModifiersState { ctrl: true, ..none() };
        assert_eq!(shortcut(&ctrl, &[Keysym::z], Keysym::z, true, true), ShotKey::Undo);
    }

    #[test]
    fn shortcuts_follow_the_latin_key_on_other_layouts() {
        let ctrl = ModifiersState { ctrl: true, ..none() };
        assert_eq!(shortcut(&ctrl, &[Keysym::Cyrillic_es, Keysym::c], Keysym::Cyrillic_es, false, true), ShotKey::Copy);
        assert_eq!(
            shortcut(&none(), &[Keysym::Cyrillic_ha, Keysym::h], Keysym::Cyrillic_ha, false, true),
            ShotKey::Tool(Tool::Highlight)
        );
    }

    #[test]
    fn a_window_job_keeps_the_window_shape_and_adds_the_shadow_margin() {
        let mut window = Image::new(40, 30).expect("image");
        for pixel in window.data.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[10, 20, 30, 255]);
        }
        let job = Job {
            frozen: Arc::new(window.clone()),
            window: Some(Arc::new(window)),
            target: Rectangle::new(Point::from((100.0, 100.0)), Size::from((40.0, 30.0))),
            scale: 1.0,
            marks: Vec::new(),
            shadow: Some(Shadow { radius: 8.0, scale: 1.0 }),
            round: 8.0,
            write: false,
        };
        let image = job.compose().expect("image");
        let (margin, top) = compose::shadow_margins(1.0);
        assert_eq!((image.width, image.height), (40 + margin * 2, 30 + margin + top));
        let corner = ((top * image.width + margin) * 4 + 3) as usize;
        assert!(image.data[corner] < 255, "the corner is rounded off");
    }

    #[test]
    fn an_area_job_crops_the_frozen_frame_at_the_output_scale() {
        let mut frozen = Image::new(200, 100).expect("image");
        let index = ((40 * 200 + 60) * 4) as usize;
        frozen.data[index..index + 4].copy_from_slice(&[255, 0, 0, 255]);
        let job = Job {
            frozen: Arc::new(frozen),
            window: None,
            target: Rectangle::new(Point::from((30.0, 20.0)), Size::from((20.0, 10.0))),
            scale: 2.0,
            marks: Vec::new(),
            shadow: None,
            round: 0.0,
            write: false,
        };
        let image = job.compose().expect("image");
        assert_eq!((image.width, image.height), (40, 20));
        assert_eq!(&image.data[..4], &[255, 0, 0, 255]);
    }
}
