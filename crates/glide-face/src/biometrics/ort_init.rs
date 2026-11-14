use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

use super::BiometricError;

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

const SYSTEM_LIBRARIES: [&str; 4] = [
    "/usr/lib/glide/face/libonnxruntime.so",
    "/usr/lib/libonnxruntime.so",
    "/usr/lib64/libonnxruntime.so",
    "/usr/local/lib/libonnxruntime.so",
];

fn library_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = SYSTEM_LIBRARIES.iter().map(PathBuf::from).collect();
    if cfg!(debug_assertions) {
        candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("lib").join("libonnxruntime.so"));
    }
    candidates
}

#[must_use]
pub fn library() -> Option<PathBuf> {
    library_candidates().into_iter().find(|candidate| candidate.exists())
}

pub fn init_ort() -> Result<(), BiometricError> {
    ORT_INIT
        .get_or_init(|| {
            let Some(lib) = library() else {
                return Err("ONNX Runtime is not installed (libonnxruntime.so)".to_string());
            };
            let lib = lib.to_string_lossy().to_string();
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let result =
                std::panic::catch_unwind(|| ort::init_from(lib.clone()).with_name("glide-face").commit().map(|_| ()));
            std::panic::set_hook(previous);
            match result {
                Ok(r) => r.map_err(|e| format!("failed to initialize ONNX Runtime environment: {e}")),
                Err(panic) => {
                    let msg = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_string()))
                        .unwrap_or_else(|| "unknown error".into());
                    Err(format!("failed to load ONNX Runtime library ({lib}): {msg}"))
                }
            }
        })
        .clone()
        .map_err(BiometricError::Init)
}

pub fn build_session(model: &[u8], name: &str) -> Result<Session, BiometricError> {
    init_ort()?;
    let wrap = |e: ort::Error| BiometricError::Init(format!("failed to create {name} session: {e}"));
    Session::builder()
        .map_err(wrap)?
        .with_optimization_level(GraphOptimizationLevel::Level2)
        .map_err(wrap)?
        .with_intra_threads(2)
        .map_err(wrap)?
        .with_inter_threads(1)
        .map_err(wrap)?
        .commit_from_memory(model)
        .map_err(wrap)
}

pub fn run_session(
    session: &Session, input_name: &str, shape: [usize; 4], data: Vec<f32>, outputs: &[&str], model: &str,
) -> Result<Vec<Vec<f32>>, BiometricError> {
    let wrap = |e: ort::Error| BiometricError::Inference(format!("{model} inference failed: {e}"));
    let tensor = ort::value::Tensor::from_array((shape.to_vec(), data.into_boxed_slice())).map_err(wrap)?;
    let inputs = ort::inputs![input_name => tensor].map_err(wrap)?;
    let result = session.run(inputs).map_err(wrap)?;
    outputs
        .iter()
        .map(|name| {
            let value = result
                .get(*name)
                .ok_or_else(|| BiometricError::Inference(format!("{model}: missing output {name}")))?;
            let (_, slice) = value.try_extract_raw_tensor::<f32>().map_err(wrap)?;
            Ok(slice.to_vec())
        })
        .collect()
}
