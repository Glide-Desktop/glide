# Glide Settings (`glide-settings`) — Project Status & Handoff Guide

> **Notice for AI Assistants & Developers:**
> This document tracks the architectural state, completed components, and remaining work for `crates/glide-settings`.
> Read it first before resuming development or reviewing the codebase.

---

## 1. Project Context & Repository Rules

`glide-settings` is the native system settings application for the **Glide Desktop Environment**.
It is built in **Rust** with **Iced 0.14** and follows the **Meridian Design System v3** (`design/MERIDIAN_DESIGN_SPEC.md`).

### Critical Workspace Invariants
1. **No comments in `.rs` files**, except a short justification next to `#[allow(...)]` and `// SAFETY:` on `unsafe`.
2. **Clippy zero-warning policy**: `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all -- --check`.
3. **Dedicated file tools only.** Never edit repo files through a shell.
4. **Meridian v3**: radii `R1`–`R4` + `R_ROUND`, state layers 8 / 13 / 18 %, 8 accent ramps, live theme switching.

---

## 2. Design Language: GNOME × macOS

The shell mixes the two platforms deliberately:

| Element | Borrowed from | Implementation |
|---|---|---|
| Sidebar with coloured rounded-square icon tiles, grouped with gaps | macOS System Settings | `sidebar.rs`, `widgets::tile`, `Panel::GROUPS`, `Panel::color` |
| Search field at the top of the sidebar, account card under it | macOS | `sidebar.rs` (`Ctrl+F` focuses search, `Esc` clears it, `Enter` opens the first hit) |
| Boxed lists: group title + description above a card of rows with inset separators | GNOME / libadwaita `AdwPreferencesGroup` | `widgets::Group` |
| Action rows: title, subtitle, prefix icon, suffix control | libadwaita `AdwActionRow` | `widgets::Row` |
| Hero card with a large tile and a master switch (Wi-Fi, Bluetooth) | macOS | `widgets::hero` |
| Centered content clamp (max 680 px) | libadwaita `AdwClamp` | `app.rs::view_content` |
| Style previews (Light / Dark mini windows), accent colour dots with check | GNOME 47 Appearance + macOS | `panels/appearance.rs` |
| Display arrangement diagram | macOS Displays | `panels/displays.rs` |
| "About" hero with logo, version and a Copy button | macOS "About This Mac" | `panels/about.rs` |

Rows inside a group sit in a 4 px padded card so their R2 hover layer is concentric with the R3 card corner.

### Visual system (pinned by `tests/layout_tests.rs`)

- **One card style** for every boxed list, hero, arrangement and About card: `card_bg`, 1 px `line` hairline, R3, `--el-1` shadow taken from `glide_theme::elevation` (the old shadow used the `state` colour, which is white in dark mode and drew a light rim under every card — the "double bottom border").
- **Blending matches the CSS reference.** iced is built with `web-colors`; without it iced blends in linear space and every translucent token (`line`, `fill-*`, `fg-2..4`, dark `acc-soft`) rendered much brighter than specified in dark mode — hairlines read as grey borders and dropdowns as grey slabs.
- **Rows** are at least 44 px: 5 px vertical padding around a 34 px content strut, so a 34 px control (button, dropdown, field, segmented) fits exactly and every row in a card has the same height. Title 14 px `fg-1`, subtitle 12 px `fg-2`. Separators are inset 12 px on both sides, never after the last row.
- **Rhythm**: page padding 24 / 32, 24 px between groups, 8 px between a group heading and its card; page header is a 26 px title plus a one-line description per panel.
- **Controls**: buttons are 34 px with transparent borders (`.btn` base = `fill-1`); icon buttons are 28 px round; the segmented track is `fill-1` without a border and its active thumb is `card-bg` (light) / `fill-3` (dark) with `--el-1`; switches are 48×24 with an 18 px thumb; the sidebar search is a borderless `fill-1` well that turns `view-bg` with an accent border on focus and gains a clear button while a query is typed.
- **Selection rings** (style previews, wallpapers, accent swatches, monitors) are 2 px with a 4 px gap, so the inner R2 image stays concentric with the R3 ring and selecting never shifts neighbours.
- **Sidebar**: 32 px rows, 24 px tiles, inactive labels `fg-1`, hover is the 8 % state layer, selected is `acc-soft` / `acc-soft-fg`; the account card uses the spec `.avatar` gradient.
- **Panels**: Sound lists output / input devices as rows with a check (a dropdown only past six devices) under volume groups; Wi-Fi rows show a disclosure chevron only when a click expands them (connected, or secured and unsaved); the Displays arrangement sits in a regular card with gradient monitor tiles; About opens with a hero card (logo, hostname, Glide version and OS, CPU and memory).

---

## 3. Milestone Execution Status

