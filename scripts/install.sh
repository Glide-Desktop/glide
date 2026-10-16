#!/bin/sh
# Install (or remove) a built Glide system-wide.
#
# Usage: sudo scripts/install.sh [--prefix DIR] [--destdir DIR] [--from DIR] [--dm] [--uninstall]
#
#   --prefix DIR   install prefix (default /usr/local)
#   --destdir DIR  stage into DIR instead of / (for distribution packages)
#   --from DIR     directory holding the glide, glidectl, glide-settings,
#                  glide-files and glide-terminal binaries (default: bin/ of a
#                  release tarball, else target/release of the source tree)
#   --dm           install and configure glide-dm (via greetd) as system display manager
#   --uninstall    remove every file this script installs
#
# Run scripts/build.sh as your normal user first; this script never compiles.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX=/usr/local
DESTDIR=""
FROM=""
UNINSTALL=0
INSTALL_DM=0

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) PREFIX="$2"; shift 2 ;;
        --prefix=*) PREFIX="${1#*=}"; shift ;;
        --destdir) DESTDIR="$2"; shift 2 ;;
        --destdir=*) DESTDIR="${1#*=}"; shift ;;
        --from) FROM="$2"; shift 2 ;;
        --from=*) FROM="${1#*=}"; shift ;;
        --dm|--with-dm) INSTALL_DM=1; shift ;;
        --without-dm|--no-dm) INSTALL_DM=0; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

PREFIX="${PREFIX%/}"
BINDIR="$PREFIX/bin"
APPDIR="$PREFIX/share/applications"
# Display managers and xdg-desktop-portal only look under /usr/share, whatever
# the prefix, so these two always go there.
SESSIONDIR=/usr/share/wayland-sessions
PORTALDIR=/usr/share/xdg-desktop-portal
PAMDIR=/etc/pam.d
if [ "$PREFIX" = /usr ]; then
    UNITDIR=/usr/lib/systemd/user
else
    UNITDIR="$PREFIX/lib/systemd/user"
fi

BINARIES="glide glidectl glide-settings glide-files glide-terminal"
FILES="$BINDIR/glide $BINDIR/glidectl $BINDIR/glide-settings $BINDIR/glide-files $BINDIR/glide-terminal $BINDIR/glide-session $BINDIR/glide-lock
$APPDIR/glide-settings.desktop $APPDIR/glide-files.desktop $APPDIR/glide-terminal.desktop $SESSIONDIR/glide.desktop $PORTALDIR/glide-portals.conf
$UNITDIR/glide.service $UNITDIR/glide-shutdown.target $PAMDIR/glide-lock"

if [ -z "$DESTDIR" ] && [ "$(id -u)" -ne 0 ]; then
    echo "error: run as root (sudo $0 $*), or pass --destdir to stage files" >&2
    exit 1
fi

if [ "$UNINSTALL" -eq 1 ]; then
    for f in $FILES; do
        if [ -e "$DESTDIR$f" ]; then
            rm -f "$DESTDIR$f"
            echo "removed $DESTDIR$f"
        fi
    done
    if [ -f "$DESTDIR/etc/greetd/config.toml.orig" ]; then
        mv -f "$DESTDIR/etc/greetd/config.toml.orig" "$DESTDIR/etc/greetd/config.toml"
        echo "restored $DESTDIR/etc/greetd/config.toml from backup"
    elif [ -f "$DESTDIR/etc/greetd/config.toml" ] && grep -q "glide --greeter" "$DESTDIR/etc/greetd/config.toml" 2>/dev/null; then
        rm -f "$DESTDIR/etc/greetd/config.toml"
        echo "removed $DESTDIR/etc/greetd/config.toml"
    fi
    if [ -z "$DESTDIR" ] && command -v systemctl >/dev/null 2>&1; then
        if systemctl is-enabled greetd.service >/dev/null 2>&1; then
            systemctl disable greetd.service 2>/dev/null || true
            echo "disabled greetd.service"
        fi
    fi
    echo "Glide uninstalled. Per-user data is kept in ~/.config/glide and ~/.local/state/glide."
    exit 0
fi

if [ -z "$FROM" ]; then
    if [ -x "$ROOT/bin/glide" ]; then
        FROM="$ROOT/bin"
    else
        FROM="$ROOT/target/release"
    fi
fi
for bin in $BINARIES; do
    if [ ! -x "$FROM/$bin" ]; then
        echo "error: $FROM/$bin not found. Build first with: scripts/build.sh" >&2
        exit 1
    fi
done

for bin in $BINARIES; do
    install -Dm755 "$FROM/$bin" "$DESTDIR$BINDIR/$bin"
done
install -Dm755 "$ROOT/data/glide-session" "$DESTDIR$BINDIR/glide-session"
install -Dm755 "$ROOT/data/glide-lock" "$DESTDIR$BINDIR/glide-lock"
install -Dm644 "$ROOT/data/glide-settings.desktop" "$DESTDIR$APPDIR/glide-settings.desktop"
install -Dm644 "$ROOT/data/glide-files.desktop" "$DESTDIR$APPDIR/glide-files.desktop"
install -Dm644 "$ROOT/data/glide-terminal.desktop" "$DESTDIR$APPDIR/glide-terminal.desktop"
install -Dm644 "$ROOT/data/glide-portals.conf" "$DESTDIR$PORTALDIR/glide-portals.conf"
install -Dm644 "$ROOT/data/pam.d/glide-lock" "$DESTDIR$PAMDIR/glide-lock"

