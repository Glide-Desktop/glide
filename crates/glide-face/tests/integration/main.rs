mod biometrics;
mod calibration;
mod camera;
mod golden_parity;
mod profile;
mod session;

pub const SAMPLE: &str = "tests/fixtures/sample.jpg";
pub const SAMPLE_RGBA: &str = "tests/fixtures/sample_rgba.bin";
pub const SAMPLE_GOLDEN: &str = "tests/fixtures/sample.jpg.json";

#[must_use]
pub fn fixtures_present() -> bool {
    let present = [SAMPLE, SAMPLE_RGBA, SAMPLE_GOLDEN].iter().all(|path| std::path::Path::new(path).is_file())
        && glide_face::biometrics::models::installed()
        && glide_face::biometrics::ort_init::library().is_some();
    if !present {
        eprintln!("skipped: FaceKey models, ONNX Runtime or face fixtures are not available");
    }
    present
}

#[must_use]
pub fn pipeline() -> Option<glide_face::biometrics::Pipeline> {
    fixtures_present().then(|| glide_face::biometrics::Pipeline::new().expect("pipeline"))
}

#[must_use]
pub fn sample() -> glide_face::biometrics::RgbaImage {
    glide_face::biometrics::RgbaImage::open(SAMPLE).expect(SAMPLE)
}
