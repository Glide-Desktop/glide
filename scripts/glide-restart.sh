#!/bin/sh
set -eu

PROFILE="${1:-release}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"

cargo build --profile "$PROFILE" --workspace --manifest-path "$REPO/Cargo.toml"

TARGET_DIR="$REPO/target/$PROFILE"
[ "$PROFILE" = "dev" ] && TARGET_DIR="$REPO/target/debug"

sudo install -Dm755 "$TARGET_DIR/glide" "$BIN_DIR/glide"
sudo install -Dm755 "$TARGET_DIR/glidectl" "$BIN_DIR/glidectl"

if systemctl --user is-active --quiet glide.service 2>/dev/null; then
    systemctl --user restart glide.service
    echo "glide.service restarted with the new build"
elif command -v glidectl >/dev/null 2>&1 && glidectl ping >/dev/null 2>&1; then
    glidectl quit
    echo "running compositor asked to quit; your display manager or launcher restarts it"
else
    echo "no running Glide session found; new binary installed"
fi
