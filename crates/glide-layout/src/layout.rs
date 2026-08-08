use std::collections::HashMap;

use crate::ids::{OutputId, WindowId, WorkspaceId};
use glide_geom::{Insets, Point, Rectangle, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewWindowPolicy {
    #[default]
    NewWorkspace,
    ActiveWorkspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Index(usize),
    Id(WorkspaceId),
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub id: OutputId,
    pub geometry: Rectangle,
    pub work_area: Rectangle,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "maximized/fullscreen/minimized are genuinely orthogonal"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowState {
    pub id: WindowId,
    pub workspace_id: WorkspaceId,
    pub geometry: Rectangle,
    pub saved_geometry: Option<Rectangle>,
    pub is_placed: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub minimized: bool,
    pub insets: Insets,
}

impl WindowState {
    #[must_use]
    pub const fn frame_geometry(&self) -> Rectangle {
        self.geometry
    }

    #[must_use]
    pub fn content_geometry(&self) -> Rectangle {
        if self.fullscreen {
            self.geometry
        } else {
            self.geometry.inset(self.insets)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub windows: Vec<WindowId>,
    pub focused: Option<WindowId>,
}

impl Workspace {
    fn new(id: WorkspaceId) -> Self {
        Self {
            id,
            windows: Vec::new(),
            focused: None,
        }
    }
}

pub const FALLBACK_OUTPUT: Rectangle = Rectangle::from_loc_and_size(0, 0, 1920, 1080);

const CASCADE_STEP: i32 = 32;

#[derive(Debug, Clone)]
pub struct Layout {
    workspaces: Vec<Workspace>,
    active: WorkspaceId,
    windows: HashMap<WindowId, WindowState>,
    focused: Option<WindowId>,
    outputs: Vec<OutputInfo>,
    policy: NewWindowPolicy,
    overview_active: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    #[must_use]
    pub fn new() -> Self {
        let initial_ws = WorkspaceId::next();
        Self {
            workspaces: vec![Workspace::new(initial_ws)],
            active: initial_ws,
            windows: HashMap::new(),
            focused: None,
            outputs: Vec::new(),
            policy: NewWindowPolicy::NewWorkspace,
            overview_active: false,
        }
    }

    #[must_use]
    pub fn primary_output_geometry(&self) -> Rectangle {
        self.outputs
            .first()
            .map_or(FALLBACK_OUTPUT, |output| output.geometry)
    }

    #[must_use]
    pub fn primary_output_work_area(&self) -> Rectangle {
        self.outputs
            .first()
            .map_or(FALLBACK_OUTPUT, |output| output.work_area)
    }

    pub fn set_policy(&mut self, policy: NewWindowPolicy) {
        self.policy = policy;
    }

    #[must_use]
    pub const fn policy(&self) -> NewWindowPolicy {
        self.policy
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces.iter()
    }

    #[must_use]
    pub const fn active_workspace(&self) -> WorkspaceId {
        self.active
    }

    #[must_use]
    pub fn workspace_ids(&self) -> Vec<WorkspaceId> {
        self.workspaces.iter().map(|ws| ws.id).collect()
    }

    #[must_use]
    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    #[must_use]
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    #[must_use]
    pub const fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    #[must_use]
    pub fn is_focused(&self, w: WindowId) -> bool {
        self.focused == Some(w)
    }

    #[must_use]
    pub fn is_visible(&self, w: WindowId) -> bool {
        self.windows
            .get(&w)
            .is_some_and(|state| state.workspace_id == self.active && !state.minimized)
    }

    #[must_use]
    pub fn window_state(&self, w: WindowId) -> Option<&WindowState> {
        self.windows.get(&w)
    }

    pub fn visible_windows(&self) -> impl Iterator<Item = WindowState> {
        let mut ordered_windows = Vec::new();
        if let Some(ws) = self.workspaces.iter().find(|ws| ws.id == self.active) {
            for id in &ws.windows {
                if let Some(state) = self.windows.get(id) {
                    if !state.minimized {
                        ordered_windows.push(state.clone());
                    }
                }
            }
        }
        ordered_windows.into_iter()
    }

    #[must_use]
    pub const fn overview_active(&self) -> bool {
        self.overview_active
    }

    pub fn open_overview(&mut self) -> bool {
        if self.overview_active {
            return false;
        }
        self.overview_active = true;
        true
    }

    pub fn close_overview(&mut self) -> bool {
        if !self.overview_active {
            return false;
        }
        self.overview_active = false;
        true
    }

    #[must_use]
    pub fn overview_windows(&self) -> Vec<WindowState> {
        let mut windows = Vec::new();
        for ws in &self.workspaces {
            for id in &ws.windows {
                if let Some(state) = self.windows.get(id) {
                    if !state.minimized {
                        windows.push(state.clone());
                    }
                }
            }
        }
        windows
    }

    pub fn raise_window(&mut self, w: WindowId) -> bool {
        let Some(ws_id) = self.windows.get(&w).map(|s| s.workspace_id) else {
            return false;
        };

        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == ws_id) {
            ws.windows.retain(|&id| id != w);
            ws.windows.push(w);
        }
        true
    }

    pub fn outputs_changed(&mut self, outputs: &[OutputInfo]) {
        self.outputs = outputs.to_vec();
        self.rescue_offscreen_windows();
    }

    #[must_use]
    pub fn outputs(&self) -> &[OutputInfo] {
        &self.outputs
    }

    #[must_use]
    pub fn resolve_target(&self, target: WorkspaceTarget) -> Option<WorkspaceId> {
        match target {
            WorkspaceTarget::Id(id) => {
                if self.workspaces.iter().any(|ws| ws.id == id) {
                    Some(id)
                } else {
                    None
                }
            }
            WorkspaceTarget::Index(idx) => self.workspaces.get(idx).map(|ws| ws.id),
            WorkspaceTarget::Next => {
                let current_idx = self.workspaces.iter().position(|ws| ws.id == self.active)?;
                if current_idx + 1 < self.workspaces.len() {
                    Some(self.workspaces[current_idx + 1].id)
                } else {
                    None
                }
            }
            WorkspaceTarget::Previous => {
                let current_idx = self.workspaces.iter().position(|ws| ws.id == self.active)?;
                if current_idx > 0 {
                    Some(self.workspaces[current_idx - 1].id)
                } else {
                    None
                }
            }
        }
    }

    pub fn switch_to_workspace(&mut self, target: WorkspaceTarget) -> bool {
        let Some(target_id) = self.resolve_target(target) else {
            return false;
        };

        if self.active == target_id {
            return true;
        }

        self.remember_focus();
        self.active = target_id;
        self.focused = self.pick_new_focus();
        true
    }

    pub fn move_window_to_workspace(&mut self, w: WindowId, target: WorkspaceTarget) -> bool {
        let Some(target_id) = self.resolve_target(target) else {
            return false;
        };

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        let old_ws_id = state.workspace_id;
        if old_ws_id == target_id {
            return true;
        }

        state.workspace_id = target_id;

        if let Some(old_ws) = self.workspaces.iter_mut().find(|ws| ws.id == old_ws_id) {
            old_ws.windows.retain(|&id| id != w);
            if old_ws.focused == Some(w) {
                old_ws.focused = None;
            }
        }

        if let Some(target_ws) = self.workspaces.iter_mut().find(|ws| ws.id == target_id) {
            target_ws.windows.push(w);
            target_ws.focused = Some(w);
        }

        if self.workspace_is_empty(old_ws_id) && self.workspaces.len() > 1 {
            self.remove_workspace_if_empty(old_ws_id);
        }

        if self.focused == Some(w) && self.active != target_id {
            self.focused = self.pick_new_focus();
        }

        true
    }

    pub fn add_window(&mut self, w: WindowId, initial_size: Size) -> WorkspaceId {
        let ws_id = match self.policy {
            NewWindowPolicy::ActiveWorkspace => self.active,
            NewWindowPolicy::NewWorkspace => {
                if self.workspace_is_empty(self.active) {
                    self.active
                } else {
                    let new_ws = WorkspaceId::next();
                    let active_idx = self
                        .workspaces
                        .iter()
                        .position(|ws| ws.id == self.active)
                        .unwrap_or(0);
                    self.remember_focus();
                    self.workspaces
                        .insert(active_idx + 1, Workspace::new(new_ws));
                    self.active = new_ws;
                    new_ws
                }
            }
        };

        let is_placed = !initial_size.is_empty();
        let index_on_ws = self
            .workspaces
            .iter()
            .find(|ws| ws.id == ws_id)
            .map_or(0, |ws| ws.windows.len());

        let geometry = if is_placed {
            self.calculate_centered_geometry(initial_size, Insets::NONE, index_on_ws)
        } else {
            Rectangle::new(Point::ZERO, initial_size)
        };

        let state = WindowState {
            id: w,
            workspace_id: ws_id,
            geometry,
            saved_geometry: None,
            is_placed,
            maximized: false,
            fullscreen: false,
            minimized: false,
            insets: Insets::NONE,
        };

        self.windows.insert(w, state);

        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == ws_id) {
            ws.windows.push(w);
        }

        self.focused = Some(w);
        self.remember_focus();
        ws_id
    }

    pub fn add_window_with_parent(
        &mut self,
        w: WindowId,
        initial_size: Size,
        parent: WindowId,
    ) -> WorkspaceId {
        let target_ws = self
            .windows
            .get(&parent)
            .map_or(self.active, |state| state.workspace_id);

        let is_placed = !initial_size.is_empty();
        let index_on_ws = self
            .workspaces
            .iter()
            .find(|ws| ws.id == target_ws)
            .map_or(0, |ws| ws.windows.len());

        let geometry = if is_placed {
            self.calculate_centered_geometry(initial_size, Insets::NONE, index_on_ws)
        } else {
            Rectangle::new(Point::ZERO, initial_size)
        };

        let state = WindowState {
            id: w,
            workspace_id: target_ws,
            geometry,
            saved_geometry: None,
            is_placed,
            maximized: false,
            fullscreen: false,
            minimized: false,
            insets: Insets::NONE,
        };

        self.windows.insert(w, state);

        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == target_ws) {
            ws.windows.push(w);
            ws.focused = Some(w);
        }

        if target_ws == self.active {
            self.focused = Some(w);
        }

        target_ws
    }

    pub fn set_maximized(&mut self, w: WindowId, maximized: bool) -> bool {
        let work_area = self.primary_output_work_area();

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.maximized == maximized {
            return false;
        }

        if maximized {
            state.saved_geometry = Some(state.geometry);
            state.geometry = work_area;
            state.maximized = true;
        } else {
            if let Some(saved) = state.saved_geometry.take() {
                state.geometry = saved;
            }
            state.maximized = false;
        }
        true
    }

    pub fn set_fullscreen(&mut self, w: WindowId, fullscreen: bool) -> bool {
        let output_geom = self.primary_output_geometry();
        let work_area = self.primary_output_work_area();

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.fullscreen == fullscreen {
            return false;
        }

        if fullscreen {
            if !state.maximized && state.saved_geometry.is_none() {
                state.saved_geometry = Some(state.geometry);
            }
            state.geometry = output_geom;
            state.fullscreen = true;
        } else {
            if state.maximized {
                state.geometry = work_area;
            } else if let Some(saved) = state.saved_geometry.take() {
                state.geometry = saved;
            }
            state.fullscreen = false;
        }
        true
    }

    pub fn toggle_maximized(&mut self, w: WindowId) -> bool {
        let is_max = self.windows.get(&w).is_some_and(|s| s.maximized);
        self.set_maximized(w, !is_max)
    }

    pub fn toggle_fullscreen(&mut self, w: WindowId) -> bool {
        let is_full = self.windows.get(&w).is_some_and(|s| s.fullscreen);
        self.set_fullscreen(w, !is_full)
    }

    pub fn set_minimized(&mut self, w: WindowId, minimized: bool) -> bool {
        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.minimized == minimized {
            return false;
        }
        state.minimized = minimized;

        if minimized && self.focused == Some(w) {
            self.focused = self.pick_new_focus();
            self.remember_focus();
        }
        true
    }

    pub fn toggle_minimize(&mut self, w: WindowId) -> bool {
        let is_min = self.windows.get(&w).is_some_and(|s| s.minimized);
        self.set_minimized(w, !is_min)
    }

    pub fn update_window_size(&mut self, w: WindowId, size: Size) -> bool {
        let Some(state) = self.windows.get(&w) else {
            return false;
        };

        if state.is_placed || size.is_empty() {
            let insets = state.insets;
            if let Some(state) = self.windows.get_mut(&w) {
                state.geometry.size =
                    Size::new(size.width + insets.width(), size.height + insets.height());
            }
            return false;
        }

        let ws_id = state.workspace_id;
        let insets = state.insets;
        let index_on_ws = self
            .workspaces
            .iter()
            .find(|ws| ws.id == ws_id)
            .and_then(|ws| ws.windows.iter().position(|&id| id == w))
            .unwrap_or(0);

        let geometry = self.calculate_centered_geometry(size, insets, index_on_ws);
        if let Some(state) = self.windows.get_mut(&w) {
            state.geometry = geometry;
            state.is_placed = true;
        }
        true
    }

    pub fn set_window_insets(&mut self, w: WindowId, insets: Insets) -> bool {
        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.is_placed && !state.geometry.size.is_empty() {
            let content = state.geometry.inset(state.insets);
            state.geometry = Rectangle::new(
                Point::new(content.loc.x - insets.left, content.loc.y - insets.top),
                Size::new(
                    content.size.width + insets.width(),
                    content.size.height + insets.height(),
                ),
            );
        }

        state.insets = insets;
        true
    }

    pub fn remove_window(&mut self, w: WindowId) -> bool {
        let Some(state) = self.windows.remove(&w) else {
            return false;
        };

        let ws_id = state.workspace_id;

        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == ws_id) {
            ws.windows.retain(|&id| id != w);
            if ws.focused == Some(w) {
                ws.focused = None;
            }
        }

        if self.workspace_is_empty(ws_id) && self.workspaces.len() > 1 {
            self.remove_workspace_if_empty(ws_id);
        }

        if self.focused == Some(w) {
            self.focused = self.pick_new_focus();
        }

        true
    }

    pub fn focus_window(&mut self, w: WindowId) -> bool {
        let Some(target_ws) = self.windows.get(&w).map(|state| state.workspace_id) else {
            return false;
        };

        if self.active != target_ws {
            self.remember_focus();
            self.active = target_ws;
        }

        if let Some(state) = self.windows.get_mut(&w) {
            state.minimized = false;
        }

        self.focused = Some(w);
        self.raise_window(w);
        self.remember_focus();
        true
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
        self.remember_focus();
    }

    pub fn set_window_geometry(&mut self, w: WindowId, geometry: Rectangle) -> bool {
        if let Some(state) = self.windows.get_mut(&w) {
            state.geometry = geometry;
            state.is_placed = true;
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn calculate_centered_geometry(
        &self,
        content_size: Size,
        insets: Insets,
        index_on_ws: usize,
    ) -> Rectangle {
        let work_area = self.primary_output_work_area();
        let frame_size = Size::new(
            content_size.width + insets.width(),
            content_size.height + insets.height(),
        );

        let step = i32::try_from(index_on_ws % 8).unwrap_or(0);
        let offset = step * CASCADE_STEP;
        let x = work_area.loc.x + (work_area.size.width - frame_size.width) / 2 + offset;
        let y = work_area.loc.y + (work_area.size.height - frame_size.height) / 2 + offset;

        Rectangle::new(Point::new(x, y), frame_size)
    }

    fn rescue_offscreen_windows(&mut self) {
        let outputs: Vec<Rectangle> = if self.outputs.is_empty() {
            vec![FALLBACK_OUTPUT]
        } else {
            self.outputs.iter().map(|o| o.geometry).collect()
        };

        let stranded: Vec<WindowId> = self
            .windows
            .values()
            .filter(|state| state.is_placed && !state.geometry.size.is_empty())
            .filter(|state| !outputs.iter().any(|rect| rect.overlaps(state.geometry)))
            .map(|state| state.id)
            .collect();

        for id in stranded {
            let Some(size) = self.windows.get(&id).map(|s| s.geometry.size) else {
                continue;
            };
            let geometry = self.calculate_centered_geometry(size, Insets::NONE, 0);
            if let Some(state) = self.windows.get_mut(&id) {
                state.geometry = geometry;
            }
        }

        let work_area = self.primary_output_work_area();
        let output_geom = self.primary_output_geometry();
        for state in self.windows.values_mut() {
            if state.fullscreen {
                state.geometry = output_geom;
            } else if state.maximized {
                state.geometry = work_area;
            }
        }
    }

    fn workspace_is_empty(&self, ws_id: WorkspaceId) -> bool {
        self.workspaces
            .iter()
            .find(|ws| ws.id == ws_id)
            .is_none_or(|ws| ws.windows.is_empty())
    }

    fn remove_workspace_if_empty(&mut self, ws_id: WorkspaceId) {
        if let Some(idx) = self.workspaces.iter().position(|ws| ws.id == ws_id) {
            if self.workspaces[idx].windows.is_empty() && self.workspaces.len() > 1 {
                self.workspaces.remove(idx);

                if self.active == ws_id {
                    let next_idx = if idx < self.workspaces.len() {
                        idx
                    } else {
                        idx.saturating_sub(1)
                    };
                    self.active = self.workspaces[next_idx].id;
                }
            }
        }
    }

    fn remember_focus(&mut self) {
        let focused = self.focused;
        let active = self.active;
        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == active) {
            ws.focused = focused;
        }
    }

    fn pick_new_focus(&self) -> Option<WindowId> {
        let active_ws = self.workspaces.iter().find(|ws| ws.id == self.active)?;
        let visible = |id: WindowId| self.windows.get(&id).is_some_and(|state| !state.minimized);

        active_ws
            .focused
            .filter(|&id| active_ws.windows.contains(&id) && visible(id))
            .or_else(|| {
                active_ws
                    .windows
                    .iter()
                    .rev()
                    .copied()
                    .find(|&id| visible(id))
            })
    }
}
