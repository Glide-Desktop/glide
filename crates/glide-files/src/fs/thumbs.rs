use std::collections::VecDeque;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use super::uri::file_uri;

pub const SIZE: u32 = 128;
const WORKERS: usize = 2;
const BUILTIN_LIMIT: u64 = 64 << 20;
const TIMEOUT: Duration = Duration::from_secs(8);
const KEEP_FOR: Duration = Duration::from_hours(90 * 24);
const CACHE_LIMIT: u64 = 256 << 20;
const BUILTIN: [&str; 5] = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnailer {
    exec: String,
    mimes: Vec<String>,
}

#[must_use]
pub(crate) fn load_thumbnailers(data: &[PathBuf]) -> Vec<Thumbnailer> {
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
            let installed = value("TryExec").is_none_or(glide_config::on_path);
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
    let mut command = Vec::new();
    let mut word = OsString::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = thumbnailer.exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                let Some(c) = chars.next() else {
                    return false;
                };
                word.push(c.encode_utf8(&mut [0; 4]));
                started = true;
            }
            '%' => {
                match chars.next() {
                    Some('i') => word.push(input.as_os_str()),
                    Some('u') => word.push(&uri),
                    Some('o') => word.push(output.as_os_str()),
                    Some('s') => word.push(&size),
                    Some('%') => word.push("%"),
                    _ => return false,
                }
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    command.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c.encode_utf8(&mut [0; 4]));
                started = true;
            }
        }
    }
    if quoted {
        return false;
    }
    if started {
        command.push(word);
    }
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
    let partial = output.with_extension(format!("{}-{:?}.part.png", std::process::id(), std::thread::current().id()));
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

type Job = (PathBuf, String, Arc<Vec<Thumbnailer>>, Arc<AtomicU64>, u64, UnboundedSender<Result>);

static QUEUE: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

fn claim_worker() -> bool {
    ACTIVE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| (active < WORKERS).then_some(active + 1)).is_ok()
}

fn work() {
    loop {
        let next = QUEUE.lock().unwrap_or_else(PoisonError::into_inner).pop_front();
        let Some((path, mime, thumbnailers, latest, generation, sender)) = next else {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
            let empty = QUEUE.lock().unwrap_or_else(PoisonError::into_inner).is_empty();
            if empty {
                return;
            }
            let reclaimed = claim_worker();
            if !reclaimed {
                return;
            }
            continue;
        };
        if latest.load(Ordering::Relaxed) != generation || sender.is_closed() {
            continue;
        }
        let made = make(&path, &mime, &thumbnailers);
        let _ = sender.unbounded_send((path, made));
    }
}

#[must_use]
pub(crate) fn generate(
    requests: Vec<(PathBuf, String)>, thumbnailers: &Arc<Vec<Thumbnailer>>, latest: &Arc<AtomicU64>, generation: u64,
) -> UnboundedReceiver<Result> {
    prune_once();
    let (sender, receiver) = unbounded();
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner).extend(
        requests
            .into_iter()
            .map(|(path, mime)| (path, mime, Arc::clone(thumbnailers), Arc::clone(latest), generation, sender.clone())),
    );
    drop(sender);
    while claim_worker() {
        if std::thread::Builder::new().name(String::from("glide-files-thumbs")).spawn(work).is_err() {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
            break;
        }
    }
    receiver
}

#[cfg(test)]
mod tests {
    use super::{ACTIVE, Thumbnailer, WORKERS, generate, run_thumbnailer};
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn repeated_requests_share_one_bounded_worker_pool() {
        let thumbnailers = Arc::new(Vec::new());
        let latest = Arc::new(AtomicU64::new(7));
        let requests = |batch: usize| {
            (0..50)
                .map(|index| {
                    let path = std::path::PathBuf::from(format!("/no-such-glide-thumb/{batch}/{index}.png"));
                    (path, String::from("image/png"))
                })
                .collect::<Vec<_>>()
        };
        let mut receivers: Vec<_> = (0..4).map(|batch| generate(requests(batch), &thumbnailers, &latest, 7)).collect();
        let mut seen = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while seen < 200 && std::time::Instant::now() < deadline {
            assert!(ACTIVE.load(Ordering::Acquire) <= WORKERS, "thumbnail workers stay bounded across calls");
            for receiver in &mut receivers {
                while let Ok((_, made)) = receiver.try_recv() {
                    assert!(made.is_none());
                    seen += 1;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(seen, 200, "every queued request is answered");
    }

    #[test]
    fn stale_generations_are_skipped() {
        let thumbnailers = Arc::new(Vec::new());
        let latest = Arc::new(AtomicU64::new(1));
        let request = vec![(std::path::PathBuf::from("/no-such-glide-thumb/stale.png"), String::from("image/png"))];
        let mut receiver = generate(request, &thumbnailers, &latest, 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match receiver.try_recv() {
                Err(err) if err.is_closed() => break,
                Ok(_) => panic!("a stale request produced a result"),
                Err(_) => {
                    assert!(std::time::Instant::now() < deadline, "the stream closes once its jobs are dropped");
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    }

    #[test]
    fn thumbnailer_arguments_preserve_quotes_field_codes_and_filename_bytes() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_nanos();
        let dir = std::env::temp_dir().join(format!("glide-thumbnailer-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&dir).expect("scratch directory");
        let program = dir.join("copy tool");
        std::os::unix::fs::symlink("/bin/cp", &program).expect("quoted program path");
        let mut thumbnailer =
            Thumbnailer { exec: format!("\"{}\" \"%i\" \"%o\"", program.display()), mimes: Vec::new() };
        let names = [OsString::from("input %s %o %% \"quoted\".png"), OsString::from_vec(b"raw-\xff-%s.png".to_vec())];
        for (index, name) in names.into_iter().enumerate() {
            let input = dir.join(name);
            let output = dir.join(format!("result {index} %i %s.png"));
            std::fs::write(&input, b"thumbnail data").expect("input");
            assert!(run_thumbnailer(&thumbnailer, &input, &output));
            assert_eq!(std::fs::read(&output).expect("output"), b"thumbnail data");
        }
        let input = dir.join("literal%i.png");
        let output = dir.join("literal-output.png");
        std::fs::write(&input, b"literal percent").expect("literal input");
        thumbnailer.exec =
            format!("\"{}\" \"{}\" \"%o\"", program.display(), input.display().to_string().replace('%', "%%"));
        assert!(run_thumbnailer(&thumbnailer, &input, &output));
        assert_eq!(std::fs::read(&output).expect("literal output"), b"literal percent");
        thumbnailer.exec = format!("\"{}\" \"%i\" \"%o", program.display());
        let output = dir.join("invalid-output.png");
        assert!(!run_thumbnailer(&thumbnailer, &input, &output));
        assert!(!output.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
