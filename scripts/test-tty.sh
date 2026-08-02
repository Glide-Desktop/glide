#!/usr/bin/env bash
# Manual M1.2a verification helper. Run this ONLY after switching to a free
# VT (e.g. Ctrl+Alt+F3) and logging in — not from inside the GNOME session.
#
# IMPORTANT — the screen briefly FREEZES, then should turn solid #295396
# blue within about a second. The freeze is the same KD_GRAPHICS/K_OFF gap
# as M1.1 (taking the session makes logind stop the kernel from drawing the
# text console or handling Ctrl+Alt+Fn itself); the blue fill is Glide's own
# first DRM/KMS-driven frame. If the screen stays frozen well past a second,
# something in modesetting failed — that is the main thing this script is
# for.
#
# Safety net: the compositor is killed after TIMEOUT seconds no matter what,
# so a stuck session can never lock you out of the tty.
#
# What to do while it runs:
#   - watch for the screen to turn solid #295396 blue (the pass/fail signal)
#   - type a few distinct keys (e.g. a, b, c)
#   - move the mouse / touchpad
#   - Ctrl+Alt+F2 to switch to GNOME, then Ctrl+Alt+F3 to come back
#   - Super+Q to quit early (otherwise wait for the timeout)

set -uo pipefail

TIMEOUT="${TIMEOUT:-60}"

if [ -n "${WAYLAND_DISPLAY:-}" ] || [ -n "${DISPLAY:-}" ]; then
    echo "error: WAYLAND_DISPLAY or DISPLAY is set — this looks like a" >&2
    echo "       nested graphical session, not a bare tty. Refusing to run:" >&2
    echo "       this script is only meant for the real tty verification." >&2
    exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_root/target/debug/glide"
log_file="/tmp/glide-tty.log"

if [ ! -x "$binary" ]; then
    echo "error: $binary not found. Build it first from the GNOME session:" >&2
    echo "       cd $repo_root && cargo build" >&2
    exit 1
fi

cat <<EOF
Starting $binary for up to ${TIMEOUT}s.

The screen will briefly freeze, then should turn solid #295396 blue —
that's the pass/fail signal. Then:

  1. press a few keys (a, b, c)
  2. move the mouse / touchpad
  3. Ctrl+Alt+F2, then Ctrl+Alt+F3
  4. Super+Q to quit early

EOF
sleep 3

# NO_COLOR keeps the log free of ANSI escapes, which would otherwise split
# field names apart and break both grepping and human reading.
timeout "$TIMEOUT" env RUST_LOG=debug NO_COLOR=1 "$binary" > "$log_file" 2>&1
status=$?

# The console comes back once the session is released, so it is safe to
# print the verdict here instead of making the user switch VTs to read it.
echo
if [ "$status" -eq 124 ]; then
    echo "Stopped by the ${TIMEOUT}s safety timeout."
else
    echo "Process exited on its own (status $status) — Super+Q likely worked."
fi

echo
echo "=== M1.2a acceptance checks ==="

# Strip ANSI escapes defensively: even with NO_COLOR some libraries may
# still emit them, and a stray escape must not turn a pass into a fail.
plain_log="$(mktemp)"
trap 'rm -f "$plain_log"' EXIT
sed -E 's/\x1b\[[0-9;]*m//g' "$log_file" > "$plain_log"

check() {
    local label="$1" pattern="$2"
    local n
    n="$(grep -acE "$pattern" "$plain_log" 2>/dev/null || true)"
    n="${n:-0}"
    if [ "$n" -gt 0 ]; then
        printf '  [ OK ]  %-34s (%s matches)\n' "$label" "$n"
    else
        printf '  [FAIL]  %-34s (none)\n' "$label"
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
        printf '  [ OK ]  %-34s (none)\n' "$label"
    else
        printf '  [FAIL]  %-34s (%s matches)\n' "$label" "$n"
    fi
}

check "seat0 detected"            'name="seat0"'
check "libinput devices added"    'New device'
check "keyboard events"           'keysym='
check "relative pointer motion"   'pointer motion \(relative\)'
check "vt switch via our handler" 'Session switch'
check "session paused"            'session paused'
check "session resumed"           'session resumed'
check "primary gpu opened"        'opening primary gpu'
check "connector connected"       'connector connected'
check_absent "no panics"          'panicked at|RUST_BACKTRACE'
check_absent "no render errors"   'failed to render frame|failed to queue frame'
check_absent "no drm errors"      'drm error:'

echo
echo "Full log: $log_file"
