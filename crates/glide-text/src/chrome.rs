use std::cell::RefCell;
use std::collections::HashMap;

use glide_sdk::theme::{R3, SP1, SP2, SP3, SP4, SP5, SP6};
use glide_sdk::theme::{TextStyle, component, motion};
use glide_sdk::widget::{
    Icon, Size, bar_strip, code_block, compact_button, empty_note, empty_state, flat_toggle_button, heading_row,
    icon_button, markdown_settings, markdown_view, pane, scrollable, separator as rule, shortcut_list, stat_rows, text,
    tooltip as tip, vertical_separator as vertical_rule,
};
use glide_sdk::window::resize_handles;
use glide_sdk::{Paint, Tone};
use iced::border::Radius;
use iced::widget::{Space, column, container, markdown, mouse_area, row, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use crate::app::{
    Answer, App, Bar, Command, Dialog, DialogKind, GOTO_ID, Item, Menu, MenuKind, Message, REPLACE_ID, Toggle,
    shortcut_label,
};
use crate::app::{EDITOR, FIND_ID, FINDER_ID, FinderMessage};
use crate::document::{Document, Ending};
use crate::edit::Case;
use crate::i18n::Key;
use crate::syntax::{Bridge, Highlight, Language};

pub const HEADER: f32 = 38.0;
pub const STATUS: f32 = 26.0;
pub(crate) const ISLAND_LIFT: f32 = 50.0;
pub(crate) const ISLAND_ZONE: f32 = 76.0;
pub(crate) const SIDEBAR: f32 = 248.0;
pub(crate) const SIDEBAR_MIN_WINDOW: f32 = 560.0;
const CONTROL: f32 = component::SMALL_CONTROL;
const MENU_WIDTH: f32 = 300.0;

fn bar_width(app: &App, bar: Bar) -> f32 {
    glide_sdk::menu::bar_width(bar.key().text(app.lang))
}

fn bar_start() -> f32 {
    SP2 + CONTROL + SP2
}

fn controls_width() -> f32 {
    3.0 * CONTROL + 2.0 * SP2 + SP2 + 2.0
}

#[must_use]
pub fn compact(app: &App) -> bool {
    let bars: f32 = Bar::ALL.iter().map(|bar| bar_width(app, *bar)).sum::<f32>() + SP1 * 5.0;
    bar_start() + bars + controls_width() + 120.0 > app.window.size.width
}

#[must_use]
pub fn bar_x(app: &App, target: Bar) -> f32 {
    if compact(app) {
        return bar_start();
    }
    let mut x = bar_start();
    for bar in Bar::ALL {
        if bar == target {
            break;
        }
        x += bar_width(app, bar) + SP1;
    }
    x
}

fn menu_bar(app: &App, paint: &Paint) -> Element<'static, Message> {
    let labels = Bar::ALL.map(|bar| bar.key().text(app.lang));
    let open = app.menu.as_ref().filter(|menu| menu.reveal.is_open()).and_then(|menu| match menu.kind {
        MenuKind::Bar(bar) => Bar::ALL.iter().position(|candidate| *candidate == bar),
        _ => None,
    });
    glide_sdk::menu::bar_of(paint, &labels, open).map(|event| match event {
        glide_sdk::menu::BarEvent::Pressed(index) => Message::BarPressed(Bar::ALL[index]),
        glide_sdk::menu::BarEvent::Hovered(index) => Message::BarHovered(Bar::ALL[index]),
    })
}

fn header(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let control_paint = app.window.controls_paint(&paint);
    let title = if app.shell_menu {
        String::new()
    } else {
        app.doc().map_or_else(
            || Key::AppName.text(app.lang).to_string(),
            |doc| format!("{}{}", if doc.dirty() { "• " } else { "" }, app.doc_name(doc)),
        )
    };
    let mut header = glide_sdk::window::Header::new(title)
        .subtitle(app.subtitle())
        .labels(Key::Maximize.text(app.lang), Key::CloseWindow.text(app.lang))
        .height(HEADER);
    let mut taken = 0.0;
    if !app.shell_menu {
        header = header.leading(if compact(app) {
            let open = app.menu.as_ref().is_some_and(|menu| menu.kind == MenuKind::Compact && menu.reveal.is_open());
            icon_button(
                &control_paint,
                Icon::Menu,
                Key::Menu.text(app.lang),
                Some(Message::BarPressed(Bar::File)),
                open,
            )
        } else {
            taken = Bar::ALL.iter().map(|bar| bar_width(app, *bar) + SP1).sum::<f32>() - CONTROL - SP2;
            menu_bar(app, &control_paint)
        });
    }
    if app.docs.len() > 1 && !app.focus {
        header = header.center(tab_bar(app));
    }
    let sidebar = Some(Message::Command(Command::ToggleSidebar));
    header = header
        .trailing(icon_button(&control_paint, Icon::Sidebar, Key::Sidebar.text(app.lang), sidebar, app.prefs.sidebar))
        .trailing(icon_button(
            &control_paint,
            Icon::Plus,
            Key::New.text(app.lang),
            Some(Message::Command(Command::New)),
            false,
        ));
    glide_sdk::window::header(&paint, &app.window, header, Message::Window, Message::CloseRequested, taken)
}

fn tab_bar(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{Mark, Tab, tab_strip};
    let tabs = app
        .docs
        .iter()
        .map(|doc| Tab::new(doc.id, app.doc_name(doc)).mark(if doc.dirty() { Mark::Dot } else { Mark::None }))
        .collect();
    tab_strip(&app.paint, tabs, app.active)
        .height(HEADER)
        .bare()
        .on_select(Message::TabPressed)
        .hovered(app.hovered_tab, Message::TabHovered)
        .on_reorder(Message::TabMoved)
        .on_close(|index| Message::Command(Command::CloseTab(index)), Key::CloseTab.text(app.lang))
        .into()
}

