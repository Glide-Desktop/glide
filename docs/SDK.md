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
8. [Languages](#languages)
9. [Keyboard](#keyboard)
10. [Design rules](#design-rules)
11. [System integration](#system-integration)
12. [Testing](#testing)
13. [Packaging](#packaging)
14. [Contracts for other languages](#contracts-for-other-languages)
15. [Limitations](#limitations)
16. [A Go SDK: how hard would it be](#a-go-sdk-how-hard-would-it-be)

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
| `close_requested(shell)` | no | The window's close button or the compositor asked to close. The default calls `shell.exit()`; override it to ask about unsaved changes first, returning whatever task that needs, and call `shell.exit()` (or return `iced::exit()`) once it may close. |
| `key(event, captured, shell)` | no | Every keyboard event the host did not use itself (dialogs, popups, F10, Tab focus and menu shortcuts with the standard chrome), with `captured` telling whether a focused widget already consumed it. Apps that bind keys themselves handle them here instead of subscribing to the keyboard. |
| `window_changed(shell)` | no | Runs after the window was resized, focused or unfocused, maximized, made fullscreen or restored; `shell.window()` holds the new state. Apps with their own chrome copy it here instead of subscribing to window events, and send window commands (`window::Action::Drag`, `ToggleMaximize`, `ToggleFullscreen`, `Minimize`, `Activate`, `Attention`, `Resize`) through `shell.window_action`. |
| `paint(shell)` | no | The colours the window's iced theme and background follow; defaults to `shell.paint()`. |
| `look_changed(shell)` | no | Runs after the host applied a new look — a Shell1 property, reduced motion, a `glide.toml` edit or the scheduled appearance turning — with `shell.look()` and `shell.paint()` already updated. Apps that cache colours, icons or translated names refresh them here instead of watching the look themselves. |
| `reopened(args, shell)` | no | Only with `Settings::single_instance`: another launch of the app was forwarded here with its command-line arguments. The host has already raised the window with the launch's activation token; the app shows what `args` asks for (a page, a file). |

`Settings::new(app_id)` takes the Wayland app id, which must equal the
desktop file name (`org.example.Notes` ↔ `org.example.Notes.desktop`): the
dock, the launcher, quicklists and the LauncherEntry badge all key on it.
`.size(w, h)`, `.min_size(w, h)`, `.font(bytes)` / `.fonts(list)` and
`.chrome(Chrome)` complete it. `Chrome::Standard` draws the header, menu bar
and resize edges around `view`; `Chrome::Own` hands the whole window to
`view` (draw it with `window::header`, `window::resize_edges` and the rest
yourself) and leaves every key and the close request to the app;
`Chrome::Server` asks the compositor for its own frame. Glide's Files,
Terminal and Text run with `Own`, Settings with `Server`.

`.single_instance(bus_name)` keeps one window per session. Before iced
starts, `run` claims `bus_name` on the session bus (`DoNotQueue`); when
another process holds it, the launch calls
`org.glide.Instance1.Open(args, activation_token)` on that process and exits
without opening a window. The running app gets the call as
`App::reopened(args, shell)` after its window was activated with the token
(`XDG_ACTIVATION_TOKEN`, else `DESKTOP_STARTUP_ID`). Without a session bus the
app simply runs on its own. `glide_sdk::instance` exposes the same pieces
(`claim_on`, `requests`, `launch_token`, `Claim::{First, Forwarded, Alone}`)
for apps that need them outside `run`. Settings uses
`org.glide.Settings`.

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
| `sized_icon_button(…, Size)`, `icon_segmented(paint, &[(value, Icon, tooltip)], selected, on_select)` | Icon buttons at any size; a segmented control of icons (view switchers). |
| `linked(paint, &[(label, Option<msg>)])` | Joined group, R2 outside and R1 inside. |
| `toggle_button(paint, label, on, Option<on_toggle>)`, `sized_toggle_button(…, Size)`, `flat_toggle_button(…)` | Neutral (or ghost, for flat) when off, tonal when on; fill and ink crossfade over t2; one- or two-letter labels make a square button (Aa, W, .*). |
| `link(paint, label, Option<msg>)` | Accent text button with a ghost hover layer. |
| `menu_button(paint, label, items, width)` | Opens a menu of actions under itself (keyboard rows, Esc, outside click). |
| `Size::width(label)` | Measured width of a button of that size with that label, for layouts that must fit buttons in. |

**Inputs**

| Widget | Notes |
|---|---|
| `text_field(paint, placeholder, value, on_input)`, `field(paint, text_input)` | Fill-1 well, accent border and a 3 px halo that fades in on focus. `field` wraps any `text_input` (ids, submit, secure). |
| `validated_field(paint, text_input, Validity::{Neutral, Valid, Invalid})` | OK / danger border and ring. |
| `search_field(paint, placeholder, value, on_input, on_clear)`, `password_field(…, revealed, on_input, on_reveal)` | Leading search glyph and a clear button; an eye toggle. |
| `search_input(paint, text_input, Option<on_clear>, clear_label)`, `hinted_search_input(…, Option<hint>)` | The same well around your own `text_input` (ids, submit); the hint (e.g. a `key_combo`) shows while there is nothing to clear. |
| `inline_field(paint, text_input, TextStyle)`, `inline_style` | A compact view-bg field with a line-2 border for editing in place (rename in a grid or list). |
| `text_area_style(paint, status)`, `bare_text_area_style` | Style for iced's `text_editor`; the bare one has no well (editors). |
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
| `Crumb::new(paint, label)` + `crumb_trail(paint, crumbs)` | Crumbs with `.glyph(glyph, tint)`, `.current(bool)`, `.hovered(bool)` (drop targets) and `.on_press(Option<msg>)`; wrap each in a `mouse_area` for context menus or drop hover. |
| `page_dots(paint, count, current, on_select)` | 8 px dots; the current one stretches to a 22 px accent pill (t2 E_OUT); arrows step. |
| `swatches(paint, &[(value, colour)], selected, on_select)`, `swatch(paint, colour, selected, Option<msg>, side)` | Colour dots; the picked one gets an accent ring and a check with a small E_OVER pop. |
| `slider(paint, range, value, on_change)`, `stepped_slider(…, step, on_change, on_release)` | 6 px groove, 18 px thumb whose acc-ring grows on hover; drags are instant, programmatic changes glide t2 E_OUT. |
| `track(paint, Indicator, selected, row_or_column)` | The sliding-indicator primitive the above are built on. |

**Feedback**

| Widget | Notes |
|---|---|
| `progress(paint, Some(fraction))`, `progress(paint, None)`, `paused_progress(paint, fraction, paused)` | 6 px bar gliding to its value; indeterminate is a 34 % sweep (E_IO, 1.4 s); paused turns fg-3 and stops. |
| `spinner(paint, large)`, `sized_spinner(paint, side)`, `busy_label(paint, label)` | 22 / 34 px or any size; a small spinner with a caption. |
| `badge(paint, label, Severity)`, `status_badge(…)`, `count_badge(paint, n)`, `dot(colour, side)` | Severities `Neutral, Accent, Ok, Warn, Danger`; the status badge leads with a dot; the counter pops in from 0.6. |
| `banner(paint, Severity, title, body, actions)`, `noted_banner(paint, Severity, glyph, title, body, actions)` | The second picks its own glyph; an empty title leaves one line. |
| `empty_state(paint, glyph, title, body, action)`, `empty_note(paint, glyph, title, body)` | The first fills and centres in its area; the note is compact, for lists and scrolled pages. |
| `avatar(paint, name, side)`, `initials(name)`, `keycap(paint, label)`, `key_combo(paint, "Ctrl+F", symbol)` | Gradient initials (first and last word); key labels; a combo of keycaps through your symbol mapping. |
| `icon_tile(glyph, colour, side)` | A coloured app tile with a white glyph, R2 / R3 / R4 by size. |
| `task_card(paint, glyph, Task { title, current, fraction, paused, detail, actions })` | A job card: title and buttons, current item, progress, detail line. |
| `skeleton(paint, width, height)` | Loading placeholder; fill-2 block that breathes over 1.4 s (E_IO), still with animations off. |
| `tooltip(paint, child, label)`, `bubble(paint, content)` | Fades and rises after the 550 ms delay; the bubble is the same surface drawn at once (drag hints, labels over images). |

**Layout**

| Widget | Notes |
|---|---|
| `card(paint, content)`, `action_card(paint, content, msg)`, `card_style`, `tinted_card_style(paint, tint)` | Action cards lift 3 px on hover (E_OVER); the tinted card washes its top in a colour. |
| `Group::new(paint)` | Boxed-list section: `.title`, `.description`, `.suffix`, `.push` / `.push_maybe` / `.extend` rows, `.build()`. |
| `ListRow::new(paint, title)` | 44 px row around 34 px controls: `.subtitle`, `.prefix`, `.suffix`, `.on_press` (R2 hover layer, keyboard focus), `.dimmed`, `.stacked(bool)` (suffix below the label on narrow windows), `.changed(bool)` (an accent dot before the title: the value differs from its default). |
| `list(paint, rows)`, `list_row(paint, title, subtitle, trailing)`, `action_row(…, msg)`, `switch_row(…, on, Option<on_toggle>)` | Shortcuts built on the two above; switch rows toggle from anywhere on them. |
| `facts(paint, rows, label_width)`, `well(paint, content)`, `sample_well(paint, content)`, `select_row(paint, leading, label, selected, msg)` | Key/value lines; a fill-1 well; a sunken sample area; a pickable row with a check. |
| `expander(paint, title, open, on_toggle, content)`, `disclosure(paint, value)`, `inert(paint, enabled, content)` | Unfolds over t3 E_OUT; a value with a chevron; a veil over controls that cannot be used. |
| `section(paint, title, content)`, `sidebar(paint, width, items)`, `sidebar_item(…)` | |
| `SidebarItem::new(paint, label)` | `.glyph` / `.tinted` / `.leading(element)`, `.ink`, `.matched(range)` (search hit), `.accessory` (eject button, status), `.selected`, `.current` (bar grows t3 E_OVER), `.hovered`, `.held`, `.on_press`. |
| `profile_card(paint, avatar, name, subtitle, selected, msg)` | The account card at the top of a sidebar. |
| `tree_row(paint, depth, expanded, glyph, label, current, msg)`, `TREE_ROW`, `TREE_INDENT` | Folder-tree rows with a turning chevron. |
| `table_header(paint, cells)` + `HeaderCell::new(paint, label)` | MICRO column headers: `.width`, `.separated`, `.sorted(Some(ascending))` or `.sort_mark(glyph)`, `.lit`, `.target` (drag), `.on_press`. |
| `status_bar(paint, left, right, corners)`, `status_button(paint, label, msg)` | 26 px bar along the window's bottom. |
| `stat_tile`, `grid(cells, per_line, spacing)`, `usage_bar`, `legend`, `usage_card` | Figures with a tile; a wrapping grid; segmented usage bars with their legend. |
| `preview_card(paint, content, label, selected, Option<msg>)`, `preview_ring_style`, `choice_card(paint, illustration, heading, subtitle, selected, msg)` | Thumbnails with a selection ring that never changes width; a card with a radio mark. |
| `hero_card(paint, glyph, colour, heading, description, trailing, stacked)`, `page_header(paint, glyph, colour, eyebrow, heading, description, trailing)` | A page's lead card and its title block. |
| `Popup::new(paint, anchor, build)` | A popover anchored to any element; `.keyboard(rows, on_pick)`, `.width(w)`. Build its rows from containers and `mouse_area` rather than iced buttons, whose status resets while the popover animates. |
| `popover`, `surface`, `separator`, `vertical_separator`, `scrollable` | |
| `text(content, TextStyle)`, `measure(text, TextStyle)`, `icon(glyph, side, tint)` | The shell's type scale; widths are ceiled. `icon` takes anything implementing `Glyph`: the SDK's `Icon`, or an app's own icon set with a one-method impl. |
| `elide(name, width, TextStyle)`, `elide_end(sentence, width, TextStyle)`, `ellipsize(name, chars)` | Measured shortening: in the middle keeping the extension, or at the end. |
| `elide_lines(name, width, TextStyle, lines)`, `measure_lines`, `breakable(name)` | Shorten a name to fit a number of wrapped lines, preferring to break before its extension. |
| `pane(background, corners, content)`, `heading_row(paint, title, actions)`, `section_title(paint, title, highlighted)` | A filled, rounded area that fills its parent (chain `.width`/`.height`/`.padding`); a MICRO heading with buttons; a sidebar section title that lights up as a drop target. |
| `sheet(paint, rows)`, `key_values(paint, rows)`, `stat_rows(paint, rows)`, `form_row(paint, label, hint, control)`, `hint_line(paint, glyph, text)`, `action_bar(buttons)`, `boxed_input(paint, placeholder, value, Validity)` | Building blocks of detail sheets and forms inside a `Group`. |
| `tinted_card(paint, tint, padding, content)`, `letter_tile(paint, name, side)` | A hero card with your own layout; an initial on a fill-2 tile for things without an icon. |
| `item_tile(paint, art, badge, label, plate, size, selected, (hover, lift))`, `corner_badge`, `table_row(paint, cells, selected, stripe, hover, height)`, `item_ink`, `selection_band(paint, rect)` | File-manager style grid tiles and list rows driven by your own hover state, and a rubber band. |
| `Art`, `art(art, side, opacity)`, `photo(path, side, radius)`, `photo_button`, `picture(handle, w, h, fit, radius)`, `svg_glyph(handle, side, tint)` | Pictures and icon sets from files or handles; `svg_glyph` is the one-line body of an app's `Glyph` impl. |
| `level_meter(paint, level, peak, Severity)`, `level_control(paint, glyph, mute, slider, level, muted)`, `slider_readout(paint, leading, slider, readout, dimmed)`, `range_slider(paint, low, slider, high)`, `frame_button(paint, content, selected, msg)` | Meters and labelled sliders; a picture button with the selection ring. |
| `editor(paint, content)`, `text_area(paint, content)`, `markdown_settings(paint, size, mono)`, `markdown_view(paint, items, settings, viewer)`, `code_block(paint, content, padding)`, `shortcut_list(paint, sections)`, `scrollable_x` | A full-window editor, a bordered text area, a Markdown preview with its code blocks, a shortcut reference, a horizontal scroller. |
| `split_view(paint, state, view)`, `palette_swatch(paint, background, foreground, accents, font, size)`, `context_area(content, on_right_press)`, `reveal(id, top, bottom)` | A Meridian pane grid; a colour-scheme sample; right-click with position and Ctrl+wheel zoom; scroll a range into view. |
| `mock::*` | Illustration primitives for previews: `screen`, `topbar`, `bar`, `window`, `titled_window`, `dock`, `mini_dock`, `wallpaper`, `monitor`, `battery`, `touchpad`, `mouse`, `document`, `schedule_bar`, `glyph_well`, `sample_area`, `click_target`, `dots`, `ring_dot`, `gradient_strip`, `block`, `outlined`, `surface`, `outlined_surface`, `at`, `centered`, `bottom`, `caption`; whole scenes: `desktop(paint, height)` (bar, two windows and a dock over the accent gradient), `desktop_over(paint, screen, w, h)` (bar and dock over your own picture), `lock_screen`, `theme_sample(paint, w, h)` and `split_theme_sample(light, dark, w, h)` (an app window in one look, or half in each), `dock_scene(paint, tiles_with_lift, bottom_gap, h)`, `bar_scene(paint, left, center, right, h)`, `window_on_screen(paint, window, h)`, `dock_hiding(paint, DockHiding)`, `new_window_placement(paint, own_workspace)`, `hot_corner(paint)`, `snap_zone(paint, left, width, on)`, `touchpad_gesture(paint, TouchpadGesture)`, `finger_pad(paint, w, h, FingerPad::{Idle, Live})` (a practice pad with live fingers), `gesture_hint(paint, fingers, before, after, vertical)` and `scroll_sample(paint, lines, h)`. |
| `captioned_picture(paint, handle, height, radius, Option<caption>)` | A cover-fit tile with a bubble label in its corner. |
| `cells::*` | Drawing a monospace character grid inside a custom widget's `draw`: `Metrics::measure(font, size, line_height)` (cell size, baseline, `grid(bounds)`, `locate(point, grid)`), `Painter` (`fill` merges backgrounds into runs that bleed to the edges and corners, `glyph` batches ASCII runs, `text` for other glyphs, `symbol` draws box-drawing and block elements as quads instead of font glyphs, `underline` in five styles, `strike`, `caret` (beam, underline, block; hollow when unfocused), `gutter` and `band` marks, `scrollbar`, `badges` for labels on their own layer) and `thumb` for the scrollbar's hit area. Glide Terminal draws its panes with it. |

**Status and summaries**

| Widget | Notes |
|---|---|
| `Issue::new(paint, title)` → `.summary`, `.details(lines, open)`, `.toggle(msg, more, less)`, `.action`, `.severity` | A problem row: a plain-language summary, technical lines folded behind a link, an action button. |
| `Glance::new(paint, glyph, colour, title)` → `.headline`, `.detail`, `.meter(fraction, colour)`, `.aside`, `.on_press` | A summary card (network, storage, battery) that lifts like an action card when it has a message. |
| `fact_grid(paint, facts, per_line)` | Label-over-value cells in a card, `per_line` to a line. |
| `stage_track(paint, &[Stage::new(at, label, colour)])` | Points along a timeline (dims → locks → sleeps); stages may arrive in any order. |
| `choice_tiles(paint, options, selected, on_select)` | Equal tiles with a title and a hint; the chosen one has a 2 px accent border. |
| `status_chips(paint, items)`, `signal_bars(paint, level, steps)` | Fill-1 chips led by a coloured dot; rising bars. |
| `info_button(paint, open, msg)`, `INFO_BUTTON` | A 22 px round "i" that turns acc-soft while its help is open. |
| `reset_action(paint, note, label, Option<msg>)`, `applies_chip(paint, label, notable)` | "N changed · Reset" for a section heading (`Group::suffix`); a CAP chip saying when a change takes effect, warn-tinted when it needs a restart. |
| `section_heading(paint, title)` | A HEAD heading aligned with group titles, for sections that are not a `Group`. |
| `labelled_switch(paint, label, on, Option<on_toggle>)`, `i18n::ago(seconds, lang)` | A CAP label beside a small switch (two per row: Banners / Sound); "5 min ago" / "5 хв тому" with plural days. |

**Bars and strips**

| Widget | Notes |
|---|---|
| `tab_strip(paint, tabs, active)` → `.height`, `.hovered`, `.dragged`, `.on_select`, `.on_close`; `Tab`, `Mark::{None, Dot, Alert}`, `tab_slot` | Document tabs with close buttons on hover, marks, and the slot a drag lands in. |
| `find_bar(paint, text_input)` → `.status`, `.steps`, `.toggle`, `.extra`, `.done`, `.replace`, `.narrow`, `.height` | A find (and replace) bar. |
| `bar_strip(paint, Option<Severity>, content)`, `info_bar(paint, Option<Severity>, title, detail, actions)`, `INFO_BAR` | A strip under the header; the info bar is a 36 px title, detail and actions. |

**Window**

| Item | Notes |
|---|---|
| `window::Header::new(title)` + `window::header(paint, window, header, on_window, on_close, room_taken)` | The client-side header: `.subtitle`, `.leading`, `.trailing`, `.center`, `.labels`, `.height`, `.corners`, `.context`, `.without_maximize`, `.without_close` (dialog headers: Cancel · title · Accept). |
| `window::maximize_button`, `window::close_button` | The header's controls on their own, for split headers. |
| `window::resize_edges(on_resize)`, `window::resize_handles(on_window)`, `window::Edge` | The 5 px edges and 10 px corners; `Edge` wraps a direction so messages can derive `PartialEq`. |

**Menus and overlays**

| Item | Notes |
|---|---|
| `menu::Item` (`Action`, `Submenu`, `Stepper`, `Heading`, `Separator`) and `menu::Popup` | Action rows carry a shortcut, a check, a swatch, a danger tint or a submenu arrow; `place` keeps bar menus below and shifts context menus to fit; `walk` moves the keyboard highlight. |
| `overlay::modal`, `dialog_card`, `dialog_buttons`, `ringed` | Build dialogs with your own body; danger and ghost choices sit on the left, Enter takes the accent one. |
| `overlay::toast_pill`, `overlay::toast_card(paint, message, ink, action, close, max_width)`, `overlay::toast_motion(card, progress, entering)` | The bottom toast, or just its card and its rise / shrink for apps that place it themselves. |
| `window::Side`, `window::side_corners(side, radius)` | Round only the window corners a pane touches. |
| `anim::Fade`, `anim::Reveal`, `system::launch_token`, `system::launch_env` | A timed 0→1 fade with separate enter and leave speeds; the popup reveal; an activation token for programs the app starts. |

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

## Languages

Glide speaks Ukrainian and English. `shell.lang()` is the language the user
chose in `glide.toml` (or the shell), else the locale. Keep every string in
one catalogue per module:

```rust
glide_sdk::catalogue! {
    Save => "Save", "Зберегти";
    Uppercase => "UPPERCASE", "ВЕЛИКІ ЛІТЕРИ" as written;
}

let label = Key::Save.text(shell.lang());
```

The macro makes `Key` with `ALL`, `en()`, `uk()`, `text(lang)` and the
`i18n::Text` trait, and a test that fails on an empty translation or a label
written in capitals (`as written` exempts a key whose capitals are its
meaning). `i18n::count(n, lang, [one, few, many])` writes a count with the
right plural form ("1 файл", "3 файли", "11 файлів"); `Lang::plural` gives the
form alone. `Lang::from_env()` takes the first set locale variable, and
`Lang::from_env_or(fallback)` the first one naming Ukrainian or English.

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
