use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt;

use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New,
    backend::{ClientId, GlobalId},
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
        Self { global, controls: Vec::new() }
    }

    fn output_of(&self, resource: &ZwlrGammaControlV1) -> Option<Output> {
        self.controls.iter().find(|(control, _)| control == resource).map(|(_, output)| output.clone())
    }
}

impl GlobalDispatch<ZwlrGammaControlManagerV1, GlobalData> for GlideState {
    fn bind(
        _state: &mut Self, _dh: &DisplayHandle, _client: &Client, resource: New<ZwlrGammaControlManagerV1>,
        _global_data: &GlobalData, data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, GlobalData);
    }

    fn can_view(client: Client, _global_data: &GlobalData) -> bool {
        crate::state::client_is_unrestricted(&client)
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, _resource: &ZwlrGammaControlManagerV1,
        request: zwlr_gamma_control_manager_v1::Request, _data: &GlobalData, _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } = request {
            let control = data_init.init(id, GlobalData);
            let Some(output) = Output::from_resource(&output) else {
                control.failed();
                return;
            };
            let taken =
                state.protocols.wlr_gamma_control_state.controls.iter().any(|(_, existing)| existing == &output);
            if taken {
                control.failed();
                return;
            }
            let Some(size) = state.gamma_size(&output).filter(|size| *size > 0) else {
                control.failed();
                return;
            };
            control.gamma_size(size);
            state.protocols.wlr_gamma_control_state.controls.push((control, output));
        }
    }
}

impl Dispatch<ZwlrGammaControlV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, resource: &ZwlrGammaControlV1, request: zwlr_gamma_control_v1::Request,
        _data: &GlobalData, _dh: &DisplayHandle, _data_init: &mut DataInit<'_, Self>,
    ) {
        let zwlr_gamma_control_v1::Request::SetGamma { fd } = request else {
            return;
        };
        let Some(output) = state.protocols.wlr_gamma_control_state.output_of(resource) else {
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

    fn destroyed(state: &mut Self, _client: ClientId, resource: &ZwlrGammaControlV1, _data: &GlobalData) {
        let gamma = &mut state.protocols.wlr_gamma_control_state;
        let output = gamma.output_of(resource);
        gamma.controls.retain(|(control, _)| control != resource);
        if let Some(output) = output {
            state.reset_gamma(&output);
        }
    }
}

fn read_ramp(fd: OwnedFd, size: usize) -> Option<Vec<u16>> {
    let file = std::fs::File::from(fd);
    let expected = size.checked_mul(3 * 2)?;
    let metadata = file.metadata().ok()?;
    if !metadata.file_type().is_file() || metadata.len() < expected as u64 {
        return None;
    }
    let mut bytes = vec![0u8; expected];
    file.read_exact_at(&mut bytes, 0).ok()?;
    Some(bytes.chunks_exact(2).map(|pair| u16::from_ne_bytes([pair[0], pair[1]])).collect())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn temp_file(bytes: &[u8]) -> std::fs::File {
        let path = std::env::temp_dir().join(format!("glide-gamma-test-{}-{}", std::process::id(), bytes.len()));
        let mut file =
            std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        file.write_all(bytes).unwrap();
        file
    }

    #[test]
    fn read_ramp_reads_regular_file_from_start_regardless_of_offset() {
        let values: Vec<u16> = (0..6).collect();
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let file = temp_file(&bytes);
        assert_eq!(read_ramp(OwnedFd::from(file), 2), Some(values));
    }

    #[test]
    fn read_ramp_rejects_short_file() {
        let file = temp_file(&[0u8; 10]);
        assert_eq!(read_ramp(OwnedFd::from(file), 2), None);
    }

    #[test]
    fn read_ramp_rejects_pipe_without_blocking() {
        let (reader, _writer) = std::io::pipe().unwrap();
        assert_eq!(read_ramp(OwnedFd::from(reader), 2), None);
    }
}
