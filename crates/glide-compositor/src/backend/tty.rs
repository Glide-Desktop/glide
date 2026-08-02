use std::{collections::HashMap, path::PathBuf};

use smithay::{
    backend::{
        allocator::{
            Fourcc,
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
        renderer::{element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer},
        session::{
            Event as SessionEvent, Session, libseat::LibSeatSession,
            libseat::LibSeatSessionNotifier,
        },
        udev::{all_gpus, primary_gpu},
    },
    output::{Mode as WlMode, Output, PhysicalProperties},
    reexports::{
        calloop::EventLoop,
        drm::control::{ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
        wayland_server::backend::GlobalId,
    },
    utils::DeviceFd,
};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};

use crate::{GlideState, state::Backend};

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

struct SurfaceData {
    #[allow(dead_code)] // kept alive for its Drop impl / future use (hotplug, M1.4)
    output: Output,
    #[allow(dead_code)]
    global: Option<GlobalId>,
    drm_output: GbmDrmOutput,
}

/// Everything tied to the single GPU device we drive. Single-GPU only: there
/// is no `GpuManager`/`MultiRenderer` here, and no hot-plug handling for
/// additional graphics cards or connectors yet (M1.4).
struct DeviceData {
    drm_output_manager: GbmDrmOutputManager,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, SurfaceData>,
    gles_renderer: GlesRenderer,
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
}

impl TtyData {
    pub fn new(session: LibSeatSession) -> Self {
        let seat_name = session.seat();
        let primary_gpu_path = primary_gpu(&seat_name)
            .unwrap()
            .or_else(|| all_gpus(&seat_name).unwrap().into_iter().next())
            .expect("no GPU found on this seat");
        let primary_gpu =
            DrmNode::from_path(&primary_gpu_path).expect("primary GPU path is not a DRM node");

        Self {
            session,
            primary_gpu,
            primary_gpu_path,
            device: None,
        }
    }
}

impl Backend for TtyData {
    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn change_vt(&mut self, vt: i32) {
        if let Err(err) = self.session.change_vt(vt) {
            tracing::error!(vt, "failed to switch vt: {}", err);
        }
    }
}

/// Set up the libinput event source and session pause/resume handling for
/// the tty backend.
pub fn init_tty(
    event_loop: &mut EventLoop<GlideState<TtyData>>,
    state: &mut GlideState<TtyData>,
    notifier: LibSeatSessionNotifier,
) -> Result<(), Box<dyn std::error::Error>> {
    let seat_name = state.backend_data.seat_name();
    let session = state.backend_data.session.clone();

    let mut libinput_context =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.into());
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|()| "failed to assign libinput seat")?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, state| {
            state.process_input_event(event);
        })?;

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
        })?;

    Ok(())
}

impl GlideState<TtyData> {
    /// Opens the primary GPU, sets up EGL/GLES rendering and DRM output
    /// management, and initializes whichever connectors are already
    /// plugged in. Must be called once, after [`init_tty`].
    pub fn device_added(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.backend_data.primary_gpu_path.clone();
        let node = self.backend_data.primary_gpu;

        tracing::info!(?path, ?node, "opening primary gpu");

        let fd = self.backend_data.session.open(
            &path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));

        // `true` requests atomic modesetting, with an internal fallback to
        // the legacy KMS API if the driver does not support it.
        let (drm_device, drm_notifier) = DrmDevice::new(fd.clone(), true)?;
        let gbm_device = GbmDevice::new(fd)?;

        self.loop_handle.insert_source(
            drm_notifier,
            move |event, metadata, state| match event {
                DrmEvent::VBlank(crtc) => state.frame_finish(crtc, metadata),
                DrmEvent::Error(err) => tracing::error!("drm error: {:?}", err),
            },
        )?;

        // Single GPU: the same device drives both KMS (modesetting) and
        // rendering, so there is no separate render node to juggle here.
        let egl_display = unsafe { EGLDisplay::new(gbm_device.clone())? };
        let egl_context = EGLContext::new(&egl_display)?;
        let gles_renderer = unsafe { GlesRenderer::new(egl_context)? };

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
        });

        self.scan_connectors()
    }

    /// Scans for connected connectors and initializes an output for each one
    /// found. Only handles the initial state at startup; reacting to
    /// connectors being plugged/unplugged afterwards is M1.4.
    fn scan_connectors(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let events: Vec<_> = {
            let device = self
                .backend_data
                .device
                .as_mut()
                .ok_or("device not initialized")?;
            device
                .drm_scanner
                .scan_connectors(device.drm_output_manager.device())?
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
                self.connector_connected(connector, crtc)?;
            }
        }

        Ok(())
    }

    fn connector_connected(
        &mut self,
        connector: connector::Info,
        crtc: crtc::Handle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mode_id = connector
            .modes()
            .iter()
            .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(0);
        let drm_mode = *connector
            .modes()
            .get(mode_id)
            .ok_or("connector has no modes")?;
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
                size: (phys_w as i32, phys_h as i32).into(),
                subpixel: connector.subpixel().into(),
                make: "Unknown".into(),
                model: "Unknown".into(),
                serial_number: "Unknown".into(),
            },
        );
        let global = output.create_global::<GlideState<TtyData>>(&self.display_handle);
        output.set_preferred(wl_mode);
        output.change_current_state(Some(wl_mode), None, None, Some((0, 0).into()));
        self.space.map_output(&output, (0, 0));

        let device = self
            .backend_data
            .device
            .as_mut()
            .ok_or("device not initialized")?;

        let drm_output = device
            .drm_output_manager
            .lock()
            .initialize_output::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                crtc,
                drm_mode,
                &[connector.handle()],
                &output,
                None,
                &mut device.gles_renderer,
                &DrmOutputRenderElements::default(),
            )?;

        device.surfaces.insert(
            crtc,
            SurfaceData {
                output,
                global: Some(global),
                drm_output,
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
        let Some(device) = self.backend_data.device.as_mut() else {
            return;
        };
        let Some(surface) = device.surfaces.get_mut(&crtc) else {
            return;
        };

        // M1.2a: an empty scene, just the clear color. Window contents are
        // added in M1.2b via `space_render_elements`.
        let elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();

        match surface.drm_output.render_frame(
            &mut device.gles_renderer,
            &elements,
            FILL_COLOR,
            FrameFlags::DEFAULT,
        ) {
            Ok(res) if !res.is_empty => {
                if let Err(err) = surface.drm_output.queue_frame(()) {
                    tracing::error!("failed to queue frame: {:?}", err);
                }
            }
            Ok(_) => {
                // No damage since the last frame: nothing to submit. The
                // screen keeps showing the last committed framebuffer, and
                // the next repaint (if any) is driven by the next VBlank.
            }
            Err(err) => {
                tracing::error!("failed to render frame: {:?}", err);
            }
        }
    }
}
