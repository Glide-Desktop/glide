#!/bin/sh
# Sign the packages in ./out, make the repository db hold exactly them, upload
# both, and keep the newest KEEP builds of every package on the server.
# Needs: gpg with the signing key imported (GLIDE_SIGNING_FPR), repo-add,
# vercmp, rsync, an ssh key that may write to GLIDE_REPO_RSYNC.
set -eu
REPO_URL="${GLIDE_REPO_URL:-https://pkg.mindenit.org/glide/x86_64}"
KEEP="${GLIDE_KEEP:-3}"
: "${GLIDE_REPO_RSYNC:?e.g. deploy@pkg.mindenit.org:x86_64/}"
: "${GLIDE_SIGNING_FPR:?fingerprint of the signing key}"
cd "$(dirname "$0")/../out"

ls ./*.pkg.tar.zst >/dev/null
for pkg in ./*.pkg.tar.zst; do
    gpg --batch --yes --detach-sign -u "$GLIDE_SIGNING_FPR" --no-armor "$pkg"
done

for f in glide.db.tar.zst glide.db.tar.zst.sig glide.files.tar.zst glide.files.tar.zst.sig; do
    curl -fsSL "$REPO_URL/$f" -o "$f" || rm -f "$f"
done
if [ -f glide.db.tar.zst ] && [ -f glide.db.tar.zst.sig ]; then
    gpg --batch --verify glide.db.tar.zst.sig glide.db.tar.zst
fi
repo-add --sign --key "$GLIDE_SIGNING_FPR" --verify glide.db.tar.zst ./*.pkg.tar.zst

names() {
    tar -xOf glide.db.tar.zst --wildcards '*/desc' | awk '/^%NAME%$/ {getline n; print n}' | sort -u
}
for pkg in ./*.pkg.tar.zst; do bsdtar -xOf "$pkg" .PKGINFO | sed -n 's/^pkgname = //p'; done | sort -u > built.txt
stale="$(names | comm -23 - built.txt)"
if [ -n "$stale" ]; then
    echo "removing from the db, no longer built: $stale"
    # shellcheck disable=SC2086
    repo-remove --sign --key "$GLIDE_SIGNING_FPR" glide.db.tar.zst $stale
fi

for name in glide.db glide.files; do
    rm -f "$name" "$name.sig"
    cp -f "$name.tar.zst" "$name"
    cp -f "$name.tar.zst.sig" "$name.sig"
done

# Packages first, the db last, so a client never sees a db naming a file
# that is not there yet.
rsync -av --delay-updates ./*.pkg.tar.zst ./*.pkg.tar.zst.sig "$GLIDE_REPO_RSYNC"
rsync -av --delay-updates glide.db glide.db.sig glide.files glide.files.sig \
    glide.db.tar.zst glide.db.tar.zst.sig glide.files.tar.zst glide.files.tar.zst.sig "$GLIDE_REPO_RSYNC"

# Keep the newest KEEP builds of each package (for pacman -U rollbacks) and
# delete the rest, through an include list over an empty source with --delete.
rsync --list-only "$GLIDE_REPO_RSYNC" | awk '{print $NF}' | grep '\.pkg\.tar\.zst$' > remote.txt || true
old="$(sed 's/-[^-]*-[^-]*-[^-]*\.pkg\.tar\.zst$//' remote.txt | sort -u | while read -r pkgname; do
    grep -E "^$(printf '%s' "$pkgname" | sed 's/[.+]/\\&/g')-[^-]+-[^-]+-[^-]+\.pkg\.tar\.zst$" remote.txt |
        while read -r file; do
            v="${file#"$pkgname"-}"; v="${v%-*.pkg.tar.zst}"
            printf '%s %s\n' "$v" "$file"
        done | sort -t' ' -k1,1 -V -r | awk -v keep="$KEEP" 'NR > keep {print $2}'
done | while read -r file; do [ -e "./$file" ] || echo "$file"; done)"
if [ -n "$old" ]; then
    echo "pruning old builds:"; echo "$old" | sed 's/^/  /'
    mkdir -p empty
    printf '%s\n' "$old" | while read -r f; do printf '+ %s\n+ %s.sig\n' "$f" "$f"; done > prune.rules
    echo '- *' >> prune.rules
    rsync -r --delete --filter="merge prune.rules" empty/ "$GLIDE_REPO_RSYNC"
fi
