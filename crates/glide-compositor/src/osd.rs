use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::element::memory::MemoryRenderBuffer,
    utils::{Logical, Point, Rectangle, Size},
};

use crate::raster::{Canvas, rect};

const WIDTH: i32 = 280;
const HEIGHT: i32 = 52;
const BOTTOM: i32 = 160;
const ICON: i32 = 22;
const BAR_HEIGHT: i32 = 6;
const HOLD: Duration = Duration::from_millis(1500);
const EXPECT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Volume,
    Brightness,
    CapsLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub kind: Kind,
    pub level: u8,
    pub on: bool,
}

impl Reading {
    fn icon(self) -> &'static str {
        match (self.kind, self.on) {
            (Kind::Volume, false) => "audio-volume-muted-symbolic",
            (Kind::Volume, true) if self.level == 0 => "audio-volume-muted-symbolic",
            (Kind::Volume, true) if self.level < 34 => "audio-volume-low-symbolic",
            (Kind::Volume, true) if self.level < 67 => "audio-volume-medium-symbolic",
            (Kind::Volume, true) => "audio-volume-high-symbolic",
            (Kind::Brightness, _) => "display-brightness-symbolic",
            (Kind::CapsLock, _) => "input-keyboard-symbolic",
        }
    }
}

#[must_use]
pub fn kind_for_key(
    raw_syms: &[smithay::input::keyboard::Keysym], modified: smithay::input::keyboard::Keysym,
) -> Option<Kind> {
    use smithay::input::keyboard::Keysym;
    let pressed = |wanted: &[Keysym]| wanted.iter().any(|key| *key == modified || raw_syms.contains(key));
    if pressed(&[Keysym::XF86_AudioRaiseVolume, Keysym::XF86_AudioLowerVolume, Keysym::XF86_AudioMute]) {
        Some(Kind::Volume)
    } else if pressed(&[Keysym::XF86_MonBrightnessUp, Keysym::XF86_MonBrightnessDown]) {
        Some(Kind::Brightness)
    } else if pressed(&[Keysym::Caps_Lock]) {
        Some(Kind::CapsLock)
    } else {
        None
    }
}

#[derive(Debug, Default)]
pub struct Osd {
    shown: Option<(Reading, Instant)>,
    expected: Option<(Kind, Instant)>,
    cache: std::collections::HashMap<i32, (u64, MemoryRenderBuffer)>,
}

impl Osd {
    pub fn expect(&mut self, kind: Kind, now: Instant) {
        self.expected = Some((kind, now));
    }

    pub fn show(&mut self, reading: Reading, now: Instant) {
        self.shown = Some((reading, now));
    }

    pub fn status(&mut self, volume: (u8, bool), brightness: u8, now: Instant) -> bool {
        let Some((kind, at)) = self.expected else {
            return false;
        };
        if now.saturating_duration_since(at) > EXPECT {
            self.expected = None;
            return false;
        }
        let reading = match kind {
            Kind::Volume => Reading { kind, level: volume.0, on: !volume.1 },
            Kind::Brightness => Reading { kind, level: brightness, on: true },
            Kind::CapsLock => return false,
        };
        if self.shown.is_some_and(|(shown, _)| shown == reading) && self.alpha(now) > 0.0 {
            self.shown = Some((reading, now));
            return false;
        }
        self.show(reading, now);
        true
    }

    #[must_use]
    pub fn alpha(&self, now: Instant) -> f32 {
        let Some((_, at)) = self.shown else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(at);
        let (rise, fall) = (glide_theme::motion::t1(), glide_theme::motion::t2());
        if elapsed < rise {
            return glide_theme::motion::ease_out(elapsed.as_secs_f64() / rise.as_secs_f64()) as f32;
        }
        let leaving = elapsed.saturating_sub(rise + HOLD);
        if fall.is_zero() || leaving >= fall {
            return if leaving.is_zero() { 1.0 } else { 0.0 };
        }
        1.0 - glide_theme::motion::ease_out(leaving.as_secs_f64() / fall.as_secs_f64()) as f32
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if self.shown.is_some() && self.alpha(now) <= 0.0 {
            let elapsed = self.shown.map_or(Duration::ZERO, |(_, at)| now.saturating_duration_since(at));
            if elapsed > glide_theme::motion::t1() {
                self.shown = None;
                return true;
            }
        }
        self.shown.is_some()
    }

    #[must_use]
    pub fn rect(output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
        Rectangle::new(
            Point::from((output.loc.x + (output.size.w - WIDTH) / 2, output.loc.y + output.size.h - BOTTOM)),
            Size::from((WIDTH, HEIGHT)),
        )
    }

