#!/bin/sh
# Run cargo without heating the laptop: one build job, one test thread, a CPU
# quota of 35 % of the machine and cores capped at 2.5 GHz. AI agents use it for
# every build and test; scripts/build.sh uses it for release builds.
#
# Usage: scripts/ai-cargo.sh <cargo arguments>
#        scripts/ai-cargo.sh test -p glide-layout overview
#        scripts/ai-cargo.sh build -p glide-compositor
#
# GLIDE_AI_CPU_PERCENT (default 35) and GLIDE_AI_MAX_MHZ (default 2500) change
# the limits. The quota needs a systemd user manager. The frequency cap turns
# boost off (amd-pstate reports the nominal clock as the maximum and only boost
# goes above it) and lowers scaling_max_freq where that is still above the cap.
# Both need passwordless sudo for tee on those files, for example in
# /etc/sudoers.d/glide-ai-cargo:
#   artem ALL=(root) NOPASSWD: /usr/bin/tee /sys/devices/system/cpu/cpufreq/boost, /usr/bin/tee /sys/devices/system/cpu/cpufreq/policy[0-9]*/scaling_max_freq
# Whatever is missing is reported on stderr and the build runs anyway.
set -eu

PERCENT="${GLIDE_AI_CPU_PERCENT:-35}"
MAX_KHZ=$((${GLIDE_AI_MAX_MHZ:-2500} * 1000))
QUOTA=$(($(nproc) * PERCENT))

CARGO_BUILD_JOBS=1
RUST_TEST_THREADS=1
export CARGO_BUILD_JOBS RUST_TEST_THREADS

note() {
    echo "ai-cargo: $*" >&2
}

SAVED=""

restore() {
    for entry in $SAVED; do
        printf '%s\n' "${entry#*=}" | sudo -n tee "${entry%%=*}" >/dev/null 2>&1 \
            || note "could not restore ${entry%%=*} to ${entry#*=}"
    done
    SAVED=""
}

write_root() {
    if printf '%s\n' "$2" | sudo -n tee "$1" >/dev/null 2>&1; then
        SAVED="$SAVED $1=$3"
        return 0
    fi
    note "no passwordless sudo for $1; cores are not capped at $((MAX_KHZ / 1000)) MHz"
    return 1
}

cap_frequency() {
    boost=/sys/devices/system/cpu/cpufreq/boost
    nominal=$(cat /sys/devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq 2>/dev/null || echo 0)
    if [ -r "$boost" ] && [ "$(cat "$boost")" = 1 ] && [ "$nominal" -le "$MAX_KHZ" ]; then
        write_root "$boost" 0 1 || return 0
    fi
    for file in /sys/devices/system/cpu/cpufreq/policy*/scaling_max_freq; do
        [ -r "$file" ] || continue
        current=$(cat "$file")
        if [ "$current" -gt "$MAX_KHZ" ]; then
            write_root "$file" "$MAX_KHZ" "$current" || return 0
        fi
    done
}

trap restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cap_frequency

set -- cargo "$@"
if systemd-run --user --scope --quiet --collect true >/dev/null 2>&1; then
    set -- systemd-run --user --scope --quiet --collect -p "CPUQuota=${QUOTA}%" -- "$@"
else
    note "no systemd user manager here; running without the ${PERCENT} % CPU quota"
fi

status=0
"$@" || status=$?
restore
exit "$status"
