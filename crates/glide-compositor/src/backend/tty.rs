//! The tty backend: DRM/KMS modesetting and the render loop.
//!
//! Glide owns the whole display pipeline here — there is no host compositor
//! to lean on, so this module is responsible for everything from opening the
//! GPU to putting pixels on screen.
//!
//! # Render loop
//!
//! Once [`GlideState::device_added`] has set up a [`SurfaceData`] per
//! connected output, repaints are driven by one of two paths:
//!
//! - **[`DrmEvent::VBlank`] → [`GlideState::frame_finish`] → [`GlideState::render_surface`]**:
//!   the normal path. A previous [`render_surface`](GlideState::render_surface)
//!   call found damage, submitted a frame via `queue_frame`, and this is the
//!   hardware confirming it was scanned out.
//! - **[`GlideState::schedule_repaint`] (a timer)**: used when a render finds
//!   *no* damage. `queue_frame` is skipped in that case, so no `VBlank` would
//!   ever arrive to drive the next attempt — the timer polls again after
//!   roughly one refresh interval instead.
//!
//! Both paths converge on the same `render_surface`, so there is exactly one
//! place that decides what ends up on screen.
//!
//! # Hot-plug
//!
//! Monitor hot-plug is driven by a [`UdevBackend`] event source watching the
//! primary GPU's device node: a [`UdevEvent::Changed`] means its connectors
//! may have changed, so [`GlideState::device_changed`] rescans them and
//! connects/disconnects outputs accordingly. Glide is single-GPU by design
//! (see [`DeviceData`]), so [`UdevEvent::Added`]/[`UdevEvent::Removed`] for
//! any *other* device are just logged and otherwise ignored.

use std::{
    collections::HashMap,
    path::PathBuf,
    time::Duration,
};

use smithay::backend::renderer::element::default_primary_scanout_output_compare;
use smithay::desktop::utils::update_surface_primary_scanout_output;

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
            element::{AsRenderElements, memory::MemoryRenderBuffer},
            gles::GlesRenderer,
        },
        session::{
            Event as SessionEvent, Session, libseat::LibSeatSession,
            libseat::LibSeatSessionNotifier,
        },
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
    utils::{DeviceFd, IsAlive, Logical, Point, Scale, Transform},
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

/// #295396, chosen specifically to be unmistakable from the frozen text
/// console (see the module-level docs on [`TtyData`]).
const FILL_COLOR: [f32; 4] = [0.160_784_3, 0.325_490_2, 0.588_235_3, 1.0];

type GbmDrmOutputManager = DrmOutputManager<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    (),
    DrmDeviceFd,
>;
type GbmDrmOutput =
    DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

const SUPPORTED_FORMATS: &[Fourcc] = &[
    Fourcc::Abgr2101010,
    Fourcc::Argb2101010,
    Fourcc::Abgr8888,
    Fourcc::Argb8888,
];

render_elements! {
    /// Everything that can end up on screen, in one type: `render_frame`
    /// takes a single slice, so window contents and the cursor have to share
    /// an element type.
    ///
    /// Deliberately tied to the concrete `GlesRenderer` (the `<=` form)
    /// rather than being generic over the renderer. `ImportAll` only exists
    /// as a blanket impl, so a generic `R: ImportAll` bound cannot be taken
    /// as a given — the compiler tries to re-derive it from
    /// `ImportMemWl + ImportDmaWl` at every nested use and fails. With one
    /// renderer there is nothing to generalise over anyway.
    pub GlideRenderElements<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, GlideWindowRenderElement<GlesRenderer>>,
    Pointer=PointerRenderElement<GlesRenderer>,
}

struct SurfaceData {
    output: Output,
    /// `None` only for the (currently unreachable) case of a connector wired
    /// up without a `wl_output` global; every real connector gets one in
    /// `connector_connected`. Used by `connector_disconnected` to retire the
    /// global when the monitor is unplugged.
    global: Option<GlobalId>,
    drm_output: GbmDrmOutput,
    /// Set while a "no damage last time, try again shortly" repaint timer is
    /// pending, so we never stack up more than one per surface.
    pending_repaint: Option<RegistrationToken>,
}

