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
- Closing the lid with an external monitor attached switches the laptop
  panel off and moves the shell to the other display; opening it, or
  unplugging the monitor, brings the panel back. *unverified*
- Ctrl+F2 reaches the top bar (the app's menus, or quick settings) and
  Ctrl+F3 the dock, which then takes arrows, Enter, its menus and Esc.

### Shell

- Quick settings follow the new concept: a header with the user, eight
  tiles in two columns (Do Not Disturb and Dark Style are new, Wi-Fi and
  Bluetooth toggle from their badge and open their list from the rest of
  the card), one card for volume and brightness with readouts and an
  output pill, a battery card with time left and a Saver / Balanced /
  Performance switch, and power buttons that confirm inline — Restart,
  Shut Down and Log Out count down from 60 s. *rendering unverified*
- Quick settings has a microphone row: level, mute, and a dot while an
  app is recording.
- The output and input names on the volume and microphone rows unfold a
  list of devices in place; picking one makes it the default.
- The Wi-Fi, Bluetooth, Sound and Battery views use cards under section
  labels; a Wi-Fi password opens in a drawer under its network with
  Cancel and Connect; Sound lists microphones too; Battery shows health,
  cycles, power draw and capacity, and picks the power profile.
  *rendering unverified*
- Quick settings: an audio output picker; airplane mode; sliders for
  controls the machine lacks (no backlight, no audio tools) are dimmed
  and inert instead of showing made-up levels.
- The launcher also finds open windows and recently used files, and Alt+F2
  turns it into a Run prompt.
- Keyboard backlight keys step the backlight and show its level; the
  Search and Calculator keys open the launcher.
- Enterprise (802.1X) Wi-Fi networks in quick settings lead to Settings
  instead of asking for a passphrase that could not work; the Wi-Fi
  password field warns about Caps Lock.
- Toasts and agent prompts that arrive behind the lock screen wait for the
  unlock instead of expiring unseen.
- A first login shows a welcome with the essential keys and gestures.
- Workspaces can be fixed in number (Settings › Multitasking).
- Light and dark can switch by the clock (Settings › Appearance).
- Windows can be moved and resized with a finger, and drags started by
  touch work. *unverified on a touchscreen*
- `[general] cursor-theme` (Settings › Appearance › Pointer) changes the
  pointer for the shell and for newly started apps; cursor, font and
  wallpaper are shared with GTK apps through GSettings.

### Settings

- Users: add and remove accounts, switch them between standard and
  administrator, and change passwords. *unverified with a polkit agent*
- Power: a Preserve battery health switch on batteries whose firmware
  supports a charge limit (UPower).
- Keyboard Shortcuts: Record takes the next key press instead of a typed
  key name.

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
- `single-pixel-buffer-v1` and `content-type-v1`.
- Sandboxed clients no longer see layer shell, the toplevel list or the
  workspace protocol, and must be allowed by the user before they inhibit
  keyboard shortcuts; Super+Esc takes the shortcuts back from any window.

### Fixes

- A configuration reload (any change in Settings, the scheduled
  appearance, the Dark Style tile) no longer switches the keyboard back to
  the first layout or clears Caps Lock; the keymap is only rebuilt when
  the layouts or options change.
- Themed cursors are drawn with their hotspot on the pointer. The text
  I-beam used to sit about half a line below and to the right of the point
  that actually received the click, so selections started somewhere else.

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