fn find_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(find) = &app.find else { return Space::new().into() };
    let lang = app.lang;
    let input = text_input(Key::FindPlaceholder.text(lang), &find.query.text)
        .id(FIND_ID)
        .on_input(Message::FindInput)
        .on_submit(Message::FindSubmit);
    let mut bar = glide_sdk::widget::find_bar(&paint, input)
        .narrow(app.window.size.width < 760.0)
        .steps(
            (Message::Command(Command::FindPrevious), Key::FindPrevious.text(lang)),
            (Message::Command(Command::FindNext), Key::FindNext.text(lang)),
        )
        .toggle("Aa", Key::MatchCase.text(lang), find.query.case, Message::FindToggle(Toggle::Case))
        .toggle("W", Key::WholeWord.text(lang), find.query.word, Message::FindToggle(Toggle::Word))
        .toggle(".*", Key::Regex.text(lang), find.query.regex, Message::FindToggle(Toggle::Regex))
        .status(app.find_label(), find.error.is_some())
        .extra(tip(
            &paint,
            flat_toggle_button(&paint, "⇄", find.replacing, Size::Small, Some(|_| Message::ToggleReplace)),
            Key::ShowReplace.text(lang),
        ))
        .done(Key::Done.text(lang), Message::FindClose);
    if find.replacing {
        let replace = text_input(Key::ReplacePlaceholder.text(lang), &find.replace)
            .id(REPLACE_ID)
            .on_input(Message::ReplaceInput)
            .on_submit(Message::ReplaceOne);
        bar = bar.replace(
            replace,
            vec![(Key::ReplaceOne.text(lang), Message::ReplaceOne), (Key::ReplaceAll.text(lang), Message::ReplaceAll)],
        );
    }
    bar.into()
}

fn goto_bar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(input) = &app.goto else { return Space::new().into() };
    let line = app.doc().map_or(1, |doc| doc.cursor().position.line + 1);
    let total = app.doc().map_or(1, |doc| doc.text.matches('\n').count() + 1);
    let field = glide_sdk::widget::field(
        &paint,
        text_input(Key::LinePlaceholder.text(app.lang), input)
            .id(GOTO_ID)
            .on_input(Message::GotoInput)
            .on_submit(Message::GotoSubmit),
    );
    bar_strip(
        &paint,
        None,
        row![
            text(Key::GoToLine.text(app.lang).trim_end_matches('…').to_string(), TextStyle::CALL),
            container(field).width(180),
            text(format!("{line} / {total}"), TextStyle::NUM).color(paint.fg_3()),
            Space::new().width(Fill),
            compact_button(&paint, Key::Cancel.text(app.lang), Tone::Neutral, Some(Message::Escape)),
        ]
        .spacing(SP2)
        .align_y(Alignment::Center),
    )
}

fn infobar(app: &App) -> Option<Element<'_, Message>> {
    let paint = app.paint;
    let doc = app.doc().filter(|doc| doc.changed_on_disk)?;
    let gone = doc.path.as_deref().is_some_and(|path| !path.exists());
    let message = if gone { Key::DeletedOnDisk } else { Key::ChangedOnDisk };
    let mut actions = Vec::new();
    if !gone {
        actions.push(compact_button(&paint, Key::Reload.text(app.lang), Tone::Neutral, Some(Message::Disk(true))));
    }
    actions.push(compact_button(&paint, Key::KeepMine.text(app.lang), Tone::Ghost, Some(Message::Disk(false))));
    let warn = Some(glide_sdk::widget::Severity::Warn);
    Some(glide_sdk::widget::info_bar(&paint, warn, message.text(app.lang), "", actions))
}

fn tree_row<'a>(app: &'a App, node: &'a crate::tree::Node) -> Element<'a, Message> {
    let current = app.doc().and_then(|doc| doc.path.as_deref()) == Some(node.entry.path.as_path());
    let folder = node.entry.folder;
    let row = glide_sdk::widget::tree_row(
        &app.paint,
        node.depth,
        folder.then_some(node.expanded),
        if folder { Icon::Folder } else { Icon::File },
        node.entry.name.as_str(),
        current,
        Message::TreeClicked(node.entry.path.clone()),
    );
    mouse_area(row).on_right_press(Message::TreeMenu(node.entry.path.clone())).into()
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let body: Element<'_, Message> = if let Some(tree) = &app.tree {
        let action = |glyph, key: Key, command| {
            icon_button(&paint, glyph, key.text(app.lang), Some(Message::Command(command)), false)
        };
        let title = heading_row(
            &paint,
            tree.title(),
            vec![
                action(Icon::Refresh, Key::Refresh, Command::RefreshTree),
                action(Icon::Close, Key::CloseFolder, Command::CloseFolder),
            ],
        );
        let rows: Vec<Element<'_, Message>> = tree.nodes.iter().map(|node| tree_row(app, node)).collect();
        let list: Element<'_, Message> = if tree.loaded && rows.is_empty() {
            empty_note(&paint, Icon::Folder, "", Key::EmptyFolder.text(app.lang))
        } else {
            scrollable(&paint, iced::widget::column(rows).padding(Padding::from([0.0, SP2]))).height(Fill).into()
        };
        column![title, list].into()
    } else {
        let open = Some(Message::Command(Command::OpenFolder));
        let button = compact_button(&paint, Key::OpenFolderButton.text(app.lang), Tone::Neutral, open);
        empty_state(&paint, Icon::Folder, "", Key::NoFolder.text(app.lang), Some(button))
    };
    let bottom_left = if app.edge_to_edge() || app.status_visible() { 0.0 } else { R3 };
    pane(paint.sidebar_bg(), Radius::default().bottom_left(bottom_left), body).width(SIDEBAR).into()
}

