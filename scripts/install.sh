#!/bin/sh
# Install (or remove) a built Glide system-wide.
#
# Usage: sudo scripts/install.sh [--prefix DIR] [--destdir DIR] [--from DIR] [--dm] [--pam] [--uninstall]
#
#   --prefix DIR   install prefix (default /usr/local)
#   --destdir DIR  stage into DIR instead of / (for distribution packages)
#   --from DIR     directory holding the glide, glidectl, glide-settings,
#                  glide-files, glide-terminal, glide-text, glide-passwords, glide-catalog, glide-portal and glide-keychain binaries (default: bin/ of a
#                  release tarball, else target/release of the source tree)
#   --dm           install and configure glide-dm (via greetd) as system display manager
#   --pam          add pam_glide_keychain to /etc/pam.d/greetd and /etc/pam.d/passwd,
#                  so the login password opens the app keys and a password change follows
#   --uninstall    remove every file this script installs, and its PAM lines
#
# Run scripts/build.sh as your normal user first; this script never compiles.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX=/usr/local
DESTDIR=""
FROM=""
UNINSTALL=0
INSTALL_DM=0
INSTALL_PAM=0

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
        --pam) INSTALL_PAM=1; shift ;;
        --without-pam|--no-pam) INSTALL_PAM=0; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

PREFIX="${PREFIX%/}"
BINDIR="$PREFIX/bin"
APPDIR="$PREFIX/share/applications"
SOUNDDIR="$PREFIX/share/glide/sounds"
# Display managers, xdg-desktop-portal and D-Bus activation only look under
# /usr/share, whatever the prefix, so these always go there.
DATADIR=/usr/share
ICONDIR="$DATADIR/icons/hicolor/scalable/apps"
SESSIONDIR=/usr/share/wayland-sessions
PORTALDIR=/usr/share/xdg-desktop-portal
DBUSDIR=/usr/share/dbus-1/services
INTERFACEDIR=/usr/share/dbus-1/interfaces
SEARCHDIR=/usr/share/gnome-shell/search-providers
AUTOSTARTDIR=/etc/xdg/autostart
PAMDIR=/etc/pam.d
PAM_MODULE=pam_glide_keychain.so
PAM_DIRS="/usr/lib/security /usr/lib64/security /lib/security /lib64/security"
for triplet in x86_64-linux-gnu aarch64-linux-gnu arm-linux-gnueabihf riscv64-linux-gnu powerpc64le-linux-gnu \
    s390x-linux-gnu i386-linux-gnu; do
    PAM_DIRS="$PAM_DIRS /usr/lib/$triplet/security /lib/$triplet/security"
done

if [ -z "$FROM" ]; then
    if [ -x "$ROOT/bin/glide" ]; then
        FROM="$ROOT/bin"
    else
        FROM="$ROOT/target/release"
    fi
fi

# ELF class and machine, so a 64-bit module never lands in the 32-bit PAM
# directory of a multilib system (Fedora and openSUSE keep both).
elf_id() {
    [ -r "$1" ] || return 1
    printf '%s %s\n' "$(od -An -tu1 -j4 -N1 "$1")" "$(od -An -tu1 -j18 -N2 "$1")" | tr -s ' \t' '  '
}

pam_module_dir() {
    wanted="$(elf_id "$FROM/libpam_glide_keychain.so" || elf_id /bin/sh || true)"
    first=""
    for dir in $PAM_DIRS; do
        [ -e "$dir/pam_unix.so" ] || continue
        [ -n "$first" ] || first="$dir"
        if [ -n "$wanted" ] && [ "$(elf_id "$dir/pam_unix.so")" = "$wanted" ]; then
            echo "$dir"
            return
        fi
    done
    echo "${first:-/usr/lib/security}"
}
PAM_MODULEDIR="$(pam_module_dir)"
# The leading "-" lets PAM skip the line silently when the module is gone.
PAM_LINES="greetd:-auth optional $PAM_MODULE
greetd:-session optional $PAM_MODULE
passwd:-password optional $PAM_MODULE"
if [ "$PREFIX" = /usr ]; then
    UNITDIR=/usr/lib/systemd/user
else
    UNITDIR="$PREFIX/lib/systemd/user"
fi

