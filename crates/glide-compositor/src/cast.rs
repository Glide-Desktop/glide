use std::sync::Arc;
use std::time::{Duration, Instant};

use glide_cast::cast::{Discovery, Handle, Source, Status};
use glide_cast::p2p::Peer;
use smithay::output::Output;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};

use crate::i18n::{Key, t};
use crate::state::GlideState;

pub const NOTICE_ID: u32 = u32::MAX - 11;
pub const STOP: &str = "stop";
const PICK_PREFIX: &str = "cast:";
const SEARCH_SECONDS: u32 = 30;
const MAX_CHOICES: usize = 3;
const FRAME_FORMAT: &str = "BGRx";

#[derive(Debug)]
pub enum Event {
    Found(Discovery),
    Status(Status),
}

pub type Send = Arc<dyn Fn(Event) + std::marker::Send + Sync>;

struct Active {
    handle: Handle,
    output: Output,
    name: String,
    interval: Option<Duration>,
    last_frame: Option<Instant>,
}

#[derive(Default)]
pub struct Cast {
    send: Option<Send>,
    searching: bool,
    displays: Vec<Peer>,
    active: Option<Active>,
}

impl Cast {
    #[must_use]
    pub fn new(send: Send) -> Self {
        Self { send: Some(send), ..Self::default() }
    }

    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.active.as_ref().filter(|active| active.interval.is_some()).map(|active| active.name.clone())
    }

    #[must_use]
    pub fn frame_due(&self, output: &Output) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.output == *output && frame_due(active.interval, active.last_frame, Instant::now())
        })
    }

    pub fn push_frame(&mut self, pixels: Vec<u8>) {
        if let Some(active) = self.active.as_mut() {
            active.last_frame = Some(Instant::now());
            active.handle.push_frame(pixels);
        }
    }
}

#[must_use]
pub fn pick_key(index: usize) -> String {
    format!("{PICK_PREFIX}{index}")
}

#[must_use]
pub fn parse_pick_key(key: &str) -> Option<usize> {
    key.strip_prefix(PICK_PREFIX)?.parse().ok()
}

#[must_use]
pub fn frame_due(interval: Option<Duration>, last: Option<Instant>, now: Instant) -> bool {
    interval.is_some_and(|interval| last.is_none_or(|last| now.saturating_duration_since(last) >= interval * 4 / 5))
}

impl GlideState {
    pub fn toggle_screen_mirroring(&mut self) {
        if let Some(active) = self.session.cast.active.as_ref() {
            active.handle.stop();
            return;
        }
        if self.session.cast.searching {
            return;
        }
        let Some(send) = self.session.cast.send.clone() else {
            return;
        };
        self.session.cast.displays.clear();
        self.session.cast.searching = true;
        let started = glide_cast::cast::discover(SEARCH_SECONDS, move |found| send(Event::Found(found)));
        if let Err(err) = started {
            self.session.cast.searching = false;
            self.cast_notice(t(Key::CastUnavailable).replace("{reason}", &err.to_string()), String::new(), vec![]);
            return;
        }
        self.cast_notice(t(Key::CastSearching).to_string(), t(Key::CastSearchingBody).to_string(), vec![]);
    }

    pub fn handle_cast(&mut self, event: Event) {
        match event {
            Event::Found(Discovery::Peers(peers)) if self.session.cast.active.is_none() => {
                let displays: Vec<Peer> = peers.into_iter().filter(Peer::is_display).collect();
                if displays.is_empty() || displays == self.session.cast.displays {
                    return;
                }
                let actions = displays
                    .iter()
                    .take(MAX_CHOICES)
                    .enumerate()
                    .map(|(index, peer)| glide_notify::Action { key: pick_key(index), label: peer.name.clone() })
                    .collect();
                self.session.cast.displays = displays;
                self.cast_notice(t(Key::CastPick).to_string(), t(Key::CastPickBody).to_string(), actions);
            }
            Event::Found(Discovery::Peers(_)) => {}
            Event::Found(Discovery::Finished) => {
                self.session.cast.searching = false;
                if self.session.cast.displays.is_empty() && self.session.cast.active.is_none() {
                    self.cast_notice(t(Key::CastNone).to_string(), t(Key::CastNoneBody).to_string(), vec![]);
                }
            }
            Event::Found(Discovery::Failed(reason)) => {
                self.session.cast.searching = false;
                self.cast_notice(t(Key::CastUnavailable).replace("{reason}", &reason), String::new(), vec![]);
            }
            Event::Status(status) => self.cast_status(status),
        }
    }

