use std::path::{Path, PathBuf};
use std::time::SystemTime;

use iced::widget::{Column, button, checkbox, column, container, float, opaque, row, rule, space, text, text_input};
use iced::{Alignment, Element, Fill, Length, Padding, Vector};

use super::header::SEMIBOLD_SPREAD;
use super::{file_icon, text_width};
use crate::app::{App, Dialog, MODAL_ID, Message, Modal, Properties};
use crate::fs::jobs::{Choice, Question};
use crate::fs::{format_bytes, format_time, home_dir, permissions_string};
use crate::i18n::{self, Key, Lang};
use crate::theme::{self, ThemeState, Tone};

const WIDTH: f32 = 400.0;
const WIDE: f32 = 520.0;
const LABEL_WIDTH: f32 = 112.0;
const BUTTON_TEXT: f32 = 13.5;

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
    let body = match &modal.dialog {
        Dialog::NewFolder => input(c, modal, (Key::NewFolderTitle, Key::NamePrompt), Key::Create),
        Dialog::NewFile => input(c, modal, (Key::NewFileTitle, Key::NamePrompt), Key::Create),
        Dialog::NewFromTemplate(_) => input(c, modal, (Key::NewDocumentTitle, Key::NamePrompt), Key::Create),
        Dialog::Tags(_) => input(c, modal, (Key::TagsTitle, Key::TagsPrompt), Key::Save),
        Dialog::Rename(_) => input(c, modal, (Key::RenameTitle, Key::NamePrompt), Key::RenameConfirm),
        Dialog::Delete(paths) => {
            let prompt = if paths.len() == 1 { Key::DeletePromptOne } else { Key::DeletePrompt };
            confirm(c, Key::DeleteTitle, prompt, Some(subject(app, paths)), Key::DeletePermanently)
        }
        Dialog::EmptyTrash => confirm(c, Key::EmptyTrashTitle, Key::EmptyTrashPrompt, None, Key::EmptyTrash),
        Dialog::Quit => confirm(c, Key::QuitTitle, Key::QuitPrompt, None, Key::QuitConfirm),
        Dialog::Properties(properties) => details(c, properties),
        Dialog::Repair(object) => confirm(
            c,
            Key::RepairTitle,
            Key::RepairPrompt,
            app.volume(object).map(|volume| format!("«{}»", volume.name(app.lang))),
            Key::RepairAction,
        ),
        Dialog::Conflict { question, for_all, .. } => conflict(c, question, *for_all),
        Dialog::Unlock(object) => unlock(c, modal, object),
    };
    let width = card_width(&modal.dialog, app.lang);
    let rise = (1.0 - progress) * 12.0;
    let t = c.theme();
    let card =
        float(container(body).width(Length::Fixed(width)).padding(theme::SP5).style(move |_| theme::popover(&t)))
            .translate(move |_, _| Vector::new(0.0, rise));
    let layer = container(card).center(Fill).style(move |_| theme::scrim(progress));
    if live { opaque(layer) } else { layer.into() }
}

#[must_use]
pub fn card_width(dialog: &Dialog, lang: Lang) -> f32 {
    let Dialog::Conflict { question, .. } = dialog else {
        return WIDTH;
    };
    let labels = std::iter::once(Key::StopJob).chain(choices(question.folders).map(|(key, _)| key));
    let row: f32 = labels
        .map(|key| text_width(key.text(lang), BUTTON_TEXT) / SEMIBOLD_SPREAD + 2.0 * theme::SP4 + theme::SP2)
        .sum();
    WIDE.max(row + theme::SP4 + 2.0 * theme::SP5)
}

fn choices(folders: bool) -> [(Key, Choice); 3] {
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
    button(container(text(label.text(c.app.lang)).size(BUTTON_TEXT).font(theme::SEMIBOLD)).center_y(Fill))
        .on_press_maybe(message)
        .height(Length::Fixed(theme::CONTROL))
        .padding(Padding::from([0.0, theme::SP4]))
        .style(move |_, status| theme::action(&t, tone, status))
        .into()
}

fn buttons<'a>(c: Ctx<'a>, confirm: Element<'a, Message>) -> Element<'a, Message> {
    row![space::horizontal(), action(c, Key::Cancel, Tone::Neutral, Some(Message::ModalCancel)), confirm,]
        .spacing(theme::SP2)
        .into()
}