/// Everything tied to the single GPU device we drive. Single-GPU only:
/// there is no `GpuManager`/`MultiRenderer` here, and no support for
/// rendering to a second graphics card. Connector hot-plug *within* this
/// device (a monitor being plugged/unplugged) is handled — see the
/// module-level `# Hot-plug` docs.
struct DeviceData {
    drm_output_manager: GbmDrmOutputManager,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, SurfaceData>,
    gles_renderer: GlesRenderer,
    /// The default cursor, loaded from the system `XCursor` theme.
    cursor: Cursor,
    /// Uploaded cursor images, keyed by the source image. Animated cursors
    /// cycle through a handful of frames, and re-uploading the same pixels
    /// on every frame would be pure waste.
    cursor_buffers: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    pointer_element: PointerElement,
}

/// Backend data for running Glide directly on a DRM/KMS output from a tty,
/// without a host compositor.
///
/// Taking control of the session (via [`LibSeatSession::new`]) alone makes
/// logind put the tty into `KD_GRAPHICS`/`K_OFF`: the kernel stops drawing
/// the text console and stops handling `Ctrl+Alt+Fn` itself. Before this
/// backend does any rendering, that leaves the screen showing a frozen
/// snapshot of whatever was last drawn — not a crash, just an empty gap
/// between the outgoing and incoming renderers.
pub struct TtyData {
    session: LibSeatSession,
    primary_gpu: DrmNode,
    primary_gpu_path: PathBuf,
    device: Option<DeviceData>,
    /// `zwp_linux_dmabuf_v1` state. `None` until [`GlideState::device_added`]
    /// has a renderer to report supported formats from; every real dispatch
    /// happens well after that (see [`GlideState::dmabuf_state`]).
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
}

impl TtyData {
    /// Finds the primary GPU for the session's seat. Fails only if udev
    /// enumeration itself fails (e.g. `/sys` unavailable) or no GPU is
    /// present at all — both genuine environment problems, not bugs, so they
    /// propagate as errors rather than panicking.
    pub fn new(session: LibSeatSession) -> anyhow::Result<Self> {
        let seat_name = session.seat();
        let primary_gpu_path = match primary_gpu(&seat_name)
            .context("failed to enumerate the primary GPU via udev")?
        {
            Some(path) => path,
            None => all_gpus(&seat_name)
                .context("failed to enumerate GPUs via udev")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("no GPU found on seat {seat_name}"))?,
        };
        let primary_gpu = DrmNode::from_path(&primary_gpu_path)
            .with_context(|| format!("{} is not a DRM node", primary_gpu_path.display()))?;

        Ok(Self {
            session,
            primary_gpu,
            primary_gpu_path,
            device: None,
            dmabuf_state: None,
        })
    }
}

impl TtyData {
    /// Name of the seat this backend is bound to, as known to logind/seatd
    /// (e.g. `"seat0"`). Used both as the `wl_seat` name advertised to
    /// clients and to assign libinput devices to the correct physical seat.
    pub fn seat_name(&self) -> String {
        self.session.seat()
    }

    /// Switch to a different virtual terminal.
    ///
    /// Not optional in practice: taking control of the session puts the tty
    /// into `K_OFF`, so the kernel stops handling `Ctrl+Alt+Fn` itself. If
    /// this is never called, a running compositor leaves no way off the tty.
    pub fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            tracing::error!(vt, "failed to switch vt: {}", err);
        }
    }
}

