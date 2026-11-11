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
    pub parent: Option<WindowId>,
    pub mapped: bool,
    pub modal: bool,
    pub app: Option<String>,
    pub keep_above: bool,
    pub sticky: bool,
}

impl WindowState {
    const fn focusable(&self) -> bool {
        self.mapped && !self.minimized
    }
}

#[derive(Debug, Clone, Default)]
struct Departing {
    id: Option<WindowId>,
    parent: Option<WindowId>,
    app: Option<String>,
}

impl Departing {
    fn from(state: &WindowState) -> Self {
        Self { id: Some(state.id), parent: state.parent, app: state.app.clone() }
    }
}

const MODAL_DEPTH: usize = 8;

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
}

impl Workspace {
    const fn new(id: WorkspaceId) -> Self {
        Self { id, windows: Vec::new() }
    }
}

pub const FALLBACK_OUTPUT: Rectangle = Rectangle::from_loc_and_size(0, 0, 1920, 1080);

const CASCADE_STEP: i32 = 32;

#[must_use]
pub(crate) fn keep_reachable(frame: Rectangle, area: Rectangle) -> Rectangle {
    let max_x = area.loc.x.saturating_add(area.size.width).saturating_sub(frame.size.width).max(area.loc.x);
    let max_y = area.loc.y.saturating_add(area.size.height).saturating_sub(frame.size.height).max(area.loc.y);
    Rectangle::new(Point::new(frame.loc.x.clamp(area.loc.x, max_x), frame.loc.y.clamp(area.loc.y, max_y)), frame.size)
}

#[must_use]
fn fills_height_against_one_side(frame: Rectangle, area: Rectangle) -> bool {
    let left = frame.loc.x == area.loc.x;
    let right = frame.loc.x.saturating_add(frame.size.width) == area.loc.x.saturating_add(area.size.width);
    let top = frame.loc.y == area.loc.y;
    let bottom = frame.loc.y.saturating_add(frame.size.height) == area.loc.y.saturating_add(area.size.height);
    top && bottom && left != right
}

#[must_use]
fn overlap_area(a: Rectangle, b: Rectangle) -> i64 {
    let span = |a_start: i32, a_len: i32, b_start: i32, b_len: i32| {
        let end = a_start.saturating_add(a_len).min(b_start.saturating_add(b_len));
        (i64::from(end) - i64::from(a_start.max(b_start))).max(0)
    };
    span(a.loc.x, a.size.width, b.loc.x, b.size.width) * span(a.loc.y, a.size.height, b.loc.y, b.size.height)
}

#[must_use]
fn sole_home(outputs: &[OutputInfo], rect: Rectangle) -> Option<&OutputInfo> {
    let mut containing = outputs.iter().filter(|output| output.geometry.contains(rect.center()));
    if let Some(first) = containing.next() {
        return containing.next().is_none().then_some(first);
    }
    let mut overlapping = outputs.iter().filter(|output| output.geometry.overlaps(rect));
    let first = overlapping.next()?;
    overlapping.next().is_none().then_some(first)
}

#[must_use]
fn home_change<'a>(
    before: &'a [OutputInfo], after: &'a [OutputInfo], rect: Rectangle,
) -> Option<(&'a OutputInfo, &'a OutputInfo)> {
    let old = sole_home(before, rect)?;
    let new = after.iter().find(|output| output.id == old.id)?;
    Some((old, new))
}

#[must_use]
fn translated(rect: Rectangle, dx: i32, dy: i32) -> Rectangle {
    Rectangle::new(Point::new(rect.loc.x.saturating_add(dx), rect.loc.y.saturating_add(dy)), rect.size)
}

#[must_use]
fn followed(rect: Rectangle, old: &OutputInfo, new: &OutputInfo) -> Rectangle {
    let moved = translated(
        rect,
        new.geometry.loc.x.saturating_sub(old.geometry.loc.x),
        new.geometry.loc.y.saturating_sub(old.geometry.loc.y),
    );
    if old.geometry.size == new.geometry.size { moved } else { keep_reachable(moved, new.work_area) }
}

