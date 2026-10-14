# Project: Glide Desktop Environment Completion

## Architecture
- **Compositor & Shell**: `crates/glide-compositor` (Smithay-based Wayland compositor, manages window lifecycle, rendering, input dispatch, topbar, dock, launcher, overview, notifications, quick settings).
- **Layout & Space**: `crates/glide-layout` (Multi-output workspace layout, floating windows, tiling, output geometry, point-to-output resolution).
- **Theme & Motion**: `crates/glide-theme` (Meridian design tokens, motion timings `t1..t4`, bezier easing `E_OUT`/`E_IO`/`E_OVER`, shape radii `R1..R4`, state layers `ST_HOVER`/`ST_PRESS`/`ST_DRAG`, typography).
- **Rendering & Paint**: `crates/glide-paint` (Drawing primitives, text rasterization, global painter generation counter).
- **Input & Gestures**: Interception of libinput gesture events in `input.rs`, 1:1 gesture state tracker, client pass-through via `zwp_pointer_gestures_v1`.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| F1 | Compositor Build Fixes | Resolve 26 compiler errors in `dock.rs`, `notifications.rs`, and `topbar.rs` plus 2 test signature errors | M1 | survey 1 |
| F2 | Topbar & Shell Cache Stability | Eliminate `panel.clear()` multi-monitor thrashing in `topbar.rs:2727` and ensure smooth widget update loop | M1 | survey 1 |
| F3 | 1:1 Touchpad Workspace Swipe | Intercept 3-finger horizontal swipes in `input.rs`, compute normalized progress, non-linear rubber-banding, velocity flick completion | M2 | survey 2 & spec miner |
| F4 | 1:1 Touchpad Overview Swipe | 3-finger vertical swipe driving `Phase::Interactive { progress }` in `OverviewUi` with velocity commit/cancel | M2 | survey 2 & spec miner |
| F5 | Pointer Gestures Passthrough | Forward unconsumed gesture events to `PointerHandle::gesture_*` for Wayland client `zwp_pointer_gestures_v1` | M2 | survey 2 & spec miner |
| F6 | Multi-Output Shell Hit-Testing | Resolve cursor position to correct output via `layout.output_at(pos)` in topbar, dock, launcher, and notifications | M3 | survey 2 & survey 1 |
| F7 | Dynamic Fractional Scale | Track surface output per monitor, broadcast scale dynamically on window move (`update_surface_scales`), sync decorations | M3 | survey 2 & spec miner |
| F8 | Wayland Protocol Integrity | Implement `dismiss_popup` via `PopupManager::dismiss_popup` to prevent ghost popups; protocol sanity | M3 | survey 2 & spec miner |
| F9 | Code Cleanliness & Cache Keys | Strip prohibited comments in `.rs` files per `AGENTS.md`, ensure all render cache keys contain scale + theme rev + painter gen, zero clippy warnings | M4 | survey 1 & spec miner |
| F10 | E2E Testing & Acceptance | 100% pass of E2E test suite (Tiers 1-4) and adversarial verification (Tier 5) | M5 | original request |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Compositor Build & Shell Fixes | F1, F2: Fix 26 compiler errors and 2 test errors in `dock.rs`, `notifications.rs`, `topbar.rs`. Remove `panel.clear()` cache thrashing. | none | DONE |
| M2 | Touchpad Gestures & Motion | F3, F4, F5: 1:1 workspace swipe state machine, 1:1 overview swipe with `Phase::Interactive`, `zwp_pointer_gestures_v1` passthrough. | M1 | PLANNED |
| M3 | Multi-Output & Protocol Integrity | F6, F7, F8: Point-aware multi-output hit-testing, dynamic fractional scale propagation, IME popup dismissal untracking. | M1 | PLANNED |
| M4 | Clean Quality Bar & Cache Keys | F9: Strip forbidden comments in `.rs`, verify all render cache keys (theme rev + painter gen + scale), zero clippy warnings, rustfmt compliance. | M1, M2, M3 | PLANNED |
| M5 | Final E2E Pass & Acceptance | F10: Run full E2E test suite across all tiers, pass 100%, adversarial validation, victory audit. | M1, M2, M3, M4, E2E | PLANNED |

## Interface Contracts
### `crates/glide-compositor/src/input.rs` ↔ `crates/glide-layout`
- Gesture tracking state machine updates workspace indices via `GlideLayout::switch_workspace` and reads `primary_output_geometry` / `output_at(point)`.
- Hit-testing resolves output boundaries via `layout.output_geometry_at(point)` and `layout.work_area_at(point)`.

### `crates/glide-compositor/src/input.rs` ↔ `OverviewUi`
- Overview swipe communicates normalized progress $q \in [0.0, 1.0]$ to `OverviewUi::set_interactive_progress(q)`.
- On gesture end, if $q > 0.35$ or velocity $> 0.5$, transition to `Phase::Opening` or `Phase::Open`; otherwise `Phase::Closing`.

### Render Cache Key Contract
- All render cache keys for cached UI buffers (dock, topbar, launcher, overview, decorations) MUST be keyed by `(..., scale: i32, theme_revision: u64, painter_generation: u64)` to prevent cache staleness.

## Code Layout
- `crates/glide-compositor/src/topbar.rs`: Topbar widgets, render cache keys, panel buffers.
- `crates/glide-compositor/src/dock.rs`: Dock rendering, tile hover geometry, pin management.
- `crates/glide-compositor/src/notifications.rs`: Notification toast buffers, ring animations, reply state.
- `crates/glide-compositor/src/input.rs`: Libinput event processing, gesture state machine, hit-testing dispatch.
- `crates/glide-compositor/src/overview.rs`: Overview UI state machine, `Phase` enum, preview layout.
- `crates/glide-compositor/src/state.rs`: Compositor state, fractional scale handler, output resolution.
- `crates/glide-compositor/src/handlers/`: Wayland protocol event handlers (`ext_workspace.rs`, `input_method.rs`, `selection.rs`, `wlr_output_management.rs`).
