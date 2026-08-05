use std::collections::HashMap;

use glide_layout::WorkspaceId;
use smithay::reexports::wayland_protocols::ext::workspace::v1::server::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1, State as ProtocolWorkspaceState},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, GlobalDispatch, New, Resource, backend::GlobalId,
};
use smithay::wayland::GlobalData;

use crate::GlideState;

pub trait ExtWorkspaceHandler {
    fn ext_workspace_state(&mut self) -> &mut ExtWorkspaceManagerState;
}

impl ExtWorkspaceHandler for GlideState {
    fn ext_workspace_state(&mut self) -> &mut ExtWorkspaceManagerState {
        &mut self.ext_workspace_state
    }
}

#[derive(Debug)]
pub struct ExtWorkspaceManagerState {
    pub global: GlobalId,
    pub instances: Vec<ExtWorkspaceManagerV1>,
    pub workspace_handles: HashMap<WorkspaceId, Vec<ExtWorkspaceHandleV1>>,
}

impl ExtWorkspaceManagerState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: ExtWorkspaceHandler + GlobalDispatch<ExtWorkspaceManagerV1, GlobalData> + 'static,
    {
        let global = dh.create_global::<D, ExtWorkspaceManagerV1, _>(1, GlobalData);
        Self {
            global,
            instances: Vec::new(),
            workspace_handles: HashMap::new(),
        }
    }

    pub fn sync_workspaces(&mut self, dh: &DisplayHandle, layout: &glide_layout::Layout) {
        let current_ids = layout.workspace_ids();
        let active_ws = layout.active_workspace();

        // 1. Remove workspaces that no longer exist
        self.workspace_handles.retain(|id, handles| {
            if current_ids.contains(id) {
                true
            } else {
                for handle in handles {
                    handle.removed();
                }
                false
            }
        });

        // 2. Create missing workspaces and update states
        for (idx, &ws_id) in current_ids.iter().enumerate() {
            let is_active = ws_id == active_ws;
            let state = if is_active {
                ProtocolWorkspaceState::Active
            } else {
                ProtocolWorkspaceState::Hidden
            };

            let handles = self.workspace_handles.entry(ws_id).or_default();

            for manager in &self.instances {
                let Some(client) = manager.client() else {
                    continue;
                };
                let has_handle = handles.iter().any(|h| h.client() == Some(client.clone()));

                if !has_handle {
                    if let Ok(group) = client
                        .create_resource::<ExtWorkspaceGroupHandleV1, _, GlideState>(
                            dh,
                            manager.version(),
                            (),
                        )
                    {
                        manager.workspace_group(&group);
                        if let Ok(handle) = client
                            .create_resource::<ExtWorkspaceHandleV1, _, GlideState>(
                                dh,
                                manager.version(),
                                ws_id,
                            )
                        {
                            group.workspace_enter(&handle);
                            handle.id(format!("{ws_id:?}"));
                            handle.name(format!("Workspace {}", idx + 1));
                            handle.state(state);
                            handles.push(handle);
                        }
                    }
                }
            }

            // Update all handles
            for handle in handles {
                handle.state(state);
            }
        }

        // Clean up empty entries
        self.workspace_handles
            .retain(|_, handles| !handles.is_empty());

        // 3. Send done
        for manager in &self.instances {
            manager.done();
        }
    }
}

impl GlobalDispatch<ExtWorkspaceManagerV1, GlobalData> for GlideState {
    fn bind(
        state: &mut Self,
        dh: &DisplayHandle,
        _client: &Client,
        resource: New<ExtWorkspaceManagerV1>,
        _global_data: &GlobalData,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let instance = data_init.init(resource, GlobalData);
        state.ext_workspace_state.instances.push(instance);
        state.ext_workspace_state.sync_workspaces(dh, &state.layout);
    }
}

