use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::output::Output;

use crate::window::GlideWindow;

#[derive(Debug, Clone)]
pub enum ShareTarget<W = GlideWindow> {
    Screen { output: Output },
    FilteredDesktop { output: Output, windows: Vec<W> },
    Window { window: W, output: Output },
    MultiWindow { windows: Vec<W>, output: Output },
}

impl<W> ShareTarget<W> {
    #[must_use]
    pub fn output(&self) -> &Output {
        match self {
            Self::Screen { output }
            | Self::FilteredDesktop { output, .. }
            | Self::Window { output, .. }
            | Self::MultiWindow { output, .. } => output,
        }
    }

    #[must_use]
    pub fn uses_window(&self, window: &W) -> bool
    where
        W: PartialEq,
    {
        match self {
            Self::Screen { .. } => false,
            Self::FilteredDesktop { windows, .. } | Self::MultiWindow { windows, .. } => {
                windows.iter().any(|w| w == window)
            }
            Self::Window { window: target, .. } => target == window,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareOptions {
    pub app_id: Option<String>,
    pub cursor: bool,
    pub framerate: u32,
}

pub trait ShareSink: Send {
    fn push_frame(&mut self, image: &glide_shell::image::Image, pts: Duration) -> Result<(), String>;
    fn stop(self: Box<Self>);
}

#[derive(Debug)]
pub struct ShareWaiter {
    pub stream: UnixStream,
    pub app_id: Option<String>,
    pub cursor: Option<bool>,
}

#[must_use]
pub fn filter_desktop_windows<T: PartialEq + Clone>(space_windows: &[T], whitelist: &[T]) -> Vec<T> {
    space_windows.iter().filter(|w| whitelist.contains(w)).cloned().collect()
}

#[must_use]
pub fn multi_window_slots(canvas: (u32, u32), window_sizes: &[(u32, u32)]) -> Vec<crate::shot::model::Area> {
    use smithay::utils::{Point, Rectangle, Size};

    let n = window_sizes.len();
    if n == 0 || canvas.0 == 0 || canvas.1 == 0 {
        return Vec::new();
    }
    let (canvas_w, canvas_h) = (f64::from(canvas.0), f64::from(canvas.1));
    if n == 1 {
        let (win_w, win_h) = (f64::from(window_sizes[0].0.max(1)), f64::from(window_sizes[0].1.max(1)));
        let scale = (canvas_w / win_w).min(canvas_h / win_h);
        let slot_w = (win_w * scale).round();
        let slot_h = (win_h * scale).round();
        let slot_x = ((canvas_w - slot_w) / 2.0).round();
        let slot_y = ((canvas_h - slot_h) / 2.0).round();
        return vec![Rectangle::new(Point::from((slot_x, slot_y)), Size::from((slot_w, slot_h)))];
    }

    let (cols, rows, pad) = if n == 2 {
        (2usize, 1usize, 16.0f64)
    } else if n <= 4 {
        (2usize, 2usize, 16.0f64)
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Window counts are small positive numbers
        let cols = (n as f64).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols);
        (cols, rows, 16.0f64)
    };

    let total_pad_w = pad * (cols as f64 + 1.0);
    let total_pad_h = pad * (rows as f64 + 1.0);
    let cell_w = ((canvas_w - total_pad_w) / cols as f64).max(1.0);
    let cell_h = ((canvas_h - total_pad_h) / rows as f64).max(1.0);

    let mut slots = Vec::with_capacity(n);
    for (i, &(w, h)) in window_sizes.iter().enumerate() {
        let c = i % cols;
        let r = i / cols;
        let cell_x = pad + c as f64 * (cell_w + pad);
        let cell_y = pad + r as f64 * (cell_h + pad);
        let (win_w, win_h) = (f64::from(w.max(1)), f64::from(h.max(1)));
        let scale = (cell_w / win_w).min(cell_h / win_h);
        let slot_w = (win_w * scale).round();
        let slot_h = (win_h * scale).round();
        let slot_x = cell_x + ((cell_w - slot_w) / 2.0).round();
        let slot_y = cell_y + ((cell_h - slot_h) / 2.0).round();
        slots.push(Rectangle::new(Point::from((slot_x, slot_y)), Size::from((slot_w, slot_h))));
    }
    slots
}

#[must_use]
pub fn composite_multi_windows(canvas: (u32, u32), frames: &[glide_shell::image::Image]) -> glide_shell::image::Image {
    let mut out = glide_shell::image::Image::new(canvas.0.max(1), canvas.1.max(1)).expect("canvas image creation");
    let Some(mut pixmap) = out.pixmap() else {
        return out;
    };
    pixmap.fill(tiny_skia::Color::from_rgba8(26, 26, 28, 255));

    let sizes: Vec<(u32, u32)> = frames.iter().map(|f| (f.width, f.height)).collect();
    let slots = multi_window_slots(canvas, &sizes);

    for (frame, slot) in frames.iter().zip(slots.iter()) {
        let Some(source) = tiny_skia::PixmapRef::from_bytes(&frame.data, frame.width, frame.height) else {
            continue;
        };
        let sx = (slot.size.w as f32) / (frame.width as f32);
        let sy = (slot.size.h as f32) / (frame.height as f32);
        let tx = slot.loc.x as f32;
        let ty = slot.loc.y as f32;
        let transform = tiny_skia::Transform::from_scale(sx, sy).post_translate(tx, ty);
        let paint =
            tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..tiny_skia::PixmapPaint::default() };
        pixmap.draw_pixmap(0, 0, source, &paint, transform, None);
    }
    out
}

pub struct Stream<W = GlideWindow> {
    pub id: u64,
    pub node_id: u32,
    pub target: ShareTarget<W>,
    pub options: ShareOptions,
    pub started: Instant,
    pub last_capture: Option<Instant>,
    pub last_frame: Option<Arc<glide_shell::image::Image>>,
    pub sink: Option<Box<dyn ShareSink>>,
}

impl<W> Stream<W> {
    #[must_use]
    pub fn new(id: u64, node_id: u32, target: ShareTarget<W>, options: ShareOptions, now: Instant) -> Self {
        Self { id, node_id, target, options, started: now, last_capture: None, last_frame: None, sink: None }
    }

