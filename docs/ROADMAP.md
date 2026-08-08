# Roadmap

Two tracks. M0–M7 is the original compositor roadmap (winit prototype
through a full desktop environment). Phase A–D is the current
infrastructure and shell-oriented work, started once M0–M5 were done and
the project's ambition grew from "compositor" to "desktop environment."

## M0–M7 — compositor

| # | Scope | Status |
|---|---|---|
| M0 | First pixel: winit backend, `xdg-shell`, seat, keyboard/mouse, client rendering | Done |
| M1 | TTY: udev + DRM/GBM, libseat, `DrmCompositor`, multi-output, hotplug, dmabuf | Done |
| M2 | `glide-layout`: dynamic workspaces, floating placement, new-window policy, dialogs | Done |
| M3 | Config (TOML), keybinds, IPC (unix socket + JSON), `glidectl` | Done |
| M4 | `wlr-layer-shell`, `xdg-decoration` (SSD), spawn `xwayland-satellite`, `cursor-shape-v1`, `fractional-scale-v1`, `viewporter` | 5/7 — missing `ext-session-lock-v1`, `xwayland-satellite` |
| M5 | `wlr-output-management`, `ext-workspace-v1`, `ext-foreign-toplevel-list-v1`, `ext-idle-notify-v1` | Done (plus `idle-inhibit`, not originally scoped) |
| M6 | Damage-driven rendering, direct scanout, VRR, animations, overview | ~65% — damage/scanout done via `DrmOutput::render_frame`; overview and titlebar/hover animations done; VRR and multi-GPU/NVIDIA not started |
| M7 | Shell (GTK4 + `gtk4-layer-shell`, or a custom toolkit), `xdg-desktop-portal` backend, settings daemon | Not started — this is what Phase A–D build toward |