#[must_use]
fn centered_geometry(work_area: Rectangle, content_size: Size, insets: Insets, index_on_ws: usize) -> Rectangle {
    let frame_size = content_size.grow(insets);

    let step = i32::try_from(index_on_ws % 8).unwrap_or(0);
    let offset = step * CASCADE_STEP;
    let x = work_area
        .loc
        .x
        .saturating_add(work_area.size.width.saturating_sub(frame_size.width) / 2)
        .saturating_add(offset);
    let y = work_area
        .loc
        .y
        .saturating_add(work_area.size.height.saturating_sub(frame_size.height) / 2)
        .saturating_add(offset);

    keep_reachable(Rectangle::new(Point::new(x, y), frame_size), work_area)
}

pub(crate) const MOVE_MIN_VISIBLE_WIDTH: i32 = 64;

pub(crate) const MOVE_MIN_GRIP_HEIGHT: i32 = 32;

#[must_use]
pub(crate) fn keep_grip_reachable(frame: Rectangle, grip_height: i32, area: Rectangle) -> Rectangle {
    let grip_height = grip_height.max(MOVE_MIN_GRIP_HEIGHT).min(frame.size.height).max(1);
    let visible = MOVE_MIN_VISIBLE_WIDTH.min(frame.size.width).max(1);
    let overhang = frame.size.width.saturating_sub(visible);
    let grip = Rectangle::new(frame.loc, Size::new(frame.size.width, grip_height));
    let reach = Rectangle::new(
        Point::new(area.loc.x.saturating_sub(overhang), area.loc.y),
        Size::new(area.size.width.saturating_add(overhang.saturating_mul(2)), area.size.height),
    );
    Rectangle::new(keep_reachable(grip, reach).loc, frame.size)
}

