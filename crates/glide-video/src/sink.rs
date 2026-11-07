use gstreamer as gst;
use gstreamer::glib::translate::from_glib_full;
use gstreamer::prelude::*;
use gstreamer_video::prelude::*;

#[link(name = "gstwayland-1.0")]
unsafe extern "C" {
    fn gst_wl_display_handle_context_new(display: *mut std::ffi::c_void) -> *mut gst::ffi::GstContext;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub display: usize,
    pub surface: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    #[must_use]
    pub fn sized(width: f32, height: f32) -> Self {
        Self { x: 0, y: 0, width: width.round().max(1.0) as i32, height: height.round().max(1.0) as i32 }
    }
}

pub fn wayland(target: Target, rect: Rect) -> Result<gst::Element, String> {
    let sink = gst::ElementFactory::make("waylandsink")
        .property("enable-last-sample", true)
        .build()
        .map_err(|err| format!("waylandsink: {err}"))?;
    // SAFETY: `target.display` is the live wl_display of the window the caller took it from; the call only wraps
    // the pointer in a new GstContext, which it returns with a full reference.
    let context: gst::Context = unsafe { from_glib_full(gst_wl_display_handle_context_new(target.display as *mut _)) };
    sink.set_context(&context);
    let overlay = sink.clone().dynamic_cast::<gstreamer_video::VideoOverlay>().map_err(|_| "no overlay")?;
    // SAFETY: `target.surface` is a live wl_surface on that display, owned by a `Surface` that outlives the sink.
    unsafe { overlay.set_window_handle(target.surface) };
    place(&sink, rect);
    Ok(sink)
}

pub fn place(sink: &gst::Element, rect: Rect) {
    if let Some(overlay) = sink.dynamic_cast_ref::<gstreamer_video::VideoOverlay>() {
        if let Err(err) = overlay.set_render_rectangle(rect.x, rect.y, rect.width, rect.height) {
            tracing::debug!("render rectangle: {err}");
        }
        overlay.expose();
    }
}
