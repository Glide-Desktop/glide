# Project: Glide

A Wayland compositor built on Smithay, growing toward a full desktop
environment. See `docs/ROADMAP.md` for milestone status and the plan
beyond the compositor itself; see `docs/VERIFY.md` for what needs manual
verification on real hardware (tty3) and can't be caught by tests alone.

## Workspace crates

| Crate | Role | Smithay/Wayland dependency |
|---|---|---|
| `glide-compositor` | The compositor binary: DRM/TTY backend, protocol handlers, input, rendering, IPC server | Yes |
| `glide-layout` | Window/workspace layout engine — dynamic workspaces, floating placement, multi-output | No — pure logic, this is what makes most of it unit-testable |
| `glide-geom` | `Point`/`Size`/`Rectangle`/`Insets` | No |
| `glide-theme` | `Color`, `DecorationTheme`, `OverviewTheme` — shared theme types, TOML-deserializable | No |
| `glide-config` | XDG config path resolution (`$XDG_CONFIG_HOME/<component>/<file>`, `$HOME/.config` fallback) | No |
| `glide-paint` | Text shaping/rendering (`cosmic-text`) and vector chrome drawing (`tiny-skia`) | No |
| `glide-ui` | Minimal widget primitives: `HoverAnimation<T>`, row/column layout, hit-testing | No |
| `glide-ipc` | Wire types shared between the compositor's IPC server and `glidectl` (`Request`/`Response`/`Event`) | No |
| `glidectl` | CLI client — one-shot commands plus `subscribe` for the live event stream | No |

The crates with no Wayland dependency exist so a future out-of-process
shell component (panel, dock, notification center — see `docs/ROADMAP.md`
Phase C) can depend on them directly instead of on the whole compositor
binary.

## `glide-compositor` code layout

- `main.rs` — entry point, event loop, session environment setup.
- `state.rs` — `GlideState` (global compositor state) and `Protocols`
  (grouped Wayland-protocol server states — see below).
- `handlers/` — one file per protocol handler (`xdg_shell.rs`,
  `layer_shell.rs`, `wlr_output_management.rs`, `ext_workspace.rs`,
  `foreign_toplevel_list.rs`, `idle_notify.rs`, `compositor.rs`, `mod.rs`).
- `decoration/` — server-side window decorations: `header.rs` (hit-testing,
  animation, redraw — one cohesive state machine, kept together on
  purpose), `input.rs` (pointer routing), `render.rs` (buffer generation),
  `menu.rs` (the titlebar chevron dropdown), `mod.rs` (shared state types).
- `window.rs` — `GlideWindow`, the wrapper Smithay's `Space` operates on.
- `overview.rs` — the `Super+O` workspace switcher.
- `input.rs` — keyboard/pointer event dispatch, keybind resolution.
- `grabs/` — interactive move/resize pointer grabs.
- `config.rs` — TOML config loading, keybind parsing; re-exports
  `glide-theme`'s types so existing `crate::config::X` paths still resolve.
- `ipc.rs` — the IPC server: request/response dispatch plus
  `IpcSubscribers` (the event-stream fan-out).
- `backend/tty.rs` — DRM/GBM/libseat backend; the only backend. There is
  no nested/windowed backend — see `docs/ROADMAP.md` Phase A for why, and
  `docs/VERIFY.md` for how changes get verified without one.

## Interface contracts

- `glide-layout` has zero Smithay/Wayland dependencies, and stays that way.
  Protocol handlers translate between Smithay types and `glide-layout`'s
  own `WindowId`/`WorkspaceId`/`OutputId`/`Rectangle` at the boundary.
- `GlideState::protocols: Protocols` holds every Wayland protocol global's
  server-side state. A new protocol handler adds its state field there,
  not to `GlideState` directly.
- IPC is newline-delimited JSON over a Unix socket
  (`$XDG_RUNTIME_DIR/glide-<wayland-socket>.sock`, mode 0600). Ordinary
  requests get exactly one response and the connection closes.
  `Request::Subscribe` upgrades a connection to a one-way event stream
  (`Response::Subscribed` once, then `Event` lines indefinitely) — see
  `IpcSubscribers::broadcast` in `ipc.rs`. Every write on that path is a
  single non-blocking attempt with no retry; a subscriber that isn't
  reading fast enough is dropped, never allowed to stall the compositor.

## Conventions

- No comments in `.rs` files, including doc comments (`///`). Write
  clear names and, where genuinely necessary, put explanation in the
  commit message instead.
- Every commit passes all four gates: `cargo build --workspace`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets`
  (`clippy::all` denied, `clippy::pedantic` warned, both workspace-wide),
  `cargo fmt --check`.
- Golden-image regression tests live under
  `<crate>/testdata/goldens/{chrome,text}/`. A mismatch fails the test
  and writes `*.actual.png` beside the reference for human review — it
  never overwrites the reference automatically.
- No nested/windowed compositor backend exists. Visual and interaction
  changes are verified by a human on a real tty per `docs/VERIFY.md`;
  everything that can be checked without one (pixel output, hit-test
  boundaries, layout math, protocol serialization) has an automated test
  instead.
