#!/bin/sh
# Download the packages of the latest release of every repository in
# components.list into ./out. A repository without a release stops the
# publish, so the database never names a dependency that is not there.
set -eu
cd "$(dirname "$0")/.."
mkdir -p out
for repo in $(grep -v '^#' components.list); do
    tag="$(gh release view --repo "Glide-Desktop/$repo" --json tagName --jq .tagName)"
    echo "$repo $tag"
    gh release download "$tag" --repo "Glide-Desktop/$repo" --pattern '*.pkg.tar.zst' --dir out --clobber
done
ls -l out
