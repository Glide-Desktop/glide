use std::sync::Mutex;

use ort::session::Session;

use super::image::RgbaImage;
use super::landmarks::compute_ear68;
use super::ort_init::{build_session, run_session};
use super::types::{
    DENSE_LANDMARK_COUNT, DetectedFace, LM_LEFT_MOUTH_CORNER, LM_NOSE_TIP, LM_RIGHT_MOUTH_CORNER, Point2D,
};
use super::{BiometricError, models};

const INPUT: usize = 192;
const OUT_COUNT: usize = 106;
const PLANE: usize = INPUT * INPUT;

const LANDMARK_106_TO_68: [usize; DENSE_LANDMARK_COUNT] = [
    1, 10, 12, 14, 16, 3, 5, 7, 0, 23, 21, 19, 32, 30, 28, 26, 17, 43, 48, 49, 51, 50, 102, 103, 104, 105, 101, 72, 73,
    74, 86, 78, 79, 80, 85, 84, 35, 41, 42, 39, 37, 36, 89, 95, 96, 93, 91, 90, 52, 64, 63, 71, 67, 68, 61, 58, 59, 53,
    56, 55, 65, 66, 62, 70, 69, 57, 60, 54,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropBox {
    pub cx: f64,
    pub cy: f64,
    pub size: f64,
}

impl CropBox {
    #[must_use]
    pub fn from_face(face: &DetectedFace) -> Self {
        Self {
            cx: f64::from(face.x) + f64::from(face.width) * 0.5,
            cy: f64::from(face.y) + f64::from(face.height) * 0.5,
            size: (f64::from(face.width.max(face.height)) * 1.5).max(16.0),
        }
    }
}

pub struct Landmark68Aligner {
    session: Mutex<Session>,
}

impl Landmark68Aligner {
    pub fn new() -> Result<Self, BiometricError> {
        Ok(Self { session: Mutex::new(build_session(&models::load(models::LANDMARK106)?, "Landmark68")?) })
    }

    pub fn predict68(&self, src: &RgbaImage, face: &mut DetectedFace) -> Result<Vec<Point2D>, BiometricError> {
        let crop = CropBox::from_face(face);
        self.predict68_in(src, face, crop)
    }

    pub fn predict68_in(
        &self, src: &RgbaImage, face: &mut DetectedFace, crop: CropBox,
    ) -> Result<Vec<Point2D>, BiometricError> {
        if src.width < 8 || src.height < 8 {
            return Err(BiometricError::Inference("image too small for landmark regression".into()));
        }
        let (src_w, src_h) = (src.width, src.height);
        let box_size = crop.size;
        let half = box_size * 0.5;
        let x0 = crop.cx - half;
        let y0 = crop.cy - half;
        let step = box_size / INPUT as f64;
        let stride = src.stride();
        let pix = &src.pix;
        let mut input = vec![0f32; 3 * PLANE];

        for dy in 0..INPUT {
            let sy = (y0 + (dy as f64 + 0.5) * step - 0.5).clamp(0.0, (src_h - 1) as f64);
            let iy0 = sy as usize;
            let iy1 = (iy0 + 1).min(src_h - 1);
            let wy = sy - iy0 as f64;
            let (row0, row1) = (iy0 * stride, iy1 * stride);
            for dx in 0..INPUT {
                let sx = (x0 + (dx as f64 + 0.5) * step - 0.5).clamp(0.0, (src_w - 1) as f64);
                let ix0 = sx as usize;
                let ix1 = (ix0 + 1).min(src_w - 1);
                let wx = sx - ix0 as f64;
                let (p00, p01, p10, p11) = (row0 + ix0 * 4, row0 + ix1 * 4, row1 + ix0 * 4, row1 + ix1 * 4);
                let w00 = (1.0 - wx) * (1.0 - wy);
                let w01 = wx * (1.0 - wy);
                let w10 = (1.0 - wx) * wy;
                let w11 = wx * wy;
                let o = dy * INPUT + dx;
                for ch in 0..3 {
                    let v = w00 * f64::from(pix[p00 + ch])
                        + w01 * f64::from(pix[p01 + ch])
                        + w10 * f64::from(pix[p10 + ch])
                        + w11 * f64::from(pix[p11 + ch]);
                    input[ch * PLANE + o] = v as f32;
                }
            }
        }

        let out = {
            let session = self.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            run_session(&session, "data", [1, 3, INPUT, INPUT], input, &["fc1"], "Landmark68")?
        };
        let raw = &out[0];
        if raw.len() < OUT_COUNT * 2 {
            return Err(BiometricError::Inference("Landmark68: short output".into()));
        }

        let mut p106 = [Point2D::default(); OUT_COUNT];
        for (i, p) in p106.iter_mut().enumerate() {
            let nx = (f64::from(raw[i * 2]) + 1.0) * 0.5;
            let ny = (f64::from(raw[i * 2 + 1]) + 1.0) * 0.5;
            *p = Point2D::new((x0 + nx * box_size) as f32, (y0 + ny * box_size) as f32);
        }
        let mut pts: Vec<Point2D> = LANDMARK_106_TO_68.iter().map(|&i| p106[i]).collect();

        let blend = |a: Point2D, b: Point2D, wa: f32| {
            let wb = 1.0 - wa;
            Point2D::new(wa * a.x + wb * b.x, wa * a.y + wb * b.y)
        };
        pts[17] = blend(p106[43], p106[48], 0.70);
        pts[18] = blend(p106[48], p106[44], 0.60);
        pts[19] = blend(p106[49], p106[45], 0.60);
        pts[20] = blend(p106[51], p106[47], 0.60);
        pts[21] = blend(p106[50], p106[46], 0.55);
        pts[22] = blend(p106[102], p106[97], 0.55);
        pts[23] = blend(p106[103], p106[98], 0.60);
        pts[24] = blend(p106[104], p106[99], 0.60);
        pts[25] = blend(p106[105], p106[100], 0.60);
        pts[26] = blend(p106[101], p106[105], 0.70);

        face.landmarks.right_eye = p106[38];
        face.landmarks.left_eye = p106[88];
        face.landmarks.nose_tip = pts[LM_NOSE_TIP];
        face.landmarks.right_mouth = pts[LM_RIGHT_MOUTH_CORNER];
        face.landmarks.left_mouth = pts[LM_LEFT_MOUTH_CORNER];
        face.ear = compute_ear68(&pts);
        Ok(pts)
    }
}
