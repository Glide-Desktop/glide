# Glide

A Wayland desktop built on [smithay](https://github.com/Smithay/smithay): a
compositor with its own top bar, dock, launcher, overview, notification
centre, quick settings, lock screen and greeter, and a set of apps.

This repository publishes Glide releases. The code lives in the component
repositories; until November 2026 it lived here, and that state is the tag
[`monorepo-final`](https://github.com/Glide-Desktop/glide/tree/monorepo-final).

| Repository | Packages |
|---|---|
| [glide-libs](https://github.com/Glide-Desktop/glide-libs) | shared crates (no package) |
| [glide-shell](https://github.com/Glide-Desktop/glide-shell) | `glide-shell`, `glide-configd`, `xdg-desktop-portal-glide`, `glide-desktop` (everything) |
| [glide-auth](https://github.com/Glide-Desktop/glide-auth) | `glide-keychain`, `glide-facekey` |
| [glide-facekey-models](https://github.com/Glide-Desktop/glide-facekey-models) | `glide-facekey-models` |
| [glide-files](https://github.com/Glide-Desktop/glide-files) | `glide-files` |
| [glide-terminal](https://github.com/Glide-Desktop/glide-terminal) | `glide-terminal` |
| [glide-text](https://github.com/Glide-Desktop/glide-text) | `glide-text` |
| [glide-photos](https://github.com/Glide-Desktop/glide-photos) | `glide-photos` |
| [glide-folio](https://github.com/Glide-Desktop/glide-folio) | `glide-folio` |
| [glide-clips](https://github.com/Glide-Desktop/glide-clips) | `glide-clips` |
| [glide-passwords](https://github.com/Glide-Desktop/glide-passwords) | `glide-passwords` |
| [glide-settings](https://github.com/Glide-Desktop/glide-settings) | `glide-settings` |
| [glide-phone-desktop](https://github.com/Glide-Desktop/glide-phone-desktop) | `glide-phone` |

## Install on Arch Linux

Trust the repository key, add the repository and install the
`glide-desktop` metapackage, which pulls in every Glide package and everything
they need (`glide` in Arch's extra is an unrelated video player):

```sh
curl -fsSL https://pkg.mindenit.org/glide/glide.asc | sudo pacman-key --add -
sudo pacman-key --lsign-key 051391C9EFCAD3924C0C1DBA0EA5B39B61F2E9FF
```

```ini
# /etc/pacman.conf
[glide]
SigLevel = Required
Server = https://pkg.mindenit.org/glide/$arch
```

```sh
sudo pacman -Syu glide-desktop
```

Then pick "Glide" in your display manager, or use Glide's own greeter:
copy `/usr/share/glide-shell/greetd-config.toml` to
`/etc/greetd/config.toml` and `systemctl enable greetd`. Updates come with
`pacman -Syu` like any other package.

## Moving from a source install

A Glide installed with the old `scripts/install.sh` lives under `/usr/local`,
which comes first in `PATH`, in the systemd user unit search and in
`XDG_DATA_DIRS`, so it has to go when the packages come. One script does the
whole move and refuses to remove anything until the packages are in place:

```sh
curl -fsSLO https://raw.githubusercontent.com/Glide-Desktop/glide-shell/main/scripts/migrate-from-local.sh
sudo sh migrate-from-local.sh --dry-run   # see what it will do
sudo sh migrate-from-local.sh
```

It backs up every file it touches to `/var/backups/glide-migrate-<time>.tar.gz`,
installs the packages over the files the old installer left in `/usr/share` and
`/etc`, points greetd and the FaceKey PAM lines at `/usr/bin`, removes the old
copies and checks that everything the login path names exists. Keep a root
shell on another TTY open while it runs, and log out and back in afterwards.
Per-user data (`~/.config/glide`, `~/.local/state/glide`, the keychain) is not
touched.

## Releasing

1. Tag a component repository (`v0.1.1`). Its Package workflow builds its
   packages in an Arch container and attaches them to the GitHub release.
2. Tag this repository. The Publish workflow takes the latest release of every
   repository in `components.list`, builds the AUR packages in `aur.list`,
   signs everything, adds it to the repository database and uploads it to the
   server: packages first, the database last. Earlier package files stay on
   the server, so `pacman -U` can roll back to them.

Before tagging, `tests/run.sh <directory with the .pkg.tar.zst files>` checks
them in throwaway Arch containers: a fresh install from the official
repositories (every dependency resolves, every binary finds its libraries) and
the move from a `/usr/local` install (greetd, PAM, D-Bus, systemd and the
session entry all point at files that exist afterwards).

## Server

The repository is a static directory served by Caddy at `pkg.mindenit.org`:

```
/srv/pkg/glide/glide.asc          the public signing key
/srv/pkg/glide/x86_64/            packages, signatures and the glide.db / glide.files databases
```

```caddy
pkg.mindenit.org {
    root * /srv/pkg
    file_server browse
}
```

Under Coolify the simplest form is a Docker Compose service with the domain
`pkg.mindenit.org`, one `caddy:2` container serving the volume `/srv/pkg`
(`caddy file-server --root /srv/pkg --browse --listen :80`), so Coolify's own
proxy handles TLS.

The workflow uploads with rsync over SSH as a user that may write only
`/srv/pkg/glide`. In the repository settings, environment `pkg.mindenit.org`:

| Name | Kind | Value |
|---|---|---|
| `GLIDE_SIGNING_KEY` | secret | the ASCII-armoured private signing key (no passphrase) |
| `GLIDE_DEPLOY_KEY` | secret | the private SSH key of the upload user |
| `GLIDE_KNOWN_HOSTS` | secret | `ssh-keyscan <server>` output |
| `GLIDE_REPO_RSYNC` | variable | `deploy@<server>:/srv/pkg/glide/x86_64/` |
| `GLIDE_REPO_URL` | variable | `https://pkg.mindenit.org/glide/x86_64` |

A signing key for this purpose only:

```sh
gpg --batch --passphrase '' --quick-gen-key "Glide packages" ed25519 sign 3y
gpg --armor --export-secret-keys "Glide packages"   # -> GLIDE_SIGNING_KEY
gpg --armor --export "Glide packages"               # -> /srv/pkg/glide/glide.asc
```
