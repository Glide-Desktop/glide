use std::path::Path;

use glide_sdk::theme::{SP2, SP3, TextStyle};
use glide_sdk::widget::{self as ui, Size};
use glide_sdk::window::{self, Header};
use iced::widget::{Space, column, container, row, text_input};
use iced::{Alignment, Element, Fill, Length, Padding};

use super::PASSWORD_ID;
use crate::app::{App, Lib, Message, PropsTab, Screen, Show};
use crate::doc::OpenError;
use crate::i18n::{self, Key};
use crate::icons::Icon;
use crate::reader::Doc;

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn frame<'a>(
    app: &'a App, title: String, subtitle: &'a str, body: Element<'a, Message>, foot: &'a str,
) -> Element<'a, Message> {
    let paint = app.paint;
    let corner = app.window.corner();
    let back = row![
        ui::icon(Icon::Back, 16.0, paint.fg_2()),
        ui::sized_button(
            &paint,
            app.t(Key::BackToLibrary),
            ui::Tone::Ghost,
            Size::Small,
            Some(Message::Library(Lib::Back))
        )
    ]
    .align_y(Alignment::Center);
    let bar = Header::new(title)
        .subtitle((!subtitle.is_empty()).then(|| subtitle.to_string()))
        .labels(app.t(Key::Maximize), app.t(Key::CloseWindow))
        .leading(back);
    let header = window::header(&paint, &app.window, bar, Message::Window, Message::Close, 120.0);
    let status = ui::status_bar(
        &paint,
        vec![ui::text(foot, TextStyle::CAP).color(paint.fg_2()).into()],
        Vec::new(),
        window::side_corners(window::Side::Bottom, corner),
    );
    ui::pane(paint.view_bg(), corner, column![header, container(body).height(Fill), status]).into()
}

pub fn loading<'a>(app: &'a App, path: &'a Path) -> Element<'a, Message> {
    let body = container(
        row![ui::spinner(&app.paint, false), ui::text(app.t(Key::Loading), TextStyle::CALL).color(app.paint.fg_2())]
            .spacing(SP2)
            .align_y(Alignment::Center),
    )
    .center(Fill);
    frame(app, name_of(path), "", body.into(), "")
}

pub fn locked(app: &App) -> Element<'_, Message> {
    let Screen::Locked { path, text, wrong, tries, reveal } = &app.screen else {
        return Space::new().into();
    };
    let paint = app.paint;
    let input = text_input(app.t(Key::Password), text)
        .id(PASSWORD_ID)
        .on_input(|t| Message::Library(Lib::Password(t)))
        .on_submit(Message::Library(Lib::TryPassword));
    let validity = if *wrong { ui::Validity::Invalid } else { ui::Validity::Neutral };
    let field = ui::secret_input(&paint, input, *reveal, Message::Library(Lib::Reveal), validity);
    let mut form = column![field].spacing(6).width(320);
    if *wrong {
        form = form.push(ui::text(app.t(Key::WrongPassword), TextStyle::CAP).color(paint.danger()));
    }
    let form = glide_sdk::anim::shake(*tries, form);
    let actions = row![
        ui::sized_button(
            &paint,
            app.t(Key::Cancel),
            ui::Tone::Neutral,
            Size::Medium,
            Some(Message::Library(Lib::Back))
        ),
        ui::sized_button(
            &paint,
            app.t(Key::Open),
            ui::Tone::Accent,
            Size::Medium,
            Some(Message::Library(Lib::TryPassword))
        ),
    ]
    .spacing(SP2);
    let content = column![
        ui::empty_state(&paint, Icon::Lock, app.t(Key::Locked), app.t(Key::LockedBody), None),
        container(form).center_x(Fill),
        container(actions).center_x(Fill),
    ]
    .spacing(SP3)
    .align_x(Alignment::Center);
    let body = container(content).center(Fill);
    frame(app, name_of(path), app.t(Key::LockedSub), body.into(), app.t(Key::LockedFoot))
}

pub fn failed<'a>(app: &'a App, path: &'a Path, error: &'a OpenError) -> Element<'a, Message> {
    let paint = app.paint;
    let (glyph, title, body) = match error {
        OpenError::Missing { package, .. } => (
            Icon::Warning,
            i18n::fill(app.t(Key::Missing), &[("package", package)]),
            app.t(Key::MissingBody).to_string(),
        ),
        OpenError::Unsupported => (Icon::Broken, app.t(Key::Unsupported).to_string(), String::new()),
        OpenError::Io(reason) => (Icon::Broken, app.t(Key::Damaged).to_string(), reason.clone()),
        _ => (Icon::Broken, app.t(Key::Damaged).to_string(), app.t(Key::DamagedFull).to_string()),
    };
    let action = ui::sized_button(
        &paint,
        app.t(Key::BackToLibrary),
        ui::Tone::Neutral,
        Size::Medium,
        Some(Message::Library(Lib::Back)),
    );
    let state = ui::empty_state(&paint, glyph, title, body, Some(action));
    frame(app, name_of(path), "", state, "")
}

