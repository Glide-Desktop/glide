# Project: Glide Wayland Compositor

## Architecture
- Workspace crates:
  - `crates/glide-compositor`: Core Wayland compositor built on Smithay (winit/DRM backends, xdg-shell, layer-shell, M5 protocol integration).
  - `crates/glide-layout`: Pure window management layout engine (SlotMap based, workspace & floating logic, zero Smithay dependencies).
  - `crates/glide-ipc`: Common request/response IPC types between compositor and `glidectl`.
  - `crates/glidectl`: CLI client for controlling the compositor.
- Protocol Integration (M5):
  - `wlr-output-management`: Output management protocol for display configuration.
  - `ext-workspace-v1`: Workspace management protocol.
  - `ext-foreign-toplevel-list-v1`: Foreign toplevel management & tracking protocol.
  - `ext-idle-notify-v1`: Idle notification protocol for screen power/dimming.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M-R1 | Code Review & Refactoring | Audit & fix issues across M0-M4 crates (`glide-compositor`, `glide-layout`, `glide-ipc`, `glidectl`), check Rc leak risks, fix clippy/linter warnings | None | DONE |
| M-R2-P1 | Protocol Integration Phase 1 | Implement `wlr-output-management` and `ext-workspace-v1` in `glide-compositor` | M-R1 | DONE |
| M-R2-P2 | Protocol Integration Phase 2 | Implement `ext-foreign-toplevel-list-v1` and `ext-idle-notify-v1` in `glide-compositor` | M-R2-P1 | DONE |
| M-VERIFY | Final Verification & Test Run | Comprehensive `cargo check`, `cargo clippy`, `cargo test`, and headless/test binary execution | M-R2-P2 | DONE |

## Code Layout
- `crates/glide-compositor/src/main.rs`: Entry point and event loop setup.
- `crates/glide-compositor/src/state.rs`: Compositor global state (`GlideState`).
- `crates/glide-compositor/src/handlers/`: Protocol event handlers (`wlr_output_management.rs`, `ext_workspace.rs`, `foreign_toplevel_list.rs`, `idle_notify.rs`, etc.).
- `crates/glide-layout/src/`: Window layout engine.
- `crates/glide-ipc/src/`: IPC message definitions.
- `crates/glidectl/src/`: CLI implementation.

## Interface Contracts
- Protocol handlers delegate events to `GlideState` methods and update `Space` / `Output` / `Workspace` state cleanly.
- `glide-layout` remains completely independent of Smithay.
