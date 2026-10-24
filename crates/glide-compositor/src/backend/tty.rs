use std::{collections::HashMap, path::PathBuf, time::Duration};

use smithay::backend::renderer::element::{RenderElementStates, default_primary_scanout_output_compare};
use smithay::desktop::utils::{
    OutputPresentationFeedback, surface_presentation_feedback_flags_from_states, surface_primary_scanout_output,
    update_surface_primary_scanout_output,
};

use anyhow::{Context, anyhow};
use smithay::{
    backend::{
        allocator::{
            Fourcc,
            dmabuf::Dmabuf,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmNode,
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
        },
        egl::{EGLContext, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            ImportDma,
            element::{
                AsRenderElements, Kind,
                memory::MemoryRenderBuffer,
                solid::{SolidColorBuffer, SolidColorRenderElement},
                surface::WaylandSurfaceRenderElement,
                utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
            },
            gles::GlesRenderer,
        },
        session::{Event as SessionEvent, Session, libseat::LibSeatSession, libseat::LibSeatSessionNotifier},
        udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu},
    },
    desktop::{
        Space,
        space::{SpaceRenderElements, space_render_elements},
    },
    input::pointer::{CursorIcon, CursorImageAttributes, CursorImageStatus},
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::{
            EventLoop, RegistrationToken,
            timer::{TimeoutAction, Timer},
        },
        drm::control::{ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::backend::GlobalId,
    },
    render_elements,
    utils::{Buffer, Clock, DeviceFd, IsAlive, Logical, Monotonic, Physical, Point, Rectangle, Scale, Transform},
    wayland::{
        compositor,
        dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
    },
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};

use crate::{
    GlideState,
    cursor::{Cursor, PointerElement, PointerRenderElement},
    window::{GlideWindow, GlideWindowRenderElement},
};

type GbmDrmOutputManager = DrmOutputManager<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;
type GbmDrmOutput = DrmOutput<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

const SUPPORTED_FORMATS: &[Fourcc] = &[Fourcc::Abgr2101010, Fourcc::Argb2101010, Fourcc::Abgr8888, Fourcc::Argb8888];

render_elements! {
    pub GlideRenderElements<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, GlideWindowRenderElement<GlesRenderer>>,
    Pointer=PointerRenderElement<GlesRenderer>,
    Overview=crate::overview::OverviewRenderElement<GlesRenderer>,
    Memory=smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement<GlesRenderer>,
    Snapshot=crate::snapshot::SnapshotRenderElement,
    Launcher=crate::launcher::LauncherRenderElement<GlesRenderer>,
    Switcher=crate::switcher::SwitcherRenderElement<GlesRenderer>,
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    LockBackdrop=SolidColorRenderElement,
    RelocatedMemory=RelocateRenderElement<smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement<GlesRenderer>>,
    RelocatedLauncher=RelocateRenderElement<crate::launcher::LauncherRenderElement<GlesRenderer>>,
    RelocatedSwitcher=RelocateRenderElement<crate::switcher::SwitcherRenderElement<GlesRenderer>>,
}

fn prepend(elements: &mut Vec<GlideRenderElements>, front: impl IntoIterator<Item = GlideRenderElements>) {
    elements.splice(0..0, front);
}

fn dock_thumbnail(
    renderer: &mut GlesRenderer, window: &crate::window::GlideWindow,
    (frame_width, live): (i32, crate::dock_preview::Live), scale: smithay::utils::Scale<f64>,
    origin: Point<i32, Logical>,
) -> Vec<GlideRenderElements> {
    if frame_width <= 0 || live.content.size.w <= 0 {
        return Vec::new();
    }
    let factor = f64::from(live.content.size.w) / f64::from(frame_width);
    let frame_origin = live.content.loc - origin;
    let buffer_offset = window.geometry().loc;
    let content_origin = Point::<i32, Logical>::from((
        frame_origin.x - buffer_offset.x,
        frame_origin.y + window.header_height() - buffer_offset.y,
    ));
    let real = content_origin.to_f64().to_physical(scale).to_i32_round();
    let frame_physical = frame_origin.to_f64().to_physical(scale).to_i32_round();
    let shape = crate::window::PreviewShape { radius: crate::dock_preview::THUMB_RADIUS, scale: factor };
    window
        .preview_render_elements(renderer, real, scale, live.alpha, shape)
        .into_iter()
        .map(|element| {
            let scaled = RescaleRenderElement::from_element(element, frame_physical, factor);
            let placed = RelocateRenderElement::from_element(scaled, (0, 0), Relocate::Relative);
            GlideRenderElements::Overview(crate::overview::OverviewRenderElement::Preview(placed))
        })
        .collect()
}

fn backdrop_element(
    renderer: &mut GlesRenderer, buffer: &MemoryRenderBuffer, alpha: f32, (phys_w, phys_h): (i32, i32),
    logical_size: (i32, i32),
) -> Option<GlideRenderElements> {
    let src = smithay::utils::Rectangle::from_size(smithay::utils::Size::<f64, Logical>::from((
        f64::from(phys_w),
        f64::from(phys_h),
    )));
    let size: smithay::utils::Size<i32, Logical> = logical_size.into();
    smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        (0.0, 0.0),
        buffer,
        Some(alpha),
        Some(src),
        Some(size),
        Kind::Unspecified,
    )
    .ok()
    .map(GlideRenderElements::Memory)
}

fn relocate<E: smithay::backend::renderer::element::Element>(
    element: E, offset: Point<i32, smithay::utils::Physical>,
) -> RelocateRenderElement<E> {
    RelocateRenderElement::from_element(element, offset, Relocate::Relative)
}

fn mode_candidates(modes: &[smithay::reexports::drm::control::Mode]) -> Vec<(i32, i32, i32, bool)> {
    modes
        .iter()
        .map(|mode| {
            let wl = WlMode::from(*mode);
            (wl.size.w, wl.size.h, wl.refresh, mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        })
        .collect()
}

fn switch_mode(
    surface: &mut SurfaceData, renderer: &mut GlesRenderer, drm_mode: smithay::reexports::drm::control::Mode,
) -> bool {
    let result = surface.drm_output.use_mode(
        drm_mode,
        renderer,
        &DrmOutputRenderElements::<GlesRenderer, GlideRenderElements>::default(),
    );
    if let Err(err) = result {
        tracing::warn!(output = %surface.output.name(), ?err, "failed to switch the display mode");
        return false;
    }
    surface.output.change_current_state(Some(WlMode::from(drm_mode)), None, None, None);
    surface.wallpaper_key = None;
    true
}

fn advertise_modes(output: &Output, modes: &[smithay::reexports::drm::control::Mode], preferred: usize) {
    for mode in modes {
        output.add_mode(WlMode::from(*mode));
    }
    if let Some(mode) = modes.get(preferred) {
        output.set_preferred(WlMode::from(*mode));
    }
}

fn layer_elements(
    renderer: &mut GlesRenderer, output: &Output, scale: Scale<f64>, upper: bool,
) -> Vec<GlideRenderElements> {
    use smithay::desktop::layer_map_for_output;
    use smithay::wayland::shell::wlr_layer::Layer;

    let map = layer_map_for_output(output);
    let wanted = if upper { [Layer::Overlay, Layer::Top] } else { [Layer::Bottom, Layer::Background] };

    let mut elements = Vec::new();
    for layer in wanted {
        for surface in map.layers_on(layer).rev() {
            let Some(geometry) = map.layer_geometry(surface) else {
                continue;
            };
            let location = geometry.loc.to_f64().to_physical(scale).to_i32_round();
            let surface_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                AsRenderElements::render_elements(surface, renderer, location, scale, 1.0);
            elements
                .extend(surface_elements.into_iter().map(SpaceRenderElements::Surface).map(GlideRenderElements::Space));
        }
    }
    elements
}

type WallpaperKey = (i32, i32, crate::config::WallpaperFit, u64, u64);

struct SurfaceData {
    output: Output,
    global: Option<GlobalId>,
    drm_output: GbmDrmOutput,
    modes: Vec<smithay::reexports::drm::control::Mode>,
    pending_repaint: Option<RegistrationToken>,
    redraw_queued: bool,
    switched_off: bool,
    wallpaper_buf: Option<MemoryRenderBuffer>,
    wallpaper_muted_buf: Option<MemoryRenderBuffer>,
    wallpaper_blur_buf: Option<MemoryRenderBuffer>,
    wallpaper_blur_rgba: Option<std::sync::Arc<Vec<u8>>>,
    wallpaper_key: Option<WallpaperKey>,
    lock_backdrop: SolidColorBuffer,
    screenshot_flash: SolidColorBuffer,
    power_scrim: SolidColorBuffer,
    power_blur: Option<(WallpaperKey, MemoryRenderBuffer)>,
}

pub(crate) struct DeviceData {
    drm_output_manager: GbmDrmOutputManager,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, SurfaceData>,
    pub(crate) gles_renderer: GlesRenderer,
    cursor: Cursor,
    cursor_buffers: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    pointer_element: PointerElement,
}

pub struct TtyData {
    session: LibSeatSession,
    primary_gpu: DrmNode,
    primary_gpu_path: PathBuf,
    pub(crate) device: Option<DeviceData>,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
}

impl TtyData {
    #[must_use]
    pub fn session_active(&self) -> bool {
        self.session.is_active()
    }

    pub fn new(session: LibSeatSession) -> anyhow::Result<Self> {
        let seat_name = session.seat();
        let primary_gpu_path = match primary_gpu(&seat_name).context("failed to enumerate the primary GPU via udev")? {
            Some(path) => path,
            None => all_gpus(&seat_name)
                .context("failed to enumerate GPUs via udev")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("no GPU found on seat {seat_name}"))?,
        };
        let primary_gpu = DrmNode::from_path(&primary_gpu_path)
            .with_context(|| format!("{} is not a DRM node", primary_gpu_path.display()))?;

        Ok(Self { session, primary_gpu, primary_gpu_path, device: None, dmabuf_state: None })
    }
}

