use glide_files::app::{Message, Step, ViewMode, handle_shortcut, window_events};
use glide_files::config::Look;
use glide_files::i18n::{self, Key, Lang};
use glide_files::theme::ThemeState;
use glide_files::widgets::content::{GRID_PADDING, Metrics, TILE_GAP};
use glide_files::widgets::{ellipsize, menu};
use glide_theme::{Accent, Appearance};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Event, Key as IcedKey, Location, Modifiers};
use iced::{Point, Size};

fn press(key: IcedKey, modifiers: Modifiers, text: Option<&str>) -> Option<Message> {
    handle_shortcut(Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::KeyA),
        location: Location::Standard,
        modifiers,
        repeat: false,
        text: text.map(Into::into),
    })
}

fn named(key: Named, modifiers: Modifiers) -> Option<Message> {
    press(IcedKey::Named(key), modifiers, None)
}

fn chord(c: &str, modifiers: Modifiers) -> Option<Message> {
    press(IcedKey::Character(c.into()), modifiers, Some(c))
}

#[test]
fn keyboard_shortcuts_map_to_intended_messages() {
    let none = Modifiers::empty();
    let ctrl = Modifiers::COMMAND;
    let alt = Modifiers::ALT;
    let shift = Modifiers::SHIFT;

    let cases = [
        (named(Named::Backspace, none), Message::Back),
        (named(Named::ArrowLeft, alt), Message::Back),
        (named(Named::ArrowRight, alt), Message::Forward),
        (named(Named::ArrowUp, alt), Message::Up),
        (named(Named::Home, alt), Message::GoHome),
        (named(Named::ArrowDown, alt), Message::OpenSelection),
        (named(Named::ArrowDown, none), Message::Move(Step::Down, false)),
        (named(Named::ArrowUp, shift), Message::Move(Step::Up, true)),
        (named(Named::Home, none), Message::Move(Step::First, false)),
        (named(Named::End, none), Message::Move(Step::Last, false)),
        (named(Named::Enter, none), Message::OpenSelection),
        (named(Named::Enter, alt), Message::Properties),
        (named(Named::F2, none), Message::Rename),
        (named(Named::F5, none), Message::Reload),
        (named(Named::Delete, none), Message::Trash),
        (named(Named::Delete, shift), Message::Delete),
        (chord("r", ctrl), Message::Reload),
        (chord("h", ctrl), Message::ToggleHidden),
        (chord("l", ctrl), Message::EditPath),
        (chord("f", ctrl), Message::ToggleSearch),
        (chord("a", ctrl), Message::SelectAll),
        (chord("c", ctrl), Message::Copy),
        (chord("x", ctrl), Message::Cut),
        (chord("v", ctrl), Message::Paste),
        (chord("i", ctrl), Message::Properties),
        (chord("N", ctrl | shift), Message::NewFolder),
        (chord("1", ctrl), Message::SetView(ViewMode::Grid)),
        (chord("2", ctrl), Message::SetView(ViewMode::List)),
        (chord("w", ctrl), Message::Quit),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual, Some(expected.clone()), "{expected:?}");
    }
    assert_eq!(chord("n", ctrl), None);
}

#[test]
fn shortcuts_follow_the_physical_key_on_cyrillic_layouts() {
    let typed = |c: &str, code: Code, modifiers: Modifiers| {
        handle_shortcut(Event::KeyPressed {
            key: IcedKey::Character(c.into()),
            modified_key: IcedKey::Character(c.into()),
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers,
            repeat: false,
            text: Some(c.into()),
        })
    };
    assert_eq!(typed("с", Code::KeyC, Modifiers::COMMAND), Some(Message::Copy));
    assert_eq!(typed("м", Code::KeyV, Modifiers::COMMAND), Some(Message::Paste));
    assert_eq!(typed("Т", Code::KeyN, Modifiers::COMMAND | Modifiers::SHIFT), Some(Message::NewFolder));
    assert_eq!(typed("й", Code::KeyQ, Modifiers::COMMAND), Some(Message::Quit));
    assert_eq!(typed("ф", Code::KeyA, Modifiers::empty()), Some(Message::TypeToSearch(String::from("ф"))));
}

