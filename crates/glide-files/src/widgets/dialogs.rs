use std::path::{Path, PathBuf};
use std::time::SystemTime;

use iced::widget::{
    Column, button, column, container, float, image, opaque, row, rule, scrollable, space, svg, text, text_input,
};
use iced::{Alignment, Element, Fill, Length, Padding, Vector};

use super::content::{FIT_SLACK, breakable};
use super::header::SEMIBOLD_SPREAD;
use super::{Ring, file_icon, fit_lines, pressable, ringed, text_width};
use crate::app::{App, Dialog, Field, MODAL_ID, Message, Modal, Properties};
use crate::fs::jobs::{Choice, Question};
use crate::fs::{Target, format_bytes, format_time, home_dir, permissions_string};
use crate::i18n::{self, Key, Lang};
use crate::icons::Icon;
use crate::native_icons::Art;
use crate::theme::{self, ThemeState, Tone};

const WIDTH: f32 = 400.0;
const WIDE: f32 = 520.0;
const LABEL_WIDTH: f32 = 112.0;
const BUTTON_TEXT: f32 = 13.5;
const ICON: f32 = 56.0;
const NAME_LINES: usize = 3;

#[derive(Clone, Copy)]
struct Ctx<'a> {
    app: &'a App,
    alpha: f32,
}

impl Ctx<'_> {
    fn theme(self) -> ThemeState {
        self.app.theme.faded(self.alpha)
    }
}

#[must_use]
pub fn view<'a>(app: &'a App, modal: &'a Modal, progress: f32, live: bool) -> Element<'a, Message> {
    let c = Ctx { app, alpha: progress };
    let focus = modal.focused();
    let body = match &modal.dialog {
        Dialog::NewFolder => input(c, modal, (Key::NewFolderTitle, Key::NamePrompt), Key::Create),
        Dialog::NewFile => input(c, modal, (Key::NewFileTitle, Key::NamePrompt), Key::Create),
        Dialog::NewFromTemplate(_) => input(c, modal, (Key::NewDocumentTitle, Key::NamePrompt), Key::Create),
        Dialog::Tags(_) => input(c, modal, (Key::TagsTitle, Key::TagsPrompt), Key::Save),
        Dialog::Rename(_) => input(c, modal, (Key::RenameTitle, Key::NamePrompt), Key::RenameConfirm),
        Dialog::Delete(paths) => {
            let prompt = if paths.len() == 1 { Key::DeletePromptOne } else { Key::DeletePrompt };
            confirm(c, Key::DeleteTitle, prompt, Some(subject(app, paths)), Key::DeletePermanently, focus)
        }
        Dialog::EmptyTrash => confirm(c, Key::EmptyTrashTitle, Key::EmptyTrashPrompt, None, Key::EmptyTrash, focus),
        Dialog::Quit => confirm(c, Key::QuitTitle, Key::QuitPrompt, None, Key::QuitConfirm, focus),
        Dialog::Replace(target) => confirm(
            c,
            Key::ChooserReplaceTitle,
            Key::ChooserReplacePrompt,
            Some(subject(app, std::slice::from_ref(target))),
            Key::Replace,
            focus,
        ),
        Dialog::Properties(properties) => details(c, properties),
        Dialog::Repair(object) => confirm(
            c,
            Key::RepairTitle,
            Key::RepairPrompt,
            app.volume(object).map(|volume| format!("«{}»", volume.name(app.lang))),
            Key::RepairAction,
            focus,
        ),
        Dialog::Conflict { question, for_all, .. } => conflict(c, question, *for_all, focus),
        Dialog::Unlock(object) => unlock(c, modal, object),
        Dialog::OpenWith { targets, mime, chosen, always } => {
            open_with(c, modal, targets, mime.as_deref(), chosen, *always)
        }
    };
    let width = card_width(&modal.dialog, app.lang);
    let rise = (1.0 - progress) * 12.0;
    let t = c.theme();
    let room = app.window_size().height - 2.0 * theme::SP4;
    let body = scrollable(body).width(Fill).style(move |_, status| theme::scroller(&t, status));
    let card = float(
        container(body)
            .width(Length::Fixed(width))
            .max_height(if room > 0.0 { room } else { f32::INFINITY })
            .padding(theme::SP5)
            .style(move |_| theme::popover(&t)),
    )
    .translate(move |_, _| Vector::new(0.0, rise));
    let layer = container(card).center(Fill).style(move |_| theme::scrim(progress));
    if live { opaque(layer) } else { layer.into() }
}

