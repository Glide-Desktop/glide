use std::{collections::HashMap, path::PathBuf, time::Duration};

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
            element::{
                AsRenderElements, memory::MemoryRenderBuffer, surface::WaylandSurfaceRenderElement,
            },
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
    pub GlideRenderElements<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, GlideWindowRenderElement<GlesRenderer>>,
    Pointer=PointerRenderElement<GlesRenderer>,
    Overview=crate::overview::OverviewRenderElement<GlesRenderer>,
}

fn layer_elements(
    renderer: &mut GlesRenderer,
    output: &Output,
    scale: Scale<f64>,
    upper: bool,
) -> Vec<GlideRenderElements> {
    use smithay::desktop::layer_map_for_output;
    use smithay::wayland::shell::wlr_layer::Layer;

    let map = layer_map_for_output(output);
    let wanted = if upper {
        [Layer::Overlay, Layer::Top]
    } else {
        [Layer::Bottom, Layer::Background]
    };

    let mut elements = Vec::new();
    for layer in wanted {
        for surface in map.layers_on(layer).rev() {
            let Some(geometry) = map.layer_geometry(surface) else {
                continue;
            };
            let location = geometry.loc.to_f64().to_physical(scale).to_i32_round();
            let surface_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                AsRenderElements::render_elements(surface, renderer, location, scale, 1.0);
            elements.extend(
                surface_elements
                    .into_iter()
                    .map(SpaceRenderElements::Surface)
                    .map(GlideRenderElements::Space),
            );
        }
    }
    elements
}

struct SurfaceData {
    output: Output,
    global: Option<GlobalId>,
    drm_output: GbmDrmOutput,
    pending_repaint: Option<RegistrationToken>,
}

struct DeviceData {
    drm_output_manager: GbmDrmOutputManager,
    drm_scanner: DrmScanner,
    surfaces: HashMap<crtc::Handle, SurfaceData>,
    gles_renderer: GlesRenderer,
    cursor: Cursor,
    cursor_buffers: Vec<(xcursor::parser::Image, MemoryRenderBuffer)>,
    pointer_element: PointerElement,
}

pub struct TtyData {
    session: LibSeatSession,
    primary_gpu: DrmNode,
    primary_gpu_path: PathBuf,
    device: Option<DeviceData>,
    dmabuf_state: Option<(DmabufState, DmabufGlobal)>,
}

impl TtyData {
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
                tracing::info!(
                    ?device_id,
                    ?path,
                    "udev device added; ignored, Glide only drives the primary GPU"
                );
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

enum FrameOutcome {
    Queued(smithay::backend::renderer::element::RenderElementStates),
    NoDamage(smithay::backend::renderer::element::RenderElementStates),
    Failed,
}

#[allow(clippy::too_many_arguments)]
fn overview_elements(
    gles_renderer: &mut GlesRenderer,
    overview: &mut crate::overview::OverviewUi,
    window_map: &HashMap<glide_layout::WindowId, GlideWindow>,
    pointer_element: &mut PointerElement,
    pointer_location: Point<f64, Logical>,
    cursor_hotspot: Point<i32, Logical>,
    scale: Scale<f64>,
    output: &Output,
    output_geometry: smithay::utils::Rectangle<i32, Logical>,
) -> Vec<GlideRenderElements> {
    let mut elements: Vec<GlideRenderElements> = Vec::new();

    elements.extend(
        pointer_element
            .render_elements::<PointerRenderElement<GlesRenderer>>(
                gles_renderer,
                (pointer_location - cursor_hotspot.to_f64())
                    .to_physical(scale)
                    .to_i32_round(),
                scale,
                1.0,
            )
            .into_iter()
            .map(GlideRenderElements::Pointer),
    );

    elements.extend(layer_elements(gles_renderer, output, scale, true));

    let painter = crate::paint::current();
    let output_scale = output.current_scale().integer_scale();
    elements.extend(
        overview
            .render(
                gles_renderer,
                output_geometry,
                scale,
                output_scale,
                &painter,
                |id| {
                    window_map
                        .get(&id)
                        .map(|window| (window.clone(), window.display_name()))
                },
            )
            .into_iter()
            .map(GlideRenderElements::Overview),
    );

    elements.extend(layer_elements(gles_renderer, output, scale, false));
    elements
}

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
        let egl_context =
            EGLContext::new(&egl_display).context("failed to create an EGL context")?;
        let gles_renderer = unsafe { GlesRenderer::new(egl_context) }
            .context("failed to create the GLES renderer")?;

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

