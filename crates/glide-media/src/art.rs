use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

pub const ART_MAX_BYTES: u64 = 8 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_LIMIT: usize = 128;
const ATTEMPT_LIMIT: usize = 512;

#[must_use]
pub fn is_remote_art(url: &str) -> bool {
    let url = url.trim();
    ["http://", "https://"]
        .iter()
        .any(|scheme| url.get(..scheme.len()).is_some_and(|head| head.eq_ignore_ascii_case(scheme)))
}

#[must_use]
pub fn art_cache_dir() -> Option<PathBuf> {
    Some(glide_config::cache_home()?.join("glide/media-art"))
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[must_use]
pub fn cached_art_path(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{:016x}", fnv1a(url.trim())))
}

#[must_use]
pub fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut out = String::with_capacity(raw.len() + 7);
    out.push_str("file://");
    for ch in raw.chars() {
        match ch {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            other => out.push(other),
        }
    }
    out
}

pub struct ArtFetcher {
    dir: Option<PathBuf>,
    requests: Option<mpsc::Sender<String>>,
    attempted: HashSet<String>,
}

impl ArtFetcher {
    #[must_use]
    pub fn start() -> Self {
        let dir = art_cache_dir();
        let requests = dir.clone().and_then(|dir| {
            let (sender, inbox) = mpsc::channel::<String>();
            std::thread::Builder::new()
                .name("glide-art".to_string())
                .spawn(move || run_fetcher(&dir, &inbox))
                .map_err(|err| tracing::warn!(?err, "could not start the cover-art fetcher"))
                .ok()
                .map(|_| sender)
        });
        Self { dir, requests, attempted: HashSet::new() }
    }

    #[cfg(test)]
    fn detached(dir: PathBuf) -> (Self, mpsc::Receiver<String>) {
        let (sender, inbox) = mpsc::channel();
        (Self { dir: Some(dir), requests: Some(sender), attempted: HashSet::new() }, inbox)
    }

    pub fn resolve(&mut self, url: &str) -> Option<String> {
        if !is_remote_art(url) {
            return None;
        }
        let dir = self.dir.as_ref()?;
        let path = cached_art_path(dir, url);
        if path.is_file() {
            return Some(file_url(&path));
        }
        if self.attempted.len() >= ATTEMPT_LIMIT {
            self.attempted.clear();
        }
        if self.attempted.insert(url.trim().to_string())
            && let Some(requests) = self.requests.as_ref()
            && requests.send(url.trim().to_string()).is_err()
        {
            self.requests = None;
        }
        None
    }

    pub fn localise(&mut self, art_url: &mut String) {
        if let Some(local) = self.resolve(art_url) {
            *art_url = local;
        }
    }
}

fn run_fetcher(dir: &Path, inbox: &mpsc::Receiver<String>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .user_agent(concat!("glide/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    while let Ok(url) = inbox.recv() {
        let target = cached_art_path(dir, &url);
        if target.is_file() {
            continue;
        }
        match download(&agent, &url, dir, &target) {
            Ok(()) => {
                tracing::debug!(%url, "cached remote cover art");
                prune(dir, CACHE_LIMIT);
            }
            Err(err) => tracing::debug!(%url, %err, "failed to fetch remote cover art"),
        }
    }
}

fn download(agent: &ureq::Agent, url: &str, dir: &Path, target: &Path) -> anyhow::Result<()> {
    let mut response = agent.get(url).call()?;
    let bytes = response.body_mut().with_config().limit(ART_MAX_BYTES).read_to_vec()?;
    anyhow::ensure!(!bytes.is_empty(), "empty response body");
    std::fs::create_dir_all(dir)?;
    let partial = target.with_extension("part");
    std::fs::write(&partial, &bytes)?;
    std::fs::rename(&partial, target)?;
    Ok(())
}

fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            Some((meta.modified().ok()?, entry.path()))
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in files.into_iter().skip(keep) {
        if let Err(err) = std::fs::remove_file(&path) {
            tracing::debug!(path = %path.display(), %err, "failed to prune cover art");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-media-art-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn only_http_urls_are_remote() {
        assert!(is_remote_art("https://i.scdn.co/image/ab67"));
        assert!(is_remote_art("  HTTP://example.com/a.jpg"));
        assert!(!is_remote_art("file:///tmp/a.png"));
        assert!(!is_remote_art(""));
        assert!(!is_remote_art("ht"));
    }

    #[test]
    fn the_cache_name_is_stable_and_ignores_surrounding_space() {
        let dir = Path::new("/cache");
        let a = cached_art_path(dir, "https://example.com/a.jpg");
        assert_eq!(a, cached_art_path(dir, " https://example.com/a.jpg "));
        assert_ne!(a, cached_art_path(dir, "https://example.com/b.jpg"));
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn a_file_url_round_trips_through_the_local_art_decoder() {
        let dir = scratch("roundtrip");
        let path = dir.join("with space %25 #x");
        std::fs::write(&path, b"x").expect("write");
        assert_eq!(crate::local_art_path(&file_url(&path)).as_deref(), Some(path.as_path()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_remote_url_is_queued_once_then_served_from_the_cache() {
        let dir = scratch("resolve");
        let (mut fetcher, inbox) = ArtFetcher::detached(dir.clone());
        let url = "https://example.com/cover.jpg";

        assert_eq!(fetcher.resolve(url), None);
        assert_eq!(fetcher.resolve(url), None);
        assert_eq!(inbox.try_iter().collect::<Vec<_>>(), vec![url.to_string()]);

        std::fs::write(cached_art_path(&dir, url), b"jpeg").expect("write");
        let mut art = url.to_string();
        fetcher.localise(&mut art);
        assert!(art.starts_with("file://"));
        assert!(crate::local_art_path(&art).is_some());

        let mut local = "file:///tmp/x.png".to_string();
        fetcher.localise(&mut local);
        assert_eq!(local, "file:///tmp/x.png");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pruning_keeps_only_the_newest_files() {
        let dir = scratch("prune");
        for index in 0..5 {
            std::fs::write(dir.join(format!("{index}")), b"x").expect("write");
        }
        prune(&dir, 2);
        assert_eq!(std::fs::read_dir(&dir).expect("read").count(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
