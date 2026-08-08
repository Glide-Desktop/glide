# Manual verification

Glide drives DRM/KMS directly and has no nested backend, so most of what
follows cannot be exercised inside another compositor. Run
`./scripts/test-tty.sh` from a free VT (e.g. `Ctrl+Alt+F3`) after logging
in there. It starts Glide, spawns alacritty, runs a fixed set of automated
log checks, and prints both logs. The steps below are what the automated
checks cannot see — everything that requires a human to actually look at
the screen or press a key.

The screen briefly freezes on startup, then turns solid `#295396` blue,
then alacritty appears on top of it within a second or so. The freeze is
expected: taking control of the session puts the tty into
`KD_GRAPHICS`/`K_OFF`, so the kernel stops drawing the text console before
Glide has drawn anything of its own. If the screen stays frozen well past
a second or two, modesetting failed; if it stays blue with no terminal,
the client connected but never got mapped.

The compositor is killed after `TIMEOUT` seconds no matter what (default:
unlimited, `Super+Q` quits early), so a stuck session can never lock you
out of the tty.

## Required packages

Some checks below need a client Glide doesn't ship and doesn't intend to
ship (`glide-panel`/`glide-dock`/`glide-notification-center` replace
`waybar`/`fuzzel`/`mako` — see `docs/ROADMAP.md` Phase C, not this list).
What's actually needed to run every check that follows:

```
sudo pacman -S wl-clipboard swaylock grim slurp xwayland-satellite noto-fonts-cjk
```

- `wl-clipboard` — `primary-selection`/`data-control` checks (Phase B1)
- `swaylock` — `ext-session-lock-v1` checks (Phase B4); not the product we ship, just the only independent client that exercises the protocol until `glide-lock` exists
- `grim` + `slurp` — `ext-image-copy-capture-v1` checks (Phase B5)
- `xwayland-satellite` — X11 app checks (Phase B6)
- `noto-fonts-cjk` — closes the A3.3 CJK-tofu gap below

A section is marked **BLOCKED** below if the check can't currently be
performed at all, with the reason and what unblocks it.

## Core interaction

- Watch for blue, then for alacritty to appear on top of it.
- Click into the terminal, type a few keys (`a`, `b`, `c`), confirm they show.
- Move the mouse / touchpad.
- `Ctrl+Alt+F2` to switch to GNOME, then `Ctrl+Alt+F3` to come back — alacritty should still be there afterwards.
- If a second monitor is available, plug/unplug it (or hotplug over USB-C/HDMI) and confirm it lights up/disappears without a restart.

## Workspaces

- `Super+Return` (or launching a second client) opens it on a *new* workspace — the first window disappears from view.
- `Super+1` / `Super+2` switch between them; `Super+Tab` cycles.
- `Super+Shift+2` moves the focused window to workspace 2.

## Titlebar and decorations

- Every window has a titlebar: a plain strip with a small square on the far LEFT (maximize) and far RIGHT (close), and a second small square immediately right of the maximize square (the chevron).
- Drag a window by the *plain* part of its titlebar (not on any of the three squares), then press `Super+1` and come back — the window must still be where you dropped it, not re-centred.
- Click the LEFT square: toggles maximize (same effect as `Super+F`).
- Click the RIGHT square: closes the window.
- Click the chevron (second-from-left square): a dropdown appears below it with one entry, "Minimize" — clicking it minimizes the window exactly like `Super+M`; clicking anywhere else while it's open just closes the dropdown without doing anything.
- Resize: move the pointer to just outside any window edge or corner (~8px band) — the cursor must change to the matching resize arrow (up-down at top/bottom, left-right at sides, diagonal at corners); press and drag there resizes the window from that edge. Confirm the cursor goes back to the normal arrow over the titlebar/content.
- Click between two windows on one workspace, then `Super+F`: the maximize must apply to the window you last clicked.
- `Super+M` minimizes the focused window; it must disappear and focus must move to whatever else is visible (or nothing, if it was the only one).
- Decorations negotiate: alacritty/foot keep the server-side titlebar; a GTK app (`glidectl exec nautilus`) should show its own header bar with NO Glide titlebar on top of it, and its content must not jump when that happens.

