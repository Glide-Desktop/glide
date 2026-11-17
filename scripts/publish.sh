#!/bin/sh
# Sign the packages in ./out, add them to the repository db and upload both.
# Needs: gpg with the signing key imported (GLIDE_SIGNING_FPR), repo-add,
# rsync, an ssh key that may write to GLIDE_REPO_RSYNC.
set -eu
REPO_URL="${GLIDE_REPO_URL:-https://pkg.mindenit.org/glide/x86_64}"
: "${GLIDE_REPO_RSYNC:?e.g. deploy@pkg.mindenit.org:/srv/pkg/glide/x86_64/}"
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
for name in glide.db glide.files; do
    rm -f "$name" "$name.sig"
    cp -f "$name.tar.zst" "$name"
    cp -f "$name.tar.zst.sig" "$name.sig"
done

# Packages first, the db last, so a client never sees a db naming a file
# that is not there yet. Old package files stay on the server.
rsync -av --delay-updates ./*.pkg.tar.zst ./*.pkg.tar.zst.sig "$GLIDE_REPO_RSYNC"
rsync -av --delay-updates glide.db glide.db.sig glide.files glide.files.sig \
    glide.db.tar.zst glide.db.tar.zst.sig glide.files.tar.zst glide.files.tar.zst.sig "$GLIDE_REPO_RSYNC"
