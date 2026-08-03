//! Window layout management.

use std::collections::HashMap;

use crate::{
    geometry::{Insets, Point, Rectangle, Size},
    ids::{OutputId, WindowId, WorkspaceId},
};

/// Policy for placing newly created windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewWindowPolicy {
    /// Create a new workspace for each new window.
    #[default]
    NewWorkspace,
    /// Place new window on the current active workspace.
    ActiveWorkspace,
}

/// Target specification for switching or moving windows between workspaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTarget {
    /// Workspace by index (0-indexed).
    Index(usize),
    /// Workspace by exact ID.
    Id(WorkspaceId),
    /// Next workspace relative to current active.
    Next,
    /// Previous workspace relative to current active.
    Previous,
}

/// Information about an output device (monitor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub id: OutputId,
    /// The output's full extent. What a fullscreen window covers.
    pub geometry: Rectangle,
    /// `geometry` minus space claimed by layer-shell surfaces (panels, bars).
    /// What a floating window is centred in and a maximized window covers.
    ///
    /// Equal to `geometry` until layer-shell support exists to shrink it.
    pub work_area: Rectangle,
}

/// State of a managed window.
///
/// `maximized`, `fullscreen` and `minimized` are independent on purpose, not
/// a candidate for merging into one enum: real clients combine them (e.g. a
/// maximized window can still be minimized, and must return to maximized —
/// not merely restored — when un-minimized).
#[expect(
    clippy::struct_excessive_bools,
    reason = "maximized/fullscreen/minimized are genuinely orthogonal, see above"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowState {
    pub id: WindowId,
    pub workspace_id: WorkspaceId,
    /// The window's frame: what the user sees and drags, decorations
    /// included. Use [`WindowState::content_geometry`] for what the client
    /// itself should be told to draw into.
    pub geometry: Rectangle,
    pub saved_geometry: Option<Rectangle>,
    pub is_placed: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    /// Hidden from view but not closed. Distinct from being on an inactive
    /// workspace: a minimized window stays on its workspace and is skipped
    /// by both `visible_windows` and focus, but reappears the moment it (or
    /// its workspace) is focused again.
    pub minimized: bool,
    /// Space a decoration claims around the content, e.g. a titlebar's
    /// height as a top inset. `Insets::NONE` for an undecorated (CSD)
    /// window. Set once via [`Layout::set_window_insets`], normally right
    /// after [`Layout::add_window`] and before the first real size arrives.
    pub insets: Insets,
}

impl WindowState {
    /// The window's frame: position and size including decorations. Equal to
    /// `geometry` — this exists so call sites can say what they mean instead
    /// of relying on readers remembering what the bare field represents.
    #[must_use]
    pub const fn frame_geometry(&self) -> Rectangle {
        self.geometry
    }

    /// The area the client should actually draw into: the frame minus
    /// decoration insets.
    ///
    /// Fullscreen is a special case: a fullscreen window covers the output
    /// with no decoration shown at all (there is nothing sensible to
    /// display a titlebar over), so insets are ignored and content equals
    /// the frame exactly.
    #[must_use]
    pub fn content_geometry(&self) -> Rectangle {
        if self.fullscreen {
            self.geometry
        } else {
            self.geometry.inset(self.insets)
        }
    }
}

/// Workspace representation.
///
/// `windows` is ordered bottom-to-top: the last entry is the topmost window.
/// This ordering is the single source of truth for stacking, so anything that
/// renders or hit-tests must follow it rather than iterating the window map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub windows: Vec<WindowId>,
    /// Window to focus when this workspace becomes active again.
    ///
    /// Without this, returning to a workspace would focus whatever happened
    /// to be added last rather than what the user was actually using.
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

/// Geometry assumed when no output has been reported yet.
///
/// Reached only in the window between startup and the first connector being
/// set up (and in unit tests). Windows placed against it are re-placed by
/// [`Layout::outputs_changed`] as soon as a real output appears.
pub const FALLBACK_OUTPUT: Rectangle = Rectangle::from_loc_and_size(0, 0, 1920, 1080);

/// Horizontal and vertical step between windows stacked on one workspace, so
/// a second window does not land exactly on top of the first.
const CASCADE_STEP: i32 = 32;

