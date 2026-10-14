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
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub id: OutputId,
    pub geometry: Rectangle,
    pub work_area: Rectangle,
}

#[expect(clippy::struct_excessive_bools, reason = "maximized/fullscreen/minimized are genuinely orthogonal")]
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
        if self.fullscreen { self.geometry } else { self.geometry.inset(self.insets) }
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
        Self { id, windows: Vec::new(), focused: None }
    }
}

pub const FALLBACK_OUTPUT: Rectangle = Rectangle::from_loc_and_size(0, 0, 1920, 1080);

const CASCADE_STEP: i32 = 32;

#[must_use]
pub fn keep_reachable(frame: Rectangle, area: Rectangle) -> Rectangle {
    let max_x = (area.loc.x + area.size.width - frame.size.width).max(area.loc.x);
    let max_y = (area.loc.y + area.size.height - frame.size.height).max(area.loc.y);
    Rectangle::new(Point::new(frame.loc.x.clamp(area.loc.x, max_x), frame.loc.y.clamp(area.loc.y, max_y)), frame.size)
}

pub const MOVE_MIN_VISIBLE_WIDTH: i32 = 64;

pub const MOVE_MIN_GRIP_HEIGHT: i32 = 32;

#[must_use]
pub fn keep_grip_reachable(frame: Rectangle, grip_height: i32, area: Rectangle) -> Rectangle {
    let grip_height = grip_height.max(MOVE_MIN_GRIP_HEIGHT).min(frame.size.height).max(1);
    let visible = MOVE_MIN_VISIBLE_WIDTH.min(frame.size.width).max(1);
    let overhang = frame.size.width - visible;
    let grip = Rectangle::new(frame.loc, Size::new(frame.size.width, grip_height));
    let reach = Rectangle::new(
        Point::new(area.loc.x - overhang, area.loc.y),
        Size::new(area.size.width + overhang * 2, area.size.height),
    );
    Rectangle::new(keep_reachable(grip, reach).loc, frame.size)
}