| Milestone | Scope | Status | Notes |
|---|---|---|---|
| **Phase 0** | Architecture survey & blueprint | ✅ Done | |
| **Milestone 1** | Crate setup & Meridian Iced theme layer | ✅ Done | `theme.rs`, extended with row / segmented / badge / banner / pick list / menu / scrollable styles. |
| **Milestone 2** | Shell master–detail layout, search, SSD | ✅ Done | `app.rs`, `sidebar.rs`, `widgets.rs`, `icons.rs` (inline SVG, tinted per theme). Window requests server-side decorations and sets `app_id = glide-settings`. |
| **Milestone 3** | Compositor IPC & Displays panel | ✅ Done | New `Request::ListOutputs` / `Response::Outputs` in `glide-ipc`, handled in the compositor and exposed as `glidectl outputs`. Falls back to `/sys/class/drm` when the compositor is not running or predates `ListOutputs`. |
| **Milestone 4** | Appearance & About panels | ✅ Done | Theme, 8 accents, wallpaper gallery with background thumbnails (built-in Glide wallpaper first), fit, custom path, animations. About reads `/proc`, `/etc/os-release`, `lspci`, `df`. |
| **Milestone 5** | Sound, Network, Bluetooth, Keyboard (+ Mouse & Touchpad) | ✅ Done | Uses the same CLI tools as the topbar (`wpctl`, `nmcli`, `bluetoothctl`, `rfkill`) instead of D-Bus, so there is one integration path. Mouse & Touchpad was added as an eighth panel because the compositor already exposes those keys. |
| **Milestone 7** | Theme propagation & remaining config coverage | ✅ Done | Theme now reaches GTK / libadwaita / portal-aware Qt apps: both the compositor (on every config apply) and the app (after each save) write `org.gnome.desktop.interface` `color-scheme`, `accent-color`, `gtk-theme` (only when it is Adwaita / Adwaita-dark) and `icon-theme`. New panels: Desktop & Dock, Windows, Power, Apps (default apps via `xdg-mime`, startup commands). Appearance gained icon theme and interface font; Sound gained over-amplification. |
| **Milestone 6** | Tests & quality audit | ✅ Done | 43 new tests (services parsing, config editing, app navigation, i18n) on top of the 53 theme tests. Workspace fmt / clippy / test / build pass. |

---

## 4. How Settings Are Applied

`glide-settings` edits `$XDG_CONFIG_HOME/glide/glide.toml` (or `~/.config/glide/glide.toml`) with `toml_edit`, so user comments, key order and unrelated sections survive. After every change it saves atomically (temp file + rename) and sends `Request::Reload`. The compositor re-reads the file and applies:

| Panel | Keys written |
|---|---|
| Appearance | `[topbar] appearance`, `accent`; `[general] wallpaper`, `wallpaper-fit`; `[animations] enabled`, `speed`, `respect-system` |
| Displays | `[[output]] name`, `scale`; `[night-light] schedule`, `from`, `to` |
| Keyboard | `[input] xkb-layout`, `xkb-variant`, `xkb-options` (only the `grp:` switch option is replaced), `repeat-delay`, `repeat-rate`; `[general] language` |
| Mouse & Touchpad | `[input] accel-speed`, `accel-profile`, `natural-scroll`, `tap-to-click` |
| Appearance (fonts & icons) | `[dock] icon-theme`; `[general] font-family` |
| Desktop & Dock | `[dock] enabled`, `autohide` (`smart` / `always` / `never`), `item-size`, `icon-size`; `[topbar] enabled`, `global-menu`, `appmenu-registrar`; `[workspaces] new-window`; `[overview] label` |
| Windows | `[decorations] title-align`, `height`, `shadow`, `border-width` |
| Power | `[general] blank-after-seconds`, `suspend-after-seconds`, `suspend-on-battery-after-seconds`; `[topbar] battery-low-pct`; power mode through `powerprofilesctl` when installed |
| Privacy & Security | `[general] blank-after-seconds`, `lock-after-seconds` (0 = with the blank, -1 = never) |
| Sound | `[topbar] max-volume-pct` (100 or 150) |
| Apps | `[general] autostart`; default handlers through `xdg-mime default` |

`crates/glide-compositor/src/config.rs::every_key_written_by_glide_settings_is_accepted` parses one file containing every key above, so a rename on either side fails the build instead of silently resetting the user's config (the compositor uses `deny_unknown_fields`).

Only keys the compositor already deserializes are written. The compositor now ignores a key it does not know (with a notification naming it) instead of rejecting the file, so a newer Settings next to an older compositor no longer resets the session to the last good config. If `glide.toml` fails to parse or cannot be read, the app shows a banner naming which, and every config-backed control is made inert (`widgets::inert`) instead of overwriting the file. Before each config-changing message the file's mtime is checked and an externally edited file is re-read first; saves go through the canonical path, so a symlinked `glide.toml` stays a symlink. Sliders commit on release or 400 ms after the last change (keyboard arrows and Ctrl+wheel never fire a release); volume sliders stream to `wpctl` with at most one command in flight.

`[animations] speed` is a duration multiplier in the compositor; the Appearance slider shows its inverse (0.5×–2×, "faster" is to the right) and writes `1 / speed`.

