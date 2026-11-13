use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::output::Output;

use crate::window::GlideWindow;

const FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone)]
pub enum Target {
    Screen(Output),
    Area { output: Output, area: crate::shot::model::Area },
    Window { window: GlideWindow, output: Output },
}

impl Target {
    #[must_use]
    pub fn output(&self) -> &Output {
        match self {
            Self::Screen(output) | Self::Area { output, .. } | Self::Window { output, .. } => output,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub audio: bool,
    pub cursor: bool,
}

#[derive(Debug, Clone)]
pub struct SavedRecording {
    pub path: PathBuf,
    pub duration: Duration,
    pub preview: Option<Arc<glide_shell::image::Image>>,
}

pub trait RecorderSink: Send {
    fn push_frame(&mut self, image: &glide_shell::image::Image, pts: Duration) -> Result<(), String>;
    fn finish(self: Box<Self>) -> Result<(), String>;
}

pub fn parse_user_dirs_videos(text: &str, home: &Path) -> Option<PathBuf> {
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("XDG_VIDEOS_DIR=").map(|value| value.trim().trim_matches('"')))?;
    if let Some(rest) = value.strip_prefix("$HOME") {
        let rest = rest.trim_start_matches('/');
        return Some(if rest.is_empty() { home.to_path_buf() } else { home.join(rest) });
    }
    value.starts_with('/').then(|| PathBuf::from(value))
}

pub fn pick_videos_dir_from(home: &Path, videos_override: Option<&Path>) -> PathBuf {
    let videos = videos_override.map_or_else(|| home.join("Videos"), Path::to_path_buf);
    if !videos.is_dir()
        && let Err(err) = std::fs::create_dir_all(&videos)
    {
        tracing::warn!(?err, dir = %videos.display(), "could not create the videos folder, saving into home");
    }
    if videos.is_dir() { videos } else { home.to_path_buf() }
}

pub fn pick_videos_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let videos_override = std::env::var_os("XDG_VIDEOS_DIR").map(PathBuf::from).or_else(|| {
        let user_dirs = glide_config::xdg_path("", "user-dirs.dirs")?;
        parse_user_dirs_videos(&std::fs::read_to_string(user_dirs).ok()?, &home)
    });
    pick_videos_dir_from(&home, videos_override.as_deref())
}

pub fn next_recording_path() -> PathBuf {
    let dir = pick_videos_dir();
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    dir.join(format!("glide-recording-{timestamp}.mp4"))
}

pub struct Session {
    pub started: Instant,
    pub target: Target,
    pub options: Options,
    pub path: PathBuf,
    pub first_frame: Option<Arc<glide_shell::image::Image>>,
    pub last_capture: Option<Instant>,
    pub sink: Option<Box<dyn RecorderSink>>,
}

impl Session {
    pub fn new(target: Target, options: Options, now: Instant) -> Self {
        Self {
            started: now,
            target,
            options,
            path: next_recording_path(),
            first_frame: None,
            last_capture: None,
            sink: None,
        }
    }