        self.sync_layout_outputs();
        self.reconcile_space();

        let Some(global) = surface.global else {
            return;
        };
        let global_for_removal = global.clone();
        self.display_handle.disable_global::<GlideState>(global);
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

    pub fn queue_redraw(&mut self) {
        let Some(device) = self.backend_data.device.as_ref() else {
            return;
        };
        let crtcs: Vec<crtc::Handle> = device.surfaces.keys().copied().collect();
        for crtc in crtcs {
            self.loop_handle
                .insert_idle(move |state| state.render_surface(crtc));
        }
    }

    fn render_surface(&mut self, crtc: crtc::Handle) {
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

        if let Some(token) = surface.pending_repaint.take() {
            self.loop_handle.remove(token);
        }

        let output = surface.output.clone();
        let scale = Scale::from(output.current_scale().fractional_scale());

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

        let elements = if let Some(overview) = self.overview.as_mut() {
            let Some(output_geometry) = self.space.output_geometry(&output) else {
                return;
            };
            overview_elements(
                gles_renderer,
                overview,
                &self.window_map,
                pointer_element,
                pointer_location,
                cursor_hotspot,
                scale,
                &output,
                output_geometry,
            )
        } else {
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
            elements
        };

        let clear_color = self.background_color.0;

        let render_result = surface.drm_output.render_frame(
            gles_renderer,
            &elements,
            clear_color,
            FrameFlags::DEFAULT,
        );

        let outcome = match render_result {
            Ok(res) if res.is_empty => FrameOutcome::NoDamage(res.states),
            Ok(res) => match surface.drm_output.queue_frame(()) {
                Ok(()) => FrameOutcome::Queued(res.states),
                Err(err) => {
                    tracing::error!("failed to queue frame: {:?}", err);
                    FrameOutcome::Failed
                }
            },
            Err(err) => {
                tracing::error!("failed to render frame: {:?}", err);
                FrameOutcome::Failed
            }
        };

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
            FrameOutcome::Queued(_) => {
                self.space.elements_for_output(output).for_each(|window| {
                    window.send_frame(
                        output,
                        self.start_time.elapsed(),
                        Some(Duration::ZERO),
                        |_, _| Some(output.clone()),
                    );
                });
            }
            FrameOutcome::NoDamage(_) | FrameOutcome::Failed => {
                self.schedule_repaint(crtc, output);
            }
        }
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
        let _ = resolve_cursor_buffer(
            &mut cursor,
            &mut cache,
            CursorIcon::NwResize,
            1,
            Duration::ZERO,
        );
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn resolve_cursor_buffer_reuses_the_cache_for_a_repeated_request() {
        let mut cursor = Cursor::load();
        let mut cache = Vec::new();
        let _ = resolve_cursor_buffer(
            &mut cursor,
            &mut cache,
            CursorIcon::Default,
            1,
            Duration::ZERO,
        );
        let _ = resolve_cursor_buffer(
            &mut cursor,
            &mut cache,
            CursorIcon::Default,
            1,
            Duration::ZERO,
        );
        assert_eq!(
            cache.len(),
            1,
            "identical icon/scale/time must not grow the cache"
        );
    }

    #[test]
    fn resolve_cursor_buffer_caches_separately_per_scale() {
        let mut cursor = Cursor::load();
        let mut cache = Vec::new();
        let _ = resolve_cursor_buffer(
            &mut cursor,
            &mut cache,
            CursorIcon::Default,
            1,
            Duration::ZERO,
        );
        let _ = resolve_cursor_buffer(
            &mut cursor,
            &mut cache,
            CursorIcon::Default,
            2,
            Duration::ZERO,
        );
        assert_eq!(cache.len(), 2);
    }
}
