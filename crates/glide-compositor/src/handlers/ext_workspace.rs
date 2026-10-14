use std::collections::HashMap;

use glide_layout::WorkspaceId;
use smithay::desktop::Space;
use smithay::reexports::wayland_protocols::ext::workspace::v1::server::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1, GroupCapabilities},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1, State, WorkspaceCapabilities},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, GlobalDispatch, New, Resource, backend::GlobalId,
};
use smithay::wayland::GlobalData;

use crate::{GlideState, window::GlideWindow};

pub trait ExtWorkspaceHandler {
    fn ext_workspace_state(&mut self) -> &mut ExtWorkspaceManagerState;
}

impl ExtWorkspaceHandler for GlideState {
    fn ext_workspace_state(&mut self) -> &mut ExtWorkspaceManagerState {
        &mut self.protocols.ext_workspace_state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub id: WorkspaceId,
    pub index: u32,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Removed(WorkspaceId),
    Added(Shown),
    Moved { id: WorkspaceId, index: u32 },
    Activated { id: WorkspaceId, active: bool },
}

#[must_use]
pub fn snapshot(layout: &glide_layout::Layout) -> Vec<Shown> {
    let active = layout.active_workspace();
    layout
        .workspace_ids()
        .into_iter()
        .zip(0u32..)
        .map(|(id, index)| Shown { id, index, active: id == active })
        .collect()
}

#[must_use]
pub fn changes(sent: &[Shown], now: &[Shown]) -> Vec<Change> {
    let mut changes: Vec<Change> =
        sent.iter().filter(|old| !now.iter().any(|new| new.id == old.id)).map(|old| Change::Removed(old.id)).collect();
    for new in now {
        let Some(old) = sent.iter().find(|old| old.id == new.id) else {
            changes.push(Change::Added(*new));
            continue;
        };
        if old.index != new.index {
            changes.push(Change::Moved { id: new.id, index: new.index });
        }
        if old.active != new.active {
            changes.push(Change::Activated { id: new.id, active: new.active });
        }
    }
    changes
}

fn state_of(active: bool) -> State {
    if active { State::Active } else { State::empty() }
}

fn describe(handle: &ExtWorkspaceHandleV1, index: u32) {
    handle.name((index + 1).to_string());
    handle.coordinates(index.to_ne_bytes().to_vec());
}

#[derive(Debug)]
struct Manager {
    resource: ExtWorkspaceManagerV1,
    group: ExtWorkspaceGroupHandleV1,
    outputs: Vec<WlOutput>,
    handles: HashMap<WorkspaceId, ExtWorkspaceHandleV1>,
    sent: Vec<Shown>,
    pending: Option<WorkspaceId>,
}

impl Manager {
    fn sync_outputs(&mut self, space: &Space<GlideWindow>) -> bool {
        let Some(client) = self.resource.client() else {
            return false;
        };
        let mut now = Vec::new();
        for output in space.outputs() {
            now.extend(output.client_outputs(&client));
        }
        let mut changed = false;
        for output in now.iter().filter(|output| !self.outputs.contains(output)) {
            self.group.output_enter(output);
            changed = true;
        }
        for output in self.outputs.iter().filter(|output| !now.contains(output) && output.is_alive()) {
            self.group.output_leave(output);
            changed = true;
        }
        self.outputs = now;
        changed
    }

