use std::path::{Path, PathBuf};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{elide, elide_end, icon_segmented_width, measure};
use iced::widget::{container, mouse_area, row, text_input};
use iced::{Element, Fill, Length};

use super::{CONTROL, HEADER_HEIGHT, SIDEBAR_WIDTH};
use crate::app::{App, Hover, MenuKind, Message, PATH_ID, SEARCH_ID, ViewMode};
use crate::fs::home_dir;
use crate::i18n::Key;
use crate::icons::Icon;
use crate::theme::{self, SP2, SP3};

pub const SEARCH_WIDTH: f32 = 220.0;
const NARROW_SEARCH: f32 = 160.0;
const WIDE_WINDOW: f32 = 1000.0;
pub const WINDOW_CONTROL: f32 = glide_sdk::window::CONTROL;
pub const GAP: f32 = SP2;
pub(crate) const INSET: f32 = SP3;
const BUTTON: f32 = theme::CONTROL;
const LEADING: f32 = 2.0 * BUTTON + GAP;

#[must_use]
pub fn search_width(app: &App) -> f32 {
    if app.chooser.is_some() || app.window_size().width >= WIDE_WINDOW { SEARCH_WIDTH } else { NARROW_SEARCH }
}

fn trailing(app: &App) -> f32 {
    let inspector = if app.inspector_fits() { BUTTON + GAP } else { 0.0 };
    search_width(app) + icon_segmented_width(2) + BUTTON + inspector + WINDOW_CONTROL + 2.0 + 4.0 * GAP
}
const CRUMB_CHROME: f32 = 2.0 * SP2 + 16.0;
const CRUMB_GLYPH: f32 = 13.0 + theme::SP1;
const CRUMB_LABEL: f32 = 200.0;
const CURRENT: TextStyle = TextStyle { weight: 600, ..TextStyle::CALL };

#[must_use]
pub fn search_title(label: &str, query: &str, room: f32) -> String {
    let frame = measure(&format!("{label} «»"), TextStyle::WIN_TITLE);
    let shown = elide_end(query, room - frame, TextStyle::WIN_TITLE);
    if shown.is_empty() { label.to_string() } else { format!("{label} «{shown}»") }
}

#[must_use]
pub fn maximize_button(app: &App) -> Element<'_, Message> {
    let paint = super::paint(&app.theme);
    glide_sdk::window::maximize_button(
        &paint,
        &app.window(),
        Key::Maximize.text(app.lang),
        Message::Window(glide_sdk::window::Action::ToggleMaximize),
    )
}

#[must_use]
pub(crate) fn bar_background(app: &App) -> iced::Color {
    if app.is_idle() { app.theme.bar_bg_idle() } else { app.theme.bar_bg() }
}

#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    use glide_sdk::widget::{Size, icon_segmented, sized_icon_button};
    let paint = super::paint(&app.theme);
    let lang = app.lang;
    let back = app.can_go_back().then_some(Message::Back);
    let forward = app.can_go_forward().then_some(Message::Forward);
    let nav = row![
        sized_icon_button(&paint, Icon::ChevronLeft, Key::Back.text(lang), back, false, Size::Medium),
        sized_icon_button(&paint, Icon::ChevronRight, Key::Forward.text(lang), forward, false, Size::Medium),
    ]
    .spacing(GAP);
    let views = icon_segmented(
        &paint,
        &[
            (ViewMode::Grid, Icon::Grid, Key::ViewGrid.text(lang)),
            (ViewMode::List, Icon::List, Key::ViewList.text(lang)),
        ],
        app.view,
        Message::SetView,
    );
    let options_open = app.menu.as_ref().is_some_and(|menu| menu.kind == MenuKind::Options);
    let location = path_field(app).unwrap_or_else(|| crumbs_row(app));
    let mut header = glide_sdk::window::Header::new("")
        .labels("", Key::Close.text(lang))
        .height(HEADER_HEIGHT)
        .without_maximize()
        .corners(theme::window_corners(theme::Side::TopRight, app.corner()))
        .leading(nav)
        .center(location)
        .trailing(search_field(app))
        .trailing(views);
    if app.inspector_fits() {
        header = header.trailing(sized_icon_button(
            &paint,
            glide_sdk::widget::Icon::Inspector,
            Key::Inspector.text(lang),
            Some(Message::ToggleInspector),
            app.inspector_shown(),
            Size::Medium,
        ));
    }
    let header = header.trailing(sized_icon_button(
        &paint,
        Icon::More,
        Key::MoreActions.text(lang),
        Some(Message::ToggleOptions),
        options_open,
        Size::Medium,
    ));
    glide_sdk::window::header(&paint, &app.window(), header, Message::Window, Message::Quit, 0.0)
}