#[test]
fn the_menu_key_and_shift_f10_open_the_context_menu_and_shortcuts_close_it() {
    assert_eq!(named(Named::ContextMenu, Modifiers::empty()), Some(Message::KeyMenu));
    assert_eq!(named(Named::F10, Modifiers::SHIFT), Some(Message::KeyMenu));
    assert_eq!(named(Named::F10, Modifiers::empty()), None);
    for closing in [Message::Trash, Message::Copy, Message::Back, Message::TypeToSearch(String::from("a"))] {
        assert!(closing.dismisses_menu(), "{closing:?}");
    }
    for kept in [Message::Move(Step::Down, false), Message::OpenSelection, Message::Escape, Message::KeyMenu] {
        assert!(!kept.dismisses_menu(), "{kept:?}");
    }
}

#[test]
fn keyboard_highlight_walks_enabled_menu_rows_and_wraps() {
    use glide_files::app::{Menu, MenuKind};
    use menu::{Entry, next_highlight};
    let item = |enabled: bool| {
        Entry::Item(menu::Item {
            label: "row".into(),
            message: enabled.then_some(Message::SelectAll),
            shortcut: None,
            submenu: false,
            danger: false,
            checked: None,
            swatch: None,
        })
    };
    let entries = [Entry::Header("h".into()), item(true), Entry::Separator, item(false), item(true)];
    assert_eq!(next_highlight(&entries, None, Step::Down), Some(1));
    assert_eq!(next_highlight(&entries, Some(1), Step::Down), Some(4), "skips separators and disabled rows");
    assert_eq!(next_highlight(&entries, Some(4), Step::Down), Some(1), "wraps");
    assert_eq!(next_highlight(&entries, None, Step::Up), Some(4));
    assert_eq!(next_highlight(&entries, Some(1), Step::Up), Some(4));
    assert_eq!(next_highlight(&entries, Some(4), Step::Left), Some(4));
    assert_eq!(next_highlight(&[Entry::Separator], None, Step::Down), None);
    let menu =
        Menu { kind: MenuKind::Options, at: Point::ORIGIN, offers: Vec::new(), open_with: false, highlight: None };
    assert_eq!(menu.highlight, None);
}

fn path(index: usize) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/nowhere/{index}.txt"))
}

