use std::time::{Duration, Instant};

use super::image::RgbaImage;
use super::klt::{extract_gray_buffer, track_point_klt};
use super::landmarks::compute_ear68;
use super::types::{DENSE_LANDMARK_COUNT, DetectedFace, Landmarks5, Point2D, clamp_f64};
use super::yunet::compute_iou;

const HISTORY: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct OneEuroFilter {
    pub min_cutoff: f64,
    pub beta: f64,
    pub d_cutoff: f64,
    x_prev: f64,
    dx_prev: f64,
    has_prev: bool,
}

impl OneEuroFilter {
    #[must_use]
    pub const fn new(min_cutoff: f64, beta: f64, d_cutoff: f64) -> Self {
        Self { min_cutoff, beta, d_cutoff, x_prev: 0.0, dx_prev: 0.0, has_prev: false }
    }

    pub fn reset(&mut self) {
        self.has_prev = false;
        self.x_prev = 0.0;
        self.dx_prev = 0.0;
    }

    pub fn filter(&mut self, x: f64, dt: f64, scale: f64) -> f64 {
        if !self.has_prev || dt <= 0.0 || dt > 0.45 {
            self.x_prev = x;
            self.dx_prev = 0.0;
            self.has_prev = true;
            return x;
        }
        let diff = (x - self.x_prev).abs();
        let deadzone = 1.0f64.max(scale * 0.005);
        if diff >= deadzone {
            self.dx_prev = (x - self.x_prev) / dt;
            self.x_prev = x;
            return x;
        }
        let alpha = 0.70 + 0.30 * (diff / deadzone);
        let x_hat = alpha * x + (1.0 - alpha) * self.x_prev;
        self.x_prev = x_hat;
        x_hat
    }
}

pub struct FaceTracker {
    track_id: i32,
    track_age: i32,
    last_time: Option<Instant>,
    last_box: DetectedFace,
    has_track: bool,
    stability: f64,

    hist_count: usize,
    hist_idx: usize,
    ear_hist: [f64; HISTORY],
    parallax_u: [f64; HISTORY],
    parallax_v: [f64; HISTORY],
    center_u: [f64; HISTORY],
    center_v: [f64; HISTORY],
    in_blink: bool,
    blink_count: i32,
    motion_score: f64,

    prev_gray: Vec<u8>,
    curr_gray: Vec<u8>,
    prev_w: usize,
    prev_h: usize,

    box_filters: [OneEuroFilter; 4],
    kps5_filters: [OneEuroFilter; 10],
    dense_filters: Vec<OneEuroFilter>,
}

