use glide_config::permissions::Access;
use iced::widget::{column, row};
use iced::{Alignment, Element, Task};

use super::common::{error_banner, presets};
use super::{Ctx, Pane};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::gsettings;
use crate::services::privacy::{self, Footprint, PRIVACY, PrivacyState};
use crate::services::run::blocking;
use crate::services::system::format_bytes;
use crate::theme::{self, Tone};
use crate::widgets::note;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Hero => "Your privacy", "Ваша конфіденційність";
        HeroQuiet => "No app is using the camera, microphone or screen", "Жодна програма не використовує камеру, мікрофон чи екран";
        HeroBusy => "Something is using the camera, microphone or screen right now", "Просто зараз щось використовує камеру, мікрофон чи екран";
        InUse => "In use now", "Використовується зараз";
        InUseHint => "The same apps light up the coloured dots in the top bar", "Ці ж програми запалюють кольорові крапки на верхній панелі";
        Camera => "Camera", "Камера";
        Microphone => "Microphone", "Мікрофон";
        Screen => "Screen recording", "Запис екрана";
        NotInUse => "Not in use", "Не використовується";
        NoPipewire => "PipeWire is not running, so only direct camera access is detected", "PipeWire не запущено, тож виявляється лише прямий доступ до камери";
        History => "File history", "Історія файлів";
        HistoryHint => "Apps list recently opened files in their “Recent” views", "Програми показують нещодавно відкриті файли в розділі «Нещодавні»";
        Remember => "Remember recent files", "Пам'ятати нещодавні файли";
        Keep => "Keep history for", "Зберігати історію";
        Day => "1 day", "1 день";
        Week => "7 days", "7 днів";
        Month => "30 days", "30 днів";
        Forever => "Forever", "Завжди";
        Items => "items", "записів";
        ClearHistory => "Clear history", "Очистити історію";
        Cleanup => "Clean up", "Прибирання";
        CleanupHint => "Files that stay on disk after you are done with them", "Файли, що лишаються на диску, коли вони вже не потрібні";
        Trash => "Trash", "Смітник";
        TrashEmpty => "Empty", "Порожній";
        EmptyTrash => "Empty Trash", "Очистити смітник";
        EmptyConfirm => "Permanently delete everything in the Trash? This cannot be undone.", "Остаточно видалити все зі смітника? Це не можна скасувати.";
        Delete => "Delete", "Видалити";
        Thumbnails => "Thumbnail cache", "Кеш мініатюр";
        ThumbnailsHint => "Previews of pictures and videos. They are recreated when needed.", "Попередні перегляди зображень і відео. Створюються знову за потреби.";
        Clear => "Clear", "Очистити";
        Done => "Done", "Готово";
        Access => "Program access", "Доступ програм";
        AccessHint => "Glide asks the first time a program tries to read the clipboard, type into other apps or record the screen", "Glide запитує, коли програма вперше намагається читати буфер обміну, вводити текст в інші програми чи записувати екран";
        AccessNone => "No program has asked yet", "Жодна програма ще не запитувала";
        AccessClipboard => "Clipboard", "Буфер обміну";
        AccessInput => "Typing into other apps", "Введення в інші програми";
        AccessScreen => "Screen recording", "Запис екрана";
        Allowed => "allowed", "дозволено";
        Denied => "denied", "заборонено";
        Forget => "Remove", "Прибрати";
    }
}

use tr::Key as T;

pub use super::common::Choice;