    #[must_use]
    pub fn with_sink(mut self, sink: Box<dyn ShareSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    #[must_use]
    pub fn source_kind(&self) -> glide_ipc::ShareSourceKind {
        match &self.target {
            ShareTarget::Screen { .. } => glide_ipc::ShareSourceKind::Screen,
            ShareTarget::FilteredDesktop { .. } => glide_ipc::ShareSourceKind::FilteredDesktop,
            ShareTarget::Window { .. } => glide_ipc::ShareSourceKind::Window,
            ShareTarget::MultiWindow { .. } => glide_ipc::ShareSourceKind::MultiWindow,
        }
    }

    #[must_use]
    pub fn window_count(&self) -> usize {
        match &self.target {
            ShareTarget::Screen { .. } => 0,
            ShareTarget::Window { .. } => 1,
            ShareTarget::MultiWindow { windows, .. } | ShareTarget::FilteredDesktop { windows, .. } => windows.len(),
        }
    }

    #[must_use]
    pub fn info(&self, size: (u32, u32)) -> glide_ipc::ShareStreamInfo {
        glide_ipc::ShareStreamInfo {
            stream_id: self.id,
            node_id: self.node_id,
            source_kind: self.source_kind(),
            width: size.0,
            height: size.1,
            window_count: self.window_count(),
            app_id: self.options.app_id.clone(),
        }
    }

    #[must_use]
    pub fn output(&self) -> &Output {
        self.target.output()
    }

    #[must_use]
    pub fn needs_frame(&self, output: &Output, now: Instant) -> bool {
        if self.output() != output {
            return false;
        }
        if self.last_frame.is_none() {
            return true;
        }
        let interval = Duration::from_micros(1_000_000 / u64::from(self.options.framerate.max(1)));
        self.last_capture.is_none_or(|last| now.saturating_duration_since(last) >= interval)
    }

    pub fn accept_frame(&mut self, frame: Arc<glide_shell::image::Image>, now: Instant) {
        self.last_capture = Some(now);
        let pts = now.saturating_duration_since(self.started);
        if let Some(sink) = self.sink.as_mut()
            && let Err(err) = sink.push_frame(&frame, pts)
        {
            tracing::error!(%err, "screen share sink push_frame failed");
        }
        self.last_frame = Some(frame);
    }

    pub fn on_window_closed(&mut self, window: &W) -> bool
    where
        W: PartialEq,
    {
        match &mut self.target {
            ShareTarget::Window { window: target_window, .. } => target_window != window,
            ShareTarget::MultiWindow { windows, .. } => {
                windows.retain(|w| w != window);
                !windows.is_empty()
            }
            ShareTarget::FilteredDesktop { windows, .. } => {
                windows.retain(|w| w != window);
                true
            }
            ShareTarget::Screen { .. } => true,
        }
    }

    pub fn set_windows(&mut self, windows: Vec<W>) {
        match &mut self.target {
            ShareTarget::MultiWindow { windows: target_windows, .. }
            | ShareTarget::FilteredDesktop { windows: target_windows, .. } => {
                *target_windows = windows;
            }
            ShareTarget::Window { window: target_window, .. } => {
                if let Some(first) = windows.into_iter().next() {
                    *target_window = first;
                }
            }
            ShareTarget::Screen { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::output::{Mode, PhysicalProperties, Subpixel};
    use smithay::utils::{Point, Size};
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
        stopped: Arc<AtomicBool>,
    }

    impl ShareSink for MockSink {
        fn push_frame(&mut self, _image: &glide_shell::image::Image, _pts: Duration) -> Result<(), String> {
            self.frames.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn stop(self: Box<Self>) {
            self.stopped.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn filter_desktop_windows_preserves_space_z_order_while_filtering() {
        let space_windows = vec![10, 20, 30, 40, 50];
        let whitelist = vec![40, 20];
        let result = filter_desktop_windows(&space_windows, &whitelist);
        assert_eq!(result, vec![20, 40]);
    }

    #[test]
    fn multi_window_slots_one_window_centered_fit() {
        let canvas = (1920, 1080);
        let sizes = [(800, 600)];
        let slots = multi_window_slots(canvas, &sizes);
        assert_eq!(slots.len(), 1);
        let slot = slots[0];
        assert_eq!(slot.size, Size::from((1440.0, 1080.0)));
        assert_eq!(slot.loc, Point::from((240.0, 0.0)));
    }

    #[test]
    fn multi_window_slots_two_windows_side_by_side() {
        let canvas = (1920, 1080);
        let sizes = [(800, 600), (800, 600)];
        let slots = multi_window_slots(canvas, &sizes);
        assert_eq!(slots.len(), 2);
        assert!(slots[0].loc.x < slots[1].loc.x);
        assert_eq!(slots[0].loc.y, slots[1].loc.y);
        assert!(slots[0].loc.x + slots[0].size.w <= slots[1].loc.x);
    }

    #[test]
    fn multi_window_slots_three_and_four_windows_grid() {
        let canvas = (1920, 1080);
        let sizes3 = [(800, 600), (800, 600), (800, 600)];
        let slots3 = multi_window_slots(canvas, &sizes3);
        assert_eq!(slots3.len(), 3);
        assert!(slots3[0].loc.x < slots3[1].loc.x);
        assert!(slots3[0].loc.y < slots3[2].loc.y);

        let sizes4 = [(800, 600), (800, 600), (800, 600), (800, 600)];
        let slots4 = multi_window_slots(canvas, &sizes4);
        assert_eq!(slots4.len(), 4);
        assert_eq!(slots4[0].loc.y, slots4[1].loc.y);
        assert_eq!(slots4[2].loc.y, slots4[3].loc.y);
        assert!(slots4[0].loc.y < slots4[2].loc.y);
    }

    #[test]
    fn composite_multi_windows_produces_dark_canvas_with_draws() {
        let canvas = (400, 300);
        let frame1 = glide_shell::image::Image::from_bgra(100, 100, vec![255; 40000]).unwrap();
        let frame2 = glide_shell::image::Image::from_bgra(100, 100, vec![200; 40000]).unwrap();
        let composite = composite_multi_windows(canvas, &[frame1, frame2]);
        assert_eq!(composite.width, 400);
        assert_eq!(composite.height, 300);
        assert_eq!(composite.data.len(), 400 * 300 * 4);
        assert_eq!(composite.data[0..4], [26, 26, 28, 255]);
    }

    #[test]
    fn stream_lifecycle_accept_frame_and_needs_frame() {
        let output = make_output("DP-1");
        let other_output = make_output("HDMI-A-1");
        let options = ShareOptions { app_id: Some("test.app".into()), cursor: true, framerate: 30 };
        let now = Instant::now();
        let target: ShareTarget = ShareTarget::Screen { output: output.clone() };

        let frames = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let mock_sink = Box::new(MockSink { frames: Arc::clone(&frames), stopped: Arc::clone(&stopped) });

        let mut stream = Stream::new(1, 10, target, options, now).with_sink(mock_sink);

        assert_eq!(stream.source_kind(), glide_ipc::ShareSourceKind::Screen);
        assert_eq!(stream.window_count(), 0);
        assert_eq!(stream.output(), &output);
        assert!(!stream.needs_frame(&other_output, now));
        assert!(stream.needs_frame(&output, now));

        let frame = Arc::new(glide_shell::image::Image::new(100, 100).unwrap());
        stream.accept_frame(Arc::clone(&frame), now);

        assert_eq!(frames.load(Ordering::SeqCst), 1);
        assert!(stream.last_frame.is_some());

        let immediate = now + Duration::from_millis(10);
        assert!(!stream.needs_frame(&output, immediate));

        let later = now + Duration::from_millis(35);
        assert!(stream.needs_frame(&output, later));

        let info = stream.info((1920, 1080));
        assert_eq!(info.stream_id, 1);
        assert_eq!(info.node_id, 10);
        assert_eq!(info.source_kind, glide_ipc::ShareSourceKind::Screen);
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert_eq!(info.window_count, 0);
        assert_eq!(info.app_id.as_deref(), Some("test.app"));

        let sink = stream.sink.take().unwrap();
        sink.stop();
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn stream_on_window_closed_pruning_and_set_windows() {
        let output = make_output("DP-1");
        let options = ShareOptions { app_id: None, cursor: true, framerate: 30 };
        let now = Instant::now();

        let window_target = ShareTarget::Window { window: 100u32, output: output.clone() };
        let mut window_stream = Stream::new(1, 1, window_target, options.clone(), now);
        assert_eq!(window_stream.window_count(), 1);
        assert!(window_stream.on_window_closed(&999));
        assert!(!window_stream.on_window_closed(&100));

        let multi_target = ShareTarget::MultiWindow { windows: vec![10, 20, 30], output: output.clone() };
        let mut multi_stream = Stream::new(2, 2, multi_target, options.clone(), now);
        assert_eq!(multi_stream.window_count(), 3);
        assert!(multi_stream.on_window_closed(&20));
        assert_eq!(multi_stream.window_count(), 2);
        assert!(multi_stream.on_window_closed(&10));
        assert_eq!(multi_stream.window_count(), 1);
        assert!(!multi_stream.on_window_closed(&30));
        assert_eq!(multi_stream.window_count(), 0);

        let filtered_target = ShareTarget::FilteredDesktop { windows: vec![10, 20], output: output.clone() };
        let mut filtered_stream = Stream::new(3, 3, filtered_target, options.clone(), now);
        assert_eq!(filtered_stream.window_count(), 2);
        assert!(filtered_stream.on_window_closed(&10));
        assert_eq!(filtered_stream.window_count(), 1);
        assert!(filtered_stream.on_window_closed(&20));
        assert_eq!(filtered_stream.window_count(), 0);
        assert!(filtered_stream.on_window_closed(&999));

        let screen_target = ShareTarget::Screen { output };
        let mut screen_stream = Stream::new(4, 4, screen_target, options, now);
        assert_eq!(screen_stream.window_count(), 0);
        assert!(screen_stream.on_window_closed(&10));

        multi_stream.set_windows(vec![40, 50]);
        assert_eq!(multi_stream.window_count(), 2);

        filtered_stream.set_windows(vec![60]);
        assert_eq!(filtered_stream.window_count(), 1);

        window_stream.set_windows(vec![70]);
        assert_eq!(window_stream.window_count(), 1);
        assert!(!window_stream.on_window_closed(&70));
    }
}