/// Set up the libinput event source and session pause/resume handling for
/// the tty backend.
pub fn init_tty(
    event_loop: &mut EventLoop<GlideState>,
    state: &mut GlideState,
    notifier: LibSeatSessionNotifier,
) -> anyhow::Result<()> {
    let seat_name = state.backend_data.seat_name();
    let session = state.backend_data.session.clone();

    let mut libinput_context =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.into());
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|()| anyhow!("failed to assign libinput seat {seat_name}"))?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, (), state| {
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

                // Without this, an in-flight or newly-triggered render can
                // end up issuing a DRM ioctl on a device libseat has just
                // revoked, which blocks in the kernel rather than erroring
                // out — hanging the whole event loop (and the tty with it)
                // until something else forces the process to die. Verified
                // by reproducing exactly that hang before this fix existed.
                if let Some(device) = state.backend_data.device.as_mut() {
                    device.drm_output_manager.pause();
                }
            }
            SessionEvent::ActivateSession => {
                tracing::info!("session resumed");
                if let Err(err) = libinput_context.resume() {
                    tracing::error!("failed to resume libinput context: {:?}", err);
                }

                let Some(device) = state.backend_data.device.as_mut() else {
                    return;
                };
                if let Err(err) = device.drm_output_manager.lock().activate(false) {
                    tracing::error!("failed to activate drm device: {:?}", err);
                    return;
                }

                // Nothing has damage right after resuming, so nothing would
                // otherwise trigger a repaint: kick every surface manually.
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

/// Watches udev for monitor hot-plug on the primary GPU. See the
/// module-level `# Hot-plug` docs. Must be called after
/// [`GlideState::device_added`], since a `Changed` event can arrive at any
/// point afterwards and is handled by rescanning `backend_data.device`.
pub fn init_udev_monitor(
    event_loop: &mut EventLoop<GlideState>,
    state: &mut GlideState,
) -> anyhow::Result<()> {
    let seat_name = state.backend_data.seat_name();
    let primary_gpu = state.backend_data.primary_gpu;

    let udev_backend =
        UdevBackend::new(&seat_name).context("failed to initialize the udev device monitor")?;
    tracing::info!(seat = seat_name, "watching udev for monitor hot-plug");

    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, state| match event {
            UdevEvent::Added { device_id, path } => {
                // Glide is single-GPU by design (see `DeviceData`): a second
                // card appearing after startup has nowhere to render to and
                // is not driven.
                tracing::info!(
                    ?device_id,
                    ?path,
                    "udev device added; ignored, Glide only drives the primary GPU"
                );
            }
            UdevEvent::Changed { device_id } => {
                match DrmNode::from_dev_id(device_id) {
                    Ok(node) if node == primary_gpu => state.device_changed(),
                    // A `Changed` event on some other device (or on a node
                    // type of the primary GPU we don't track, e.g. its
                    // render node) has nothing for us to rescan.
                    Ok(_) => {}
                    Err(err) => {
                        tracing::warn!(?device_id, ?err, "udev change event on a non-DRM device");
                    }
                }
            }
            UdevEvent::Removed { device_id } => {
                if DrmNode::from_dev_id(device_id).ok() == Some(primary_gpu) {
                    // Glide has no fallback GPU to switch to and no code
                    // path to fully tear down and rebuild `DeviceData`, so
                    // this is effectively fatal. Logging (rather than
                    // panicking) at least gets the reason into the log file
                    // instead of a bare crash.
                    tracing::error!("the primary GPU was removed; Glide cannot recover from this");
                }
            }
        })
        .map_err(|e| e.error)
        .context("failed to register the udev device-monitor event source")?;

    Ok(())
}

/// A client-provided cursor surface carries a hotspot: the offset within the
/// image that is the actual "point" of the pointer. The image has to be
/// drawn shifted back by it. Named shapes (the themed default) have no
/// client-supplied hotspot, so they draw at `(0, 0)`.
fn cursor_hotspot(status: &CursorImageStatus) -> Point<i32, Logical> {
    match status {
        CursorImageStatus::Surface(surface) => compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<std::sync::Mutex<CursorImageAttributes>>()
                .map(|attrs| {
                    attrs
                        .lock()
                        .expect("single-threaded event loop, this mutex is never poisoned")
                        .hotspot
                })
                .unwrap_or_default()
        }),
        _ => (0, 0).into(),
    }
}

/// Renders (or reuses a cached render of) the themed cursor image for the
/// given icon shape, output scale and elapsed time. Animated cursors cycle
/// through a handful of frames, so `cursor_buffers` is checked before
/// uploading again — the cache keys on the source image, so switching
/// between shapes (default ↔ a resize arrow at a window edge) reuses each
/// shape's upload rather than churning.
fn resolve_cursor_buffer(
    cursor: &mut Cursor,
    cursor_buffers: &mut Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    icon: CursorIcon,
    scale: u32,
    elapsed: Duration,
) -> MemoryRenderBuffer {
    let image = cursor.image(icon, scale, elapsed);
    if let Some((_, buffer)) = cursor_buffers.iter().find(|(cached, _)| cached == &image) {
        return buffer.clone();
    }

    let buffer = MemoryRenderBuffer::from_slice(
        &image.pixels_rgba,
        Fourcc::Argb8888,
        (image.width.cast_signed(), image.height.cast_signed()),
        1,
        Transform::Normal,
        None,
    );
    cursor_buffers.push((image, buffer.clone()));
    buffer
}

