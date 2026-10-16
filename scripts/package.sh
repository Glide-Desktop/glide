#!/bin/sh
# Build a release tarball others can install without a Rust toolchain:
#   dist/glide-<version>-<arch>.tar.gz  (bin/, data/, scripts/install.sh, README.md)
#
# The binaries link against the system glibc, libinput, libseat, libgbm,
# libxkbcommon and libdisplay-info of the build machine, so a tarball runs
# only on distributions at least as new as the one it was built on. For
# anything else, build from source with scripts/build.sh.
#
# Usage: scripts/package.sh [--skip-build]
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BUILD=1
for arg in "$@"; do
    case "$arg" in
        --skip-build) BUILD=0 ;;
        -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

[ "$BUILD" -eq 1 ] && "$REPO/scripts/build.sh"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/crates/glide-compositor/Cargo.toml" | head -n1)"
REV="$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || true)"
[ -n "$REV" ] && VERSION="$VERSION+$REV"
NAME="glide-$VERSION-$(uname -m)"
STAGE="$REPO/dist/$NAME"

rm -rf "$STAGE"
install -d "$STAGE/bin" "$STAGE/data" "$STAGE/scripts"
for bin in glide glidectl glide-settings glide-files glide-terminal; do
    install -m755 "$REPO/target/release/$bin" "$STAGE/bin/$bin"
done
for f in glide-session glide-lock glide.desktop glide.service glide-shutdown.target glide-portals.conf glide-settings.desktop glide-files.desktop glide-terminal.desktop greetd-glide.toml; do
    cp -p "$REPO/data/$f" "$STAGE/data/$f"
done
install -Dm644 "$REPO/data/pam.d/glide-lock" "$STAGE/data/pam.d/glide-lock"
install -Dm644 "$REPO/data/sounds/terminal-bell.mp3" "$STAGE/data/sounds/terminal-bell.mp3"
install -m755 "$REPO/scripts/install.sh" "$STAGE/scripts/install.sh"
cp "$REPO/README.md" "$STAGE/README.md"
[ -f "$REPO/LICENSE" ] && cp "$REPO/LICENSE" "$STAGE/LICENSE"

GLIBC="$(objdump -T "$STAGE/bin/glide" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -V | tail -n1 || true)"
{
    echo "glide $VERSION ($(uname -m))"
    echo "built on: $(. /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-unknown}")"
    [ -n "$GLIBC" ] && echo "requires: ${GLIBC#GLIBC_} or newer glibc"
    echo "links:"
    ldd "$STAGE/bin/glide" | awk '/=>/ {print "  " $1}' | grep -Ev 'libc\.so|libm\.so|libgcc_s|ld-linux|vdso'
} > "$STAGE/BUILD-INFO"

tar -C "$REPO/dist" -czf "$REPO/dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"

echo
echo "dist/$NAME.tar.gz"
echo "recipients: tar xf $NAME.tar.gz && cd $NAME && sudo scripts/install.sh"
