use std::path::{Path, PathBuf};
use std::time::SystemTime;

use glide_sdk::Tone;
use glide_sdk::overlay::{dialog_buttons, dialog_card, modal};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::{
    Size, Validity, Wrapping, art, elide, elide_lines, empty_note, facts, measure, scrollable, select_row,
    sized_toggle_button, text, validated_field, well,
};
use iced::widget::{Column, column, row, text_input};
use iced::{Alignment, Element};

use super::file_icon;
use crate::app::{App, Dialog, MODAL_ID, Message, Modal, Properties, PropertiesTab};
use crate::fs::archive::Format;
use crate::fs::jobs::{Choice, Question};
use crate::fs::rename::{self, Problem, Rule};
use crate::fs::{Target, format_bytes, format_time, home_dir, permissions_string};
use crate::i18n::{self, Key, Lang};
use crate::icons::Icon;
use crate::theme;

const WIDTH: f32 = 400.0;
const WIDE: f32 = 520.0;
const LABEL_WIDTH: f32 = 112.0;
const ICON: f32 = 56.0;
const NAME_LINES: usize = 3;
const WRAP: Wrapping = Wrapping::WordOrGlyph;
const BUTTON: TextStyle = TextStyle { weight: 600, ..TextStyle::MENU_ITEM };
const EMPHASIS: TextStyle = TextStyle { weight: 600, ..TextStyle::CALL };
const NAME: TextStyle = TextStyle { weight: 600, ..TextStyle::T3 };

type Choices<'a> = Vec<(&'a str, Tone, Option<Message>)>;

struct Parts<'a> {
    title: String,
    body: Element<'a, Message>,
    choices: Choices<'a>,
    focus: Option<usize>,
}

#[must_use]
pub fn view<'a>(app: &'a App, modal_state: &'a Modal, progress: f32) -> Element<'a, Message> {
    let paint = super::paint(&app.theme.faded(progress));
    let focus = modal_state.focused();
    let parts = match &modal_state.dialog {
        Dialog::NewFolder => input(app, &paint, modal_state, (Key::NewFolderTitle, Key::NamePrompt), Key::Create),
        Dialog::NewFile => input(app, &paint, modal_state, (Key::NewFileTitle, Key::NamePrompt), Key::Create),
        Dialog::NewFromTemplate(_) => {
            input(app, &paint, modal_state, (Key::NewDocumentTitle, Key::NamePrompt), Key::Create)
        }
        Dialog::Tags(_) => input(app, &paint, modal_state, (Key::TagsTitle, Key::TagsPrompt), Key::Save),
        Dialog::Rename(_) => input(app, &paint, modal_state, (Key::RenameTitle, Key::NamePrompt), Key::RenameConfirm),
        Dialog::Delete(paths) => {
            let prompt = if paths.len() == 1 { Key::DeletePromptOne } else { Key::DeletePrompt };
            confirm(app, &paint, (Key::DeleteTitle, prompt), Some(subject(app, paths)), Key::DeletePermanently, focus)
        }
        Dialog::EmptyTrash => {
            confirm(app, &paint, (Key::EmptyTrashTitle, Key::EmptyTrashPrompt), None, Key::EmptyTrash, focus)
        }
        Dialog::Quit => confirm(app, &paint, (Key::QuitTitle, Key::QuitPrompt), None, Key::QuitConfirm, focus),
        Dialog::Replace(target) => confirm(
            app,
            &paint,
            (Key::ChooserReplaceTitle, Key::ChooserReplacePrompt),
            Some(subject(app, std::slice::from_ref(target))),
            Key::Replace,
            focus,
        ),
        Dialog::Properties(properties) => details(app, &paint, properties),
        Dialog::Repair(object) => confirm(
            app,
            &paint,
            (Key::RepairTitle, Key::RepairPrompt),
            app.volume(object).map(|volume| format!("«{}»", volume.name(app.lang))),
            Key::RepairAction,
            focus,
        ),
        Dialog::Conflict { question, for_all, .. } => conflict(app, &paint, question, *for_all, focus),
        Dialog::Unlock(object) => unlock(app, &paint, modal_state, object),
        Dialog::OpenWith { targets, mime, chosen, always } => {
            open_with(app, &paint, modal_state, targets, mime.as_deref(), chosen, *always)
        }
        Dialog::Compress { format, .. } => compress(app, &paint, modal_state, *format),
        Dialog::BatchRename { replace, with, .. } => batch_rename(app, &paint, modal_state, *replace, with),
        Dialog::SelectPattern => {
            input(app, &paint, modal_state, (Key::SelectMatchingTitle, Key::SelectMatchingPrompt), Key::SelectConfirm)
        }
    };
    let width = card_width(&modal_state.dialog, app.lang);
    let buttons = dialog_buttons(&paint, parts.choices, parts.focus);
    let card = dialog_card(&paint, &parts.title, Some(parts.body), buttons, width, app.window_size());
    modal(&paint, progress, card, None, app.corner())
}

