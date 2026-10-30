use std::path::{Path, PathBuf};

use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row};
use iced::{Alignment, Color, Element, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::run::{Generation, blocking};
use crate::services::storage::{self, Category, HomeUsage, Volume};
use crate::services::system::format_bytes;
use crate::theme::{self, Accent, ThemeState, ramp_color};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        SystemDisk => "System disk", "Системний диск";
        Free => "free of", "вільно з";
        Used => "Used", "Зайнято";
        SystemAndApps => "System & apps", "Система й програми";
        HomeFolder => "Home folder", "Домашня тека";
        Measuring => "Measuring…", "Підрахунок…";
        Rescan => "Measure again", "Підрахувати знову";
        Documents => "Documents", "Документи";
        Downloads => "Downloads", "Завантаження";
        Pictures => "Pictures", "Зображення";
        Music => "Music", "Музика";
        Videos => "Videos", "Відео";
        Desktop => "Desktop", "Стільниця";
        Cache => "App caches", "Кеш програм";
        Trash => "Trash", "Смітник";
        Other => "Other files", "Інші файли";
        OtherHint => "Settings, app data and everything else", "Налаштування, дані програм і все інше";
        FreeUp => "Free up space", "Звільнити місце";
        FreeUpHint => "These cannot be undone", "Ці дії не можна скасувати";
        EmptyTrash => "Empty the Trash", "Очистити смітник";
        EmptyTrashHint => "Deletes the files in the Trash for good", "Остаточно видаляє файли зі смітника";
        ClearCache => "Clear app caches", "Очистити кеш програм";
        ClearCacheHint => "Apps rebuild what they need; running apps may slow down for a while", "Програми відновлять потрібне; запущені програми можуть якийсь час працювати повільніше";
        Empty => "Empty…", "Очистити…";
        Clear => "Clear…", "Очистити…";
        Confirm => "Delete", "Видалити";
        Cancel => "Cancel", "Скасувати";
        Freed => "Freed", "Звільнено";
        Disks => "Disks", "Диски";
        DisksHint => "Mounted file systems", "Змонтовані файлові системи";
        NoDisks => "No disks found", "Дисків не знайдено";
        Nothing => "Nothing to remove", "Нічого видаляти";
        Loading => "Loading…", "Завантаження…";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    EmptyTrash,
    ClearCache,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Volumes(Vec<Volume>),
    Measured(u64, Box<HomeUsage>),
    Rescan,
    Open(PathBuf),
    Opened(Option<Result<(), String>>),
    Ask(Action),
    Cancel,
    Confirm,
    Cleaned(Action, Option<Result<u64, String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    volumes: Vec<Volume>,
    loaded: bool,
    usage: Option<HomeUsage>,
    measuring: bool,
    confirm: Option<Action>,
    busy: bool,
    freed: Option<u64>,
    error: Option<String>,
    generation: Generation,
}

const fn category_meta(category: Category) -> (T, Icon, Option<Accent>) {
    match category {
        Category::Documents => (T::Documents, Icon::Document, Some(Accent::Azurit)),
        Category::Downloads => (T::Downloads, Icon::Download, Some(Accent::Malachite)),
        Category::Pictures => (T::Pictures, Icon::Image, Some(Accent::Amethyst)),
        Category::Music => (T::Music, Icon::Music, Some(Accent::Cinnabar)),
        Category::Videos => (T::Videos, Icon::Film, Some(Accent::Amber)),
        Category::Desktop => (T::Desktop, Icon::Display, Some(Accent::Terracotta)),
        Category::Cache => (T::Cache, Icon::Archive, Some(Accent::Conifer)),
        Category::Trash => (T::Trash, Icon::Trash, Some(Accent::Graphite)),
        Category::Other => (T::Other, Icon::Folder, None),
    }
}

fn category_color(t: &ThemeState, category: Category) -> Color {
    category_meta(category).2.map_or_else(|| t.fg_3(), |accent| ramp_color(accent, 3))
}

fn share(part: u64, whole: u64) -> f32 {
    if whole == 0 { 0.0 } else { ((part as f64 / whole as f64).clamp(0.0, 1.0)) as f32 }
}

fn by_size(usage: &HomeUsage) -> impl Iterator<Item = (Category, u64)> {
    let mut sized = usage.sizes.clone();
    sized.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    sized.into_iter()
}

fn free_of(c: &Ctx, volume: &Volume) -> String {
    format!("{} {} {}", format_bytes(volume.avail, c.lang), c.t(T::Free), format_bytes(volume.total, c.lang))
}

fn short_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home).map_or_else(|_| path.display().to_string(), |rest| format!("~/{}", rest.display()))
}

