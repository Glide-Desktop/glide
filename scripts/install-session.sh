#!/bin/sh
set -eu

PROFILE="${1:-release}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
SESSION_DIR="${SESSION_DIR:-/usr/share/wayland-sessions}"
UNIT_DIR="${UNIT_DIR:-$HOME/.config/systemd/user}"
PORTAL_DIR="${PORTAL_DIR:-/usr/share/xdg-desktop-portal}"

cargo build --profile "$PROFILE" --workspace --manifest-path "$REPO/Cargo.toml"

TARGET_DIR="$REPO/target/$PROFILE"
[ "$PROFILE" = "dev" ] && TARGET_DIR="$REPO/target/debug"

sudo install -Dm755 "$TARGET_DIR/glide" "$BIN_DIR/glide"
sudo install -Dm755 "$TARGET_DIR/glidectl" "$BIN_DIR/glidectl"
sudo install -Dm755 "$REPO/data/glide-session" "$BIN_DIR/glide-session"
sudo install -Dm644 "$REPO/data/glide.desktop" "$SESSION_DIR/glide.desktop"
sudo install -Dm644 "$REPO/data/glide-portals.conf" "$PORTAL_DIR/glide-portals.conf"

install -Dm644 "$REPO/data/glide.service" "$UNIT_DIR/glide.service"
install -Dm644 "$REPO/data/glide-shutdown.target" "$UNIT_DIR/glide-shutdown.target"
systemctl --user daemon-reload

echo "installed: $BIN_DIR/glide, $BIN_DIR/glidectl, $BIN_DIR/glide-session"
echo "session:   $SESSION_DIR/glide.desktop (pick Glide in your display manager)"
echo "units:     $UNIT_DIR/glide.service, glide-shutdown.target"
echo "portals:   $PORTAL_DIR/glide-portals.conf"
echo
echo "recommended packages for a complete session:"
echo "  xdg-desktop-portal xdg-desktop-portal-gtk xdg-desktop-portal-wlr  (file dialogs, screen share)"
echo "  pipewire wireplumber                                             (audio, screen share)"
echo "  xwayland-satellite                                               (X11 apps)"
echo "  swaylock  brightnessctl  playerctl  wl-clipboard                 (lock, media keys, clipboard)"
echo "  mako or swaync                                                   (notifications)"
