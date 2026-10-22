# Glide SDK

`glide-sdk` is the toolkit for writing applications that look and behave like
Glide's own: the Meridian window chrome, the shell's appearance, accent,
language, font and motion settings, menus in the top bar, dialogs, toasts and
the shared window frame drawn by the compositor. It is a Rust crate built on
[iced](https://iced.rs) 0.14; the parts that cross process boundaries are
plain D-Bus and Wayland protocols, listed at the end, so other languages can
implement them too.

Two examples come with it: [`counter.rs`](../crates/glide-sdk/examples/counter.rs)
is the quick start below, [`notes.rs`](../crates/glide-sdk/examples/notes.rs)
a complete app with a sidebar, boxed lists, menus, a dialog, toasts,
notifications and a dock badge. The **component catalogue**,
[`crates/glide-catalog`](../crates/glide-catalog), shows every component live
and doubles as a notification and dock test bench:

```
cargo run -p glide-catalog
cargo run -p glide-sdk --example notes
```

## Contents

1. [Quick start](#quick-start)
2. [The `App` trait](#the-app-trait)
3. [The `Shell` handle](#the-shell-handle)
4. [Menus and shortcuts](#menus-and-shortcuts)
5. [Dialogs, toasts and context menus](#dialogs-toasts-and-context-menus)
6. [Widgets](#widgets)
7. [Motion](#motion)
8. [Keyboard](#keyboard)
9. [Design rules](#design-rules)
10. [System integration](#system-integration)
11. [Testing](#testing)
12. [Packaging](#packaging)
13. [Contracts for other languages](#contracts-for-other-languages)
14. [Limitations](#limitations)
15. [A Go SDK: how hard would it be](#a-go-sdk-how-hard-would-it-be)

## Quick start

```toml
[dependencies]
glide-sdk = { path = "../glide/crates/glide-sdk" }
```

The SDK is not on crates.io and the repository has no public remote yet, so
depend on a checkout by path (or vendor it); switch to a `git = …` line once
the repository is published. Copy Glide's `Cargo.lock` the first time so iced
and its dependencies resolve to the versions Glide is tested with.

`glide-sdk` re-exports `iced`, so an app depends on nothing else to draw.
Use `glide_sdk::iced` rather than adding iced yourself, or the versions will
drift apart.

```rust
use glide_sdk::iced::widget::column;
use glide_sdk::prelude::*;

struct Counter {
    count: u32,
}

#[derive(Debug, Clone)]
enum Message {
    Increment,
    Reset,
}

impl App for Counter {
    type Message = Message;

    fn new(_shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        (Self { count: 0 }, Task::none())
    }

    fn title(&self, shell: &Shell<Message>) -> String {
        shell.t("Лічильник", "Counter").into()
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Increment => self.count += 1,
            Message::Reset => {
                self.count = 0;
                shell.toast(shell.t("Скинуто", "Reset"));
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        container(
            column![
                text(self.count.to_string(), TextStyle::T1),
                widget::primary(paint, shell.t("Додати", "Add"), Message::Increment),
            ]
            .spacing(SP3),
        )
        .center(Fill)
        .into()
    }

    fn menus(&self, shell: &Shell<Message>) -> Vec<Menu<Message>> {
        vec![Menu::new(
            shell.t("Правка", "Edit"),
            vec![Item::action(shell.t("Скинути", "Reset"), Message::Reset).shortcut("Ctrl+R")],
        )]
    }
}

fn main() -> glide_sdk::iced::Result {
    glide_sdk::run::<Counter>(Settings::new("org.example.Counter").size(480.0, 360.0))
}
```

That is a finished Glide app: rounded window with the Meridian header,
maximize and close buttons, resize edges, a menu bar that moves into the top
bar while the shell offers it, Ctrl+R bound by physical key on every layout,
light/dark/accent/language/font/motion following the shell live, and the
compositor's shadow, ring and corner clip around it.

## The `App` trait

| Method | Required | What it is for |
|---|---|---|
| `new(shell)` | yes | Build the state; the returned `Task` runs at start. |
| `title(shell)` | yes | Window title and the header's centred title. |
| `update(message, shell)` | yes | Handle a message; the returned `Task` is run. |
| `view(shell)` | yes | The content below the header. It fills the window. |
| `subtitle(shell)` | no | Second line under the title (hidden below 360 px of height). |
| `menus(shell)` | no | The menu bar. Rebuilt after every message, so `enabled` and `checked` can follow state. |
| `toolbar(shell)` | no | Extra header buttons, left of close. Use `widget::icon_button`. |
| `subscription()` | no | Your own subscriptions (timers, file watchers…). |
| `close_requested(shell)` | no | Return `false` to keep the window, e.g. to ask about unsaved changes, then call `shell.exit()` later. |

`Settings::new(app_id)` takes the Wayland app id, which must equal the
desktop file name (`org.example.Notes` ↔ `org.example.Notes.desktop`): the
dock, the launcher, quicklists and the LauncherEntry badge all key on it.
`.size(w, h)`, `.min_size(w, h)` and `.font(bytes)` complete it.

## The `Shell` handle

Every method receives the `Shell`, which is the app's connection to the
system.

| Call | Effect |
|---|---|
| `paint()` | Current colour tokens (`Paint`), already following appearance and accent. Pass it to every widget. |
| `lang()`, `t(uk, en)` | Interface language (`Lang::Uk` / `Lang::En`) and a two-way pick. |
| `look()` | Everything the shell publishes: appearance, accent, language, motion scale, font family, icon theme, sounds. |
| `window()` | Size, focus, `Frame::{Floating, Maximized, Fullscreen}`. |
| `pointer()` | Last pointer position in the window. |
| `menu_in_top_bar()` | Whether the top bar is showing the app's menus. |
| `toast(text)`, `toast_with(text, action, msg)` | Bottom pill for 4 s, optionally with an action button (Undo). |
| `preview_look(Some((appearance, accent)))` | Repaint this window alone in another look (previews, galleries); `None` follows the shell again. |
| `dialog(Dialog)`, `close_dialog()`, `has_dialog()` | Modal dialog. |
| `context_menu(items)`, `context_menu_at(point, items)`, `close_menu()` | Popup menu at the pointer or a point. |
| `window_action(Action)` | `Drag`, `ToggleMaximize`, `Minimize`, `Resize(direction)`. |
| `exit()` | Quit after this update. |

## Menus and shortcuts

```rust
Menu::new("File", vec![
    Item::action("Save", Message::Save).shortcut("Ctrl+S").enabled(self.dirty),
    Item::separator(),
    Item::submenu("Export", vec![Item::action("PDF", Message::Pdf)]),
    Item::heading("Recent"),
    Item::action("Word wrap", Message::Wrap).checked(self.wrap),
])
```

- While Glide's global menu is on, the menus are exported over
  `com.canonical.dbusmenu` at `/MenuBar` and attached to the window with
  `org_kde_kwin_appmenu`; the header then shows no menu bar. Otherwise the
  header shows them, with hover switching, F10, arrows, Enter and Esc.
- A shortcut string is the binding: `Ctrl`, `Shift`, `Alt`, `Super` plus a
  key (`S`, `+`, `F3`, `Delete`, `Up`…). Letters match by physical key, so
  Ctrl+S works on a Ukrainian layout. Disabled items do not fire. Keys a
  focused widget consumes are left to it: an iced 0.14 text field takes
  typed characters, Ctrl+A/C/X/V, Backspace, Delete, Home, End, arrows,
  Enter and Esc, and lets every other chord through to the menus.

## Dialogs, toasts and context menus

```rust
shell.dialog(
    Dialog::new("Delete “Groceries”?", "This cannot be undone.")
        .choice("Delete", Tone::Danger, Message::Delete)
        .cancel("Cancel")
        .choice("Archive", Tone::Accent, Message::Archive),
);
```

Danger choices sit at the left, the rest at the right in the order given.
Enter takes the accent choice, Esc and a click on the scrim dismiss. The card
opens and closes with the spec's scale and fade. For a dialog with custom
content, build it in `view` with `widget::popover` and `iced::widget::stack`.

## Widgets

All live in `glide_sdk::widget` and take `&Paint` first. Every one moves by
the spec's motion table; the catalogue shows each of them.

**Buttons**

| Widget | Notes |
|---|---|
| `button(paint, label, tone, Option<msg>)`, `primary`, `secondary`, `tonal`, `outline`, `ghost`, `danger` | `Tone::{Accent, Tonal, Neutral, Outline, Ghost, Danger}`; the primary rises El1 → El2, danger fills on hover, a press squeezes to .975 and state layers crossfade over t1. Labels may be `&str` or `String`. |
| `sized_button(…, Size::{Small, Medium, Large}, …)`, `compact_button` | 28 / 34 / 42 px with the spec's padding and label sizes. |
| `icon_button(paint, Icon, tooltip, Option<msg>, on)` | 28 px square with a tooltip. |
| `linked(paint, &[(label, Option<msg>)])` | Joined group, R2 outside and R1 inside. |
| `toggle_button(paint, label, on, Option<on_toggle>)` | Neutral when off, tonal when on; fill and ink crossfade over t2. |
| `menu_button(paint, label, items, width)` | Opens a menu of actions under itself (keyboard rows, Esc, outside click). |

**Inputs**

| Widget | Notes |
|---|---|
| `text_field(paint, placeholder, value, on_input)`, `field(paint, text_input)` | Fill-1 well, accent border and a 3 px halo that fades in on focus. `field` wraps any `text_input` (ids, submit, secure). |
| `validated_field(paint, text_input, Validity::{Neutral, Valid, Invalid})` | OK / danger border and ring. |
| `search_field(paint, placeholder, value, on_input, on_clear)`, `password_field(…, revealed, on_input, on_reveal)` | Leading search glyph and a clear button; an eye toggle. |
| `text_area_style(paint, status)` | Style for iced's `text_editor`. |
| `stepper(paint, value, previous, next, on_change)` | − value +. |
| `dropdown(paint, options, selected, placeholder, on_select)` | Opener with a chevron; the list fades and rises in, follows the keyboard (↑ ↓ Enter Esc) and closes on an outside click. |

**Selection**

| Widget | Notes |
|---|---|
| `switch(paint, on, Option<on_toggle>)`, `sized_switch(…, SwitchSize::{Panel, Bar}, …)` | 44×26 or 28×16; the thumb travels t2 E_OVER, the track colour t2 E_OUT, the thumb stretches while pressed. |
| `checkbox(paint, label, Check::{Off, On, Mixed}, Option<msg>)`, `check_box`, `radio_group`, `radio_mark` | 20 px marks; the check and dot pop with E_OVER. |
| `segmented(paint, &[(value, label)], selected, on_select)`, `tabs(…)` | The raised thumb / the accent underline slides between options. |
| `sidebar_list(paint, &[(icon, label)], selected, on_select)` | A sliding acc-soft selection with the 3×18 bar growing t3 E_OVER. |
| `chip(paint, label, on, Option<msg>)` | Toggle chip; fill and ink crossfade. |
| `breadcrumbs(paint, &[label], on_select)` | Path crumbs: hover layer on each, chevrons between, the last one semibold. |
| `page_dots(paint, count, current, on_select)` | 8 px dots; the current one stretches to a 22 px accent pill (t2 E_OUT); arrows step. |
| `swatches(paint, &[(value, colour)], selected, on_select)` | 24 px colour dots; the picked one gets an accent ring and a check with a small E_OVER pop. |
| `slider(paint, range, value, on_change)`, `stepped_slider(…, step, on_change, on_release)` | 6 px groove, 18 px thumb whose acc-ring grows on hover; drags are instant, programmatic changes glide t2 E_OUT. |
| `track(paint, Indicator, selected, row_or_column)` | The sliding-indicator primitive the above are built on. |

**Feedback**

| Widget | Notes |
|---|---|
| `progress(paint, Some(fraction))`, `progress(paint, None)` | 6 px bar gliding to its value; indeterminate is a 34 % sweep (E_IO, 1.4 s). |
| `spinner(paint, large)` | 22 / 34 px. |
| `badge(paint, label, Severity)`, `count_badge(paint, n)` | Severities `Neutral, Accent, Ok, Warn, Danger`; the counter pops in from 0.6. |
| `banner(paint, Severity, title, body, actions)`, `empty_state(paint, icon, title, body, action)` | |
| `avatar(paint, name, side)`, `keycap(paint, label)` | Gradient initial; a key label. |
| `skeleton(paint, width, height)` | Loading placeholder; fill-2 block that breathes over 1.4 s (E_IO), still with animations off. |
| `tooltip(paint, child, label)` | Fades and rises after the 550 ms delay. |

**Layout**

| Widget | Notes |
|---|---|
| `card(paint, content)`, `action_card(paint, content, msg)` | Action cards lift 3 px on hover (E_OVER). |
| `switch_row(paint, title, subtitle, on, Option<on_toggle>)` | A list row that toggles from anywhere on it, with a hover layer and the switch at its end. |
| `list(paint, rows)`, `list_row(paint, title, subtitle, trailing)`, `action_row(…, msg)` | Boxed list; action rows carry a chevron and a hover layer. |
| `expander(paint, title, open, on_toggle, content)` | Unfolds over t3 E_OUT; the chevron turns. |
| `section(paint, title, content)`, `sidebar(paint, width, items)`, `sidebar_item(…)` | |
| `Popup::new(paint, anchor, build)` | A popover anchored to any element; `.keyboard(rows, on_pick)`, `.width(w)`. Build its rows from containers and `mouse_area` rather than iced buttons, whose status resets while the popover animates. |
| `popover`, `surface`, `separator`, `vertical_separator`, `scrollable` | |
| `text(content, TextStyle)`, `measure(text, TextStyle)`, `icon(Icon, side, tint)` | The shell's type scale; widths are ceiled. |

`Paint` exposes every colour token as a method (`win_bg`, `view_bg`,
`fg_1`…`fg_4`, `fill_1`…`fill_3`, `acc_bg`, `acc_soft`, `danger`, …),
`layer(base, opacity)` for state layers and `faded(alpha)` for fading a whole
subtree.

## Motion

`glide_sdk::anim` animates anything of your own the same way:

```rust
use glide_sdk::anim::{self, Speed};

anim::animate(if open { 1.0 } else { 0.0 }, Speed::SPRING, |value| {
    let value = value.clone();
    container(content).style(move |_| rounded(mix(idle, active, value.get())))
})
.offset(|value| Vector::new(0.0, -3.0 * value));
```

- `Speed::{STATE, PRESS, COLOR, SLIDE, SPRING, EXPAND, GROW, FADE}` are the
  rows of the spec's table (duration + curve); every duration is scaled by
  the shell and `0` means instant.
- `animate(target, speed, build)` tweens towards `target`; the value reaches
  style closures through `Value::get()`, and `.scale()` / `.offset()` /
  `.transform()` move the content. `.from(start)` animates in on first show.
- `hover(speed, build)` targets 1 while the pointer is over the content.
- `collapse(open, content)` animates height; `spin(period, build)` gives a
  looping phase. `Motion` is the retargetable tween underneath, for custom
  widgets.

## Keyboard

Tab and Shift+Tab walk every enabled control in view order: buttons, icon
buttons, switches, check boxes, radios, sliders, chips, segmented controls,
tabs, sidebar rows, action rows and cards, expanders, dropdowns, menu buttons
and text fields. The focused one wears a 3 px `acc-ring` that fades in over
t1 and leaves at the next pointer press, so rings show for the keyboard only.

- Enter / Space activate buttons, switches, check boxes, radios, chips and
  rows, and open a dropdown or menu button (so does ↓); inside it ↑ ↓ Enter
  Esc work as before.
- ← → (↑ ↓) step a segmented control or tab strip; Home / End jump to the
  ends.
- A slider moves by its step (a twentieth of its range when it has none) on
  ← → ↑ ↓, five steps on Page Up / Down, to the ends on Home / End, and
  sends `on_release` after each key.

Wrap your own widget with `focusable(paint, content, radius)`, then
`.on_activate(Some(msg))` and/or `.on_key(|named| vec![…])`; `.inset()` draws
the ring inside the bounds for rows in clipped lists, `.id(id)` lets
`iced::widget::operation::focus(id)` move focus there.

## Design rules

The normative spec is `design/MERIDIAN_DESIGN_SPEC.md`. The ones apps break
most often:

- Spacing comes from `SP1`…`SP6` (4, 8, 12, 16, 24, 32), radii from
  `R1`…`R4` (4, 8, 12, 18). No values in between.
- Durations are `motion::t1()`…`t4()`, never the raw constants: the shell's
  speed setting and reduced motion scale them, and `0` must mean instant.
  Curves are `E_OUT`, `E_IO`, `E_OVER`; no springs.
- Hover is an 8 % state layer, press 13 %, drag 18 % (`motion::ST_*`).
- Only `TextStyle::MICRO` is uppercase. Size containers from measured text
  with `.ceil()`, never `as i32`, or labels end in "…".
- Ukrainian strings are the longest; check layouts in Ukrainian at the
  window's minimum size.

## System integration

`glide_sdk::system`:

| Call | What it does |
|---|---|
| `notify(Notification)` (async), `withdraw(id)` | `org.freedesktop.Notifications`; `Notification::new(app_id, summary).body(…).icon(…).urgency(…).action(key, label).reply(label, placeholder).progress(Some(40)).resident(true).transient(true).sound(name).silent(true).timeout(ms).category(…).image(path).replaces(id)`. Use `Task::perform`. |
| `notification_events()` | Stream for `Subscription::run`: `NotificationEvent::{Action { id, key }, Replied { id, text }, Closed { id, reason }}`. Notifications and their replies share one bus connection, so inline replies reach the app. |
| `show_osd(Osd::{Volume, Brightness, KeyboardBrightness, CapsLock}, level, on)`, `open_quick_settings(QuickSettings::…)` | The shell's on-screen indicator and quick settings views. |
| `Dock::new(app_id)` → `.progress(Some(0.4))`, `.count(Some(3))`, `.urgent(true)` | `com.canonical.Unity.LauncherEntry`: progress bar, count pill and attention on the dock tile. |
| `activation_token(app_id)`, `activation_env(app_id)` | A granted xdg-activation token while the app is focused, so what it launches gets focus. |
| `open(uri_or_path, app_id)`, `spawn_detached(command, app_id)` | Launch with the token; children are reaped. |
| `reveal(path, app_id)` | Show the file in Files (`org.glide.Files`, else `org.freedesktop.FileManager1`). |
| `open_terminal_tab(dir, argv, token)` | New tab in a running glide-terminal. |
| `quicklist::QuickMenu`, `search_provider::Provider` | Dock menu entries and launcher search results (`org.gnome.Shell.SearchProvider2`). |
| `frame_windows()` | Called by `run` for you: asks the compositor to frame this process's windows. |

## Testing

`Harness<A>` drives an app without a window, a bus or threads:

```rust
let mut app = Harness::<Notebook>::new("org.example.Notes", Look::default(), Size::new(900.0, 600.0));
app.message(Message::AskDelete);
assert!(app.shell().has_dialog());
app.send(glide_sdk::app::Event::Dialog(glide_sdk::overlay::Answer::Chose(0)));
```

`harness.view()` and `harness.theme()` feed `iced_test::Simulator` for
headless screenshots (`Simulator::with_size(settings, size, harness.view())`
then `.snapshot(&theme)`), which is how the example was checked in both
appearances and languages.

### Frame budget

Controls ask for frames only while something moves. `glide-catalog`'s
`tests/frames.rs` lays out every catalogue page, sends redraw events at
60 Hz and counts the frames each page asks for: every page except the one
with spinners and the indeterminate bar stops asking within t4 (most at
once; the Layout page takes 20 frames for its sidebar bar's grow-in), and
in a release build view + layout of a page costs 0.5–1.8 ms (4.8 ms for the
first, which loads the fonts). Popover content is rebuilt at most 31 times
per fade, since its cache is keyed on 30 reveal steps.

## Packaging

- Desktop file named after the app id, with `StartupWMClass=` the same id.
- Icons under `share/icons/hicolor/scalable/apps/<app-id>.svg`; the dock and
  launcher resolve them through the icon theme (Papirus by default).

## Contracts for other languages

Everything an app needs from the system is a protocol. A toolkit in another
language implements these and draws with the tokens.

| Contract | Where | Notes |
|---|---|---|
| Look | `org.glide.Shell` / `/org/glide/Shell1` / `org.glide.Shell1` properties `Appearance`, `Accent`, `Language`, `AnimationScale`, `ReducedMotion`, `FontFamily`, `IconTheme`, `Locked`, `ActiveWorkspace` | `PropertiesChanged` on change. Without the service, read `~/.config/glide/glide.toml` (`[topbar] appearance/accent/appearance-schedule`, `[general] language/font-family`, `[animations]`). |
| Tokens | `org.glide.Shell1.Tokens` (JSON string) or `glidectl tokens [light\|dark] [ACCENT]` | Schema below. Follows appearance and accent live. |
| Window frame | `org.glide.Shell1.FrameWindows()` | The compositor draws the shared shadow, 1 px ring and R3 clip around every toplevel of the calling process (matched by pid). Draw your own header, no frame. |
| Activation | `org.glide.Shell1.ActivationToken()` | Granted only to the focused client's process; pass as `XDG_ACTIVATION_TOKEN` + `DESKTOP_STARTUP_ID`. |
| Menu bar | `com.canonical.dbusmenu` at `/MenuBar` on your connection, attached with `org_kde_kwin_appmenu.set_address(unique_name, "/MenuBar")` | The manager global exists only while the global menu is on; its presence means "hide your in-window menu bar". |
| Dock badge | `com.canonical.Unity.LauncherEntry.Update("application://<id>.desktop", {progress, progress-visible, count, count-visible, urgent, quicklist})` | `quicklist` is a dbusmenu object path. |
| Search | `org.gnome.Shell.SearchProvider2` + an `.ini` in `share/gnome-shell/search-providers` | |
| Notifications, portals, MPRIS, StatusNotifierItem | freedesktop specs | Glide implements the server side. |
| Files, Terminal | `org.glide.Files` (`org.freedesktop.FileManager1` interface), `org.glide.Terminal1.OpenTab(dir, argv, token)` | |

Token sheet (`version` 1):

```json
{
  "version": 1, "appearance": "dark", "accent": "blue",
  "colors": { "win-bg": "#rrggbbaa", "fg-1": "…", "acc-bg": "…", "…": "…" },
  "light":  { "…": "the other appearance's colours" },
  "radius": { "r1": 4, "r2": 8, "r3": 12, "r4": 18 },
  "spacing": { "sp1": 4, "…": "…", "sp8": 64 },
  "type": { "body": { "size": 14, "line-height": 21, "weight": 400, "tracking-em": -0.006, "uppercase": false, "tabular": false }, "…": "…" },
  "motion": { "t1-ms": 120, "t2-ms": 200, "t3-ms": 320, "t4-ms": 420, "e-out": [0.2, 0, 0, 1], "e-io": […], "e-over": […], "hover": 0.08, "press": 0.13, "drag": 0.18 },
  "elevation": { "el1": [{ "dx": 0, "dy": 1, "blur": 2, "spread": 0, "color": "#…" }], "el2": […], "el3": […], "el-win": […] },
  "component": { "control": 34, "small-control": 28, "window-header": 48, "menu-item": 34, "tooltip-delay-ms": 550, "…": "…" }
}
```

The key under which the other palette sits is that appearance's name
(`"light"` in a dark sheet, `"dark"` in a light one). Multiply durations by
`AnimationScale`.

## Limitations

- One window per app; no multi-window API yet.
- Dialog bodies are text; richer modals are built by hand in `view`.
- A focused control is not scrolled into view yet, and a radio group is one
  Tab stop per option rather than one stop with arrows.
- iced cannot fade a whole subtree, so popovers fade by repainting their
  rows with a faded `Paint`; arbitrary iced buttons placed in a `Popup` look
  disabled while it animates (see `Popup` above).
- No IME, no drag-and-drop helpers (Files' `dnd.rs` shows the protocol), no
  printing.
- Checked headlessly (`Harness` + `iced_test`, both appearances, Ukrainian,
  hover, open menus and dropdowns, keyboard rows, focus rings) and the examples were
  started in a live Glide session; the motion itself is only visible live.

## A Go SDK: how hard would it be

Not written yet. The assessment, by layer:

**Integration layer — easy to moderate, 1–2 weeks.** Everything in the
contracts table is D-Bus or JSON. With `github.com/godbus/dbus/v5`:
following `org.glide.Shell1` (look + tokens), `FrameWindows`,
`ActivationToken`, Notifications, LauncherEntry, SearchProvider2 and the
file-chooser portal are a few hundred lines each at most. The largest piece
is a `com.canonical.dbusmenu` server for the menu bar and quicklists
(`crates/glide-app/src/menubar.rs` is 447 lines of Rust; expect the same in
Go).

**Wayland glue — moderate, a few days, needs cgo.** Two requests must be made
on the app's own `wl_surface`: `org_kde_kwin_appmenu.set_address` (menu bar)
and `xdg_activation_v1.activate` (raising itself with a token). A pure-Go
Wayland client cannot share the toolkit's `wl_display`, so this is a small C
shim over `libwayland-client` with its own event queue on the foreign display
(what `glide-app` does with `Backend::from_foreign_display`), fed the display
and surface pointers the toolkit exposes (Gio: `app.WaylandViewEvent`).

**Drawing layer — the hard part, 4–8 weeks to reach today's `glide-sdk`.**
Go has no iced. The realistic base is **Gio** (`gioui.org`): native Wayland,
undecorated windows with move/resize actions, immediate-mode drawing that can
reproduce the chrome. Work to do, roughly the size of `glide-iced` +
`glide-sdk`'s chrome and widgets (5–8 k lines of Go):

- A `theme` package generated from the token sheet (`go:generate` running
  `glidectl tokens`), live-updated from the bus.
- Window chrome (header, controls, resize zones, rounded clip), menu bar,
  popup menus with keyboard navigation and positioning, dialogs, toasts,
  tooltips — Gio has no popup/menu or dropdown primitives, these are custom.
- The widget set: tones, fields with the focus halo, switch, checkbox,
  segmented, slider, dropdown, boxed lists.
- Elevation: Gio has no blur, so shadows are approximated with stacked
  translucent rounded rects or pre-rendered nine-slice images.
- Typography: the type scale needs exact weights (450, 610, 630…) and
  letter-spacing; Gio's shaper (`go-text/typesetting`) handles variable
  weights, letter-spacing support must be checked, and glyphs will not be
  pixel-identical to cosmic-text's.
- Motion: tweens on Gio's frame invalidation, with the same scaled durations
  and cubic curves.

**Alternatives considered and rejected:**

- *Bindings to the Rust SDK over a C ABI.* iced's model is generic messages
  and Rust closures in a typed widget tree; exposing it to Go means inventing
  a declarative UI protocol (Go sends a tree, Rust renders and sends events
  back). That is a second toolkit with worse ergonomics and two runtimes in
  one process.
- *Fyne with a custom theme.* Colours and sizes map in an afternoon, but
  Fyne draws its own chrome, cannot export menus to the top bar and its
  widgets do not follow Meridian's shapes and motion. It gives
  "Glide-coloured", not "Glide-native".

**Verdict: moderate-to-hard overall — about 6–10 weeks for one experienced
developer to reach parity, 3–4 weeks for a usable MVP** (tokens, look
following, chrome, menu bar, eight core widgets, dialogs). The design choice
that keeps it bounded is already made on the Rust side: every integration
point is a protocol (D-Bus, Wayland, the JSON token sheet, frame by pid
instead of an app-id list), so a Go SDK only reimplements drawing, never the
shell.