install -d "$DESTDIR$SESSIONDIR" "$DESTDIR$UNITDIR"
sed "s|^Exec=.*|Exec=$BINDIR/glide-session|" "$ROOT/data/glide.desktop" > "$DESTDIR$SESSIONDIR/glide.desktop"
chmod 644 "$DESTDIR$SESSIONDIR/glide.desktop"
sed "s|^ExecStart=.*|ExecStart=$BINDIR/glide|" "$ROOT/data/glide.service" > "$DESTDIR$UNITDIR/glide.service"
chmod 644 "$DESTDIR$UNITDIR/glide.service"
install -Dm644 "$ROOT/data/glide-shutdown.target" "$DESTDIR$UNITDIR/glide-shutdown.target"

if [ "$INSTALL_DM" -eq 1 ]; then
    GREETD_DIR=/etc/greetd
    GREETD_CONF="$GREETD_DIR/config.toml"
    install -d "$DESTDIR$GREETD_DIR"
    if [ -f "$DESTDIR$GREETD_CONF" ] && [ ! -f "$DESTDIR$GREETD_CONF.orig" ] && ! grep -q "glide --greeter" "$DESTDIR$GREETD_CONF" 2>/dev/null; then
        cp -p "$DESTDIR$GREETD_CONF" "$DESTDIR$GREETD_CONF.orig"
        echo "backed up existing $GREETD_CONF to $GREETD_CONF.orig"
    fi
    sed "s|command = \"glide --greeter\"|command = \"$BINDIR/glide --greeter\"|" "$ROOT/data/greetd-glide.toml" > "$DESTDIR$GREETD_CONF"
    chmod 644 "$DESTDIR$GREETD_CONF"

    if [ -z "$DESTDIR" ]; then
        if ! id -u greeter >/dev/null 2>&1 && ! id -u _greeter >/dev/null 2>&1; then
            useradd -M -N -r -d /var/lib/greetd -s /bin/false greeter 2>/dev/null || true
        fi
        GREETER_USER="greeter"
        id -u _greeter >/dev/null 2>&1 && GREETER_USER="_greeter"
        if id -u "$GREETER_USER" >/dev/null 2>&1; then
            install -d -m 755 -o "$GREETER_USER" -g "$GREETER_USER" /var/lib/greetd 2>/dev/null || install -d -m 755 /var/lib/greetd
        else
            install -d -m 755 /var/lib/greetd
        fi

        if ! command -v greetd >/dev/null 2>&1; then
            echo
            echo "warning: greetd daemon binary was not found in PATH."
            if command -v pacman >/dev/null 2>&1; then
                echo "         Install it on Arch Linux with: pacman -S greetd"
            elif command -v apt-get >/dev/null 2>&1; then
                echo "         Install it on Debian/Ubuntu with: apt install greetd"
            fi
        fi

        if command -v systemctl >/dev/null 2>&1; then
            echo "enabling greetd.service..."
            systemctl enable greetd.service 2>/dev/null || true
        fi
    fi
fi

echo "installed under ${DESTDIR:-/}:"
for f in $FILES; do
    echo "  $f"
done
if [ "$INSTALL_DM" -eq 1 ]; then
    echo "  /etc/greetd/config.toml (glide-dm)"
fi

if [ -z "$DESTDIR" ]; then
    for home in /home/* /root; do
        if [ -e "$home/.config/systemd/user/glide.service" ]; then
            echo
            echo "warning: $home/.config/systemd/user/glide.service overrides the unit installed"
            echo "         above; delete it so the session starts $BINDIR/glide."
        fi
    done
fi

missing=""
TOOLS="xwayland-satellite wpctl brightnessctl playerctl nmcli bluetoothctl udisksctl"
if [ "$INSTALL_DM" -eq 1 ]; then
    TOOLS="$TOOLS greetd"
fi
for tool in $TOOLS; do
    command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
[ -x /usr/lib/xdg-desktop-portal ] || [ -x /usr/libexec/xdg-desktop-portal ] || missing="$missing xdg-desktop-portal"

echo
if [ "$INSTALL_DM" -eq 1 ]; then
    echo "Glide and glide-dm (via greetd) are installed. Reboot or start greetd.service to test."
else
    echo "Log out and pick \"Glide\" in your display manager."
fi
if [ -n "$missing" ]; then
    echo
    echo "Optional runtime tools not found:$missing"
    echo "  xwayland-satellite   X11 apps"
    echo "  xdg-desktop-portal(-gtk, -wlr), pipewire, wireplumber   file dialogs, screen sharing"
    echo "  wpctl (wireplumber), brightnessctl, playerctl   volume, brightness and media keys"
    echo "  nmcli (NetworkManager), bluetoothctl (bluez)   Wi-Fi and Bluetooth in quick settings"
    echo "  udisksctl (udisks2)   removable drives in Glide Files"
fi
echo
echo "Password prompts for Settings need a polkit agent (polkit-gnome, polkit-kde,"
echo "lxqt-policykit, mate-polkit, xfce-polkit or soteria); Glide starts the first one installed."
echo
echo "Glide has its own notification centre; do not autostart mako, dunst or swaync in this session."