struct Preview {
    paint: Paint,
}

fn fenced(language: Language, code: &str) -> std::sync::Arc<crate::syntax::Lines> {
    thread_local! {
        static CACHE: RefCell<HashMap<(Language, String), std::sync::Arc<crate::syntax::Lines>>> =
            RefCell::new(HashMap::new());
    }
    let key = (language, code.to_string());
    if let Some(lines) = CACHE.with_borrow(|cache| cache.get(&key).cloned()) {
        return lines;
    }
    let lines = std::sync::Arc::new(crate::syntax::highlight(language, code));
    CACHE.with_borrow_mut(|cache| {
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(key, std::sync::Arc::clone(&lines));
    });
    lines
}

impl<'a> markdown::Viewer<'a, Message> for Preview {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::Link(url)
    }

    fn code_block(
        &self, settings: markdown::Settings, language: Option<&'a str>, code: &'a str, _lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        let paint = self.paint;
        let language = language.and_then(Language::from_key).unwrap_or(Language::Plain);
        let palette = crate::syntax::Palette::new(&paint.tokens, crate::MONO);
        let lines = fenced(language, code.trim_end_matches('\n'));
        let rows = code.trim_end_matches('\n').split('\n').enumerate().map(|(index, line)| {
            let mut spans: Vec<iced::widget::text::Span<'a, (), iced::Font>> = Vec::new();
            let mut at = 0;
            for (range, style) in lines.get(index).map(Vec::as_slice).unwrap_or_default() {
                if range.start < at || range.end > line.len() {
                    continue;
                }
                if range.start > at {
                    spans.push(iced::widget::span(&line[at..range.start]).font(crate::MONO));
                }
                let highlight = palette.format(*style);
                spans.push(
                    iced::widget::span(&line[range.clone()])
                        .font(highlight.font.unwrap_or(crate::MONO))
                        .color_maybe(highlight.color),
                );
                at = range.end;
            }
            if at < line.len() || line.is_empty() {
                spans.push(iced::widget::span(&line[at..]).font(crate::MONO));
            }
            iced::widget::rich_text(spans).size(settings.code_size).into()
        });
        code_block(&paint, iced::widget::column(rows), settings.code_size.0)
    }
}

fn table_view<'a>(app: &'a App, rows: &'a [Vec<String>]) -> Element<'a, Message> {
    use glide_sdk::widget::{TABLE_HEADER, table_header, table_row};
    let paint = app.paint;
    let Some((head, body)) = rows.split_first() else {
        return empty_note(&paint, Icon::File, "", Key::EmptyTable.text(app.lang));
    };
    let widths = crate::table::widths(rows);
    let cells = |values: &'a [String], bold: bool, color: iced::Color| -> Vec<Element<'a, Message>> {
        let style = if bold { TextStyle { weight: 600, ..TextStyle::CAP } } else { TextStyle::CAP };
        (0..widths.len())
            .map(|column| {
                let value = values.get(column).map_or("", String::as_str);
                container(text(value, style).color(color).wrapping(glide_sdk::widget::Wrapping::None))
                    .width(Length::FillPortion(widths[column]))
                    .padding(Padding::from([0.0, SP1]))
                    .clip(true)
                    .into()
            })
            .collect()
    };
    let header = table_header(&paint, cells(head, true, paint.fg_2()));
    let lines = body.iter().enumerate().map(|(index, values)| {
        let content = iced::widget::Row::with_children(cells(values, false, paint.fg_1()));
        table_row(&paint, content, false, index % 2 == 1, 0.0, TABLE_HEADER)
    });
    column![header, scrollable(&paint, iced::widget::column(lines)).height(Fill)].into()
}

fn editor(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let Some(doc) = app.doc() else {
        return empty_state(&paint, Icon::File, Key::EmptyTitle.text(app.lang), Key::EmptyBody.text(app.lang), None);
    };
    let blocked = app.dialog.is_some() || app.menu_open() || app.finder.is_some();
    let editor = glide_sdk::widget::editor(&paint, &doc.content)
        .id(EDITOR)
        .on_action(Message::Edit)
        .key_binding(move |press| if blocked { None } else { crate::app::binding(press) })
        .font(app.font())
        .size(app.font_size())
        .padding(Padding { top: SP3, bottom: SP6, left: SP5, right: SP4 })
        .highlight_with::<Bridge>(app.settings(doc), Highlight::to_format);
    let editor = mouse_area(editor).on_right_press(Message::Command(Command::ContextMenu));
    if !app.previewing() {
        if app.focus {
            let width = app.font_size() * 0.62 * 72.0 + SP5 + SP4;
            return container(container(editor).width(Fill).max_width(width)).width(Fill).center_x(Fill).into();
        }
        return editor.into();
    }
    if let Some((_, _, rows)) = &app.table {
        return row![
            container(editor).width(Fill),
            vertical_rule(&paint),
            container(table_view(app, rows)).width(Fill)
        ]
        .into();
    }
    let items = app.markdown.as_ref().map(|(_, _, items)| items.as_slice()).unwrap_or_default();
    let settings = markdown_settings(&paint, app.font_size(), crate::MONO);
    let viewer = Preview { paint };
    let preview = markdown_view(&paint, items, settings, &viewer);
    row![
        container(editor).width(Fill),
        vertical_rule(&paint),
        container(glide_sdk::widget::arriving(preview)).width(Fill)
    ]
    .into()
}

