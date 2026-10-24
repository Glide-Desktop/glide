use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use smithay::input::dnd::{DndAction, Source, SourceMetadata};
use smithay::utils::{IsAlive, Logical, Point};

use crate::state::GlideState;

const URI_LIST: &str = "text/uri-list";
const READ_LIMIT: u64 = 256 << 20;
const DROP_WAIT: Duration = Duration::from_secs(5);

pub struct SharedSource<S>(pub Arc<S>);

impl<S: Source> IsAlive for SharedSource<S> {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl<S: Source> Source for SharedSource<S> {
    fn is_client_local(&self, target: &dyn std::any::Any) -> bool {
        self.0.is_client_local(target)
    }

    fn metadata(&self) -> Option<SourceMetadata> {
        self.0.metadata()
    }

    fn accepted(&self, mime_type: Option<String>) {
        self.0.accepted(mime_type);
    }

    fn choose_action(&self, action: DndAction) {
        self.0.choose_action(action);
    }

    fn send(&self, mime_type: &str, fd: OwnedFd) {
        self.0.send(mime_type, fd);
    }

    fn drop_performed(&self) {
        self.0.drop_performed();
    }

    fn cancel(&self) {
        self.0.cancel();
    }

    fn finished(&self) {
        self.0.finished();
    }
}

type Slot = Arc<Mutex<Option<(String, Vec<u8>)>>>;

#[derive(Default)]
pub struct DockDrop {
    pub source: Option<Arc<dyn Source>>,
    prefetch: Option<Slot>,
}

impl std::fmt::Debug for DockDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockDrop")
            .field("source", &self.source.is_some())
            .field("prefetching", &self.prefetch.is_some())
            .finish()
    }
}

fn decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(byte) = text.get(index + 1..index + 3).and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

#[must_use]
pub fn local_paths(uri_list: &str) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    uri_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let rest = line.strip_prefix("file://")?;
            let path = rest.find('/').map(|slash| &rest[slash..])?;
            Some(PathBuf::from(std::ffi::OsString::from_vec(decode(path))))
        })
        .collect()
}

const GNOME_FILES: &str = "x-special/gnome-copied-files";
const NAMED_BYTES: &str = "application/octet-stream;name=";
const IMAGES: [(&str, &str); 4] =
    [("image/png", "png"), ("image/jpeg", "jpg"), ("image/webp", "webp"), ("image/gif", "gif")];
const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropped {
    Path(PathBuf),
    Url(String),
}

impl Dropped {
    #[must_use]
    pub fn arg(&self) -> String {
        match self {
            Self::Path(path) => path.to_string_lossy().into_owned(),
            Self::Url(url) => url.clone(),
        }
    }
}

#[must_use]
pub fn preferred_offer(offered: &[String]) -> Option<String> {
    let has = |wanted: &str| offered.iter().any(|mime| mime == wanted);
    [URI_LIST, GNOME_FILES]
        .into_iter()
        .find(|wanted| has(wanted))
        .map(str::to_string)
        .or_else(|| offered.iter().find(|mime| mime.starts_with(NAMED_BYTES)).cloned())
        .or_else(|| IMAGES.into_iter().map(|(mime, _)| mime).find(|wanted| has(wanted)).map(str::to_string))
        .or_else(|| TEXTS.into_iter().find(|wanted| has(wanted)).map(str::to_string))
}

fn is_url(line: &str) -> bool {
    line.split_once("://").is_some_and(|(scheme, rest)| {
        !scheme.is_empty()
            && scheme.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
            && !rest.is_empty()
            && !line.chars().any(char::is_whitespace)
    })
}

