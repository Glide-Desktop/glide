use glide_ipc::keychain::{Hidden, Request, Response};
use glide_sdk::Shell;
use glide_sdk::chooser::{self, Ask, Chosen, Purpose};
use glide_sdk::theme::{SP1, SP2, SP4, TextStyle};
use glide_sdk::widget::{self as ui, Tone};
use iced::widget::{Column, Space, column, row};
use iced::{Element, Fill, Task};

use crate::app::{App, Message, ask};
use crate::i18n::{Key, plural};

const MAX_IMPORT: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Payload {
    pub text: Hidden,
    pub binary: bool,
}

impl Payload {
    #[must_use]
    pub fn of(bytes: Vec<u8>) -> Self {
        match String::from_utf8(bytes) {
            Ok(text) => Self { text: Hidden::new(text), binary: false },
            Err(raw) => {
                use base64::Engine as _;
                let bytes = zeroize::Zeroizing::new(raw.into_bytes());
                Self { text: Hidden::new(base64::engine::general_purpose::STANDARD.encode(&*bytes)), binary: true }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Transfer {
    Importing { contents: Payload },
    ImportPassword { contents: Payload, password: String, wrong: bool, busy: bool },
    Export { password: String, repeat: String, mismatch: bool, busy: bool },
    Saving { data: String, count: u32 },
}

#[derive(Debug, Clone)]
pub enum Event {
    Import,
    ImportChosen(Chosen),
    ImportRead(Result<Payload, String>),
    ImportDone(Result<Response, String>),
    Password(String),
    SubmitPassword,
    Export,
    ExportPassword(String),
    ExportRepeat(String),
    ExportSubmit,
    Exported(Result<Response, String>),
    SaveChosen(Chosen),
    Saved(Result<u32, String>),
    Close,
}

fn read(path: &std::path::Path) -> Result<Payload, String> {
    let size = std::fs::metadata(path).map_err(|err| err.to_string())?.len();
    if size > MAX_IMPORT {
        return Err(String::new());
    }
    std::fs::read(path).map(Payload::of).map_err(|err| err.to_string())
}

fn choose_import(app: &App) -> Task<Message> {
    let ask = Ask {
        purpose: Purpose::Open,
        title: Key::ImportTitle.text(app.lang).to_string(),
        accept: Key::ImportAccept.text(app.lang).to_string(),
        name: None,
        folder: glide_config::home(),
    };
    Task::perform(chooser::spawn(ask), |chosen| {
        Message::Transfer(Event::ImportChosen(chosen.unwrap_or(Chosen::Cancelled)))
    })
}

fn target(app: &App) -> String {
    if app.data.default.is_empty() { glide_ipc::keychain::LOCAL.to_string() } else { app.data.default.clone() }
}

fn import(app: &App, contents: Payload, password: Option<Hidden>) -> Task<Message> {
    let request = Request::Import { vault: target(app), contents: contents.text, password, binary: contents.binary };
    ask(request, |result| Message::Transfer(Event::ImportDone(result)))
}

fn export_name(now: u64) -> String {
    let (year, month, day) = crate::i18n::civil(now);
    format!("glide-passwords-{year:04}-{month:02}-{day:02}.json")
}

pub fn update(app: &mut App, event: Event, shell: &mut Shell<Message>) -> Task<Message> {
    let t = |key: Key| key.text(app.lang);
    match event {
        Event::Import => return choose_import(app),
        Event::ImportChosen(Chosen::Paths(paths)) => {
            if let Some(path) = paths.into_iter().next() {
                return Task::perform(async move { read(&path) }, |result| {
                    Message::Transfer(Event::ImportRead(result))
                });
            }
        }
        Event::ImportChosen(Chosen::Cancelled) => {}
        Event::ImportRead(Ok(contents)) => {
            app.transfer = Some(Transfer::Importing { contents: contents.clone() });
            return import(app, contents, None);
        }
        Event::ImportRead(Err(message)) => {
            shell.toast(if message.is_empty() { t(Key::FileTooBig).to_string() } else { message });
        }
        Event::ImportDone(result) => return imported(app, result, shell),
        Event::Password(value) => {
            if let Some(Transfer::ImportPassword { password, wrong, .. }) = app.transfer.as_mut() {
                *password = value;
                *wrong = false;
            }
        }
        Event::SubmitPassword => {
            if let Some(Transfer::ImportPassword { contents, password, busy, .. }) = app.transfer.as_mut() {
                if password.is_empty() {
                    return Task::none();
                }
                *busy = true;
                let (contents, password) = (contents.clone(), Hidden::new(password.clone()));
                return import(app, contents, Some(password));
            }
        }
        Event::Export => {
            app.transfer =
                Some(Transfer::Export { password: String::new(), repeat: String::new(), mismatch: false, busy: false });
        }
        Event::ExportPassword(value) => {
            if let Some(Transfer::Export { password, mismatch, .. }) = app.transfer.as_mut() {
                *password = value;
                *mismatch = false;
            }
        }
        Event::ExportRepeat(value) => {
            if let Some(Transfer::Export { repeat, mismatch, .. }) = app.transfer.as_mut() {
                *repeat = value;
                *mismatch = false;
            }
        }
        Event::ExportSubmit => {
            if let Some(Transfer::Export { password, repeat, mismatch, busy }) = app.transfer.as_mut() {
                if password.is_empty() || password != repeat {
                    *mismatch = true;
                    return Task::none();
                }
                *busy = true;
                let request = Request::Export { vault: None, password: Hidden::new(password.clone()) };
                return ask(request, |result| Message::Transfer(Event::Exported(result)));
            }
        }
        Event::Exported(result) => return exported(app, result, shell),
        Event::SaveChosen(chosen) => {
            let Some(Transfer::Saving { data, count }) = app.transfer.take() else {
                return Task::none();
            };
            match chosen {
                Chosen::Paths(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        return Task::perform(
                            async move {
                                glide_config::write_private(&path, data.as_bytes())
                                    .map(|()| count)
                                    .map_err(|err| err.to_string())
                            },
                            |result| Message::Transfer(Event::Saved(result)),
                        );
                    }
                }
                Chosen::Failed(message) => shell.toast(message),
                Chosen::Cancelled => {}
            }
        }
        Event::Saved(Ok(count)) => {
            shell.toast(plural(app.lang, u64::from(count), [Key::ExportedOne, Key::ExportedFew, Key::ExportedMany]));
        }
        Event::Saved(Err(message)) | Event::ImportChosen(Chosen::Failed(message)) => shell.toast(message),
        Event::Close => app.transfer = None,
    }
    Task::none()
}

fn imported(app: &mut App, result: Result<Response, String>, shell: &mut Shell<Message>) -> Task<Message> {
    let t = |key: Key| key.text(app.lang);
    match result {
        Ok(Response::NeedsFilePassword) => {
            match app.transfer.take() {
                Some(Transfer::Importing { contents }) => {
                    app.transfer =
                        Some(Transfer::ImportPassword { contents, password: String::new(), wrong: false, busy: false });
                }
                Some(Transfer::ImportPassword { contents, password, wrong, .. }) => {
                    app.transfer = Some(Transfer::ImportPassword { contents, password, wrong, busy: false });
                }
                other => app.transfer = other,
            }
            return Task::none();
        }
        Ok(Response::WrongPassword) => {
            if let Some(Transfer::ImportPassword { wrong, busy, password, .. }) = app.transfer.as_mut() {
                *wrong = true;
                *busy = false;
                password.clear();
            }
            return Task::none();
        }
        Ok(Response::Imported { count, skipped }) => {
            app.transfer = None;
            let mut message = if count == 0 {
                t(Key::ImportEmpty).to_string()
            } else {
                plural(app.lang, u64::from(count), [Key::ImportedOne, Key::ImportedFew, Key::ImportedMany])
            };
            if skipped > 0 {
                message.push_str(&t(Key::ImportSkipped).replace("{n}", &skipped.to_string()));
            }
            shell.toast(message);
            return app.reload();
        }
        Ok(Response::Error { message }) | Err(message) => {
            app.transfer = None;
            shell.toast(crate::app::humane(app.lang, &message));
        }
        Ok(_) => app.transfer = None,
    }
    Task::none()
}

fn exported(app: &mut App, result: Result<Response, String>, shell: &mut Shell<Message>) -> Task<Message> {
    match result {
        Ok(Response::Exported { data, count }) => {
            app.transfer = Some(Transfer::Saving { data, count });
            let ask = Ask {
                purpose: Purpose::Save,
                title: Key::ExportSaveTitle.text(app.lang).to_string(),
                accept: Key::Save.text(app.lang).to_string(),
                name: Some(export_name(crate::app::unix_now())),
                folder: glide_config::home(),
            };
            Task::perform(chooser::spawn(ask), |chosen| {
                Message::Transfer(Event::SaveChosen(chosen.unwrap_or(Chosen::Cancelled)))
            })
        }
        Ok(Response::Error { message }) | Err(message) => {
            app.transfer = None;
            shell.toast(crate::app::humane(app.lang, &message));
            Task::none()
        }
        Ok(_) => {
            app.transfer = None;
            Task::none()
        }
    }
}

#[must_use]
pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let paint = app.paint;
    let t = |key: Key| key.text(app.lang);
    let heading = |title: Key, body: Key| -> Element<'_, Message> {
        column![ui::text(t(title), TextStyle::T2), ui::text(t(body), TextStyle::CALL).color(paint.fg_2())]
            .spacing(SP1)
            .into()
    };
    let close = Message::Transfer(Event::Close);
    let content: Column<'_, Message> = match app.transfer.as_ref()? {
        Transfer::Saving { .. } | Transfer::Importing { .. } => return None,
        Transfer::ImportPassword { contents, password, wrong, busy } => {
            let (title, body) = if contents.binary {
                (Key::KeePassPassword, Key::KeePassPasswordBody)
            } else {
                (Key::FilePassword, Key::FilePasswordBody)
            };
            let mut content = Column::new().spacing(SP4).push(heading(title, body)).push(crate::view::secret_input(
                &paint,
                t(title),
                password,
                |value| Message::Transfer(Event::Password(value)),
            ));
            if *wrong {
                content = content.push(ui::text(t(Key::FileWrongPassword), TextStyle::CAP).color(paint.danger()));
            }
            content.push(
                row![
                    Space::new().width(Fill),
                    ui::secondary(&paint, t(Key::Cancel), close.clone()),
                    ui::button(
                        &paint,
                        t(Key::ImportAccept),
                        Tone::Accent,
                        (!busy && !password.is_empty()).then_some(Message::Transfer(Event::SubmitPassword))
                    ),
                ]
                .spacing(SP2),
            )
        }
        Transfer::Export { password, repeat, mismatch, busy } => {
            let mut content = Column::new()
                .spacing(SP4)
                .push(heading(Key::ExportTitle, Key::ExportBody))
                .push(crate::wizard::labelled(
                    &paint,
                    t(Key::FilePassword),
                    crate::view::secret_input(&paint, t(Key::FilePassword), password, |value| {
                        Message::Transfer(Event::ExportPassword(value))
                    }),
                ))
                .push(crate::wizard::labelled(
                    &paint,
                    t(Key::RepeatPassword),
                    crate::view::secret_input(&paint, t(Key::RepeatPassword), repeat, |value| {
                        Message::Transfer(Event::ExportRepeat(value))
                    }),
                ));
            if *mismatch {
                content = content.push(ui::text(t(Key::Mismatch), TextStyle::CAP).color(paint.danger()));
            }
            let action: Element<'_, Message> = if *busy {
                ui::busy_label(&paint, t(Key::ExportAccept))
            } else {
                ui::button(&paint, t(Key::ExportAccept), Tone::Accent, Some(Message::Transfer(Event::ExportSubmit)))
            };
            content.push(
                row![Space::new().width(Fill), ui::secondary(&paint, t(Key::Cancel), close.clone()), action]
                    .spacing(SP2),
            )
        }
    };
    Some(crate::view::dialog_frame(app, 480.0, content, close))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_files_go_as_they_are_and_databases_as_base64() {
        let text = Payload::of(b"name,password\n".to_vec());
        assert_eq!((text.text.expose(), text.binary), ("name,password\n", false));
        let database = Payload::of(vec![0x03, 0xd9, 0xa2, 0x9a, 0x67]);
        assert_eq!((database.text.expose(), database.binary), ("A9mimmc=", true));
    }

    #[test]
    fn the_export_file_is_named_after_the_day() {
        assert_eq!(export_name(1_800_000_000), "glide-passwords-2027-01-15.json");
    }
}