#[must_use]
pub(crate) fn path_field(app: &App) -> Option<Element<'_, Message>> {
    let path = app.path_edit.as_ref()?;
    let input = text_input(Key::PathPlaceholder.text(app.lang), path)
        .id(PATH_ID)
        .on_input(Message::PathChanged)
        .on_submit(Message::PathSubmitted);
    Some(glide_sdk::widget::rising(glide_sdk::widget::field(&super::paint(&app.theme), input)))
}

pub fn search_field(app: &App) -> Element<'_, Message> {
    let query = app.search.as_ref().map_or("", |search| search.query.as_str());
    let hint = if search_width(app) < SEARCH_WIDTH { Key::Search } else { Key::SearchHere };
    let input = text_input(hint.text(app.lang), query).id(SEARCH_ID).on_input(Message::SearchChanged);
    let clear = (!query.is_empty()).then(|| Message::SearchChanged(String::new()));
    let field =
        glide_sdk::widget::search_input(&super::paint(&app.theme), input, clear, Key::ClearSearch.text(app.lang));
    container(field).width(Length::Fixed(search_width(app))).into()
}

pub struct Crumb {
    pub glyph: Option<Icon>,
    pub label: String,
    pub path: PathBuf,
}

#[must_use]
pub fn crumb_room(app: &App) -> f32 {
    if app.chooser.is_some() {
        return app.pane.width - SEARCH_WIDTH - 3.0 * CONTROL - 4.0 * GAP - 2.0 * SP3;
    }
    app.window_size().width - SIDEBAR_WIDTH - 1.0 - 2.0 * SP2 - LEADING - trailing(app) - 2.0 * GAP
}

#[must_use]
pub fn trail(app: &App) -> Vec<Crumb> {
    let mut crumbs = crumbs(app);
    let budget = crumb_room(app);
    if crumbs.iter().map(width).sum::<f32>() > budget {
        let mut hidden = None;
        while crumbs.len() > 2 && CRUMB_CHROME + CRUMB_GLYPH + crumbs.iter().map(width).sum::<f32>() > budget {
            hidden = Some(crumbs.remove(1).path);
        }
        if let Some(path) = hidden {
            crumbs.insert(1, Crumb { glyph: Some(Icon::More), label: String::new(), path });
        }
    }
    if crumbs.len() > 2 && crumbs.iter().map(width).sum::<f32>() > budget {
        let rest: f32 = crumbs[1..].iter().map(width).sum();
        let root = &mut crumbs[0];
        let label_room = budget - rest - (width(root) - measure(&root.label, CURRENT));
        root.label = elide(&root.label, label_room, CURRENT);
        if measure(&root.label, CURRENT) > label_room {
            root.label.clear();
        }
    }
    if crumbs.len() > 1 && crumbs.iter().map(width).sum::<f32>() > budget {
        crumbs.drain(..crumbs.len() - 1);
    }
    if let [only] = crumbs.as_mut_slice()
        && face(only) > budget
    {
        only.label = elide(&only.label, budget - (face(only) - measure(&only.label, CURRENT)), CURRENT);
    }
    crumbs
}