#[must_use]
pub fn listed(text: &str) -> Vec<Dropped> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && *line != "copy" && *line != "cut")
        .filter_map(|line| {
            if line.starts_with("file://") {
                local_paths(line).into_iter().next().map(Dropped::Path)
            } else if line.starts_with('/') {
                Some(Dropped::Path(PathBuf::from(line)))
            } else if is_url(line) {
                Some(Dropped::Url(line.to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn safe_name(name: &str) -> Option<String> {
    let name = name.trim().trim_matches('"');
    let name = std::path::Path::new(name).file_name()?.to_string_lossy().into_owned();
    (!name.is_empty() && name != "." && name != "..").then_some(name)
}

fn spool(dir: &std::path::Path, name: &str, bytes: &[u8]) -> Option<Dropped> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).ok()?;
    Some(Dropped::Path(path))
}

#[must_use]
pub fn parse_drop(mime: &str, bytes: &[u8], spool_dir: &std::path::Path) -> Vec<Dropped> {
    if let Some(name) = mime.strip_prefix(NAMED_BYTES) {
        let name = safe_name(name).unwrap_or_else(|| String::from("Dropped file"));
        return spool(spool_dir, &name, bytes).into_iter().collect();
    }
    if let Some((_, ext)) = IMAGES.into_iter().find(|(image, _)| *image == mime) {
        return spool(spool_dir, &format!("Dropped image.{ext}"), bytes).into_iter().collect();
    }
    let text = String::from_utf8_lossy(bytes);
    let mut items = listed(&text);
    items.retain(|item| match item {
        Dropped::Path(path) => path.exists(),
        Dropped::Url(_) => true,
    });
    if items.is_empty() && TEXTS.contains(&mime) && !text.trim().is_empty() {
        return spool(spool_dir, "Dropped text.txt", text.as_bytes()).into_iter().collect();
    }
    items
}

fn spool_dir() -> PathBuf {
    let cache = glide_config::cache_home().unwrap_or_else(std::env::temp_dir);
    let root = cache.join("glide/dropped");
    if let Ok(entries) = std::fs::read_dir(&root) {
        let day = Duration::from_hours(24);
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|at| at.elapsed().is_ok_and(|age| age > day));
            if old {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    root.join(stamp.to_string())
}

#[must_use]
pub fn expand_exec(exec: &str, name: &str, files: &[String]) -> Vec<String> {
    let quoted: Vec<String> = files.iter().map(|file| shell_quote(file)).collect();
    let single = !exec.contains("%F") && !exec.contains("%U") && (exec.contains("%f") || exec.contains("%u"));
    let runs: Vec<&[String]> = if single { quoted.chunks(1).collect() } else { vec![quoted.as_slice()] };
    let mut any_code = false;
    let mut lines: Vec<String> = runs
        .into_iter()
        .map(|run| {
            let mut out = String::with_capacity(exec.len());
            let mut chars = exec.chars();
            while let Some(ch) = chars.next() {
                if ch != '%' {
                    out.push(ch);
                    continue;
                }
                match chars.next() {
                    Some('%') => out.push('%'),
                    Some('f' | 'u') => {
                        any_code = true;
                        out.push_str(run.first().map_or("", String::as_str));
                    }
                    Some('F' | 'U') => {
                        any_code = true;
                        out.push_str(&run.join(" "));
                    }
                    Some('c') => out.push_str(&shell_quote(name)),
                    _ => {}
                }
            }
            out.trim().to_string()
        })
        .collect();
    if !any_code {
        lines = vec![command_with_files(lines.first().map_or("", String::as_str), files)];
    }
    lines
}

#[must_use]
pub fn first_folder(items: &[Dropped]) -> Option<PathBuf> {
    let path = items.iter().find_map(|item| match item {
        Dropped::Path(path) => Some(path),
        Dropped::Url(_) => None,
    })?;
    if path.is_dir() { Some(path.clone()) } else { path.parent().map(std::path::Path::to_path_buf) }
}

#[must_use]
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[must_use]
pub fn command_with_files(command: &str, files: &[String]) -> String {
    let mut line = command.trim().to_string();
    for file in files {
        line.push(' ');
        line.push_str(&shell_quote(file));
    }
    line
}

fn is_terminal(item: &crate::dock::DockItem) -> bool {
    glide_app::integration::is_glide_terminal(&item.command) || item.app_id == "glide-terminal"
}

impl GlideState {
    fn drop_tile_at(&self, pointer: Point<f64, Logical>) -> Option<usize> {
        let area = self.dock_area();
        if !self.dock.contains(area, pointer) {
            return None;
        }
        let index = self.dock.item_at(area, pointer)?;
        let item = self.dock.items().get(index)?;
        let usable = item.action == crate::dock::DockAction::Activate
            && (!item.command.trim().is_empty() || self.launch_command_for(&item.app_id).is_some());
        usable.then_some(index)
    }

    fn launch_command_for(&self, app_id: &str) -> Option<String> {
        self.app_directory.index().and_then(|index| index.get(app_id)).map(|app| app.command.clone())
    }

    fn open_dropped(&mut self, index: usize, items: &[Dropped]) {
        let Some(item) = self.dock.items().get(index).cloned() else {
            return;
        };
        if is_terminal(&item) {
            let Some(folder) = first_folder(items) else {
                return;
            };
            let token = self.mint_granted_token(None);
            let terminal = crate::global_menu::terminal_command();
            let spawned = std::thread::Builder::new().name("glide-dock-drop-open".into()).spawn(move || {
                if glide_app::integration::open_terminal_tab(&folder, &[], &token).is_ok() {
                    return;
                }
                let Some(program) = terminal.split_whitespace().next() else {
                    return;
                };
                let mut command = std::process::Command::new(program);
                command
                    .arg("--working-directory")
                    .arg(&folder)
                    .env("XDG_ACTIVATION_TOKEN", &token)
                    .env("DESKTOP_STARTUP_ID", &token);
                if let Err(err) = crate::spawn_detached(command) {
                    tracing::warn!(?err, "failed to open a terminal for a dropped folder");
                }
            });
            if let Err(err) = spawned {
                tracing::warn!("cannot open the dropped folder: {err}");
            }
            return;
        }
        let files: Vec<String> = items.iter().map(Dropped::arg).collect();
        if files.is_empty() {
            tracing::info!(app = %item.app_id, "nothing to open from the drop on the dock");
            return;
        }
        let entry = self.app_directory.index().and_then(|index| index.get(&item.app_id)).cloned();
        let commands = match entry {
            Some(app)
                if !app.exec.trim().is_empty()
                    && (item.command.trim().is_empty() || item.command.trim() == app.command.trim()) =>
            {
                expand_exec(&app.exec, &app.name, &files)
            }
            Some(app) if item.command.trim().is_empty() => vec![command_with_files(&app.command, &files)],
            _ if !item.command.trim().is_empty() => vec![command_with_files(&item.command, &files)],
            _ => Vec::new(),
        };
        for command in commands.iter().filter(|command| !command.trim().is_empty()) {
            tracing::info!(app = %item.app_id, %command, "opening dropped files");
            self.launch_app(&item.app_id, &item.name, command);
        }
    }

    pub fn prefetch_dock_drop(&mut self, pointer: Point<f64, Logical>) {
        if self.drag.dock_drop.prefetch.is_some() {
            return;
        }
        if self.drop_tile_at(pointer).is_none() {
            return;
        }
        let Some(source) = self.drag.dock_drop.source.clone() else {
            return;
        };
        let offered = source.metadata().map(|metadata| metadata.mime_types).unwrap_or_default();
        let Some(mime) = preferred_offer(&offered) else {
            tracing::info!(?offered, "the dragged data offers nothing the dock can open");
            return;
        };
        let Ok((reader, writer)) = std::io::pipe() else {
            return;
        };
        source.send(&mime, OwnedFd::from(writer));
        let slot: Slot = Arc::default();
        let filled = Arc::clone(&slot);
        let spawned = std::thread::Builder::new().name("glide-dock-drop".into()).spawn(move || {
            let mut bytes = Vec::new();
            if reader.take(READ_LIMIT).read_to_end(&mut bytes).is_ok() {
                *filled.lock().unwrap_or_else(PoisonError::into_inner) = Some((mime, bytes));
            }
        });
        if spawned.is_ok() {
            self.drag.dock_drop.prefetch = Some(slot);
        }
    }

    pub fn finish_dock_drop(&mut self, location: Point<f64, Logical>) {
        let Some(slot) = self.drag.dock_drop.prefetch.take() else {
            return;
        };
        let Some(wanted) = self.drop_tile_at(location) else {
            return;
        };
        let (sender, receiver) = smithay::reexports::calloop::channel::channel::<Vec<Dropped>>();
        let spawned = std::thread::Builder::new().name("glide-dock-drop-wait".into()).spawn(move || {
            let started = Instant::now();
            loop {
                let taken = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
                if let Some((mime, bytes)) = taken {
                    let items = parse_drop(&mime, &bytes, &spool_dir());
                    tracing::info!(%mime, bytes = bytes.len(), items = items.len(), "read a drop on the dock");
                    let _ = sender.send(items);
                    return;
                }
                if started.elapsed() >= DROP_WAIT {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        if let Err(err) = spawned {
            tracing::warn!("cannot wait for the dropped files: {err}");
            return;
        }
        let inserted = self.loop_handle.insert_source(receiver, move |event, (), state| {
            if let smithay::reexports::calloop::channel::Event::Msg(items) = event {
                state.open_dropped(wanted, &items);
                state.queue_redraw();
            }
        });
        if let Err(err) = inserted {
            tracing::warn!(?err, "cannot watch the dropped files");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_local_uri_names_the_folder_to_open() {
        let dir = std::env::temp_dir();
        let uri = glide_app::integration::file_uri(&dir);
        assert_eq!(first_folder(&listed(&format!("# comment\r\n{uri}\r\n"))), Some(dir.clone()));
        let file = dir.join("glide dock drop.txt");
        assert_eq!(first_folder(&listed(&glide_app::integration::file_uri(&file))), Some(dir));
        assert_eq!(first_folder(&listed("https://example.com/\n")), None);
        assert_eq!(first_folder(&[]), None);
    }

    #[test]
    fn links_images_named_bytes_and_text_from_any_app_become_openable_items() {
        let spool = std::env::temp_dir().join(format!("glide-dock-drop-{}", std::process::id()));
        assert_eq!(
            parse_drop(URI_LIST, b"https://example.com/a b\nhttps://example.com/x\n", &spool),
            [Dropped::Url("https://example.com/x".into())],
            "a line with spaces is not a URL"
        );
        let named = parse_drop("application/octet-stream;name=\"../report.pdf\"", b"%PDF", &spool);
        assert_eq!(named, [Dropped::Path(spool.join("report.pdf"))], "the name cannot leave the spool");
        assert_eq!(std::fs::read(spool.join("report.pdf")).expect("spooled"), b"%PDF");
        assert_eq!(parse_drop("image/png", b"png", &spool), [Dropped::Path(spool.join("Dropped image.png"))]);
        assert_eq!(parse_drop("text/plain", b"hello", &spool), [Dropped::Path(spool.join("Dropped text.txt"))]);
        assert!(parse_drop(URI_LIST, b"file:///nowhere/at/all\n", &spool).is_empty(), "missing files are skipped");
        let _ = std::fs::remove_dir_all(spool);
    }

    #[test]
    fn exec_field_codes_take_the_dropped_files() {
        let files = ["/tmp/a b".to_string(), "/tmp/c".to_string()];
        assert_eq!(expand_exec("gimp %U", "GIMP", &files), ["gimp '/tmp/a b' '/tmp/c'"]);
        assert_eq!(expand_exec("mpv --player %f", "mpv", &files), ["mpv --player '/tmp/a b'", "mpv --player '/tmp/c'"]);
        assert_eq!(
            expand_exec("flatpak run --file-forwarding org.app @@u %U @@", "App", &files[1..]),
            ["flatpak run --file-forwarding org.app @@u '/tmp/c' @@"]
        );
        assert_eq!(expand_exec("app --name %c", "My App", &files[1..]), ["app --name 'My App' '/tmp/c'"]);
        assert_eq!(expand_exec("app 100%%", "App", &files[1..]), ["app 100% '/tmp/c'"]);
    }

    #[test]
    fn every_file_offer_reads_back_as_paths() {
        let offered = ["text/plain".to_string(), GNOME_FILES.to_string()];
        assert_eq!(preferred_offer(&offered).as_deref(), Some(GNOME_FILES));
        let chrome = ["text/plain".to_string(), "application/octet-stream;name=\"a.zip\"".to_string()];
        assert_eq!(preferred_offer(&chrome).as_deref(), Some("application/octet-stream;name=\"a.zip\""));
        assert_eq!(preferred_offer(&["image/png".to_string()]).as_deref(), Some("image/png"));
        assert_eq!(preferred_offer(&["application/x-unknown".to_string()]), None);
        let tmp = std::env::temp_dir();
        let uri = format!("file://{}", tmp.display());
        let spool = tmp.join("unused");
        assert_eq!(parse_drop(GNOME_FILES, format!("copy\n{uri}\n").as_bytes(), &spool), [Dropped::Path(tmp.clone())]);
        assert_eq!(parse_drop("text/plain", format!("{}\n", tmp.display()).as_bytes(), &spool), [Dropped::Path(tmp)]);
    }

    #[test]
    fn dropped_files_are_appended_shell_quoted() {
        let files = ["/tmp/it's here.txt".to_string(), "/tmp/b".to_string()];
        assert_eq!(
            command_with_files("gnome-text-editor", &files),
            "gnome-text-editor '/tmp/it'\\''s here.txt' '/tmp/b'"
        );
        assert_eq!(
            local_paths("file:///a%20b\nhttps://x\nfile://host/c\n"),
            [PathBuf::from("/a b"), PathBuf::from("/c")]
        );
    }
}