#[derive(Debug, Clone)]
pub struct Layout {
    workspaces: Vec<Workspace>,
    active: WorkspaceId,
    windows: HashMap<WindowId, WindowState>,
    focused: Option<WindowId>,
    outputs: Vec<OutputInfo>,
    policy: NewWindowPolicy,
    overview_active: bool,
    placement_anchor: Option<Point>,
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
            placement_anchor: None,
        }
    }

    pub fn set_placement_anchor(&mut self, point: Point) {
        self.placement_anchor = Some(point);
    }

    #[must_use]
    pub fn placement_work_area(&self) -> Rectangle {
        self.placement_anchor.map_or_else(|| self.primary_output_work_area(), |anchor| self.work_area_at(anchor))
    }

    #[must_use]
    pub fn primary_output_geometry(&self) -> Rectangle {
        self.outputs.first().map_or(FALLBACK_OUTPUT, |output| output.geometry)
    }

    #[must_use]
    pub fn primary_output_work_area(&self) -> Rectangle {
        self.outputs.first().map_or(FALLBACK_OUTPUT, |output| output.work_area)
    }

    #[must_use]
    pub fn output_at(&self, point: Point) -> Option<&OutputInfo> {
        self.outputs.iter().find(|output| output.geometry.contains(point))
    }

    #[must_use]
    pub fn work_area_at(&self, point: Point) -> Rectangle {
        self.output_at(point).map_or_else(|| self.primary_output_work_area(), |output| output.work_area)
    }

    #[must_use]
    pub fn output_geometry_at(&self, point: Point) -> Rectangle {
        self.output_at(point).map_or_else(|| self.primary_output_geometry(), |output| output.geometry)
    }

    fn output_for_window(&self, w: WindowId) -> Option<&OutputInfo> {
        let state = self.windows.get(&w)?;
        let centre = state.geometry.center();
        self.output_at(centre).or_else(|| self.outputs.iter().find(|output| output.geometry.overlaps(state.geometry)))
    }

    #[must_use]
    pub fn work_area_for_window(&self, w: WindowId) -> Rectangle {
        self.output_for_window(w).map_or_else(|| self.primary_output_work_area(), |output| output.work_area)
    }

    #[must_use]
    pub fn output_geometry_for_window(&self, w: WindowId) -> Rectangle {
        self.output_for_window(w).map_or_else(|| self.primary_output_geometry(), |output| output.geometry)
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
        self.windows.get(&w).is_some_and(|state| state.workspace_id == self.active && !state.minimized)
    }

    #[must_use]
    pub fn window_state(&self, w: WindowId) -> Option<&WindowState> {
        self.windows.get(&w)
    }

    pub fn visible_windows(&self) -> impl Iterator<Item = WindowState> {
        self.shown_windows_of(self.active).into_iter()
    }

    #[must_use]
    pub fn workspace_windows_at(&self, index: usize) -> Vec<WindowState> {
        self.workspaces.get(index).map(|ws| self.shown_windows_of(ws.id)).unwrap_or_default()
    }

    fn shown_windows_of(&self, workspace: WorkspaceId) -> Vec<WindowState> {
        self.workspace(workspace)
            .into_iter()
            .flat_map(|ws| ws.windows.iter())
            .filter_map(|id| self.windows.get(id))
            .filter(|state| !state.minimized)
            .cloned()
            .collect()
    }

    #[must_use]
    pub const fn overview_active(&self) -> bool {
        self.overview_active
    }

    pub const fn open_overview(&mut self) -> bool {
        !std::mem::replace(&mut self.overview_active, true)
    }

    pub const fn close_overview(&mut self) -> bool {
        std::mem::replace(&mut self.overview_active, false)
    }

    #[must_use]
    pub fn overview_windows(&self) -> Vec<WindowState> {
        self.workspaces
            .iter()
            .flat_map(|ws| ws.windows.iter())
            .filter_map(|id| self.windows.get(id))
            .filter(|state| !state.minimized)
            .cloned()
            .collect()
    }

    fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|ws| ws.id == id)
    }

    fn workspace_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|ws| ws.id == id)
    }

    fn active_index(&self) -> Option<usize> {
        self.workspaces.iter().position(|ws| ws.id == self.active)
    }

    pub fn raise_window(&mut self, w: WindowId) -> bool {
        let Some(ws_id) = self.windows.get(&w).map(|s| s.workspace_id) else {
            return false;
        };

        if let Some(ws) = self.workspace_mut(ws_id) {
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
            WorkspaceTarget::Id(id) => self.workspace(id).map(|ws| ws.id),
            WorkspaceTarget::Index(idx) => self.workspaces.get(idx).map(|ws| ws.id),
            WorkspaceTarget::Next => self.workspaces.get(self.active_index()? + 1).map(|ws| ws.id),
            WorkspaceTarget::Previous => self.workspaces.get(self.active_index()?.checked_sub(1)?).map(|ws| ws.id),
            WorkspaceTarget::New => self.workspaces.last().filter(|ws| ws.windows.is_empty()).map(|ws| ws.id),
        }
    }

    #[must_use]
    pub fn last_workspace_is_empty(&self) -> bool {
        self.workspaces.last().is_some_and(|ws| ws.windows.is_empty())
    }

    fn resolve_or_create(&mut self, target: WorkspaceTarget) -> Option<WorkspaceId> {
        self.resolve_target(target).or_else(|| {
            (target == WorkspaceTarget::New).then(|| {
                let id = WorkspaceId::next();
                self.workspaces.push(Workspace::new(id));
                id
            })
        })
    }

    fn leave_workspace(&mut self, left: WorkspaceId) {
        if left != self.active {
            self.remove_workspace_if_empty(left);
        }
    }

    pub fn switch_to_workspace(&mut self, target: WorkspaceTarget) -> bool {
        let Some(target_id) = self.resolve_or_create(target) else {
            return false;
        };

        if self.active == target_id {
            return true;
        }

        self.remember_focus();
        let left = self.active;
        self.active = target_id;
        self.leave_workspace(left);
        self.focused = self.pick_new_focus();
        true
    }

    pub fn move_window_to_workspace(&mut self, w: WindowId, target: WorkspaceTarget) -> bool {
        if !self.windows.contains_key(&w) {
            return false;
        }
        let Some(target_id) = self.resolve_or_create(target) else {
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

        self.detach(w, old_ws_id);

        if let Some(target_ws) = self.workspace_mut(target_id) {
            target_ws.windows.push(w);
            target_ws.focused = Some(w);
        }

        let old_ws_vanishes = self.workspace_is_empty(old_ws_id) && self.workspaces.len() > 1;
        if old_ws_vanishes && self.active == old_ws_id && self.focused == Some(w) {
            self.active = target_id;
        }

        let active_changed = old_ws_vanishes && self.remove_workspace_if_empty(old_ws_id);

        if active_changed || (self.focused == Some(w) && self.active != target_id) {
            self.focused = self.pick_new_focus();
        }

        true
    }

    pub fn add_window(&mut self, w: WindowId, initial_size: Size) -> WorkspaceId {
        let ws_id = match self.policy {
            NewWindowPolicy::NewWorkspace if !self.workspace_is_empty(self.active) => {
                let new_ws = WorkspaceId::next();
                let at = self.active_index().unwrap_or(0) + 1;
                self.remember_focus();
                self.workspaces.insert(at, Workspace::new(new_ws));
                self.active = new_ws;
                new_ws
            }
            _ => self.active,
        };
        self.insert_window(w, ws_id, initial_size)
    }

    pub fn add_window_with_parent(&mut self, w: WindowId, initial_size: Size, parent: WindowId) -> WorkspaceId {
        let ws_id = self.windows.get(&parent).map_or(self.active, |state| state.workspace_id);
        self.insert_window(w, ws_id, initial_size)
    }

    fn insert_window(&mut self, w: WindowId, ws_id: WorkspaceId, size: Size) -> WorkspaceId {
        let is_placed = !size.is_empty();
        let index_on_ws = self.workspace(ws_id).map_or(0, |ws| ws.windows.len());
        let geometry = if is_placed {
            self.calculate_centered_geometry(size, Insets::NONE, index_on_ws)
        } else {
            Rectangle::new(Point::ZERO, size)
        };
        self.windows.insert(
            w,
            WindowState {
                id: w,
                workspace_id: ws_id,
                geometry,
                saved_geometry: None,
                is_placed,
                maximized: false,
                fullscreen: false,
                minimized: false,
                insets: Insets::NONE,
            },
        );
        if let Some(ws) = self.workspace_mut(ws_id) {
            ws.windows.push(w);
            ws.focused = Some(w);
        }
        if ws_id == self.active {
            self.focused = Some(w);
        }
        ws_id
    }

    pub fn set_maximized(&mut self, w: WindowId, maximized: bool) -> bool {
        let work_area = self.work_area_for_window(w);

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.maximized == maximized {
            return false;
        }

        if state.fullscreen {
            state.maximized = maximized;
            return true;
        }

        if maximized {
            if work_area.size.is_empty() {
                return false;
            }
            state.saved_geometry = state.is_placed.then_some(state.geometry);
            state.geometry = work_area;
            state.maximized = true;
            state.is_placed = true;
        } else {
            match state.saved_geometry.take() {
                Some(saved) => state.geometry = saved,
                None => state.is_placed = false,
            }
            state.maximized = false;
        }
        true
    }

    pub fn set_fullscreen(&mut self, w: WindowId, fullscreen: bool) -> bool {
        let output_geom = self.output_geometry_for_window(w);
        let work_area = self.work_area_for_window(w);

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.fullscreen == fullscreen {
            return false;
        }

        if fullscreen {
            if output_geom.size.is_empty() {
                return false;
            }
            if !state.maximized && state.saved_geometry.is_none() {
                state.saved_geometry = state.is_placed.then_some(state.geometry);
            }
            state.geometry = output_geom;
            state.fullscreen = true;
            state.is_placed = true;
        } else {
            if state.maximized {
                state.geometry = work_area;
            } else {
                match state.saved_geometry.take() {
                    Some(saved) => state.geometry = saved,
                    None => state.is_placed = false,
                }
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

        if state.maximized || state.fullscreen {
            return false;
        }

        let insets = state.insets;
        if state.is_placed || size.is_empty() {
            if let Some(state) = self.windows.get_mut(&w) {
                state.geometry.size = size.grow(insets);
            }
            return false;
        }

        let index_on_ws =
            self.workspace(state.workspace_id).and_then(|ws| ws.windows.iter().position(|&id| id == w)).unwrap_or(0);

        let geometry = self.calculate_centered_geometry(size, insets, index_on_ws);
        if let Some(state) = self.windows.get_mut(&w) {
            state.geometry = geometry;
            state.is_placed = true;
        }
        true
    }

    pub fn set_window_insets(&mut self, w: WindowId, insets: Insets) -> bool {
        let work_area = self.work_area_for_window(w);
        let output_geom = self.output_geometry_for_window(w);

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.fullscreen {
            state.geometry = output_geom;
        } else if state.maximized {
            state.geometry = work_area;
        } else if state.is_placed && !state.geometry.size.is_empty() {
            let grown = state.geometry.inset(state.insets).outset(insets);
            state.geometry = if grown.loc.y < work_area.loc.y { keep_reachable(grown, work_area) } else { grown };
        }

        state.saved_geometry = state.saved_geometry.map(|saved| saved.inset(state.insets).outset(insets));
        state.insets = insets;
        true
    }

    pub fn remove_window(&mut self, w: WindowId) -> bool {
        let Some(state) = self.windows.remove(&w) else {
            return false;
        };

        self.detach(w, state.workspace_id);
        let active_changed = self.remove_workspace_if_empty(state.workspace_id);

        if active_changed || self.focused == Some(w) {
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
            let left = self.active;
            self.active = target_ws;
            self.leave_workspace(left);
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
        self.windows.get_mut(&w).is_some_and(|state| {
            state.geometry = geometry;
            state.is_placed = true;
            true
        })
    }

    #[must_use]
    pub fn settled_after_move(&self, w: WindowId, anchor: Point) -> Option<Rectangle> {
        let state = self.windows.get(&w)?;
        if state.maximized || state.fullscreen || !state.is_placed {
            return None;
        }
        let area = self.work_area_at(anchor);
        let settled = keep_grip_reachable(state.geometry, state.insets.top, area);
        (settled != state.geometry).then_some(settled)
    }

    #[must_use]
    pub fn calculate_centered_geometry(&self, content_size: Size, insets: Insets, index_on_ws: usize) -> Rectangle {
        let work_area = self.placement_work_area();
        let frame_size = content_size.grow(insets);

        let step = i32::try_from(index_on_ws % 8).unwrap_or(0);
        let offset = step * CASCADE_STEP;
        let x = work_area.loc.x + (work_area.size.width - frame_size.width) / 2 + offset;
        let y = work_area.loc.y + (work_area.size.height - frame_size.height) / 2 + offset;

        keep_reachable(Rectangle::new(Point::new(x, y), frame_size), work_area)
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

        let stranded_saved: Vec<(WindowId, Size)> = self
            .windows
            .values()
            .filter_map(|state| state.saved_geometry.map(|saved| (state.id, saved)))
            .filter(|(_, saved)| !saved.size.is_empty())
            .filter(|(_, saved)| !outputs.iter().any(|rect| rect.overlaps(*saved)))
            .map(|(id, saved)| (id, saved.size))
            .collect();

        for (id, size) in stranded_saved {
            let geometry = self.calculate_centered_geometry(size, Insets::NONE, 0);
            if let Some(state) = self.windows.get_mut(&id) {
                state.saved_geometry = Some(geometry);
            }
        }

        let locked: Vec<(WindowId, bool)> = self
            .windows
            .values()
            .filter(|state| state.fullscreen || state.maximized)
            .map(|state| (state.id, state.fullscreen))
            .collect();
        for (id, fullscreen) in locked {
            let geometry = if fullscreen { self.output_geometry_for_window(id) } else { self.work_area_for_window(id) };
            if let Some(state) = self.windows.get_mut(&id) {
                state.geometry = geometry;
            }
        }
    }

    fn workspace_is_empty(&self, ws_id: WorkspaceId) -> bool {
        self.workspace(ws_id).is_none_or(|ws| ws.windows.is_empty())
    }

    fn detach(&mut self, w: WindowId, ws_id: WorkspaceId) {
        if let Some(ws) = self.workspace_mut(ws_id) {
            ws.windows.retain(|&id| id != w);
            if ws.focused == Some(w) {
                ws.focused = None;
            }
        }
    }

    fn remove_workspace_if_empty(&mut self, ws_id: WorkspaceId) -> bool {
        if let Some(idx) = self.workspaces.iter().position(|ws| ws.id == ws_id)
            && self.workspaces[idx].windows.is_empty()
            && self.workspaces.len() > 1
        {
            self.workspaces.remove(idx);
            if self.active == ws_id {
                let next_idx = idx.min(self.workspaces.len() - 1);
                self.active = self.workspaces[next_idx].id;
                return true;
            }
        }
        false
    }

    fn remember_focus(&mut self) {
        let focused = self.focused;
        if let Some(ws) = self.workspace_mut(self.active) {
            ws.focused = focused;
        }
    }

    fn pick_new_focus(&self) -> Option<WindowId> {
        let active_ws = self.workspace(self.active)?;
        let visible = |id: WindowId| self.windows.get(&id).is_some_and(|state| !state.minimized);

        active_ws
            .focused
            .filter(|&id| active_ws.windows.contains(&id) && visible(id))
            .or_else(|| active_ws.windows.iter().rev().copied().find(|&id| visible(id)))
    }
}
