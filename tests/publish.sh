#!/bin/sh
# Check scripts/publish.sh in a throwaway Arch container (needs docker): four
# releases of a dummy package `a`, a package `b` that is dropped after the
# first, published to a local directory with a throwaway key.
# Expect: the db holds only the newest `a`, `b` leaves the db, the newest three
# builds of `a` stay on the "server" and the oldest one is pruned.
set -eu
HERE="$(cd "$(dirname "$0")/.." && pwd)"
docker run --rm -v "$HERE:/glide:ro" archlinux:base-devel bash -c '
set -eu
pacman -Syu --noconfirm --needed rsync gnupg >/dev/null 2>&1
useradd -m builder
gpg --batch --passphrase "" --quick-gen-key "test" ed25519 sign never 2>/dev/null
export GLIDE_SIGNING_FPR="$(gpg --list-secret-keys --with-colons | awk -F: "/^fpr/ {print \$10; exit}")"
export GLIDE_REPO_RSYNC=/srv/repo/ GLIDE_REPO_URL=file:///srv/repo
mkdir -p /srv/repo
build() {
    d=/tmp/build-$1-$2; mkdir -p $d
    printf "pkgname=%s\npkgver=%s\npkgrel=1\narch=(any)\npackage() { :; }\n" "$1" "$2" > $d/PKGBUILD
    chown -R builder $d; (cd $d && sudo -u builder makepkg -f >/dev/null 2>&1); cp $d/*.pkg.tar.zst /work/out/
}
release() {
    rm -rf /work; mkdir -p /work/out; cp -r /glide/scripts /work/
    for spec in "$@"; do build ${spec%%=*} ${spec#*=}; done
    /work/scripts/publish.sh >/tmp/publish.log 2>&1 || { tail -20 /tmp/publish.log; exit 1; }
}
release a=1.0 b=1.0
release a=2.0
release a=3.0
release a=10.0
db="$(tar -xOf /srv/repo/glide.db.tar.zst --wildcards "*/desc" | awk "/^%NAME%\$/ {getline n} /^%VERSION%\$/ {getline v; print n\"=\"v}" | tr "\n" " ")"
files="$(ls /srv/repo | grep "pkg.tar.zst$" | tr "\n" " ")"
echo "db: $db"; echo "server: $files"
[ "$db" = "a=10.0-1 " ] || { echo "FAIL db"; exit 1; }
for v in 2.0 3.0 10.0; do [ -f /srv/repo/a-$v-1-any.pkg.tar.zst ] && [ -f /srv/repo/a-$v-1-any.pkg.tar.zst.sig ] || { echo "FAIL a-$v kept"; exit 1; }; done
[ ! -e /srv/repo/a-1.0-1-any.pkg.tar.zst ] && [ ! -e /srv/repo/a-1.0-1-any.pkg.tar.zst.sig ] || { echo "FAIL a-1.0 pruned"; exit 1; }
gpg --batch --verify /srv/repo/glide.db.sig /srv/repo/glide.db 2>/dev/null || { echo "FAIL db signature"; exit 1; }
echo "RESULT: PASS (publish)"
'