#[must_use]
pub fn card_width(dialog: &Dialog, lang: Lang) -> f32 {
    match dialog {
        Dialog::OpenWith { .. } => WIDE,
        Dialog::Conflict { question, .. } => {
            let labels = std::iter::once(Key::StopJob).chain(choices(question.folders).map(|(key, _)| key));
            let row: f32 = labels
                .map(|key| text_width(key.text(lang), BUTTON_TEXT) / SEMIBOLD_SPREAD + 2.0 * theme::SP4 + theme::SP2)
                .sum::<f32>()
                + 8.0 * theme::RING;
            WIDE.max(row + theme::SP4 + 2.0 * theme::SP5)
        }
        _ => WIDTH,
    }
}

#[must_use]
pub fn shown_name(name: &str) -> String {
    let room = (WIDTH - 2.0 * theme::SP5 - ICON - theme::SP4) * SEMIBOLD_SPREAD * FIT_SLACK;
    breakable(&fit_lines(name, room, theme::T3, NAME_LINES))
}

#[must_use]
pub fn choices(folders: bool) -> [(Key, Choice); 3] {
    if folders {
        [(Key::Skip, Choice::Skip), (Key::KeepBothFolders, Choice::KeepBoth), (Key::Merge, Choice::Merge)]
    } else {
        [(Key::Skip, Choice::Skip), (Key::KeepBoth, Choice::KeepBoth), (Key::Replace, Choice::Replace)]
    }
}

fn title(label: &str) -> Element<'_, Message> {
    text(label).size(theme::T3).font(theme::SEMIBOLD).wrapping(WRAP).into()
}

const WRAP: text::Wrapping = text::Wrapping::WordOrGlyph;

fn action(c: Ctx<'_>, label: Key, tone: Tone, message: Option<Message>) -> Element<'_, Message> {
    let t = c.theme();
    pressable(
        button(container(text(label.text(c.app.lang)).size(BUTTON_TEXT).font(theme::SEMIBOLD)).center_y(Fill))
            .on_press_maybe(message)
            .height(Length::Fixed(theme::CONTROL))
            .padding(Padding::from([0.0, theme::SP4]))
            .style(move |_, status| theme::action(&t, tone, status)),
    )
}

fn focusable(c: Ctx<'_>, label: Key, tone: Tone, message: Message, focused: bool) -> Element<'_, Message> {
    ringed(&c.theme(), action(c, label, tone, Some(message)), if focused { Ring::Focus } else { Ring::Off })
}

fn buttons<'a>(c: Ctx<'a>, confirm: Element<'a, Message>) -> Element<'a, Message> {
    row![space::horizontal(), action(c, Key::Cancel, Tone::Neutral, Some(Message::ModalCancel)), confirm,]
        .spacing(theme::SP2)
        .into()
}

fn field<'a>(c: Ctx<'a>, modal: &'a Modal, placeholder: &str, secure: bool) -> Element<'a, Message> {
    let t = c.theme();
    let input = text_input(placeholder, &modal.input)
        .id(MODAL_ID)
        .on_input(Message::ModalInput)
        .on_submit(Message::ModalSubmit)
        .secure(secure)
        .padding(Padding::from([7.0, theme::SP3]))
        .size(BUTTON_TEXT)
        .style(move |_, status| {
            let mut style = theme::field(&t, status);
            if modal.error.is_some() {
                style.border.color = t.danger();
            }
            style
        });
    let ring = if modal.error.is_some() {
        Ring::Invalid
    } else if c.app.focused_field() == Some(Field::Modal) {
        Ring::Focus
    } else {
        Ring::Off
    };
    ringed(&t, input, ring)
}

fn input<'a>(c: Ctx<'a>, modal: &'a Modal, (heading, prompt): (Key, Key), submit: Key) -> Element<'a, Message> {
    let lang = c.app.lang;
    let valid = prompt == Key::TagsPrompt || !modal.input.trim().is_empty();
    let mut body = column![
        title(heading.text(lang)),
        text(prompt.text(lang)).size(theme::CAP).color(c.theme().fg_2()),
        field(c, modal, "", false),
    ]
    .spacing(theme::SP2);
    if let Some(error) = &modal.error {
        body = body.push(text(error).size(theme::CAP).color(c.theme().danger()).wrapping(WRAP));
    }
    body.push(space().height(theme::SP2))
        .push(buttons(c, action(c, submit, Tone::Primary, valid.then_some(Message::ModalSubmit))))
        .into()
}

fn unlock<'a>(c: Ctx<'a>, modal: &'a Modal, object: &'a str) -> Element<'a, Message> {
    let lang = c.app.lang;
    let name = c.app.volume(object).map(|volume| format!("«{}»", volume.name(lang))).unwrap_or_default();
    column![
        title(Key::UnlockTitle.text(lang)),
        text(name).size(theme::CALL).font(theme::SEMIBOLD).wrapping(WRAP),
        text(Key::UnlockPrompt.text(lang)).size(theme::CAP).color(c.theme().fg_2()),
        field(c, modal, "", true),
        space().height(theme::SP2),
        buttons(c, action(c, Key::Unlock, Tone::Primary, (!modal.input.is_empty()).then_some(Message::ModalSubmit))),
    ]
    .spacing(theme::SP2)
    .into()
}