impl Page {
    fn measure(&mut self) -> Task<Msg> {
        self.measuring = true;
        let generation = self.generation.bump();
        Task::perform(blocking(storage::measure_home), move |usage| {
            Msg::Measured(generation, Box::new(usage.unwrap_or_default()))
        })
    }

    fn volumes_task() -> Task<Msg> {
        Task::perform(blocking(storage::volumes), |volumes| Msg::Volumes(volumes.unwrap_or_default()))
    }

    fn refresh(&mut self) -> Task<Msg> {
        Task::batch([self.measure(), Self::volumes_task()])
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["storage", "disk", "space", "trash", "cache", "usage", "сховище", "диск", "місце", "смітник", "кеш"];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        let mut tasks = vec![Self::volumes_task()];
        if self.usage.is_none() && !self.measuring {
            tasks.push(self.measure());
        }
        Task::batch(tasks)
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Volumes(volumes) => {
                self.volumes = volumes;
                self.loaded = true;
            }
            Msg::Measured(generation, usage) => {
                if self.generation.is_current(generation) {
                    self.usage = Some(*usage);
                    self.measuring = false;
                }
            }
            Msg::Rescan => return self.refresh(),
            Msg::Open(path) => {
                return Task::perform(blocking(move || storage::open(&path)), Msg::Opened);
            }
            Msg::Opened(result) => {
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
            }
            Msg::Ask(action) => {
                self.confirm = Some(action);
                self.freed = None;
            }
            Msg::Cancel => self.confirm = None,
            Msg::Confirm => {
                let Some(action) = self.confirm.take() else {
                    return Task::none();
                };
                self.busy = true;
                self.error = None;
                let job = match action {
                    Action::EmptyTrash => storage::empty_trash,
                    Action::ClearCache => storage::clear_cache,
                };
                return Task::perform(blocking(job), move |result| Msg::Cleaned(action, result));
            }
            Msg::Cleaned(_, result) => {
                self.busy = false;
                match result {
                    Some(Ok(bytes)) => self.freed = Some(bytes),
                    Some(Err(err)) => self.error = Some(err),
                    None => {}
                }
                return self.refresh();
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        let t = c.theme;
        if !self.loaded {
            return widgets::empty_state(t, Icon::Drive, c.t(T::Loading), "");
        }
        let mut page = column![self.hero(c), error_banner(t, self.error.as_deref())].spacing(theme::SECTION_GAP);
        if let Some(freed) = self.freed {
            page = page.push(widgets::banner(
                t,
                crate::theme::Tone::Ok,
                Icon::Check,
                format!("{} {}", c.t(T::Freed), format_bytes(freed, c.lang)),
                None,
            ));
        }
        page.push(self.home_group(c)).push(self.clean_group(c)).push(self.disks_group(c)).into()
    }
}

impl Page {
    fn hero(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let Some(root) = self.volumes.first() else {
            return widgets::list_empty(t, Icon::Drive, c.t(T::NoDisks), "");
        };
        let mut segments: Vec<(f32, Color, String, String)> = Vec::new();
        match &self.usage {
            Some(usage) if usage.same_disk_as_root && root.mount == "/" => {
                let mut home_bytes = 0;
                for (category, size) in by_size(usage).filter(|(_, size)| *size > 0) {
                    home_bytes += size;
                    segments.push((
                        share(size, root.total),
                        category_color(&t, category),
                        c.t(category_meta(category).0).to_string(),
                        format_bytes(size, c.lang),
                    ));
                }
                let system = root.used.saturating_sub(home_bytes);
                segments.push((
                    share(system, root.total),
                    t.fg_4(),
                    c.t(T::SystemAndApps).to_string(),
                    format_bytes(system, c.lang),
                ));
            }
            _ => {
                let used = format_bytes(root.used, c.lang);
                segments.push((root.fraction(), t.acc_bg(), c.t(T::Used).to_string(), used));
            }
        }
        widgets::usage_card(t, format!("{} · {}", c.t(T::SystemDisk), root.mount), free_of(c, root), segments)
    }

