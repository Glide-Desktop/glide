#!/bin/sh
# Build Glide from source: checks the toolchain and system libraries first,
# then compiles every binary with the versions pinned in Cargo.lock.
# The build runs through scripts/ai-cargo.sh (one job, 35 % CPU, boost off)
# so the machine stays cool; --fast uses plain cargo and every core.
#
# Usage: scripts/build.sh [--debug] [--skip-checks] [--fast]
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE=release
CHECKS=1
CARGO="$REPO/scripts/ai-cargo.sh"
MIN_RUST=1.96

for arg in "$@"; do
    case "$arg" in
        --debug) PROFILE=dev ;;
        --skip-checks) CHECKS=0 ;;
        --fast) CARGO=cargo ;;
        -h|--help)
            sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

if [ "$(id -u)" -eq 0 ]; then
    echo "error: do not build as root; run scripts/build.sh as your user, then sudo scripts/install.sh" >&2
    exit 1
fi

distro_id() {
    [ -r /etc/os-release ] || { echo unknown; return; }
    # shellcheck disable=SC1091
    . /etc/os-release
    for id in ${ID:-} ${ID_LIKE:-}; do
        case "$id" in
            arch|debian|ubuntu|fedora|rhel|opensuse*|suse|void|alpine|gentoo) echo "$id"; return ;;
        esac
    done
    echo "${ID:-unknown}"
}

package_hint() {
    case "$(distro_id)" in
        arch)
            echo "sudo pacman -S --needed base-devel git pkgconf seatd systemd-libs libinput mesa libxkbcommon libdisplay-info" ;;
        debian|ubuntu)
            echo "sudo apt install build-essential git pkg-config libseat-dev libudev-dev libinput-dev libgbm-dev libxkbcommon-dev libdisplay-info-dev" ;;
        fedora|rhel)
            echo "sudo dnf install gcc git pkgconf-pkg-config libseat-devel systemd-devel libinput-devel mesa-libgbm-devel libxkbcommon-devel libdisplay-info-devel" ;;
        opensuse*|suse)
            echo "sudo zypper install gcc git pkgconf libseat-devel systemd-devel libinput-devel libgbm-devel libxkbcommon-devel libdisplay-info-devel" ;;
        void)
            echo "sudo xbps-install base-devel git pkg-config libseat-devel eudev-libudev-devel libinput-devel libgbm-devel libxkbcommon-devel libdisplay-info-devel" ;;
        alpine)
            echo "sudo apk add build-base git pkgconf seatd-dev eudev-dev libinput-dev mesa-dev libxkbcommon-dev libdisplay-info-dev" ;;
        gentoo)
            echo "sudo emerge --noreplace sys-auth/seatd virtual/libudev dev-libs/libinput media-libs/mesa x11-libs/libxkbcommon media-libs/libdisplay-info" ;;
        *)
            echo "install the development packages for: libseat libudev libinput gbm xkbcommon libdisplay-info" ;;
    esac
}

version_ge() {
    [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]
}

if [ "$CHECKS" -eq 1 ]; then
    missing=""

    if ! command -v cargo >/dev/null 2>&1; then
        echo "error: cargo not found. Install Rust with rustup: https://rustup.rs" >&2
        exit 1
    fi
    rust_version="$(cd "$REPO" && rustc --version | awk '{print $2}')"
    if ! version_ge "$rust_version" "$MIN_RUST"; then
        echo "error: rustc $rust_version is older than the required $MIN_RUST." >&2
        echo "       Distribution Rust packages are often too old; use rustup: https://rustup.rs" >&2
        exit 1
    fi

    for tool in cc pkg-config git; do
        command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
    done

    if command -v pkg-config >/dev/null 2>&1; then
        for module in libseat libudev "libinput >= 1.19" gbm xkbcommon "libdisplay-info >= 0.1" "libdisplay-info < 0.4"; do
            pkg-config --exists "$module" || missing="$missing [$module]"
        done
    fi

    if [ -n "$missing" ]; then
        echo "error: missing build dependencies:$missing" >&2
        echo "       $(package_hint)" >&2
        exit 1
    fi
    echo "toolchain: rustc $rust_version, system libraries found"
fi

cd "$REPO"
"$CARGO" build --locked --profile "$PROFILE" --workspace --bins
"$CARGO" build --locked --profile "$PROFILE" -p glide-pam

OUT="$REPO/target/release"
[ "$PROFILE" = dev ] && OUT="$REPO/target/debug"

echo
echo "built:"
for bin in glide glidectl glide-settings glide-files glide-terminal glide-text glide-passwords glide-catalog glide-portal glide-keychain glide-phone glide-welcome; do
    echo "  $OUT/$bin"
done
echo "  $OUT/libpam_glide_keychain.so"
echo
echo "next: sudo scripts/install.sh"