#[must_use]
pub fn card_width(dialog: &Dialog, lang: Lang) -> f32 {
    match dialog {
        Dialog::OpenWith { .. } | Dialog::BatchRename { .. } => WIDE,
        Dialog::Conflict { question, .. } => {
            let labels = std::iter::once(Key::StopJob).chain(choices(question.folders).map(|(key, _)| key));
            let row: f32 =
                labels.map(|key| measure(key.text(lang), BUTTON) + 2.0 * theme::SP4 + theme::SP2).sum::<f32>()
                    + 8.0 * theme::RING;
            WIDE.max(row + theme::SP4 + 2.0 * theme::SP5)
        }
        _ => WIDTH,
    }
}

#[must_use]
pub fn shown_name(name: &str) -> String {
    let room = WIDTH - 2.0 * theme::SP5 - ICON - theme::SP4;
    elide_lines(name, room, NAME, NAME_LINES)
}

#[must_use]
pub fn choices(folders: bool) -> [(Key, Choice); 3] {
    if folders {
        [(Key::Skip, Choice::Skip), (Key::KeepBothFolders, Choice::KeepBoth), (Key::Merge, Choice::Merge)]
    } else {
        [(Key::Skip, Choice::Skip), (Key::KeepBoth, Choice::KeepBoth), (Key::Replace, Choice::Replace)]
    }
}

fn cancel(app: &App) -> (&str, Tone, Option<Message>) {
    (Key::Cancel.text(app.lang), Tone::Neutral, Some(Message::ModalCancel))
}

fn prompt<'a>(paint: &glide_sdk::Paint, label: &'a str) -> Element<'a, Message> {
    text(label, TextStyle::CAP).color(paint.fg_2()).into()
}

fn emphasis(label: String) -> Element<'static, Message> {
    text(label, EMPHASIS).wrapping(WRAP).into()
}

fn field<'a>(paint: &glide_sdk::Paint, modal: &'a Modal, placeholder: &str, secure: bool) -> Element<'a, Message> {
    let input = text_input(placeholder, &modal.input)
        .id(MODAL_ID)
        .on_input(Message::ModalInput)
        .on_submit(Message::ModalSubmit);
    let validity = if modal.error.is_some() { Validity::Invalid } else { Validity::Neutral };
    if secure {
        return glide_sdk::widget::secret_input(paint, input, modal.shown, Message::RevealModal, validity);
    }
    validated_field(paint, input, validity)
}