    #[must_use]
    pub fn with_sink(mut self, sink: Box<dyn RecorderSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    #[must_use]
    pub fn output(&self) -> &Output {
        self.target.output()
    }

    #[must_use]
    pub fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.started)
    }

    #[must_use]
    pub fn needs_frame(&self, output: &Output, now: Instant) -> bool {
        if self.output() != output {
            return false;
        }
        if self.first_frame.is_none() {
            return true;
        }
        if self.sink.is_some() {
            self.last_capture.is_none_or(|last| now.saturating_duration_since(last) >= FRAME_INTERVAL)
        } else {
            false
        }
    }

    #[allow(clippy::needless_pass_by_value)] // Public Session API accepts owned Arc frame
    pub fn accept_frame(&mut self, frame: Arc<glide_shell::image::Image>, now: Instant) {
        if self.first_frame.is_none() {
            self.first_frame = Some(Arc::clone(&frame));
        }
        self.last_capture = Some(now);
        let pts = self.elapsed(now);
        if let Some(sink) = self.sink.as_mut()
            && let Err(err) = sink.push_frame(&frame, pts)
        {
            tracing::error!(%err, "recording sink push_frame failed");
        }
    }

    #[must_use]
    pub fn uses_window(&self, window: &GlideWindow) -> bool {
        match &self.target {
            Target::Window { window: target_window, .. } => target_window == window,
            _ => false,
        }
    }

    pub fn finish(mut self, now: Instant) -> SavedRecording {
        let duration = self.elapsed(now);
        if let Some(sink) = self.sink.take()
            && let Err(err) = sink.finish()
        {
            tracing::error!(%err, "recording sink finish failed");
        }
        SavedRecording { path: self.path, duration, preview: self.first_frame }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::output::{Mode, PhysicalProperties, Subpixel};
    use smithay::utils::{Point, Rectangle, Size};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn make_output(name: &str) -> Output {
        let output = Output::new(
            name.to_string(),
            PhysicalProperties {
                size: (1920, 1080).into(),
                subpixel: Subpixel::Unknown,
                make: "Glide".into(),
                model: "Virtual".into(),
                serial_number: "0x00".into(),
            },
        );
        output.change_current_state(Some(Mode { size: (1920, 1080).into(), refresh: 60000 }), None, None, None);
        output
    }

    struct MockSink {
        frames: Arc<AtomicUsize>,
        finished: Arc<AtomicBool>,
    }

    impl RecorderSink for MockSink {
        fn push_frame(&mut self, _image: &glide_shell::image::Image, _pts: Duration) -> Result<(), String> {
            self.frames.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn finish(self: Box<Self>) -> Result<(), String> {
            self.finished.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn parse_user_dirs_videos_supports_dollar_home_and_absolute_paths() {
        let home = Path::new("/home/test");
        let text = "XDG_VIDEOS_DIR=\"$HOME/Відео\"\nXDG_MUSIC_DIR=\"$HOME/Музика\"\n";
        assert_eq!(parse_user_dirs_videos(text, home), Some(PathBuf::from("/home/test/Відео")));
        assert_eq!(
            parse_user_dirs_videos("XDG_VIDEOS_DIR=\"/mnt/media/videos\"", home),
            Some(PathBuf::from("/mnt/media/videos"))
        );
        assert_eq!(parse_user_dirs_videos("XDG_PICTURES_DIR=\"$HOME/Pics\"", home), None);
    }

    #[test]
    fn next_recording_path_generates_mp4_extension() {
        let path = next_recording_path();
        assert_eq!(path.extension().and_then(|s| s.to_str()), Some("mp4"));
        assert!(path.file_name().and_then(|s| s.to_str()).unwrap().starts_with("glide-recording-"));
    }

    #[test]
    fn session_lifecycle_elapsed_and_finish() {
        let output = make_output("DP-1");
        let target = Target::Screen(output.clone());
        let options = Options { audio: true, cursor: false };
        let now = Instant::now();
        let session = Session::new(target, options, now);

        assert_eq!(session.output(), &output);
        assert_eq!(session.options, options);
        assert_eq!(session.first_frame, None);
        let future = now + Duration::from_secs(3);
        assert_eq!(session.elapsed(future), Duration::from_secs(3));

        let saved = session.finish(future);
        assert_eq!(saved.duration, Duration::from_secs(3));
        assert!(saved.preview.is_none());
    }

    #[test]
    fn session_needs_frame_and_accept_frame_captures_first_frame_and_notifies_sink() {
        let output1 = make_output("DP-1");
        let output2 = make_output("DP-2");
        let target = Target::Screen(output1.clone());
        let options = Options { audio: false, cursor: true };
        let now = Instant::now();

        let frames_count = Arc::new(AtomicUsize::new(0));
        let finished_flag = Arc::new(AtomicBool::new(false));
        let mock_sink = Box::new(MockSink { frames: Arc::clone(&frames_count), finished: Arc::clone(&finished_flag) });

        let mut session = Session::new(target, options, now).with_sink(mock_sink);

        assert!(!session.needs_frame(&output2, now));
        assert!(session.needs_frame(&output1, now));

        let image = Arc::new(glide_shell::image::Image::new(100, 100).expect("image creation"));
        session.accept_frame(Arc::clone(&image), now);

        assert!(session.first_frame.is_some());
        assert_eq!(frames_count.load(Ordering::SeqCst), 1);

        let immediate = now + Duration::from_millis(5);
        assert!(!session.needs_frame(&output1, immediate));

        let later = now + Duration::from_millis(20);
        assert!(session.needs_frame(&output1, later));

        let saved = session.finish(later);
        assert!(finished_flag.load(Ordering::SeqCst));
        assert!(saved.preview.is_some());
    }

    #[test]
    fn target_output_and_area_extraction() {
        let output = make_output("HDMI-A-1");
        let area = Rectangle::new(Point::from((10.0, 20.0)), Size::from((400.0, 300.0)));
        let target = Target::Area { output: output.clone(), area };
        assert_eq!(target.output(), &output);
    }
}