fn rows(app: &App, items: Vec<(String, String)>) -> Element<'_, Message> {
    ui::facts(&app.paint, items, 112.0)
}

pub fn properties<'a>(app: &'a App, doc: &'a Doc, tab: PropsTab) -> Element<'a, Message> {
    let paint = app.paint;
    let tabs = [
        (PropsTab::General, app.t(Key::General)),
        (PropsTab::Fonts, app.t(Key::Fonts)),
        (PropsTab::Security, app.t(Key::Security)),
    ];
    let body = props_body(app, doc, tab);
    let head = row![
        ui::text(app.t(Key::Properties), TextStyle::T3).width(Fill),
        window::close_button(&paint, &app.window, app.t(Key::Close), Message::Show(Show::Properties(false))),
    ]
    .align_y(Alignment::Center);
    let actions = row![
        ui::sized_button(
            &paint,
            app.t(Key::ShowInFiles),
            ui::Tone::Neutral,
            Size::Medium,
            Some(Message::Show(Show::Reveal))
        ),
        Space::new().width(Fill),
        ui::sized_button(
            &paint,
            app.t(Key::Done),
            ui::Tone::Accent,
            Size::Medium,
            Some(Message::Show(Show::Properties(false)))
        ),
    ];
    let card = column![head, ui::segmented(&paint, &tabs, tab, |t| Message::Show(Show::Tab(t))), body, actions]
        .spacing(SP3 + 4.0)
        .padding(Padding::from(24.0))
        .width(Length::Fixed(500.0));
    let card = container(card).style(move |_| glide_sdk::glide_iced::style::popover(&paint));
    glide_sdk::overlay::modal(&paint, 1.0, card, Some(Message::Show(Show::Properties(false))), app.window.corner())
}

fn props_body<'a>(app: &'a App, doc: &'a Doc, tab: PropsTab) -> Element<'a, Message> {
    let paint = app.paint;
    let info = &doc.opened.info;
    let bytes = std::fs::metadata(&doc.path).map_or(0, |m| m.len());
    let page = doc.size(0);
    let mm = |pt: f32| (pt * 25.4 / 72.0).round();
    let none = app.t(Key::None).to_string();
    let or_none = |value: &str| if value.is_empty() { none.clone() } else { value.to_string() };
    match tab {
        PropsTab::General => rows(
            app,
            vec![
                (app.t(Key::Title).into(), or_none(&info.title)),
                (app.t(Key::Author).into(), or_none(&info.author)),
                (
                    app.t(Key::File).into(),
                    doc.path.to_string_lossy().replacen(
                        &glide_config::home().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default(),
                        "~",
                        1,
                    ),
                ),
                (app.t(Key::Pages).into(), format!("{} · {} × {} mm", doc.count(), mm(page.w), mm(page.h))),
                (app.t(Key::Size).into(), i18n::bytes(app.lang, bytes)),
                (app.t(Key::Created).into(), or_none(&i18n::date(app.lang, &info.created, true))),
                (app.t(Key::Modified).into(), or_none(&i18n::date(app.lang, &info.modified, true))),
                (
                    app.t(Key::Producer).into(),
                    or_none(if info.producer.is_empty() { &info.creator } else { &info.producer }),
                ),
                (app.t(Key::Version).into(), or_none(&info.version)),
            ],
        ),
        PropsTab::Fonts => {
            let list = info
                .fonts
                .iter()
                .map(|f| {
                    (
                        f.name.clone(),
                        i18n::fill(
                            app.t(if f.embedded { Key::Embedded } else { Key::NotEmbedded }),
                            &[("kind", &f.kind)],
                        ),
                    )
                })
                .collect();
            let all = info.fonts.iter().all(|f| f.embedded);
            column![
                rows(app, list),
                ui::text(app.t(if all { Key::AllEmbedded } else { Key::SomeMissing }), TextStyle::CALL)
                    .color(paint.fg_2())
            ]
            .spacing(SP3)
            .into()
        }
        PropsTab::Security => rows(
            app,
            vec![
                (app.t(Key::Encryption).into(), if info.encrypted { app.t(Key::Present).into() } else { none.clone() }),
                (app.t(Key::Printing).into(), app.t(if info.print { Key::Allowed } else { Key::Forbidden }).into()),
                (app.t(Key::Copying).into(), app.t(if info.copy { Key::Allowed } else { Key::Forbidden }).into()),
                (app.t(Key::Signatures).into(), {
                    let signed = doc
                        .opened
                        .fields
                        .iter()
                        .filter(|f| matches!(f.kind, crate::doc::FieldKind::Signature { signed: true, .. }))
                        .count();
                    if signed == 0 { none.clone() } else { signed.to_string() }
                }),
                (app.t(Key::JavaScript).into(), app.t(if info.javascript { Key::JsFound } else { Key::JsOff }).into()),
                (
                    app.t(Key::Attachments).into(),
                    if info.attachments == 0 {
                        none.clone()
                    } else {
                        i18n::fill(app.t(Key::AttachmentsSaveAs), &[("n", &info.attachments.to_string())])
                    },
                ),
            ],
        ),
    }
}