fn input<'a>(
    app: &'a App, paint: &glide_sdk::Paint, modal: &'a Modal, (heading, label): (Key, Key), submit: Key,
) -> Parts<'a> {
    let lang = app.lang;
    let valid = label == Key::TagsPrompt || !modal.input.trim().is_empty();
    let mut body = column![prompt(paint, label.text(lang)), field(paint, modal, "", false)].spacing(theme::SP2);
    if let Some(error) = &modal.error {
        body = body.push(text(error, TextStyle::CAP).color(paint.danger()).wrapping(WRAP));
    }
    Parts {
        title: heading.text(lang).to_string(),
        body: body.into(),
        choices: vec![cancel(app), (submit.text(lang), Tone::Accent, valid.then_some(Message::ModalSubmit))],
        focus: None,
    }
}

fn failure<'a>(paint: &glide_sdk::Paint, error: impl Into<std::borrow::Cow<'a, str>>) -> Element<'a, Message> {
    text(error, TextStyle::CAP).color(paint.danger()).wrapping(WRAP).into()
}

fn compress<'a>(app: &'a App, paint: &glide_sdk::Paint, modal: &'a Modal, format: Format) -> Parts<'a> {
    let lang = app.lang;
    let formats = Format::ALL.map(|format| (format, format.extension()));
    let mut body = column![
        prompt(paint, Key::NamePrompt.text(lang)),
        field(paint, modal, "", false),
        prompt(paint, Key::ArchiveFormat.text(lang)),
        glide_sdk::widget::segmented_fill(paint, &formats, format, Message::CompressFormat),
    ]
    .spacing(theme::SP2);
    if let Some(error) = &modal.error {
        body = body.push(failure(paint, error.as_str()));
    }
    let ready = (!modal.input.trim().is_empty()).then_some(Message::ModalSubmit);
    Parts {
        title: Key::CompressTitle.text(lang).to_string(),
        body: body.into(),
        choices: vec![cancel(app), (Key::CompressConfirm.text(lang), Tone::Accent, ready)],
        focus: None,
    }
}

fn batch_rename<'a>(
    app: &'a App, paint: &glide_sdk::Paint, modal: &'a Modal, replace: bool, with: &'a str,
) -> Parts<'a> {
    const PREVIEW: usize = 4;
    let lang = app.lang;
    let modes = [(false, Key::RenameByTemplate.text(lang)), (true, Key::RenameByReplace.text(lang))];
    let mut body =
        column![glide_sdk::widget::segmented_fill(paint, &modes, replace, Message::RenameMode)].spacing(theme::SP2);
    if replace {
        let second = text_input("", with).on_input(Message::RenameWith).on_submit(Message::ModalSubmit);
        body = body
            .push(prompt(paint, Key::RenameFind.text(lang)))
            .push(field(paint, modal, "", false))
            .push(prompt(paint, Key::RenameWith.text(lang)))
            .push(glide_sdk::widget::field(paint, second));
    } else {
        body = body
            .push(prompt(paint, Key::NamePrompt.text(lang)))
            .push(field(paint, modal, "", false))
            .push(prompt(paint, Key::RenameTemplateHint.text(lang)));
    }
    let (plan, rule) = modal.rename_plan().unwrap_or_else(|| (Vec::new(), Rule::Template(String::new())));
    let listed: std::collections::HashSet<&Path> = app.entries.iter().map(|entry| entry.path.as_path()).collect();
    let problem = rename::problem(&plan, &rule, |path| listed.contains(path));
    let room = (WIDE - 2.0 * theme::SP5 - 2.0 * theme::SP3 - theme::SP4) / 2.0;
    let mut rows: Vec<(String, String)> = plan
        .iter()
        .take(PREVIEW)
        .map(|(from, to)| (elide(&display_name(from), room, TextStyle::CAP), display_name(to)))
        .collect();
    if plan.len() > PREVIEW {
        rows.push((String::from("…"), i18n::items(plan.len() - PREVIEW, lang)));
    }
    body = body.push(well(paint, facts(paint, rows, room)));
    let complaint = match problem {
        Some(Problem::Invalid) => Some(Key::ErrName),
        Some(Problem::Repeats) => Some(Key::RenameRepeats),
        Some(Problem::Taken) => Some(Key::RenameTaken),
        Some(Problem::Unchanged) if modal.input != rename::NAME_TOKEN => Some(Key::RenameUnchanged),
        Some(Problem::Unchanged | Problem::Empty) | None => None,
    };
    if let Some(error) = modal.error.as_deref().or(complaint.map(|key| key.text(lang))) {
        body = body.push(failure(paint, error));
    }
    Parts {
        title: format!("{} · {}", Key::BatchRenameTitle.text(lang), i18n::items(plan.len(), lang)),
        body: body.into(),
        choices: vec![
            cancel(app),
            (Key::RenameConfirm.text(lang), Tone::Accent, problem.is_none().then_some(Message::ModalSubmit)),
        ],
        focus: None,
    }
}

