use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

use base64::Engine as _;

pub const ART_MAX_BYTES: u64 = 8 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_LIMIT: usize = 128;
const ATTEMPT_LIMIT: usize = 512;
const RETRY_AFTER: Duration = Duration::from_mins(1);
const RESTART_AFTER: Duration = Duration::from_secs(30);
const MAX_HOPS: usize = 4;

fn authority(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let (host, port) = match authority.strip_prefix('[') {
        Some(bracketed) => {
            let (host, tail) = bracketed.split_once(']')?;
            (host, tail.strip_prefix(':'))
        }
        None => match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    let port = match port {
        Some(port) => port.parse().ok()?,
        None if scheme.eq_ignore_ascii_case("https") => 443,
        None => 80,
    };
    Some((host.trim_end_matches('.').to_ascii_lowercase(), port))
}

#[must_use]
pub(crate) fn local_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || first == 0
                || (first == 100 && second & 0xc0 == 64)
        }
        std::net::IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || ip.is_unspecified()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                || ip.to_ipv4_mapped().is_some_and(|mapped| local_ip(std::net::IpAddr::V4(mapped)))
        }
    }
}

#[must_use]
pub(crate) fn local_host(url: &str) -> bool {
    let Some((host, _)) = authority(url) else { return true };
    if host.is_empty() || host == "localhost" || matches!(host.rsplit_once('.'), Some((_, "local" | "localhost"))) {
        return true;
    }
    host.parse::<std::net::IpAddr>().is_ok_and(local_ip)
}

fn resolves_locally(url: &str) -> bool {
    use std::net::ToSocketAddrs;
    let Some((host, port)) = authority(url) else { return true };
    match (host.as_str(), port).to_socket_addrs() {
        Ok(addresses) => {
            let addresses: Vec<_> = addresses.collect();
            addresses.is_empty() || addresses.iter().any(|address| local_ip(address.ip()))
        }
        Err(_) => true,
    }
}

#[must_use]
pub(crate) fn is_remote_art(url: &str) -> bool {
    let url = url.trim();
    ["http://", "https://"]
        .iter()
        .any(|scheme| url.get(..scheme.len()).is_some_and(|head| head.eq_ignore_ascii_case(scheme)))
}

#[must_use]
pub(crate) fn art_cache_dir() -> Option<PathBuf> {
    Some(glide_config::cache_home()?.join("glide/media-art"))
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[must_use]
pub(crate) fn cached_art_path(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{:016x}", fnv1a(url.trim())))
}

#[must_use]
pub(crate) fn is_inline_art(url: &str) -> bool {
    url.trim().get(..5).is_some_and(|head| head.eq_ignore_ascii_case("data:"))
}

fn inline_art(url: &str) -> Option<Vec<u8>> {
    let url = url.trim();
    let rest = url.get(5..).filter(|_| is_inline_art(url))?;
    let (header, payload) = rest.split_once(',')?;
    let mut parts = header.split(';');
    let mime = parts.next()?.trim();
    if !mime.get(..6).is_some_and(|head| head.eq_ignore_ascii_case("image/")) {
        return None;
    }
    if !parts.any(|part| part.trim().eq_ignore_ascii_case("base64")) {
        return None;
    }
    if payload.len() as u64 > ART_MAX_BYTES / 3 * 4 + 8 {
        return None;
    }
    let payload: String = payload.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let engine = base64::engine::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    );
    let bytes = engine.decode(payload).ok()?;
    (!bytes.is_empty() && bytes.len() as u64 <= ART_MAX_BYTES).then_some(bytes)
}

fn store(dir: &Path, target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let partial = target.with_extension("part");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, target)
}

fn spawn_fetcher(dir: &Path) -> Option<mpsc::SyncSender<String>> {
    let dir = dir.to_path_buf();
    let (sender, inbox) = mpsc::sync_channel::<String>(1);
    std::thread::Builder::new()
        .name("glide-art".to_string())
        .spawn(move || run_fetcher(&dir, &inbox))
        .map_err(|err| tracing::warn!(?err, "could not start the cover-art fetcher"))
        .ok()
        .map(|_| sender)
}

pub(crate) struct ArtFetcher {
    dir: Option<PathBuf>,
    requests: Option<mpsc::SyncSender<String>>,
    attempted: HashMap<String, Instant>,
    spawned: Instant,
}

impl ArtFetcher {
    #[must_use]
    pub fn start() -> Self {
        let dir = art_cache_dir();
        let requests = dir.as_deref().and_then(spawn_fetcher);
        Self { dir, requests, attempted: HashMap::new(), spawned: Instant::now() }
    }

    #[cfg(test)]
    fn detached(dir: PathBuf) -> (Self, mpsc::Receiver<String>) {
        let (sender, inbox) = mpsc::sync_channel(1);
        (Self { dir: Some(dir), requests: Some(sender), attempted: HashMap::new(), spawned: Instant::now() }, inbox)
    }

