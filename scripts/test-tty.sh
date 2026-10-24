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
# so a stuck session can never lock you out of the tty. It only runs the
# automated log checks below; interaction still needs a person at the screen.

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
# Interaction that needs a human — keyboard, vt-switch, overview, IPC hang
# safety, and everything else that can't be grepped out of a log — is checked
# by hand. Also excluded on purpose: xcursor theme (an
# environment detail, not a compositor bug signal), and client-connected /
# window-mapped (client startup races the compositor; a flaky alacritty
# launch shouldn't mask a clean compositor run, or vice versa).
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