fn unlock<'a>(app: &'a App, paint: &glide_sdk::Paint, modal: &'a Modal, object: &'a str) -> Parts<'a> {
    let lang = app.lang;
    let name = app.volume(object).map(|volume| format!("«{}»", volume.name(lang))).unwrap_or_default();
    let ready = (!modal.input.is_empty()).then_some(Message::ModalSubmit);
    Parts {
        title: Key::UnlockTitle.text(lang).to_string(),
        body: column![emphasis(name), prompt(paint, Key::UnlockPrompt.text(lang)), field(paint, modal, "", true)]
            .spacing(theme::SP2)
            .into(),
        choices: vec![cancel(app), (Key::Unlock.text(lang), Tone::Accent, ready)],
        focus: None,
    }
}

fn app_icon<'a>(app: &App, name: Option<&str>, size: f32, paint: &glide_sdk::Paint) -> Element<'a, Message> {
    if let Some(name) = name
        && let Some(art) = app.icons.lookup(&[name.to_string(), "application-x-executable".to_string()])
    {
        return self::art(art, size, 1.0);
    }
    super::icon(Icon::FileBinary, size, paint.fg_2())
}

fn open_with<'a>(
    app: &'a App, paint: &glide_sdk::Paint, modal: &'a Modal, targets: &'a [Target], mime: Option<&'a str>,
    chosen: &'a str, always: bool,
) -> Parts<'a> {
    let lang = app.lang;
    let (matching, others) = app.open_with_apps(targets, &modal.input);
    let paths: Vec<PathBuf> = targets.iter().map(|target| target.path.clone()).collect();
    let has_selection = matching.iter().chain(&others).any(|entry| entry.id == chosen);
    let rows: Vec<Element<'a, Message>> = matching
        .into_iter()
        .chain(others)
        .map(|entry| {
            let icon = app_icon(app, entry.icon.as_deref(), 22.0, paint);
            let name = elide(&entry.name, 360.0, TextStyle::CALL);
            select_row(paint, Some(icon), name, entry.id == chosen, Message::PickApp(entry.id.clone()))
        })
        .collect();
    let list: Element<'a, Message> = if rows.is_empty() {
        empty_note(paint, Icon::Search, "", Key::NoSearchResults.text(lang))
    } else {
        scrollable(paint, Column::with_children(rows).spacing(2.0)).height(200.0).into()
    };
    let mut body = column![emphasis(subject(app, &paths)), field(paint, modal, Key::SearchApps.text(lang), false)]
        .spacing(theme::SP2)
        .push(well(paint, list));
    if mime.is_some() {
        body =
            body.push(super::check(&app.theme, Key::AlwaysOpenWithThis.text(lang), always, Message::ToggleAlwaysOpen));
    }
    Parts {
        title: Key::OpenWith.text(lang).to_string(),
        body: body.into(),
        choices: vec![cancel(app), (Key::Open.text(lang), Tone::Accent, has_selection.then_some(Message::ModalSubmit))],
        focus: None,
    }
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