Network, Bluetooth and Power tag each background read with a generation; a user action bumps it, so a read that started before the action can no longer revert the new state. Saved Wi-Fi networks are matched by the profile's `802-11-wireless.ssid` and connected / forgotten by UUID.

When the compositor is not reachable the change is still saved, and a banner says it applies on the next start. When it answers `Reload` with an error (it keeps the running config), an error banner shows the compositor's message. An IPC event subscription tracks whether Glide is up and refreshes Displays on `OutputChanged`.

---

## 5. File Map

```
crates/glide-settings/
├── STATUS.md
├── src/
│   ├── main.rs            window settings, app id, SSD; `glide-settings <panel>` opens a panel
│   ├── lib.rs
│   ├── app.rs             root state, message routing, save + reload, subscriptions
│   ├── sidebar.rs         search, account card, grouped panel list
│   ├── widgets.rs         Group, Row, hero, segmented, switch, dropdown, banner, badge, …
│   ├── icons.rs           inline SVG glyphs
│   ├── i18n.rs            uk / en catalogue (follows `[general] language`)
│   ├── theme.rs           Meridian tokens and Iced styles
│   ├── panels/            27 panels; mod.rs holds the `Pane` trait, the `with_panels!` registry and sidebar groups
│   └── services/          config, ipc, display, audio, network, bluetooth, input, system, wallpaper, run
└── tests/
    ├── theme_tests.rs, adversarial_theme_tests.rs, theme_adversarial_tests.rs
    ├── layout_tests.rs    card / row / spacing / ring invariants of the visual system
    ├── services_tests.rs  parsers for wpctl / nmcli / bluetoothctl / xkb / proc, config editing
    └── app_tests.rs       navigation, search, live theming, keyboard sources, i18n completeness
```

`data/glide-settings.desktop` is installed by `scripts/install-session.sh`, and the quick-settings "Settings" button now tries `glide-settings` first.

---

## 6. Known Gaps

- **2026-09-25 expansion (12 → 27 panels) is only partly checked by hand.** Dock, Storage, Power & Battery, Keyboard Shortcuts and Network were screenshotted in dark mode; the other panels, light mode and interaction states were not. Runtime effects not exercised: DRM mode switching, night-light temperature, libinput per-device options, Num Lock, hot corner, titlebar button layout, notification banner position / DND schedule, XDG autostart at sign-in, CUPS actions, polkit-guarded timedate1 / locale1 / hostname1 / AccountsService writes, hotspot, VPN import.
- Keyboard Shortcuts has no key capture: the key name is typed, because panels get no keyboard subscription.
- The Notifications panel shows the configured Do Not Disturb value, not the live top-bar toggle state.

- **Theme propagation needs the new compositor build.** The installed Glide (built 2026-09-23) predates the GSettings sync; until `scripts/install-session.sh` is rerun, only the Settings app itself syncs the theme, and only when a setting is changed from it.
- `appmenu-registrar` only takes effect on the next sign-in.

- **Not verified by hand**: connecting to a secured Wi-Fi network, pairing a Bluetooth device, and changing the output scale on the new compositor build. The UI and parsers are tested; the effects on real hardware were not exercised.
- **Resolution and refresh rate are read-only.** The compositor config has no mode key, so there is nothing to write.
- **Wi-Fi passwords never reach argv.** A connect with a password goes over NetworkManager's D-Bus API (`AddAndActivateConnection` with an `802-11-wireless-security.psk` setting, the strongest matching AP as the specific object), waits for activation and deletes the new profile if activation fails. Everything else still uses `nmcli`. The D-Bus signatures and property names were checked against the running NetworkManager with `busctl`; an actual secured connect was not exercised.
- **No transitions.** Iced has no built-in tweening for these widgets, so the Meridian motion rules (t1–t4) are not applied inside the app.
- **Bluetooth pairing** relies on bluetoothctl's default agent; devices that need a PIN confirmation will fail with the tool's error message.
- **Visual pass verified on screen** (2026-09-24): every panel was screenshotted in dark and light (light via a temporary `XDG_CONFIG_HOME` with `appearance = "light"`) before and after the redesign. Hover / press / focus states and open dropdown menus were not captured.
- There are no sub-pages, so there is no back navigation; Wi-Fi details expand inline.
- iced's toggler has a fixed 2:1 aspect ratio, so the switch is 48×24 rather than the spec's 44×26, and neither the switch thumb shadow nor the 3 px focus ring (`box-shadow`) can be drawn.
- **Layout at 760×480 was inspected on screen** (Sound, Network, Mouse, Keyboard, Appearance, plus the read-only banner) by launching with `GLIDE_SETTINGS_SIZE=760x480`. Row labels and suffix controls split the row evenly (`FillPortion(1)` each, suffix capped at its old fixed width), the accent hint moved to the group description, and TOML parse errors are condensed to one line (`line L, column C: message`). Long device names may be truncated inside dropdowns at the minimum width.
- **Saved-network lookup costs one `nmcli` call per Wi-Fi profile** on every 4 s refresh of the Network panel.

---

## 7. Verification Commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```
