#!/usr/bin/env bash
# Manual M1.1 verification helper. Run this ONLY after switching to a free
# VT (e.g. Ctrl+Alt+F3) and logging in — not from inside the GNOME session.
#
# IMPORTANT — the screen will appear FROZEN while this runs. That is expected:
# taking control of the session makes logind put the tty into KD_GRAPHICS
# (kernel stops drawing the text console) and K_OFF (kernel stops handling
# keys, including Ctrl+Alt+Fn). Glide does not render anything yet, so what
# you see is simply the last framebuffer contents. Rendering arrives in M1.2.
#
# Consequence: you are typing blind. That is fine — everything lands in the
# log, and this script prints a summary at the end so you don't have to
# switch back to read it.
#
# Safety net: the compositor is killed after TIMEOUT seconds no matter what,
# so a stuck session can never lock you out of the tty.
#
# What to do while it runs (blind):
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

The screen is about to FREEZE — this is expected, not a crash.
You will be typing blind. Do this, then wait:

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
echo "=== M1.1 acceptance checks ==="

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

check "seat0 detected"            'name="seat0"'
check "libinput devices added"    'New device'
check "keyboard events"           'keysym='
check "relative pointer motion"   'pointer motion \(relative\)'
check "vt switch via our handler" 'Session switch'
check "session paused"            'session paused'
check "session resumed"           'session resumed'

echo
echo "Full log: $log_file"