Gaps M0–M7 never covered, needed regardless of M6/M7 status: `text-input-v3` + `input-method-v2` + `virtual-keyboard-v1` (IME), `primary-selection` + `wlr-data-control` (middle-click paste, clipboard managers), `xdg-activation-v1`, `wlr-foreign-toplevel-management-v1` (Smithay doesn't implement this one — would need writing by hand), `ext-image-copy-capture-v1` (screenshots/screencast), `security-context-v1`, `pointer-constraints-v1` + `relative-pointer-v1`, `presentation-time`.

## Phase A–D — infrastructure toward a DE

Decided when the project's scope widened from "window manager" to "desktop
environment": the panel/dock/notification-center/lock-screen envisioned
for M7 needs infrastructure this codebase didn't have — a way to verify
visual changes without a compositor session, crates other processes can
depend on, a UI layer that isn't one 1700-line file, and an IPC that can
push events instead of only answering requests.

### Phase A — Infrastructure (done)

No nested/windowed backend (`winit`) was rebuilt for this — deliberately
rejected; interaction and rendering are verified by hand on tty3 per
`docs/VERIFY.md`, with golden-image tests covering everything that can be
checked without a compositor session.

- **A1** — Golden-image regression harness for `paint.rs`'s pixel output
  (`glide-paint/testdata/goldens/`); `docs/VERIFY.md` for what only a
  human on tty3 can check.
- **A2** — Extracted `glide-geom` (`Point`/`Size`/`Rectangle`/`Insets`),
  `glide-theme` (`Color`/`DecorationTheme`/`OverviewTheme`), `glide-config`
  (XDG path resolution) out of the compositor binary, so a future shell
  process can depend on them without depending on the whole compositor.
- **A3** — Extracted `glide-paint`; fixed a per-frame font deep-clone
  (`Arc<Painter>`); replaced `fontdue` with `cosmic-text` for real text
  shaping, bidi, font fallback and colour emoji.
- **A4** — Split `decoration.rs` (1708 lines) into `decoration/{mod,
  header,input,render,menu}.rs`; grouped `GlideState`'s 18 protocol-state
  fields into `GlideState::protocols: Protocols` so new protocol handlers
  stop growing the top-level struct directly.
- **A5** — `glide-ui`: `HoverAnimation<T>` (replaced two independent,
  near-identical hover-fade implementations in `HeaderBar` and
  `OverviewUi`), `layout_row`/`layout_column`/`hit_test` (replaced
  `HeaderBar`'s hand-rolled titlebar/menu hit-testing). Extracted from
  code that already existed and was already tested twice over, not
  designed against an imagined future consumer.
- **A6** — `glide-ipc` gained `Request::Subscribe` / `Response::Subscribed`
  / `Event`; the compositor fans events out to subscribers with a single
  non-blocking write attempt per subscriber, dropping (not retrying) any
  subscriber that isn't keeping up — the same "must never block the
  compositor" property that IPC request handling and DRM session-pause
  handling were already held to. `glidectl subscribe` exercises it.
  `WindowOpened`/`WindowClosed`/`FocusChanged` are wired to real emission
  points; `TitleChanged`/`AppIdChanged`/`WorkspaceChanged`/`OutputChanged`
  exist in the wire protocol but aren't emitted yet.

### Phase B — Protocols the shell needs

Smithay (at the pinned rev) already implements most of these; wiring a
handler is the work, not implementing the protocol. `wlr-foreign-
toplevel-management-v1` is the one exception — Smithay doesn't have it,
and Glide's own `ext-foreign-toplevel-list-v1` (read-only) plus
`glide-ipc` cover a dock's needs without it, so it's deferred rather than
written by hand up front.

| # | Protocol | Unlocks |
|---|---|---|
| B1 | `primary-selection`, `wlr-data-control`, `ext-data-control` | Middle-click paste, clipboard managers — the most-missed daily-driver gap |
| B2 | `xdg-activation-v1` | A dock/notification click can raise a window |
| B3 | `ext-session-lock-v1` | Screen lock (also closes the M4 gap) |
| B4 | `ext-image-capture-source-v1`, `ext-image-copy-capture-v1` | Screenshots, screen recording, dock/alt-tab preview thumbnails |
| B5 | `text-input-v3`, `input-method-v2`, `virtual-keyboard-v1` | IME |
| B6 | `pointer-constraints-v1`, `relative-pointer-v1`, `presentation-time` | Games, 3D, correctly-paced video |
| B7 | `security-context-v1` | Flatpak sandboxing awareness; lets the shell trust itself specifically |
| B8 | `xwayland-satellite` | Steam, older Electron apps, anything not ported to Wayland — in scope per explicit decision, not optional |

### Phase C — Shell components

Each is a separate process (layer-shell client), not code inside the
compositor — a panel crash must not be a session crash. Built on
`smithay-client-toolkit` + `tiny-skia`, per explicit decision (not
GTK4/iced). Get state via both channels deliberately: standard protocols
(`ext-workspace-v1`, `ext-foreign-toplevel-list-v1`, B4's image capture)
so third-party panels/docks work against Glide too, and `glide-ipc`'s
event stream for anything Glide-specific.

`glide-notification-center` doesn't wait on any Phase B protocol — it's a
D-Bus service (`org.freedesktop.Notifications`), and can be built any
time.

1. `glide-shell-client` — shared layer-shell + theme + IPC-client scaffolding
2. `glide-panel` — clock, workspaces, tray, indicators (needs `ext-workspace-v1`, already done)
3. `glide-dock` — needs B2 (activation) and ideally B4 (previews) to be more than a static icon strip
4. `glide-notification-center` — D-Bus `org.freedesktop.Notifications`, independent of the above
5. `glide-lock` — needs B3

### Phase D — Full DE integration

- `glide-portal` — `xdg-desktop-portal` backend (Screenshot, ScreenCast, Settings, Inhibit); needs B4
- Settings daemon — portal `Settings` interface (theme, accent colour, fonts); GTK4/libadwaita apps won't follow the theme without it
- `glide-session` — systemd user units, a `.desktop` session file, session-scoped D-Bus activation environment (`dbus-update-activation-environment`) — this is the actual fix for "an app opens in the wrong compositor," which a shared D-Bus session bus with another desktop cannot be fixed around from inside Glide alone
- A settings/control-centre UI

## Conventions established along the way

- No comments in `.rs` files, including doc comments (`///`) — established explicitly, re-enforced after a slip during Phase A (see A6.1's commit).
- Every commit passes `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets` (pedantic on, workspace-deny on `clippy::all`), `cargo fmt --check`.
- Golden-image tests live under `<crate>/testdata/goldens/{chrome,text}/`; a mismatch fails the test and writes `*.actual.png` beside the reference for review, never overwrites it silently.
- `glide-layout` stays free of Smithay and Wayland dependencies — it's tested as pure logic and is what makes ~75 of the workspace's 300+ tests possible without any compositor state at all.
- Manual, human-only verification steps live in `docs/VERIFY.md`, not scattered across commit messages or comments in `scripts/test-tty.sh`.