fn status_bar(app: &App) -> Element<'_, Message> {
    let (left, right) = status_parts(app);
    let corners =
        if app.edge_to_edge() { Radius::default() } else { Radius::default().bottom_left(R3).bottom_right(R3) };
    glide_sdk::widget::status_bar(&app.paint, left, right, corners)
}

type Parts<'a> = (Vec<Element<'a, Message>>, Vec<Element<'a, Message>>);

fn status_parts(app: &App) -> Parts<'_> {
    use glide_sdk::widget::status_button;
    let paint = app.paint;
    let lang = app.lang;
    let mut left: Vec<Element<'_, Message>> = Vec::new();
    let mut right: Vec<Element<'_, Message>> = Vec::new();
    if let Some(doc) = app.doc() {
        let cursor = doc.cursor().position;
        let line_start =
            crate::document::offset(&doc.text, iced::widget::text_editor::Position { line: cursor.line, column: 0 });
        let caret = crate::document::offset(&doc.text, cursor);
        let column = doc.text[line_start..caret].chars().count() + 1;
        left.push(
            text(
                format!("{} {}, {} {column}", Key::Line.text(lang), cursor.line + 1, Key::Column.text(lang)),
                TextStyle::CAP,
            )
            .color(paint.fg_2())
            .into(),
        );
        let selected = doc.selected();
        if !selected.is_empty() {
            let count = doc.text[selected].chars().count();
            left.push(text(format!("{count} {}", Key::Selected.text(lang)), TextStyle::CAP).color(paint.fg_3()).into());
        }
        if doc.text.len() <= 2 * 1024 * 1024 {
            left.push(
                text(format!("{} {}", doc.words(), Key::Words.text(lang)), TextStyle::CAP).color(paint.fg_3()).into(),
            );
        }
        let language = if doc.pinned.is_some() {
            doc.language().name().to_string()
        } else {
            format!("{} · {}", doc.language().name(), Key::Detected.text(lang))
        };
        let indent = if app.prefs.spaces {
            format!("{}: {}", Key::Spaces.text(lang), app.prefs.tab_width)
        } else {
            format!("{}: {}", Key::Tabs.text(lang), app.prefs.tab_width)
        };
        let at = app.window.size.width - MENU_WIDTH - SP2;
        right.extend([
            status_button(&paint, language, Message::StatusMenu(MenuKind::Syntax, at)),
            status_button(&paint, indent, Message::StatusMenu(MenuKind::TabWidths, at)),
            status_button(&paint, doc.ending.name(), Message::StatusMenu(MenuKind::Endings, at)),
            text(doc.encoding.name(), TextStyle::CAP).color(paint.fg_3()).into(),
        ]);
    }
    (left, right)
}

fn status_islands(app: &App) -> Element<'_, Message> {
    let (left, right) = status_parts(app);
    let shown = if app.prefs.auto_hide_status { app.status_reveal.value(motion::t2()) as f32 } else { 1.0 };
    glide_sdk::widget::status_islands(&app.paint, shown, left, right)
}

fn labelled(label: String, command: Command, enabled: bool, on: Option<bool>) -> Item {
    let mut item = Item::action(label, command).enabled(enabled);
    if let Some(on) = on {
        item = item.checked(on);
    }
    if opens_more(command) { item.cascades() } else { item }
}

fn action(app: &App, key: Key, command: Command, enabled: bool) -> Item {
    let item = labelled(key.text(app.lang).to_string(), command, enabled, None);
    match shortcut_label(command) {
        Some(chord) => item.shortcut(chord),
        None => item,
    }
}

fn check(app: &App, key: Key, command: Command, on: bool, enabled: bool) -> Item {
    action(app, key, command, enabled).checked(on)
}

