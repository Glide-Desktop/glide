#!/bin/sh
# Kept for existing muscle memory: builds as the calling user, then installs
# system-wide. See scripts/build.sh and scripts/install.sh.
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
"$REPO/scripts/build.sh"
sudo "$REPO/scripts/install.sh" --dm "$@"

OLD_UNIT="$HOME/.config/systemd/user/glide.service"
if [ -f "$OLD_UNIT" ]; then
    rm -f "$OLD_UNIT" "$HOME/.config/systemd/user/glide-shutdown.target"
    echo "removed the old per-user units in ~/.config/systemd/user"
fi
systemctl --user daemon-reload 2>/dev/null || true
