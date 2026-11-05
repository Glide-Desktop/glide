# Glide

A Wayland desktop: compositor with its own top bar, dock, launcher, overview,
notification centre, quick settings, and a settings app (`glide-settings`).
Built on [smithay](https://github.com/Smithay/smithay). Runs directly on
DRM/KMS from a display manager or a TTY; it does not run nested inside another
compositor.

Interface languages: Ukrainian and English. The language follows `LANG`, and
you can override it with `[general] language` in `~/.config/glide/glide.toml`.

## Requirements

- Linux with a KMS-capable GPU. Intel, AMD, and Nouveau/NVK work through
  Mesa. The proprietary NVIDIA driver needs `nvidia-drm.modeset=1` and a
  driver with GBM support (495 or newer).
- `systemd-logind` or `seatd`, `udev`, Mesa EGL/GLES.
- Rust 1.96 or newer. Install it with [rustup](https://rustup.rs); distribution
  packages are often too old.

### Build dependencies

| Distribution | Command |
|---|---|
| Arch | `sudo pacman -S --needed base-devel git pkgconf seatd systemd-libs libinput mesa libxkbcommon libdisplay-info` |
| Debian 13 / Ubuntu 24.04+ | `sudo apt install build-essential git pkg-config libseat-dev libudev-dev libinput-dev libgbm-dev libxkbcommon-dev libdisplay-info-dev` |
| Fedora | `sudo dnf install gcc git pkgconf-pkg-config libseat-devel systemd-devel libinput-devel mesa-libgbm-devel libxkbcommon-devel libdisplay-info-devel` |
| openSUSE | `sudo zypper install gcc git pkgconf libseat-devel systemd-devel libinput-devel libgbm-devel libxkbcommon-devel libdisplay-info-devel` |

`scripts/build.sh` checks all of these and prints the right command for your
distribution when something is missing.

### Runtime (optional, recommended)

| Package | For |
|---|---|
| `xwayland-satellite` | X11 applications |
| `xdg-desktop-portal`, `-gtk`, `-wlr`, `pipewire`, `wireplumber` | file dialogs, screen sharing, audio |
| `libpam` with `/etc/pam.d/glide-lock` (installed by `scripts/install.sh`) | the built-in lock screen (Super+L, power menu, idle); without PAM it cannot be unlocked |
| `gnome-keyring` (or another `org.freedesktop.secrets` provider) | saved passwords for browsers, mail and sync clients; add `pam_gnome_keyring.so` to your greeter's PAM service so the keyring unlocks at login |
| `fcitx5` or `ibus` | input methods (CJK, Indic); started from `/etc/xdg/autostart` |
| `brightnessctl` | brightness keys |
| `pactl` (libpulse), `rtkit` | sound devices and headphone unplugging; realtime priority for PipeWire |
| `NetworkManager`, `bluez`, `power-profiles-daemon` | Wi-Fi, Bluetooth and power mode in quick settings |
| `polkit` (with `polkit-agent-helper-1`) | administrator password prompts; Glide is its own polkit agent and asks in the status island (`[general] polkit-agent = false` turns it off, and then another agent has to be started through `[general] autostart`) |
| Colloid icon theme (`colloid-icon-theme-git` on the AUR, or `./install.sh -t all` from [vinceliuice/Colloid-icon-theme](https://github.com/vinceliuice/Colloid-icon-theme)) | the default icon set: the variant that matches the appearance and accent (`Colloid-Teal-Dark`, …) is picked unless `[dock] icon-theme` names another |
| `papirus-icon-theme` | the fallback when Colloid is not installed (then Adwaita/hicolor) |

Glide has its own notification daemon. Do not also run mako, dunst or swaync
in a Glide session.

XDG autostart entries (`/etc/xdg/autostart`, `~/.config/autostart`) are started
at login: `OnlyShowIn` / `NotShowIn` are matched against `Glide`,
`AutostartCondition` (`GSettings`, `if-exists`, `unless-exists`, `GNOME3
if-session` / `unless-session`), `X-GNOME-Autostart-Phase` and
`X-GNOME-Autostart-Delay` are honoured, `Terminal=true` entries are skipped.
`[general] xdg-autostart = false` turns this off.

## Build and install

On Arch Linux and derivatives, run `sh scripts/setup.sh`. It offers to install
missing build and recommended runtime packages, builds Glide, then installs it
with sudo. It offers to configure Rust when missing; Rust 1.96 or newer is required. Installer options
such as `--dm` and `--pam` can be passed to this command. On other distributions,
install the dependencies manually and use the commands below.

For audio, PipeWire and WirePlumber must also be running in the user session.
Glide Phone discovery needs UDP 1716 and TCP 1716–1764 allowed on the local
network; another KDE Connect service using UDP 1716 prevents discovery. The
setup script does not change firewall rules and asks before enabling services.

```sh
git clone <repo> glide && cd glide
scripts/build.sh               # as your user; never as root
sudo scripts/install.sh        # installs to /usr/local
```

`build.sh` compiles through `scripts/ai-cargo.sh`: one job, 35 % of the CPU
and boost off, so a laptop stays cool but the build takes longer. Pass
`--fast` (also accepted by `install-session.sh`) to use every core.

Then log out and choose **Glide** in your display manager (GDM, SDDM,
LightDM, greetd…). From a bare TTY, run `glide-session`.

`install.sh` options:

- `--prefix /usr` installs under `/usr` instead of `/usr/local`.
- `--destdir DIR` stages the files for a distribution package.
- `--uninstall` removes everything it installed.

### What gets installed

| Path | |
|---|---|
| `PREFIX/bin/glide`, `glidectl`, `glide-settings` | compositor, IPC client, settings app |
| `PREFIX/bin/glide-session`, `glide-lock` | session launcher, screen-lock helper |
| `/usr/share/wayland-sessions/glide.desktop` | display-manager entry |
| `PREFIX/lib/systemd/user/glide.service`, `glide-shutdown.target` | systemd user units |
| `/usr/share/xdg-desktop-portal/glide-portals.conf` | portal backend selection |
| `PREFIX/share/applications/glide-settings.desktop` | Settings launcher entry |

User files live in `~/.config/glide/` (`glide.toml`, `pins.toml`). The log is
`~/.local/state/glide/glide.log`.

## Sharing a prebuilt build

```sh
scripts/package.sh             # writes dist/glide-<version>-<arch>.tar.gz
```

Whoever receives it runs:

```sh
tar xf glide-*.tar.gz && cd glide-*/ && sudo scripts/install.sh
```

The binaries are linked against the build machine's glibc and system
libraries, and `BUILD-INFO` in the tarball lists them. A tarball built on a
rolling distribution such as Arch does **not** run on older distributions
(Debian stable, Ubuntu LTS). Those users should build from source.

## Configuration

Glide works without a config file. By default:

- The keyboard layout comes from the system (`localectl`, `/etc/vconsole.conf`,
  `/etc/default/keyboard` or `XKB_DEFAULT_*`), else from your locale.
- The output scale is picked from the screen's physical size and resolution:
  1, 1.5, 2 or 3. A value in the config always wins:

```toml
[[output]]
name = "eDP-1"      # glidectl outputs lists the names
scale = 1.5
```

Most options are also available in the Settings app. After editing the file by
hand, run `glidectl reload`.

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`scripts/test-tty.sh` runs a debug build from a free VT and checks the log.
Project rules are in `AGENTS.md`.

## Writing apps for Glide

`crates/glide-sdk` is the toolkit for third-party apps in Glide's design
system: window chrome, menus in the top bar, dialogs, widgets and system
integration in one crate. Start with `docs/SDK.md` and
`cargo run -p glide-sdk --example notes`.

## License

GPL-3.0-or-later.
