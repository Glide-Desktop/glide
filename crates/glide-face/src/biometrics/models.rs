use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use super::BiometricError;

pub const SYSTEM_DIR: &str = "/usr/share/glide/face";

pub const YUNET: &str = "face_detection_yunet_2023mar.onnx";
pub const RECOGNIZER: &str = "ghostfacenet_v1_w1.3_s1_ms1mv3.onnx";
pub const MINIFASNET: &str = "MiniFASNetV2.onnx";
pub const LANDMARK106: &str = "2d106det.onnx";

pub const ALL: [&str; 4] = [YUNET, RECOGNIZER, MINIFASNET, LANDMARK106];

#[must_use]
pub fn dir() -> PathBuf {
    let system = Path::new(SYSTEM_DIR);
    if cfg!(debug_assertions) && !system.join(YUNET).exists() {
        return Path::new(env!("CARGO_MANIFEST_DIR")).join("models");
    }
    system.to_path_buf()
}

#[must_use]
pub fn installed() -> bool {
    let dir = dir();
    ALL.iter().all(|name| dir.join(name).is_file())
}

pub fn load(name: &str) -> Result<Vec<u8>, BiometricError> {
    let path = dir().join(name);
    let missing = |e: std::io::Error| BiometricError::Init(format!("model {}: {e}", path.display()));
    let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&path).map_err(missing)?;
    let meta = file.metadata().map_err(missing)?;
    if path.starts_with(SYSTEM_DIR) && (meta.uid() != 0 || meta.mode() & 0o022 != 0) {
        return Err(BiometricError::Init(format!(
            "model {} must be owned by root and not writable by others",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    file.read_to_end(&mut bytes).map_err(missing)?;
    Ok(bytes)
}