/// Core layout engine for managing workspaces and windows.
#[derive(Debug, Clone)]
pub struct Layout {
    workspaces: Vec<Workspace>,
    active: WorkspaceId,
    windows: HashMap<WindowId, WindowState>,
    focused: Option<WindowId>,
    outputs: Vec<OutputInfo>,
    policy: NewWindowPolicy,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    /// Creates a new `Layout` initialized with one empty workspace.
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
        }
    }

    /// Full geometry of the output windows are placed against. What a
    /// fullscreen window covers.
    ///
    /// Single-output for now: additional outputs are tracked but not yet used
    /// for placement, so a second monitor shows whatever falls within its
    /// region rather than hosting windows of its own.
    #[must_use]
    pub fn primary_output_geometry(&self) -> Rectangle {
        self.outputs
            .first()
            .map_or(FALLBACK_OUTPUT, |output| output.geometry)
    }

    /// Usable area of the output: `geometry` minus panels. What floating
    /// windows are centred in and maximized windows cover.
    #[must_use]
    pub fn primary_output_work_area(&self) -> Rectangle {
        self.outputs
            .first()
            .map_or(FALLBACK_OUTPUT, |output| output.work_area)
    }

    /// Set the policy for new window placement.
    pub fn set_policy(&mut self, policy: NewWindowPolicy) {
        self.policy = policy;
    }

    /// Get the current placement policy.
    #[must_use]
    pub const fn policy(&self) -> NewWindowPolicy {
        self.policy
    }

    /// Returns an iterator over all workspaces.
    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces.iter()
    }

    /// Returns the active workspace ID.
    #[must_use]
    pub const fn active_workspace(&self) -> WorkspaceId {
        self.active
    }

    /// Returns ordered list of all workspace IDs.
    #[must_use]
    pub fn workspace_ids(&self) -> Vec<WorkspaceId> {
        self.workspaces.iter().map(|ws| ws.id).collect()
    }

    /// Returns total number of workspaces.
    #[must_use]
    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    /// Returns total number of managed windows across all workspaces.
    #[must_use]
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Returns the currently focused window, if any.
    #[must_use]
    pub const fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    /// Checks if a window is focused.
    #[must_use]
    pub fn is_focused(&self, w: WindowId) -> bool {
        self.focused == Some(w)
    }

    /// Checks if a window is on the active workspace and not minimized.
    #[must_use]
    pub fn is_visible(&self, w: WindowId) -> bool {
        self.windows
            .get(&w)
            .is_some_and(|state| state.workspace_id == self.active && !state.minimized)
    }

    /// Gets state for a specific window.
    #[must_use]
    pub fn window_state(&self, w: WindowId) -> Option<&WindowState> {
        self.windows.get(&w)
    }

    /// Window states on the active workspace, ordered bottom-to-top, minus
    /// any that are minimized.
    ///
    /// Ordering comes from the workspace's own stack rather than the window
    /// map: iterating a `HashMap` would give a different Z-order on every
    /// call, making windows randomly swap places on screen.
    pub fn visible_windows(&self) -> impl Iterator<Item = &WindowState> {
        self.workspaces
            .iter()
            .find(|ws| ws.id == self.active)
            .into_iter()
            .flat_map(|ws| ws.windows.iter())
            .filter_map(|id| self.windows.get(id))
            .filter(|state| !state.minimized)
    }

    /// Raises a window to the top of its workspace's stack.
    ///
    /// Returns `false` if the window is unknown.
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

    /// Replaces the set of known outputs.
    ///
    /// Windows are deliberately **not** re-centred wholesale: a window the
    /// user has dragged somewhere should stay there across an unrelated
    /// monitor hotplug. Only windows that would be left completely outside
    /// every output — and so unreachable — are pulled back into view.
    pub fn outputs_changed(&mut self, outputs: &[OutputInfo]) {
        self.outputs = outputs.to_vec();
        self.rescue_offscreen_windows();
    }

    /// Returns configured outputs.
    #[must_use]
    pub fn outputs(&self) -> &[OutputInfo] {
        &self.outputs
    }

    /// Resolves a `WorkspaceTarget` to a `WorkspaceId` if it exists.
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

    /// Switches active workspace to the target workspace.
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

    /// Moves a window to a target workspace.
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

        // Remove from old workspace
        if let Some(old_ws) = self.workspaces.iter_mut().find(|ws| ws.id == old_ws_id) {
            old_ws.windows.retain(|&id| id != w);
            if old_ws.focused == Some(w) {
                old_ws.focused = None;
            }
        }

        // Add to target workspace, on top and as its remembered focus: a
        // window sent to another workspace should be what greets you there.
        if let Some(target_ws) = self.workspaces.iter_mut().find(|ws| ws.id == target_id) {
            target_ws.windows.push(w);
            target_ws.focused = Some(w);
        }

        // Clean up old workspace if empty and not the last one left
        if self.workspace_is_empty(old_ws_id) && self.workspaces.len() > 1 {
            self.remove_workspace_if_empty(old_ws_id);
        }

        // Update focus if window was focused and moved away from active workspace
        if self.focused == Some(w) && self.active != target_id {
            self.focused = self.pick_new_focus();
        }

        true
    }

    /// Adds a window to layout according to current policy.
    /// Returns the assigned `WorkspaceId`.
    pub fn add_window(&mut self, w: WindowId, initial_size: Size) -> WorkspaceId {
        let ws_id = match self.policy {
            NewWindowPolicy::ActiveWorkspace => self.active,
            NewWindowPolicy::NewWorkspace => {
                // If active workspace is currently empty, reuse it instead of creating another
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

        // Insets are always NONE here: they are set afterwards via
        // `set_window_insets`, once the state below exists to hold them (see
        // that method's docs). `initial_size` is also always empty for real
        // windows — see `content_geometry`'s docs on deferred placement —
        // so in practice this branch and its lack of insets are only ever
        // exercised by tests that pass a size directly.
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

    /// Adds a window to layout associated with a parent window.
    /// Places the child window on the parent's workspace.
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

        // See the equivalent branch in `add_window`: insets aren't known yet.
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
            // Remember it as that workspace's focus even when the workspace is
            // not active, so switching there lands on the dialog.
            ws.focused = Some(w);
        }

        // Only steal focus when the parent's workspace is the one on screen.
        // A background application spawning a dialog must not yank the user
        // away from what they are doing.
        if target_ws == self.active {
            self.focused = Some(w);
        }

        target_ws
    }

    /// Sets window maximized state.
    ///
    /// Covers the work area (`geometry` minus panels), not the full output —
    /// a maximized window must not tuck itself under a bar.
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

    /// Sets window fullscreen state.
    ///
    /// Covers the full output, panels included — unlike maximize, fullscreen
    /// is meant to take over the whole screen.
    pub fn set_fullscreen(&mut self, w: WindowId, fullscreen: bool) -> bool {
        let output_geom = self.primary_output_geometry();

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
            if !state.maximized {
                if let Some(saved) = state.saved_geometry.take() {
                    state.geometry = saved;
                }
            }
            state.fullscreen = false;
        }
        true
    }

    /// Toggles maximized state for window.
    pub fn toggle_maximized(&mut self, w: WindowId) -> bool {
        let is_max = self.windows.get(&w).is_some_and(|s| s.maximized);
        self.set_maximized(w, !is_max)
    }

    /// Toggles fullscreen state for window.
    pub fn toggle_fullscreen(&mut self, w: WindowId) -> bool {
        let is_full = self.windows.get(&w).is_some_and(|s| s.fullscreen);
        self.set_fullscreen(w, !is_full)
    }

    /// Sets a window's minimized state.
    ///
    /// Minimizing the focused window moves focus to whatever else is visible
    /// on the workspace (or clears it, if nothing is left) — a minimized
    /// window keeping focus would mean keybindings silently act on something
    /// no longer on screen.
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

    /// Toggles minimized state for window.
    pub fn toggle_minimize(&mut self, w: WindowId) -> bool {
        let is_min = self.windows.get(&w).is_some_and(|s| s.minimized);
        self.set_minimized(w, !is_min)
    }

    /// Records a window's current content size, placing it the first time a
    /// real size is known.
    ///
    /// `size` is the *content* size — what the client itself reports, insets
    /// excluded — matching what `content_geometry` later hands back out.
    ///
    /// Returns `true` if this call performed the initial placement, i.e. the
    /// caller needs to push a new position to the window.
    ///
    /// Placement happens exactly once. A client attaches its first buffer
    /// some time after the toplevel is created, so a window has no size to
    /// centre until then — but re-centring on every later commit would drag
    /// windows back to the middle of the screen whenever their content
    /// changed size, and would silently undo any move by the user.
    pub fn update_window_size(&mut self, w: WindowId, size: Size) -> bool {
        let Some(state) = self.windows.get(&w) else {
            return false;
        };

        // Already positioned: track the new size in place, keeping the
        // top-left corner where it is. The frame grows/shrinks by the same
        // amount as the content — insets themselves never change size.
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

    /// Sets a window's decoration insets (see [`WindowState::insets`]).
    ///
    /// Should be called once, right after [`Layout::add_window`] /
    /// [`Layout::add_window_with_parent`] and before the first real size
    /// arrives via [`Layout::update_window_size`] — that ordering is what
    /// lets the very first placement already account for the decoration,
    /// rather than the window jumping slightly when insets are applied
    /// retroactively.
    pub fn set_window_insets(&mut self, w: WindowId, insets: Insets) -> bool {
        let Some(state) = self.windows.get_mut(&w) else {
            return false;
        };
        state.insets = insets;
        true
    }

    /// Removes a window from the layout.
    /// Returns `true` if window was found and removed.
    pub fn remove_window(&mut self, w: WindowId) -> bool {
        let Some(state) = self.windows.remove(&w) else {
            return false;
        };

        let ws_id = state.workspace_id;

        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == ws_id) {
            ws.windows.retain(|&id| id != w);
            // Drop the remembered focus too, otherwise returning to this
            // workspace would try to focus a window that no longer exists.
            if ws.focused == Some(w) {
                ws.focused = None;
            }
        }

        // Clean up workspace if empty and not the last one left
        if self.workspace_is_empty(ws_id) && self.workspaces.len() > 1 {
            self.remove_workspace_if_empty(ws_id);
        }

        // Update focus if removed window was focused
        if self.focused == Some(w) {
            self.focused = self.pick_new_focus();
        }

        true
    }

    /// Focuses the given window, switching to its workspace, raising it to
    /// the top of that workspace's stack, and un-minimizing it.
    ///
    /// This is the single entry point for focus changes; the compositor must
    /// not set keyboard focus behind the layout's back, or the two disagree
    /// about which window keybindings act on.
    ///
    /// Un-minimizing here (rather than requiring a separate call) is also
    /// currently the *only* way to bring a minimized window back: there is no
    /// taskbar yet, so `glidectl window focus <id>` doubles as "restore".
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

    /// Drops focus entirely, e.g. after a click on the desktop background.
    ///
    /// The workspace's remembered focus is cleared too, so returning to it
    /// later does not resurrect a selection the user explicitly dismissed.
    pub fn clear_focus(&mut self) {
        self.focused = None;
        self.remember_focus();
    }

    /// Sets explicit geometry for a window.
    pub fn set_window_geometry(&mut self, w: WindowId, geometry: Rectangle) -> bool {
        if let Some(state) = self.windows.get_mut(&w) {
            state.geometry = geometry;
            state.is_placed = true;
            true
        } else {
            false
        }
    }

    /// Centres a window's frame in the primary output's work area, offset by
    /// a cascade step so the n-th window on a workspace does not sit exactly
    /// on its predecessor.
    ///
    /// `content_size` plus `insets` is what gets centred — the whole
    /// decorated frame, titlebar included, not just the content — and the
    /// returned rectangle is that frame, ready to store as
    /// [`WindowState::geometry`] directly.
    ///
    /// Centred in the work area, not the full output: a newly placed window
    /// must not appear under a panel any more than a maximized one should.
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

        let offset = i32::try_from(index_on_ws).unwrap_or(i32::MAX) * CASCADE_STEP;
        let x = work_area.loc.x + (work_area.size.width - frame_size.width) / 2 + offset;
        let y = work_area.loc.y + (work_area.size.height - frame_size.height) / 2 + offset;

        Rectangle::new(Point::new(x, y), frame_size)
    }

    // Helper functions

    /// Re-centres only those windows that no longer overlap any output.
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
            // `geometry.size` here is already the full frame (decorations
            // included), so it goes through as-is: `Insets::NONE` leaves a
            // size unchanged rather than adding a second helping of insets
            // on top of what `geometry` already accounts for.
            let Some(size) = self.windows.get(&id).map(|s| s.geometry.size) else {
                continue;
            };
            let geometry = self.calculate_centered_geometry(size, Insets::NONE, 0);
            if let Some(state) = self.windows.get_mut(&id) {
                state.geometry = geometry;
            }
        }

        // Maximized and fullscreen windows track the output itself, so they
        // must follow a resolution change even though they are never
        // stranded. Each tracks its own reference rect (see `set_maximized`
        // / `set_fullscreen` for why they differ).
        let work_area = self.primary_output_work_area();
        let output_geom = self.primary_output_geometry();
        for state in self.windows.values_mut() {
            // Fullscreen wins when both are set (a maximized window can still
            // be fullscreened without unmaximizing first) — it is meant to
            // cover panels, maximize is not.
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

    /// Stores the current focus on the active workspace, so switching away and
    /// back returns to the window the user was actually on.
    fn remember_focus(&mut self) {
        let focused = self.focused;
        let active = self.active;
        if let Some(ws) = self.workspaces.iter_mut().find(|ws| ws.id == active) {
            ws.focused = focused;
        }
    }

    /// Focus to use for the active workspace: whatever was focused there
    /// last, falling back to the topmost non-minimized window.
    ///
    /// Never returns a minimized window: picking one back up as "focused"
    /// would immediately un-minimize it as a side effect of `focus_window`,
    /// undoing the very thing that triggered this pick in the first place.
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
