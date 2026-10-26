#!/bin/sh
# Remove build artifacts that no build has read for a while. Cargo keeps
# every variant it ever produced (each change of a dependency, a feature set
# or a profile gets a new hash) and never deletes the old ones, so target/
# grows without bound. Artifacts in use are read on every build and keep a
# fresh access time (relatime refreshes it at least daily); the rest go.
#
# Usage: scripts/prune-target.sh [--days N] [--dry-run]
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$REPO/target}"
DAYS=2
DRY=0

while [ $# -gt 0 ]; do
    case "$1" in
        --days) DAYS="${2:?--days needs a number}"; shift ;;
        --dry-run) DRY=1 ;;
        -h|--help)
            sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

case "$DAYS" in
    ''|*[!0-9]*) echo "--days takes a whole number" >&2; exit 2 ;;
esac

[ -d "$TARGET" ] || { echo "nothing to prune: $TARGET does not exist"; exit 0; }

size() {
    du -sh "$1" 2>/dev/null | cut -f1
}

remove() {
    if [ "$DRY" -eq 1 ]; then
        echo "would remove $1"
    else
        rm -rf -- "$1"
    fi
}

stale_dir() {
    [ -z "$(find "$1" -atime -"$((DAYS + 1))" -print -quit 2>/dev/null)" ]
}

echo "before: $(size "$TARGET")"
for profile in "$TARGET"/*/; do
    [ -d "$profile/deps" ] || continue
    for files in deps examples; do
        [ -d "$profile/$files" ] || continue
        find "$profile/$files" -mindepth 1 -maxdepth 1 -type f -atime +"$DAYS" | while read -r file; do
            remove "$file"
        done
    done
    for dirs in incremental build .fingerprint; do
        [ -d "$profile/$dirs" ] || continue
        find "$profile/$dirs" -mindepth 1 -maxdepth 1 -type d | while read -r dir; do
            if stale_dir "$dir"; then
                remove "$dir"
            fi
        done
    done
done
echo "after: $(size "$TARGET")"