impl smithay::reexports::wayland_server::Dispatch<ExtWorkspaceManagerV1, GlobalData>
    for GlideState
{
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ExtWorkspaceManagerV1,
        _request: ext_workspace_manager_v1::Request,
        _data: &GlobalData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl smithay::reexports::wayland_server::Dispatch<ExtWorkspaceGroupHandleV1, ()> for GlideState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ExtWorkspaceGroupHandleV1,
        _request: ext_workspace_group_handle_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl smithay::reexports::wayland_server::Dispatch<ExtWorkspaceHandleV1, WorkspaceId>
    for GlideState
{
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ExtWorkspaceHandleV1,
        request: ext_workspace_handle_v1::Request,
        data: &WorkspaceId,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // `Activate` is the only request we act on. `Destroy` needs no
        // teardown of ours: workspaces outlive the handles clients hold on
        // them, so the protocol object going away is not a workspace going
        // away.
        if matches!(request, ext_workspace_handle_v1::Request::Activate) {
            state
                .layout
                .switch_to_workspace(glide_layout::WorkspaceTarget::Id(*data));
            state.reconcile_space();
            state
                .ext_workspace_state
                .sync_workspaces(&state.display_handle, &state.layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::reexports::wayland_server::Display;

    #[test]
    fn test_ext_workspace_manager_state_creation() {
        let display = Display::<GlideState>::new().unwrap();
        let dh = display.handle();

        let manager_state = ExtWorkspaceManagerState::new::<GlideState>(&dh);
        assert!(manager_state.instances.is_empty());
        assert!(manager_state.workspace_handles.is_empty());
    }

    #[test]
    fn test_workspace_protocol_state_values() {
        assert_ne!(
            ProtocolWorkspaceState::Active,
            ProtocolWorkspaceState::Hidden
        );
    }

    #[test]
    fn test_rapid_workspace_switching_stress() {
        let mut layout = glide_layout::Layout::new();
        let ws1 = layout.active_workspace();

        // Create 10 workspaces
        let mut ws_ids = vec![ws1];
        for _ in 0..9 {
            let win_id = glide_layout::WindowId::next();
            let new_ws = layout.add_window(win_id, glide_layout::Size::new(800, 600));
            ws_ids.push(new_ws);
        }

        // Stress switch 10,000 times rapidly between targets
        for i in 0..10_000 {
            match i % 6 {
                0 => {
                    let ok = layout.switch_to_workspace(glide_layout::WorkspaceTarget::Index(0));
                    assert!(ok);
                }
                1 => {
                    let _ = layout.switch_to_workspace(glide_layout::WorkspaceTarget::Next);
                }
                2 => {
                    let _ = layout.switch_to_workspace(glide_layout::WorkspaceTarget::Previous);
                }
                3 => {
                    let target_id = ws_ids[i % ws_ids.len()];
                    let ok =
                        layout.switch_to_workspace(glide_layout::WorkspaceTarget::Id(target_id));
                    assert!(ok);
                }
                4 => {
                    // Out of bounds index
                    let ok =
                        layout.switch_to_workspace(glide_layout::WorkspaceTarget::Index(99999));
                    assert!(!ok);
                }
                5 => {
                    // Nonexistent workspace ID
                    let bogus_id = glide_layout::WorkspaceId::next();
                    let ok =
                        layout.switch_to_workspace(glide_layout::WorkspaceTarget::Id(bogus_id));
                    assert!(!ok);
                }
                _ => unreachable!(),
            }
        }
        assert!(layout.workspace_count() > 0);
    }

    #[test]
    fn test_sync_workspaces_empty_and_multiple_instances() {
        let display = Display::<GlideState>::new().unwrap();
        let dh = display.handle();

        let mut manager_state = ExtWorkspaceManagerState::new::<GlideState>(&dh);
        let layout = glide_layout::Layout::new();

        // Calling sync_workspaces with no instances must not panic
        manager_state.sync_workspaces(&dh, &layout);
        assert!(manager_state.workspace_handles.is_empty());
    }
}
