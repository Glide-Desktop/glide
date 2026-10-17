use std::time::Duration;

use glide_files::widgets::squish::{BUTTON, eased};

#[test]
fn a_pressed_button_settles_at_the_reference_scale_over_t1() {
    let t1 = Duration::from_millis(120);
    assert!((BUTTON - 0.975).abs() < f32::EPSILON, "style.css: .btn:active {{ transform: scale(.975) }}");
    assert!((eased(1.0, BUTTON, Duration::ZERO, t1) - 1.0).abs() < f32::EPSILON);
    let halfway = eased(1.0, BUTTON, t1 / 2, t1);
    assert!(halfway < 1.0 && halfway > BUTTON, "{halfway}");
    assert!(1.0 - halfway > (1.0 - BUTTON) / 2.0, "E_OUT front-loads the motion: {halfway}");
    assert!((eased(1.0, BUTTON, t1 * 3, t1) - BUTTON).abs() < f32::EPSILON);
    assert!(
        (eased(BUTTON, 1.0, Duration::from_millis(5), Duration::ZERO) - 1.0).abs() < f32::EPSILON,
        "no motion, no delay"
    );
}