    fn send(&mut self, dh: &DisplayHandle, change: Change) {
        match change {
            Change::Removed(id) => {
                if let Some(handle) = self.handles.remove(&id) {
                    self.group.workspace_leave(&handle);
                    handle.removed();
                }
            }
            Change::Added(shown) => {
                let Some(client) = self.resource.client() else {
                    return;
                };
                let Ok(handle) = client.create_resource::<ExtWorkspaceHandleV1, _, GlideState>(
                    dh,
                    self.resource.version(),
                    shown.id,
                ) else {
                    return;
                };
                self.resource.workspace(&handle);
                handle.id(shown.id.as_u64().to_string());
                describe(&handle, shown.index);
                handle.capabilities(WorkspaceCapabilities::Activate);
                handle.state(state_of(shown.active));
                self.group.workspace_enter(&handle);
                self.handles.insert(shown.id, handle);
            }
            Change::Moved { id, index } => {
                if let Some(handle) = self.handles.get(&id) {
                    describe(handle, index);
                }
            }
            Change::Activated { id, active } => {
                if let Some(handle) = self.handles.get(&id) {
                    handle.state(state_of(active));
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct ExtWorkspaceManagerState {
    pub global: GlobalId,
    managers: Vec<Manager>,
}

impl ExtWorkspaceManagerState {
    pub fn new<D>(dh: &DisplayHandle) -> Self
    where
        D: ExtWorkspaceHandler + GlobalDispatch<ExtWorkspaceManagerV1, GlobalData> + 'static,
    {
        let global = dh.create_global::<D, ExtWorkspaceManagerV1, _>(1, GlobalData);
        Self { global, managers: Vec::new() }
    }

    pub fn sync_workspaces(&mut self, dh: &DisplayHandle, layout: &glide_layout::Layout, space: &Space<GlideWindow>) {
        self.managers.retain(|manager| manager.resource.is_alive());
        let now = snapshot(layout);
        for manager in &mut self.managers {
            let outputs = manager.sync_outputs(space);
            let changes = changes(&manager.sent, &now);
            for change in &changes {
                manager.send(dh, *change);
            }
            manager.sent.clone_from(&now);
            if outputs || !changes.is_empty() {
                manager.resource.done();
            }
        }
    }

    fn manager_mut(&mut self, resource: &ExtWorkspaceManagerV1) -> Option<&mut Manager> {
        self.managers.iter_mut().find(|manager| &manager.resource == resource)
    }
}

impl GlobalDispatch<ExtWorkspaceManagerV1, GlobalData> for GlideState {
    fn bind(
        state: &mut Self, dh: &DisplayHandle, client: &Client, resource: New<ExtWorkspaceManagerV1>,
        _global_data: &GlobalData, data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, GlobalData);
        let Ok(group) = client.create_resource::<ExtWorkspaceGroupHandleV1, _, Self>(dh, manager.version(), ()) else {
            return;
        };
        manager.workspace_group(&group);
        group.capabilities(GroupCapabilities::empty());
        state.protocols.ext_workspace_state.managers.push(Manager {
            resource: manager,
            group,
            outputs: Vec::new(),
            handles: HashMap::new(),
            sent: Vec::new(),
            pending: None,
        });
        state.sync_ext_workspaces();
    }
}

impl smithay::reexports::wayland_server::Dispatch<ExtWorkspaceManagerV1, GlobalData> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, resource: &ExtWorkspaceManagerV1,
        request: ext_workspace_manager_v1::Request, _data: &GlobalData, _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ext_workspace_manager_v1::Request::Commit => {
                let pending = state
                    .protocols
                    .ext_workspace_state
                    .manager_mut(resource)
                    .and_then(|manager| manager.pending.take());
                if let Some(id) = pending
                    && id != state.layout.active_workspace()
                {
                    state.switch_workspace_animated(glide_layout::WorkspaceTarget::Id(id));
                    state.sync_ext_workspaces();
                }
            }
            ext_workspace_manager_v1::Request::Stop => {
                state.protocols.ext_workspace_state.managers.retain(|manager| &manager.resource != resource);
                resource.finished();
            }
            _ => {}
        }
    }
}

impl smithay::reexports::wayland_server::Dispatch<ExtWorkspaceGroupHandleV1, ()> for GlideState {
    fn request(
        _state: &mut Self, _client: &Client, _resource: &ExtWorkspaceGroupHandleV1,
        _request: ext_workspace_group_handle_v1::Request, _data: &(), _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl smithay::reexports::wayland_server::Dispatch<ExtWorkspaceHandleV1, WorkspaceId> for GlideState {
    fn request(
        state: &mut Self, _client: &Client, resource: &ExtWorkspaceHandleV1, request: ext_workspace_handle_v1::Request,
        data: &WorkspaceId, _dhandle: &DisplayHandle, _data_init: &mut DataInit<'_, Self>,
    ) {
        if !matches!(request, ext_workspace_handle_v1::Request::Activate) {
            return;
        }
        if let Some(manager) = state
            .protocols
            .ext_workspace_state
            .managers
            .iter_mut()
            .find(|manager| manager.handles.get(data) == Some(resource))
        {
            manager.pending = Some(*data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::reexports::wayland_server::Display;

    fn shown(id: u64, index: u32, active: bool) -> Shown {
        Shown { id: WorkspaceId(id), index, active }
    }

    #[test]
    fn test_ext_workspace_manager_state_creation() {
        let display = Display::<GlideState>::new().unwrap();
        let dh = display.handle();

        let manager_state = ExtWorkspaceManagerState::new::<GlideState>(&dh);
        assert!(manager_state.managers.is_empty());
    }

    #[test]
    fn an_inactive_workspace_is_not_hidden_from_pagers() {
        assert_eq!(state_of(false), State::empty());
        assert_eq!(state_of(true), State::Active);
    }

    #[test]
    fn a_fresh_layout_shows_one_active_workspace() {
        let layout = glide_layout::Layout::new();
        let now = snapshot(&layout);
        assert_eq!(now, vec![Shown { id: layout.active_workspace(), index: 0, active: true }]);
        assert_eq!(changes(&[], &now), vec![Change::Added(now[0])]);
    }

    #[test]
    fn an_unchanged_layout_sends_nothing() {
        let now = vec![shown(1, 0, true), shown(2, 1, false)];
        assert!(changes(&now, &now).is_empty());
    }

    #[test]
    fn removals_come_first_and_shifted_workspaces_are_renamed() {
        let sent = vec![shown(1, 0, false), shown(2, 1, true), shown(3, 2, false)];
        let now = vec![shown(2, 0, true), shown(3, 1, false), shown(4, 2, false)];
        assert_eq!(
            changes(&sent, &now),
            vec![
                Change::Removed(WorkspaceId(1)),
                Change::Moved { id: WorkspaceId(2), index: 0 },
                Change::Moved { id: WorkspaceId(3), index: 1 },
                Change::Added(shown(4, 2, false)),
            ]
        );
    }

    #[test]
    fn switching_flips_both_states() {
        let sent = vec![shown(1, 0, true), shown(2, 1, false)];
        let now = vec![shown(1, 0, false), shown(2, 1, true)];
        assert_eq!(
            changes(&sent, &now),
            vec![
                Change::Activated { id: WorkspaceId(1), active: false },
                Change::Activated { id: WorkspaceId(2), active: true },
            ]
        );
    }

    #[test]
    fn test_rapid_workspace_switching_stress() {
        let mut layout = glide_layout::Layout::new();
        let ws1 = layout.active_workspace();

        let mut ws_ids = vec![ws1];
        for _ in 0..9 {
            let win_id = glide_layout::WindowId::next();
            let new_ws = layout.add_window(win_id, glide_layout::Size::new(800, 600));
            ws_ids.push(new_ws);
        }

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
                    let ok = layout.switch_to_workspace(glide_layout::WorkspaceTarget::Id(target_id));
                    assert!(ok);
                }
                4 => {
                    let ok = layout.switch_to_workspace(glide_layout::WorkspaceTarget::Index(99999));
                    assert!(!ok);
                }
                5 => {
                    let bogus_id = glide_layout::WorkspaceId::next();
                    let ok = layout.switch_to_workspace(glide_layout::WorkspaceTarget::Id(bogus_id));
                    assert!(!ok);
                }
                _ => unreachable!(),
            }
        }
        assert!(layout.workspace_count() > 0);
    }
}