#[derive(Debug, Clone)]
pub struct Layout {
    workspaces: Vec<Workspace>,
    active: WorkspaceId,
    recent: Vec<WorkspaceId>,
    windows: HashMap<WindowId, WindowState>,
    focused: Option<WindowId>,
    mru: Vec<WindowId>,
    outputs: Vec<OutputInfo>,
    policy: NewWindowPolicy,
    overview_active: bool,
    placement_anchor: Option<Point>,
    anchors: HashMap<WindowId, Point>,
    fixed: Option<usize>,
    unsettled: HashMap<WindowId, Assigned>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assigned {
    ByPolicy,
    ByHand,
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
            recent: Vec::new(),
            windows: HashMap::new(),
            focused: None,
            mru: Vec::new(),
            outputs: Vec::new(),
            policy: NewWindowPolicy::NewWorkspace,
            overview_active: false,
            placement_anchor: None,
            anchors: HashMap::new(),
            fixed: None,
            unsettled: HashMap::new(),
        }
    }

    pub fn set_placement_anchor(&mut self, point: Point) {
        self.placement_anchor = Some(point);
    }

    #[must_use]
    pub(crate) fn placement_work_area(&self) -> Rectangle {
        self.work_area_near(self.placement_anchor)
    }

    #[must_use]
    fn work_area_near(&self, anchor: Option<Point>) -> Rectangle {
        anchor.map_or_else(|| self.primary_output_work_area(), |anchor| self.work_area_at(anchor))
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
    pub(crate) fn output_at(&self, point: Point) -> Option<&OutputInfo> {
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
        if !state.is_placed {
            return self.anchors.get(&w).and_then(|&anchor| self.output_at(anchor));
        }
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

    pub(crate) const MAX_FIXED_WORKSPACES: usize = 36;

    pub fn set_fixed_workspaces(&mut self, count: Option<usize>) -> bool {
        let count = count.map(|count| count.clamp(1, Self::MAX_FIXED_WORKSPACES));
        if self.fixed == count {
            return false;
        }
        self.fixed = count;
        if let Some(count) = count {
            while self.workspaces.len() < count {
                self.workspaces.push(Workspace::new(WorkspaceId::next()));
            }
            while self.workspaces.len() > count {
                self.fold_last_workspace();
            }
        } else {
            let empty: Vec<WorkspaceId> = self
                .workspaces
                .iter()
                .filter(|ws| ws.id != self.active && ws.windows.is_empty())
                .map(|ws| ws.id)
                .collect();
            for id in empty {
                self.remove_workspace_if_empty(id);
            }
        }
        true
    }

    #[must_use]
    pub const fn fixed_workspaces(&self) -> Option<usize> {
        self.fixed
    }

    fn fold_last_workspace(&mut self) {
        let Some(last) = self.workspaces.pop() else {
            return;
        };
        let Some(into) = self.workspaces.last().map(|ws| ws.id) else {
            self.workspaces.push(last);
            return;
        };
        self.recent.retain(|&seen| seen != last.id);
        self.rehome_unsettled(last.id, into);
        for id in &last.windows {
            if let Some(state) = self.windows.get_mut(id) {
                state.workspace_id = into;
            }
        }
        if let Some(target) = self.workspace_mut(into) {
            target.windows.extend(last.windows.iter().copied());
        }
        if self.active == last.id {
            self.recent.retain(|&seen| seen != into);
            self.active = into;
            if !self.focused.is_some_and(|focused| last.windows.contains(&focused)) {
                self.focused = self.fallback_focus(&Departing::default());
            }
        }
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
        self.windows.get(&w).is_some_and(|state| state.workspace_id == self.active && state.focusable())
    }

    #[must_use]
    pub fn is_mapped(&self, w: WindowId) -> bool {
        self.windows.get(&w).is_some_and(|state| state.mapped)
    }

    #[must_use]
    pub fn recent_windows(&self) -> &[WindowId] {
        &self.mru
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
        let (mut below, above): (Vec<WindowState>, Vec<WindowState>) = self
            .workspace(workspace)
            .into_iter()
            .flat_map(|ws| ws.windows.iter())
            .filter_map(|id| self.windows.get(id))
            .filter(|state| state.focusable())
            .cloned()
            .partition(|state| !state.keep_above);
        below.extend(above);
        below
    }

    pub fn set_keep_above(&mut self, w: WindowId, keep_above: bool) -> bool {
        self.windows.get_mut(&w).is_some_and(|state| std::mem::replace(&mut state.keep_above, keep_above) != keep_above)
    }

    pub fn set_sticky(&mut self, w: WindowId, sticky: bool) -> bool {
        let changed =
            self.windows.get_mut(&w).is_some_and(|state| std::mem::replace(&mut state.sticky, sticky) != sticky);
        if changed && sticky {
            self.carry_sticky(self.active);
        }
        changed
    }

    fn carry_sticky(&mut self, to: WorkspaceId) {
        let moving: Vec<WindowId> = self
            .windows
            .values()
            .filter(|state| state.sticky && state.workspace_id != to && !self.unsettled.contains_key(&state.id))
            .map(|state| state.id)
            .collect();
        for id in moving {
            for ws in &mut self.workspaces {
                ws.windows.retain(|&known| known != id);
            }
            if let Some(ws) = self.workspace_mut(to) {
                ws.windows.push(id);
            }
            if let Some(state) = self.windows.get_mut(&id) {
                state.workspace_id = to;
            }
        }
    }

    #[must_use]
    pub const fn overview_active(&self) -> bool {
        self.overview_active
    }

    pub const fn open_overview(&mut self) -> bool {
        !std::mem::replace(&mut self.overview_active, true)
    }

    pub fn close_overview(&mut self) -> bool {
        let was_open = std::mem::replace(&mut self.overview_active, false);
        if !was_open {
            return false;
        }
        if self.remove_workspace_if_empty(self.active) {
            self.focused = self.fallback_focus(&Departing::default());
        }
        true
    }

    pub fn commit_focus(&mut self) {
        if let Some(focused) = self.focused {
            self.touch(focused);
        }
    }

    #[must_use]
    pub fn overview_windows(&self) -> Vec<WindowState> {
        self.workspaces
            .iter()
            .flat_map(|ws| ws.windows.iter())
            .filter_map(|id| self.windows.get(id))
            .filter(|state| state.focusable())
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

    pub(crate) fn raise_window(&mut self, w: WindowId) -> bool {
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
        let homes = self.follow_moved_outputs(outputs);
        self.outputs = outputs.to_vec();
        if !self.outputs.is_empty() {
            self.rescue_offscreen_windows(&homes);
        }
    }

    fn follow_moved_outputs(&mut self, incoming: &[OutputInfo]) -> HashMap<WindowId, OutputId> {
        let mut homes = HashMap::new();
        for state in self.windows.values_mut() {
            if state.is_placed
                && let Some((old, new)) = home_change(&self.outputs, incoming, state.geometry)
            {
                if state.fullscreen || state.maximized {
                    homes.insert(state.id, old.id);
                }
                state.geometry = followed(state.geometry, old, new);
            }
            if let Some(saved) = state.saved_geometry
                && let Some((old, new)) = home_change(&self.outputs, incoming, saved)
            {
                state.saved_geometry = Some(followed(saved, old, new));
            }
        }
        homes
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
            (target == WorkspaceTarget::New && self.fixed.is_none()).then(|| {
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

    fn activate(&mut self, id: WorkspaceId) {
        if self.active == id {
            return;
        }
        let left = self.active;
        self.recent.retain(|&seen| seen != left && seen != id);
        self.recent.insert(0, left);
        self.active = id;
        self.carry_sticky(id);
    }

    pub fn move_workspace(&mut self, id: WorkspaceId, index: usize) -> bool {
        let Some(from) = self.workspaces.iter().position(|ws| ws.id == id) else {
            return false;
        };
        let to = index.min(self.workspaces.len() - 1);
        if from == to {
            return false;
        }
        let workspace = self.workspaces.remove(from);
        self.workspaces.insert(to, workspace);
        true
    }

    pub fn switch_to_workspace(&mut self, target: WorkspaceTarget) -> bool {
        if !self.browse_workspace(target) {
            return false;
        }
        self.commit_focus();
        true
    }

    pub fn browse_workspace(&mut self, target: WorkspaceTarget) -> bool {
        let Some(target_id) = self.resolve_or_create(target) else {
            return false;
        };

        if self.active == target_id {
            return true;
        }

        let left = self.active;
        self.activate(target_id);
        self.leave_workspace(left);
        self.focused = self.fallback_focus(&Departing::default());
        true
    }

    pub fn move_window_to_workspace(&mut self, w: WindowId, target: WorkspaceTarget) -> bool {
        if !self.windows.contains_key(&w) {
            return false;
        }
        if self.unsettled.contains_key(&w) {
            return self.assign_unsettled(w, target);
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
        }

        let keeps_active = self.overview_active && old_ws_id == self.active;
        let old_ws_vanishes =
            !keeps_active && self.fixed.is_none() && self.workspace_is_empty(old_ws_id) && self.workspaces.len() > 1;
        if old_ws_vanishes && self.active == old_ws_id && self.focused == Some(w) {
            self.activate(target_id);
        }

        let active_changed = old_ws_vanishes && self.remove_workspace_if_empty(old_ws_id);

        if active_changed || (self.focused == Some(w) && self.active != target_id) {
            let departing = self.windows.get(&w).map(Departing::from).unwrap_or_default();
            self.focused = self.fallback_focus(&departing);
        }

        true
    }

    pub fn add_window(&mut self, w: WindowId, initial_size: Size) -> WorkspaceId {
        self.admit_window(w, initial_size, None);
        self.map_window(w, true);
        self.windows.get(&w).map_or(self.active, |state| state.workspace_id)
    }

    pub fn add_window_with_parent(&mut self, w: WindowId, initial_size: Size, parent: WindowId) -> WorkspaceId {
        let ws_id = self.admit_window(w, initial_size, Some(parent));
        self.map_window(w, ws_id == self.active);
        ws_id
    }

    pub fn admit_window(&mut self, w: WindowId, initial_size: Size, parent: Option<WindowId>) -> WorkspaceId {
        let parent = parent.filter(|parent| self.windows.contains_key(parent));
        let ws_id = parent.and_then(|parent| self.windows.get(&parent)).map_or(self.active, |state| state.workspace_id);
        self.insert_window(w, ws_id, initial_size, parent);
        self.unsettled.insert(w, Assigned::ByPolicy);
        let centred = if initial_size.is_empty() {
            None
        } else {
            Some(
                parent.and_then(|parent| self.centered_over_parent(parent, initial_size, Insets::NONE)).unwrap_or_else(
                    || centered_geometry(self.placement_work_area(), initial_size, Insets::NONE, self.cascade_index(w)),
                ),
            )
        };
        if let (Some(geometry), Some(state)) = (centred, self.windows.get_mut(&w)) {
            state.geometry = geometry;
        }
        ws_id
    }

    fn joins(&self, state: &WindowState, assigned: Assigned) -> Option<WorkspaceId> {
        if state.sticky {
            return Some(self.active);
        }
        let known = |id: WorkspaceId| self.workspace(id).map(|ws| ws.id);
        let beside_parent =
            state.parent.and_then(|parent| self.windows.get(&parent)).and_then(|parent| known(parent.workspace_id));
        let chosen = (assigned == Assigned::ByHand).then(|| known(state.workspace_id)).flatten();
        let fresh = self.policy == NewWindowPolicy::NewWorkspace
            && self.fixed.is_none()
            && !self.workspace_is_empty(self.active);
        beside_parent.or(chosen).or_else(|| (!fresh).then_some(self.active))
    }

    fn cascade_index(&self, w: WindowId) -> usize {
        let Some(state) = self.windows.get(&w) else {
            return 0;
        };
        let joined = match self.unsettled.get(&w) {
            Some(&assigned) => self.joins(state, assigned),
            None => Some(state.workspace_id),
        };
        joined
            .and_then(|id| self.workspace(id))
            .map_or(0, |ws| ws.windows.iter().position(|&id| id == w).unwrap_or(ws.windows.len()))
    }

    fn settle(&mut self, w: WindowId, assigned: Assigned) {
        let Some(joined) = self.windows.get(&w).map(|state| self.joins(state, assigned)) else {
            return;
        };
        let ws_id = joined.unwrap_or_else(|| {
            let fresh = WorkspaceId::next();
            let at = self.active_index().unwrap_or(0) + 1;
            self.workspaces.insert(at, Workspace::new(fresh));
            fresh
        });
        if let Some(state) = self.windows.get_mut(&w) {
            state.workspace_id = ws_id;
        }
        if let Some(ws) = self.workspace_mut(ws_id) {
            ws.windows.push(w);
        }
    }

    fn assign_unsettled(&mut self, w: WindowId, target: WorkspaceTarget) -> bool {
        let Some(target_id) = self.resolve_target(target) else {
            return false;
        };
        if let Some(state) = self.windows.get_mut(&w) {
            state.workspace_id = target_id;
        }
        self.unsettled.insert(w, Assigned::ByHand);
        true
    }

    fn rehome_unsettled(&mut self, from: WorkspaceId, to: WorkspaceId) {
        for id in self.unsettled.keys() {
            if let Some(state) = self.windows.get_mut(id).filter(|state| state.workspace_id == from) {
                state.workspace_id = to;
            }
        }
    }

    pub fn map_window(&mut self, w: WindowId, focus: bool) -> bool {
        if self.windows.get(&w).is_none_or(|state| state.mapped) {
            return false;
        }
        if let Some(assigned) = self.unsettled.remove(&w) {
            self.settle(w, assigned);
        }
        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };
        state.mapped = true;
        let ws_id = state.workspace_id;
        if focus {
            self.focus_window(w);
            return true;
        }
        let below = self.focused.filter(|&focused| self.windows.get(&focused).is_some_and(|s| s.workspace_id == ws_id));
        if let (Some(below), Some(ws)) = (below, self.workspace_mut(ws_id)) {
            ws.windows.retain(|&id| id != w);
            let at = ws.windows.iter().position(|&id| id == below).unwrap_or(ws.windows.len());
            ws.windows.insert(at, w);
        }
        self.mru.retain(|&id| id != w);
        self.mru.insert(self.mru.len().min(1), w);
        true
    }

    pub fn unmap_window(&mut self, w: WindowId) -> bool {
        let Some(state) = self.windows.get_mut(&w).filter(|state| state.mapped) else {
            return false;
        };
        state.mapped = false;
        if self.focused == Some(w) {
            let departing = self.windows.get(&w).map(Departing::from).unwrap_or_default();
            self.focused = self.fallback_focus(&departing);
        }
        true
    }

    pub fn set_app(&mut self, w: WindowId, app: Option<String>) {
        if let Some(state) = self.windows.get_mut(&w) {
            state.app = app;
        }
    }

    pub fn set_modal(&mut self, w: WindowId, modal: bool) -> bool {
        self.windows.get_mut(&w).is_some_and(|state| std::mem::replace(&mut state.modal, modal) != modal)
    }

    pub fn set_parent(&mut self, w: WindowId, parent: Option<WindowId>) {
        let parent = parent.filter(|&parent| parent != w && self.windows.contains_key(&parent));
        if let Some(state) = self.windows.get_mut(&w) {
            state.parent = parent;
        }
    }

    fn centered_over_parent(&self, parent: WindowId, content_size: Size, insets: Insets) -> Option<Rectangle> {
        let host = self.windows.get(&parent).filter(|state| state.is_placed && !state.minimized)?.frame_geometry();
        let frame_size = content_size.grow(insets);
        let x = host.loc.x.saturating_add(host.size.width.saturating_sub(frame_size.width) / 2);
        let y = host.loc.y.saturating_add(host.size.height.saturating_sub(frame_size.height) / 2);
        Some(keep_reachable(Rectangle::new(Point::new(x, y), frame_size), self.work_area_for_window(parent)))
    }

    fn insert_window(&mut self, w: WindowId, ws_id: WorkspaceId, size: Size, parent: Option<WindowId>) {
        let is_placed = !size.is_empty();
        let geometry = Rectangle::new(Point::ZERO, size);
        if let Some(anchor) = self.placement_anchor {
            self.anchors.insert(w, anchor);
        }
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
                parent,
                mapped: false,
                modal: false,
                app: None,
                keep_above: false,
                sticky: false,
            },
        );
    }

    fn restored_frame(&self, w: WindowId) -> Option<Rectangle> {
        let saved = self.windows.get(&w)?.saved_geometry?;
        let area = self.work_area_at(saved.center());
        Some(if saved.loc.y < area.loc.y { keep_reachable(saved, area) } else { saved })
    }

    pub fn set_maximized(&mut self, w: WindowId, maximized: bool) -> bool {
        let work_area = self.work_area_for_window(w);
        let restored = self.restored_frame(w);

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
            match state.saved_geometry.take().and(restored) {
                Some(saved) => state.geometry = saved,
                None => state.is_placed = false,
            }
            state.maximized = false;
        }
        true
    }

    pub fn set_fullscreen(&mut self, w: WindowId, fullscreen: bool) -> bool {
        self.set_fullscreen_on(w, fullscreen, None)
    }

    pub fn set_fullscreen_on(&mut self, w: WindowId, fullscreen: bool, on: Option<Point>) -> bool {
        let requested = on.and_then(|point| self.output_at(point)).map(|output| output.geometry);
        let output_geom = requested.unwrap_or_else(|| self.output_geometry_for_window(w));
        let work_area = self.work_area_for_window(w);
        let restored = self.restored_frame(w);

        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };

        if state.fullscreen && fullscreen && requested.is_some_and(|area| area != state.geometry) {
            state.geometry = output_geom;
            return true;
        }

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
                match state.saved_geometry.take().and(restored) {
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

        if minimized {
            self.mru.retain(|&id| id != w);
            self.mru.push(w);
            if self.focused == Some(w) {
                let departing = self.windows.get(&w).map(Departing::from).unwrap_or_default();
                self.focused = self.fallback_focus(&departing);
            }
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

        if state.maximized || state.fullscreen || size.is_empty() {
            return false;
        }

        let insets = state.insets;
        if state.is_placed {
            if let Some(state) = self.windows.get_mut(&w) {
                state.geometry.size = size.grow(insets);
            }
            return false;
        }

        let index_on_ws = self.cascade_index(w);

        let area = self.work_area_near(self.anchors.get(&w).copied());
        let geometry = state
            .parent
            .and_then(|parent| self.centered_over_parent(parent, size, insets))
            .unwrap_or_else(|| centered_geometry(area, size, insets, index_on_ws));
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
        } else if state.is_placed
            && !state.geometry.size.is_empty()
            && !fills_height_against_one_side(state.geometry, work_area)
        {
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
        self.anchors.remove(&w);
        self.mru.retain(|&id| id != w);
        for other in self.windows.values_mut() {
            if other.parent == Some(w) {
                other.parent = state.parent;
            }
        }
        if self.unsettled.remove(&w).is_some() {
            return true;
        }

        self.detach(w, state.workspace_id);
        let active_changed = self.remove_workspace_if_empty(state.workspace_id);

        if active_changed || self.focused == Some(w) {
            self.focused = self.fallback_focus(&Departing::from(&state));
        }

        true
    }

    pub fn focus_window(&mut self, w: WindowId) -> bool {
        let target = self.resolve_modal(w);
        let Some(target_ws) = self.windows.get(&target).filter(|state| state.mapped).map(|state| state.workspace_id)
        else {
            return false;
        };

        if self.active != target_ws {
            let left = self.active;
            self.activate(target_ws);
            self.leave_workspace(left);
        }

        if let Some(state) = self.windows.get_mut(&target) {
            state.minimized = false;
        }

        self.focused = Some(target);
        if target != w {
            self.raise_window(w);
        }
        self.raise_window(target);
        self.touch(target);
        true
    }

    pub fn clear_focus(&mut self) {
        self.focused = None;
    }

    fn touch(&mut self, w: WindowId) {
        if self.mru.first() != Some(&w) {
            self.mru.retain(|&id| id != w);
            self.mru.insert(0, w);
        }
    }

    #[must_use]
    pub fn resolve_modal(&self, w: WindowId) -> WindowId {
        let mut current = w;
        for _ in 0..MODAL_DEPTH {
            let child = self
                .windows
                .values()
                .filter(|state| state.parent == Some(current) && state.modal && state.focusable())
                .map(|state| state.id)
                .max();
            match child {
                Some(child) => current = child,
                None => break,
            }
        }
        current
    }

    fn fallback_focus(&self, departing: &Departing) -> Option<WindowId> {
        let active = self.workspace(self.active)?;
        let candidate = |id: WindowId| {
            departing.id != Some(id)
                && self.windows.get(&id).is_some_and(|state| state.workspace_id == self.active && state.focusable())
        };
        let same_app = |id: WindowId| {
            departing.app.is_some() && self.windows.get(&id).is_some_and(|state| state.app == departing.app)
        };
        departing
            .parent
            .filter(|&parent| candidate(parent))
            .or_else(|| self.mru.iter().copied().find(|&id| candidate(id) && same_app(id)))
            .or_else(|| self.mru.iter().copied().find(|&id| candidate(id)))
            .or_else(|| active.windows.iter().rev().copied().find(|&id| candidate(id)))
            .map(|id| self.resolve_modal(id))
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
        let grip = state.insets.top;
        if self
            .outputs
            .iter()
            .any(|output| keep_grip_reachable(state.geometry, grip, output.work_area) == state.geometry)
        {
            return None;
        }
        let settled = keep_grip_reachable(state.geometry, grip, self.work_area_at(anchor));
        (settled != state.geometry).then_some(settled)
    }

    fn grip_settlement(&self, frame: Rectangle, grip_height: i32, orphans: &mut usize) -> Option<Rectangle> {
        if frame.size.is_empty()
            || self.outputs.iter().any(|output| keep_grip_reachable(frame, grip_height, output.work_area) == frame)
        {
            return None;
        }
        let host = self
            .outputs
            .iter()
            .filter(|output| output.geometry.overlaps(frame))
            .max_by_key(|output| overlap_area(output.geometry, frame));
        if let Some(output) = host {
            return Some(keep_grip_reachable(frame, grip_height, output.work_area));
        }
        let index = *orphans;
        *orphans += 1;
        Some(centered_geometry(self.placement_work_area(), frame.size, Insets::NONE, index))
    }

    fn rescue_offscreen_windows(&mut self, homes: &HashMap<WindowId, OutputId>) {
        let mut ids: Vec<WindowId> = self.windows.keys().copied().collect();
        ids.sort_unstable();
        let mut orphans = 0;
        let settled: Vec<(WindowId, Option<Rectangle>, Option<Rectangle>)> = ids
            .iter()
            .filter_map(|id| self.windows.get(id))
            .map(|state| {
                let grip = state.insets.top;
                let frame = state.is_placed.then(|| self.grip_settlement(state.geometry, grip, &mut orphans)).flatten();
                let saved = state.saved_geometry.and_then(|saved| self.grip_settlement(saved, grip, &mut orphans));
                (state.id, frame, saved)
            })
            .collect();

        for (id, frame, saved) in settled {
            if let Some(state) = self.windows.get_mut(&id) {
                state.geometry = frame.unwrap_or(state.geometry);
                state.saved_geometry = saved.or(state.saved_geometry);
            }
        }

        let locked: Vec<(WindowId, bool)> = self
            .windows
            .values()
            .filter(|state| state.fullscreen || state.maximized)
            .map(|state| (state.id, state.fullscreen))
            .collect();
        for (id, fullscreen) in locked {
            let home = homes.get(&id).and_then(|home| self.outputs.iter().find(|output| output.id == *home));
            let geometry = match (home, fullscreen) {
                (Some(output), true) => output.geometry,
                (Some(output), false) => output.work_area,
                (None, true) => self.output_geometry_for_window(id),
                (None, false) => self.work_area_for_window(id),
            };
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
        }
    }

    fn remove_workspace_if_empty(&mut self, ws_id: WorkspaceId) -> bool {
        if self.fixed.is_some() || (self.overview_active && ws_id == self.active) {
            return false;
        }
        let only_sticky =
            |ws: &Workspace| ws.windows.iter().all(|id| self.windows.get(id).is_some_and(|state| state.sticky));
        if let Some(idx) = self.workspaces.iter().position(|ws| ws.id == ws_id)
            && only_sticky(&self.workspaces[idx])
            && self.workspaces.len() > 1
        {
            let carried = self.workspaces.remove(idx).windows;
            self.recent.retain(|&seen| seen != ws_id);
            let neighbour = self.workspaces[idx.min(self.workspaces.len() - 1)].id;
            let was_active = self.active == ws_id;
            let home = if was_active {
                let back = self.recent.first().copied().unwrap_or(neighbour);
                self.recent.retain(|&seen| seen != back);
                self.active = back;
                back
            } else {
                self.active
            };
            self.rehome_unsettled(ws_id, home);
            for id in carried {
                if let Some(state) = self.windows.get_mut(&id) {
                    state.workspace_id = home;
                }
                if let Some(ws) = self.workspace_mut(home) {
                    ws.windows.push(id);
                }
            }
            return was_active;
        }
        false
    }
}
