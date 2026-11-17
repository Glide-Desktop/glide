#!/bin/sh
# Check a set of built packages in throwaway Arch containers (needs docker):
#   upgrade  reproduce a /usr/local install made by the monorepo's install.sh
#            (greetd, the keychain and FaceKey PAM lines, glide-secret-service),
#            run glide-shell's migrate-from-local.sh and check the result
#   fresh    install the packages on a clean system from the official repos
#
# Usage: tests/run.sh PACKAGE_DIR [upgrade|fresh ...]
set -eu
PKGS="$(cd "$1" && pwd)"
shift
[ $# -gt 0 ] || set -- upgrade fresh
HERE="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/mono" "$WORK/migrate"
git -C "$HERE" archive monorepo-final scripts data crates/glide-face | tar -x -C "$WORK/mono"
for f in models lib; do
    [ -d "$HERE/crates/glide-face/$f" ] && cp -r "$HERE/crates/glide-face/$f" "$WORK/mono/crates/glide-face/"
done
LOCAL_MIGRATE="$HERE/../glide-shell/scripts/migrate-from-local.sh"
if [ -f "$LOCAL_MIGRATE" ]; then
    echo "migration script: $LOCAL_MIGRATE"
    cp "$LOCAL_MIGRATE" "$WORK/migrate/migrate-from-local.sh"
else
    echo "migration script: glide-shell main on GitHub"
    curl -fsSL https://raw.githubusercontent.com/Glide-Desktop/glide-shell/main/scripts/migrate-from-local.sh \
        -o "$WORK/migrate/migrate-from-local.sh"
fi
cp "$HERE/tests/container.sh" "$WORK/migrate/"

status=0
for mode in "$@"; do
    docker run --rm --privileged -v "$PKGS:/pkgs:ro" -v "$WORK/mono:/mono:ro" -v "$WORK/migrate:/migrate:ro" \
        archlinux:base-devel bash /migrate/container.sh "$mode" > "$WORK/$mode.log" 2>&1 || status=1
    tail -1 "$WORK/$mode.log"
    if grep -q FAIL "$WORK/$mode.log"; then
        sed 's/^/  | /' "$WORK/$mode.log"
    fi
done
exit $status
