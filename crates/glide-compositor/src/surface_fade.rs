use std::time::{Duration, Instant};

use smithay::{reexports::wayland_server::protocol::wl_surface::WlSurface, wayland::compositor::with_states};

struct Born(Instant);

fn span() -> Duration {
    glide_theme::motion::t2()
}

fn progress(born: Instant) -> f32 {
    let span = span();
    if span.is_zero() {
        return 1.0;
    }
    glide_theme::motion::ease_out((glide_ui::clock::since(born).as_secs_f64() / span.as_secs_f64()).clamp(0.0, 1.0))
        as f32
}

fn born(surface: &WlSurface) -> Instant {
    with_states(surface, |states| states.data_map.get_or_insert(|| Born(glide_ui::clock::now())).0)
}

const POPUP_TRAVEL: f32 = 6.0;

#[must_use]
pub fn travel(surface: &WlSurface) -> f32 {
    (1.0 - progress(born(surface))) * -POPUP_TRAVEL
}

#[must_use]
pub fn alpha(surface: &WlSurface) -> f32 {
    (progress(born(surface)) / 0.4).min(1.0)
}

#[must_use]
pub fn animating(surface: &WlSurface) -> bool {
    !span().is_zero() && glide_ui::clock::since(born(surface)) < span()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_fades_in_from_nothing_and_settles_at_full() {
        let now = Instant::now();
        if glide_theme::motion::is_disabled() {
            assert!((progress(now) - 1.0).abs() < f32::EPSILON);
            return;
        }
        assert!(progress(now) < 0.5);
        let earlier = now.checked_sub(span() * 2).expect("the clock has run for longer than the fade");
        assert!((progress(earlier) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn travel_slides_six_pixels() {
        let now = Instant::now();
        if glide_theme::motion::is_disabled() {
            return;
        }
        let earlier = now.checked_sub(span() * 2).expect("valid clock");
        assert_eq!((1.0 - progress(earlier)) * -POPUP_TRAVEL, 0.0);
    }
}