fn app_icon<'a>(app: &App, name: Option<&str>, size: f32, theme: &ThemeState) -> Element<'a, Message> {
    if let Some(name) = name
        && let Some(art) = app.icons.lookup(&[name.to_string(), "application-x-executable".to_string()])
    {
        return match art {
            Art::Vector(handle) => svg(handle).width(size).height(size).into(),
            Art::Raster(handle) => image(handle).width(size).height(size).into(),
        };
    }
    super::icon(Icon::FileBinary, size, theme.fg_2())
}

fn open_with<'a>(
    c: Ctx<'a>, modal: &'a Modal, targets: &'a [Target], mime: Option<&'a str>, chosen: &'a str, always: bool,
) -> Element<'a, Message> {
    let (t, lang) = (c.theme(), c.app.lang);
    let (matching, others) = c.app.open_with_apps(targets, &modal.input);
    let paths: Vec<PathBuf> = targets.iter().map(|target| target.path.clone()).collect();
    let total_apps = matching.len() + others.len();
    let has_selection = matching.iter().chain(&others).any(|app| app.id == chosen);

    let mut body = column![
        title(Key::OpenWith.text(lang)),
        text(subject(c.app, &paths)).size(theme::CALL).font(theme::SEMIBOLD).wrapping(WRAP),
        field(c, modal, Key::SearchApps.text(lang), false),
    ]
    .spacing(theme::SP2);

    let rows: Vec<Element<'a, Message>> = if total_apps == 0 {
        vec![
            container(text(Key::NoSearchResults.text(lang)).size(theme::CAP).color(t.fg_3()))
                .center_x(Fill)
                .center_y(Fill)
                .height(Length::Fixed(180.0))
                .into(),
        ]
    } else {
        let mut list: Vec<Element<'a, Message>> = Vec::new();
        for app in matching.into_iter().chain(others) {
            let selected = app.id == chosen;
            let icon_el = app_icon(c.app, app.icon.as_deref(), 22.0, &t);
            let name_el =
                text(super::fit(&app.name, 360.0, BUTTON_TEXT)).size(BUTTON_TEXT).wrapping(text::Wrapping::None);
            let mut row_content = row![icon_el, name_el].spacing(theme::SP3).align_y(Alignment::Center);
            if selected {
                row_content =
                    row_content.push(space::horizontal()).push(super::icon(Icon::Check, 16.0, t.acc_soft_fg()));
            }
            let btn = button(container(row_content).height(Fill).align_y(Alignment::Center))
                .on_press(Message::PickApp(app.id.clone()))
                .width(Fill)
                .height(Length::Fixed(theme::CONTROL))
                .padding(Padding::from([0.0, theme::SP3]))
                .style(move |_, status| theme::list_row(&t, selected, false, 0.0, status));
            list.push(btn.into());
        }
        list
    };

    let list_container = container(
        scrollable(Column::with_children(rows).spacing(2.0))
            .height(Length::Fixed(200.0))
            .style(move |_, status| theme::scroller(&t, status)),
    )
    .padding(theme::SP1)
    .width(Fill)
    .style(move |_| theme::rounded(t.fill_1(), theme::R2));

    body = body.push(list_container);

    if mime.is_some() {
        body = body.push(super::check(&t, Key::AlwaysOpenWithThis.text(lang), always, Message::ToggleAlwaysOpen));
    }

    body.push(space().height(theme::SP1))
        .push(buttons(c, action(c, Key::Open, Tone::Primary, has_selection.then_some(Message::ModalSubmit))))
        .into()
}