/// Collects everything to draw for one frame: the cursor (always on top,
/// since `render_frame` wants elements front-to-back) followed by the
/// windows in `space`. Returns `None` if collecting the space's render
/// elements fails, having already logged why.
fn build_render_elements(
    gles_renderer: &mut GlesRenderer,
    space: &Space<GlideWindow>,
    pointer_element: &mut PointerElement,
    pointer_location: Point<f64, Logical>,
    cursor_hotspot: Point<i32, Logical>,
    scale: Scale<f64>,
    output: &Output,
) -> Option<Vec<GlideRenderElements>> {
    let mut elements: Vec<GlideRenderElements> = Vec::new();

    let cursor_position = (pointer_location - cursor_hotspot.to_f64())
        .to_physical(scale)
        .to_i32_round();
    elements.extend(
        pointer_element
            .render_elements::<PointerRenderElement<GlesRenderer>>(
                gles_renderer,
                cursor_position,
                scale,
                1.0,
            )
            .into_iter()
            .map(GlideRenderElements::Pointer),
    );

    match space_render_elements(gles_renderer, [space], output, 1.0) {
        Ok(space_elements) => {
            elements.extend(space_elements.into_iter().map(GlideRenderElements::Space));
            Some(elements)
        }
        Err(err) => {
            tracing::error!(?err, "failed to collect render elements");
            None
        }
    }
}

impl GlideState {
    /// Opens the primary GPU, sets up EGL/GLES rendering and DRM output
    /// management, and initializes whichever connectors are already
    /// plugged in. Must be called once, after [`init_tty`].
    ///
    /// # Errors
    ///
    /// Returns an error if the primary GPU cannot be opened, if EGL/GLES
    /// initialization fails, or if scanning/initializing connectors fails —
    /// all genuine environment problems (missing permissions, unsupported
    /// hardware) rather than bugs.
    pub fn device_added(&mut self) -> anyhow::Result<()> {
        let path = self.backend_data.primary_gpu_path.clone();
        let node = self.backend_data.primary_gpu;

        tracing::info!(?path, ?node, "opening primary gpu");

        let fd = self
            .backend_data
            .session
            .open(
                &path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .with_context(|| format!("failed to open {}", path.display()))?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        // `true` requests atomic modesetting, with an internal fallback to
        // the legacy KMS API if the driver does not support it.
        let (drm_device, drm_notifier) =
            DrmDevice::new(fd.clone(), true).context("failed to initialize the DRM device")?;
        let gbm_device = GbmDevice::new(fd).context("failed to initialize the GBM device")?;

        self.loop_handle
            .insert_source(drm_notifier, move |event, metadata, state| match event {
                DrmEvent::VBlank(crtc) => state.frame_finish(crtc, metadata),
                DrmEvent::Error(err) => tracing::error!("drm error: {:?}", err),
            })
            .context("failed to register the DRM event source")?;

        // Single GPU: the same device drives both KMS (modesetting) and
        // rendering, so there is no separate render node to juggle here.
        //
        // SAFETY: `EGLDisplay::new`'s contract is about *other* code also
        // calling `eglGetPlatformDisplay` on the same native display and
        // then tearing it down via `eglTerminate` behind smithay's back.
        // `gbm_device` is used exclusively through this smithay-owned
        // `EGLDisplay` for the remainder of the process, so that can't
        // happen here.
        let egl_display = unsafe { EGLDisplay::new(gbm_device.clone()) }
            .context("failed to create an EGL display for the GBM device")?;
        let egl_context =
            EGLContext::new(&egl_display).context("failed to create an EGL context")?;
        // SAFETY: `GlesRenderer::new`'s contract is that `egl_context` must
        // not be active on another thread. Glide is single-threaded (one
        // event loop, no rendering threads), and `egl_context` was just
        // created above and has not been shared anywhere yet.
        let gles_renderer = unsafe { GlesRenderer::new(egl_context) }
            .context("failed to create the GLES renderer")?;

        // Advertise `zwp_linux_dmabuf_v1` using whatever formats this
        // renderer can actually import as a texture, so a client asking for
        // the default feedback only ever hears about buffers we can use.
        let dmabuf_formats = gles_renderer.dmabuf_formats();
        let default_dmabuf_feedback =
            DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats.clone())
                .build()
                .context("failed to build the default dmabuf feedback")?;
        let mut dmabuf_state = DmabufState::new();
        let dmabuf_global = dmabuf_state.create_global_with_default_feedback::<GlideState>(
            &self.display_handle,
            &default_dmabuf_feedback,
        );
        self.backend_data.dmabuf_state = Some((dmabuf_state, dmabuf_global));
        tracing::info!(
            formats = dmabuf_formats.indexset().len(),
            "dmabuf global created"
        );

        let allocator = GbmAllocator::new(
            gbm_device.clone(),
            GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
        );
        let framebuffer_exporter = GbmFramebufferExporter::new(gbm_device.clone(), node.into());
        let render_formats = gles_renderer
            .egl_context()
            .dmabuf_render_formats()
            .iter()
            .copied()
            .collect::<Vec<_>>();

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

    /// Scans for connected connectors and initializes an output for each one
    /// found. Called once at startup for whatever is already plugged in;
    /// [`GlideState::device_changed`] is the hot-plug equivalent used for
    /// everything after that (see the module-level `# Hot-plug` docs).
    ///
    /// Unlike `device_changed`, a scan failure here is fatal: with no
    /// outputs yet, there is nothing for the compositor to show, so this
    /// propagates the error instead of silently starting on a black screen.
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

        for event in events {
            if let DrmScanEvent::Connected {
                connector,
                crtc: Some(crtc),
            } = event
            {
                tracing::info!(?crtc, interface = ?connector.interface(), "connector connected");
                self.connector_connected(&connector, crtc)?;
            }
        }

        Ok(())
    }

