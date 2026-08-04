#!/usr/bin/env bash
# Manual verification helper. Run this ONLY after switching to a free VT
# (e.g. Ctrl+Alt+F3) and logging in — Glide drives DRM/KMS directly and
# cannot run inside another compositor.
#
# IMPORTANT — the screen briefly FREEZES, then should turn solid #295396
# blue, and an alacritty window should appear on top of it within a second
# or so after that. The freeze is expected: taking control of the session
# puts the tty into KD_GRAPHICS/K_OFF, so the kernel stops drawing the text
# console before Glide has drawn anything of its own. If the screen stays
# frozen well past a second or two, modesetting failed; if it stays blue
# with no terminal, the client connected but never got mapped.
#
# Safety net: the compositor is killed after TIMEOUT seconds no matter what,
# so a stuck session can never lock you out of the tty.
#
# What to do while it runs:
#   - watch for blue, then for alacritty to appear on top of it
#   - click into the terminal, type a few keys (a, b, c), confirm they show
#   - move the mouse / touchpad
#   - Ctrl+Alt+F2 to switch to GNOME, then Ctrl+Alt+F3 to come back —
#     alacritty should still be there afterwards
#   - if a second monitor is available, plug/unplug it (or hotplug over
#     USB-C/HDMI) and confirm it lights up/disappears without a restart —
#     this exercises M1.4 hot-plug and can't be scripted here
#   - workspaces: Super+Return (or launch a second client) opens it on a NEW
#     workspace, so the first window disappears from view. Super+1 / Super+2
#     switch between them; Super+Tab cycles. Super+Shift+2 moves the focused
#     window to workspace 2.
#   - every window should now have a titlebar: a plain strip with a small
#     square on the far LEFT (maximize) and far RIGHT (close), and a second
#     small square immediately right of the maximize square (the chevron)
#   - drag a window by the *plain* part of its titlebar (not on any of the
#     three squares), then press Super+1 and come back — the window must
#     still be where you dropped it, not re-centred. This is the first time
#     this is actually testable with alacritty: it draws no titlebar of its
#     own, so before M4.3 there was nothing to drag by at all
#   - click the LEFT square: toggles maximize (same effect as Super+F)
#   - click the RIGHT square: closes the window
#   - click the chevron (second-from-left square): a small dropdown appears
#     below it with one entry, "Minimize" — clicking it minimizes the window
#     exactly like Super+M; clicking anywhere else while it's open just
#     closes the dropdown without doing anything
#   - layer-shell: `glidectl exec waybar` — a panel appears at the screen
#     edge WITHOUT a Glide titlebar, existing windows must not sit under it,
#     and NEW windows and maximize must respect the reduced work area
#     (fullscreen must still cover the panel). `glidectl exec fuzzel` — the
#     launcher opens, receives keyboard input immediately, Escape closes it
#     and the keyboard returns to the window that had it. `glidectl exec
#     "swaybg -c '#336699'"` — background fills the screen behind windows.
#     Compositor keybinds (Super+1..) must NOT fire while typing in fuzzel,
#     but Ctrl+Alt+F2 must still switch VTs
#   - decorations negotiate: alacritty/foot keep the server-side titlebar;
#     a GTK app (if installed — try `TIMEOUT=120 ... glidectl exec gtk4-demo`
#     or zenity) should show its OWN header bar with NO Glide titlebar on
#     top of it, and its content must not jump when that happens
#   - resize: move the pointer to just OUTSIDE any window edge or corner
#     (~8px band) — the cursor must change to the matching resize arrow
#     (up-down at top/bottom, left-right at sides, diagonal at corners);
#     press and drag there resizes the window from that edge. Also check
#     the cursor goes back to the normal arrow over the titlebar/content
#   - click between two windows on one workspace, then Super+F: the maximize
#     must apply to the window you last clicked
#   - Super+M minimizes the focused window; it must disappear and focus must
#     move to whatever else is visible (or nothing, if it was the only one)
#   - `glidectl window list` must show `*` under MINIMIZED for it; running
#     `glidectl window focus <id>` on that id must bring it back
#   - IPC: from another VT or via `glidectl`, run `glidectl ping`,
#     `glidectl workspace list`, `glidectl window list`
#   - M4.1/M4.2 note: windows are routed through a GlideWindow wrapper, and
#     Space positions a window's content area rather than its frame
#     directly — if mapping, stacking, moving, resizing or closing
#     regressed even where a titlebar isn't directly involved, it broke here
#   - IPC must never hang the compositor: `nc -U "$GLIDE_IPC_SOCK"` and then
#     just sitting there must NOT freeze the screen (this used to wedge the
#     whole session)
#   - Super+Q to quit early (otherwise wait for the timeout)
#
# Two logs are produced, and both matter:
#   - Glide's own structured log (see $glide_log below), written directly by
#     the compositor
#   - a raw capture of stdout/stderr, which is where third-party output
#     lands: panics, libinput warnings, and crucially the spawned client's
#     own errors. A past investigation was solved by an alacritty EGL error
#     that appears only there.