fn subject(app: &App, paths: &[PathBuf]) -> String {
    match paths {
        [single] => format!("«{}»", display_name(single)),
        _ => i18n::items(paths.len(), app.lang),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy()).into_owned()
}

fn confirm(
    c: Ctx<'_>, heading: Key, prompt: Key, subject: Option<String>, submit: Key, focus: Option<usize>,
) -> Element<'_, Message> {
    let mut body = column![title(heading.text(c.app.lang))].spacing(theme::SP3);
    if let Some(subject) = subject {
        body = body.push(text(subject).size(theme::CALL).font(theme::SEMIBOLD).wrapping(WRAP));
    }
    let actions = row![
        space::horizontal(),
        focusable(c, Key::Cancel, Tone::Neutral, Message::ModalCancel, focus == Some(0)),
        focusable(c, submit, Tone::Danger, Message::ModalSubmit, focus == Some(1)),
    ]
    .spacing(theme::SP2);
    body.push(text(prompt.text(c.app.lang)).size(theme::CALL).color(c.theme().fg_2()))
        .push(space().height(theme::SP1))
        .push(actions)
        .into()
}

fn conflict<'a>(c: Ctx<'a>, question: &'a Question, for_all: bool, focus: Option<usize>) -> Element<'a, Message> {
    let (t, lang) = (c.theme(), c.app.lang);
    let (heading, hint) = if question.folders {
        (Key::FolderExists.text(lang).to_string(), Key::FolderExistsHint)
    } else {
        (format!("«{}» {}", display_name(&question.dest), Key::FileExists.text(lang)), Key::FileExistsHint)
    };
    let [(skip, skipped), (both, kept), (primary, chosen)] = choices(question.folders);
    let check = super::check(&t, Key::ApplyToAll.text(lang), for_all, Message::ToggleForAll);
    column![
        text(heading).size(theme::T3).font(theme::SEMIBOLD).wrapping(WRAP),
        text(hint.text(lang)).size(theme::CALL).color(t.fg_2()),
        text(tilde(&question.dest)).size(theme::CAP).color(t.fg_3()).wrapping(WRAP),
        check,
        space().height(theme::SP1),
        row![
            focusable(c, Key::StopJob, Tone::Ghost, Message::Resolve(Choice::Cancel), focus == Some(0)),
            space::horizontal(),
            focusable(c, skip, Tone::Neutral, Message::Resolve(skipped), focus == Some(1)),
            focusable(c, both, Tone::Neutral, Message::Resolve(kept), focus == Some(2)),
            focusable(c, primary, Tone::Primary, Message::Resolve(chosen), focus == Some(3)),
        ]
        .spacing(theme::SP2),
    ]
    .spacing(theme::SP3)
    .into()
}

fn tilde(path: &Path) -> String {
    match home_dir().and_then(|home| path.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(relative) if relative.as_os_str().is_empty() => String::from("~"),
        Some(relative) => format!("~/{}", relative.display()),
        None => path.display().to_string(),
    }
}