    fn cast_status(&mut self, status: Status) {
        let name = self.session.cast.active.as_ref().map(|active| active.name.clone()).unwrap_or_default();
        let stop = || vec![glide_notify::Action { key: STOP.into(), label: t(Key::CastStop).into() }];
        match status {
            Status::Connecting(display) => {
                self.cast_notice(t(Key::CastConnecting).replace("{name}", &display), String::new(), stop());
            }
            Status::Negotiated(mode) => {
                tracing::info!(width = mode.width, height = mode.height, fps = mode.fps, "mirroring mode agreed");
            }
            Status::Streaming(mode) => {
                let interval = Duration::from_secs(1) / mode.fps.max(1);
                if let Some(active) = self.session.cast.active.as_mut() {
                    active.interval = Some(interval);
                }
                self.arm_cast_frames(interval);
                let body = format!("{}×{}, {} fps", mode.width, mode.height, mode.fps);
                self.cast_notice(t(Key::CastStreaming).replace("{name}", &name), body, stop());
                self.sync_screen_capture();
            }
            Status::Ended(reason) => {
                self.session.cast.active = None;
                self.sync_screen_capture();
                let summary = match &reason {
                    Some(reason) => t(Key::CastFailed).replace("{reason}", reason),
                    None => t(Key::CastEnded).to_string(),
                };
                tracing::info!(?reason, "mirroring ended");
                self.cast_notice(summary, String::new(), vec![]);
            }
        }
        self.queue_redraw();
    }

    pub fn cast_action(&mut self, key: &str) {
        if key == STOP {
            if let Some(active) = self.session.cast.active.as_ref() {
                active.handle.stop();
            }
            return;
        }
        let Some(peer) = parse_pick_key(key).and_then(|index| self.session.cast.displays.get(index).cloned()) else {
            return;
        };
        self.start_cast(peer);
    }

    fn start_cast(&mut self, peer: Peer) {
        let (Some(send), None) = (self.session.cast.send.clone(), self.session.cast.active.as_ref()) else {
            return;
        };
        let pointer = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let outputs: Vec<_> = self
            .space
            .outputs()
            .filter_map(|output| Some((output.clone(), self.space.output_geometry(output)?)))
            .collect();
        let Some(output) = crate::screenshot_ui::target_output(&outputs, pointer) else {
            return;
        };
        let Some(mode) = output.current_mode() else {
            return;
        };
        let size = (u32::try_from(mode.size.w).unwrap_or(0), u32::try_from(mode.size.h).unwrap_or(0));
        let source = Source { frame_format: FRAME_FORMAT, size };
        let name = peer.name.clone();
        match glide_cast::cast::start(peer, source, move |status| send(Event::Status(status))) {
            Ok(handle) => {
                tracing::info!(display = name, output = output.name(), "mirroring started");
                self.session.cast.active = Some(Active { handle, output, name, interval: None, last_frame: None });
            }
            Err(err) => {
                self.cast_notice(t(Key::CastUnavailable).replace("{reason}", &err.to_string()), String::new(), vec![]);
            }
        }
    }

    fn arm_cast_frames(&mut self, interval: Duration) {
        let timer = Timer::from_duration(interval);
        let armed = self.loop_handle.insert_source(timer, move |_, (), state| {
            let streaming = state.session.cast.active.as_ref().filter(|active| active.interval.is_some());
            let Some(output) = streaming.map(|active| active.output.clone()) else {
                return TimeoutAction::Drop;
            };
            state.queue_redraw_output(&output);
            TimeoutAction::ToDuration(interval)
        });
        if let Err(err) = armed {
            tracing::warn!(?err, "could not pace the mirrored frames; the display updates only on screen changes");
        }
    }

    fn cast_notice(&mut self, summary: String, body: String, actions: Vec<glide_notify::Action>) {
        let notice = glide_notify::Notification {
            id: NOTICE_ID,
            app_name: "Glide".to_string(),
            app_icon: "video-display".to_string(),
            summary,
            body,
            resident: !actions.is_empty(),
            actions,
            transient: true,
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_key_names_the_display_it_starts() {
        assert_eq!(parse_pick_key(&pick_key(2)), Some(2));
        assert_eq!(parse_pick_key(STOP), None);
    }

    #[test]
    fn frames_are_taken_at_the_agreed_rate_only_while_streaming() {
        let now = Instant::now();
        let interval = Some(Duration::from_millis(33));
        assert!(!frame_due(None, None, now), "nothing is captured before PLAY");
        assert!(frame_due(interval, None, now));
        assert!(!frame_due(interval, Some(now), now + Duration::from_millis(10)));
        assert!(frame_due(interval, Some(now), now + Duration::from_millis(40)));
    }

    #[test]
    fn a_timer_tick_that_lands_a_little_early_still_takes_its_frame() {
        let interval = Duration::from_millis(33);
        let start = Instant::now();
        let mut last = None;
        for tick in 1..=3 {
            let early = interval.saturating_sub(Duration::from_millis(2));
            let now = start + interval * (tick - 1) + early;
            assert!(frame_due(Some(interval), last, now), "tick {tick} was skipped");
            last = Some(now);
        }
    }

    #[test]
    fn the_notice_id_is_its_own() {
        assert!(crate::notifications::is_local(NOTICE_ID));
        for other in [crate::autofill::FILL_ID, crate::otp::OFFER_ID, crate::welcome::NOTICE_ID] {
            assert_ne!(NOTICE_ID, other);
        }
    }
}