impl TtyData {
    pub fn seat_name(&self) -> String {
        self.session.seat()
    }

    pub fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            tracing::error!(vt, "failed to switch vt: {}", err);
        }
    }
}

pub fn init_tty(
    event_loop: &mut EventLoop<GlideState>, state: &mut GlideState, notifier: LibSeatSessionNotifier,
) -> anyhow::Result<()> {
    let seat_name = state.backend_data.seat_name();
    let session = state.backend_data.session.clone();

    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.into());
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|()| anyhow!("failed to assign libinput seat {seat_name}"))?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    event_loop
        .handle()
        .insert_source(libinput_backend, move |mut event, (), state| {
            use smithay::backend::input::InputEvent;
            match &mut event {
                InputEvent::DeviceAdded { device } => {
                    state.configure_input_device(device);
                    state.input_devices.push(device.clone());
                    state.sync_keyboard_leds();
                }
                InputEvent::DeviceRemoved { device } => {
                    state.input_devices.retain(|known| known != device);
                }
                _ => {}
            }
            state.process_input_event(event);
        })
        .map_err(|e| e.error)
        .context("failed to register the libinput event source")?;

    event_loop
        .handle()
        .insert_source(notifier, move |event, &mut (), state| match event {
            SessionEvent::PauseSession => {
                tracing::info!("session paused");
                libinput_context.suspend();

                if let Some(device) = state.backend_data.device.as_mut() {
                    device.drm_output_manager.pause();
                }
            }
            SessionEvent::ActivateSession => {
                tracing::info!("session resumed");
                if let Err(err) = libinput_context.resume() {
                    tracing::error!("failed to resume libinput context: {:?}", err);
                }
                state.release_held_keys("session resumed");

                let Some(device) = state.backend_data.device.as_mut() else {
                    return;
                };
                if let Err(err) = device.drm_output_manager.lock().activate(false) {
                    tracing::error!("failed to activate drm device: {:?}", err);
                    return;
                }

                let crtcs: Vec<_> = device.surfaces.keys().copied().collect();
                for crtc in crtcs {
                    state.render_surface(crtc);
                }
            }
        })
        .map_err(|e| e.error)
        .context("failed to register the libseat session-event source")?;

    Ok(())
}

pub fn init_udev_monitor(event_loop: &mut EventLoop<GlideState>, state: &mut GlideState) -> anyhow::Result<()> {
    let seat_name = state.backend_data.seat_name();
    let primary_gpu = state.backend_data.primary_gpu;

    let udev_backend = UdevBackend::new(&seat_name).context("failed to initialize the udev device monitor")?;
    tracing::info!(seat = seat_name, "watching udev for monitor hot-plug");

    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, state| match event {
            UdevEvent::Added { device_id, path } => {
                tracing::info!(?device_id, ?path, "udev device added; ignored, Glide only drives the primary GPU");
            }
            UdevEvent::Changed { device_id } => match DrmNode::from_dev_id(device_id) {
                Ok(node) if node == primary_gpu => state.device_changed(),
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(?device_id, ?err, "udev change event on a non-DRM device");
                }
            },
            UdevEvent::Removed { device_id } => {
                if DrmNode::from_dev_id(device_id).ok() == Some(primary_gpu) {
                    tracing::error!("the primary GPU was removed; Glide cannot recover from this");
                }
            }
        })
        .map_err(|e| e.error)
        .context("failed to register the udev device-monitor event source")?;

    Ok(())
}

fn cursor_hotspot(status: &CursorImageStatus, named: Point<f64, Logical>) -> Point<f64, Logical> {
    match status {
        CursorImageStatus::Surface(surface) => compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<std::sync::Mutex<CursorImageAttributes>>()
                .map(|attrs| attrs.lock().expect("single-threaded event loop, this mutex is never poisoned").hotspot)
                .unwrap_or_default()
                .to_f64()
        }),
        CursorImageStatus::Named(_) => named,
        CursorImageStatus::Hidden => Point::default(),
    }
}

fn named_hotspot(image: &xcursor::parser::Image, scale: u32) -> Point<f64, Logical> {
    let scale = f64::from(scale.max(1));
    Point::from((f64::from(image.xhot) / scale, f64::from(image.yhot) / scale))
}

fn resolve_cursor_buffer(
    cursor: &mut Cursor, cursor_buffers: &mut Vec<(xcursor::parser::Image, MemoryRenderBuffer)>, icon: CursorIcon,
    scale: u32, elapsed: Duration,
) -> (MemoryRenderBuffer, Point<f64, Logical>) {
    let image = cursor.image(icon, scale, elapsed);
    let hotspot = named_hotspot(&image, scale);
    if let Some((_, buffer)) = cursor_buffers.iter().find(|(cached, _)| cached == &image) {
        return (buffer.clone(), hotspot);
    }

    let buffer = MemoryRenderBuffer::from_slice(
        &image.pixels_rgba,
        Fourcc::Argb8888,
        (image.width.cast_signed(), image.height.cast_signed()),
        scale.cast_signed().max(1),
        Transform::Normal,
        None,
    );
    cursor_buffers.push((image, buffer.clone()));
    (buffer, hotspot)
}

enum FrameOutcome {
    Queued(smithay::backend::renderer::element::RenderElementStates),
    NoDamage(smithay::backend::renderer::element::RenderElementStates),
    Failed,
}

fn take_presentation_feedback(
    output: &Output, space: &Space<GlideWindow>, render_element_states: &RenderElementStates,
) -> OutputPresentationFeedback {
    use smithay::desktop::layer_map_for_output;

    let mut feedback = OutputPresentationFeedback::new(output);

    for window in space.elements() {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(&mut feedback, surface_primary_scanout_output, |surface, _| {
                surface_presentation_feedback_flags_from_states(surface, None, render_element_states)
            });
        }
    }

    for layer_surface in layer_map_for_output(output).layers() {
        layer_surface.take_presentation_feedback(&mut feedback, surface_primary_scanout_output, |surface, _| {
            surface_presentation_feedback_flags_from_states(surface, None, render_element_states)
        });
    }

    feedback
}

fn present_feedback(
    output: &Output, mut feedback: OutputPresentationFeedback, metadata: Option<&DrmEventMetadata>,
    clock: &Clock<Monotonic>,
) {
    use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind;
    use smithay::wayland::presentation::Refresh;

    let hw_time = metadata.and_then(|metadata| match metadata.time {
        smithay::backend::drm::DrmEventTime::Monotonic(tp) => (!tp.is_zero()).then_some(tp),
        smithay::backend::drm::DrmEventTime::Realtime(_) => None,
    });
    let sequence = metadata.map_or(0, |metadata| metadata.sequence);

    let (time, flags): (Duration, Kind) = hw_time
        .map_or_else(|| (clock.now().into(), Kind::Vsync), |tp| (tp, Kind::Vsync | Kind::HwClock | Kind::HwCompletion));

    let refresh = output
        .current_mode()
        .map_or(Refresh::Unknown, |mode| Refresh::fixed(Duration::from_secs_f64(1_000.0 / f64::from(mode.refresh))));

    feedback.presented::<_, Monotonic>(time, refresh, u64::from(sequence), flags);
}

const OVERVIEW_WALLPAPER_ZOOM: f64 = 1.06;

const MIRROR_CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

const OVERVIEW_WALLPAPER_SATURATION: f32 = 0.9;

const BACKDROP_BLUR_SIGMA: f64 = 22.0;

const BACKDROP_SATURATION: f32 = 1.6;

fn backdrop_box_radius(fractional_scale: f64) -> i32 {
    glide_paint::css_blur_box_radius((BACKDROP_BLUR_SIGMA * 2.0 * fractional_scale) as f32)
}

