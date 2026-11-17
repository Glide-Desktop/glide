#!/bin/sh
# Download the packages of the latest release of every repository in
# components.list into ./out.
set -eu
cd "$(dirname "$0")/.."
mkdir -p out
grep -v '^#' components.list | while read -r repo; do
    [ -n "$repo" ] || continue
    tag="$(gh release view --repo "Glide-Desktop/$repo" --json tagName --jq .tagName)"
    echo "$repo $tag"
    gh release download "$tag" --repo "Glide-Desktop/$repo" --pattern '*.pkg.tar.zst' --dir out --clobber
done
ls -l out
