use std::path::PathBuf;

use chrono::{DateTime, Local};
use serde::Serialize;

use crate::biometrics::{BiometricEvaluation, DetectedFace, duration_ms};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub session_id: String,
    #[serde(with = "crate::biometrics::go_time")]
    pub timestamp: DateTime<Local>,
    pub verdict: String,
    pub reason_code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template_hash: String,
    pub consensus_frames: i32,
    pub required_frames: i32,
    pub metrics: ReportMetrics,
    #[serde(rename = "latencies")]
    pub latency: ReportLatencies,
    pub system: SystemMetadata,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportMetrics {
    pub similarity: f64,
    pub similarity_threshold: f64,
    pub matched: bool,
    pub liveness_score: f64,
    pub liveness_threshold: f64,
    pub liveness_probabilities: [f64; 3],
    pub is_live: bool,
    pub blur_score: f64,
    pub brightness: f64,
    pub contrast: f64,
    pub face_size: f64,
    pub pose_yaw: f64,
    pub pose_pitch: f64,
    pub pose_roll: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_box: Option<DetectedFace>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportLatencies {
    pub capture_ms: f64,
    pub detection_ms: f64,
    pub quality_ms: f64,
    pub alignment_ms: f64,
    pub recognition_ms: f64,
    pub anti_spoof_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMetadata {
    pub os: String,
    pub arch: String,
    pub execution_provider: String,
    pub model_detection: String,
    pub model_landmarks: String,
    pub model_recognition: String,
    pub model_anti_spoof: String,
}

fn arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        "arm" => "arm",
        other => other,
    }
}

impl SystemMetadata {
    fn current() -> Self {
        Self {
            os: std::env::consts::OS.into(),
            arch: arch_name().into(),
            execution_provider: "CPUExecutionProvider".into(),
            model_detection: "YuNet 2023Mar (ONNX)".into(),
            model_landmarks: "InsightFace 2d106det -> 68pt (ONNX)".into(),
            model_recognition: "GhostFaceNetV1-1.3-1 MS1MV3 (ONNX)".into(),
            model_anti_spoof: "MiniFASNetV2 + Hybrid PAD (ONNX)".into(),
        }
    }
}

#[must_use]
pub fn build_report(
    session_id: &str, verdict: bool, reason: &str, consensus: i32, required: i32, eval: Option<&BiometricEvaluation>,
    template_hash: &str,
) -> DiagnosticReport {
    let mut rep = DiagnosticReport {
        session_id: session_id.into(),
        timestamp: Local::now(),
        verdict: if verdict { "PASS" } else { "FAIL" }.into(),
        reason_code: reason.into(),
        template_hash: template_hash.into(),
        consensus_frames: consensus,
        required_frames: required,
        metrics: ReportMetrics::default(),
        latency: ReportLatencies::default(),
        system: SystemMetadata::current(),
    };
    if let Some(e) = eval {
        let l = &e.latency;
        rep.latency = ReportLatencies {
            capture_ms: duration_ms(l.capture),
            detection_ms: duration_ms(l.detection),
            quality_ms: duration_ms(l.quality),
            alignment_ms: duration_ms(l.alignment),
            recognition_ms: duration_ms(l.recognition),
            anti_spoof_ms: duration_ms(l.anti_spoof),
            total_ms: duration_ms(l.total),
        };
        let m = &mut rep.metrics;
        m.blur_score = e.quality.blur_score;
        m.brightness = e.quality.brightness;
        m.contrast = e.quality.contrast;
        m.face_size = e.quality.face_size;
        m.pose_yaw = e.quality.pose_yaw;
        m.pose_pitch = e.quality.pose_pitch;
        m.pose_roll = e.quality.pose_roll;
        m.face_box.clone_from(&e.face);
        if let Some(a) = &e.anti_spoof {
            m.liveness_score = a.score;
            m.liveness_threshold = a.threshold;
            m.liveness_probabilities = a.class_probs;
            m.is_live = a.is_live;
        }
        if let Some(r) = &e.recognition {
            m.similarity = r.similarity;
            m.similarity_threshold = r.threshold;
            m.matched = r.matched;
        }
    }
    rep
}

pub fn to_pretty_json<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let s = serde_json::to_string_pretty(value)?;
    Ok(s.replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026"))
}

pub fn user_config_dir() -> Result<PathBuf, String> {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME")
        && !x.is_empty()
    {
        let p = PathBuf::from(x);
        if p.is_absolute() {
            return Ok(p);
        }
        return Err("path in $XDG_CONFIG_HOME is relative".into());
    }
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => Ok(PathBuf::from(h).join(".config")),
        _ => Err("neither $XDG_CONFIG_HOME nor $HOME are defined".into()),
    }
}

impl DiagnosticReport {
    pub fn to_json(&self) -> serde_json::Result<String> {
        to_pretty_json(self)
    }

    pub fn save_to_file(&self, custom_path: &str) -> Result<PathBuf, String> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let data = self.to_json().map_err(|e| e.to_string())?;
        let target = if custom_path.is_empty() {
            let cfg = user_config_dir().map_err(|e| format!("failed to locate user config directory: {e}"))?;
            let dir = cfg.join("glide-face").join("reports");
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&dir)
                .map_err(|e| format!("failed to create reports directory: {e}"))?;
            dir.join(format!("report_{}_{}.json", self.session_id, self.timestamp.format("%Y%m%d_%H%M%S")))
        } else {
            PathBuf::from(custom_path)
        };
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            let mut f =
                std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&target)?;
            f.write_all(data.as_bytes())
        };
        write().map_err(|e| format!("failed to save report to {}: {e}", target.display()))?;
        Ok(target)
    }
}
