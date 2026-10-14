use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::output_power_management::v1::server::{
    zwlr_output_power_manager_v1::{self, ZwlrOutputPowerManagerV1},
    zwlr_output_power_v1::{self, Mode, ZwlrOutputPowerV1},
};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, backend::GlobalId,
};
use smithay::wayland::GlobalData;

use crate::GlideState;

pub struct WlrOutputPowerState {
    global: GlobalId,
    controls: Vec<(ZwlrOutputPowerV1, Output)>,
}

impl std::fmt::Debug for WlrOutputPowerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WlrOutputPowerState")
            .field("global", &self.global)
            .field("controls", &self.controls.len())
            .finish()
    }
}

impl WlrOutputPowerState {
    #[must_use]
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ZwlrOutputPowerManagerV1, GlobalData> + 'static,
    {
        let global = display.create_global::<D, ZwlrOutputPowerManagerV1, _>(1, GlobalData);
        Self { global, controls: Vec::new() }
    }

    pub fn broadcast_mode(&mut self, blanked: bool) {
        self.controls.retain(|(control, _)| control.is_alive());
        let mode = if blanked { Mode::Off } else { Mode::On };
        for (control, _) in &self.controls {
            control.mode(mode);
        }
    }
}

impl GlobalDispatch<ZwlrOutputPowerManagerV1, GlobalData> for GlideState {
    fn bind(
        _state: &mut Self, _dh: &DisplayHandle, _client: &Client, resource: New<ZwlrOutputPowerManagerV1>,
        _global_data: &GlobalData, data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, GlobalData);
    }

    fn can_view(client: Client, _global_data: &GlobalData) -> bool {
        crate::state::client_is_unrestricted(&client)
    }
}

impl Dispatch<ZwlrOutputPowerManagerV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, _resource: &ZwlrOutputPowerManagerV1,
        request: zwlr_output_power_manager_v1::Request, _data: &GlobalData, _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_output_power_manager_v1::Request::GetOutputPower { id, output } = request {
            let control = data_init.init(id, GlobalData);
            let Some(output) = Output::from_resource(&output) else {
                control.failed();
                return;
            };
            control.mode(if state.outputs_blanked { Mode::Off } else { Mode::On });
            state.protocols.wlr_output_power_state.controls.push((control, output));
        }
    }
}

impl Dispatch<ZwlrOutputPowerV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, resource: &ZwlrOutputPowerV1, request: zwlr_output_power_v1::Request,
        _data: &GlobalData, _dh: &DisplayHandle, _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_output_power_v1::Request::SetMode { mode } => {
                let blank = matches!(mode, smithay::reexports::wayland_server::WEnum::Value(Mode::Off));
                state.set_outputs_blanked(blank);
                state.protocols.wlr_output_power_state.broadcast_mode(state.outputs_blanked);
            }
            zwlr_output_power_v1::Request::Destroy => {
                state.protocols.wlr_output_power_state.controls.retain(|(control, _)| control != resource);
            }
            _ => {}
        }
    }
}
