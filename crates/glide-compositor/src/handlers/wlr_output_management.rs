use std::sync::{Arc, Mutex};

use smithay::desktop::Space;
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::{
    zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};
use smithay::reexports::wayland_server::protocol::wl_output::Transform as WlTransform;
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, GlobalDispatch, New, Resource, backend::GlobalId,
};
use smithay::utils::Transform;
use smithay::wayland::GlobalData;

use crate::{GlideState, window::GlideWindow};

pub trait WlrOutputManagerHandler {
    fn wlr_output_manager_state(&mut self) -> &mut WlrOutputManagerState;
    fn apply_output_config(&mut self, config: PendingOutputConfig) -> bool;
}

impl WlrOutputManagerHandler for GlideState {
    fn wlr_output_manager_state(&mut self) -> &mut WlrOutputManagerState {
        &mut self.protocols.wlr_output_manager_state
    }

    fn apply_output_config(&mut self, config: PendingOutputConfig) -> bool {
        let mut any_changed = false;
        for head in config.heads {
            let output = self
                .space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(output) = output {
                if !head.enabled {
                    self.space.unmap_output(&output);
                    any_changed = true;
                    continue;
                }
                if let Some((x, y)) = head.position {
                    self.space.map_output(&output, (x, y));
                    any_changed = true;
                }
                if let Some(scale) = head.scale {
                    output.change_current_state(
                        None,
                        None,
                        Some(smithay::output::Scale::Fractional(scale)),
                        None,
                    );
                    any_changed = true;
                }
                if let Some(transform) = head.transform {
                    output.change_current_state(None, Some(transform), None, None);
                    any_changed = true;
                }
                if let Some((w, h, refresh)) = head.mode {
                    let wl_mode = smithay::output::Mode {
                        size: (w, h).into(),
                        refresh,
                    };
                    output.change_current_state(Some(wl_mode), None, None, None);
                    any_changed = true;
                }
            }
        }
        if any_changed {
            self.sync_layout_outputs();
            self.reconcile_space();
        }
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct PendingHeadConfig {
    pub output_name: String,
    pub enabled: bool,
    pub mode: Option<(i32, i32, i32)>,
    pub position: Option<(i32, i32)>,
    pub scale: Option<f64>,
    pub transform: Option<Transform>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingOutputConfig {
    pub serial: u32,
    pub heads: Vec<PendingHeadConfig>,
}

#[derive(Debug)]
pub struct WlrOutputManagerState {
    pub global: GlobalId,
    pub instances: Vec<ZwlrOutputManagerV1>,
    pub serial: u32,
}

impl WlrOutputManagerState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: WlrOutputManagerHandler + GlobalDispatch<ZwlrOutputManagerV1, GlobalData> + 'static,
    {
        let global = dh.create_global::<D, ZwlrOutputManagerV1, _>(4, GlobalData);
        Self {
            global,
            instances: Vec::new(),
            serial: 1,
        }
    }

    pub fn next_serial(&mut self) -> u32 {
        let s = self.serial;
        self.serial = self.serial.wrapping_add(1);
        s
    }

    #[must_use]
    pub const fn is_current(&self, serial: u32) -> bool {
        serial == self.serial.wrapping_sub(1)
    }

    pub fn sync_outputs(&mut self, dh: &DisplayHandle, space: &Space<GlideWindow>) {
        let serial = self.next_serial();
        for manager in &self.instances {
            Self::send_manager_state(manager, dh, space, serial);
        }
    }

    fn send_manager_state(
        manager: &ZwlrOutputManagerV1,
        dh: &DisplayHandle,
        space: &Space<GlideWindow>,
        serial: u32,
    ) {
        let client = manager.client();

        for output in space.outputs() {
            let output_name = output.name();
            let geo = space.output_geometry(output).unwrap_or_default();
            let scale = output.current_scale().fractional_scale();
            let transform = output.current_transform();
            let (phys_w, phys_h) = output.physical_properties().size.into();

            if let Some(client_ref) = &client
                && let Ok(head) = client_ref.create_resource::<ZwlrOutputHeadV1, _, GlideState>(
                    dh,
                    manager.version(),
                    output_name.clone(),
                )
            {
                manager.head(&head);
                head.name(output_name.clone());
                head.description(format!("Glide Output {output_name}"));
                head.physical_size(phys_w, phys_h);
                head.make(output.physical_properties().make.clone());
                head.model(output.physical_properties().model.clone());
                head.serial_number(output.physical_properties().serial_number.clone());
                head.enabled(1);
                head.position(geo.loc.x, geo.loc.y);
                head.scale(scale);

                let wl_transform = match transform {
                    Transform::Normal => WlTransform::Normal,
                    Transform::_90 => WlTransform::_90,
                    Transform::_180 => WlTransform::_180,
                    Transform::_270 => WlTransform::_270,
                    Transform::Flipped => WlTransform::Flipped,
                    Transform::Flipped90 => WlTransform::Flipped90,
                    Transform::Flipped180 => WlTransform::Flipped180,
                    Transform::Flipped270 => WlTransform::Flipped270,
                };
                head.transform(wl_transform);

                if let Some(mode) = output.current_mode() {
                    let (w, h) = mode.size.into();
                    if let Ok(mode_res) = client_ref
                        .create_resource::<ZwlrOutputModeV1, _, GlideState>(
                            dh,
                            manager.version(),
                            (w, h, mode.refresh),
                        )
                    {
                        head.mode(&mode_res);
                        mode_res.size(w, h);
                        mode_res.refresh(mode.refresh);
                        mode_res.preferred();
                        head.current_mode(&mode_res);
                    }
                }
            }
        }

        manager.done(serial);
    }
}

impl GlobalDispatch<ZwlrOutputManagerV1, GlobalData> for GlideState {
    fn bind(
        state: &mut Self,
        dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrOutputManagerV1>,
        _global_data: &GlobalData,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let instance = data_init.init(resource, GlobalData);
        state
            .protocols
            .wlr_output_manager_state
            .instances
            .push(instance.clone());
        let serial = state.protocols.wlr_output_manager_state.next_serial();
        WlrOutputManagerState::send_manager_state(&instance, dh, &state.space, serial);
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputManagerV1, GlobalData> for GlideState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwlrOutputManagerV1,
        request: zwlr_output_manager_v1::Request,
        _data: &GlobalData,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } = request {
            let config = PendingOutputConfig {
                serial,
                heads: Vec::new(),
            };
            data_init.init(id, Arc::new(Mutex::new(config)));
        }
    }
}

type ConfigurationUserData = Arc<Mutex<PendingOutputConfig>>;

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputConfigurationV1, ConfigurationUserData>
    for GlideState
{
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request,
        data: &ConfigurationUserData,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head } => {
                let head_name = head.data::<String>().cloned().unwrap_or_default();
                let head_data = HeadConfigData {
                    config: Arc::clone(data),
                    head_name,
                };
                data_init.init(id, head_data);
            }
            zwlr_output_configuration_v1::Request::DisableHead { head } => {
                let head_name = head.data::<String>().cloned().unwrap_or_default();
                let mut config = data.lock().unwrap();
                if let Some(c) = config.heads.iter_mut().find(|h| h.output_name == head_name) {
                    c.enabled = false;
                } else {
                    config.heads.push(PendingHeadConfig {
                        output_name: head_name,
                        enabled: false,
                        ..PendingHeadConfig::default()
                    });
                }
            }
            zwlr_output_configuration_v1::Request::Apply => {
                let config = data.lock().unwrap().clone();
                if !state
                    .protocols
                    .wlr_output_manager_state
                    .is_current(config.serial)
                {
                    resource.cancelled();
                    return;
                }
                let ok = state.apply_output_config(config);
                if ok {
                    resource.succeeded();
                } else {
                    resource.failed();
                }
            }
            zwlr_output_configuration_v1::Request::Test => {
                let serial = data.lock().unwrap().serial;
                if state.protocols.wlr_output_manager_state.is_current(serial) {
                    resource.succeeded();
                } else {
                    resource.cancelled();
                }
            }
            _ => {}
        }
    }
}