fn push_wallpaper_elements(
    gles_renderer: &mut GlesRenderer, surface: &mut SurfaceData, buffer_size: (i32, i32), logical_size: (i32, i32),
    backdrop_t: f64, elements: &mut Vec<GlideRenderElements>,
) {
    use smithay::backend::renderer::element::{Kind, memory::MemoryRenderBufferRenderElement};

    let (buf_w, buf_h) = buffer_size;
    let (log_w, log_h) = logical_size;
    let zoom = 1.0 + (OVERVIEW_WALLPAPER_ZOOM - 1.0) * backdrop_t.clamp(0.0, 1.0);

    let zoomed_w = (f64::from(log_w) * zoom).round() as i32;
    let zoomed_h = (f64::from(log_h) * zoom).round() as i32;
    let origin = (f64::from(log_w - zoomed_w) / 2.0, f64::from(log_h - zoomed_h) / 2.0);
    let size: Option<smithay::utils::Size<i32, Logical>> = Some((zoomed_w, zoomed_h).into());
    let src = Some(smithay::utils::Rectangle::from_size(smithay::utils::Size::<f64, Logical>::from((
        f64::from(buf_w),
        f64::from(buf_h),
    ))));

    let muted_alpha = backdrop_t.clamp(0.0, 1.0) as f32;
    if muted_alpha > 0.0
        && let Some(buffer) = surface.wallpaper_muted_buf.as_mut()
        && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            gles_renderer,
            origin,
            buffer,
            Some(muted_alpha),
            src,
            size,
            Kind::Unspecified,
        )
    {
        elements.push(GlideRenderElements::Memory(element));
    }

    if let Some(buffer) = surface.wallpaper_buf.as_mut()
        && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
            gles_renderer,
            origin,
            buffer,
            Some(1.0),
            src,
            size,
            Kind::Unspecified,
        )
    {
        elements.push(GlideRenderElements::Memory(element));
    }
}

fn overview_elements(
    gles_renderer: &mut GlesRenderer, overview: &mut crate::overview::OverviewUi, icons: &mut crate::icon::IconLoader,
    scale: Scale<f64>, output: &Output, output_geometry: smithay::utils::Rectangle<i32, Logical>,
    lookup: impl FnMut(glide_layout::WindowId) -> Option<crate::overview::Tile>,
) -> Vec<GlideRenderElements> {
    let mut elements: Vec<GlideRenderElements> = Vec::new();

    elements.extend(layer_elements(gles_renderer, output, scale, true));

    let painter = crate::paint::current();
    let output_scale = output.current_scale().integer_scale();
    elements.extend(
        overview
            .render(gles_renderer, output_geometry, scale, output_scale, &painter, icons, lookup)
            .into_iter()
            .map(GlideRenderElements::Overview),
    );

    elements.extend(layer_elements(gles_renderer, output, scale, false));
    elements
}

fn build_render_elements(
    gles_renderer: &mut GlesRenderer, space: &Space<GlideWindow>, output: &Output,
) -> Option<Vec<GlideRenderElements>> {
    match space_render_elements(gles_renderer, [space], output, 1.0) {
        Ok(space_elements) => Some(space_elements.into_iter().map(GlideRenderElements::Space).collect()),
        Err(err) => {
            tracing::error!(?err, "failed to collect render elements");
            None
        }
    }
}

fn is_window_element(element: &GlideRenderElements) -> bool {
    matches!(element, GlideRenderElements::Space(SpaceRenderElements::Element(_)) | GlideRenderElements::Overview(_))
}

fn leading_layer_count(elements: &[GlideRenderElements]) -> usize {
    elements
        .iter()
        .take_while(|element| matches!(element, GlideRenderElements::Space(SpaceRenderElements::Surface(_))))
        .count()
}

fn above_windows_index(elements: &[GlideRenderElements]) -> usize {
    elements.iter().position(is_window_element).unwrap_or_else(|| leading_layer_count(elements))
}

fn below_windows_index(elements: &[GlideRenderElements]) -> usize {
    elements.iter().rposition(is_window_element).map_or_else(|| leading_layer_count(elements), |index| index + 1)
}

const CURSOR_EXTENT: i32 = 128;

fn pointer_on_output(
    pointer_location: Point<f64, Logical>, output_geometry: Rectangle<i32, Logical>,
) -> Option<Point<f64, Logical>> {
    let origin = pointer_location.to_i32_floor() - Point::from((CURSOR_EXTENT, CURSOR_EXTENT));
    let reach = Rectangle::new(origin, smithay::utils::Size::from((CURSOR_EXTENT * 2, CURSOR_EXTENT * 2)));
    reach.overlaps(output_geometry).then(|| pointer_location - output_geometry.loc.to_f64())
}

fn cursor_origin(
    pointer_location: Point<f64, Logical>, cursor_hotspot: Point<f64, Logical>, scale: Scale<f64>,
) -> Point<i32, Physical> {
    (pointer_location - cursor_hotspot).to_physical(scale).to_i32_round()
}

fn cursor_elements(
    gles_renderer: &mut GlesRenderer, pointer_element: &mut PointerElement, pointer_location: Point<f64, Logical>,
    cursor_hotspot: Point<f64, Logical>, scale: Scale<f64>,
) -> Vec<GlideRenderElements> {
    let position = cursor_origin(pointer_location, cursor_hotspot, scale);
    pointer_element
        .render_elements::<PointerRenderElement<GlesRenderer>>(gles_renderer, position, scale, 1.0)
        .into_iter()
        .map(GlideRenderElements::Pointer)
        .collect()
}

fn dnd_icon_elements(
    gles_renderer: &mut GlesRenderer, icon: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    pointer_location: Point<f64, Logical>, scale: Scale<f64>,
) -> Vec<GlideRenderElements> {
    let position = pointer_location.to_physical(scale).to_i32_round();
    smithay::backend::renderer::element::surface::render_elements_from_surface_tree(
        gles_renderer,
        icon,
        position,
        scale,
        1.0,
        Kind::Unspecified,
    )
    .into_iter()
    .map(GlideRenderElements::Surface)
    .collect()
}

impl GlideState {
    pub fn device_added(&mut self) -> anyhow::Result<()> {
        let path = self.backend_data.primary_gpu_path.clone();
        let node = self.backend_data.primary_gpu;

        tracing::info!(?path, ?node, "opening primary gpu");

        let fd = self
            .backend_data
            .session
            .open(&path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)
            .with_context(|| format!("failed to open {}", path.display()))?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        let (drm_device, drm_notifier) =
            DrmDevice::new(fd.clone(), true).context("failed to initialize the DRM device")?;
        let gbm_device = GbmDevice::new(fd).context("failed to initialize the GBM device")?;

        self.loop_handle
            .insert_source(drm_notifier, move |event, metadata, state| match event {
                DrmEvent::VBlank(crtc) => state.frame_finish(crtc, metadata),
                DrmEvent::Error(err) => tracing::error!("drm error: {:?}", err),
            })
            .context("failed to register the DRM event source")?;

        let egl_display = unsafe { EGLDisplay::new(gbm_device.clone()) }
            .context("failed to create an EGL display for the GBM device")?;
        let egl_context = EGLContext::new(&egl_display).context("failed to create an EGL context")?;
        let gles_renderer = unsafe { GlesRenderer::new(egl_context) }.context("failed to create the GLES renderer")?;

        let dmabuf_formats = gles_renderer.dmabuf_formats();
        let default_dmabuf_feedback = DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats.clone())
            .build()
            .context("failed to build the default dmabuf feedback")?;
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state
            .create_global_with_default_feedback::<GlideState>(&self.display_handle, &default_dmabuf_feedback);
        self.backend_data.dmabuf_state = Some((dmabuf_state, dmabuf_global));
        tracing::info!(formats = dmabuf_formats.indexset().len(), "dmabuf global created");

        let allocator = GbmAllocator::new(gbm_device.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
        let framebuffer_exporter = GbmFramebufferExporter::new(gbm_device.clone(), node.into());
        let render_formats = gles_renderer.egl_context().dmabuf_render_formats().iter().copied().collect::<Vec<_>>();

        let drm_output_manager = DrmOutputManager::new(
            drm_device,
            allocator,
            framebuffer_exporter,
            Some(gbm_device),
            SUPPORTED_FORMATS.iter().copied(),
            render_formats,
        );

        self.backend_data.device = Some(DeviceData {
            drm_output_manager,
            drm_scanner: DrmScanner::new(),
            surfaces: HashMap::new(),
            gles_renderer,
            cursor: Cursor::load(),
            cursor_buffers: Vec::new(),
            pointer_element: PointerElement::default(),
        });

        self.scan_connectors()
    }

    fn scan_connectors(&mut self) -> anyhow::Result<()> {
        let events: Vec<_> = {
            let device = self
                .backend_data
                .device
                .as_mut()
                .expect("device_added() always sets backend_data.device before calling this");
            device
                .drm_scanner
                .scan_connectors(device.drm_output_manager.device())
                .context("failed to scan DRM connectors")?
                .into_iter()
                .collect()
        };

        let mut last_error = None;
        let mut initialized = 0usize;
        for event in events {
            if let DrmScanEvent::Connected { connector, crtc: Some(crtc) } = event {
                tracing::info!(?crtc, interface = ?connector.interface(), "connector connected");
                match self.connector_connected(&connector, crtc) {
                    Ok(()) => initialized += 1,
                    Err(err) => {
                        tracing::error!(?err, interface = ?connector.interface(), "failed to initialize connector, skipping it");
                        last_error = Some(err);
                    }
                }
            }
        }

        match last_error {
            Some(err) if initialized == 0 => Err(err.context("no connected output could be initialized")),
            _ => Ok(()),
        }
    }

    fn device_changed(&mut self) {
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };

        let events: Vec<_> = match device.drm_scanner.scan_connectors(device.drm_output_manager.device()) {
            Ok(result) => result.into_iter().collect(),
            Err(err) => {
                tracing::warn!(?err, "failed to rescan DRM connectors");
                return;
            }
        };

