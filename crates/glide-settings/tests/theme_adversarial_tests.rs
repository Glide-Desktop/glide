use glide_settings::theme::{ST_HOVER, ST_PRESS, blend_state_layer, hover_layer, press_layer};
use iced::Color;

fn close(a: Color, b: Color) -> bool {
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)].iter().all(|(x, y)| (x - y).abs() < 1e-5)
}

#[test]
fn state_layers_blend_source_over_exactly() {
    let base = Color::from_rgb(0.2, 0.4, 0.8);
    for opacity in [ST_HOVER, ST_PRESS] {
        let mix = |channel: f32| opacity + (1.0 - opacity) * channel;
        let expected = Color::from_rgb(mix(0.2), mix(0.4), mix(0.8));
        assert!(close(blend_state_layer(base, Color::WHITE, opacity), expected));
    }
    for base in [Color::BLACK, Color::from_rgba(0.3, 0.7, 0.2, 0.5)] {
        assert!(close(blend_state_layer(base, Color::WHITE, 0.0), base));
        assert!(close(blend_state_layer(base, Color::WHITE, 1.0), Color::WHITE));
    }
    assert_eq!(blend_state_layer(Color::TRANSPARENT, Color::WHITE, 0.0), Color::TRANSPARENT);
    assert_eq!(blend_state_layer(base, Color::WHITE, -5.0), base);
    assert_eq!(blend_state_layer(base, Color::WHITE, 100.0), Color::WHITE);
}

#[test]
fn stronger_states_move_further_from_the_surface() {
    for (base, state, lighter) in
        [(Color::from_rgb(0.1, 0.1, 0.1), Color::WHITE, true), (Color::from_rgb(0.9, 0.9, 0.9), Color::BLACK, false)]
    {
        let (hover, press) = (hover_layer(base, state), press_layer(base, state));
        for (a, b, c) in [(base.r, hover.r, press.r), (base.g, hover.g, press.g), (base.b, hover.b, press.b)] {
            assert_eq!(lighter, a < b && b < c);
            assert_eq!(!lighter, a > b && b > c);
        }
    }
}

#[test]
fn blending_never_produces_out_of_range_channels() {
    let steps = [0.0, 0.3, 0.7, 1.0];
    for &channel in &steps {
        for &alpha in &steps {
            let base = Color::from_rgba(channel, 1.0 - channel, channel, alpha);
            for &state_alpha in &steps {
                let state = Color::from_rgba(1.0, channel, 0.0, state_alpha);
                for opacity in [0.0, 1e-40, ST_HOVER, 0.5, 1.0] {
                    let out = blend_state_layer(base, state, opacity);
                    assert!([out.r, out.g, out.b, out.a].iter().all(|v| (0.0..=1.0).contains(v)));
                }
            }
        }
    }
}
