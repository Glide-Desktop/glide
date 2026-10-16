use std::collections::VecDeque;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iced::futures::channel::mpsc::{UnboundedReceiver, unbounded};

use super::uri::file_uri;

pub const SIZE: u32 = 128;
const WORKERS: usize = 2;
const BUILTIN_LIMIT: u64 = 64 << 20;
const TIMEOUT: Duration = Duration::from_secs(8);
const KEEP_FOR: Duration = Duration::from_secs(90 * 24 * 60 * 60);
const CACHE_LIMIT: u64 = 256 << 20;
const BUILTIN: [&str; 5] = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnailer {
    exec: String,
    mimes: Vec<String>,
}

#[must_use]
pub fn load_thumbnailers(data: &[PathBuf]) -> Vec<Thumbnailer> {
    let mut found = Vec::new();
    for dir in data.iter().map(|dir| dir.join("thumbnailers")) {
        let Ok(files) = std::fs::read_dir(dir) else {
            continue;
        };
        for file in files.flatten().map(|entry| entry.path()) {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let value = |key: &str| {
                text.lines().find_map(|line| line.trim().strip_prefix(key)?.strip_prefix('=')).map(str::trim)
            };
            let installed = value("TryExec").is_none_or(|program| {
                if program.contains('/') {
                    Path::new(program).is_file()
                } else {
                    std::env::var_os("PATH")
                        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
                }
            });
            if let (true, Some(exec), Some(mimes)) = (installed, value("Exec"), value("MimeType")) {
                found.push(Thumbnailer {
                    exec: exec.to_string(),
                    mimes: mimes.split(';').filter(|mime| !mime.is_empty()).map(str::to_string).collect(),
                });
            }
        }
    }
    found
}

#[must_use]
pub fn can_thumbnail(mime: &str, thumbnailers: &[Thumbnailer]) -> bool {
    BUILTIN.contains(&mime)
        || thumbnailers.iter().any(|thumbnailer| thumbnailer.mimes.iter().any(|known| known == mime))
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

fn cache_dir() -> Option<PathBuf> {
    Some(glide_config::cache_home()?.join("glide/thumbnails"))
}

#[must_use]
pub fn cache_name(path: &Path, modified_nanos: u128, len: u64) -> String {
    let key =
        fnv(path.as_os_str().as_bytes().iter().copied().chain(modified_nanos.to_le_bytes()).chain(len.to_le_bytes()));
    format!("{key:016x}.png")
}

fn run_thumbnailer(thumbnailer: &Thumbnailer, input: &Path, output: &Path) -> bool {
    let uri = file_uri(input);
    let size = SIZE.to_string();
    let command: Vec<String> = thumbnailer
        .exec
        .split_whitespace()
        .map(|word| {
            word.replace("%i", &input.to_string_lossy())
                .replace("%u", &uri)
                .replace("%o", &output.to_string_lossy())
                .replace("%s", &size)
                .replace("%%", "%")
        })
        .collect();
    let Some((program, args)) = command.split_first() else {
        return false;
    };
    let Ok(mut child) =
        Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    else {
        return false;
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success() && output.is_file(),
            Ok(None) if started.elapsed() < TIMEOUT => {
                std::thread::sleep(Duration::from_millis(40));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn builtin(input: &Path, output: &Path) -> bool {
    let decoded = image::ImageReader::open(input)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(image::ImageError::IoError)
        .and_then(image::ImageReader::decode);
    decoded
        .map(|image| image.thumbnail(SIZE, SIZE))
        .and_then(|thumb| thumb.save_with_format(output, image::ImageFormat::Png))
        .is_ok()
}

#[must_use]
pub fn make(input: &Path, mime: &str, thumbnailers: &[Thumbnailer]) -> Option<PathBuf> {
    make_in(&cache_dir()?, input, mime, thumbnailers)
}

#[must_use]
pub fn make_in(dir: &Path, input: &Path, mime: &str, thumbnailers: &[Thumbnailer]) -> Option<PathBuf> {
    let meta = std::fs::metadata(input).ok().filter(std::fs::Metadata::is_file)?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let output = dir.join(cache_name(input, modified, meta.len()));
    if output.is_file() {
        return Some(output);
    }
    let failed = output.with_extension("fail");
    if failed.exists() {
        return None;
    }
    std::fs::create_dir_all(dir).ok()?;
    let partial = output.with_extension("part.png");
    let external = thumbnailers
        .iter()
        .filter(|thumbnailer| thumbnailer.mimes.iter().any(|known| known == mime))
        .any(|thumbnailer| run_thumbnailer(thumbnailer, input, &partial));
    let made = external || (BUILTIN.contains(&mime) && meta.len() <= BUILTIN_LIMIT && builtin(input, &partial));
    if made && std::fs::rename(&partial, &output).is_ok() {
        return Some(output);
    }
    let _ = std::fs::remove_file(&partial);
    let _ = std::fs::write(failed, b"");
    None
}

pub fn prune_in(dir: &Path, now: SystemTime, keep_for: Duration, limit: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            let modified = meta.modified().ok()?;
            let used = meta.accessed().map_or(modified, |accessed| accessed.max(modified));
            Some((used, meta.len(), entry.path()))
        })
        .collect();
    files.sort();
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    for (used, len, path) in files {
        let stale = now.duration_since(used).is_ok_and(|age| age > keep_for);
        if !stale && total <= limit {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn prune_once() {
    static PRUNED: Once = Once::new();
    PRUNED.call_once(|| {
        let _ = std::thread::Builder::new().name(String::from("glide-files-prune")).spawn(|| {
            if let Some(dir) = cache_dir() {
                prune_in(&dir, SystemTime::now(), KEEP_FOR, CACHE_LIMIT);
            }
        });
    });
}

pub type Result = (PathBuf, Option<PathBuf>);

#[must_use]
pub fn generate(
    requests: Vec<(PathBuf, String)>, thumbnailers: &Arc<Vec<Thumbnailer>>, latest: &Arc<AtomicU64>, generation: u64,
) -> UnboundedReceiver<Result> {
    prune_once();
    let (sender, receiver) = unbounded();
    let queue = Arc::new(Mutex::new(VecDeque::from(requests)));
    for _ in 0..WORKERS {
        let (queue, sender) = (Arc::clone(&queue), sender.clone());
        let (thumbnailers, latest) = (Arc::clone(thumbnailers), Arc::clone(latest));
        let _ = std::thread::Builder::new().name(String::from("glide-files-thumbs")).spawn(move || {
            while latest.load(Ordering::Relaxed) == generation {
                let next = queue.lock().unwrap_or_else(PoisonError::into_inner).pop_front();
                let Some((path, mime)) = next else {
                    return;
                };
                let made = make(&path, &mime, &thumbnailers);
                if sender.unbounded_send((path, made)).is_err() {
                    return;
                }
            }
        });
    }
    receiver
}