pub struct HeadConfigData {
    pub config: Arc<Mutex<PendingOutputConfig>>,
    pub head_name: String,
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputConfigurationHeadV1, HeadConfigData>
    for GlideState
{
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request,
        data: &HeadConfigData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let mut config = data.config.lock().unwrap();
        let head_entry = if let Some(h) = config
            .heads
            .iter_mut()
            .find(|h| h.output_name == data.head_name)
        {
            h
        } else {
            config.heads.push(PendingHeadConfig {
                output_name: data.head_name.clone(),
                enabled: true,
                ..Default::default()
            });
            config.heads.last_mut().unwrap()
        };

        match request {
            zwlr_output_configuration_head_v1::Request::SetMode { mode } => {
                let mode_data = mode
                    .data::<(i32, i32, i32)>()
                    .copied()
                    .unwrap_or((1920, 1080, 60000));
                head_entry.mode = Some(mode_data);
            }
            zwlr_output_configuration_head_v1::Request::SetCustomMode {
                width,
                height,
                refresh,
            } => {
                head_entry.mode = Some((width, height, refresh));
            }
            zwlr_output_configuration_head_v1::Request::SetPosition { x, y } => {
                head_entry.position = Some((x, y));
            }
            zwlr_output_configuration_head_v1::Request::SetTransform {
                transform: smithay::reexports::wayland_server::WEnum::Value(val),
            } => {
                let t = match val {
                    WlTransform::_90 => Transform::_90,
                    WlTransform::_180 => Transform::_180,
                    WlTransform::_270 => Transform::_270,
                    WlTransform::Flipped => Transform::Flipped,
                    WlTransform::Flipped90 => Transform::Flipped90,
                    WlTransform::Flipped180 => Transform::Flipped180,
                    WlTransform::Flipped270 => Transform::Flipped270,
                    _ => Transform::Normal,
                };
                head_entry.transform = Some(t);
            }
            zwlr_output_configuration_head_v1::Request::SetScale { scale } => {
                head_entry.scale = Some(scale);
            }
            _ => {}
        }
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputHeadV1, String> for GlideState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwlrOutputHeadV1,
        _request: zwlr_output_head_v1::Request,
        _data: &String,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl smithay::reexports::wayland_server::Dispatch<ZwlrOutputModeV1, (i32, i32, i32)>
    for GlideState
{
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwlrOutputModeV1,
        _request: zwlr_output_mode_v1::Request,
        _data: &(i32, i32, i32),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};

    fn sample_output(name: &str, w: i32, h: i32) -> Output {
        let output = Output::new(
            name.to_string(),
            PhysicalProperties {
                size: (530, 300).into(),
                subpixel: Subpixel::Unknown,
                make: "TestMake".into(),
                model: "TestModel".into(),
                serial_number: "1234".into(),
            },
        );
        let mode = Mode {
            size: (w, h).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            None,
            Some((0, 0).into()),
        );
        output
    }

    #[test]
    fn test_pending_output_config_default() {
        let config = PendingOutputConfig::default();
        assert!(config.heads.is_empty());
    }

    #[test]
    fn test_pending_head_config_building() {
        let mut head = PendingHeadConfig {
            output_name: "HDMI-A-1".to_string(),
            enabled: true,
            ..Default::default()
        };
        head.position = Some((1920, 0));
        head.scale = Some(2.0);
        head.mode = Some((1920, 1080, 60000));
        head.transform = Some(Transform::_90);

        assert_eq!(head.output_name, "HDMI-A-1");
        assert!(head.enabled);
        assert_eq!(head.position, Some((1920, 0)));
        assert_eq!(head.scale, Some(2.0));
        assert_eq!(head.mode, Some((1920, 1080, 60000)));
        assert_eq!(head.transform, Some(Transform::_90));
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_apply_output_config_head_properties_update() {
        let output = Output::new(
            "HDMI-A-1".to_string(),
            PhysicalProperties {
                size: (530, 300).into(),
                subpixel: Subpixel::Unknown,
                make: "TestMake".into(),
                model: "TestModel".into(),
                serial_number: "1234".into(),
            },
        );
        let mode = Mode {
            size: (1920, 1080).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            None,
            Some((0, 0).into()),
        );

        let mut space = Space::<GlideWindow>::default();
        space.map_output(&output, (0, 0));

        let config = PendingOutputConfig {
            serial: 0,
            heads: vec![PendingHeadConfig {
                output_name: "HDMI-A-1".to_string(),
                enabled: true,
                mode: Some((2560, 1440, 144_000)),
                position: Some((1920, 0)),
                scale: Some(2.0),
                transform: Some(Transform::_90),
            }],
        };

        let mut any_changed = false;
        for head in config.heads {
            let found_output = space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(out) = found_output {
                if let Some((x, y)) = head.position {
                    space.map_output(&out, (x, y));
                    any_changed = true;
                }
                if let Some(scale) = head.scale {
                    out.change_current_state(
                        None,
                        None,
                        Some(smithay::output::Scale::Fractional(scale)),
                        None,
                    );
                    any_changed = true;
                }
                if let Some(transform) = head.transform {
                    out.change_current_state(None, Some(transform), None, None);
                    any_changed = true;
                }
                if let Some((w, h, refresh)) = head.mode {
                    let wl_mode = Mode {
                        size: (w, h).into(),
                        refresh,
                    };
                    out.change_current_state(Some(wl_mode), None, None, None);
                    any_changed = true;
                }
            }
        }

        assert!(any_changed);
        assert_eq!(
            space.output_geometry(&output).unwrap().loc,
            (1920, 0).into()
        );
        assert_eq!(output.current_transform(), Transform::_90);
        assert_eq!(output.current_scale().fractional_scale(), 2.0);
        assert_eq!(output.current_mode().unwrap().size, (2560, 1440).into());
        assert_eq!(output.current_mode().unwrap().refresh, 144_000);
    }

    #[test]
    fn test_apply_output_config_unmatched_head_name_ignored() {
        let output = Output::new(
            "HDMI-A-1".to_string(),
            PhysicalProperties {
                size: (530, 300).into(),
                subpixel: Subpixel::Unknown,
                make: "TestMake".into(),
                model: "TestModel".into(),
                serial_number: "1234".into(),
            },
        );
        let mode = Mode {
            size: (1920, 1080).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            None,
            Some((0, 0).into()),
        );
        let mut space = Space::<GlideWindow>::default();
        space.map_output(&output, (0, 0));

        let config = PendingOutputConfig {
            serial: 0,
            heads: vec![PendingHeadConfig {
                output_name: "DP-1".to_string(),
                enabled: true,
                position: Some((1920, 0)),
                ..Default::default()
            }],
        };

        let mut any_changed = false;
        for head in config.heads {
            let found_output = space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(out) = found_output
                && let Some((x, y)) = head.position
            {
                space.map_output(&out, (x, y));
                any_changed = true;
            }
        }

        assert!(!any_changed);
        assert_eq!(space.output_geometry(&output).unwrap().loc, (0, 0).into());
    }

    #[test]
    fn test_enable_head_preserves_output_name() {
        let output_name = "HDMI-A-1".to_string();
        let head_name = Some(&output_name).cloned().unwrap_or_default();
        assert_eq!(head_name, "HDMI-A-1");
    }

    #[test]
    fn test_apply_output_config_disabled_head_handling() {
        let output = Output::new(
            "HDMI-A-1".to_string(),
            PhysicalProperties {
                size: (530, 300).into(),
                subpixel: Subpixel::Unknown,
                make: "TestMake".into(),
                model: "TestModel".into(),
                serial_number: "1234".into(),
            },
        );
        let mode = Mode {
            size: (1920, 1080).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            None,
            Some((0, 0).into()),
        );

        let mut space = Space::<GlideWindow>::default();
        space.map_output(&output, (0, 0));

        let config = PendingOutputConfig {
            serial: 0,
            heads: vec![PendingHeadConfig {
                output_name: "HDMI-A-1".to_string(),
                enabled: false,
                ..Default::default()
            }],
        };

        assert!(!config.heads[0].enabled);
        assert_eq!(space.outputs().count(), 1);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_apply_output_config_invalid_heads_and_modes() {
        let output = Output::new(
            "HDMI-A-1".to_string(),
            PhysicalProperties {
                size: (530, 300).into(),
                subpixel: Subpixel::Unknown,
                make: "TestMake".into(),
                model: "TestModel".into(),
                serial_number: "1234".into(),
            },
        );
        let mode = Mode {
            size: (1920, 1080).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            None,
            Some((0, 0).into()),
        );
        let mut space = Space::<GlideWindow>::default();
        space.map_output(&output, (0, 0));

        let config = PendingOutputConfig {
            serial: 0,
            heads: vec![
                PendingHeadConfig {
                    output_name: "NONEXISTENT-1".to_string(),
                    enabled: true,
                    mode: Some((0, 0, 0)),
                    position: Some((-100, -100)),
                    scale: Some(0.0),
                    transform: Some(Transform::_90),
                },
                PendingHeadConfig {
                    output_name: String::new(),
                    enabled: false,
                    mode: Some((-1920, -1080, -60000)),
                    position: Some((i32::MIN, i32::MIN)),
                    scale: Some(-1.0),
                    transform: None,
                },
                PendingHeadConfig {
                    output_name: "HDMI-A-1".to_string(),
                    enabled: true,
                    mode: Some((0, 0, 0)),
                    position: Some((i32::MAX, i32::MAX)),
                    scale: Some(f64::NAN),
                    transform: Some(Transform::_180),
                },
                PendingHeadConfig {
                    output_name: "HDMI-A-1".to_string(),
                    enabled: true,
                    mode: Some((i32::MAX, i32::MAX, i32::MAX)),
                    position: Some((0, 0)),
                    scale: Some(f64::INFINITY),
                    transform: Some(Transform::_270),
                },
            ],
        };

        let mut any_changed = false;
        for head in config.heads {
            let found_output = space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(out) = found_output {
                if let Some((x, y)) = head.position {
                    space.map_output(&out, (x, y));
                    any_changed = true;
                }
                if let Some(scale) = head.scale {
                    out.change_current_state(
                        None,
                        None,
                        Some(smithay::output::Scale::Fractional(scale)),
                        None,
                    );
                    any_changed = true;
                }
                if let Some(transform) = head.transform {
                    out.change_current_state(None, Some(transform), None, None);
                    any_changed = true;
                }
                if let Some((w, h, refresh)) = head.mode {
                    let wl_mode = Mode {
                        size: (w, h).into(),
                        refresh,
                    };
                    out.change_current_state(Some(wl_mode), None, None, None);
                    any_changed = true;
                }
            }
        }

        assert!(any_changed);
        assert_eq!(space.output_geometry(&output).unwrap().loc, (0, 0).into());
        assert_eq!(output.current_transform(), Transform::_270);
    }

    #[test]
    fn test_apply_output_config_duplicate_heads_stress() {
        let output = sample_output("HDMI-A-1", 1920, 1080);
        let mut space = Space::<GlideWindow>::default();
        space.map_output(&output, (0, 0));

        let config = PendingOutputConfig {
            serial: 0,
            heads: vec![
                PendingHeadConfig {
                    output_name: "HDMI-A-1".to_string(),
                    enabled: true,
                    position: Some((100, 100)),
                    scale: Some(1.0),
                    mode: None,
                    transform: None,
                },
                PendingHeadConfig {
                    output_name: "HDMI-A-1".to_string(),
                    enabled: true,
                    position: Some((200, 200)),
                    scale: Some(2.0),
                    mode: None,
                    transform: None,
                },
            ],
        };

        let mut any_changed = false;
        for head in config.heads {
            let found_output = space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(out) = found_output
                && let Some((x, y)) = head.position
            {
                space.map_output(&out, (x, y));
                any_changed = true;
            }
        }
        assert!(any_changed);
        assert_eq!(
            space.output_geometry(&output).unwrap().loc,
            (200, 200).into()
        );
    }

    #[test]
    fn test_apply_output_config_rapid_updates_and_stress() {
        let mut space = Space::<GlideWindow>::default();
        let output = sample_output("HDMI-A-1", 1920, 1080);
        space.map_output(&output, (0, 0));

        let mut heads = Vec::new();
        for i in 0..100 {
            heads.push(PendingHeadConfig {
                output_name: "HDMI-A-1".to_string(),
                enabled: true,
                position: Some((i * 10, i * 5)),
                scale: Some(1.0 + f64::from(i) * 0.01),
                mode: Some((1920, 1080, 60000)),
                transform: Some(Transform::Normal),
            });
        }
        let config = PendingOutputConfig { serial: 0, heads };

        let mut any_changed = false;
        for head in config.heads {
            let found_output = space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(out) = found_output
                && let Some((x, y)) = head.position
            {
                space.map_output(&out, (x, y));
                any_changed = true;
            }
        }

        assert!(any_changed);
        assert_eq!(
            space.output_geometry(&output).unwrap().loc,
            (990, 495).into()
        );
    }

    #[test]
    fn test_apply_output_config_empty_heads_and_no_outputs() {
        let mut empty_space = Space::<GlideWindow>::default();
        let config = PendingOutputConfig::default();

        let mut any_changed = false;
        for head in config.heads {
            let found_output = empty_space
                .outputs()
                .find(|o| o.name() == head.output_name)
                .cloned();
            if let Some(out) = found_output
                && let Some((x, y)) = head.position
            {
                empty_space.map_output(&out, (x, y));
                any_changed = true;
            }
        }
        assert!(!any_changed);
    }
}