fn field<'a>(c: Ctx<'a>, modal: &'a Modal) -> text_input::TextInput<'a, Message> {
    let t = c.theme();
    text_input("", &modal.input)
        .id(MODAL_ID)
        .on_input(Message::ModalInput)
        .on_submit(Message::ModalSubmit)
        .padding(Padding::from([7.0, theme::SP3]))
        .size(BUTTON_TEXT)
        .style(move |_, status| {
            let mut style = theme::field(&t, status);
            if modal.error.is_some() {
                style.border.color = t.danger();
            }
            style
        })
}

fn input<'a>(c: Ctx<'a>, modal: &'a Modal, (heading, prompt): (Key, Key), submit: Key) -> Element<'a, Message> {
    let lang = c.app.lang;
    let valid = prompt == Key::TagsPrompt || !modal.input.trim().is_empty();
    let mut body = column![
        title(heading.text(lang)),
        text(prompt.text(lang)).size(theme::CAP).color(c.theme().fg_2()),
        field(c, modal),
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
        field(c, modal).secure(true),
        space().height(theme::SP2),
        buttons(c, action(c, Key::Unlock, Tone::Primary, (!modal.input.is_empty()).then_some(Message::ModalSubmit))),
    ]
    .spacing(theme::SP2)
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

fn confirm(c: Ctx<'_>, heading: Key, prompt: Key, subject: Option<String>, submit: Key) -> Element<'_, Message> {
    let mut body = column![title(heading.text(c.app.lang))].spacing(theme::SP3);
    if let Some(subject) = subject {
        body = body.push(text(subject).size(theme::CALL).font(theme::SEMIBOLD).wrapping(WRAP));
    }
    body.push(text(prompt.text(c.app.lang)).size(theme::CALL).color(c.theme().fg_2()))
        .push(space().height(theme::SP1))
        .push(buttons(c, action(c, submit, Tone::Danger, Some(Message::ModalSubmit))))
        .into()
}

fn conflict<'a>(c: Ctx<'a>, question: &'a Question, for_all: bool) -> Element<'a, Message> {
    let (t, lang) = (c.theme(), c.app.lang);
    let (heading, hint) = if question.folders {
        (Key::FolderExists.text(lang).to_string(), Key::FolderExistsHint)
    } else {
        (format!("«{}» {}", display_name(&question.dest), Key::FileExists.text(lang)), Key::FileExistsHint)
    };
    let [(skip, skipped), (both, kept), (primary, chosen)] = choices(question.folders);
    let check = checkbox(for_all)
        .label(Key::ApplyToAll.text(lang))
        .on_toggle(|_| Message::ToggleForAll)
        .size(20)
        .spacing(theme::SP2)
        .text_size(BUTTON_TEXT)
        .style(move |_, status| theme::checkbox(&t, status));
    column![
        text(heading).size(theme::T3).font(theme::SEMIBOLD).wrapping(WRAP),
        text(hint.text(lang)).size(theme::CALL).color(t.fg_2()),
        text(tilde(&question.dest)).size(theme::CAP).color(t.fg_3()).wrapping(WRAP),
        check,
        space().height(theme::SP1),
        row![
            action(c, Key::StopJob, Tone::Ghost, Some(Message::Resolve(Choice::Cancel))),
            space::horizontal(),
            action(c, skip, Tone::Neutral, Some(Message::Resolve(skipped))),
            action(c, both, Tone::Neutral, Some(Message::Resolve(kept))),
            action(c, primary, Tone::Primary, Some(Message::Resolve(chosen))),
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

    let heading = row![
        file_icon(c.app, entry, 56.0),
        column![title(&entry.name), text(entry.category.name(lang)).size(theme::CAP).color(t.fg_3()),].spacing(2),
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
            button(
                container(text(if on { letter } else { '-' }.to_string()).size(theme::CAP).font(theme::SEMIBOLD))
                    .center(Fill),
            )
            .on_press(Message::Permission(bit))
            .width(Length::Fixed(theme::SMALL_CONTROL))
            .height(Length::Fixed(theme::SMALL_CONTROL))
            .padding(0)
            .style(move |_, status| theme::action(&t, if on { Tone::Primary } else { Tone::Neutral }, status))
            .into()
        });
        row![label(c, key), iced::widget::Row::with_children(bits).spacing(theme::SP1)]
            .spacing(theme::SP3)
            .align_y(Alignment::Center)
            .into()
    });
    Column::with_children(rows).spacing(theme::SP1).into()
}
