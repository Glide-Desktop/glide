#!/bin/sh
set -eu

icons=${XDG_DATA_HOME:-"$HOME/.local/share"}/icons
for base in "$icons" /usr/local/share/icons /usr/share/icons; do
    for theme in "$base"/Colloid*; do
        if [ -e "$theme" ] || [ -L "$theme" ]; then
            echo "Colloid icons already exist at $theme; leaving them unchanged."
            exit 0
        fi
    done
done

tmp=$(mktemp -d "${TMPDIR:-/tmp}/glide-colloid.XXXXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
git clone --depth 1 https://github.com/vinceliuice/Colloid-icon-theme.git "$tmp/colloid"
mkdir -p "$icons"
bash "$tmp/colloid/install.sh" -d "$icons" -t all