## Layer-shell — BLOCKED

No layer-shell client is installed or intended to be (`waybar`/`fuzzel`
are being replaced by `glide-panel`/`glide-launcher`, see
`docs/ROADMAP.md` Phase C, not installed as test tools). Until
`glide-panel` exists, layer-shell has **no manual check and no automated
one** — this is a real, accepted gap, not an oversight. Re-enable this
section once `glide-panel` lands, using it as the client. In the
meantime the protocol is exercised only implicitly, if at all.

The checks that used to live here, for when a client is available again:
a panel appears at the screen edge WITHOUT a Glide titlebar; existing
windows must not sit under it; new windows and maximize must respect the
reduced work area (fullscreen must still cover the panel); a launcher
opens and receives keyboard input immediately, Escape closes it and the
keyboard returns to the window that had it; compositor keybinds
(`Super+1..`) must NOT fire while typing into it, but `Ctrl+Alt+F2` must
still switch VTs; a background client fills the screen behind windows.

## IPC

- `glidectl window list` must show `*` under MINIMIZED for a minimized window; `glidectl window focus <id>` on that id must bring it back.
- `glidectl ping`, `glidectl workspace list`, `glidectl window list` from another VT or terminal.
- `nc -U "$GLIDE_IPC_SOCK"` and then just sitting there must NOT freeze the screen (this used to wedge the whole session).

## Overview (`Super+O`)

Open at least three windows on different workspaces first, otherwise there is nothing to look at.

- Previews must show the windows' actual contents, each with its own titlebar shrunk to match. A preview wearing a full-size titlebar, or showing a flat coloured rectangle, is a regression.
- Every workspace appears, in order, not just the current one.
- Opening and closing are animated: previews travel from where the windows really are to their slots and back, ~200ms. An instant snap is a regression.
- Moving the pointer over a preview highlights that one only, and the highlight must track the preview, not the window's old position.
- Arrow keys (or hjkl) move the highlight; Escape closes with NO workspace change; Return/Space picks the highlighted one.
- Clicking a preview switches to that window's workspace and focuses it; clicking empty space just closes the overview.
- Middle-click on a preview closes that window, and the remaining previews re-flow to fill the gap.
- (BLOCKED until `glide-panel` exists — see Layer-shell above) a layer-shell panel stays visible and still takes clicks while the overview is up; a wallpaper stays BEHIND the previews.
- `Ctrl+Alt+F2` must still switch VTs from inside the overview — this is the one thing that must never be swallowed.
- With the overview open and idle, the screen must not be redrawing continuously (this used to burn a full 60Hz repaint on a static image).

## Reading the logs

Two logs are produced by `test-tty.sh`, and both matter:

- Glide's own structured log (`${XDG_STATE_HOME:-$HOME/.local/state}/glide/glide.log`), written directly by the compositor.
- A raw capture of stdout/stderr (`/tmp/glide-tty-raw.log`), where third-party output lands: panics, libinput warnings, and crucially the spawned client's own errors. A past investigation was solved by an alacritty EGL error that appeared only there.

---

# Per-step checklists

Sections below are added as steps land — see `docs/ROADMAP.md` for the
current phase. Each says what changed, what the automated gates already
cover, and what needs a human.

## A3.3 — text rendering (fontdue → cosmic-text)

Golden tests cover deterministic ASCII/Cyrillic rendering with the
embedded font only. They cannot cover system-font fallback or colour
emoji, since those depend on what's actually installed.

- Set a window's title to include an emoji (`printf '\033]0;тест 🔥\007'`
  in alacritty) and confirm it renders in colour, not as a tofu box or a
  grey glyph.
- A CJK title will render as tofu unless a CJK font (e.g. `noto-fonts-cjk`)
  is installed — that's expected, not a bug, until one is.

