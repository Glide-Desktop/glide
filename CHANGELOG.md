# Changelog

Glide has not had a release yet. Everything below is in the `Unreleased`
section until the first tag. Items marked *unverified* have unit tests but
have not been tried on real hardware or against the real system service.

## Unreleased

### Session and security

- The lock state is mirrored to logind's `LockedHint` and read back at
  startup, so a compositor that crashes while locked comes back locked.
- A logind `handle-power-key` inhibitor is held: the power key opens the
  power dialog instead of powering off underneath it.
- Log Out, Restart and Shut Down ask every window to close first and wait;
  a window that keeps the session alive for 10 s cancels the action with a
  notice, and choosing it again within a minute ends the session anyway.
- Force Quit in the window menu and as the bindable `force-quit` action.
- Volume, brightness and media keys work on the lock screen; the lock
  screen and the greeter warn when Caps Lock is on.
- The greeter remembers the last user and session.

### Power and idle

- Idle policy: blank, then lock, then optionally suspend (separately on
  battery), configurable in Settings › Privacy and Settings › Power.
- `org.freedesktop.ScreenSaver` is served on the session bus, so browsers
  and video players can keep the screen on; logind gets `SetIdleHint`.
- The key or click that wakes a blanked screen is not passed to the app.
- Low and critical battery notifications.

### Notifications, agents and devices

- Bluetooth pairing agent (`org.bluez.Agent1`) and NetworkManager secret
  agent: PIN, passkey and password requests appear as notifications with
  buttons or a masked reply field. *unverified with bluetoothd / NM*
- Notification sounds from the `sound-file` / `sound-name` hints, with a
  switch in Settings; `xdg-system-bell-v1` plays the bell sound.
- A notice when the home disk is almost full, with a button to Storage.
- A notice for a newly attached removable drive with an Open button.
  *unverified with udisksd*
- On-screen indicator for volume, brightness and Caps Lock. *unverified*

### Windows and outputs

- Keyboard tiling: Super+Left / Right halves, Super+Up maximizes,
  Super+Down restores; Super+Shift+Left / Right moves a window to the
  neighbouring monitor.
- Super + left drag moves a window, Super + right drag resizes it; a short
  Super tap opens the launcher.
- Half-screen windows are told they are tiled (`xdg_toplevel` `tiled_*`).
- Fullscreen goes to the output the client asks for.
- `[[output]]` sections match monitors by make, model and serial before the
  connector name; screenshots take the monitor under the pointer.
- New key actions: show desktop, move window to next / previous workspace,
  notification centre, quick settings, Do Not Disturb, caffeine.

### Apps and launching

- XDG autostart is on by default and honours `AutostartCondition`,
  `X-GNOME-Autostart-Phase` and `X-GNOME-Autostart-Delay`.
- `Terminal=true` desktop entries open in a terminal; the default terminal
  is `[general] terminal`, else `$TERMINAL`, else the first one installed;
  Ctrl+Alt+T opens it.
- The launcher finds every Settings page.
- `xwayland-satellite` is restarted when it dies, and `DISPLAY` is only
  exported while it runs.
- `glide --help` and `glide --version`.

### Protocols

- `ext-workspace-v1` announces workspaces correctly: pagers no longer get
  an unknown object, and inactive workspaces are no longer marked hidden.
- `wlr-output-management-v1` keeps one head per output instead of a new
  one on every change, advertises every mode, validates configurations,
  really modesets, and can switch an output off and on again.
  *unverified with wlr-randr / kanshi*
- `xdg-foreign-v2`, so portal dialogs attach to the app that opened them.

### Robustness

- An unknown key in `glide.toml` is dropped with a notification instead of
  invalidating the whole file; a config that fails to load is reported as a
  notification.
- The five previous logs are kept (`glide.log.1` … `.5`).

### Packaging

- Release tarballs now include `glide-files`, which `install.sh` requires.
- Release builds keep line tables, so panic backtraces in `glide.log` name
  files and lines.
- CI workflow (Arch Linux container): format, clippy, tests, build.