#[allow(clippy::too_many_lines)] // The whole menu bar as one table, so menus and shortcuts read side by side
fn bar_items(app: &App, bar: Bar) -> Vec<Item> {
    let doc = app.doc();
    let selection = doc.is_some_and(|doc| !doc.selected().is_empty());
    let path = doc.is_some_and(|doc| doc.path.is_some());
    let has = doc.is_some();
    match bar {
        Bar::File => vec![
            action(app, Key::New, Command::New, true),
            action(app, Key::NewWindow, Command::NewWindow, true),
            Item::Separator,
            action(app, Key::Open, Command::Open, true),
            action(app, Key::OpenFolder, Command::OpenFolder, true),
            action(app, Key::OpenRecent, Command::Recent, !app.recent.is_empty()),
            Item::Separator,
            action(app, Key::Save, Command::Save, has),
            action(app, Key::SaveAs, Command::SaveAs, has),
            action(app, Key::SaveAll, Command::SaveAll, app.docs.iter().any(Document::dirty)),
            action(app, Key::Revert, Command::Revert, path && doc.is_some_and(Document::dirty)),
            Item::Separator,
            action(app, Key::ShowInFiles, Command::ShowInFiles, path),
            action(app, Key::CopyPath, Command::CopyPath, path),
            Item::Separator,
            action(app, Key::CloseTab, Command::Close, has),
            action(app, Key::CloseFolder, Command::CloseFolder, app.tree.is_some()),
            action(app, Key::CloseWindow, Command::CloseWindow, true),
        ],
        Bar::Edit => vec![
            action(app, Key::Undo, Command::Undo, doc.is_some_and(|doc| doc.history.can_undo())),
            action(app, Key::Redo, Command::Redo, doc.is_some_and(|doc| doc.history.can_redo())),
            Item::Separator,
            action(app, Key::Cut, Command::Cut, selection),
            action(app, Key::Copy, Command::Copy, selection),
            action(app, Key::Paste, Command::Paste, has),
            action(app, Key::SelectAll, Command::SelectAll, has),
            Item::Separator,
            action(app, Key::DuplicateLine, Command::DuplicateLine, has),
            action(app, Key::DeleteLine, Command::DeleteLine, has),
            action(app, Key::MoveLineUp, Command::MoveLine(true), has),
            action(app, Key::MoveLineDown, Command::MoveLine(false), has),
            action(app, Key::JoinLines, Command::JoinLines, has),
            action(app, Key::SortLines, Command::SortLines, selection),
            Item::Separator,
            action(
                app,
                Key::ToggleComment,
                Command::ToggleComment,
                doc.is_some_and(|doc| {
                    doc.language().line_comment().is_some() || doc.language().block_comment().is_some()
                }),
            ),
            action(
                app,
                Key::ToggleCheck,
                Command::ToggleCheck,
                doc.is_some_and(|doc| doc.language() == Language::Markdown),
            ),
            action(app, Key::Indent, Command::Indent, has),
            action(app, Key::Unindent, Command::Unindent, has),
            Item::Separator,
            action(app, Key::Uppercase, Command::Case(Case::Upper), has),
            action(app, Key::Lowercase, Command::Case(Case::Lower), has),
            action(app, Key::TitleCase, Command::Case(Case::Title), has),
            action(app, Key::InvertCase, Command::Case(Case::Invert), has),
            Item::Separator,
            action(app, Key::TrimTrailing, Command::TrimTrailing, has),
            action(app, Key::InsertDate, Command::InsertDate, has),
        ],
        Bar::View => vec![
            check(app, Key::Sidebar, Command::ToggleSidebar, app.prefs.sidebar, true),
            check(app, Key::StatusBar, Command::ToggleStatusBar, app.prefs.status_bar, true),
            check(app, Key::Islands, Command::ToggleIslands, app.prefs.status_islands, app.prefs.status_bar),
            check(
                app,
                Key::AutoHideStatus,
                Command::ToggleAutoHideStatus,
                app.prefs.auto_hide_status,
                app.prefs.status_bar && app.prefs.status_islands,
            ),
            Item::Separator,
            check(app, Key::Monospace, Command::ToggleMonospace, app.prefs.monospace, true),
            check(app, Key::Preview, Command::TogglePreview, app.previewing(), doc.is_some_and(Document::previewable)),
            Item::Separator,
            action(app, Key::ZoomIn, Command::ZoomIn, true),
            action(app, Key::ZoomOut, Command::ZoomOut, true),
            action(app, Key::ZoomReset, Command::ZoomReset, app.zoom != 0.0),
            Item::Separator,
            check(app, Key::HiddenFiles, Command::ToggleHidden, app.prefs.hidden_files, true),
            action(app, Key::Refresh, Command::RefreshTree, app.tree.is_some()),
            Item::Separator,
            check(app, Key::Focus, Command::ToggleFocus, app.focus, true),
            check(app, Key::FullScreen, Command::FullScreen, app.window.frame == crate::app::Frame::Fullscreen, true),
        ],
        Bar::Search => vec![
            action(app, Key::Find, Command::Find, has),
            action(app, Key::Replace, Command::Replace, has),
            Item::Separator,
            action(app, Key::FindNext, Command::FindNext, has),
            action(app, Key::FindPrevious, Command::FindPrevious, has),
            action(app, Key::UseSelection, Command::UseSelection, selection),
            Item::Separator,
            action(app, Key::GoToLine, Command::GoToLine, has),
            Item::Separator,
            action(app, Key::QuickOpen, Command::QuickOpen, true),
            action(app, Key::GoToSymbol, Command::Symbols, has),
            action(app, Key::SearchFolder, Command::SearchFolder, true),
        ],
        Bar::Format => {
            let language = doc.map_or(Language::Plain, Document::language);
            let ending = doc.map_or(Ending::Lf, |doc| doc.ending);
            vec![
                labelled(format!("{} {}", Key::Syntax.text(app.lang), language.name()), Command::Syntax, has, None),
                Item::Separator,
                check(app, Key::SpacesForTabs, Command::ToggleSpaces, app.prefs.spaces, true),
                labelled(
                    format!("{}: {}", Key::TabWidth.text(app.lang), app.prefs.tab_width),
                    Command::TabWidths,
                    true,
                    None,
                ),
                check(app, Key::AutoIndent, Command::ToggleAutoIndent, app.prefs.auto_indent, true),
                check(app, Key::AutoPair, Command::ToggleAutoPair, app.prefs.auto_pair, true),
                Item::Separator,
                labelled(format!("{} {}", Key::LineEndings.text(app.lang), ending.name()), Command::Endings, has, None),
                check(app, Key::TrimOnSave, Command::ToggleTrimOnSave, app.prefs.trim_on_save, true),
                Item::Separator,
                action(app, Key::Statistics, Command::Statistics, has),
            ]
        }
        Bar::Help => {
            vec![action(app, Key::Shortcuts, Command::Shortcuts, true), action(app, Key::About, Command::About, true)]
        }
    }
}