    fn home_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let suffix: Element<'_, Msg> = if self.measuring {
            widgets::spinner_label(t, c.t(T::Measuring))
        } else {
            widgets::link(t, c.t(T::Rescan), (!self.busy).then_some(Msg::Rescan))
        };
        let mut group = Group::new(t).title(c.t(T::HomeFolder)).suffix(suffix);
        let Some(usage) = &self.usage else {
            return group.push(Row::new(t, c.t(T::Measuring)).dimmed(true).build()).build();
        };
        group = group.description(format!("{} · {}", usage.home.display(), format_bytes(usage.total, c.lang)));
        for (category, size) in by_size(usage) {
            let (title, glyph, _) = category_meta(category);
            let path = usage.path(category);
            let subtitle = path.map_or_else(|| c.t(T::OtherHint).to_string(), |path| short_path(path, &usage.home));
            let mut suffix = row![
                column![
                    text(format_bytes(size, c.lang), TextStyle { weight: 600, ..TextStyle::NUM }).color(t.fg_1()),
                    container(widgets::progress_bar(t, share(size, usage.total))).width(96),
                ]
                .spacing(4)
                .align_x(Alignment::End)
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if let Some(path) = path.filter(|_| usage.present.contains(&category)) {
                suffix = suffix.push(widgets::icon_button(t, Icon::External, Some(Msg::Open(path.clone()))));
            }
            group = group.push(
                Row::new(t, c.t(title))
                    .subtitle(subtitle)
                    .prefix(widgets::tile(glyph, category_color(&t, category), 28.0))
                    .suffix(suffix)
                    .build(),
            );
        }
        group.build()
    }

    fn clean_row(&self, c: &Ctx, action: Action, size: Option<u64>) -> Element<'_, Msg> {
        let t = c.theme;
        let (title, hint, label, icon) = match action {
            Action::EmptyTrash => (T::EmptyTrash, T::EmptyTrashHint, T::Empty, Icon::Trash),
            Action::ClearCache => (T::ClearCache, T::ClearCacheHint, T::Clear, Icon::Archive),
        };
        let nothing = size == Some(0);
        let suffix: Element<'_, Msg> = if self.confirm == Some(action) {
            row![
                widgets::secondary(t, c.t(T::Cancel), Some(Msg::Cancel)),
                widgets::danger(t, c.t(T::Confirm), Some(Msg::Confirm)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else {
            widgets::danger(t, c.t(label), (!self.busy && !nothing).then_some(Msg::Ask(action)))
        };
        let subtitle = match size {
            Some(0) => c.t(T::Nothing).to_string(),
            Some(size) => format!("{} · {}", format_bytes(size, c.lang), c.t(hint)),
            None => c.t(hint).to_string(),
        };
        Row::new(t, c.t(title)).subtitle(subtitle).prefix(widgets::tile(icon, t.danger(), 28.0)).suffix(suffix).build()
    }

    fn clean_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let size = |category| self.usage.as_ref().map(|usage| usage.size(category));
        Group::new(t)
            .title(c.t(T::FreeUp))
            .description(c.t(T::FreeUpHint))
            .push(self.clean_row(c, Action::EmptyTrash, size(Category::Trash)))
            .push(self.clean_row(c, Action::ClearCache, size(Category::Cache)))
            .build()
    }

    fn disks_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Disks)).description(c.t(T::DisksHint));
        if self.volumes.is_empty() {
            group = group.push(Row::new(t, c.t(T::NoDisks)).dimmed(true).build());
        }
        for volume in &self.volumes {
            let title = if volume.mount == "/" { c.t(T::SystemDisk).to_string() } else { volume.mount.clone() };
            let path = PathBuf::from(&volume.mount);
            group = group.push(
                Row::new(t, title)
                    .subtitle(format!("{} · {} · {}", volume.mount, volume.source, volume.fstype))
                    .prefix(widgets::tile(Icon::Drive, t.fill_3(), 28.0))
                    .suffix(
                        row![
                            column![
                                text(free_of(c, volume), TextStyle::CAP).color(t.fg_2()),
                                container(widgets::progress_bar(t, volume.fraction())).width(160),
                            ]
                            .spacing(4)
                            .align_x(Alignment::End),
                            widgets::icon_button(t, Icon::External, Some(Msg::Open(path))),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .build(),
            );
        }
        group.build()
    }
}