    /// Rescans the primary GPU's connectors in response to a udev `Changed`
    /// event, connecting/disconnecting outputs as needed. See the
    /// module-level `# Hot-plug` docs.
    ///
    /// Unlike the startup scan in [`GlideState::device_added`], a failure
    /// here is logged rather than propagated: there is already a running
    /// compositor with a (possibly empty) set of working outputs, and
    /// failing to notice one more monitor is not worth tearing that down
    /// for.
    fn device_changed(&mut self) {
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };

        let events: Vec<_> = match device
            .drm_scanner
            .scan_connectors(device.drm_output_manager.device())
        {
            Ok(result) => result.into_iter().collect(),
            Err(err) => {
                tracing::warn!(?err, "failed to rescan DRM connectors");
                return;
            }
        };

        for event in events {
            match event {
                DrmScanEvent::Connected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    tracing::info!(?crtc, interface = ?connector.interface(), "connector connected");
                    if let Err(err) = self.connector_connected(&connector, crtc) {
                        tracing::error!(?err, "failed to initialize newly connected connector");
                    }
                }
                DrmScanEvent::Disconnected {
                    connector,
                    crtc: Some(crtc),
                } => {
                    tracing::info!(?crtc, interface = ?connector.interface(), "connector disconnected");
                    self.connector_disconnected(crtc);
                }
                // Either the connector's mode list changed while it stayed
                // connected (M2 territory: re-evaluating an already-mapped
                // output's mode is a window-management concern), or the
                // scanner could not map a CRTC to it (nothing to drive it
                // with — a real possibility on GPUs with fewer CRTCs than
                // connectors).
                _ => {}
            }
        }
    }

    /// Tears down the output for a connector that just got unplugged:
    /// removes its `wl_output` global (giving clients a moment to notice
    /// before the backing state is freed — see [`DisplayHandle::disable_global`])
    /// and unmaps it from `space`, moving any windows that were on it back
    /// into view on whatever outputs remain.
    fn connector_disconnected(&mut self, crtc: crtc::Handle) {
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let Some(surface) = device.surfaces.remove(&crtc) else {
            return;
        };

        self.space.unmap_output(&surface.output);

        // The layout must hear about this before anything renders again:
        // windows that were on the departing monitor are now outside every
        // output, and would be unreachable if not pulled back into view.
        self.sync_layout_outputs();
        self.reconcile_space();

        let Some(global) = surface.global else {
            return;
        };
        let global_for_removal = global.clone();
        self.display_handle.disable_global::<GlideState>(global);
        // A few seconds is the delay the smithay docs suggest between
        // disabling and removing a global, so already-connected clients
        // have a chance to notice it going away before the state backing
        // it (and the protocol error a late bind would otherwise hit) is
        // freed.
        let _ = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_secs(5)),
            move |_, (), state| {
                state
                    .display_handle
                    .remove_global::<GlideState>(global_for_removal.clone());
                TimeoutAction::Drop
            },
        );
    }

    fn connector_connected(
        &mut self,
        connector: &connector::Info,
        crtc: crtc::Handle,
    ) -> anyhow::Result<()> {
        let mode_id = connector
            .modes()
            .iter()
            .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(0);
        let drm_mode = *connector.modes().get(mode_id).ok_or_else(|| {
            anyhow!(
                "connector {:?} advertises no modes at all",
                connector.interface()
            )
        })?;
        let wl_mode = WlMode::from(drm_mode);

        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );
        let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));

        let output = Output::new(
            output_name,
            PhysicalProperties {
                size: (phys_w.cast_signed(), phys_h.cast_signed()).into(),
                subpixel: connector.subpixel().into(),
                make: "Unknown".into(),
                model: "Unknown".into(),
                serial_number: "Unknown".into(),
            },
        );
        let global = output.create_global::<GlideState>(&self.display_handle);
        output.set_preferred(wl_mode);
        output.change_current_state(Some(wl_mode), None, None, Some((0, 0).into()));
        self.space.map_output(&output, (0, 0));

        self.sync_layout_outputs();

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

        device.surfaces.insert(
            crtc,
            SurfaceData {
                output,
                global: Some(global),
                drm_output,
                pending_repaint: None,
            },
        );

        // Kick off the render loop for this surface; every following repaint
        // is driven by `frame_finish` in response to a VBlank event.
        self.loop_handle
            .insert_idle(move |state| state.render_surface(crtc));

        Ok(())
    }

    fn frame_finish(&mut self, crtc: crtc::Handle, _metadata: &mut Option<DrmEventMetadata>) {
        {
            let Some(device) = self.backend_data.device.as_mut() else {
                return;
            };
            let Some(surface) = device.surfaces.get_mut(&crtc) else {
                return;
            };
            if let Err(err) = surface.drm_output.frame_submitted() {
                tracing::error!("failed to mark frame submitted: {:?}", err);
            }
        }

        self.render_surface(crtc);
    }

    fn render_surface(&mut self, crtc: crtc::Handle) {
        // While the session is paused (we're switched away to another VT)
        // the DRM device is revoked, so every render would fail with
        // `DeviceInactive`. Bailing out *without* rescheduling matters:
        // the error path below re-arms the repaint timer, which would spin
        // at the refresh rate for the entire time the user is on another
        // VT. `ActivateSession` re-triggers rendering on the way back, so
        // nothing is lost by stopping here.
        if !self.backend_data.session.is_active() {
            return;
        }

        self.reset_dead_cursor_surface();

        let pointer_location = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
            .unwrap_or_default();
        let cursor_hotspot = cursor_hotspot(&self.cursor_status);
        let cursor_status = self.cursor_status.clone();
        let elapsed = self.start_time.elapsed();

        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        // Split the borrow across fields up front: `render_frame` below
        // needs `&mut gles_renderer` at the same time as `surface` (itself
        // borrowed from `surfaces`), and those are disjoint fields of the
        // same `DeviceData`.
        let DeviceData {
            surfaces,
            gles_renderer,
            cursor,
            cursor_buffers,
            pointer_element,
            ..
        } = device;
        let Some(surface) = surfaces.get_mut(&crtc) else {
            return;
        };

        // We're rendering right now, so a "no damage last time" retry timer
        // would be redundant — and if this render also finds no damage,
        // it gets re-armed via `schedule_repaint` below anyway. Clearing it
        // first also means the timer callback (which is what usually calls
        // us) never re-enters itself here.
        if let Some(token) = surface.pending_repaint.take() {
            self.loop_handle.remove(token);
        }

        let output = surface.output.clone();
        let scale = Scale::from(output.current_scale().fractional_scale());

        // Which themed shape to draw when no client surface overrides it.
        // Set both by clients (via cursor-shape-v1) and by our own
        // decorations (resize arrows at window borders).
        let icon = match &cursor_status {
            CursorImageStatus::Named(icon) => *icon,
            _ => CursorIcon::Default,
        };
        let cursor_buffer = resolve_cursor_buffer(
            cursor,
            cursor_buffers,
            icon,
            output.current_scale().integer_scale().cast_unsigned(),
            elapsed,
        );
        pointer_element.set_buffer(cursor_buffer);
        pointer_element.set_status(cursor_status);

        let Some(elements) = build_render_elements(
            gles_renderer,
            &self.space,
            pointer_element,
            pointer_location,
            cursor_hotspot,
            scale,
            &output,
        ) else {
            return;
        };

        let clear_color = if self.layout.overview_active {
            [0.1, 0.1, 0.1, 1.0] // Dark gray
        } else {
            FILL_COLOR
        };

        let render_result = surface.drm_output.render_frame(
            gles_renderer,
            &elements,
            clear_color,
            FrameFlags::DEFAULT,
        );

        match render_result {
            Ok(res) => {
                if !res.is_empty {
                    if let Err(err) = surface.drm_output.queue_frame(()) {
                        tracing::error!("failed to queue frame: {:?}", err);
                        self.schedule_repaint(crtc, &output);
                        return;
                    }
                }

                self.space.elements().for_each(|window| {
                    window.with_surfaces(|s, states| {
                        update_surface_primary_scanout_output(
                            s,
                            &output,
                            states,
                            None,
                            &res.states,
                            default_primary_scanout_output_compare,
                        );
                    });
                });

                if !res.is_empty {
                    self.space.elements_for_output(&output).for_each(|window| {
                        window.send_frame(
                            &output,
                            self.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        );
                    });
                } else {
                    self.schedule_repaint(crtc, &output);
                }
            }
            Err(err) => {
                tracing::error!("failed to render frame: {:?}", err);
                self.schedule_repaint(crtc, &output);
            }
        }
    }

    /// A client can hand us a cursor surface and then die before we next
    /// render. Falls back to the themed cursor rather than referencing a
    /// dead surface.
    fn reset_dead_cursor_surface(&mut self) {
        if let CursorImageStatus::Surface(surface) = &self.cursor_status
            && !surface.alive()
        {
            self.cursor_status = CursorImageStatus::default_named();
        }
    }

    /// Arms a one-shot timer to retry rendering `crtc` after approximately
    /// one refresh interval, unless one is already pending. Used when a
    /// render finds no damage (and so never calls `queue_frame`, meaning no
    /// `VBlank` will arrive to drive the next attempt) or hits a transient
    /// error.
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
            .map_or(Duration::from_millis(16), |mode| {
                Duration::from_secs_f64(1_000f64 / f64::from(mode.refresh))
            });

        let token = self
            .loop_handle
            .insert_source(Timer::from_duration(frame_duration), move |_, (), state| {
                if let Some(device) = state.backend_data.device.as_mut() {
                    if let Some(surface) = device.surfaces.get_mut(&crtc) {
                        surface.pending_repaint = None;
                    }
                }
                state.render_surface(crtc);
                TimeoutAction::Drop
            })
            .ok();

        surface.pending_repaint = token;
    }
}

impl DmabufHandler for GlideState {
    /// # Panics
    ///
    /// Panics if called before [`GlideState::device_added`] has run.
    /// `device_added` is the only place that creates the dmabuf global this
    /// state backs, so no client can have bound it (and so no dispatch can
    /// reach this) any earlier.
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self
            .backend_data
            .dmabuf_state
            .as_mut()
            .expect("device_added() always sets backend_data.dmabuf_state before calling this")
            .0
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let Some(device) = self.backend_data.device.as_mut() else {
            notifier.failed();
            return;
        };

        match device.gles_renderer.import_dmabuf(&dmabuf, None) {
            Ok(_texture) => {
                // A failure here just means the client already hung up
                // before we could reply; nothing for us to do about that.
                let _ = notifier.successful::<GlideState>();
            }
            Err(err) => {
                tracing::warn!(?err, "failed to import client dmabuf");
                notifier.failed();
            }
        }
    }
}
