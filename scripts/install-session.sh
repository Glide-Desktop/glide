#!/bin/sh
# Kept for existing muscle memory: builds as the calling user, then installs
# system-wide with display manager and PAM integration.
# See scripts/build.sh and scripts/install.sh.
# Flags for build.sh (--fast, --debug, --skip-checks) are forwarded to build.sh;
# all other arguments go to scripts/install.sh.
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_FLAGS=""
INSTALL_FLAGS=""
DEBUG=0

for arg in "$@"; do
    case "$arg" in
        --fast) BUILD_FLAGS="$BUILD_FLAGS --fast" ;;
        --skip-checks) BUILD_FLAGS="$BUILD_FLAGS --skip-checks" ;;
        --debug)
            BUILD_FLAGS="$BUILD_FLAGS --debug"
            DEBUG=1
            ;;
        -h|--help)
            echo "Usage: $0 [--fast] [--debug] [--skip-checks] [--prefix DIR] [--destdir DIR] [--no-dm] [--no-pam]"
            echo "Builds Glide and installs it system-wide with greetd and PAM integration."
            exit 0
            ;;
        *)
            INSTALL_FLAGS="$INSTALL_FLAGS $arg"
            ;;
    esac
done

# shellcheck disable=SC2086
"$REPO/scripts/build.sh" $BUILD_FLAGS

if [ "$DEBUG" -eq 1 ]; then
    INSTALL_FLAGS="--from $REPO/target/debug $INSTALL_FLAGS"
fi

# shellcheck disable=SC2086
sudo "$REPO/scripts/install.sh" --dm --pam $INSTALL_FLAGS

OLD_UNIT="$HOME/.config/systemd/user/glide.service"
if [ -f "$OLD_UNIT" ]; then
    rm -f "$OLD_UNIT" "$HOME/.config/systemd/user/glide-shutdown.target"
    echo "removed the old per-user units in ~/.config/systemd/user"
fi
systemctl --user daemon-reload 2>/dev/null || true