BINARIES="glide glidectl glide-settings glide-files glide-terminal glide-text glide-passwords glide-catalog glide-portal glide-keychain glide-phone glide-welcome glide-xembed"
FILES="$BINDIR/glide $BINDIR/glidectl $BINDIR/glide-settings $BINDIR/glide-files $BINDIR/glide-terminal $BINDIR/glide-text $BINDIR/glide-passwords $BINDIR/glide-catalog $BINDIR/glide-portal $BINDIR/glide-keychain $BINDIR/glide-phone $BINDIR/glide-session $BINDIR/glide-lock
$BINDIR/glide-welcome $BINDIR/glide-xembed $APPDIR/glide-settings.desktop $APPDIR/glide-files.desktop $APPDIR/glide-terminal.desktop $APPDIR/glide-text.desktop $APPDIR/glide-passwords.desktop $APPDIR/glide-catalog.desktop $APPDIR/glide-phone.desktop $APPDIR/glide-welcome.desktop $AUTOSTARTDIR/glide-phone.desktop $ICONDIR/glide-phone.svg $ICONDIR/glide-welcome.svg $ICONDIR/org.kde.kdeconnect.svg $SOUNDDIR/terminal-bell.mp3 $SESSIONDIR/glide.desktop $PORTALDIR/glide-portals.conf
$PORTALDIR/portals/glide.portal $DBUSDIR/org.freedesktop.impl.portal.desktop.glide.service
$DBUSDIR/org.glide.Files.service $DBUSDIR/org.glide.Terminal.service $DBUSDIR/org.glide.Phone.service
$INTERFACEDIR/org.glide.Shell1.xml
$SEARCHDIR/glide-files-search-provider.ini $SEARCHDIR/glide-terminal-search-provider.ini
$UNITDIR/glide.service $UNITDIR/glide-shutdown.target $PAMDIR/glide-lock $PAM_MODULEDIR/$PAM_MODULE"

pam_lines() {
    action="$1"
    echo "$PAM_LINES" | while IFS=: read -r service line; do
        file="$DESTDIR$PAMDIR/$service"
        if [ ! -f "$file" ]; then
            if [ "$action" = add ]; then
                echo "warning: $file not found; \"$line\" was not added, so $service will not use $PAM_MODULE" >&2
            fi
            continue
        fi
        if [ "$action" = add ]; then
            if ! grep -qxF -- "$line" "$file"; then
                printf '%s\n' "$line" >> "$file"
                if grep -qxF -- "$line" "$file"; then
                    echo "added \"$line\" to $file"
                else
                    echo "warning: could not add \"$line\" to $file" >&2
                fi
            fi
        elif grep -qxF -- "$line" "$file"; then
            grep -vxF -- "$line" "$file" > "$file.glide-tmp"
            cat "$file.glide-tmp" > "$file"
            rm -f "$file.glide-tmp"
            echo "removed \"$line\" from $file"
        fi
    done
}

if [ -z "$DESTDIR" ] && [ "$(id -u)" -ne 0 ]; then
    echo "error: run as root (sudo $0 $*), or pass --destdir to stage files" >&2
    exit 1
fi

if [ "$UNINSTALL" -eq 1 ]; then
    pam_lines remove
    for f in $FILES; do
        if [ -e "$DESTDIR$f" ]; then
            rm -f "$DESTDIR$f"
            echo "removed $DESTDIR$f"
        fi
    done
    for dir in $PAM_DIRS; do
        if [ -e "$DESTDIR$dir/$PAM_MODULE" ]; then
            rm -f "$DESTDIR$dir/$PAM_MODULE"
            echo "removed $DESTDIR$dir/$PAM_MODULE"
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

for bin in $BINARIES libpam_glide_keychain.so; do
    if [ ! -e "$FROM/$bin" ]; then
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
install -Dm644 "$ROOT/data/glide-text.desktop" "$DESTDIR$APPDIR/glide-text.desktop"
install -Dm644 "$ROOT/data/glide-passwords.desktop" "$DESTDIR$APPDIR/glide-passwords.desktop"
install -Dm644 "$ROOT/data/glide-catalog.desktop" "$DESTDIR$APPDIR/glide-catalog.desktop"
install -Dm644 "$ROOT/data/glide-phone.desktop" "$DESTDIR$APPDIR/glide-phone.desktop"
install -Dm644 "$ROOT/data/glide-welcome.desktop" "$DESTDIR$APPDIR/glide-welcome.desktop"
install -Dm644 "$ROOT/data/glide.svg" "$DESTDIR$ICONDIR/glide-welcome.svg"
install -Dm644 "$ROOT/data/glide-phone.svg" "$DESTDIR$ICONDIR/glide-phone.svg"
install -Dm644 "$ROOT/data/glide-phone.svg" "$DESTDIR$ICONDIR/org.kde.kdeconnect.svg"
install -Dm644 "$ROOT/data/sounds/terminal-bell.mp3" "$DESTDIR$SOUNDDIR/terminal-bell.mp3"
install -Dm644 "$ROOT/data/glide-portals.conf" "$DESTDIR$PORTALDIR/glide-portals.conf"
install -Dm644 "$ROOT/data/glide.portal" "$DESTDIR$PORTALDIR/portals/glide.portal"
install -d "$DESTDIR$DBUSDIR"
sed "s|^Exec=.*|Exec=$BINDIR/glide-portal|" "$ROOT/data/org.freedesktop.impl.portal.desktop.glide.service" > "$DESTDIR$DBUSDIR/org.freedesktop.impl.portal.desktop.glide.service"
chmod 644 "$DESTDIR$DBUSDIR/org.freedesktop.impl.portal.desktop.glide.service"
for name in glide-files-search-provider.ini glide-terminal-search-provider.ini; do
    install -Dm644 "$ROOT/data/$name" "$DESTDIR$SEARCHDIR/$name"