fn label<'a>(c: Ctx<'_>, key: Key) -> Element<'a, Message> {
    text(key.text(c.app.lang))
        .size(theme::CAP)
        .color(c.theme().fg_2())
        .align_x(Alignment::End)
        .width(Length::Fixed(LABEL_WIDTH))
        .into()
}

fn details<'a>(c: Ctx<'a>, properties: &'a Properties) -> Element<'a, Message> {
    let (t, lang) = (c.theme(), c.app.lang);
    let entry = &properties.entry;
    let now = SystemTime::now();

    let name = text(shown_name(&entry.name)).size(theme::T3).font(theme::SEMIBOLD).wrapping(WRAP);
    let heading = row![
        file_icon(c.app, entry, ICON),
        column![name, text(entry.category.name(lang)).size(theme::CAP).color(t.fg_3()),].spacing(2),
    ]
    .spacing(theme::SP4)
    .align_y(Alignment::Center);

    let mut facts: Vec<(Key, String)> = Vec::new();
    if let Some(count) = properties.contents {
        facts.push((Key::Contents, i18n::items(count, lang)));
        let size = properties.total.map_or_else(
            || Key::Calculating.text(lang).to_string(),
            |(bytes, items)| format!("{} · {}", format_bytes(bytes, lang), i18n::items(items, lang)),
        );
        facts.push((Key::TotalSize, size));
    } else {
        facts.push((Key::Size, format_bytes(entry.size, lang)));
    }
    if let Some(parent) = entry.path.parent() {
        facts.push((Key::Location, tilde(parent)));
    }
    if let Some(original) = &properties.original {
        facts.push((Key::OriginalLocation, tilde(original)));
    }
    if let Some(target) = &properties.target {
        facts.push((Key::SymlinkTarget, target.display().to_string()));
    }
    if let Some(modified) = entry.modified {
        facts.push((Key::ModifiedDate, format_time(modified, now, lang)));
    }
    if let Some(created) = properties.created {
        facts.push((Key::CreatedDate, format_time(created, now, lang)));
    }
    facts.push((Key::Permissions, permissions_string(entry.mode)));

    let rows = Column::with_children(facts.into_iter().map(|(key, value)| {
        row![label(c, key), text(value).size(theme::CAP).width(Fill).wrapping(WRAP)].spacing(theme::SP3).into()
    }))
    .spacing(theme::SP2);

    let divider = rule::horizontal(1).style(move |_| theme::line(&t));
    let mut body = column![heading, divider, rows, permissions(c, properties)].spacing(theme::SP4);
    if let Some(error) = &properties.error {
        body = body.push(text(error).size(theme::CAP).color(t.danger()).wrapping(WRAP));
    }
    body.push(row![space::horizontal(), action(c, Key::Close, Tone::Neutral, Some(Message::ModalCancel))]).into()
}

fn permissions<'a>(c: Ctx<'a>, properties: &'a Properties) -> Element<'a, Message> {
    let t = c.theme();
    let mode = properties.entry.mode;
    let who = [Key::Owner, Key::Group, Key::Others];
    let rows = who.into_iter().enumerate().map(|(index, key)| {
        let shift = 6 - 3 * index as u32;
        let bits = [('r', 4), ('w', 2), ('x', 1)].into_iter().map(move |(letter, value)| {
            let bit = value << shift;
            let on = mode & bit != 0;
            pressable(
                button(
                    container(text(if on { letter } else { '-' }.to_string()).size(theme::CAP).font(theme::SEMIBOLD))
                        .center(Fill),
                )
                .on_press(Message::Permission(bit))
                .width(Length::Fixed(theme::SMALL_CONTROL))
                .height(Length::Fixed(theme::SMALL_CONTROL))
                .padding(0)
                .style(move |_, status| theme::action(&t, if on { Tone::Primary } else { Tone::Neutral }, status)),
            )
        });
        row![label(c, key), iced::widget::Row::with_children(bits).spacing(theme::SP1)]
            .spacing(theme::SP3)
            .align_y(Alignment::Center)
            .into()
    });
    Column::with_children(rows).spacing(theme::SP1).into()
}
