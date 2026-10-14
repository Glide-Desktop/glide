use smithay::wayland::selection::{
    ext_data_control::{DataControlHandler as ExtDataControlHandler, DataControlState as ExtDataControlState},
    primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
    wlr_data_control::{DataControlHandler as WlrDataControlHandler, DataControlState as WlrDataControlState},
};

use crate::GlideState;

impl PrimarySelectionHandler for GlideState {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.protocols.primary_selection_state
    }
}

impl WlrDataControlHandler for GlideState {
    fn data_control_state(&mut self) -> &mut WlrDataControlState {
        &mut self.protocols.wlr_data_control_state
    }
}

impl ExtDataControlHandler for GlideState {
    fn data_control_state(&mut self) -> &mut ExtDataControlState {
        &mut self.protocols.ext_data_control_state
    }
}