#[must_use]
#[allow(clippy::too_many_lines)] // Every popup menu as one table, the source for both drawing and keyboard order
pub fn menu_items(app: &App, kind: &MenuKind) -> Vec<Item> {
    match kind {
        MenuKind::Bar(bar) => bar_items(app, *bar),
        MenuKind::Compact => Bar::ALL
            .into_iter()
            .flat_map(|bar| std::iter::once(Item::heading(bar.key().text(app.lang))).chain(bar_items(app, bar)))
            .collect(),
        MenuKind::Context => {
            let doc = app.doc();
            let selection = doc.is_some_and(|doc| !doc.selected().is_empty());
            vec![
                action(app, Key::Undo, Command::Undo, doc.is_some_and(|doc| doc.history.can_undo())),
                action(app, Key::Redo, Command::Redo, doc.is_some_and(|doc| doc.history.can_redo())),
                Item::Separator,
                action(app, Key::Cut, Command::Cut, selection),
                action(app, Key::Copy, Command::Copy, selection),
                action(app, Key::Paste, Command::Paste, true),
                action(app, Key::SelectAll, Command::SelectAll, true),
                Item::Separator,
                action(app, Key::ToggleComment, Command::ToggleComment, true),
                action(app, Key::DuplicateLine, Command::DuplicateLine, true),
                action(app, Key::Uppercase, Command::Case(Case::Upper), true),
                action(app, Key::Lowercase, Command::Case(Case::Lower), true),
                Item::Separator,
                action(app, Key::UseSelection, Command::UseSelection, selection),
                labelled(Key::Syntax.text(app.lang).to_string(), Command::Syntax, true, None),
            ]
        }
        MenuKind::Syntax => {
            let doc = app.doc();
            let pinned = doc.and_then(|doc| doc.pinned);
            let detected = doc.map_or(Language::Plain, |doc| doc.detected);
            let mut items = vec![
                labelled(
                    format!("{} ({})", Key::Automatic.text(app.lang), detected.name()),
                    Command::SetLanguage(None),
                    true,
                    Some(pinned.is_none()),
                ),
                Item::Separator,
            ];
            items.extend(Language::ALL.into_iter().map(|language| {
                labelled(
                    language.name().to_string(),
                    Command::SetLanguage(Some(language)),
                    true,
                    Some(pinned == Some(language)),
                )
            }));
            items
        }
        MenuKind::TabWidths => {
            let mut items: Vec<Item> = [2, 4, 8]
                .into_iter()
                .map(|width| {
                    labelled(width.to_string(), Command::TabWidth(width), true, Some(app.prefs.tab_width == width))
                })
                .collect();
            items.push(Item::Separator);
            items.push(check(app, Key::SpacesForTabs, Command::ToggleSpaces, app.prefs.spaces, true));
            items
        }
        MenuKind::Endings => {
            let ending = app.doc().map_or(Ending::Lf, |doc| doc.ending);
            vec![
                labelled(
                    Key::UnixEndings.text(app.lang).to_string(),
                    Command::SetEnding(Ending::Lf),
                    true,
                    Some(ending == Ending::Lf),
                ),
                labelled(
                    Key::WindowsEndings.text(app.lang).to_string(),
                    Command::SetEnding(Ending::CrLf),
                    true,
                    Some(ending == Ending::CrLf),
                ),
            ]
        }
        MenuKind::Recent => {
            if app.recent.is_empty() {
                return vec![labelled(Key::NoRecent.text(app.lang).to_string(), Command::ClearRecent, false, None)];
            }
            let mut items: Vec<Item> = app
                .recent
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let name = path
                        .file_name()
                        .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
                    labelled(name, Command::OpenRecent(index), true, None)
                })
                .collect();
            items.push(Item::Separator);
            items.push(labelled(Key::ClearRecent.text(app.lang).to_string(), Command::ClearRecent, true, None));
            items
        }
        MenuKind::Tree(_) => vec![
            labelled(Key::OpenItem.text(app.lang).to_string(), Command::TreeOpen, true, None),
            labelled(Key::ShowInFiles.text(app.lang).to_string(), Command::TreeShow, true, None),
            labelled(Key::CopyPath.text(app.lang).to_string(), Command::TreeCopyPath, true, None),
            Item::Separator,
            labelled(Key::Refresh.text(app.lang).to_string(), Command::RefreshTree, true, None),
        ],
    }
}

#[must_use]
pub fn menu_actions(app: &App, kind: &MenuKind) -> Vec<Command> {
    menu_items(app, kind).iter().filter_map(Item::message).copied().collect()
}

fn opens_more(command: Command) -> bool {
    matches!(command, Command::Recent | Command::Syntax | Command::TabWidths | Command::Endings)
}

fn submenu_of(command: Command) -> Option<MenuKind> {
    match command {
        Command::Recent => Some(MenuKind::Recent),
        Command::Syntax => Some(MenuKind::Syntax),
        Command::TabWidths => Some(MenuKind::TabWidths),
        Command::Endings => Some(MenuKind::Endings),
        _ => None,
    }
}

fn nested(app: &App, items: Vec<Item>) -> Vec<Item> {
    items
        .into_iter()
        .filter(|item| !matches!(item, Item::Heading(_)))
        .map(|item| {
            if let Item::Action { label, message, enabled, .. } = &item
                && let Some(kind) = submenu_of(*message)
            {
                let children = nested(app, menu_items(app, &kind));
                let enabled = *enabled && !children.is_empty();
                return Item::submenu(label.trim_end_matches('…'), children).enabled(enabled);
            }
            item
        })
        .collect()
}

#[must_use]
pub(crate) fn export_menu(app: &App) -> (Vec<glide_app::menubar::Entry>, Vec<Command>) {
    let menus: Vec<glide_sdk::menu::Menu<Command>> = Bar::ALL
        .into_iter()
        .map(|bar| glide_sdk::menu::Menu::new(bar.key().text(app.lang), nested(app, bar_items(app, bar))))
        .collect();
    glide_sdk::menu::export(&menus)
}

