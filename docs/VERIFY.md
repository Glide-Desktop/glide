# Hardware verification checklist

Everything here builds, passes `cargo test` and has unit tests for its logic,
but none of it has been run on a real seat. Work through it on hardware
before a release and tick items off; anything that fails goes into an issue
with the `glide.log` excerpt (`$XDG_STATE_HOME/glide/glide.log`, previous
runs in `glide.log.1` … `.5`).

Run from a Glide session unless an item says otherwise. "Another VT" means
Ctrl+Alt+F3 and a text login.

## Session, lock and power

1. **LockedHint.** `loginctl lock-session`, then from another VT
   `loginctl show-session <id> -p LockedHint` → `yes`; unlock → `no`.
2. **Crash while locked.** Super+L, then from another VT
   `pkill -SEGV -x glide`. When `glide.service` restarts it, the session
   comes back on the lock screen, not on the desktop.
3. **Power key inhibitor.** `systemd-inhibit --list` lists Glide holding
   `handle-power-key` (block). Press the power key: the power dialog opens
   and the machine stays on.
4. **Power key on the lock screen.** Locked, press the power key: the Shut
   Down capsule arms; press again: it disarms.
5. **Log Out with unsaved work.** Type into a text editor, do not save,
   choose Log Out: the editor asks to save and the session stays. Leave it
   for 10 s: a toast says the session stayed. Choose Log Out again within a
   minute: the session ends anyway.
6. **Force Quit.** `kill -STOP <pid>` a GTK app, then titlebar menu › Force
   Quit: the process is gone (`pgrep`).
7. **Keys on the lock screen.** Volume, mute, brightness and media keys work
   while locked; the keyboard-backlight keys too (laptops with one). With
   Caps Lock on, the hint line says so (lock screen and greeter).
8. **Greeter memory.** Log in as a user with a non-default session, log out:
   the greeter preselects that user and session.

## Idle

9. **Blank and lock.** `[general] blank-after-seconds = 60`,
   `lock-after-seconds = 0`. Wait a minute: the screen blanks and the session
   locks. Press a key: the lock screen shows and the key is not typed into
   the password field.
10. **Inhibit from a browser.** Play a video in Chromium or Firefox and wait
    past the blank timeout: the screen stays on.
    `dbus-send --session --print-reply --dest=org.freedesktop.ScreenSaver /org/freedesktop/ScreenSaver org.freedesktop.ScreenSaver.GetActive`
    answers.
11. **IdleHint.** `loginctl show-session <id> -p IdleHint` turns `yes` with
    the blank.
12. **Sleep.** Suspend from quick settings, resume: the lock screen is drawn
    before anything else; no desktop frame flashes.

## Apps and launching

13. **XDG autostart.** A `~/.config/autostart/test.desktop` with
    `Exec=notify-send started` and `X-GNOME-Autostart-Delay=5` shows the
    toast about five seconds after login; `AutostartCondition=unless-exists
    glide-skip` with `~/.config/glide-skip` present skips it.
14. **Terminal.** Ctrl+Alt+T opens the default terminal; launching `htop`
    from the launcher opens it inside a terminal.
15. **XWayland supervision.** `pkill xwayland-satellite`: it is back within
    a second, `DISPLAY` is set in a new terminal and `xeyes` runs.
16. **Notification sound.** `notify-send -h string:sound-name:message-new-instant hi`
    plays a sound; with Settings › Notifications › Sounds off it does not.
17. **Bell.** In a terminal that speaks `xdg-system-bell-v1`, `printf '\a'`
    plays the bell.

## Agents and devices

18. **Bluetooth pairing.** Pair a keyboard from `bluetoothctl`: a toast shows
    the passkey to type. A device asking for a PIN gets a reply field.
19. **Network secrets.** `nmcli con modify <wifi> wifi-sec.psk-flags 2`,
    reconnect: a toast asks for the password with a masked field. Same for a
    VPN with a password.
20. **Removable drive.** Plug a USB stick: a toast offers Open, which mounts
    it and opens Glide Files there.
21. **Low disk.** `fallocate -l <size> ~/big` until less than the threshold
    is free: a toast offers Settings › Storage.
22. **Battery.** Discharge to `[topbar] battery-low-pct`: one toast; at 5 %
    a critical one.

## Windows and outputs

23. **Keyboard tiling.** Super+Left / Right halves, Super+Up maximizes,
    Super+Down restores. A GTK4 app on a half has no shadow or rounded
    corners on the tiled edge.
24. **Move to monitor.** Two monitors: Super+Shift+Left / Right moves the
    window and keeps its half or maximized state.
25. **Super drag.** Super + left drag moves a window, Super + right drag
    resizes it; a Super tap alone opens the launcher, Super+other key does
    not.
26. **Fullscreen output.** `mpv --fs --fs-screen=1 video.mkv` goes fullscreen
    on the second monitor.
27. **On-screen indicator.** Hold a volume key: the bar follows without
    flicker; Caps Lock shows on / off; keyboard-backlight keys show a level
    on laptops with a backlight.
