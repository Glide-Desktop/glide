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
    KeyboardBrightness,
    CapsLock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levels {
    pub volume: Option<u8>,
    pub muted: bool,
    pub brightness: Option<u8>,
    pub keyboard: Option<u8>,
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
            (Kind::KeyboardBrightness, _) => "keyboard-brightness-symbolic",
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
    } else if pressed(&[Keysym::XF86_KbdBrightnessUp, Keysym::XF86_KbdBrightnessDown]) {
        Some(Kind::KeyboardBrightness)
    } else if pressed(&[Keysym::Caps_Lock]) {
        Some(Kind::CapsLock)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct Shown {
    reading: Reading,
    since: Instant,
    updated: Instant,
}

#[derive(Debug, Default)]
pub struct Osd {
    shown: Option<Shown>,
    expected: Option<(Kind, Instant)>,
    bar: Option<glide_ui::Tween>,
    cache: std::collections::HashMap<i32, (u64, MemoryRenderBuffer)>,
}

impl Osd {
    pub fn expect(&mut self, kind: Kind, now: Instant) {
        self.expected = Some((kind, now));
    }

    pub fn show(&mut self, reading: Reading, now: Instant) {
        let live = self.shown.filter(|_| self.alpha(now) > 0.0);
        let since = live.map_or(now, |shown| shown.since);
        let target = if reading.on { f64::from(reading.level.min(100)) } else { 0.0 };
        match (&mut self.bar, live.filter(|shown| shown.reading.kind == reading.kind)) {
            (Some(bar), Some(_)) => {
                bar.retarget(target, glide_theme::motion::t2(), glide_theme::motion::ease_out);
            }
            (slot, _) => *slot = Some(glide_ui::Tween::at(target)),
        }
        self.shown = Some(Shown { reading, since, updated: now });
    }

    fn bar_level(&self, reading: Reading) -> i32 {
        let target = if reading.on { f64::from(reading.level.min(100)) } else { 0.0 };
        let level = self.bar.map_or(target, |bar| bar.value(glide_theme::motion::t2(), glide_theme::motion::ease_out));
        level.round().clamp(0.0, 100.0) as i32
    }

    pub fn status(&mut self, levels: Levels, now: Instant) -> bool {
        let Some((kind, at)) = self.expected else {
            return false;
        };
        if now.saturating_duration_since(at) > EXPECT {
            self.expected = None;
            return false;
        }
        let (level, on) = match kind {
            Kind::Volume => (levels.volume, !levels.muted),
            Kind::Brightness => (levels.brightness, true),
            Kind::KeyboardBrightness => (levels.keyboard, true),
            Kind::CapsLock => (None, true),
        };
        let Some(level) = level else {
            return false;
        };
        let reading = Reading { kind, level, on };
        let unchanged = self.shown.is_some_and(|shown| shown.reading == reading) && self.alpha(now) > 0.0;
        self.show(reading, now);
        !unchanged
    }

    #[must_use]
    pub fn alpha(&self, now: Instant) -> f32 {
        let Some(shown) = self.shown else {
            return 0.0;
        };
        let (rise, fall) = (glide_theme::motion::t1(), glide_theme::motion::t2());
        let entering = now.saturating_duration_since(shown.since);
        let fade_in = if rise.is_zero() || entering >= rise {
            1.0
        } else {
            glide_theme::motion::ease_out(entering.as_secs_f64() / rise.as_secs_f64())
        };
        let leaving = now.saturating_duration_since(shown.updated).saturating_sub(rise + HOLD);
        let fade_out = if leaving.is_zero() {
            0.0
        } else if fall.is_zero() || leaving >= fall {
            1.0
        } else {
            glide_theme::motion::ease_out(leaving.as_secs_f64() / fall.as_secs_f64())
        };
        (fade_in.min(1.0 - fade_out)) as f32
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(shown) = self.shown
            && self.alpha(now) <= 0.0
            && now.saturating_duration_since(shown.since) > glide_theme::motion::t1()
        {
            self.shown = None;
            return true;
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
        let reading = self.shown?.reading;
        let level = self.bar_level(reading);
        let key = glide_theme::Revision::new()
            .str(&format!("{:?}{:?}{reading:?}{level}", tokens.appearance, tokens.accent))
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
                let filled = width * level / 100;
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

    fn levels(volume: u8, brightness: u8) -> Levels {
        Levels { volume: Some(volume), muted: false, brightness: Some(brightness), keyboard: None }
    }

    #[test]
    fn no_indicator_without_a_real_reading() {
        let mut osd = Osd::default();
        let now = Instant::now();
        osd.expect(Kind::Brightness, now);
        assert!(!osd.status(Levels { brightness: None, ..levels(40, 80) }, now), "no backlight, no made-up 80 %");
        osd.expect(Kind::Volume, now);
        assert!(!osd.status(Levels { volume: None, ..levels(40, 80) }, now));
        assert!(osd.status(levels(40, 80), now));
    }

    #[test]
    fn a_status_change_shows_only_after_a_media_key() {
        let mut osd = Osd::default();
        let now = Instant::now();
        assert!(!osd.status(levels(40, 80), now), "a slider drag or another app does not pop the OSD");
        osd.expect(Kind::Volume, now);
        assert!(osd.status(levels(45, 80), now + Duration::from_millis(150)));
        assert!(!osd.status(levels(45, 80), now + Duration::from_millis(300)), "the same reading only extends it");
        osd.expect(Kind::Brightness, now);
        assert!(!osd.status(levels(45, 60), now + EXPECT + Duration::from_millis(1)), "a stale expectation lapses");
    }

    #[test]
    fn the_keyboard_backlight_shows_only_where_there_is_one() {
        let mut osd = Osd::default();
        let now = Instant::now();
        osd.expect(Kind::KeyboardBrightness, now);
        assert!(!osd.status(levels(40, 80), now), "no keyboard backlight, no indicator");
        let lit = Levels { keyboard: Some(66), ..levels(40, 80) };
        assert!(osd.status(lit, now));
        assert_eq!(
            Reading { kind: Kind::KeyboardBrightness, level: 66, on: true }.icon(),
            "keyboard-brightness-symbolic"
        );
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
    fn holding_a_volume_key_updates_the_bar_without_fading_it_again() {
        let mut osd = Osd::default();
        let start = Instant::now();
        let rise = glide_theme::motion::t1();
        osd.expect(Kind::Volume, start);
        assert!(osd.status(levels(40, 80), start));
        let steady = start + rise + Duration::from_millis(50);
        assert!(osd.status(levels(45, 80), steady), "a new level is drawn");
        assert!(osd.alpha(steady + Duration::from_millis(1)) >= 0.99, "no fresh fade-in on every step");
        let still = steady + HOLD;
        assert!(osd.alpha(still) >= 0.99, "the hold restarts from the last change");
    }

    #[test]
    fn the_bar_glides_to_a_new_level_while_the_indicator_stays_up() {
        let mut osd = Osd::default();
        let now = Instant::now();
        let volume = |level| Reading { kind: Kind::Volume, level, on: true };
        osd.show(volume(40), now);
        assert_eq!(osd.bar_level(volume(40)), 40, "a fresh indicator starts at its level");
        osd.show(volume(80), now + glide_theme::motion::t1() + Duration::from_millis(10));
        if !glide_theme::motion::is_disabled() {
            let drawn = osd.bar_level(volume(80));
            assert!((40..80).contains(&drawn), "the bar has not jumped to 80: {drawn}");
        }
        osd.show(Reading { kind: Kind::Brightness, level: 10, on: true }, now + glide_theme::motion::t1());
        assert_eq!(osd.bar_level(Reading { kind: Kind::Brightness, level: 10, on: true }), 10, "another kind snaps");
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
        assert_eq!(kind_for_key(&[], Keysym::XF86_KbdBrightnessDown), Some(Kind::KeyboardBrightness));
        assert_eq!(kind_for_key(&[Keysym::a], Keysym::a), None);
    }

    #[test]
    fn it_sits_centred_above_the_dock() {
        let area = Osd::rect(Rectangle::new(Point::from((1920, 0)), Size::from((2560, 1440))));
        assert_eq!(area.loc.x * 2 + area.size.w, 1920 * 2 + 2560);
        assert_eq!(area.loc.y, 1440 - BOTTOM);
    }
}