fn menu_layer<'a>(app: &'a App, menu: &'a Menu) -> Element<'a, Message> {
    use glide_sdk::menu::{self, Event};
    let progress = menu.reveal.value(motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let items = menu_items(app, &menu.kind);
    let actionable = menu::actionable(&items);
    let selected = menu.selected.and_then(|at| actionable.get(at).copied());
    let width = menu::width(&items, MENU_WIDTH);
    let pops = !matches!(menu.kind, MenuKind::Bar(_) | MenuKind::Compact);
    let (at, size) = menu::place(menu.at, menu::height(&items), width, app.window.size, pops);
    let rows = menu::rows(&paint, &items, selected)
        .into_iter()
        .zip(&items)
        .enumerate()
        .map(|(index, (row, item))| {
            let hovered = actionable.iter().position(|row| *row == index);
            let command = item.message().copied();
            row.map(move |event| match (event, hovered, command) {
                (Event::Hovered(_), Some(position), _) => Message::MenuHovered(position),
                (Event::Picked(_), _, Some(command)) => Message::Command(command),
                (Event::Dismiss, ..) => Message::Dismiss,
                _ => Message::Noop,
            })
        })
        .collect();
    menu::layer(&paint, rows, at, size, progress, Some(Message::Dismiss))
}

fn shortcut_rows(app: &App, paint: &Paint) -> Element<'static, Message> {
    let sections = Bar::ALL
        .into_iter()
        .map(|bar| {
            let items = bar_items(app, bar)
                .into_iter()
                .filter_map(|item| match item {
                    Item::Action { label, shortcut: Some(shortcut), .. } => Some((label, shortcut)),
                    _ => None,
                })
                .collect();
            (bar.key().text(app.lang).to_string(), items)
        })
        .collect();
    shortcut_list(paint, sections)
}

type Choices<'a> = Vec<(&'a str, Tone, Option<Message>)>;

fn finder_layer<'a>(app: &'a App, finder: &'a crate::finder::Finder) -> Element<'a, Message> {
    use crate::finder::Mode;
    let progress = finder.reveal.value(motion::t2()) as f32;
    let lang = app.lang;
    let hint = match finder.mode {
        Mode::Files => Key::QuickOpenHint,
        Mode::Symbols => Key::SymbolHint,
        Mode::Search => Key::SearchHint,
    };
    let input = glide_sdk::widget::search_input(
        &app.paint.faded(progress),
        text_input(hint.text(lang), &finder.query)
            .id(FINDER_ID)
            .on_input(|query| Message::Finder(FinderMessage::Input(query)))
            .on_submit(Message::Finder(FinderMessage::Submit)),
        None,
        "",
    );
    let empty = match (finder.busy, finder.mode, finder.query.is_empty()) {
        (true, _, _) => Key::Searching,
        (_, Mode::Symbols, _) => Key::NoSymbols,
        (_, Mode::Search, true) => Key::SearchHint,
        _ => Key::NoResults,
    };
    glide_sdk::overlay::finder(
        &app.paint,
        progress,
        input,
        &finder.rows(),
        finder.selected,
        empty.text(lang),
        |index| Message::Finder(FinderMessage::Pick(index)),
        Message::Finder(FinderMessage::Close),
        app.window.size,
        if app.edge_to_edge() { 0.0 } else { R3 },
    )
}

fn dialog<'a>(app: &'a App, dialog: &'a Dialog) -> Element<'a, Message> {
    use glide_sdk::overlay::{dialog_buttons, dialog_card, modal};
    let progress = dialog.reveal.value(motion::t2()) as f32;
    let paint = app.paint.faded(progress);
    let (title, body, choices, width) = dialog_parts(app, &paint, dialog);
    let card = dialog_card(&paint, &title, Some(body), dialog_buttons(&paint, choices, None), width, app.window.size);
    modal(&paint, progress, card, None, if app.edge_to_edge() { 0.0 } else { R3 })
}

fn dialog_parts<'a>(
    app: &'a App, paint: &Paint, dialog: &'a Dialog,
) -> (String, Element<'a, Message>, Choices<'a>, f32) {
    let paint = *paint;
    let lang = app.lang;
    let close = || vec![(Key::Close.text(lang), Tone::Accent, Some(Message::Dialog(Answer::Cancel)))];
    match &dialog.kind {
        DialogKind::Unsaved { docs, .. } => {
            let title = match docs.as_slice() {
                [id] => {
                    let name =
                        app.docs.iter().find(|doc| doc.id == *id).map(|doc| app.doc_name(doc)).unwrap_or_default();
                    format!("{} «{name}»?", Key::UnsavedOne.text(lang))
                }
                _ => Key::UnsavedMany.text(lang).to_string(),
            };
            let mut body = column![text(Key::UnsavedBody.text(lang), TextStyle::CALL).color(paint.fg_2())].spacing(SP1);
            if docs.len() > 1 {
                for doc in app.docs.iter().filter(|doc| docs.contains(&doc.id)) {
                    body = body.push(text(format!("• {}", app.doc_name(doc)), TextStyle::CALL));
                }
            }
            (
                title,
                body.into(),
                vec![
                    (Key::DontSave.text(lang), Tone::Danger, Some(Message::Dialog(Answer::Discard))),
                    (Key::Cancel.text(lang), Tone::Neutral, Some(Message::Dialog(Answer::Cancel))),
                    (Key::Save.text(lang), Tone::Accent, Some(Message::Dialog(Answer::Save))),
                ],
                420.0,
            )
        }
        DialogKind::About => (
            Key::AppName.text(lang).to_string(),
            column![
                text(Key::AboutBody.text(lang), TextStyle::CALL).color(paint.fg_2()),
                text(format!("glide-text {}", env!("CARGO_PKG_VERSION")), TextStyle::CAP).color(paint.fg_3()),
            ]
            .spacing(SP2)
            .into(),
            close(),
            400.0,
        ),
        DialogKind::Statistics => {
            let rows = app
                .doc()
                .map(|doc| {
                    let stats = crate::edit::stats(&doc.text);
                    [
                        (Key::Lines, stats.lines.to_string()),
                        (Key::WordsLong, stats.words.to_string()),
                        (Key::Characters, stats.characters.to_string()),
                        (Key::CharactersNoSpaces, stats.characters_without_spaces.to_string()),
                        (Key::Bytes, stats.bytes.to_string()),
                        (Key::Encoding, format!("{} · {}", doc.encoding.name(), doc.ending.name())),
                    ]
                    .into_iter()
                    .map(|(key, value)| (key.text(lang).to_string(), value))
                    .collect()
                })
                .unwrap_or_default();
            (Key::Statistics.text(lang).trim_end_matches('…').to_string(), stat_rows(&paint, rows), close(), 360.0)
        }
        DialogKind::Shortcuts => (
            Key::Shortcuts.text(lang).to_string(),
            scrollable(&paint, container(shortcut_rows(app, &paint)).padding(Padding::from([0.0, SP2]).left(0.0)))
                .height(Length::Fixed((app.window.size.height - 220.0).clamp(120.0, 520.0)))
                .into(),
            close(),
            460.0,
        ),
    }
}

