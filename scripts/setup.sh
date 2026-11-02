#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [ "$(id -u)" -eq 0 ]; then
    echo "Run as your normal user, not root; sudo will be requested for installation." >&2
    exit 1
fi

if ! command -v pacman >/dev/null 2>&1; then
    echo "Automatic dependency installation currently supports Arch Linux and its derivatives only." >&2
    echo "See README.md for other distributions." >&2
    exit 1
fi

packages='base-devel git pkgconf seatd systemd-libs libinput mesa libxkbcommon libdisplay-info gstreamer gst-plugins-base-libs pam pipewire pipewire-audio pipewire-alsa pipewire-pulse wireplumber rtkit upower libpulse libcanberra sound-theme-freedesktop networkmanager bluez bluez-utils brightnessctl udisks2 ntfs-3g ntfsprogs gvfs-mtp gvfs-gphoto2 ffmpegthumbnailer polkit power-profiles-daemon python-gobject xdg-user-dirs xdg-desktop-portal xdg-desktop-portal-gtk xdg-desktop-portal-wlr gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav gst-plugin-pipewire gst-plugin-va xorg-xwayland xwayland-satellite libayatana-appindicator papirus-icon-theme qt5ct qt6ct kvantum kvantum-qt5 ttf-jetbrains-mono noto-fonts noto-fonts-cjk noto-fonts-emoji'
for arg in "$@"; do
    case "$arg" in --dm|--with-dm) packages="$packages greetd" ;; esac
done
if pacman -Q pulseaudio >/dev/null 2>&1; then
    echo "PulseAudio is installed. Glide expects PipeWire with pipewire-pulse in its place;"
    echo "pacman will ask to replace pulseaudio when pipewire-pulse is installed."
fi
missing=''
for package in $packages; do
    if ! pacman -Q "$package" >/dev/null 2>&1; then
        missing="$missing $package"
    fi
done

if [ -n "$missing" ]; then
    echo "Missing recommended build and runtime packages:$missing"
    printf 'Upgrade the system and install missing packages with pacman? [y/N] '
    read -r answer
    case "$answer" in
        y|Y|yes|YES) sudo pacman -Syu --needed $missing ;;
        *) echo "Installation cancelled."; exit 1 ;;
    esac
fi

if ! command -v cargo >/dev/null 2>&1 || ! rustc --version >/dev/null 2>&1; then
    if ! command -v rustup >/dev/null 2>&1; then
        printf 'Rust is missing. Install rustup using pacman? [y/N] '
        read -r answer
        case "$answer" in
            y|Y|yes|YES) sudo pacman -Syu --needed rustup ;;
            *) echo "Installation cancelled."; exit 1 ;;
        esac
    fi
    printf 'Configure the stable Rust toolchain with rustup? [y/N] '
    read -r answer
    case "$answer" in
        y|Y|yes|YES) rustup default stable ;;
        *) echo "Installation cancelled."; exit 1 ;;
    esac
fi

printf 'Install Colloid icons from the official repository for this user? [y/N] '
read -r answer
case "$answer" in y|Y|yes|YES) sh "$ROOT/scripts/colloid.sh" ;; esac

echo "Glide Phone requires UDP 1716 and TCP 1716–1764 on your local network."
echo "Check firewall rules and ensure KDE Connect is not already using UDP 1716."
echo "Audio requires a running PipeWire and WirePlumber session; package installation alone does not start them."

"$ROOT/scripts/build.sh"
sudo "$ROOT/scripts/install.sh" "$@"

printf 'Enable PipeWire audio and WirePlumber for your user session? [y/N] '
read -r answer
case "$answer" in
    y|Y|yes|YES) systemctl --user enable --now pipewire.socket pipewire-pulse.socket wireplumber.service ;;
esac
printf 'Enable NetworkManager and Bluetooth system services? [y/N] '
read -r answer
case "$answer" in
    y|Y|yes|YES) sudo systemctl enable --now NetworkManager.service bluetooth.service ;;
esac
for arg in "$@"; do
    case "$arg" in
        --dm|--with-dm)
            echo 'greetd configuration was installed. Enabling it may replace your current display manager.'
            printf 'Enable greetd for the next boot? [y/N] '
            read -r answer
            case "$answer" in y|Y|yes|YES) sudo systemctl enable greetd.service ;; esac
            break ;;
    esac
done
systemctl --user daemon-reload || echo 'Could not reload user services; they will be picked up at next login.'