fn confirm<'a>(
    app: &'a App, paint: &glide_sdk::Paint, (heading, question): (Key, Key), subject: Option<String>, submit: Key,
    focus: Option<usize>,
) -> Parts<'a> {
    let mut body = column![].spacing(theme::SP3);
    if let Some(subject) = subject {
        body = body.push(emphasis(subject));
    }
    let body = body.push(text(question.text(app.lang), TextStyle::CALL).color(paint.fg_2()));
    Parts {
        title: heading.text(app.lang).to_string(),
        body: body.into(),
        choices: vec![cancel(app), (submit.text(app.lang), Tone::Danger, Some(Message::ModalSubmit))],
        focus,
    }
}

fn conflict<'a>(
    app: &'a App, paint: &glide_sdk::Paint, question: &'a Question, for_all: bool, focus: Option<usize>,
) -> Parts<'a> {
    let lang = app.lang;
    let (heading, hint) = if question.folders {
        (Key::FolderExists.text(lang).to_string(), Key::FolderExistsHint)
    } else {
        (format!("«{}» {}", display_name(&question.dest), Key::FileExists.text(lang)), Key::FileExistsHint)
    };
    let [(skip, skipped), (both, kept), (primary, chosen)] = choices(question.folders);
    let place = question.dest.parent().map(|parent| app.location_name(parent)).unwrap_or_default();
    let eyebrow = format!("{} · {}", Key::Conflict.text(lang), place);
    Parts {
        title: heading,
        body: column![
            text(eyebrow, TextStyle::MICRO).color(paint.fg_3()),
            text(hint.text(lang), TextStyle::CALL).color(paint.fg_2()),
            compare(app, paint, question),
            super::check(&app.theme, Key::ApplyToAll.text(lang), for_all, Message::ToggleForAll),
        ]
        .spacing(theme::SP3)
        .into(),
        choices: vec![
            (Key::StopJob.text(lang), Tone::Ghost, Some(Message::Resolve(Choice::Cancel))),
            (skip.text(lang), Tone::Neutral, Some(Message::Resolve(skipped))),
            (both.text(lang), Tone::Neutral, Some(Message::Resolve(kept))),
            (primary.text(lang), Tone::Accent, Some(Message::Resolve(chosen))),
        ],
        focus,
    }
}

fn compare<'a>(app: &App, paint: &glide_sdk::Paint, question: &Question) -> Element<'a, Message> {
    use glide_sdk::widget::compare_card;
    let lang = app.lang;
    let now = SystemTime::now();
    let newer = match (question.existing.and_then(|s| s.modified), question.incoming.and_then(|s| s.modified)) {
        (Some(existing), Some(incoming)) => incoming > existing,
        _ => false,
    };
    let lines = |stamp: Option<crate::fs::jobs::Stamp>| -> Vec<String> {
        let Some(stamp) = stamp else { return vec![String::from("—")] };
        let mut lines = vec![if question.folders { String::new() } else { format_bytes(stamp.size, lang) }];
        lines.retain(|line| !line.is_empty());
        lines.extend(stamp.modified.map(|time| format_time(time, now, lang)));
        lines
    };
    let glyph = if question.folders { Icon::Folder } else { Icon::File };
    let lead = |tint| super::icon(glyph, 32.0, tint);
    let existing =
        compare_card(paint, lead(paint.fg_2()), Key::Existing.text(lang), None, lines(question.existing), false);
    let badge = newer.then(|| Key::Newer.text(lang).to_string());
    let incoming =
        compare_card(paint, lead(paint.acc_text()), Key::Incoming.text(lang), badge, lines(question.incoming), newer);
    row![existing, incoming].spacing(theme::SP3).into()
}

pub(crate) fn tilde(path: &Path) -> String {
    match home_dir().and_then(|home| path.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(relative) if relative.as_os_str().is_empty() => String::from("~"),
        Some(relative) => format!("~/{}", relative.display()),
        None => path.display().to_string(),
    }
}