set -uo pipefail

TIMEOUT="${TIMEOUT:-0}"
# How many tail lines to show from each log after the session ends.
LOG_LINES="${LOG_LINES:-60}"

if [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; then
    echo "error: WAYLAND_DISPLAY or DISPLAY is set — this looks like a" >&2
    echo "       graphical session, not a bare tty. Glide drives DRM/KMS" >&2
    echo "       directly and must be run from a real tty." >&2
    exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_root/target/debug/glide"
raw_log="/tmp/glide-tty-raw.log"
glide_log="${XDG_STATE_HOME:-$HOME/.local/state}/glide/glide.log"

if [ ! -x "$binary" ]; then
    echo "error: $binary not found. Build it first:" >&2
    echo "       cd $repo_root && cargo build" >&2
    exit 1
fi

if [ "$TIMEOUT" -gt 0 ]; then
    echo "Starting $binary for up to ${TIMEOUT}s."
else
    echo "Starting $binary (running until exit / Super+Q)."
fi

cat <<EOF

The screen will briefly freeze, then turn solid #295396 blue, then an
alacritty window should appear on top of it. Then:

  1. click into the terminal, press a few keys (a, b, c)
  2. move the mouse / touchpad
  3. Ctrl+Alt+F2, then Ctrl+Alt+F3 — alacritty should still be visible
  4. Super+Q to quit early

EOF
sleep 3

if [ "$TIMEOUT" -gt 0 ]; then
    timeout "$TIMEOUT" "$binary" > "$raw_log" 2>&1
    status=$?
else
    "$binary" > "$raw_log" 2>&1
    status=$?
fi

# The console comes back once the session is released, so it is safe to
# print the verdict here instead of making the user switch VTs to read it.
echo
if [ "$status" -eq 124 ]; then
    echo "Stopped by the safety timeout."
else
    echo "Process exited on its own (status $status) — Super+Q likely worked."
fi

# ── Log output ────────────────────────────────────────────────────────────
# Show both logs immediately so there's no need to cat them manually.
# Raw first (chronological order of what the kernel/EGL/libinput printed),
# then Glide's structured log where the interesting compositor events live.
echo
echo "━━━  raw output (stdout/stderr)  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "    $raw_log"
echo "────────────────────────────────────────────────────────────────────────────"
if [ -s "$raw_log" ]; then
    tail -n "$LOG_LINES" "$raw_log"
else
    echo "    (empty)"
fi

echo
echo "━━━  glide structured log  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "    $glide_log"
echo "────────────────────────────────────────────────────────────────────────────"
if [ -f "$glide_log" ] && [ -s "$glide_log" ]; then
    tail -n "$LOG_LINES" "$glide_log"
else
    echo "    (no log — Glide may have died before logging started)"
    echo "    Check the raw output above for the reason."
fi

if [ ! -f "$glide_log" ]; then
    echo >&2
    echo "error: no log at $glide_log — Glide died before logging started." >&2
    echo "       Check $raw_log for the reason." >&2
    exit 1
fi

# ── Acceptance checks ─────────────────────────────────────────────────────
# Checks that only need the compositor to have started successfully.
# Removed:
#   - keyboard events / vt-switch / session-paused / session-resumed:
#     these depend on the user actually pressing keys or switching VTs
#     during the run, so they produce false failures on a quick smoke test.
#   - xcursor theme check: whether the system theme loads is an environment
#     detail, not a compositor correctness signal.
#   - client-connected / window-mapped: client startup races with the
#     compositor; a flaky alacritty launch shouldn't mask a clean compositor
#     run, and vice versa.
echo
echo "=== acceptance checks ==="

# Strip ANSI escapes defensively. Glide writes its own log without them,
# but the raw capture contains third-party output that may not.
plain_log="$(mktemp)"
trap 'rm -f "$plain_log"' EXIT
cat "$glide_log" "$raw_log" | sed -E 's/\x1b\[[0-9;]*m//g' > "$plain_log"

pass=0
fail=0

check() {
    local label="$1" pattern="$2"
    local n
    n="$(grep -acE "$pattern" "$plain_log" 2>/dev/null || true)"
    n="${n:-0}"
    if [ "$n" -gt 0 ]; then
        printf '  [ OK ]  %-44s (%s)\n' "$label" "$n"
        pass=$((pass + 1))
    else
        printf '  [FAIL]  %-44s (none)\n' "$label"
        fail=$((fail + 1))
    fi
}

# Inverse of check(): passes when the pattern is *absent*, for things that
# should never appear in a healthy run.
check_absent() {
    local label="$1" pattern="$2"
    local n
    n="$(grep -acE "$pattern" "$plain_log" 2>/dev/null || true)"
    n="${n:-0}"
    if [ "$n" -eq 0 ]; then
        printf '  [ OK ]  %-44s (none)\n' "$label"
        pass=$((pass + 1))
    else
        printf '  [FAIL]  %-44s (%s occurrences)\n' "$label" "$n"
        fail=$((fail + 1))
    fi
}

# ── Infrastructure: must always succeed regardless of user interaction ─────
check "seat0 detected"                  'seat0'
check "libinput devices added"          'New device'
check "primary gpu opened"              'opening primary gpu'
check "connector connected"             'connector connected'
check "udev hotplug monitor started"    'watching udev for monitor hot-plug'
check "dmabuf global created"           'dmabuf global created'
check "config loaded or defaulted"      'config loaded successfully|using default config'
check "layout outputs synchronized"     'layout outputs synchronized'
check "ipc server started"              'IPC server started'
check "client spawned"                  'spawned client'

# ── Absence checks: things that must never appear in a healthy run ─────────
check_absent "no panics"                'panicked at|RUST_BACKTRACE'
check_absent "no fatal errors"          'fatal:'
check_absent "no render errors"         'failed to render frame|failed to queue frame'
check_absent "no drm errors"            'drm error:'
check_absent "no dmabuf import failures" 'failed to import client dmabuf'
# Rendering while the session is paused fails with DeviceInactive, and the
# error path re-arms the repaint timer — which used to spin at the refresh
# rate for the whole time the user was away on another VT.
check_absent "no repaint spin while paused" 'DeviceInactive'
check_absent "no unknown config keys"   'failed to parse config file'
check_absent "no empty keybind actions" 'keybind action table is empty'
check_absent "no unrecognized keys"     'unrecognized key name in config'

echo
echo "  ${pass} passed, ${fail} failed"
echo
echo "Full logs:"
echo "  Glide : $glide_log  (previous: ${glide_log}.1)"
echo "  Raw   : $raw_log"

[ "$fail" -eq 0 ]
