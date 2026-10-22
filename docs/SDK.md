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
notifications and a dock badge:

```
cargo run -p glide-sdk --example notes
```

## Contents

1. [Quick start](#quick-start)
2. [The `App` trait](#the-app-trait)
3. [The `Shell` handle](#the-shell-handle)
4. [Menus and shortcuts](#menus-and-shortcuts)
5. [Dialogs, toasts and context menus](#dialogs-toasts-and-context-menus)
6. [Widgets](#widgets)
7. [Design rules](#design-rules)
8. [System integration](#system-integration)
9. [Testing](#testing)
10. [Packaging](#packaging)
11. [Contracts for other languages](#contracts-for-other-languages)
12. [Limitations](#limitations)
13. [A Go SDK: how hard would it be](#a-go-sdk-how-hard-would-it-be)

## Quick start

```toml
[dependencies]
glide-sdk = { git = "https://github.com/artem/glide" }
```

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
| `toast(text)` | Bottom pill for 4 s. |
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
  focused text field consumes are left to it.

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

All live in `glide_sdk::widget` and take `&Paint` first.

| Widget | Notes |
|---|---|
| `button(paint, label, tone, Option<msg>)`, `primary`, `secondary`, `ghost`, `danger` | 34 px; `Tone::{Accent, Neutral, Ghost, Danger}`; press shrinks to .975, state layers crossfade over t1. |
| `compact_button` | 28 px, for bars and dense rows. |
| `icon_button(paint, Icon, tooltip, Option<msg>, on)` | 28 px square with a tooltip. |
| `text_field(paint, placeholder, value, on_input)`, `field(paint, text_input)` | Fill-1 well, accent border and 3 px halo when focused. `field` wraps any `text_input` (ids, submit, secure). |
| `switch(paint, on, Option<on_toggle>)` | Spec switch with the token thumb. |
| `checkbox(paint, label, checked, Option<on_toggle>)` | |
| `segmented(paint, &[(value, label)], selected, on_select)` | Raised thumb with El1. |
| `slider(paint, range, value, on_change)` | 6 px groove, accent ring on hover and drag. |
| `progress(paint, fraction)` | 6 px R1 bar. |
| `dropdown(paint, options, selected, on_select)` | Pick list with an R3 popover. |
| `list(paint, rows)`, `list_row(paint, title, subtitle, trailing)` | Boxed list (R3 card, hairlines between rows). |
| `section(paint, title, content)` | MICRO caps heading over content. |
| `card`, `popover`, `surface` | Surfaces with the right fill, ring and elevation. |
| `sidebar(paint, width, items)`, `sidebar_item(paint, icon, label, selected, msg)` | |
| `badge(paint, label, tone)`, `empty_state(paint, icon, title, body)`, `tooltip(paint, child, label)` | |
| `separator`, `vertical_separator` | Opaque hairlines. |
| `scrollable(paint, content)` | Thin scrollbar that darkens on hover. |
| `text(content, TextStyle)` | The shell's type scale (`TextStyle::T1`, `BODY`, `CALL`, `CAP`, `MICRO`, …). |
| `measure(text, TextStyle)` | Width in px, ceiled. |

`Paint` exposes every colour token as a method (`win_bg`, `view_bg`,
`fg_1`…`fg_4`, `fill_1`…`fill_3`, `acc_bg`, `acc_soft`, `danger`, …),
`layer(base, opacity)` for state layers and `faded(alpha)` for fading a whole
subtree.

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
| `notify(Notification)` (async), `withdraw(id)` | `org.freedesktop.Notifications`; `Notification::new(app_id, summary).body(…).icon(…).urgency(…).action(key, label)`. Use `Task::perform`. |
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
- Notification action replies are not delivered back yet (`ActionInvoked`
  is not subscribed).
- No IME, no drag-and-drop helpers (Files' `dnd.rs` shows the protocol), no
  printing.
- The live session has not been exercised with a third-party app yet: the
  chrome, menus, overlays and example were checked headlessly through
  `Harness` + `iced_test`, the bus contracts against a private
  `dbus-daemon`.

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
