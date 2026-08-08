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
- Decorations negotiate: alacritty/foot keep the server-side titlebar; a GTK app (if installed — try `TIMEOUT=120 ... glidectl exec gtk4-demo` or `zenity`) should show its own header bar with NO Glide titlebar on top of it, and its content must not jump when that happens.

## Layer-shell

- `glidectl exec waybar` — a panel appears at the screen edge WITHOUT a Glide titlebar; existing windows must not sit under it; new windows and maximize must respect the reduced work area (fullscreen must still cover the panel).
- `glidectl exec fuzzel` — the launcher opens, receives keyboard input immediately, Escape closes it and the keyboard returns to the window that had it. Compositor keybinds (`Super+1..`) must NOT fire while typing in fuzzel, but `Ctrl+Alt+F2` must still switch VTs.
- `glidectl exec "swaybg -c '#336699'"` — background fills the screen behind windows.

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
- A layer-shell panel (`glidectl exec waybar`) stays visible and still takes clicks while the overview is up; a wallpaper stays BEHIND the previews.
- `Ctrl+Alt+F2` must still switch VTs from inside the overview — this is the one thing that must never be swallowed.
- With the overview open and idle, the screen must not be redrawing continuously (this used to burn a full 60Hz repaint on a static image).

## Reading the logs

Two logs are produced by `test-tty.sh`, and both matter:

- Glide's own structured log (`${XDG_STATE_HOME:-$HOME/.local/state}/glide/glide.log`), written directly by the compositor.
- A raw capture of stdout/stderr (`/tmp/glide-tty-raw.log`), where third-party output lands: panics, libinput warnings, and crucially the spawned client's own errors. A past investigation was solved by an alacritty EGL error that appeared only there.

---

# Per-step checklists

Sections below are added as Phase A steps land — see `PROJECT.md` for the
current phase. Each says what changed, what the automated gates already
cover, and what needs a human.
