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
    glide_theme::motion::ease_out((born.elapsed().as_secs_f64() / span.as_secs_f64()).clamp(0.0, 1.0)) as f32
}

fn born(surface: &WlSurface) -> Instant {
    with_states(surface, |states| states.data_map.get_or_insert(|| Born(Instant::now())).0)
}

#[must_use]
pub fn alpha(surface: &WlSurface) -> f32 {
    progress(born(surface))
}

#[must_use]
pub fn animating(surface: &WlSurface) -> bool {
    !span().is_zero() && born(surface).elapsed() < span()
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
}
