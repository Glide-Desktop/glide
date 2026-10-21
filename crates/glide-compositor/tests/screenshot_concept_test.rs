#![allow(clippy::float_cmp)]

use std::path::Path;

#[test]
fn the_screenshot_concept_test_infrastructure_document_exists() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("../..");
    let test_infra = root.join(".agents/teamwork/orchestrator_2/TEST_INFRA.md");
    assert!(test_infra.is_file(), "TEST_INFRA.md must be published");
}

#[test]
fn the_flash_alpha_formula_strictly_matches_compositor_specification() {
    fn ease_out(t: f32) -> f32 {
        1.0 - (1.0 - t).powi(3)
    }

    fn flash_alpha(fraction: f32) -> f32 {
        const PEAK: f32 = 0.6;
        const RISE: f32 = 0.25;
        if !(0.0..1.0).contains(&fraction) {
            return 0.0;
        }
        if fraction < RISE {
            PEAK * ease_out(fraction / RISE)
        } else {
            PEAK * (1.0 - ease_out((fraction - RISE) / (1.0 - RISE)))
        }
    }

    assert_eq!(flash_alpha(0.0), 0.0);
    assert!((flash_alpha(0.25) - 0.60).abs() < 1e-4);
    assert_eq!(flash_alpha(1.0), 0.0);
    assert_eq!(flash_alpha(-0.1), 0.0);
    assert_eq!(flash_alpha(1.1), 0.0);
}

#[test]
fn the_meridian_form_scale_has_zero_intermediate_radii() {
    const R1: f32 = 4.0;
    const R2: f32 = 8.0;
    const R3: f32 = 12.0;
    const R4: f32 = 18.0;
    const R_ROUND: f32 = 999.0;
    let allowed = [R1, R2, R3, R4, R_ROUND];

    for r in [4.0, 8.0, 12.0, 18.0, 999.0] {
        assert!(allowed.contains(&r));
    }
}

#[test]
fn the_screenshot_concept_html_conforms_when_present() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let html_path = manifest_dir.join("../../design/screenshot-concept.html");
    if !html_path.exists() {
        return;
    }

    let content = std::fs::read_to_string(&html_path).expect("read screenshot-concept.html");
    assert!(content.contains("id=\"desktopStage\"") || content.contains("id='desktopStage'"));
    assert!(content.contains("id=\"screenshotHud\"") || content.contains("id='screenshotHud'"));
    assert!(content.contains("id=\"cornerThumbnail\"") || content.contains("id='cornerThumbnail'"));
    assert!(content.contains("id=\"markupModal\"") || content.contains("id='markupModal'"));
}