        for event in events {
            match event {
                DrmScanEvent::Connected { connector, crtc: Some(crtc) } => {
                    tracing::info!(?crtc, interface = ?connector.interface(), "connector connected");
                    if let Err(err) = self.connector_connected(&connector, crtc) {
                        tracing::error!(?err, "failed to initialize newly connected connector");
                    }
                }
                DrmScanEvent::Disconnected { connector, crtc: Some(crtc) } => {
                    tracing::info!(?crtc, interface = ?connector.interface(), "connector disconnected");
                    self.connector_disconnected(crtc);
                }
                _ => {}
            }
        }
    }

    fn connector_disconnected(&mut self, crtc: crtc::Handle) {
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let Some(surface) = device.surfaces.remove(&crtc) else {
            return;
        };

        self.space.unmap_output(&surface.output);
        self.screencopy.fail_for(&surface.output);
        {
            let mut map = smithay::desktop::layer_map_for_output(&surface.output);
            let layers: Vec<_> = map.layers().cloned().collect();
            for layer in layers {
                layer.layer_surface().send_close();
                map.unmap_layer(&layer);
            }
        }
        if let Some(lock) = self.session_lock.as_mut() {
            lock.forget_output(&surface.output);
            lock.confirm_if_all_presented(self.space.outputs());
        }

        self.sync_layout_outputs();
        self.reconcile_space();
        self.session.lid_hidden.retain(|panel| panel != &surface.output);
        self.session.mirrors.retain(|mirror| mirror != &surface.output);
        if !self.apply_lid() {
            self.apply_output_configs();
        }

        let Some(global) = surface.global else {
            return;
        };
        let global_for_removal = global.clone();
        self.display_handle.disable_global::<GlideState>(global);
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(5)), move |_, (), state| {
            state.display_handle.remove_global::<GlideState>(global_for_removal.clone());
            TimeoutAction::Drop
        });
    }

    fn connector_connected(&mut self, connector: &connector::Info, crtc: crtc::Handle) -> anyhow::Result<()> {
        let output_name = format!("{}-{}", connector.interface().as_str(), connector.interface_id());
        let drm_modes = connector.modes().to_vec();
        let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));

        let display_info = self.backend_data.device.as_ref().and_then(|device| {
            smithay_drm_extras::display_info::for_connector(device.drm_output_manager.device(), connector.handle())
        });
        #[allow(clippy::redundant_closure_for_method_calls)]
        let (make, model, serial_number) = (
            display_info.as_ref().and_then(|info| info.make()).unwrap_or_else(|| "Unknown".into()),
            display_info.as_ref().and_then(|info| info.model()).unwrap_or_else(|| "Unknown".into()),
            display_info.as_ref().and_then(|info| info.serial()).unwrap_or_else(|| "Unknown".into()),
        );
        let identity = crate::config::OutputIdentity {
            name: output_name.clone(),
            make: make.clone(),
            model: model.clone(),
            serial: serial_number.clone(),
        };
        let (preferred_id, mode_id) = self.initial_mode(&identity, &drm_modes);
        let drm_mode = *drm_modes
            .get(mode_id)
            .ok_or_else(|| anyhow!("connector {:?} advertises no modes at all", connector.interface()))?;
        let wl_mode = WlMode::from(drm_mode);

        let output = Output::new(
            output_name,
            PhysicalProperties {
                size: (phys_w.cast_signed(), phys_h.cast_signed()).into(),
                subpixel: connector.subpixel().into(),
                make,
                model,
                serial_number,
            },
        );
        advertise_modes(&output, &drm_modes, preferred_id);
        output.change_current_state(Some(wl_mode), None, None, Some((0, 0).into()));

        let device = self
            .backend_data
            .device
            .as_mut()
            .expect("device_added() always sets backend_data.device before calling this");

        let drm_output = device
            .drm_output_manager
            .lock()
            .initialize_output::<_, GlideRenderElements>(
                crtc,
                drm_mode,
                &[connector.handle()],
                &output,
                None,
                &mut device.gles_renderer,
                &DrmOutputRenderElements::default(),
            )
            .map_err(|err| anyhow!("failed to initialize DRM output: {err}"))?;

        let global = output.create_global::<GlideState>(&self.display_handle);
        device.surfaces.insert(
            crtc,
            SurfaceData {
                output: output.clone(),
                global: Some(global),
                drm_output,
                modes: drm_modes,
                pending_repaint: None,
                redraw_queued: false,
                switched_off: false,
                wallpaper_buf: None,
                wallpaper_muted_buf: None,
                wallpaper_blur_buf: None,
                wallpaper_blur_rgba: None,
                wallpaper_key: None,
                lock_backdrop: SolidColorBuffer::default(),
                screenshot_flash: SolidColorBuffer::default(),
                power_scrim: SolidColorBuffer::default(),
                power_blur: None,
            },
        );

        self.space.map_output(&output, (0, 0));
        self.sync_layout_outputs();
        self.apply_output_configs();
        self.apply_lid();

        self.loop_handle.insert_idle(move |state| state.render_surface(crtc));

        Ok(())
    }

    fn frame_finish(&mut self, crtc: crtc::Handle, metadata: &mut Option<DrmEventMetadata>) {
        {
            let Some(device) = self.backend_data.device.as_mut() else {
                return;
            };
            let Some(surface) = device.surfaces.get_mut(&crtc) else {
                return;
            };
            let output = surface.output.clone();

            match surface.drm_output.frame_submitted() {
                Ok(user_data) => {
                    if let Some(feedback) = user_data.flatten() {
                        present_feedback(&output, feedback, metadata.as_ref(), &self.clock);
                    }
                }
                Err(err) => {
                    tracing::error!("failed to mark frame submitted: {:?}", err);
                }
            }

            if let Some(lock) = self.session_lock.as_mut() {
                lock.frame_presented(&output, self.space.outputs());
            }
        }

        self.render_surface(crtc);
    }

    #[must_use]
    pub fn gamma_size(&self, output: &Output) -> Option<u32> {
        use smithay::reexports::drm::control::Device as ControlDevice;
        let device = self.backend_data.device.as_ref()?;
        let crtc = self.crtc_for_output(output)?;
        let info = device.drm_output_manager.device().get_crtc(crtc).ok()?;
        Some(info.gamma_length())
    }

    fn crtc_for_output(&self, output: &Output) -> Option<crtc::Handle> {
        let device = self.backend_data.device.as_ref()?;
        device.surfaces.iter().find(|(_, surface)| &surface.output == output).map(|(crtc, _)| *crtc)
    }

    pub fn apply_gamma(&mut self, output: &Output, ramp: &[u16]) -> bool {
        use smithay::reexports::drm::control::Device as ControlDevice;
        let Some(crtc) = self.crtc_for_output(output) else {
            return false;
        };
        let Some(device) = self.backend_data.device.as_ref() else {
            return false;
        };
        let size = ramp.len() / 3;
        let (red, rest) = ramp.split_at(size);
        let (green, blue) = rest.split_at(size);
        match device.drm_output_manager.device().set_gamma(crtc, red, green, blue) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!(?err, "failed to set the gamma ramp");
                false
            }
        }
    }

    pub fn reset_gamma(&mut self, output: &Output) {
        let Some(size) = self.gamma_size(output).map(|size| size as usize) else {
            return;
        };
        if size < 2 {
            return;
        }
        let mut ramp = vec![0u16; size * 3];
        for index in 0..size {
            let value = ((index as f64 / (size as f64 - 1.0)) * f64::from(u16::MAX)).round() as u16;
            ramp[index] = value;
            ramp[size + index] = value;
            ramp[size * 2 + index] = value;
        }
        self.apply_gamma(output, &ramp);
    }

    fn initial_mode(
        &self, identity: &crate::config::OutputIdentity, modes: &[smithay::reexports::drm::control::Mode],
    ) -> (usize, usize) {
        let preferred = modes.iter().position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED)).unwrap_or(0);
        let chosen = crate::config::output_config(&self.output_configs, identity)
            .and_then(crate::config::OutputConfig::mode_spec)
            .and_then(|spec| spec.pick(&mode_candidates(modes)))
            .unwrap_or(preferred);
        (preferred, chosen)
    }

    pub fn apply_output_modes(&mut self) {
        let configs = self.output_configs.clone();
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let mut switched = Vec::new();
        for (crtc, surface) in &mut device.surfaces {
            if self.space.output_geometry(&surface.output).is_none() {
                continue;
            }
            let identity = crate::config::OutputIdentity::of(&surface.output);
            let candidates = mode_candidates(&surface.modes);
            let wanted = crate::config::output_config(&configs, &identity)
                .and_then(crate::config::OutputConfig::mode_spec)
                .and_then(|spec| spec.pick(&candidates))
                .or_else(|| candidates.iter().position(|mode| mode.3));
            let Some(drm_mode) = wanted.and_then(|index| surface.modes.get(index).copied()) else {
                continue;
            };
            if surface.output.current_mode() == Some(WlMode::from(drm_mode)) {
                continue;
            }
            if switch_mode(surface, &mut device.gles_renderer, drm_mode) {
                switched.push(*crtc);
            }
        }
        for crtc in switched {
            self.loop_handle.insert_idle(move |state| state.render_surface(crtc));
        }
    }

    pub fn set_output_mode(&mut self, output: &Output, mode: WlMode) -> bool {
        let Some(device) = self.backend_data.device.as_mut() else {
            return false;
        };
        let Some((&crtc, surface)) = device.surfaces.iter_mut().find(|(_, surface)| &surface.output == output) else {
            return false;
        };
        if surface.output.current_mode() == Some(mode) {
            return true;
        }
        let Some(drm_mode) = surface.modes.iter().copied().find(|candidate| WlMode::from(*candidate) == mode) else {
            return false;
        };
        if !switch_mode(surface, &mut device.gles_renderer, drm_mode) {
            return false;
        }
        self.loop_handle.insert_idle(move |state| state.render_surface(crtc));
        true
    }

    pub fn reload_cursor_theme(&mut self) {
        if let Some(device) = self.backend_data.device.as_mut() {
            device.cursor = Cursor::load();
            device.cursor_buffers.clear();
        }
        self.queue_redraw();
    }

    #[must_use]
    pub fn connected_outputs(&self) -> Vec<Output> {
        self.backend_data
            .device
            .as_ref()
            .map(|device| device.surfaces.values().map(|surface| surface.output.clone()).collect())
            .unwrap_or_default()
    }

    pub fn set_outputs_blanked(&mut self, blanked: bool) {
        if self.outputs_blanked == blanked {
            return;
        }
        self.outputs_blanked = blanked;
        self.protocols.wlr_output_power_state.broadcast_mode(blanked);

        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        if blanked {
            for surface in device.surfaces.values_mut() {
                if let Err(err) =
                    surface.drm_output.with_compositor(smithay::backend::drm::compositor::DrmCompositor::clear)
                {
                    tracing::error!(?err, "failed to blank an output");
                }
                if let Some(token) = surface.pending_repaint.take() {
                    self.loop_handle.remove(token);
                }
                surface.redraw_queued = false;
            }
        } else {
            let crtcs: Vec<crtc::Handle> = device.surfaces.keys().copied().collect();
            for crtc in crtcs {
                self.render_surface(crtc);
            }
        }
    }

    pub fn queue_redraw(&mut self) {
        if self.outputs_blanked {
            return;
        }
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let crtcs: Vec<crtc::Handle> = device
            .surfaces
            .iter_mut()
            .filter_map(|(&crtc, surface)| {
                if surface.redraw_queued {
                    None
                } else {
                    surface.redraw_queued = true;
                    Some(crtc)
                }
            })
            .collect();
        for crtc in crtcs {
            self.loop_handle.insert_idle(move |state| state.render_surface(crtc));
        }
    }

    pub fn queue_redraw_output(&mut self, output: &Output) {
        if self.outputs_blanked {
            return;
        }
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let crtc = device.surfaces.iter_mut().find_map(|(&crtc, surface)| {
            (surface.output == *output && !surface.redraw_queued).then(|| {
                surface.redraw_queued = true;
                crtc
            })
        });
        if let Some(crtc) = crtc {
            self.loop_handle.insert_idle(move |state| state.render_surface(crtc));
        }
    }

    #[profiling::function]
    fn render_surface(&mut self, crtc: crtc::Handle) {
        if !self.backend_data.session.is_active() || self.outputs_blanked {
            return;
        }

        self.reset_dead_cursor_surface();

        let pointer_location = self.seat.get_pointer().map(|pointer| pointer.current_location()).unwrap_or_default();
        let cursor_status = self.cursor_status.clone();
        let elapsed = self.start_time.elapsed();
        let dock_area = self.dock_area();
        let launcher_area = self.launcher_area();
        let fullscreen_center = self.focused_fullscreen_center();
        let topbar_popup_open = self.topbar.has_open_popup();
        let dock_suppressed = self.dock_suppressed();
        let overview_apps = self.overview_apps();

        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let DeviceData { surfaces, gles_renderer, cursor, cursor_buffers, pointer_element, .. } = device;
        let Some(surface) = surfaces.get_mut(&crtc) else {
            return;
        };
        surface.redraw_queued = false;

        if let Some(token) = surface.pending_repaint.take() {
            self.loop_handle.remove(token);
        }

        let target = surface.output.clone();
        let mirroring = self.session.mirrors.contains(&target);
        let output = if mirroring {
            self.space.outputs().next().cloned().unwrap_or_else(|| target.clone())
        } else {
            target.clone()
        };
        if self.space.output_geometry(&output).is_none() {
            if !surface.switched_off {
                if let Err(err) =
                    surface.drm_output.with_compositor(smithay::backend::drm::compositor::DrmCompositor::clear)
                {
                    tracing::warn!(output = %output.name(), ?err, "failed to switch off a disabled output");
                }
                self.screencopy.fail_for(&output);
                surface.switched_off = true;
            }
            return;
        }
        surface.switched_off = false;
        let fractional = output.current_scale().fractional_scale();
        let integer_scale = output.current_scale().integer_scale();
        let scale = Scale::from(fractional);
        let painter = crate::paint::current();

        let icon = match &cursor_status {
            CursorImageStatus::Named(icon) => *icon,
            _ => CursorIcon::Default,
        };
        let (cursor_buffer, named) =
            resolve_cursor_buffer(cursor, cursor_buffers, icon, integer_scale.cast_unsigned(), elapsed);
        let cursor_hotspot = cursor_hotspot(&cursor_status, named);
        pointer_element.set_buffer(cursor_buffer);
        pointer_element.set_status(cursor_status);

        let output_mode = output.current_mode().map_or((1920, 1080), |m| (m.size.w, m.size.h));
        let (phys_w, phys_h) = output_mode;
        let wallpaper_fit = self.wallpaper_fit;
        let wallpaper_key = (phys_w, phys_h, wallpaper_fit, glide_paint::wallpaper_generation(), fractional.to_bits());
        if surface.wallpaper_key != Some(wallpaper_key) {
            let mut buf = vec![0; usize::try_from(phys_w * phys_h * 4).unwrap_or_default()];
            glide_paint::draw_wallpaper_into(&mut buf, phys_w, phys_h, wallpaper_fit);
            let opaque_regions = wallpaper_fit
                .always_covers_the_output()
                .then(|| vec![Rectangle::<i32, Buffer>::from_size((phys_w, phys_h).into())]);
            let make_buffer = |bytes: &[u8]| {
                MemoryRenderBuffer::from_slice(
                    bytes,
                    Fourcc::Argb8888,
                    (phys_w, phys_h),
                    1,
                    Transform::Normal,
                    opaque_regions.clone(),
                )
            };
            surface.wallpaper_buf = Some(make_buffer(&buf));
            let mut blurred = buf.clone();
            glide_paint::box_blur_rgba(&mut blurred, phys_w, phys_h, backdrop_box_radius(fractional));
            glide_paint::saturate_in_place(&mut blurred, BACKDROP_SATURATION);
            surface.wallpaper_blur_buf = Some(make_buffer(&blurred));
            surface.wallpaper_blur_rgba = Some(std::sync::Arc::new(blurred));
            glide_paint::desaturate_in_place(&mut buf, OVERVIEW_WALLPAPER_SATURATION);
            surface.wallpaper_muted_buf = Some(make_buffer(&buf));
            surface.wallpaper_key = Some(wallpaper_key);
        }

        let logical_size =
            ((f64::from(phys_w) / fractional).round() as i32, (f64::from(phys_h) / fractional).round() as i32);
        let output_geometry =
            self.space.output_geometry(&output).unwrap_or_else(|| Rectangle::from_size(logical_size.into()));
        let primary = self.space.outputs().next() == Some(&output);
        let origin = output_geometry.loc.to_f64().to_physical(scale).to_i32_round::<i32>();
        let shell_offset = Point::<i32, smithay::utils::Physical>::from((-origin.x, -origin.y));
        let topbar_hidden = !topbar_popup_open && fullscreen_center.is_some_and(|c| output_geometry.contains(c));
        let dock_hidden_here = !primary || dock_suppressed;

        let backdrop_t = self.overview.as_ref().map_or(0.0, crate::overview::OverviewUi::backdrop_progress);

        let unlocking = self.greeter.as_ref().is_some_and(|greeter| greeter.is_lock() && greeter.is_leaving());
        let mut elements = Vec::new();
        let mut blur_patches: Vec<Rectangle<i32, Logical>> = Vec::new();
        if let Some(greeter) = self.greeter.as_mut().filter(|_| !unlocking) {
            let area = Rectangle::new(output_geometry.loc, logical_size.into());
            let drawn = greeter.render(gles_renderer, &painter, area, scale, integer_scale, primary);
            elements.extend(drawn.into_iter().map(GlideRenderElements::Memory));
            if let Some(buffer) = surface.wallpaper_blur_buf.as_ref() {
                elements.extend(backdrop_element(gles_renderer, buffer, 1.0, output_mode, logical_size));
            }
        } else if let Some(lock) = self.session_lock.as_ref() {
            if let Some(surface) = lock.surfaces.get(&output) {
                elements.extend(
                    smithay::backend::renderer::element::surface::render_elements_from_surface_tree(
                        gles_renderer,
                        surface.wl_surface(),
                        (0, 0),
                        scale,
                        1.0,
                        Kind::Unspecified,
                    )
                    .into_iter()
                    .map(GlideRenderElements::Surface),
                );
            }
            surface.lock_backdrop.update((logical_size.0, logical_size.1), [0.0, 0.0, 0.0, 1.0]);
            elements.push(GlideRenderElements::LockBackdrop(SolidColorRenderElement::from_buffer(
                &surface.lock_backdrop,
                (0, 0),
                scale,
                1.0,
                Kind::Unspecified,
            )));
        } else if let Some(overview) = self.overview.as_mut() {
            let window_map = &self.window_map;
            elements.extend(overview_elements(
                gles_renderer,
                overview,
                &mut self.launcher_icons,
                scale,
                &output,
                output_geometry,
                |id| {
                    window_map.get(&id).map(|window| crate::overview::Tile {
                        window: window.clone(),
                        title: window.display_name(),
                        app: overview_apps.get(&id).cloned(),
                    })
                },
            ));
        } else {
            let Some(space_elements) = build_render_elements(gles_renderer, &self.space, &output) else {
                return;
            };
            elements.extend(space_elements);
            blur_patches = crate::background_effect::blur_rects(&self.space, output_geometry);
            if let Some(preview) = self.snap_preview.as_mut()
                && output_geometry.overlaps(preview.rect)
            {
                use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
                let (appearance, accent) = self.shell_palette;
                let tokens = glide_theme::Tokens::new(appearance, accent);
                let position = (preview.rect.loc - output_geometry.loc).to_f64().to_physical(scale);
                let alpha = preview.alpha();
                if let Some(buffer) = preview.buffer(tokens.acc_soft, tokens.acc_bg, integer_scale)
                    && let Ok(element) = MemoryRenderBufferRenderElement::from_buffer(
                        gles_renderer,
                        position,
                        buffer,
                        Some(alpha),
                        None,
                        None,
                        Kind::Unspecified,
                    )
                {
                    let index = below_windows_index(&elements);
                    elements.insert(index, GlideRenderElements::Memory(element));
                }
            }
        }

        let locked = self.session_lock.is_some() || (self.greeter.is_some() && !unlocking);
        if self.snapshots.is_animating() && !locked {
            let snapshots = self.snapshots.render_elements(gles_renderer, scale, output_geometry);
            let index = above_windows_index(&elements);
            elements.splice(index..index, snapshots.into_iter().map(GlideRenderElements::Snapshot));
        }
        let launcher_openness = self.launcher.as_ref().map_or(0.0, crate::launcher::Launcher::backdrop_openness);
        if let Some(launcher) = self.launcher.as_mut().filter(|_| !locked && primary) {
            let grid = crate::launcher::render(
                launcher,
                gles_renderer,
                &mut self.launcher_icons,
                &painter,
                &self.launcher_theme,
                launcher_area,
                scale,
                integer_scale,
            );
            let mut ordered: Vec<GlideRenderElements> = grid
                .into_iter()
                .map(|element| GlideRenderElements::RelocatedLauncher(relocate(element, shell_offset)))
                .collect();
            if let Some(buffer) = surface.wallpaper_blur_buf.as_ref().filter(|_| launcher_openness > 0.0) {
                let alpha = launcher_openness.clamp(0.0, 1.0) as f32;
                ordered.extend(backdrop_element(gles_renderer, buffer, alpha, output_mode, logical_size));
            }
            prepend(&mut elements, ordered);
        }

        let blur_pixels = surface.wallpaper_blur_rgba.clone();
        let dock_backdrop = blur_pixels.as_ref().map(|pixels| crate::dock::Backdrop {
            pixels: pixels.as_slice(),
            width: phys_w,
            height: phys_h,
            fractional_scale: fractional,
            generation: glide_paint::wallpaper_generation(),
        });
        let dock_hidden = backdrop_t.max(launcher_openness).max(self.dock.autohide_level()).clamp(0.0, 1.0);
        let dock = if locked || dock_hidden_here {
            Vec::new()
        } else {
            crate::dock::render(
                &mut self.dock,
                gles_renderer,
                &painter,
                dock_area,
                scale,
                integer_scale,
                dock_hidden,
                dock_backdrop.as_ref(),
            )
        };
        prepend(
            &mut elements,
            dock.into_iter().map(|element| GlideRenderElements::RelocatedMemory(relocate(element, shell_offset))),
        );
        if !locked && !dock_hidden_here && dock_hidden <= 0.0 {
            let mut group = Vec::new();
            if let Some((preview, theme, layout, revision)) = self.dock.preview_parts(dock_area) {
                let drawn = crate::dock_preview::render(
                    preview,
                    gles_renderer,
                    &painter,
                    theme,
                    &layout,
                    (scale, integer_scale, revision),
                );
                let memory = |element| GlideRenderElements::RelocatedMemory(relocate(element, shell_offset));
                group.extend(drawn.top.into_iter().map(memory));
                for live in drawn.live {
                    let id = glide_layout::WindowId::from_u64(live.id);
                    let (Some(window), Some(state)) = (self.window_map.get(&id), self.layout.window_state(id)) else {
                        continue;
                    };
                    let frame = state.frame_geometry().size;
                    group.extend(dock_thumbnail(
                        gles_renderer,
                        window,
                        (frame.width, live),
                        scale,
                        output_geometry.loc,
                    ));
                }
                group.extend(drawn.base.into_iter().map(memory));
            }
            prepend(&mut elements, group);
        }

        let output_rect = Rectangle::from_size(logical_size.into());
        if !locked && (!topbar_hidden || self.topbar.floats()) {
            let topbar = crate::topbar::render(
                &mut self.topbar,
                gles_renderer,
                &painter,
                &mut self.dock.icons,
                output_rect,
                scale,
                integer_scale,
                (dock_backdrop.as_ref(), topbar_hidden),
            );
            prepend(&mut elements, topbar.into_iter().map(GlideRenderElements::Memory));
        }

        if let Some(switcher) = self.switcher.as_mut().filter(|_| !locked && primary) {
            let panel = crate::switcher::render(
                switcher,
                gles_renderer,
                &mut self.launcher_icons,
                &painter,
                &self.launcher_theme,
                launcher_area,
                scale,
                integer_scale,
                dock_backdrop.as_ref(),
            );
            prepend(
                &mut elements,
                panel
                    .into_iter()
                    .map(|element| GlideRenderElements::RelocatedSwitcher(relocate(element, shell_offset))),
            );
        }

        if let Some(dialog) = self.power_dialog.as_mut().filter(|_| !locked) {
            let openness = dialog.openness();
            let tokens = glide_theme::Tokens::new(glide_theme::Appearance::Dark, self.shell_palette.1);
            let content = crate::power_dialog::render(
                dialog,
                gles_renderer,
                &mut self.launcher_icons,
                &painter,
                self.launcher_theme.revision(),
                &crate::power_dialog::Palette::from_tokens(&tokens),
                output_rect,
                scale,
                integer_scale,
            );
            if openness > 0.0 {
                let mut ordered: Vec<GlideRenderElements> =
                    content.into_iter().map(GlideRenderElements::Switcher).collect();
                let alpha = openness.clamp(0.0, 1.0) as f32;
                surface.power_scrim.update(logical_size, crate::power_dialog::scrim_color());
                ordered.push(GlideRenderElements::LockBackdrop(SolidColorRenderElement::from_buffer(
                    &surface.power_scrim,
                    (0, 0),
                    scale,
                    alpha,
                    Kind::Unspecified,
                )));
                if surface.power_blur.as_ref().is_none_or(|(key, _)| Some(*key) != surface.wallpaper_key) {
                    surface.power_blur =
                        surface.wallpaper_key.zip(surface.wallpaper_blur_rgba.as_ref()).map(|(key, pixels)| {
                            let buffer = MemoryRenderBuffer::from_slice(
                                pixels,
                                Fourcc::Argb8888,
                                (phys_w, phys_h),
                                1,
                                Transform::Normal,
                                None,
                            );
                            (key, buffer)
                        });
                }
                if let Some((_, buffer)) = surface.power_blur.as_ref() {
                    ordered.extend(backdrop_element(gles_renderer, buffer, alpha, output_mode, logical_size));
                }
                prepend(&mut elements, ordered);
            }
        } else {
            surface.power_blur = None;
        }

        if let Some(session) = self.session.shot.as_mut().filter(|session| session.shows_on(&output)) {
            let tokens = glide_theme::Tokens::new(self.shell_palette.0, self.shell_palette.1);
            let palette = crate::shot::render::Palette::from_tokens(&tokens);
            let mut cache = std::mem::take(&mut session.cache);
            let pieces = session.scene().map_or_else(Vec::new, |scene| {
                crate::shot::render::render(
                    &mut cache,
                    &scene,
                    gles_renderer,
                    &painter,
                    &palette,
                    self.launcher_theme.revision(),
                    integer_scale,
                )
            });
            session.cache = cache;
            let overlay: Vec<GlideRenderElements> = pieces
                .into_iter()
                .map(|piece| match piece {
                    crate::shot::render::Piece::Memory(element) => GlideRenderElements::Memory(element),
                    crate::shot::render::Piece::Solid(element) => GlideRenderElements::LockBackdrop(element),
                })
                .collect();
            prepend(&mut elements, overlay);
        }

        let osd_alpha = self.session.osd.alpha(std::time::Instant::now());
        if osd_alpha > 0.0 && primary {
            let tokens = glide_theme::Tokens::new(self.shell_palette.0, self.shell_palette.1);
            let area = crate::osd::Osd::rect(output_rect);
            if let Some(buffer) = self.session.osd.buffer(&painter, &mut self.launcher_icons, &tokens, integer_scale)
                && let Ok(element) =
                    smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement::from_buffer(
                        gles_renderer,
                        area.loc.to_f64().to_physical(scale),
                        buffer,
                        Some(osd_alpha),
                        None,
                        None,
                        Kind::Unspecified,
                    )
            {
                prepend(&mut elements, std::iter::once(GlideRenderElements::Memory(element)));
            }
        }

        let local_pointer = pointer_on_output(pointer_location, output_geometry);
        let mut ordered = local_pointer.map_or_else(Vec::new, |local| {
            cursor_elements(gles_renderer, pointer_element, local, cursor_hotspot, scale)
        });
        let mut cursor_count = ordered.len();
        let icon_at = self.drag.touch_at.map_or(local_pointer, |at| pointer_on_output(at, output_geometry));
        if let Some(icon) = self.drag.icon.as_ref().filter(|icon| IsAlive::alive(*icon))
            && let Some(local) = icon_at
        {
            let corner = local + self.drag.icon_offset().to_f64();
            ordered.extend(dnd_icon_elements(gles_renderer, icon, corner, scale));
        }
        prepend(&mut elements, ordered);

        if let Some(greeter) = self.greeter.as_mut().filter(|_| unlocking) {
            let area = Rectangle::new(output_geometry.loc, logical_size.into());
            let drawn = greeter.render(gles_renderer, &painter, area, scale, integer_scale, primary);
            let mut cover: Vec<GlideRenderElements> = drawn.into_iter().map(GlideRenderElements::Memory).collect();
            if let Some(buffer) = surface.wallpaper_blur_buf.as_ref() {
                cover.extend(backdrop_element(
                    gles_renderer,
                    buffer,
                    greeter.cover_alpha() as f32,
                    output_mode,
                    logical_size,
                ));
            }
            elements.splice(cursor_count..cursor_count, cover);
        }

        if let Some(handoff) = self.handoff.as_mut() {
            let drawn = handoff.cover.render(
                gles_renderer,
                &painter,
                logical_size.into(),
                scale,
                integer_scale,
                handoff.alpha(),
            );
            let covered: Vec<GlideRenderElements> = drawn.into_iter().map(GlideRenderElements::Memory).collect();
            handoff.mark_frame();
            cursor_count += covered.len();
            prepend(&mut elements, covered);
        }

        if let Some(buffer) = surface.wallpaper_blur_buf.as_mut() {
            for rect in blur_patches {
                let local = Rectangle::new(rect.loc - output_geometry.loc, rect.size);
                let src = Rectangle::<f64, Logical>::new(
                    (f64::from(local.loc.x) * fractional, f64::from(local.loc.y) * fractional).into(),
                    (f64::from(local.size.w) * fractional, f64::from(local.size.h) * fractional).into(),
                );
                if let Ok(element) =
                    smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement::from_buffer(
                        gles_renderer,
                        local.loc.to_f64().to_physical(scale),
                        buffer,
                        Some(1.0),
                        Some(src),
                        Some(local.size),
                        Kind::Unspecified,
                    )
                {
                    elements.push(GlideRenderElements::Memory(element));
                }
            }
        }
        push_wallpaper_elements(gles_renderer, surface, (phys_w, phys_h), logical_size, backdrop_t, &mut elements);

        if mirroring {
            let target_mode = target.current_mode().map_or(output_mode, |mode| (mode.size.w, mode.size.h));
            let (factor, offset) = crate::mirror::fit(output_mode, target_mode);
            let copied: Vec<_> = elements
                .into_iter()
                .map(|element| relocate(RescaleRenderElement::from_element(element, (0, 0).into(), factor), offset))
                .collect();
            let failed =
                match surface.drm_output.render_frame(gles_renderer, &copied, MIRROR_CLEAR, FrameFlags::DEFAULT) {
                    Ok(result) if !result.is_empty => surface.drm_output.queue_frame(None).is_err(),
                    Ok(_) => false,
                    Err(err) => {
                        tracing::error!(output = %target.name(), ?err, "failed to render a mirrored frame");
                        true
                    }
                };
            if failed {
                self.schedule_repaint(crtc, &target);
            }
            return;
        }

        let masks = self.topbar.polkit_mask(output_rect);
        if !masks.is_empty() {
            let tokens = glide_theme::Tokens::new(self.shell_palette.0, self.shell_palette.1);
            let [r, g, b, a] = tokens.island_bg.0;
            let covers = masks.iter().map(|rect| {
                let geometry = rect.to_f64().to_physical(scale).to_i32_round();
                GlideRenderElements::LockBackdrop(SolidColorRenderElement::new(
                    smithay::backend::renderer::element::Id::new(),
                    geometry,
                    smithay::backend::renderer::utils::CommitCounter::default(),
                    [r * a, g * a, b * a, a],
                    Kind::Unspecified,
                ))
            });
            elements.splice(cursor_count..cursor_count, covers);
        }
        let transform = output.current_transform();
        let capture_frame = |renderer: &mut GlesRenderer, elements: &[GlideRenderElements]| {
            super::screenshot::capture_frame(renderer, elements, output_mode, fractional, transform)
        };
        if self.pending_screenshot.as_ref() == Some(&output) {
            self.pending_screenshot = None;
            match capture_frame(gles_renderer, &elements[cursor_count..]) {
                Ok(pixels) => {
                    if let Some(capture) = self.session.capture.take().filter(|capture| capture.output == output) {
                        self.loop_handle.insert_idle(move |state| {
                            state.deliver_capture(&capture, &pixels, output_mode, fractional);
                        });
                    } else if self.session.shot.as_ref().is_some_and(|session| session.awaiting_freeze(&output)) {
                        let frozen = output.clone();
                        self.loop_handle.insert_idle(move |state| {
                            state.shot_frozen(&frozen, pixels, output_mode, fractional);
                        });
                    } else {
                        self.screenshot_flash.start(
                            output.clone(),
                            std::time::Instant::now(),
                            glide_theme::motion::t2(),
                        );
                        crate::screenshot_ui::save_in_background(&self.loop_handle, pixels, output_mode);
                    }
                }
                Err(err) => tracing::error!(?err, "failed to capture screenshot"),
            }
        }

        if let Some(capture) = self.session.window_capture.take_if(|capture| capture.output == output) {
            let window = &capture.window;
            let geometry = window.geometry();
            let header = window.header_height();
            let frame = smithay::utils::Size::<i32, Logical>::from((geometry.size.w, geometry.size.h + header));
            let physical = frame.to_f64().to_physical(fractional).to_i32_ceil::<i32>();
            let location = Point::<i32, Logical>::from((-geometry.loc.x, header - geometry.loc.y))
                .to_f64()
                .to_physical(fractional)
                .to_i32_round();
            let shape = crate::window::PreviewShape { radius: capture.clip, scale: 1.0 };
            let alone = window.preview_render_elements(gles_renderer, location, scale, 1.0, shape);
            match super::screenshot::capture_elements(
                gles_renderer,
                &alone,
                (physical.w, physical.h),
                fractional,
                Transform::Normal,
                [0.0; 4],
            ) {
                Ok(pixels) => {
                    let size = (physical.w, physical.h);
                    self.loop_handle
                        .insert_idle(move |state| state.window_captured(&capture, pixels, size, fractional));
                }
                Err(err) => tracing::error!(?err, "failed to capture the window"),
            }
        }

        let captured = self.screencopy.has_pending_for(&output).then(|| capture_frame(gles_renderer, &elements));

        if self.session.cast.frame_due(&output) {
            match capture_frame(gles_renderer, &elements) {
                Ok(pixels) => self.session.cast.push_frame(pixels),
                Err(err) => tracing::warn!(?err, "could not capture a mirrored frame"),
            }
        }
        elements.drain(cursor_count..cursor_count + masks.len());

        let flash = self.screenshot_flash.alpha_on(&output, std::time::Instant::now());
        if flash > 0.0 {
            surface.screenshot_flash.update((logical_size.0, logical_size.1), [flash, flash, flash, flash]);
            elements.insert(
                cursor_count.min(elements.len()),
                GlideRenderElements::LockBackdrop(SolidColorRenderElement::from_buffer(
                    &surface.screenshot_flash,
                    (0, 0),
                    scale,
                    1.0,
                    Kind::Unspecified,
                )),
            );
        }

        let clear_color = self.background_color.0;

        let render_result = surface.drm_output.render_frame(gles_renderer, &elements, clear_color, FrameFlags::DEFAULT);

        let outcome = match render_result {
            Ok(res) if res.is_empty => FrameOutcome::NoDamage(res.states),
            Ok(res) => {
                let feedback = take_presentation_feedback(&output, &self.space, &res.states);
                match surface.drm_output.queue_frame(Some(feedback)) {
                    Ok(()) => FrameOutcome::Queued(res.states),
                    Err(err) => {
                        tracing::error!("failed to queue frame: {:?}", err);
                        FrameOutcome::Failed
                    }
                }
            }
            Err(err) => {
                tracing::error!("failed to render frame: {:?}", err);
                FrameOutcome::Failed
            }
        };

        if let FrameOutcome::Queued(_) = outcome
            && let Some(lock) = self.session_lock.as_mut()
        {
            lock.frame_queued(&output);
        }

        match captured {
            Some(Ok(pixels)) => {
                let presented = self.clock.now().into();
                self.serve_screencopy(&output, &pixels, output_mode.into(), presented);
            }
            Some(Err(err)) => {
                tracing::error!(?err, "screen capture failed");
                for capture in self.screencopy.take_for(&output) {
                    capture.frame.failed();
                }
            }
            None => {}
        }

        self.finish_frame(&outcome, crtc, &output);
    }

    fn finish_frame(&mut self, outcome: &FrameOutcome, crtc: crtc::Handle, output: &Output) {
        let states = match outcome {
            FrameOutcome::Queued(states) | FrameOutcome::NoDamage(states) => Some(states),
            FrameOutcome::Failed => None,
        };

        if let Some(states) = states {
            self.space.elements().for_each(|window| {
                window.with_surfaces(|s, surface_states| {
                    update_surface_primary_scanout_output(
                        s,
                        output,
                        surface_states,
                        None,
                        states,
                        default_primary_scanout_output_compare,
                    );
                });
            });
        }

        match outcome {
            FrameOutcome::Queued(_) | FrameOutcome::NoDamage(_) => {
                self.send_frame_callbacks(output);
            }
            FrameOutcome::Failed => {
                self.schedule_repaint(crtc, output);
            }
        }
    }

    fn send_frame_callbacks(&self, output: &Output) {
        use smithay::desktop::{layer_map_for_output, utils::send_frames_surface_tree};

        let now = self.start_time.elapsed();
        let throttle = Some(Duration::ZERO);
        self.space.elements_for_output(output).for_each(|window| {
            window.send_frame(output, now, throttle, |_, _| Some(output.clone()));
        });
        for layer in layer_map_for_output(output).layers() {
            layer.send_frame(output, now, throttle, |_, _| Some(output.clone()));
        }
        if let Some(lock_surface) = self.session_lock.as_ref().and_then(|lock| lock.surfaces.get(output)) {
            send_frames_surface_tree(lock_surface.wl_surface(), output, now, throttle, |_, _| Some(output.clone()));
        }
        if let CursorImageStatus::Surface(surface) = &self.cursor_status
            && surface.alive()
        {
            send_frames_surface_tree(surface, output, now, throttle, |_, _| Some(output.clone()));
        }
        if let Some(icon) = self.drag.icon.as_ref().filter(|icon| IsAlive::alive(*icon)) {
            send_frames_surface_tree(icon, output, now, throttle, |_, _| Some(output.clone()));
        }
    }

    pub fn queue_animation_redraw(&mut self) {
        if self.outputs_blanked {
            return;
        }
        let Some(device) = self.backend_data.device.as_ref() else {
            return;
        };
        let pending: Vec<(crtc::Handle, Output)> = device
            .surfaces
            .iter()
            .filter(|(_, surface)| surface.pending_repaint.is_none() && !surface.redraw_queued)
            .map(|(&crtc, surface)| (crtc, surface.output.clone()))
            .collect();
        for (crtc, output) in pending {
            self.schedule_repaint(crtc, &output);
        }
    }

    pub fn is_cursor_animating(&mut self) -> bool {
        if self.outputs_blanked {
            return false;
        }
        let icon = match &self.cursor_status {
            CursorImageStatus::Named(icon) => *icon,
            _ => return false,
        };
        let elapsed = self.start_time.elapsed();
        let scale = self
            .space
            .outputs()
            .map(|output| output.current_scale().integer_scale().cast_unsigned())
            .max()
            .unwrap_or(1);
        let Some(device) = self.backend_data.device.as_mut() else {
            return false;
        };
        device.cursor.time_to_next_frame(icon, scale, elapsed).is_some()
    }

    fn reset_dead_cursor_surface(&mut self) {
        if let CursorImageStatus::Surface(surface) = &self.cursor_status
            && !surface.alive()
        {
            self.cursor_status = CursorImageStatus::default_named();
        }
    }

    fn schedule_repaint(&mut self, crtc: crtc::Handle, output: &Output) {
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let Some(surface) = device.surfaces.get_mut(&crtc) else {
            return;
        };
        if surface.pending_repaint.is_some() {
            return;
        }

        let frame_duration = output
            .current_mode()
            .map_or(Duration::from_millis(16), |mode| Duration::from_secs_f64(1_000f64 / f64::from(mode.refresh)));

        let token = self
            .loop_handle
            .insert_source(Timer::from_duration(frame_duration), move |_, (), state| {
                if let Some(device) = state.backend_data.device.as_mut()
                    && let Some(surface) = device.surfaces.get_mut(&crtc)
                {
                    surface.pending_repaint = None;
                }
                state.render_surface(crtc);
                TimeoutAction::Drop
            })
            .ok();

        surface.pending_repaint = token;
    }
}