done
for name in org.glide.Files.service org.glide.Terminal.service org.glide.Phone.service; do
    sed "s|^Exec=/usr/local/bin/|Exec=$BINDIR/|" "$ROOT/data/$name" > "$DESTDIR$DBUSDIR/$name"
    chmod 644 "$DESTDIR$DBUSDIR/$name"
done
install -Dm644 "$ROOT/data/glide-phone-autostart.desktop" "$DESTDIR$AUTOSTARTDIR/glide-phone.desktop"
for name in org.glide.Shell1.xml; do
    install -Dm644 "$ROOT/data/dbus/$name" "$DESTDIR$INTERFACEDIR/$name"
done
install -Dm644 "$ROOT/data/pam.d/glide-lock" "$DESTDIR$PAMDIR/glide-lock"
install -Dm755 "$FROM/libpam_glide_keychain.so" "$DESTDIR$PAM_MODULEDIR/$PAM_MODULE"
if [ "$INSTALL_PAM" -eq 1 ]; then
    pam_lines add
fi

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
TOOLS="xwayland-satellite wpctl pactl brightnessctl nmcli bluetoothctl udisksctl"
if [ "$INSTALL_DM" -eq 1 ]; then
    TOOLS="$TOOLS greetd"
fi
for tool in $TOOLS; do
    command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
[ -x /usr/lib/xdg-desktop-portal ] || [ -x /usr/libexec/xdg-desktop-portal ] || missing="$missing xdg-desktop-portal"
[ -x /usr/lib/rtkit-daemon ] || [ -x /usr/libexec/rtkit-daemon ] || [ -x /usr/lib/rtkit/rtkit-daemon ] ||
    missing="$missing rtkit"

qt_plugin() {
    for root in /usr/lib/qt6/plugins /usr/lib64/qt6/plugins /usr/lib/x86_64-linux-gnu/qt6/plugins \
        /usr/lib/aarch64-linux-gnu/qt6/plugins /usr/lib/qt/plugins /usr/lib64/qt5/plugins /usr/lib/qt5/plugins \
        /usr/lib/x86_64-linux-gnu/qt5/plugins /usr/lib/aarch64-linux-gnu/qt5/plugins; do
        [ -e "$root/$1" ] && return 0
    done
    return 1
}
themes=""
[ -d /usr/share/themes/adw-gtk3/gtk-3.0 ] || [ -d /usr/local/share/themes/adw-gtk3/gtk-3.0 ] || themes="$themes adw-gtk3"
qt_plugin platformthemes/libqt6ct.so || themes="$themes qt6ct"
qt_plugin platformthemes/libqt5ct.so || themes="$themes qt5ct"
qt_plugin styles/libkvantum.so || themes="$themes kvantum"
[ -f /usr/share/icons/Colloid/index.theme ] || [ -f /usr/local/share/icons/Colloid/index.theme ] || themes="$themes colloid"

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
    echo "  wpctl (wireplumber), pactl (libpulse), brightnessctl   volume, sound devices, brightness"
    echo "  rtkit   realtime priority for PipeWire; without it sound crackles and drops under load"
    echo "  nmcli (NetworkManager), bluetoothctl (bluez)   Wi-Fi and Bluetooth in quick settings"
    echo "  udisksctl (udisks2)   removable drives in Glide Files"
fi
if [ -n "$themes" ]; then
    echo
    echo "Theme parts not found:$themes"
    echo "  adw-gtk3 (adw-gtk-theme)   GTK 3, Chrome and Electron apps in Glide colours"
    echo "  qt6ct, qt5ct               Qt 6 / Qt 5 apps in Glide colours"
    echo "  kvantum                    Qt controls in the Glide shapes"
    echo "  colloid (colloid-icon-theme-git, or ./install.sh -t all from vinceliuice/Colloid-icon-theme)"
    echo "                             the default icons, in the accent's colour; Papirus is used without them"
    echo "  Settings > Appearance > Other apps shows what is in use."
fi
if [ "$INSTALL_PAM" -eq 0 ] && ! grep -qs "$PAM_MODULE" "$DESTDIR$PAMDIR/greetd"; then
    echo
    echo "The login password does not open the app keys yet (Chrome and other Secret Service"
    echo "programs then keep their key in a private file). Run again with --pam to add"
    echo "$PAM_MODULE to $PAMDIR/greetd and $PAMDIR/passwd."
fi
echo
echo "Glide has its own notification centre; do not autostart mako, dunst or swaync in this session."