const AGES: [i64; 4] = [1, 7, 30, -1];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleanup {
    Trash,
    History,
    Thumbnails,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<PrivacyState>),
    Remember(bool),
    MaxAge(Choice),
    Ask(Cleanup),
    CancelAsk,
    Clean(Cleanup),
    Cleaned(Cleanup, Option<Result<String, String>>),
    Done(Option<Result<String, String>>),
    Forget(Access, String),
    Forgotten(Option<Result<String, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    state: PrivacyState,
    loading: bool,
    confirm: Option<Cleanup>,
    cleaned: Vec<Cleanup>,
    error: Option<String>,
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "privacy",
        "security",
        "lock",
        "camera",
        "microphone",
        "trash",
        "history",
        "recent",
        "конфіденційність",
        "безпека",
        "блокування",
        "камера",
        "мікрофон",
        "смітник",
        "кошик",
        "історія",
    ];
    const POLLS: bool = true;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.cleaned.clear();
        self.reload()
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(state) => {
                self.state = *state;
                self.loading = false;
            }
            Msg::Remember(on) => {
                self.state.remember_recent = on;
                return Task::perform(
                    blocking(move || gsettings::set_bool(PRIVACY, "remember-recent-files", on)),
                    Msg::Done,
                );
            }
            Msg::MaxAge(choice) => {
                self.state.recent_max_age = choice.value;
                let value = choice.value.to_string();
                return Task::perform(
                    blocking(move || gsettings::set(PRIVACY, "recent-files-max-age", &value)),
                    Msg::Done,
                );
            }
            Msg::Ask(kind) => self.confirm = Some(kind),
            Msg::CancelAsk => self.confirm = None,
            Msg::Clean(kind) => {
                self.confirm = None;
                self.error = None;
                let job = match kind {
                    Cleanup::Trash => privacy::empty_trash,
                    Cleanup::History => privacy::clear_recent,
                    Cleanup::Thumbnails => privacy::clear_thumbnails,
                };
                return Task::perform(blocking(job), move |result| Msg::Cleaned(kind, result));
            }
            Msg::Cleaned(kind, result) => {
                match result {
                    Some(Ok(_)) => self.cleaned.push(kind),
                    Some(Err(err)) => self.error = Some(err),
                    None => {}
                }
                return self.reload();
            }
            Msg::Done(result) => {
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
            }
            Msg::Forget(access, program) => {
                self.error = None;
                self.state.permissions.forget(access, &program);
                return Task::perform(blocking(move || privacy::forget_permission(access, &program)), Msg::Forgotten);
            }
            Msg::Forgotten(result) => {
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.reload();
            }
        }
        Task::none()
    }

    fn tick(&mut self, _c: Ctx) -> Task<Msg> {
        self.reload()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let usage = &self.state.usage;
        column![
            widgets::section_hero(
                t,
                Icon::Hand,
                super::Panel::Privacy.color(),
                c.t(T::Hero),
                c.t(if usage.is_empty() { T::HeroQuiet } else { T::HeroBusy }),
                Some(if usage.is_empty() {
                    widgets::status_pill(t, Tone::Ok, c.t(T::NotInUse))
                } else {
                    widgets::status_pill(t, Tone::Warn, c.t(T::InUse))
                }),
            ),
            error_banner(t, self.error.as_deref()),
            self.in_use(&c),
            self.access(&c),
            self.history(&c),
            self.cleanup(&c),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn reload(&mut self) -> Task<Msg> {
        if self.loading {
            return Task::none();
        }
        self.loading = true;
        Task::perform(blocking(privacy::read), |state| Msg::Loaded(Box::new(state.unwrap_or_default())))
    }

    #[must_use]
    pub const fn state(&self) -> &PrivacyState {
        &self.state
    }

    fn in_use(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let usage = &self.state.usage;
        let kinds = [
            (Icon::Camera, T::Camera, &usage.camera, theme::ramp_color(theme::Accent::Conifer, 3)),
            (Icon::Mic, T::Microphone, &usage.microphone, theme::ramp_color(theme::Accent::Terracotta, 3)),
            (Icon::Display, T::Screen, &usage.screen, theme::ramp_color(theme::Accent::Amethyst, 3)),
        ];
        let mut group = Group::new(t).title(c.t(T::InUse)).description(c.t(T::InUseHint));
        for (glyph, label, apps, color) in kinds {
            let active = !apps.is_empty();
            group = group.push(
                Row::new(t, c.t(label))
                    .subtitle(if active { apps.join(", ") } else { c.t(T::NotInUse).to_string() })
                    .prefix(if active {
                        widgets::tile(glyph, color, 28.0)
                    } else {
                        iced::widget::container(widgets::icon(glyph, 18.0, t.fg_3()))
                            .center_x(28.0)
                            .center_y(28.0)
                            .into()
                    })
                    .suffix(if active { widgets::status_dot(color, 8.0) } else { column![].into() })
                    .build(),
            );
        }
        if self.state.loaded && !self.state.pipewire {
            group = group.push(note(t, Icon::Info, c.t(T::NoPipewire)));
        }
        group.build()
    }

    fn access(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Access)).description(c.t(T::AccessHint));
        let mut empty = true;
        for access in Access::ALL {
            let (glyph, kind) = match access {
                Access::Clipboard => (Icon::Copy, T::AccessClipboard),
                Access::Input => (Icon::Keyboard, T::AccessInput),
                Access::Screen => (Icon::Display, T::AccessScreen),
            };
            let rule = self.state.permissions.rule(access);
            let entries = rule
                .allow
                .iter()
                .map(|program| (program, true))
                .chain(rule.deny.iter().map(|program| (program, false)));
            for (program, allowed) in entries {
                empty = false;
                let name =
                    std::path::Path::new(program).file_name().and_then(std::ffi::OsStr::to_str).unwrap_or(program);
                let verdict = c.t(if allowed { T::Allowed } else { T::Denied });
                group = group.push(
                    Row::new(t, name)
                        .subtitle(format!("{} · {verdict} · {program}", c.t(kind)))
                        .prefix(widgets::icon(glyph, 18.0, t.fg_2()))
                        .suffix(widgets::secondary(t, c.t(T::Forget), Some(Msg::Forget(access, program.clone()))))
                        .build(),
                );
            }
        }
        if empty {
            group = group.push(note(t, Icon::Shield, c.t(T::AccessNone)));
        }
        group.build()
    }

    fn history(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::History)).description(c.t(T::HistoryHint));
        if self.state.gsettings {
            let (options, selected) = presets(&AGES, self.state.recent_max_age, |value| match value {
                1 => c.t(T::Day).to_string(),
                7 => c.t(T::Week).to_string(),
                30 => c.t(T::Month).to_string(),
                -1 => c.t(T::Forever).to_string(),
                _ => value.to_string(),
            });
            group = group
                .push(
                    Row::new(t, c.t(T::Remember))
                        .prefix(widgets::icon(Icon::History, 18.0, t.fg_2()))
                        .suffix(widgets::switch(t, self.state.remember_recent, Msg::Remember))
                        .build(),
                )
                .push(
                    Row::new(t, c.t(T::Keep))
                        .dimmed(!self.state.remember_recent)
                        .suffix(widgets::dropdown(t, options, selected, Msg::MaxAge, 180.0))
                        .build(),
                );
        }
        group
            .push(self.cleanup_row(
                c,
                Cleanup::History,
                c.t(T::History),
                format!("{} {}", self.state.recent.items, c.t(T::Items)),
                self.state.recent.items == 0,
            ))
            .build()
    }

    fn cleanup(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let trash = self.state.trash;
        let thumbs = self.state.thumbnails;
        let mut group = Group::new(t).title(c.t(T::Cleanup)).description(c.t(T::CleanupHint)).push(self.cleanup_row(
            c,
            Cleanup::Trash,
            c.t(T::Trash),
            footprint(c, trash),
            trash.items == 0,
        ));
        if self.confirm == Some(Cleanup::Trash) {
            group = group.push(note(t, Icon::Warning, c.t(T::EmptyConfirm)));
        }
        group
            .push(self.cleanup_row(
                c,
                Cleanup::Thumbnails,
                c.t(T::Thumbnails),
                format!("{} · {}", footprint(c, thumbs), c.t(T::ThumbnailsHint)),
                thumbs.items == 0,
            ))
            .build()
    }

    fn cleanup_row(&self, c: &Ctx, kind: Cleanup, title: &str, subtitle: String, empty: bool) -> Element<'_, Msg> {
        let t = c.theme;
        let (glyph, action) = match kind {
            Cleanup::Trash => (Icon::Trash, T::EmptyTrash),
            Cleanup::History => (Icon::Clock, T::ClearHistory),
            Cleanup::Thumbnails => (Icon::Image, T::Clear),
        };
        let suffix: Element<'_, Msg> = if self.confirm == Some(kind) {
            row![
                widgets::secondary(t, c.t(crate::i18n::Key::Cancel), Some(Msg::CancelAsk)),
                widgets::danger(t, c.t(T::Delete), Some(Msg::Clean(kind))),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else if empty && self.cleaned.contains(&kind) {
            widgets::status_pill(t, Tone::Ok, c.t(T::Done))
        } else {
            widgets::secondary(t, c.t(action), (!empty).then_some(Msg::Ask(kind)))
        };
        Row::new(t, title).subtitle(subtitle).prefix(widgets::icon(glyph, 18.0, t.fg_2())).suffix(suffix).build()
    }
}

fn footprint(c: &Ctx, footprint: Footprint) -> String {
    if footprint.items == 0 {
        c.t(T::TrashEmpty).to_string()
    } else {
        format!("{} {} · {}", footprint.items, c.t(T::Items), format_bytes(footprint.bytes, c.lang))
    }
}