impl DmabufHandler for GlideState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self
            .backend_data
            .dmabuf_state
            .as_mut()
            .expect("device_added() always sets backend_data.dmabuf_state before calling this")
            .0
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        let Some(device) = self.backend_data.device.as_mut() else {
            notifier.failed();
            return;
        };

        match device.gles_renderer.import_dmabuf(&dmabuf, None) {
            Ok(_texture) => {
                let _ = notifier.successful::<GlideState>();
            }
            Err(err) => {
                tracing::warn!(?err, "failed to import client dmabuf");
                notifier.failed();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cursor_buffer_threads_the_requested_icon_through() {
        let mut cursor = Cursor::load();
        let mut cache = Vec::new();
        let _ = resolve_cursor_buffer(&mut cursor, &mut cache, CursorIcon::NwResize, 1, Duration::ZERO);
        assert_eq!(cache.len(), 1);
    }

    fn text_beam(scale: u32) -> xcursor::parser::Image {
        let size = 24 * scale;
        xcursor::parser::Image {
            size,
            width: size,
            height: size,
            xhot: 11 * scale,
            yhot: 12 * scale,
            delay: 0,
            pixels_rgba: Vec::new(),
            pixels_argb: Vec::new(),
        }
    }

    #[test]
    fn a_themed_cursor_hotspot_is_in_logical_pixels() {
        assert_eq!(named_hotspot(&text_beam(1), 1), Point::from((11.0, 12.0)));
        assert_eq!(named_hotspot(&text_beam(2), 2), Point::from((11.0, 12.0)));
        assert_eq!(named_hotspot(&text_beam(1), 0), Point::from((11.0, 12.0)));
    }

    #[test]
    fn a_named_cursor_puts_its_hotspot_on_the_pointer() {
        let hotspot = cursor_hotspot(&CursorImageStatus::Named(CursorIcon::Text), named_hotspot(&text_beam(1), 1));
        assert_eq!(cursor_origin(Point::from((100.0, 100.0)), hotspot, Scale::from(1.0)), Point::from((89, 88)));
        assert_eq!(cursor_origin(Point::from((100.0, 100.0)), hotspot, Scale::from(1.5)), Point::from((134, 132)));
    }

    #[test]
    fn a_hidden_cursor_has_no_hotspot() {
        assert_eq!(cursor_hotspot(&CursorImageStatus::Hidden, Point::from((11.0, 12.0))), Point::default());
    }

    #[test]
    fn pointer_is_drawn_in_output_local_coordinates() {
        let secondary = Rectangle::new(Point::from((1920, 0)), (2560, 1440).into());
        let local = pointer_on_output(Point::from((2000.0, 300.0)), secondary);
        assert_eq!(local, Some(Point::from((80.0, 300.0))));
    }

    #[test]
    fn pointer_is_skipped_on_outputs_it_is_far_from() {
        let primary = Rectangle::new(Point::from((0, 0)), (1920, 1080).into());
        assert_eq!(pointer_on_output(Point::from((3000.0, 500.0)), primary), None);
        assert!(pointer_on_output(Point::from((1930.0, 500.0)), primary).is_some());
    }

    #[test]
    fn resolve_cursor_buffer_reuses_the_cache_for_a_repeated_request() {
        let mut cursor = Cursor::load();
        let mut cache = Vec::new();
        let _ = resolve_cursor_buffer(&mut cursor, &mut cache, CursorIcon::Default, 1, Duration::ZERO);
        let _ = resolve_cursor_buffer(&mut cursor, &mut cache, CursorIcon::Default, 1, Duration::ZERO);
        assert_eq!(cache.len(), 1, "identical icon/scale/time must not grow the cache");
    }

    #[test]
    fn resolve_cursor_buffer_caches_separately_per_scale() {
        let mut cursor = Cursor::load();
        let mut cache = Vec::new();
        let _ = resolve_cursor_buffer(&mut cursor, &mut cache, CursorIcon::Default, 1, Duration::ZERO);
        let _ = resolve_cursor_buffer(&mut cursor, &mut cache, CursorIcon::Default, 2, Duration::ZERO);
        assert_eq!(cache.len(), 2);
    }
}