impl Default for FaceTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl FaceTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            track_id: 0,
            track_age: 0,
            last_time: None,
            last_box: DetectedFace::default(),
            has_track: false,
            stability: 0.0,
            hist_count: 0,
            hist_idx: 0,
            ear_hist: [0.0; HISTORY],
            parallax_u: [0.0; HISTORY],
            parallax_v: [0.0; HISTORY],
            center_u: [0.0; HISTORY],
            center_v: [0.0; HISTORY],
            in_blink: false,
            blink_count: 0,
            motion_score: 0.92,
            prev_gray: Vec::new(),
            curr_gray: Vec::new(),
            prev_w: 0,
            prev_h: 0,
            box_filters: [OneEuroFilter::new(1.8, 1.4, 1.0); 4],
            kps5_filters: [OneEuroFilter::new(2.2, 1.8, 1.0); 10],
            dense_filters: vec![OneEuroFilter::new(2.0, 1.6, 1.0); DENSE_LANDMARK_COUNT * 2],
        }
    }

    pub fn reset(&mut self) {
        self.has_track = false;
        self.track_age = 0;
        self.stability = 0.0;
        self.hist_count = 0;
        self.hist_idx = 0;
        self.in_blink = false;
        self.blink_count = 0;
        self.motion_score = 0.92;
        self.prev_w = 0;
        self.prev_h = 0;
        self.box_filters.iter_mut().for_each(OneEuroFilter::reset);
        self.kps5_filters.iter_mut().for_each(OneEuroFilter::reset);
        self.dense_filters.iter_mut().for_each(OneEuroFilter::reset);
    }

    #[allow(clippy::too_many_lines, clippy::similar_names)] // One tracking step: target pick, KLT fusion and filtering share the frame state
    pub fn select_and_track(
        &mut self, frame: Option<&RgbaImage>, mut faces: Vec<DetectedFace>, frame_w: usize, frame_h: usize,
        now: Instant,
    ) -> Vec<DetectedFace> {
        if faces.is_empty() {
            if let Some(last) = self.last_time
                && now.saturating_duration_since(last) > Duration::from_millis(350)
            {
                self.reset();
            }
            return faces;
        }

        let (mut gw, mut gh) = (0, 0);
        if let Some(f) = frame {
            (gw, gh) = extract_gray_buffer(f, &mut self.curr_gray);
        }
        let has_klt = frame.is_some()
            && gw > 0
            && gh > 0
            && gw == self.prev_w
            && gh == self.prev_h
            && self.prev_gray.len() == gw * gh;

        if faces.len() > 1 {
            let cx_f = frame_w as f64 * 0.5;
            let cy_f = frame_h as f64 * 0.5;
            let diag = cx_f.hypot(cy_f).max(1.0);
            let mut best_idx = 0;
            let mut best_score = -1e9;
            for (i, f) in faces.iter().enumerate() {
                let fcx = f64::from(f.x + f.width * 0.5);
                let fcy = f64::from(f.y + f.height * 0.5);
                let center_w = 1.0 - 0.35 * ((fcx - cx_f).hypot(fcy - cy_f) / diag);
                let area_w = f64::from(f.width * f.height).sqrt();
                let bonus = if self.has_track && compute_iou(f, &self.last_box) > 0.35 { 80.0 } else { 0.0 };
                let s = f64::from(f.score) * area_w * center_w + bonus;
                if s > best_score {
                    best_score = s;
                    best_idx = i;
                }
            }
            faces.swap(0, best_idx);
        }

        let mut raw = faces[0].clone();
        let dt = self.last_time.map_or(f64::MAX, |t| now.saturating_duration_since(t).as_secs_f64());

        if !self.has_track || dt <= 0.0 || dt > 0.45 || compute_iou(&raw, &self.last_box) < 0.25 {
            self.reset();
            self.track_id += 1;
            self.has_track = true;
            self.track_age = 1;
            self.stability = 0.95;
            self.motion_score = 0.92;
            self.last_time = Some(now);
            self.last_box = raw.clone();
            let scale = f64::from(raw.width).max(f64::from(raw.height));
            self.box_filters[0].filter(f64::from(raw.x), 0.0, scale);
            self.box_filters[1].filter(f64::from(raw.y), 0.0, scale);
            self.box_filters[2].filter(f64::from(raw.width), 0.0, scale);
            self.box_filters[3].filter(f64::from(raw.height), 0.0, scale);
            for (i, p) in raw.landmarks.as_array().iter().enumerate() {
                self.kps5_filters[i * 2].filter(f64::from(p.x), 0.0, scale);
                self.kps5_filters[i * 2 + 1].filter(f64::from(p.y), 0.0, scale);
            }
            for (i, p) in raw.dense.iter().take(DENSE_LANDMARK_COUNT).enumerate() {
                self.dense_filters[i * 2].filter(f64::from(p.x), 0.0, scale);
                self.dense_filters[i * 2 + 1].filter(f64::from(p.y), 0.0, scale);
            }
            self.record_temporal_sample(&raw);
            self.swap_gray(frame.is_some(), gw, gh);
            let f0 = &mut faces[0];
            f0.track_id = self.track_id;
            f0.track_age = self.track_age;
            f0.stability = self.stability;
            f0.ear = compute_ear68(&raw.dense);
            f0.motion_score = self.motion_score;
            f0.blink_count = self.blink_count;
            return faces;
        }

        self.track_age += 1;
        self.last_time = Some(now);
        let scale = f64::from(raw.width).max(f64::from(raw.height)).max(32.0);

        let prev_kps = self.last_box.landmarks.as_array();
        let mut curr_kps = raw.landmarks.as_array();
        if has_klt {
            for i in 0..5 {
                let (pt, conf) =
                    track_point_klt(&self.prev_gray, &self.curr_gray, gw, gh, prev_kps[i], curr_kps[i], scale);
                if conf > 0.0 {
                    let w = conf as f32;
                    curr_kps[i].x = w * pt.x + (1.0 - w) * curr_kps[i].x;
                    curr_kps[i].y = w * pt.y + (1.0 - w) * curr_kps[i].y;
                }
            }
            if self.last_box.has_dense() && raw.has_dense() {
                for i in 0..DENSE_LANDMARK_COUNT {
                    let (pt, conf) = track_point_klt(
                        &self.prev_gray,
                        &self.curr_gray,
                        gw,
                        gh,
                        self.last_box.dense[i],
                        raw.dense[i],
                        scale,
                    );
                    if conf > 0.0 {
                        let w = conf as f32;
                        raw.dense[i].x = w * pt.x + (1.0 - w) * raw.dense[i].x;
                        raw.dense[i].y = w * pt.y + (1.0 - w) * raw.dense[i].y;
                    }
                }
            }
        }

        let motion: f64 = (0..5)
            .map(|i| f64::from(curr_kps[i].x - prev_kps[i].x).hypot(f64::from(curr_kps[i].y - prev_kps[i].y)) / scale)
            .sum::<f64>()
            / 5.0;
        self.stability = 0.70 * self.stability + 0.30 * (-motion * 14.0).exp();

        self.record_temporal_sample(&raw);

        raw.x = self.box_filters[0].filter(f64::from(raw.x), dt, scale) as f32;
        raw.y = self.box_filters[1].filter(f64::from(raw.y), dt, scale) as f32;
        raw.width = self.box_filters[2].filter(f64::from(raw.width), dt, scale) as f32;
        raw.height = self.box_filters[3].filter(f64::from(raw.height), dt, scale) as f32;

        let mut filtered = [Point2D::default(); 5];
        for (i, p) in curr_kps.iter().enumerate() {
            filtered[i] = Point2D::new(
                self.kps5_filters[i * 2].filter(f64::from(p.x), dt, scale) as f32,
                self.kps5_filters[i * 2 + 1].filter(f64::from(p.y), dt, scale) as f32,
            );
        }
        raw.landmarks = Landmarks5::from_array(filtered);
        for i in 0..raw.dense.len().min(DENSE_LANDMARK_COUNT) {
            let p = raw.dense[i];
            raw.dense[i] = Point2D::new(
                self.dense_filters[i * 2].filter(f64::from(p.x), dt, scale) as f32,
                self.dense_filters[i * 2 + 1].filter(f64::from(p.y), dt, scale) as f32,
            );
        }

        self.swap_gray(frame.is_some(), gw, gh);
        raw.track_id = self.track_id;
        raw.track_age = self.track_age;
        raw.stability = clamp_f64(self.stability, 0.0, 1.0);
        raw.ear = compute_ear68(&raw.dense);
        raw.motion_score = self.motion_score;
        raw.blink_count = self.blink_count;
        self.last_box = raw.clone();
        faces[0] = raw;
        faces
    }

    fn swap_gray(&mut self, has_frame: bool, gw: usize, gh: usize) {
        if has_frame && gw > 0 && gh > 0 {
            std::mem::swap(&mut self.prev_gray, &mut self.curr_gray);
            self.prev_w = gw;
            self.prev_h = gh;
        }
    }

    #[allow(clippy::many_single_char_names, clippy::similar_names)] // Running statistics over paired u / v series
    fn record_temporal_sample(&mut self, f: &DetectedFace) {
        let ear = compute_ear68(&f.dense);
        let (r, l, nose) = (f.landmarks.right_eye, f.landmarks.left_eye, f.landmarks.nose_tip);
        let mut d_eyes = f64::from(l.x - r.x).hypot(f64::from(l.y - r.y));
        if d_eyes < 4.0 {
            d_eyes = f64::from(f.width) * 0.42;
        }
        let eye_mid_x = 0.5 * f64::from(r.x + l.x);
        let eye_mid_y = 0.5 * f64::from(r.y + l.y);
        let idx = self.hist_idx % HISTORY;
        self.ear_hist[idx] = ear;
        self.parallax_u[idx] = (f64::from(nose.x) - eye_mid_x) / d_eyes;
        self.parallax_v[idx] = (f64::from(nose.y) - eye_mid_y) / d_eyes;
        self.center_u[idx] = f64::from(f.x + 0.5 * f.width) / d_eyes;
        self.center_v[idx] = f64::from(f.y + 0.5 * f.height) / d_eyes;
        self.hist_idx += 1;
        if self.hist_count < HISTORY {
            self.hist_count += 1;
        }
        if self.hist_count < 4 {
            return;
        }

        let n = self.hist_count as f64;
        let c = self.hist_count;
        let mean = |a: &[f64; HISTORY]| a[..c].iter().sum::<f64>() / n;
        let (m_ear, m_pu, m_pv, m_cu, m_cv) = (
            mean(&self.ear_hist),
            mean(&self.parallax_u),
            mean(&self.parallax_v),
            mean(&self.center_u),
            mean(&self.center_v),
        );

        if !self.in_blink && ear < 0.22f64.min(m_ear * 0.82) {
            self.in_blink = true;
        } else if self.in_blink && ear > m_ear * 0.92 {
            self.in_blink = false;
            self.blink_count += 1;
        }

        let (mut var_ear, mut var_par, mut var_trans) = (0.0, 0.0, 0.0);
        for i in 0..c {
            let de = self.ear_hist[i] - m_ear;
            var_ear += de * de;
            let (dpu, dpv) = (self.parallax_u[i] - m_pu, self.parallax_v[i] - m_pv);
            var_par += dpu * dpu + dpv * dpv;
            let (dcu, dcv) = (self.center_u[i] - m_cu, self.center_v[i] - m_cv);
            var_trans += dcu * dcu + dcv * dcv;
        }
        let std_ear = (var_ear / n).sqrt();
        let std_par = (var_par / n).sqrt();
        let std_trans = (var_trans / n).sqrt();

        if self.track_age >= 8 && std_trans > 0.025 {
            let ratio = std_par / (std_trans + 1e-5);
            if ratio < 0.035 && std_ear < 0.0015 && self.blink_count == 0 {
                self.motion_score = 0.25;
                return;
            }
        }
        let score = if self.blink_count > 0 {
            1.00
        } else if std_ear > 0.0020 || std_par > 0.0030 {
            0.96
        } else {
            0.90
        };
        self.motion_score = 0.75 * self.motion_score + 0.25 * score;
    }
}
