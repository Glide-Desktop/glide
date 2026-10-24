# Roadmap

Conventions that bind every crate (see also AGENTS.md):

- No comments in `.rs` files, doc comments included. Keep `// SAFETY:` on
  `unsafe` blocks and a short reason on an `#[allow]`. Record invariants as
  tests.

## Goal

Make it possible for applications that are not part of this repository to
integrate as deeply as Files, Terminal or the NetBird integration do today,
without code in the compositor. The shell draws everything in Meridian;
applications describe data and the shell renders it.

## Platform track

### P1. One versioned contract, one identity, one permission store

- Interface XML for every `org.glide.*` D-Bus interface lives in
  `data/dbus/`, and a test checks that the served interface matches it.
  Versions sit in the interface name (`Shell1`, `Shell2`), and a
  `Capabilities` property lists the optional parts.
- Rule: per-window state goes over Wayland, session state over D-Bus. The IPC
  socket stays for `glidectl` and tests.
- `glide-app` is the public, UI-free client crate with a stable API;
  `glide-sdk` is the iced layer on top.
- One resolver, two tiers. The attested identity is what permissions key on:
  the security-context app id set by the sandbox launcher, or the program from
  `/proc/<pid>/exe` of the socket or D-Bus peer (the key `permissions.toml`
  already uses). The presented identity (name, icon, grouping) is the desktop
  entry matched to the attested one. A Wayland `app_id` and `.desktop` files
  are written by the client itself, so they never grant anything.
- One permission store keyed by the attested identity serves Wayland globals,
  D-Bus methods and portals. Requests that arrive together are asked in one
  prompt. `.desktop` files may declare
  `X-Glide-Capabilities=clipboard;screen;input` to pre-fill that prompt.

### P2. Quick settings tiles from applications

`org.glide.QuickSettings1`: a tile (toggle, label, state) and a declarative
detail view (rows, switches, lists, buttons). NetBird moves out of the
compositor into its own agent process on top of it.

### P3. Shared system services

- `org.glide.Status1`: the compositor's aggregated network, audio, battery,
  Bluetooth and privacy state as properties with change signals.
- `org.glide.Apps1`: the desktop-entry index (search, icons, which window
  belongs to which app), so Files, Settings and others stop scanning.
- Settings schemas: an app ships `X-Glide-Settings=<schema>` and Settings
  renders the panel. The Terminal panel moves to a schema, and Settings stops
  depending on `glide-terminal`.

### P4. Island, windows, portals

- `org.glide.Island1`: live activities from templates (timer, progress, call,
  status).
- `glide_window_v1` Wayland protocol: per-surface frame style (replaces
  `FrameWindows`), edited-document mark, window progress, attention.
- Portal backends in `glide-portal`: ScreenCast (PipeWire), GlobalShortcuts,
  Inhibit, Background, Settings (`org.freedesktop.appearance`).
- `org.freedesktop.Application` (DBusActivatable) support in the SDK, used by
  the dock and launcher to open files and run actions in a running app.

### P5. Exchange between applications

`org.glide.Share1` share sheet with targets declared in `.desktop` files, and
action handlers beyond MIME types.

## User tasks (user_plan.md)

| # | Task | State on 2026-10-24 | Where it lands |
|---|---|---|---|
| 1 | GTK and Qt follow Glide | Theme export, adw-gtk3, qt5ct/qt6ct and Kvantum exist; Qt apps were never checked live | Needs the concrete failures. xdg-desktop-portal-gtk already serves `org.freedesktop.appearance` from the gsettings Glide writes |
| 2 | First-run onboarding | Only a welcome notification (`welcome.rs`) | `glide-welcome` SDK app on top of P3 |
| 3 | "What's new" window with animated demos | Absent | Same app as 2, opened once per new version |
| 4 | Dock: several windows per app | Done: one dot per window (up to three), the wheel walks the windows, hovering shows live window previews; clicks stay macOS-style | — |
| 5 | Dock: progress from a window | `LauncherEntry` progress per desktop ID exists | Check which apps send it; window progress in `glide_window_v1` (P4) |
| 6 | Launcher animation in step with the overview | Done: the overview's scene length and curves (content E_OUT, backdrop E_IO) | — |
| 7 | Lock N seconds after the screen blanks | `[general] lock-after-seconds` exists (0 = with the blank; skipped without libpam) | Exists; needs what did not happen on the user's machine |
| 8 | Connected headset widget | Done: island glance on connect, a top-bar applet with the battery that opens a popover (battery, volume, mode, codec, play here, disconnect) | Per-bud battery, ANC and in-ear state need vendor data BlueZ does not expose |
| 9 | ScreenCast | Routed to xdg-desktop-portal-wlr over SHM screencopy; its chooser is not configured and the failure was not recorded | Find what fails first; native PipeWire backend with a Glide picker in P4 |
| 10 | Tiling | Edge snapping and keyboard halves only | Separate track after P2 |

## Order

1. P1.
2. Shell polish: 6, 4, 8 (and 7 once verified).
3. P2 with the NetBird agent.
4. P3, then 2 and 3 on top of it.
5. P4 (9 and 5 inside it), then 10, then P5.