## A4.1/A4.2 — decoration split, GlideState::protocols

Pure refactors, but A4.1 moved hit-testing and input-handling code across
file boundaries.

- Titlebar buttons: hover each of maximize/chevron/close, click each,
  confirm the chevron menu opens and Minimize works.
- Drag by the plain part of the titlebar; resize from every edge and
  corner; confirm the cursor changes appropriately at each edge.
- `Super+F`, `Super+M` on the last-clicked window of two overlapping ones.

## A5.2/A5.3 — glide-ui migration (titlebar/menu hit-testing, hover animations)

All boundary math is golden-tested (`glide-ui`'s
`matches_the_titlebar_button_cluster_geometry_exactly` and the decoration
crate's `zone_boundaries_are_exact`), but routing pointer *events* through
the new code isn't something a unit test can see.

- Repeat the titlebar/menu checks above — this is the second time that
  code path changed, so it's worth re-checking specifically for anything
  A4.1's check might have missed.
- Open the overview (`Super+O`) with 3+ windows on different workspaces,
  hover between previews, confirm the highlight ring still fades in
  smoothly and follows the correct preview with no lag or stale
  highlight.
- Close the currently-highlighted preview's window while the overview is
  open (e.g. via `glidectl window list` + kill from another VT) and
  reopen the overview — confirm the highlight lands on a remaining
  preview without an unwanted fade-in (this exercises
  `retarget_without_restarting`).

## A6 — IPC event stream

The one thing goldens categorically cannot check: this is live inter-
process communication.

- In one terminal: `glidectl subscribe`. In another: open a window
  (`glidectl exec alacritty`), close it, click between two windows.
  Confirm `window opened`, `window closed` and `focus changed` lines
  appear promptly and correctly.
- With a subscriber connected, run `nc -U "$GLIDE_IPC_SOCK"`, send
  `{"type":"subscribe"}\n`, and then just sit there without reading
  anything — the screen must stay fully responsive (this is the specific
  failure mode `broadcast_never_blocks_on_a_subscriber_whose_buffer_is_full`
  guards against, but only a real session proves it end to end).
- Kill a `glidectl subscribe` process (Ctrl+C) and confirm the compositor
  keeps running normally and later events don't error in the log.

## B1 — clipboard (primary-selection, wlr-data-control, ext-data-control)

Pure protocol wiring — delegation to Smithay's own state machines, no
Glide-side logic to unit test. Needs `wl-clipboard` (see Required
packages above).

- Regular clipboard: `echo hello | wl-copy` in one alacritty, `wl-paste`
  in another must print `hello`.
- Primary selection: select some text in alacritty with the mouse (no
  explicit copy), middle-click in a second alacritty — the selected text
  must be pasted. This is the protocol `wl-copy`/`wl-paste` don't
  exercise by default (they use the regular clipboard); it needs an
  actual text selection.
- `wl-paste --watch cat` in a terminal, then `wl-copy` something in
  another — the watcher must print the new content immediately, proving
  `data-control` (not just the regular per-client clipboard) works.
- Copy in alacritty, close alacritty, paste in a different application —
  content must survive the source client closing (this is what
  `used_sources`/selection ownership handoff is for).

## B2 — xdg-activation-v1

The policy (`token_serial_is_fresh`, `token_is_fresh_enough`) is unit
tested; what a unit test can't see is the actual raise-and-focus
happening in a live session, or a stale/foreign token correctly being a
no-op instead of erroring.

- Open two windows on two different workspaces. From one, trigger an
  action that requests activation of the other (exact trigger depends on
  the client — GTK apps typically do this via `gtk_window_present()`, for
  instance from a second `nautilus` invocation asking to raise an
  existing window). The target window's workspace must switch to and
  focus it, not just reorder it in place.
- An activation request using a token older than 10 seconds must be
  silently ignored — the target window must not raise or steal focus.
- Nothing in the log should error or warn for an activation request that
  gets rejected (stale token, wrong seat) — rejection is a normal,
  silent no-op, not a failure.
