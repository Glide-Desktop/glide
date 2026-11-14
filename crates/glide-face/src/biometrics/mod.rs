pub mod align;
pub mod antispoof;
pub mod calibration;
pub mod image;
pub mod klt;
pub mod landmark_onnx;
pub mod landmarks;
pub mod ort_init;
pub mod pipeline;
pub mod pose3d;
pub mod quality;
pub mod recognizer;
pub mod tracker;
pub mod types;
pub mod yunet;

pub use self::image::RgbaImage;
pub use antispoof::{DEFAULT_LIVENESS_THRESHOLD, analyze_texture_and_screen_artifacts};
pub use calibration::{default_match_threshold, thresholds};
pub use landmarks::{FACE_MESH_CONTOUR_EDGES, FACE_MESH_TRIANGULATION_EDGES, compute_ear68, extract_dense_landmarks};
pub use pipeline::Pipeline;
pub use pose3d::estimate_head_pose_3d;
pub use quality::{DEFAULT_QUALITY_THRESHOLDS, QualityThresholds, compute_quality};
pub use recognizer::{
    EMBEDDING_DIM, RECOGNIZER_ID, average_embeddings, best_match_similarity, compute_adaptive_threshold,
    cosine_similarity, weighted_average_embeddings,
};
pub use types::*;

pub mod models;

#[derive(Debug, Clone, thiserror::Error)]
pub enum BiometricError {
    #[error("{0}")]
    Init(String),
    #[error("{0}")]
    Inference(String),
}