    fn sender(&mut self) -> Option<&mpsc::SyncSender<String>> {
        if self.requests.is_none() && self.spawned.elapsed() >= RESTART_AFTER {
            self.spawned = Instant::now();
            self.requests = self.dir.as_deref().and_then(spawn_fetcher);
        }
        self.requests.as_ref()
    }

    fn recently_tried(&mut self, url: &str) -> bool {
        if self.attempted.len() >= ATTEMPT_LIMIT {
            self.attempted.clear();
        }
        if self.attempted.get(url).is_some_and(|at| at.elapsed() < RETRY_AFTER) {
            return true;
        }
        self.attempted.insert(url.to_string(), Instant::now());
        false
    }

    fn resolve_inline(&mut self, url: &str) -> Option<String> {
        let path = cached_art_path(self.dir.as_ref()?, url);
        if path.is_file() {
            return Some(glide_config::uri::file_uri(&path));
        }
        if self.recently_tried(url) {
            return None;
        }
        let bytes = inline_art(url)?;
        let dir = self.dir.as_ref()?;
        if let Err(err) = store(dir, &path, &bytes) {
            tracing::debug!(%err, "failed to cache inline cover art");
            return None;
        }
        prune(dir, CACHE_LIMIT);
        Some(glide_config::uri::file_uri(&path))
    }

    pub fn resolve(&mut self, url: &str) -> Option<String> {
        let url = url.trim();
        if is_inline_art(url) {
            return self.resolve_inline(url);
        }
        if !is_remote_art(url) || local_host(url) {
            return None;
        }
        let path = cached_art_path(self.dir.as_ref()?, url);
        if path.is_file() {
            return Some(glide_config::uri::file_uri(&path));
        }
        if self.recently_tried(url) {
            return None;
        }
        match self.sender().map(|requests| requests.try_send(url.to_string())) {
            Some(Ok(())) => {}
            Some(Err(mpsc::TrySendError::Full(_))) | None => {
                self.attempted.remove(url);
            }
            Some(Err(mpsc::TrySendError::Disconnected(_))) => {
                tracing::debug!("the cover-art fetcher stopped; it is started again later");
                self.requests = None;
                self.attempted.remove(url);
            }
        }
        None
    }

    pub(crate) fn localise(&mut self, art_url: &mut String) {
        if let Some(local) = self.resolve(art_url) {
            *art_url = local;
        }
    }
}