28. **Monitor identity.** An `[[output]]` section with `make` / `model` /
    `serial` applies when the monitor moves to another port. Print takes the
    monitor under the pointer.
29. **Output management.** `wlr-randr` lists every mode of every head.
    `wlr-randr --output HDMI-A-1 --mode 1280x720` really changes the mode;
    `--off` turns the monitor dark (it stays listed as disabled); `--on`
    brings it back where it was. Unplug and replug a monitor twice:
    `wlr-randr` shows no duplicates. A kanshi profile applies.
30. **Lid.** Laptop with an external monitor: close the lid, the internal
    panel switches off and the dock, topbar and windows move; open it, the
    panel comes back in its old place. Unplug the external monitor with the
    lid closed: the panel comes back (or logind suspends).

## Shell

31. **Workspaces in a pager.** waybar with the `ext/workspaces` module shows
    every workspace, highlights the active one and switches on click.
32. **Audio output.** Quick settings › chevron beside the volume slider lists
    the outputs; picking HDMI or headphones moves the sound and the slider
    follows.
33. **Cursor theme.** Settings › Appearance › Pointer: the shell cursor
    changes at once; a newly started GTK app uses it;
    `gsettings get org.gnome.desktop.interface cursor-theme` matches.
34. **Font and wallpaper.** With `font-family` and a wallpaper set,
    `gsettings get org.gnome.desktop.interface font-name` and
    `gsettings get org.gnome.desktop.background picture-uri` match.
35. **Sandboxed shortcuts.** A Flatpak app that inhibits shortcuts (a remote
    desktop or VM viewer) triggers an Allow / Cancel toast; after Allow,
    Super+Esc takes the shortcuts back and says so.
36. **Config repair.** Add `[general] nonsense = 1` to `glide.toml`: a toast
    names the key and the rest of the file still applies.
37. **Search windows and files.** With a minimized Firefox window on another
    workspace, open the launcher and type part of its title: a Windows row
    appears; Enter switches workspace and restores it. Type part of a
    document you opened recently in a GTK app: a Recent Files row opens it.
38. **Keyboard to the shell.** Ctrl+F2 opens the app's first menu (or quick
    settings with a focus ring). Ctrl+F3 highlights the first dock tile;
    arrows move, Up opens its menu, Down / Enter pick a row, Esc backs out.
    With `[dock] autohide = "always"` the dock comes up for it. With the
    dock highlighted, click a window: the click lands in it.
39. **Run prompt.** Alt+F2, type `gnome-calculator`, Enter: it starts.
40. **Quick settings tiles.** Airplane mode on: `rfkill list` shows every
    radio soft-blocked; off again unblocks them. Power mode: each click
    moves `powerprofilesctl get` to the next profile. On a desktop without
    a backlight the brightness slider is dimmed and does nothing.
41. **Charge limit.** On a laptop whose UPower battery reports
    `ChargeThresholdSupported`, Settings › Power shows Preserve battery
    health; switching it on asks polkit and `ChargeThresholdEnabled` turns
    true.
42. **Users.** With a polkit agent running, add a user with a password:
    they can log in through the greeter. Make them administrator and back,
    change their password, remove them keeping and then deleting files.
    If the password step fails after the account was created, note what
    the greeter does with a password-less account.
43. **Touch.** On a touchscreen, drag a GTK4 app by its header bar with a
    finger: it moves and snaps to a half at the screen edge. Drag its
    corner: it resizes. Drag a file from Glide Files (or Nautilus) into
    another app with a finger: the drag icon follows the finger and the
    drop lands.
44. **Enterprise Wi-Fi.** Near an eduroam (802.1X) network, its quick
    settings row reads "Set up in Settings" and a click opens the Network
    page instead of a password field.
45. **Toasts behind the lock.** Lock, then `notify-send -t 3000 hi` from
    another VT or ssh session; wait a minute and unlock: the toast is still
    on screen and then times out normally.
46. **Recording shortcuts.** Settings › Keyboard Shortcuts › edit a binding
    › Record, press Ctrl+Alt+K: the editor shows that combination.
47. **Fixed workspaces.** Settings › Multitasking › Fixed number, 4: four
    workspaces exist with no windows, closing the last window of one keeps
    it, and a three-finger swipe past the fourth bounces back.
48. **Scheduled appearance.** Settings › Appearance › Switch automatically
    in the afternoon: the shell is light. Set the clock (a VM, or
    `timedatectl set-time`) past 19:00: within a second the shell turns
    dark — that switch is a full configuration reload, so watch for any
    flicker on outputs or input settings.
49. **Single-pixel buffers.** A recent GTK4 app or mpv with letterboxing
    renders its solid areas correctly.

## Packaging

50. **Tarball.** `scripts/package.sh`, unpack on another machine,
    `sudo scripts/install.sh`: it installs, including `glide-files`.
51. **Crash log.** After item 2, the log of the killed run survives as
    `glide.log.1`. When a release build panics, its log holds the message
    and a backtrace with file and line numbers (a signal such as SIGSEGV
    leaves no backtrace).
