#!/bin/bash
# Runs inside archlinux:base-devel as root. /pkgs holds the built packages,
# /mono a read-only copy of the monorepo (its scripts/install.sh and data),
# /migrate the migration script. $1 = upgrade | fresh.
set -uo pipefail
MODE="$1"
fail=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; fail=1; }

pacman -Sy --noconfirm --needed archlinux-keyring >/dev/null 2>&1
pacman -Su --noconfirm >/dev/null 2>&1

PKGS=$(ls /pkgs/*.pkg.tar.zst | grep -v -- '-debug-')

if [ "$MODE" = upgrade ]; then
    echo "== simulate the /usr/local install"
    pacman -S --noconfirm --needed greetd polkit sudo pam >/dev/null 2>&1 || bad "base packages"
    useradd -m tester 2>/dev/null
    # glide-secret-service, as on the real machine
    install -d /tmp/gss && cp /mono/data/arch/glide-secret-service/PKGBUILD /tmp/gss/ && chown -R tester /tmp/gss
    su tester -c 'cd /tmp/gss && makepkg -f --noconfirm >/dev/null 2>&1' && pacman -U --noconfirm /tmp/gss/*.pkg.tar.zst >/dev/null 2>&1 \
        && ok "glide-secret-service installed" || bad "glide-secret-service"
    install -d /tmp/stubs
    for b in glide glidectl glide-configd glide-settings glide-files glide-terminal glide-text glide-passwords \
        glide-photos glide-folio glide-clips glide-catalog glide-portal glide-keychain glide-phone glide-welcome \
        glide-xembed facekey libpam_glide_keychain.so; do
        printf '#!/bin/sh\necho old\n' > "/tmp/stubs/$b"; chmod 755 "/tmp/stubs/$b"
    done
    cp -r /mono /tmp/mono
    sh /tmp/mono/scripts/install.sh --prefix /usr/local --from /tmp/stubs --dm --pam >/tmp/old-install.log 2>&1 \
        && ok "old install.sh ran" || { bad "old install.sh"; tail -20 /tmp/old-install.log; }
    for pair in sudo:sudo polkit:polkit-1; do
        f="/etc/pam.d/${pair#*:}"
        [ -f "$f" ] || printf '#%%PAM-1.0\nauth include system-auth\n' > "$f"
        sed -i "1a # glide-facekey\nauth       sufficient   pam_exec.so quiet /usr/local/bin/facekey -verify -source ${pair%%:*}" "$f"
    done
    grep -q '/usr/local/bin/glide --greeter' /etc/greetd/config.toml && ok "greetd points at /usr/local" || bad "greetd setup"
    systemctl is-enabled greetd.service >/dev/null 2>&1 && ok "greetd enabled" || bad "greetd not enabled before"
    ls /usr/share/glide/face/*.onnx >/dev/null 2>&1 && ok "models copied by the old installer" || echo "  note no models in the copy"

    echo "== migrate"
    # shellcheck disable=SC2086
    sh /migrate/migrate-from-local.sh --yes $PKGS >/tmp/migrate.log 2>&1
    rc=$?
    sed 's/^/    /' /tmp/migrate.log | tail -40
    [ $rc -eq 0 ] && ok "migration exited 0" || bad "migration exit $rc"

    echo "== second run"
    # shellcheck disable=SC2086
    sh /migrate/migrate-from-local.sh --yes $PKGS >/tmp/migrate2.log 2>&1 && ok "second run exits 0 ($(tail -1 /tmp/migrate2.log))" \
        || { bad "second run"; tail -10 /tmp/migrate2.log; }
else
    echo "== fresh install from the official repositories"
    # shellcheck disable=SC2086
    pacman -U --noconfirm $PKGS >/tmp/fresh.log 2>&1 && ok "pacman -U resolved every dependency" \
        || { bad "pacman -U"; tail -20 /tmp/fresh.log; }
fi

echo "== checks"
for p in glide glide-shell glide-configd xdg-desktop-portal-glide glide-keychain glide-facekey glide-facekey-models \
    glide-settings glide-files glide-terminal glide-text glide-passwords glide-photos glide-folio glide-clips glide-phone; do
    pacman -Qq "$p" >/dev/null 2>&1 || bad "$p not installed"
done
pacman -Qq glide-secret-service >/dev/null 2>&1 && bad "glide-secret-service still installed"
pacman -T org.freedesktop.secrets >/dev/null && ok "org.freedesktop.secrets provided" || bad "org.freedesktop.secrets"
out=$(pacman -Qkk $(pacman -Qqs '^(glide|xdg-desktop-portal-glide)') 2>&1 | grep -v ' 0 altered files' | grep -v 'backup file' )
[ -z "$out" ] && ok "pacman -Qkk clean" || { bad "pacman -Qkk"; echo "$out" | head; }
missing=$(for f in $(pacman -Qql $(pacman -Qqs '^(glide|xdg-desktop-portal-glide)') | grep -E '/usr/(bin|lib)/.+[^/]$'); do
    [ -f "$f" ] && file -b "$f" | grep -q ELF && ldd "$f" 2>/dev/null | grep 'not found' | sed "s|^|$f: |"; done)
[ -z "$missing" ] && ok "every ELF finds its libraries" || { bad "missing libraries"; echo "$missing" | sort -u | head -20; }
for f in /usr/share/dbus-1/services/org.glide.*.service /usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.glide.service; do
    e=$(sed -n 's/^Exec=\([^ ]*\).*/\1/p' "$f"); [ -x "$e" ] && ok "$(basename "$f") -> $e" || bad "$(basename "$f") -> $e"
done
e=$(sed -n 's/^ExecStart=\([^ ]*\).*/\1/p' /usr/lib/systemd/user/glide.service); [ -x "$e" ] && ok "glide.service -> $e" || bad "glide.service -> $e"
e=$(sed -n 's/^Exec=\([^ ]*\).*/\1/p' /usr/share/wayland-sessions/glide.desktop); [ -x "$e" ] && ok "session -> $e" || bad "session -> $e"
grep -q '/usr/bin/facekey' /usr/share/polkit-1/actions/org.glide.facekey.policy && ok "polkit policy names /usr/bin/facekey" || bad "polkit policy path"
ls /usr/share/glide/defaults/ | tr '\n' ' '; echo
for n in shell passwords permissions pins terminal text folio; do
    [ -f "/usr/share/glide/defaults/$n.json" ] || bad "defaults $n.json missing"
done
if [ "$MODE" = upgrade ]; then
    cmd=$(sed -n 's/^command *= *"\([^ "]*\).*/\1/p' /etc/greetd/config.toml)
    [ "$cmd" = /usr/bin/glide ] && ok "greetd command $cmd" || bad "greetd command $cmd"
    systemctl is-enabled greetd.service >/dev/null 2>&1 && ok "greetd still enabled" || bad "greetd disabled"
    for f in /etc/pam.d/sudo /etc/pam.d/polkit-1; do
        grep -q 'pam_exec.so quiet /usr/bin/facekey' "$f" && ! grep -q /usr/local "$f" && ok "$f uses /usr/bin/facekey" || bad "$f: $(grep facekey "$f")"
    done
    grep -q '^-auth optional pam_glide_keychain.so' /etc/pam.d/greetd && ok "keychain PAM lines kept" || bad "keychain PAM lines"
    left=$(find /usr/local -name 'glide*' -o -name facekey | grep -v cpu-cap)
    [ -z "$left" ] && ok "nothing left under /usr/local" || { bad "left under /usr/local"; echo "$left"; }
    [ -f /usr/share/icons/hicolor/scalable/apps/glide-welcome.svg ] && bad "welcome icon left" || ok "welcome icon removed"
    ls /var/backups/glide-migrate-*.tar.gz >/dev/null 2>&1 && ok "backup $(ls /var/backups/glide-migrate-*.tar.gz | head -1)" || bad "no backup"
fi
/usr/bin/glide --version && ok "glide --version runs" || bad "glide --version"
/usr/bin/glidectl --help >/dev/null 2>&1; [ $? -le 2 ] && ok "glidectl runs" || bad "glidectl"
echo
[ $fail -eq 0 ] && echo "RESULT: PASS ($MODE)" || echo "RESULT: FAIL ($MODE)"
exit $fail