fn details<'a>(app: &'a App, paint: &glide_sdk::Paint, properties: &'a Properties) -> Parts<'a> {
    let lang = app.lang;
    let entry = &properties.entry;
    let now = SystemTime::now();
    let name = text(shown_name(&entry.name), NAME).wrapping(WRAP);
    let heading = row![
        file_icon(app, entry, ICON),
        column![name, text(entry.category.name(lang), TextStyle::CAP).color(paint.fg_3())].spacing(2),
    ]
    .spacing(theme::SP4)
    .align_y(Alignment::Center);

    let mut rows: Vec<(Key, String)> = Vec::new();
    if let Some(count) = properties.contents {
        rows.push((Key::Contents, i18n::items(count, lang)));
        let size = properties.total.map_or_else(
            || Key::Calculating.text(lang).to_string(),
            |(bytes, items)| format!("{} · {}", format_bytes(bytes, lang), i18n::items(items, lang)),
        );
        rows.push((Key::TotalSize, size));
    } else {
        rows.push((Key::Size, format_bytes(entry.size, lang)));
    }
    if let Some(parent) = entry.path.parent() {
        rows.push((Key::Location, tilde(parent)));
    }
    if let Some(original) = &properties.original {
        rows.push((Key::OriginalLocation, tilde(original)));
    }
    if let Some(target) = &properties.target {
        rows.push((Key::SymlinkTarget, target.display().to_string()));
    }
    if let Some(modified) = entry.modified {
        rows.push((Key::ModifiedDate, format_time(modified, now, lang)));
    }
    if let Some(created) = properties.created {
        rows.push((Key::CreatedDate, format_time(created, now, lang)));
    }
    rows.push((Key::Permissions, permissions_string(entry.mode)));
    let rows = rows.into_iter().map(|(key, value)| (key.text(lang).to_string(), value)).collect();

    let tab = properties.tab;
    let tabs = glide_sdk::widget::tabs(
        paint,
        &[(PropertiesTab::General, Key::General.text(lang)), (PropertiesTab::Permissions, Key::Permissions.text(lang))],
        tab,
        Message::PropertiesTab,
    );
    let page: Element<'a, Message> = match tab {
        PropertiesTab::General => facts(paint, rows, LABEL_WIDTH),
        PropertiesTab::Permissions => column![
            permissions(app, paint, properties),
            text(permissions_string(entry.mode), TextStyle::NUM).color(paint.fg_2()),
        ]
        .spacing(theme::SP3)
        .into(),
    };
    let page = glide_sdk::widget::swapped(tab as u8, None, page);
    let mut body = column![heading, tabs, page].spacing(theme::SP4);
    if let Some(error) = &properties.error {
        body = body.push(text(error, TextStyle::CAP).color(paint.danger()).wrapping(WRAP));
    }
    Parts {
        title: String::new(),
        body: body.into(),
        choices: vec![(Key::Close.text(lang), Tone::Neutral, Some(Message::ModalCancel))],
        focus: None,
    }
}

fn permissions<'a>(app: &'a App, paint: &glide_sdk::Paint, properties: &'a Properties) -> Element<'a, Message> {
    let mode = properties.entry.mode;
    let who = [Key::Owner, Key::Group, Key::Others];
    let rows = who.into_iter().enumerate().map(|(index, key)| {
        let shift = 6 - 3 * index as u32;
        let bits = [('r', 4), ('w', 2), ('x', 1)].into_iter().map(move |(letter, value)| {
            let bit = value << shift;
            let on = mode & bit != 0;
            let label = if on { letter } else { '-' }.to_string();
            sized_toggle_button(paint, label, on, Size::Small, Some(move |_| Message::Permission(bit)))
        });
        row![
            text(key.text(app.lang), TextStyle::CAP).color(paint.fg_2()).align_x(Alignment::End).width(LABEL_WIDTH),
            iced::widget::Row::with_children(bits).spacing(theme::SP1),
        ]
        .spacing(theme::SP3)
        .align_y(Alignment::Center)
        .into()
    });
    Column::with_children(rows).spacing(theme::SP1).into()
}