fn crumbs(app: &App) -> Vec<Crumb> {
    let current = &app.current;
    let trash = current.ancestors().find(|dir| app.trash.is_files_dir(dir)).map(Path::to_path_buf);
    let (glyph, label, root, first) = if let Some(files) = trash {
        (Icon::Trash, Key::Trash.text(app.lang).to_string(), files, app.trash.files())
    } else if let Some(home) = home_dir().filter(|home| current.starts_with(home)) {
        let name = home
            .file_name()
            .map_or_else(|| Key::Home.text(app.lang).to_string(), |name| name.to_string_lossy().into_owned());
        (Icon::Home, name, home.clone(), home)
    } else {
        (Icon::Drive, Key::Root.text(app.lang).to_string(), PathBuf::from("/"), PathBuf::from("/"))
    };
    let mut path = root.clone();
    let mut crumbs = vec![Crumb { glyph: Some(glyph), label, path: first }];
    for component in current.strip_prefix(&root).unwrap_or(Path::new("")) {
        path.push(component);
        let label = elide(&app.location_name(&path), CRUMB_LABEL, TextStyle::CALL);
        crumbs.push(Crumb { glyph: None, label, path: path.clone() });
    }
    crumbs
}

fn face(crumb: &Crumb) -> f32 {
    let glyph = if crumb.glyph.is_some() { CRUMB_GLYPH } else { 0.0 };
    measure(&crumb.label, CURRENT) + 2.0 * SP2 + glyph
}

fn width(crumb: &Crumb) -> f32 {
    face(crumb) + CRUMB_CHROME - 2.0 * SP2
}

#[must_use]
pub(crate) fn crumbs_row(app: &App) -> Element<'_, Message> {
    let t = &app.theme;
    let paint = super::paint(t);
    let crumbs = trail(app);
    let last = crumbs.len() - 1;
    let items = crumbs.into_iter().enumerate().map(|(index, crumb)| {
        let key = Hover::Crumb(crumb.path.clone());
        let target = MenuKind::Location(crumb.path.clone());
        let mut face = glide_sdk::widget::Crumb::new(&paint, crumb.label)
            .current(index == last)
            .hovered(app.is_drop_target(&crumb.path))
            .on_press(Some(Message::Navigate(crumb.path)));
        if let Some(glyph) = crumb.glyph {
            face = face.glyph(glyph, t.fg_2());
        }
        mouse_area(Element::from(face))
            .on_right_press(Message::MenuTarget(target))
            .on_enter(Message::Hover(key.clone(), true))
            .on_exit(Message::Hover(key, false))
            .into()
    });
    container(glide_sdk::widget::crumb_trail(&paint, items.collect())).width(Fill).clip(true).into()
}

#[cfg(test)]
mod tests {
    use super::{crumb_room, face, trail, width};
    use glide_sdk::window::{Frame, Window};

    #[test]
    fn the_current_folder_fits_the_header_of_the_smallest_window() {
        let (mut app, _) = crate::App::new();
        app.follow_window(Window { size: iced::Size::new(720.0, 480.0), focused: true, frame: Frame::Floating });
        app.current = std::path::PathBuf::from("/Документи з дуже довгою назвою для перевірки");
        let crumbs = trail(&app);
        assert_eq!(crumbs.len(), 1);
        assert!(crumbs[0].label.contains('…'));
        assert!(face(&crumbs[0]) <= crumb_room(&app));
    }

    #[test]
    fn collapsed_paths_fit_and_keep_the_root_when_its_icon_and_ellipsis_fit() {
        let (mut app, _) = crate::App::new();
        app.current = "/srv/projects/customer-archive/two-thousand-twenty-six/quarterly-reports/october".into();
        for window_width in (900..=1400).step_by(10) {
            app.follow_window(Window {
                size: iced::Size::new(window_width as f32, 600.0),
                focused: true,
                frame: Frame::Floating,
            });
            let crumbs = trail(&app);
            assert!(crumbs.iter().map(width).sum::<f32>() <= crumb_room(&app));
            if crumbs.len() > 1 {
                assert_eq!(crumbs[0].path, std::path::Path::new("/"));
                assert_eq!(crumbs.last().map(|crumb| &crumb.path), Some(&app.current));
            }
        }
    }
}