fn run_fetcher(dir: &Path, inbox: &mpsc::Receiver<String>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .max_redirects(0)
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

fn next_hop(current: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if is_remote_art(location) {
        return Some(location.to_string());
    }
    let (scheme, rest) = current.trim().split_once("://")?;
    if let Some(elsewhere) = location.strip_prefix("//") {
        return Some(format!("{scheme}://{elsewhere}"));
    }
    if location.starts_with('/') {
        let authority = rest.split(['/', '?', '#']).next()?;
        return Some(format!("{scheme}://{authority}{location}"));
    }
    None
}

fn image_content(content_type: Option<&str>) -> bool {
    let Some(content_type) = content_type else { return true };
    let mime = content_type.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    mime.starts_with("image/") || mime == "application/octet-stream" || mime == "binary/octet-stream"
}

fn download(agent: &ureq::Agent, url: &str, dir: &Path, target: &Path) -> anyhow::Result<()> {
    let mut current = url.to_string();
    for _ in 0..=MAX_HOPS {
        anyhow::ensure!(
            is_remote_art(&current) && !local_host(&current) && !resolves_locally(&current),
            "cover art that points into the local network is not fetched"
        );
        let mut response = agent.get(&current).call()?;
        let status = response.status();
        if status.is_redirection() {
            let location = response.headers().get("location").and_then(|value| value.to_str().ok()).unwrap_or("");
            current =
                next_hop(&current, location).ok_or_else(|| anyhow::anyhow!("redirect without a usable target"))?;
            continue;
        }
        anyhow::ensure!(status.is_success(), "the server answered {status}");
        let content_type = response.headers().get("content-type").and_then(|value| value.to_str().ok());
        anyhow::ensure!(image_content(content_type), "the response is not an image");
        let bytes = response.body_mut().with_config().limit(ART_MAX_BYTES).read_to_vec()?;
        anyhow::ensure!(!bytes.is_empty(), "empty response body");
        store(dir, target, &bytes)?;
        return Ok(());
    }
    anyhow::bail!("too many redirects")
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
    fn covers_on_this_machine_or_the_local_network_are_refused() {
        assert!(local_host("http://localhost/a.jpg"));
        assert!(local_host("http://[::ffff:127.0.0.1]:8080/a.jpg"));
        assert!(local_host("http://user@192.168.1.4/a.jpg"));
        assert!(local_host("https://printer.local/a.jpg"));
        assert!(!local_host("https://i.scdn.co/image/ab67"));
        assert!(resolves_locally("http://2130706433/a.jpg"));
        assert!(resolves_locally("http://0x7f000001/a.jpg"));
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
        assert_eq!(crate::local_art_path(&glide_config::uri::file_uri(&path)).as_deref(), Some(path.as_path()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cached_cover_under_a_non_utf8_folder_is_still_found() {
        use std::os::unix::ffi::OsStrExt;
        let root = scratch("latin1");
        let dir = root.join(std::ffi::OsStr::from_bytes(b"caf\xe9"));
        std::fs::create_dir_all(&dir).expect("non-UTF-8 dir");
        let (mut fetcher, _inbox) = ArtFetcher::detached(dir.clone());
        let url = "https://example.com/cover.jpg";
        std::fs::write(cached_art_path(&dir, url), b"jpeg").expect("write");
        let local = fetcher.resolve(url).expect("served from the cache");
        assert_eq!(crate::local_art_path(&local), Some(cached_art_path(&dir, url)));
        let _ = std::fs::remove_dir_all(root);
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
    fn a_failed_cover_is_asked_for_again_after_a_while() {
        let dir = scratch("retry");
        let (mut fetcher, inbox) = ArtFetcher::detached(dir.clone());
        let url = "https://example.com/flaky.jpg";
        assert_eq!(fetcher.resolve(url), None);
        assert_eq!(inbox.try_recv().unwrap(), url);
        assert_eq!(fetcher.resolve(url), None);
        assert!(inbox.try_recv().is_err(), "not again straight away");
        let long_ago = Instant::now().checked_sub(RETRY_AFTER * 2).expect("monotonic clock");
        fetcher.attempted.insert(url.to_string(), long_ago);
        assert_eq!(fetcher.resolve(url), None);
        assert_eq!(inbox.try_recv().unwrap(), url);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stopped_fetcher_is_started_again_only_after_a_pause() {
        let dir = scratch("restart");
        let (mut fetcher, inbox) = ArtFetcher::detached(dir.clone());
        drop(inbox);
        assert_eq!(fetcher.resolve("https://example.com/a.jpg"), None);
        assert!(fetcher.requests.is_none());
        assert!(fetcher.sender().is_none(), "no restart loop right after a failure");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn redirects_are_followed_only_to_web_addresses() {
        let from = "https://cdn.example.com/a/b.jpg?x=1";
        assert_eq!(next_hop(from, "https://img.example.org/c.jpg").as_deref(), Some("https://img.example.org/c.jpg"));
        assert_eq!(next_hop(from, "//img.example.org/c.jpg").as_deref(), Some("https://img.example.org/c.jpg"));
        assert_eq!(next_hop(from, "/c.jpg").as_deref(), Some("https://cdn.example.com/c.jpg"));
        assert_eq!(next_hop(from, "file:///etc/passwd"), None);
        assert_eq!(next_hop(from, "c.jpg"), None);
        assert_eq!(next_hop(from, ""), None);
        assert!(local_host(&next_hop(from, "http://127.0.0.1/x").unwrap()), "each hop is checked again");
    }

    #[test]
    fn only_image_responses_are_cached() {
        assert!(image_content(Some("image/jpeg")));
        assert!(image_content(Some("Image/PNG; charset=binary")));
        assert!(image_content(Some("application/octet-stream")));
        assert!(image_content(None));
        assert!(!image_content(Some("text/html; charset=utf-8")));
    }

    #[test]
    fn an_inline_cover_is_decoded_into_the_cache() {
        let dir = scratch("inline");
        let (mut fetcher, inbox) = ArtFetcher::detached(dir.clone());
        let url = "data:image/png;base64,iVBORw0K\nGgo=";
        let local = fetcher.resolve(url).expect("decoded");
        let path = crate::local_art_path(&local).expect("a local file");
        assert_eq!(std::fs::read(path).unwrap(), b"\x89PNG\r\n\x1a\n");
        assert!(inbox.try_recv().is_err(), "nothing is downloaded");
        assert_eq!(fetcher.resolve(url), Some(local));
        assert_eq!(inline_art("data:image/png;base64,iVBORw0KGgo").as_deref(), Some(&b"\x89PNG\r\n\x1a\n"[..]));
        assert_eq!(inline_art("data:text/plain;base64,aGk="), None);
        assert_eq!(inline_art("data:image/svg+xml,<svg/>"), None);
        assert_eq!(inline_art("data:image/png;base64,!!!"), None);
        assert_eq!(fetcher.resolve("data:image/png;base64,"), None);
        std::fs::remove_dir_all(dir).unwrap();
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

    #[test]
    fn a_busy_fetcher_bounds_its_queue_and_retries_the_current_cover() {
        let dir = scratch("busy");
        let (mut fetcher, inbox) = ArtFetcher::detached(dir.clone());
        let first = "https://example.com/first.jpg";
        let current = "https://example.com/current.jpg";
        assert_eq!(fetcher.resolve(first), None);
        assert_eq!(fetcher.resolve(current), None);
        assert_eq!(inbox.try_recv().unwrap(), first);
        assert!(inbox.try_recv().is_err());
        assert_eq!(fetcher.resolve(current), None);
        assert_eq!(inbox.try_recv().unwrap(), current);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
