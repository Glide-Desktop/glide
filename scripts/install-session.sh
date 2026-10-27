#!/bin/sh
# Kept for existing muscle memory: builds as the calling user, then installs
# system-wide. See scripts/build.sh and scripts/install.sh.
# The build is throttled like scripts/build.sh; --fast lifts that, every other
# argument goes to scripts/install.sh.
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_FLAGS=""
for arg in "$@"; do
    shift
    if [ "$arg" = --fast ]; then
        BUILD_FLAGS=--fast
    else
        set -- "$@" "$arg"
    fi
done
# shellcheck disable=SC2086
"$REPO/scripts/build.sh" $BUILD_FLAGS
sudo "$REPO/scripts/install.sh" --dm "$@"

OLD_UNIT="$HOME/.config/systemd/user/glide.service"
if [ -f "$OLD_UNIT" ]; then
    rm -f "$OLD_UNIT" "$HOME/.config/systemd/user/glide-shutdown.target"
    echo "removed the old per-user units in ~/.config/systemd/user"
fi
systemctl --user daemon-reload 2>/dev/null || true
