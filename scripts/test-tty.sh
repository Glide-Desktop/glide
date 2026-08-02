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

TIMEOUT="${TIMEOUT:-60}"

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

cat <<EOF
Starting $binary for up to ${TIMEOUT}s.

The screen will briefly freeze, then turn solid #295396 blue, then an
alacritty window should appear on top of it. Then:

  1. click into the terminal, press a few keys (a, b, c)
  2. move the mouse / touchpad
  3. Ctrl+Alt+F2, then Ctrl+Alt+F3 — alacritty should still be visible
  4. Super+Q to quit early

EOF
sleep 3

timeout "$TIMEOUT" "$binary" > "$raw_log" 2>&1
status=$?

# The console comes back once the session is released, so it is safe to
# print the verdict here instead of making the user switch VTs to read it.
echo
if [ "$status" -eq 124 ]; then
    echo "Stopped by the ${TIMEOUT}s safety timeout."
else
    echo "Process exited on its own (status $status) — Super+Q likely worked."
fi

if [ ! -f "$glide_log" ]; then
    echo
    echo "error: no log at $glide_log — Glide died before logging started." >&2
    echo "       Check $raw_log for the reason." >&2
    exit 1
fi

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
        printf '  [ OK ]  %-40s (%s)\n' "$label" "$n"
        pass=$((pass + 1))
    else
        printf '  [FAIL]  %-40s (none)\n' "$label"
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
        printf '  [ OK ]  %-40s (none)\n' "$label"
        pass=$((pass + 1))
    else
        printf '  [FAIL]  %-40s (%s)\n' "$label" "$n"
        fail=$((fail + 1))
    fi
}

check "seat0 detected"                'seat0'
check "libinput devices added"        'New device'
check "keyboard events"               'keysym='
check "vt switch via our handler"     'Session switch'
check "session paused"                'session paused'
check "session resumed"               'session resumed'
check "primary gpu opened"            'opening primary gpu'
check "connector connected"           'connector connected'
check "client spawned"                'spawned client'
check "client connected"              'New client connected'
check "window mapped"                 'new toplevel'
# The built-in fallback cursor exists so the pointer is never invisible,
# but hitting it means the system theme failed to load and the cursor will
# look crude — worth knowing about rather than silently accepting.
check_absent "xcursor theme loaded"   'using built-in fallback cursor'
check_absent "no panics"              'panicked at|RUST_BACKTRACE'
check_absent "no fatal errors"        'fatal:'
check_absent "no render errors"       'failed to render frame|failed to queue frame'
check_absent "no drm errors"          'drm error:'
# Rendering while the session is paused fails with DeviceInactive, and the
# error path re-arms the repaint timer — which used to spin at the refresh
# rate for the whole time the user was away on another VT.
check_absent "no repaint spin while paused" 'DeviceInactive'
# Client-side failures land in the raw capture, not in Glide's own log.
check_absent "client started cleanly" 'InitializationFailed'

echo
echo "  ${pass} passed, ${fail} failed"
echo
echo "Glide log: $glide_log  (previous run: ${glide_log}.1)"
echo "Raw output: $raw_log"

[ "$fail" -eq 0 ]
