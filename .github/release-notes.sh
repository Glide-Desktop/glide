#!/bin/sh
# Print the release notes for a tag as Markdown: the tag's own message (for an
# annotated tag), the commits since the previous v* tag, and how to install.
#
# Usage: .github/release-notes.sh TAG [PACKAGE_FILE]
set -eu

TAG="$1"
PACKAGE="${2:-glide-<version>-1-x86_64.pkg.tar.zst}"
REPO_URL="https://github.com/${GITHUB_REPOSITORY:-Glide-Desktop/glide}"

if [ "$(git cat-file -t "$TAG")" = tag ]; then
    subject="$(git tag -l --format='%(contents:subject)' "$TAG")"
    body="$(git tag -l --format='%(contents:body)' "$TAG")"
    if [ -n "$subject" ]; then
        printf '%s\n\n' "$subject"
        [ -n "$body" ] && printf '%s\n\n' "$body"
    fi
fi

prev="$(git describe --tags --abbrev=0 --match 'v*' "$TAG^" 2>/dev/null || true)"

if [ -n "$prev" ]; then
    echo "## Changes since $prev"
    echo
    git log --no-merges --reverse --format='- %s (%h)' "$prev..$TAG"
    echo
    echo "**Full changelog:** $REPO_URL/compare/$prev...$TAG"
else
    count="$(git rev-list --count --no-merges "$TAG")"
    echo "## First release"
    echo
    echo "The first tagged version of Glide, built from $count commits."
    echo "History: $REPO_URL/commits/$TAG"
fi

cat <<EOF

## Install on Arch Linux

\`\`\`sh
sudo pacman -U $PACKAGE
\`\`\`

Then log out and choose **Glide** in your display manager. Optional runtime
packages (X11 apps, portals, PipeWire, NetworkManager, themes) are listed as
optional dependencies: \`pacman -Qi glide\`.

Other distributions: build from source, see the README.
EOF
