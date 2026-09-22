use std::io::Read;
use std::os::fd::OwnedFd;

use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, backend::GlobalId,
};
use smithay::wayland::GlobalData;

use crate::GlideState;

pub struct WlrGammaControlState {
    global: GlobalId,
    controls: Vec<(ZwlrGammaControlV1, Output)>,
}

impl std::fmt::Debug for WlrGammaControlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WlrGammaControlState")
            .field("global", &self.global)
            .field("controls", &self.controls.len())
            .finish()
    }
}

impl WlrGammaControlState {
    #[must_use]
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ZwlrGammaControlManagerV1, GlobalData> + 'static,
    {
        let global = display.create_global::<D, ZwlrGammaControlManagerV1, _>(1, GlobalData);
        Self {
            global,
            controls: Vec::new(),
        }
    }

    #[must_use]
    pub const fn global(&self) -> &GlobalId {
        &self.global
    }
}

impl GlobalDispatch<ZwlrGammaControlManagerV1, GlobalData> for GlideState {
    fn bind(
        _state: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrGammaControlManagerV1>,
        _global_data: &GlobalData,
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, GlobalData);
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwlrGammaControlManagerV1,
        request: zwlr_gamma_control_manager_v1::Request,
        _data: &GlobalData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } = request {
            let control = data_init.init(id, GlobalData);
            let Some(output) = Output::from_resource(&output) else {
                control.failed();
                return;
            };
            let Some(size) = state.gamma_size(&output).filter(|size| *size > 0) else {
                control.failed();
                return;
            };
            control.gamma_size(size);
            state
                .protocols
                .wlr_gamma_control_state
                .controls
                .push((control, output));
        }
    }
}

impl Dispatch<ZwlrGammaControlV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwlrGammaControlV1,
        request: zwlr_gamma_control_v1::Request,
        _data: &GlobalData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_gamma_control_v1::Request::SetGamma { fd } => {
                let output = state
                    .protocols
                    .wlr_gamma_control_state
                    .controls
                    .iter()
                    .find(|(control, _)| control == resource)
                    .map(|(_, output)| output.clone());
                let Some(output) = output else {
                    return;
                };
                let Some(size) = state.gamma_size(&output).filter(|size| *size > 0) else {
                    resource.failed();
                    return;
                };
                match read_ramp(fd, size as usize) {
                    Some(ramp) => {
                        if !state.apply_gamma(&output, &ramp) {
                            resource.failed();
                        }
                    }
                    None => resource.failed(),
                }
            }
            zwlr_gamma_control_v1::Request::Destroy => {
                let output = state
                    .protocols
                    .wlr_gamma_control_state
                    .controls
                    .iter()
                    .find(|(control, _)| control == resource)
                    .map(|(_, output)| output.clone());
                state
                    .protocols
                    .wlr_gamma_control_state
                    .controls
                    .retain(|(control, _)| control != resource);
                if let Some(output) = output {
                    state.reset_gamma(&output);
                }
            }
            _ => {}
        }
    }
}

fn read_ramp(fd: OwnedFd, size: usize) -> Option<Vec<u16>> {
    let mut file = std::fs::File::from(fd);
    let mut bytes = vec![0u8; size * 3 * 2];
    file.read_exact(&mut bytes).ok()?;
    Some(
        bytes
            .chunks_exact(2)
            .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
            .collect(),
    )
}