    pub fn buffer(
        &mut self, painter: &crate::paint::Painter, icons: &mut crate::icon::IconLoader, tokens: &glide_theme::Tokens,
        scale: i32,
    ) -> Option<&MemoryRenderBuffer> {
        let (reading, _) = self.shown?;
        let key = glide_theme::Revision::new()
            .str(&format!("{:?}{:?}{reading:?}", tokens.appearance, tokens.accent))
            .u64(crate::paint::painter_generation())
            .finish();
        if self.cache.get(&scale).is_none_or(|(cached, _)| *cached != key) {
            let mut canvas =
                Canvas::popover((WIDTH, HEIGHT), scale, glide_theme::shape::R4, tokens.popover_bg, tokens.line)?
                    .with_icons(Some(icons));
            let pad = glide_theme::spacing::SP4;
            canvas.icon(reading.icon(), ICON, (pad, (HEIGHT - ICON) / 2), tokens.fg_1);
            let left = pad + ICON + glide_theme::spacing::SP3;
            let width = WIDTH - left - pad;
            if reading.kind == Kind::CapsLock {
                let text = crate::i18n::t(if reading.on {
                    crate::i18n::Key::CapsLockOn
                } else {
                    crate::i18n::Key::CapsLockOff
                });
                let style = glide_theme::TextStyle::BODY;
                canvas.text(
                    painter,
                    (left, left + width, 0, HEIGHT),
                    crate::paint::TextAlign::Left,
                    text,
                    tokens.fg_1,
                    &style,
                );
            } else {
                let track = rect(left, (HEIGHT - BAR_HEIGHT) / 2, width, BAR_HEIGHT);
                canvas.fill(track, tokens.fill_2, glide_theme::shape::R1);
                let filled = if reading.on { width * i32::from(reading.level.min(100)) / 100 } else { 0 };
                if filled > 0 {
                    canvas.fill(rect(left, track.loc.y, filled, BAR_HEIGHT), tokens.acc_bg, glide_theme::shape::R1);
                }
            }
            self.cache.insert(scale, (key, canvas.finish()));
        }
        self.cache.get(&scale).map(|(_, buffer)| buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_change_shows_only_after_a_media_key() {
        let mut osd = Osd::default();
        let now = Instant::now();
        assert!(!osd.status((40, false), 80, now), "a slider drag or another app does not pop the OSD");
        osd.expect(Kind::Volume, now);
        assert!(osd.status((45, false), 80, now + Duration::from_millis(150)));
        assert!(!osd.status((45, false), 80, now + Duration::from_millis(300)), "the same reading only extends it");
        osd.expect(Kind::Brightness, now);
        assert!(!osd.status((45, false), 60, now + EXPECT + Duration::from_millis(1)), "a stale expectation lapses");
    }

    #[test]
    fn it_fades_in_holds_and_fades_out() {
        let mut osd = Osd::default();
        let now = Instant::now();
        osd.show(Reading { kind: Kind::Brightness, level: 50, on: true }, now);
        assert!(osd.alpha(now + glide_theme::motion::t1() + HOLD / 2) >= 0.99);
        let gone = now + glide_theme::motion::t1() + HOLD + glide_theme::motion::t2() + Duration::from_millis(10);
        assert!(osd.alpha(gone) <= 0.0);
        assert!(osd.tick(gone), "one last frame to clear it");
        assert!(!osd.tick(gone));
    }

    #[test]
    fn the_volume_icon_follows_the_level_and_mute() {
        let volume = |level, on| Reading { kind: Kind::Volume, level, on }.icon();
        assert_eq!(volume(80, false), "audio-volume-muted-symbolic");
        assert_eq!(volume(0, true), "audio-volume-muted-symbolic");
        assert_eq!(volume(20, true), "audio-volume-low-symbolic");
        assert_eq!(volume(50, true), "audio-volume-medium-symbolic");
        assert_eq!(volume(90, true), "audio-volume-high-symbolic");
    }

    #[test]
    fn media_and_caps_lock_keys_pick_their_indicator() {
        use smithay::input::keyboard::Keysym;
        assert_eq!(kind_for_key(&[], Keysym::XF86_AudioMute), Some(Kind::Volume));
        assert_eq!(kind_for_key(&[Keysym::XF86_MonBrightnessUp], Keysym::NoSymbol), Some(Kind::Brightness));
        assert_eq!(kind_for_key(&[Keysym::Caps_Lock], Keysym::Caps_Lock), Some(Kind::CapsLock));
        assert_eq!(kind_for_key(&[Keysym::a], Keysym::a), None);
    }

    #[test]
    fn it_sits_centred_above_the_dock() {
        let area = Osd::rect(Rectangle::new(Point::from((1920, 0)), Size::from((2560, 1440))));
        assert_eq!(area.loc.x * 2 + area.size.w, 1920 * 2 + 2560);
        assert_eq!(area.loc.y, 1440 - BOTTOM);
    }
}