fn listed_app() -> glide_files::App {
    use glide_files::fs::{FileCategory, FileEntry};
    let (mut app, _) = glide_files::App::new();
    let empty = std::env::temp_dir().join(format!("glide-files-tests-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("scratch");
    app.current = empty;
    app.view = ViewMode::List;
    app.entries = (0..6)
        .map(|index| FileEntry {
            name: format!("{index}.txt"),
            path: path(index),
            is_dir: false,
            is_symlink: false,
            size: 0,
            modified: None,
            mode: 0o644,
            category: FileCategory::Text,
            created: None,
            tags: Vec::new(),
        })
        .collect();
    app
}

#[test]
fn escape_unwinds_one_layer_at_a_time() {
    let mut app = listed_app();
    let _ = app.update(Message::Pressed(path(1)));
    let _ = app.update(Message::Released);
    let _ = app.update(Message::KeyMenu);
    assert!(app.menu.as_ref().is_some_and(|menu| menu.highlight.is_some()), "the keyboard menu starts highlighted");
    let _ = app.update(Message::Move(Step::Down, false));
    assert_eq!(app.selection.len(), 1, "arrows walk the menu, not the files");
    let _ = app.update(Message::Escape);
    assert!(app.menu.is_none());
    assert_eq!(app.selection.len(), 1, "the first Escape only closes the menu");
    let _ = app.update(Message::Escape);
    assert!(app.selection.is_empty());
}

#[test]
fn escape_closes_the_search_the_path_field_and_a_dialog_in_one_press() {
    let mut app = listed_app();
    let _ = app.update(Message::TypeToSearch(String::from("o")));
    assert!(app.search.is_some());
    let _ = app.update(Message::Escape);
    assert!(app.search.is_none(), "one Escape leaves the search");
    let _ = app.update(Message::EditPath);
    assert!(app.path_edit.is_some());
    let _ = app.update(Message::Escape);
    assert!(app.path_edit.is_none(), "one Escape leaves the path field");
    let _ = app.update(Message::NewFolder);
    assert!(app.modal.is_some());
    let _ = app.update(Message::Escape);
    assert!(app.modal.is_none(), "one Escape closes the dialog");
}

#[test]
fn stop_and_quit_waits_for_the_cancelled_job_then_exits() {
    use glide_files::app::{Dialog, JobEvent};
    use glide_files::fs::{ClipboardAction, FileClipboard};
    let mut app = listed_app();
    let scratch = std::env::temp_dir().join(format!("glide-quit-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch");
    app.current = scratch.clone();
    app.clipboard = Some(FileClipboard { action: ClipboardAction::Copy, paths: vec![path(9)] });
    let _ = app.update(Message::Paste);
    assert_eq!(app.jobs.len(), 1);
    let _ = app.update(Message::Quit);
    assert!(matches!(app.modal.as_ref().map(|modal| &modal.dialog), Some(Dialog::Quit)), "quitting mid-job asks first");
    let _ = app.update(Message::ModalSubmit);
    assert!(app.is_quitting());
    assert_eq!(app.jobs.len(), 1, "the job is told to stop, not dropped");
    let _ = app.update(Message::Quit);
    assert!(app.modal.is_none(), "asking again while stopping is pointless");
    let _ = app.update(Message::Paste);
    assert_eq!(app.jobs.len(), 1, "no new work starts while quitting");
    let exit = app.update(Message::Job(1, JobEvent::Done(Box::default())));
    assert!(app.jobs.is_empty());
    assert_eq!(exit.units(), 1, "the last job to finish hands back the exit task");
    let _ = std::fs::remove_dir_all(scratch);
}

#[test]
fn a_drop_from_another_app_copies_into_the_folder_unless_it_is_already_there() {
    use glide_files::dnd::Event as Dnd;
    let mut app = listed_app();
    let scratch = std::env::temp_dir().join(format!("glide-drop-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch");
    app.current = scratch.clone();
    let _ = app.update(Message::Dnd(Dnd::Drop { paths: vec![scratch.join("here.txt")], at: (5.0, 5.0), moved: false }));
    assert!(app.jobs.is_empty(), "dropping a folder's own file back into it does nothing");
    let _ = app.update(Message::Dnd(Dnd::Drop { paths: vec![path(4)], at: (5.0, 5.0), moved: true }));
    assert_eq!(app.jobs.len(), 1, "a foreign file starts a transfer");
    let _ = std::fs::remove_dir_all(scratch);
}

#[test]
fn nothing_new_is_created_inside_the_trash() {
    let mut app = listed_app();
    app.current = app.trash.files();
    let _ = app.update(Message::NewFolder);
    let _ = app.update(Message::NewFile);
    assert!(app.modal.is_none(), "Ctrl+Shift+N in the Trash asks for nothing");
}

#[test]
fn a_search_result_offers_its_folder() {
    let labels = |app: &glide_files::App| -> Vec<String> {
        let menu = app.menu.as_ref().expect("menu");
        menu::entries(app, menu)
            .into_iter()
            .filter_map(|entry| match entry {
                menu::Entry::Item(item) => Some(item.label.into_owned()),
                _ => None,
            })
            .collect()
    };
    let show = Key::ShowInFolder.text(Lang::Uk);
    let mut app = listed_app();
    app.lang = Lang::Uk;
    let _ = app.update(Message::Pressed(path(1)));
    let _ = app.update(Message::Released);
    let _ = app.update(Message::KeyMenu);
    assert!(!labels(&app).iter().any(|label| label == show), "a plain folder view needs no detour");
    let _ = app.update(Message::Escape);
    let _ = app.update(Message::TypeToSearch(String::from("1")));
    let _ = app.update(Message::Pressed(path(1)));
    let _ = app.update(Message::Released);
    let _ = app.update(Message::KeyMenu);
    assert!(labels(&app).iter().any(|label| label == show));
}

#[test]
fn renaming_a_search_result_runs_the_search_again() {
    use glide_files::fs::FileEntry;
    let mut app = listed_app();
    let scratch = std::env::temp_dir().join(format!("glide-search-rename-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch");
    std::fs::write(scratch.join("old.txt"), b"x").expect("file");
    app.current = scratch.clone();
    app.entries = FileEntry::from_path(scratch.join("old.txt")).into_iter().collect();
    let _ = app.update(Message::TypeToSearch(String::from("o")));
    app.entries = FileEntry::from_path(scratch.join("old.txt")).into_iter().collect();
    if let Some(search) = app.search.as_mut() {
        search.pending = false;
    }
    let _ = app.update(Message::Pressed(scratch.join("old.txt")));
    let _ = app.update(Message::Released);
    let _ = app.update(Message::Rename);
    let _ = app.update(Message::RenameInput(String::from("new.txt")));
    let _ = app.update(Message::RenameSubmit);
    assert!(scratch.join("new.txt").exists());
    assert!(app.search.as_ref().is_some_and(|search| search.pending), "the results are fetched again");
    let _ = std::fs::remove_dir_all(scratch);
}

#[test]
fn the_sort_menu_picks_a_key_and_only_the_column_header_flips_the_order() {
    use glide_files::fs::SortBy;
    let mut app = listed_app();
    let ascending = app.sort.ascending;
    let _ = app.update(Message::SortKey(app.sort.by));
    assert_eq!(app.sort.ascending, ascending, "choosing the checked key again keeps the order");
    let _ = app.update(Message::SortKey(SortBy::Size));
    assert_eq!((app.sort.by, app.sort.ascending), (SortBy::Size, ascending));
    let _ = app.update(Message::SortBy(SortBy::Size));
    assert_eq!(app.sort.ascending, !ascending, "a second click on the header reverses");
}

#[test]
fn ctrl_z_inside_a_text_field_never_reverts_a_file_operation() {
    use glide_files::app::{Outcome, Undo};
    let mut app = listed_app();
    let done = Outcome {
        text: Some(String::from("moved")),
        select: Vec::new(),
        undo: Some(Undo::Restore(Vec::new())),
        clear_clipboard: false,
    };
    let _ = app.update(Message::Done(done));
    assert!(app.notice.is_some());
    let _ = app.update(Message::Pressed(path(2)));
    let _ = app.update(Message::Released);
    let _ = app.update(Message::Rename);
    let _ = app.update(Message::Undo);
    assert!(app.notice.is_some(), "Ctrl+Z while renaming leaves the undo alone");
    let _ = app.update(Message::Escape);
    let _ = app.update(Message::EditPath);
    let _ = app.update(Message::Undo);
    assert!(app.notice.is_some(), "and so does Ctrl+Z in the path field");
    let _ = app.update(Message::Escape);
    let _ = app.update(Message::Undo);
    assert!(app.notice.is_none(), "outside a field it undoes");
}

#[test]
fn leaving_the_folder_by_keyboard_ends_an_inline_rename() {
    for leave in [Message::GoHome, Message::KeyMenu] {
        let mut app = listed_app();
        let _ = app.update(Message::Pressed(path(3)));
        let _ = app.update(Message::Released);
        let _ = app.update(Message::Rename);
        assert!(app.renaming.is_some());
        let _ = app.update(leave.clone());
        assert!(app.renaming.is_none(), "{leave:?}");
    }
}

#[test]
fn shift_arrows_grow_and_shrink_the_range_from_where_it_started() {
    use std::path::PathBuf;
    let mut app = listed_app();
    let selected = |app: &glide_files::App| {
        let mut picked: Vec<PathBuf> = app.selection.iter().cloned().collect();
        picked.sort();
        picked
    };
    let _ = app.update(Message::Pressed(path(2)));
    let _ = app.update(Message::Released);
    for _ in 0..2 {
        let _ = app.update(Message::Move(Step::Down, true));
    }
    assert_eq!(selected(&app), [path(2), path(3), path(4)]);
    let _ = app.update(Message::Move(Step::Up, true));
    assert_eq!(selected(&app), [path(2), path(3)], "moving back shrinks the range");
    for _ in 0..3 {
        let _ = app.update(Message::Move(Step::Up, true));
    }
    assert_eq!(selected(&app), [path(0), path(1), path(2)], "and it crosses its starting point");
    let _ = app.update(Message::Move(Step::Down, false));
    assert_eq!(selected(&app), [path(1)], "a plain arrow moves from the cursor, not the anchor");
    let _ = app.update(Message::SelectAll);
    let _ = app.update(Message::Move(Step::Down, true));
    assert_eq!(selected(&app).len(), 6, "Shift+Down after Select All keeps everything");
    let _ = app.update(Message::Move(Step::Up, true));
    assert_eq!(selected(&app).len(), 5, "and Shift+Up gives back the last row");
}

#[test]
fn escape_reaches_the_app_even_when_a_text_field_captured_it() {
    use iced::event::Status;
    assert_eq!(named(Named::Escape, Modifiers::empty()), None, "handled once, by window_events");
    let escape = iced::Event::Keyboard(Event::KeyPressed {
        key: IcedKey::Named(Named::Escape),
        modified_key: IcedKey::Named(Named::Escape),
        physical_key: Physical::Code(Code::Escape),
        location: Location::Standard,
        modifiers: Modifiers::empty(),
        repeat: false,
        text: None,
    });
    for status in [Status::Captured, Status::Ignored] {
        let window = iced::window::Id::unique();
        assert_eq!(window_events(escape.clone(), status, window), Some(Message::Escape));
    }
}

#[test]
fn the_mouse_side_buttons_walk_the_history() {
    use iced::event::Status;
    use iced::mouse::{Button, Event as Mouse};
    let window = iced::window::Id::unique();
    let pressed = |button| window_events(iced::Event::Mouse(Mouse::ButtonPressed(button)), Status::Ignored, window);
    assert_eq!(pressed(Button::Back), Some(Message::Back));
    assert_eq!(pressed(Button::Forward), Some(Message::Forward));
    assert_eq!(pressed(Button::Middle), None);
}

#[test]
fn printable_keys_start_type_ahead_search() {
    assert_eq!(chord("д", Modifiers::empty()), Some(Message::TypeToSearch(String::from("д"))));
    assert_eq!(press(IcedKey::Named(Named::Tab), Modifiers::empty(), Some("\t")), None);
    assert_eq!(chord("q", Modifiers::ALT), None);
}

#[test]
fn modal_dialogs_swallow_shortcut_actions() {
    for blocked in [
        Message::Trash,
        Message::Delete,
        Message::OpenSelection,
        Message::Paste,
        Message::Back,
        Message::TypeToSearch(String::from("a")),
    ] {
        assert!(!blocked.passes_modal(), "{blocked:?}");
    }
    for allowed in [Message::ModalSubmit, Message::ModalCancel, Message::Escape, Message::Tick] {
        assert!(allowed.passes_modal(), "{allowed:?}");
    }
}

#[test]
fn every_catalogue_entry_is_translated() {
    for key in Key::ALL {
        assert!(!key.en().trim().is_empty(), "{key:?} lacks English");
        assert!(!key.uk().trim().is_empty(), "{key:?} lacks Ukrainian");
    }
}

#[test]
fn item_counts_use_ukrainian_plural_forms() {
    let uk = |n| i18n::items(n, Lang::Uk);
    assert_eq!(uk(1), "1 об’єкт");
    assert_eq!(uk(3), "3 об’єкти");
    assert_eq!(uk(5), "5 об’єктів");
    assert_eq!(uk(11), "11 об’єктів");
    assert_eq!(uk(12), "12 об’єктів");
    assert_eq!(uk(21), "21 об’єкт");
    assert_eq!(uk(104), "104 об’єкти");
    assert_eq!(i18n::items(1, Lang::En), "1 item");
    assert_eq!(i18n::items(0, Lang::En), "0 items");
}

#[test]
fn look_reads_only_the_documented_sections() {
    let look =
        Look::parse("[general]\nlanguage = \"en\"\n\n[dock]\nappearance = \"light\"\n\n[topbar]\naccent = \"green\"\n");
    assert_eq!(look.appearance, Appearance::Dark);
    assert_eq!(look.accent, Accent::Green);
    assert_eq!(look.language, Some(Lang::En));

    let light = Look::parse("[topbar]\nappearance = \"light\"\n");
    assert_eq!(light.appearance, Appearance::Light);
    assert_eq!(light.language, None);
    assert_eq!(light.icon_theme, None);
    assert_eq!(Look::parse("[dock]\nicon-theme = \"Papirus-Dark\"\n").icon_theme, Some(String::from("Papirus-Dark")));
    assert_eq!(Look::parse("not toml ["), Look::default());
}

#[test]
fn themes_contrast_and_hover_is_an_eight_percent_layer() {
    let dark = ThemeState::default();
    assert!(dark.fg_1().r > 0.8 && dark.view_bg().r < 0.2);
    let light = ThemeState::new(Appearance::Light, Accent::Blue);
    assert!(light.fg_1().r < 0.2 && light.view_bg().r > 0.8);

    let hovered = dark.layered(iced::Color::TRANSPARENT, iced::widget::button::Status::Hovered);
    assert!((hovered.a - glide_theme::motion::ST_HOVER).abs() < 1e-6);
    let pressed = dark.layered(iced::Color::TRANSPARENT, iced::widget::button::Status::Pressed);
    assert!((pressed.a - glide_theme::motion::ST_PRESS).abs() < 1e-6);

    let half = dark.hover(iced::Color::TRANSPARENT, 0.5);
    assert!((half.a - glide_theme::motion::ST_HOVER / 2.0).abs() < 1e-6);
    assert_eq!(dark.hover(dark.view_bg(), 0.0), dark.view_bg());
}

#[test]
fn fading_a_theme_scales_every_colour_alpha() {
    let theme = ThemeState::default();
    let faded = theme.faded(0.25);
    assert!((faded.fg_1().a - theme.fg_1().a * 0.25).abs() < 1e-6);
    assert!((faded.popover_bg().a - theme.popover_bg().a * 0.25).abs() < 1e-6);
    assert_eq!(faded.faded(0.0).acc_bg().a, 0.0);
    assert_eq!(theme.faded(4.0), theme);
}

#[test]
fn animation_scale_follows_the_compositor_rules() {
    let scale = |text: &str, reduced| Look::parse_with(text, reduced).motion_scale;
    assert_eq!(scale("", false), 1.0);
    assert_eq!(scale("[animations]\nspeed = 0.5\n", false), 0.5);
    assert_eq!(scale("[animations]\nenabled = false\n", false), 0.0);
    assert_eq!(scale("", true), 0.0);
    assert_eq!(scale("[animations]\nrespect-system = false\n", true), 1.0);
}

#[test]
fn grid_geometry_matches_keyboard_navigation() {
    let m = Metrics::default();
    let fits = |columns: f32| 2.0 * GRID_PADDING + columns * m.tile_w + (columns - 1.0) * TILE_GAP;
    assert_eq!(m.columns(0.0), 1);
    assert_eq!(m.columns(fits(1.0)), 1);
    assert_eq!(m.columns(fits(4.0)), 4);
    assert_eq!(m.columns(fits(4.0) - 1.0), 3);

    let width = fits(4.0);
    let (first_top, _) = m.span(ViewMode::Grid, 0, width);
    let (same_row, _) = m.span(ViewMode::Grid, 3, width);
    let (next_row, _) = m.span(ViewMode::Grid, 4, width);
    assert_eq!(first_top, same_row);
    assert!(next_row > same_row);

    let (top, bottom) = m.span(ViewMode::List, 10, width);
    assert!(bottom > top);
    assert_eq!(m.span(ViewMode::List, 11, width).0 - top, bottom - top - 8.0);
}

#[test]
fn menus_flip_away_from_window_edges() {
    let window = Size::new(1000.0, 600.0);
    assert_eq!(menu::place(Point::new(100.0, 100.0), 200.0, window), Point::new(100.0, 100.0));
    assert_eq!(menu::place(Point::new(900.0, 500.0), 200.0, window), Point::new(900.0 - menu::WIDTH, 392.0));
    assert_eq!(menu::place(Point::new(10.0, 10.0), 900.0, window), Point::new(10.0, 8.0));
}

#[test]
fn long_names_keep_their_extension_when_shortened() {
    assert_eq!(ellipsize("short.txt", 28), "short.txt");
    let shortened = ellipsize("a-very-long-holiday-photo-from-the-beach.jpeg", 20);
    assert_eq!(shortened.chars().count(), 20);
    assert_eq!(shortened.chars().rev().take(5).collect::<String>(), "gepj.", "{shortened}");
    assert!(shortened.contains('…'));
    let fitted = glide_files::widgets::fit("a-very-long-holiday-photo-from-the-beach.jpeg", 120.0, 13.0);
    assert!(glide_files::widgets::text_width(&fitted, 13.0) <= 120.0, "{fitted}");
    assert!(fitted.rsplit('.').next() == Some("jpeg") && fitted.contains('…'), "{fitted}");
    assert_eq!(glide_files::widgets::fit("Звіт.odt", 120.0, 13.0), "Звіт.odt");
    for max in 0..6 {
        assert!(ellipsize("abcdef.txt", max).chars().count() <= max.max(1), "max {max}");
    }
    for width in [-80.0, 0.0, 1.0, 20.0] {
        let tight = glide_files::widgets::fit("quarterly report.odt", width, 13.0);
        assert!(tight.contains('…'), "{width}: {tight}");
    }
    let no_extension = ellipsize("LICENSE-APACHE-VERSION-TWO-POINT-ZERO", 12);
    assert_eq!(no_extension.chars().count(), 12);
}

#[test]
fn drops_refuse_self_parent_and_trash_loops() {
    use glide_files::app::droppable;
    use std::path::PathBuf;
    let trash = PathBuf::from("/home/u/.local/share/Trash/files");
    let docs = PathBuf::from("/home/u/Documents");
    let photo = PathBuf::from("/home/u/Pictures/a.png");
    assert!(droppable(std::slice::from_ref(&photo), &docs, &trash));
    assert!(!droppable(std::slice::from_ref(&docs), &docs, &trash), "a folder can't be dropped onto itself");
    assert!(!droppable(std::slice::from_ref(&docs), &docs.join("inner"), &trash), "nor into its own subfolder");
    assert!(!droppable(std::slice::from_ref(&photo), &PathBuf::from("/home/u/Pictures"), &trash), "same folder");
    assert!(droppable(std::slice::from_ref(&photo), &trash, &trash), "the Trash place accepts files");
    assert!(!droppable(&[trash.join("old.txt")], &trash, &trash));
    assert!(!droppable(std::slice::from_ref(&photo), &trash.join("sub"), &trash), "never into Trash internals");
}
