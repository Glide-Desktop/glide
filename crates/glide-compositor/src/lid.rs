use smithay::backend::input::{InputBackend, Switch, SwitchState, SwitchToggleEvent};
use smithay::output::Output;

use crate::GlideState;

#[must_use]
pub fn is_internal(name: &str) -> bool {
    ["eDP", "LVDS", "DSI"].iter().any(|prefix| name.starts_with(prefix))
}

#[must_use]
pub fn panels_hidden(lid_closed: bool, connected: &[String]) -> bool {
    lid_closed && connected.iter().any(|name| !is_internal(name))
}

impl GlideState {
    pub fn handle_switch<I: InputBackend>(&mut self, event: &I::SwitchToggleEvent) {
        if event.switch() != Some(Switch::Lid) {
            return;
        }
        let closed = event.state() == SwitchState::On;
        if !closed {
            self.protocols.idle_notifier_state.notify_activity(&self.seat);
            self.idle.activity(std::time::Instant::now());
            if self.outputs_blanked {
                self.set_outputs_blanked(false);
            }
        }
        if self.session.lid_closed != closed {
            tracing::info!(closed, "lid switch");
            self.session.lid_closed = closed;
            self.apply_lid();
        }
    }

    pub fn apply_lid(&mut self) {
        let outputs = self.connected_outputs();
        let names: Vec<String> = outputs.iter().map(Output::name).collect();
        let mut changed = false;
        if panels_hidden(self.session.lid_closed, &names) {
            let panels: Vec<Output> = outputs
                .iter()
                .filter(|output| is_internal(&output.name()) && self.space.output_geometry(output).is_some())
                .cloned()
                .collect();
            for panel in panels {
                tracing::info!(output = %panel.name(), "lid closed with another display, switching the panel off");
                self.space.unmap_output(&panel);
                self.session.lid_hidden.push(panel);
                changed = true;
            }
        } else {
            for panel in std::mem::take(&mut self.session.lid_hidden) {
                if outputs.contains(&panel) {
                    self.space.map_output(&panel, panel.current_location());
                    changed = true;
                }
            }
        }
        if changed {
            self.apply_output_configs();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn laptop_panels_are_recognised_by_connector() {
        assert!(is_internal("eDP-1"));
        assert!(is_internal("LVDS-1"));
        assert!(is_internal("DSI-1"));
        assert!(!is_internal("DP-1"));
        assert!(!is_internal("HDMI-A-1"));
    }

    #[test]
    fn a_closed_lid_hides_the_panel_only_when_another_display_is_connected() {
        assert!(panels_hidden(true, &names(&["eDP-1", "HDMI-A-1"])));
        assert!(!panels_hidden(true, &names(&["eDP-1"])));
        assert!(!panels_hidden(true, &[]));
        assert!(!panels_hidden(false, &names(&["eDP-1", "HDMI-A-1"])));
    }
}