fn notice<'a>(app: &'a App, message: &'a str, at: std::time::Instant) -> Element<'a, Message> {
    let pill = glide_sdk::overlay::toast_pill(&app.paint, App::reveal(at), message, None, None, app.window.size);
    let bottom = if app.status_visible() { STATUS } else { 0.0 };
    container(pill).padding(Padding::ZERO.bottom(bottom)).into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let paint = app.paint;
    let mut main = column![];
    if app.find.is_some() {
        main = main.push(find_bar(app)).push(rule(&paint));
    }
    if app.goto.is_some() {
        main = main.push(glide_sdk::widget::arriving(goto_bar(app))).push(rule(&paint));
    }
    if let Some(bar) = infobar(app) {
        main = main.push(glide_sdk::widget::arriving(bar)).push(rule(&paint));
    }
    main = main.push(editor(app));
    let bottom_right = if app.edge_to_edge() || app.status_visible() { 0.0 } else { R3 };
    let bottom_left = if app.sidebar_visible() { 0.0 } else { bottom_right };
    let main = pane(paint.view_bg(), Radius::default().bottom_right(bottom_right).bottom_left(bottom_left), main);
    let main: Element<'_, Message> =
        if app.status_floating() { iced::widget::stack![main, status_islands(app)].into() } else { main.into() };
    let mut middle = row![];
    if app.sidebar_visible() {
        middle = middle.push(glide_sdk::widget::arriving(sidebar(app))).push(vertical_rule(&paint));
    }
    middle = middle.push(main);
    let mut body = column![header(app), rule(&paint), middle.height(Fill)];
    if app.status_visible() {
        body = body.push(rule(&paint)).push(status_bar(app));
    }
    let body = mouse_area(body).on_move(Message::Pointer);
    let mut layers: Vec<Element<'_, Message>> = vec![body.into()];
    if !app.edge_to_edge() {
        layers.push(resize_handles(Message::Window));
    }
    if let Some((text, at)) = &app.notice {
        layers.push(notice(app, text, *at));
    }
    if let Some(menu) = &app.menu {
        layers.push(menu_layer(app, menu));
    }
    if let Some(finder) = &app.finder {
        layers.push(finder_layer(app, finder));
    }
    if let Some(dialog) = app.dialog.as_ref().or(app.closing.as_ref()) {
        layers.push(self::dialog(app, dialog));
    }
    iced::widget::stack(layers).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_enabled_menu_row_is_reachable_from_the_keyboard() {
        let (app, _) = App::launch(None, &[]);
        for bar in Bar::ALL {
            let items = bar_items(&app, bar);
            let enabled = items.iter().filter(|item| matches!(item, Item::Action { enabled: true, .. })).count();
            assert_eq!(menu_actions(&app, &MenuKind::Bar(bar)).len(), enabled);
            assert!(enabled > 0, "{bar:?} has nothing to do");
        }
        let compact = menu_items(&app, &MenuKind::Compact);
        assert_eq!(compact.iter().filter(|item| matches!(item, Item::Heading(_))).count(), Bar::ALL.len());
    }

    #[test]
    fn the_menu_bar_collapses_on_narrow_windows() {
        let (mut app, _) = App::launch(None, &[]);
        app.window.size = iced::Size::new(1200.0, 700.0);
        assert!(!compact(&app));
        assert!(bar_x(&app, Bar::Edit) > bar_x(&app, Bar::File));
        app.window.size = iced::Size::new(420.0, 400.0);
        assert!(compact(&app));
        assert_eq!(bar_x(&app, Bar::Help), bar_x(&app, Bar::File));
    }

    #[test]
    fn the_exported_menu_bar_mirrors_the_window_menus_with_real_submenus() {
        use glide_app::menubar::Entry;
        fn walk<'a>(entries: &'a [Entry], actions: &mut usize, syntax: &mut Option<&'a [Entry]>) {
            for entry in entries {
                match entry {
                    Entry::Action { id, .. } => {
                        *actions += 1;
                        assert_eq!(usize::try_from(*id).unwrap(), *actions, "ids follow the command table");
                    }
                    Entry::Submenu { children, .. } => {
                        if children.len() > Language::ALL.len() {
                            *syntax = Some(children);
                        }
                        walk(children, actions, syntax);
                    }
                    Entry::Separator => {}
                }
            }
        }
        let (app, _) = App::launch(None, &[]);
        let (menu, commands) = export_menu(&app);
        assert_eq!(menu.len(), Bar::ALL.len());
        let mut actions = 0;
        let mut syntax = None;
        walk(&menu, &mut actions, &mut syntax);
        assert_eq!(actions, commands.len());
        assert!(syntax.is_some(), "the highlight mode list is a submenu, not an in-window popup");
        assert!(!commands.iter().any(|command| opens_more(*command)), "commands that open popups are expanded");
    }
}
