use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{column, container, image, row, svg, text};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::i18n::Lang;
use crate::icons::Icon;
use crate::services::apps::{self, DesktopApp, FILE_TYPES, FileType, Kind, MimeList, Role};
use crate::services::config::ConfigStore;
use crate::services::run::{Generation, blocking};
use crate::theme::{self, ThemeState, Tone};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Heading => "Default apps", "Типові програми";
        Summary => "apps installed", "встановлених програм";
        Handled => "file types have an app", "типів файлів мають програму";
        Roles => "Everyday", "Щоденне";
        RolesHint => "Links, mail and the files you open most", "Посилання, пошта й файли, які ви відкриваєте найчастіше";
        RoleWeb => "Web browser", "Браузер";
        RoleMail => "Mail", "Пошта";
        RoleCalendar => "Calendar", "Календар";
        RoleFiles => "Files", "Файли";
        RoleText => "Text editor", "Текстовий редактор";
        RoleImages => "Photos", "Фото";
        RoleMusic => "Music", "Музика";
        RoleVideo => "Video", "Відео";
        RolePdf => "Documents (PDF)", "Документи (PDF)";
        NoHandlers => "No installed app", "Немає встановлених програм";
        FileTypes => "File types", "Типи файлів";
        FileTypesHint => "Pick the app for one kind of file or link", "Виберіть програму для окремого типу файлів чи посилань";
        SearchTypes => "Search: pdf, .mkv, image/png…", "Пошук: pdf, .mkv, image/png…";
        All => "All", "Усі";
        Documents => "Documents", "Документи";
        Images => "Images", "Зображення";
        Audio => "Audio", "Аудіо";
        Video => "Video", "Відео";
        Archives => "Archives", "Архіви";
        Code => "Code", "Код";
        Links => "Links", "Посилання";
        NoTypes => "No matching file types", "Немає відповідних типів файлів";
        Automatic => "Chosen automatically", "Вибрано автоматично";
        Reset => "Reset", "Скидання";
        ResetRow => "Reset to system defaults", "Відновити системні типові";
        ResetHint => "Forget every choice made here and in other apps", "Забути всі вибори, зроблені тут та в інших програмах";
        ResetButton => "Reset…", "Скинути…";
        ResetConfirm => "Reset all", "Скинути все";
        Cancel => "Cancel", "Скасувати";
        ResetDone => "Your choices were removed; system defaults apply again.", "Ваші вибори видалено; знову діють системні типові.";
        NothingToReset => "There were no choices of your own to remove.", "Власних виборів, які можна видалити, не було.";
        Loading => "Loading…", "Завантаження…";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub apps: Vec<DesktopApp>,
    pub lists: Vec<MimeList>,
    pub descriptions: HashMap<&'static str, String>,
    pub icons: HashMap<String, PathBuf>,
}

impl Catalog {
    #[must_use]
    pub fn default_for(&self, mime: &str) -> Option<String> {
        apps::resolve_default(&self.lists, &self.apps, mime)
    }

    #[must_use]
    pub fn icon_of(&self, id: &str) -> Option<&PathBuf> {
        let app = self.apps.iter().find(|app| app.id == id)?;
        self.icons.get(&app.icon)
    }

    #[must_use]
    pub fn describe(&self, file: &FileType) -> String {
        self.descriptions.get(file.mime).cloned().unwrap_or_else(|| file.mime.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Lang, Box<Catalog>),
    SetDefault(Role, DesktopApp),
    SetType(&'static str, DesktopApp),
    Done(Option<Result<String, String>>),
    Filter(Option<Kind>),
    Query(String),
    AskReset,
    CancelReset,
    ConfirmReset,
    ResetDone(Option<Result<usize, String>>),
}

#[allow(clippy::struct_excessive_bools)] // Independent view flags: loaded, loading, busy, confirming
#[derive(Debug, Default)]
pub struct Page {
    catalog: Catalog,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    notice: Option<T>,
    lang: Option<Lang>,
    overrides: HashMap<String, String>,
    kind: Option<Kind>,
    query: String,
    confirm_reset: bool,
    busy: bool,
    generation: Generation,
}

const fn role_label(role: Role) -> (T, Icon) {
    match role {
        Role::Web => (T::RoleWeb, Icon::Globe),
        Role::Mail => (T::RoleMail, Icon::Bell),
        Role::Calendar => (T::RoleCalendar, Icon::Clock),
        Role::Files => (T::RoleFiles, Icon::Drive),
        Role::Text => (T::RoleText, Icon::Keyboard),
        Role::Images => (T::RoleImages, Icon::Image),
        Role::Music => (T::RoleMusic, Icon::Headphones),
        Role::Video => (T::RoleVideo, Icon::Display),
        Role::Pdf => (T::RolePdf, Icon::Printer),
    }
}

const fn kind_key(kind: Kind) -> T {
    match kind {
        Kind::Documents => T::Documents,
        Kind::Images => T::Images,
        Kind::Audio => T::Audio,
        Kind::Video => T::Video,
        Kind::Archives => T::Archives,
        Kind::Code => T::Code,
        Kind::Links => T::Links,
    }
}

fn load(lang: Lang, theme: Option<&str>) -> Catalog {
    let apps = apps::installed(lang.key());
    let icons = apps::icon_paths(apps.iter().map(|app| app.icon.as_str()), theme);
    Catalog { lists: apps::load_mimeapps(), descriptions: apps::mime_descriptions(lang.key()), icons, apps }
}

#[must_use]
pub fn app_icon<'a, M: 'a>(t: &ThemeState, path: Option<&PathBuf>, name: &str, size: f32) -> Element<'a, M> {
    if let Some(path) = path {
        if path.extension().is_some_and(|ext| ext == "svg") {
            return svg(svg::Handle::from_path(path)).width(size).height(size).into();
        }
        return image(image::Handle::from_path(path)).width(size).height(size).into();
    }
    let letter: String =
        name.chars().find(|c| c.is_alphanumeric()).map_or_else(|| String::from("?"), |c| c.to_uppercase().collect());
    let well = t.fill_2();
    container(text(letter).size((size * 0.46).round()).font(theme::SEMIBOLD).color(t.fg_2()))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_: &iced::Theme| theme::solid(well, theme::radius_r2()))
        .into()
}

#[must_use]
pub fn matches_query(file: &FileType, description: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let bare = query.trim_start_matches('.');
    file.mime.contains(&query)
        || file.extension.trim_start_matches('.').starts_with(bare)
        || description.to_lowercase().contains(&query)
}

impl Page {
    fn refresh(&mut self, lang: Lang, config: &ConfigStore) -> Task<Msg> {
        self.loading = true;
        self.lang = Some(lang);
        let generation = self.generation.bump();
        let theme = config.optional_str("dock", "icon-theme");
        Task::perform(blocking(move || load(lang, theme.as_deref())), move |catalog| {
            Msg::Loaded(generation, lang, Box::new(catalog.unwrap_or_default()))
        })
    }

    fn current(&self, mime: &str) -> Option<String> {
        self.overrides.get(mime).cloned().or_else(|| self.catalog.default_for(mime))
    }

    #[must_use]
    pub fn visible_types(&self) -> Vec<&'static FileType> {
        FILE_TYPES
            .iter()
            .filter(|file| {
                if self.query.trim().is_empty() {
                    self.kind.is_none_or(|kind| file.kind == kind)
                } else {
                    matches_query(file, &self.catalog.describe(file), &self.query)
                }
            })
            .collect()
    }

    fn set(&mut self, mimes: &'static [&'static str], app: &DesktopApp) -> Task<Msg> {
        for mime in mimes {
            self.overrides.insert((*mime).to_string(), app.id.clone());
        }
        self.notice = None;
        self.busy = true;
        let id = app.id.clone();
        Task::perform(blocking(move || apps::set_default_mimes(mimes, &id)), Msg::Done)
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "default",
        "browser",
        "mail",
        "calendar",
        "file type",
        "mime",
        "open with",
        "associations",
        "типові",
        "браузер",
        "пошта",
        "календар",
        "тип файлу",
        "відкривати",
    ];

    fn open(&mut self, c: Ctx, config: &ConfigStore) -> Task<Msg> {
        if (self.loaded || self.loading) && self.lang == Some(c.lang) {
            return Task::none();
        }
        self.refresh(c.lang, config)
    }

    fn update(&mut self, msg: Msg, c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, lang, catalog) => {
                if self.generation.is_current(generation) && self.lang == Some(lang) {
                    self.catalog = *catalog;
                    self.overrides.clear();
                    self.loaded = true;
                    self.loading = false;
                }
            }
            Msg::SetDefault(role, app) => return self.set(role.mime_types(), &app),
            Msg::SetType(mime, app) => {
                if let Some(file) = FILE_TYPES.iter().find(|file| file.mime == mime) {
                    return self.set(std::slice::from_ref(&file.mime), &app);
                }
            }
            Msg::Done(result) => {
                self.busy = false;
                self.error = result.and_then(Result::err);
                return self.refresh(c.lang, config);
            }
            Msg::Filter(kind) => {
                self.kind = kind;
                self.query.clear();
            }
            Msg::Query(query) => self.query = query,
            Msg::AskReset => self.confirm_reset = true,
            Msg::CancelReset => self.confirm_reset = false,
            Msg::ConfirmReset => {
                self.confirm_reset = false;
                self.busy = true;
                return Task::perform(blocking(|| apps::reset_defaults(&apps::managed_mime_types())), Msg::ResetDone);
            }
            Msg::ResetDone(result) => {
                self.busy = false;
                match result {
                    Some(Ok(0)) => self.notice = Some(T::NothingToReset),
                    Some(Ok(_)) => self.notice = Some(T::ResetDone),
                    Some(Err(err)) => self.error = Some(err),
                    None => {}
                }
                return self.refresh(c.lang, config);
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        let t = c.theme;
        if !self.loaded {
            return widgets::empty_state(t, Icon::Apps, c.t(T::Loading), "");
        }
        column![
            self.hero(c),
            error_banner(t, self.error.as_deref()),
            self.notice.map(|notice| widgets::banner(t, t.ok(), Icon::Check, c.t(notice), None)),
            self.roles_group(c),
            self.types_group(c),
            self.reset_group(c),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn hero(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let visible = self.catalog.apps.iter().filter(|app| !app.no_display).count();
        let handled = FILE_TYPES.iter().filter(|file| self.current(file.mime).is_some()).count();
        let trailing: Element<'_, Msg> = if self.busy || self.loading {
            widgets::spinner_label(t, c.t(T::Loading))
        } else {
            widgets::pill(t, Tone::Accent, format!("{handled}/{} {}", FILE_TYPES.len(), c.t(T::Handled)))
        };
        widgets::section_hero(
            t,
            Icon::Apps,
            super::Panel::Apps.color(),
            c.t(T::Heading),
            format!("{visible} {}", c.t(T::Summary)),
            Some(trailing),
        )
    }

    fn app_dropdown<'a>(
        &'a self, c: &Ctx, mimes: &[&str], current: Option<&str>, on_select: impl Fn(DesktopApp) -> Msg + 'a,
    ) -> Element<'a, Msg> {
        let t = c.theme;
        let options = apps::mime_options(&self.catalog.apps, mimes, current);
        if options.is_empty() {
            return text(c.t(T::NoHandlers)).size(13).color(t.fg_3()).into();
        }
        let selected = current.and_then(|id| options.iter().find(|app| app.id == id).cloned());
        widgets::inert(t, !self.busy, widgets::dropdown(t, options, selected, on_select, 240.0))
    }

    fn prefix_for(&self, t: &ThemeState, current: Option<&String>, fallback: Icon) -> Element<'_, Msg> {
        match current {
            Some(id) => {
                let name = apps::resolve(&self.catalog.apps, id).name;
                app_icon(t, self.catalog.icon_of(id), &name, 28.0)
            }
            None => widgets::tile(fallback, t.fill_3(), 28.0),
        }
    }

    fn roles_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let mut group = Group::new(t).title(c.t(T::Roles)).description(c.t(T::RolesHint));
        for role in Role::ALL {
            let role = *role;
            let mime = role.mime_types()[0];
            let current = self.current(mime);
            let explicit = self.overrides.contains_key(mime) || apps::explicit_default(&self.catalog.lists, mime);
            let (label, glyph) = role_label(role);
            let mut line = Row::new(t, c.t(label)).prefix(self.prefix_for(&t, current.as_ref(), glyph)).suffix(
                self.app_dropdown(c, role.mime_types(), current.as_deref(), move |app| Msg::SetDefault(role, app)),
            );
            if current.is_some() && !explicit {
                line = line.subtitle(c.t(T::Automatic));
            }
            group = group.push(line.build());
        }
        group.build()
    }

    fn types_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let searching = !self.query.trim().is_empty();
        let chip = |label, kind: Option<Kind>| {
            widgets::chip(t, label, self.kind == kind && !searching, Some(Msg::Filter(kind)))
        };
        let chips = row(std::iter::once(chip(c.t(T::All), None))
            .chain(Kind::ALL.iter().map(|kind| chip(c.t(kind_key(*kind)), Some(*kind)))))
        .spacing(6);
        let controls = column![
            widgets::field(t, c.t(T::SearchTypes), &self.query, Msg::Query, None, false).width(Fill),
            chips.wrap().vertical_spacing(6),
        ]
        .spacing(10);
        let mut group = Group::new(t).title(c.t(T::FileTypes)).description(c.t(T::FileTypesHint));
        let files = self.visible_types();
        if files.is_empty() {
            group = group.push(Row::new(t, c.t(T::NoTypes)).dimmed(true).build());
        }
        for file in files {
            let mime = file.mime;
            let current = self.current(mime);
            group = group.push(
                Row::new(t, self.catalog.describe(file))
                    .subtitle(format!("{} · {}", file.extension, file.mime))
                    .prefix(self.prefix_for(&t, current.as_ref(), Icon::Apps))
                    .suffix(self.app_dropdown(c, &[mime], current.as_deref(), move |app| Msg::SetType(mime, app)))
                    .build(),
            );
        }
        column![container(controls).padding([0, 4]), group.build()].spacing(12).into()
    }

    fn reset_group(&self, c: &Ctx) -> Element<'_, Msg> {
        let t = c.theme;
        let suffix: Element<'_, Msg> = if self.confirm_reset {
            row![
                widgets::secondary(t, c.t(T::Cancel), Some(Msg::CancelReset)),
                widgets::danger(t, c.t(T::ResetConfirm), Some(Msg::ConfirmReset)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else {
            widgets::danger(t, c.t(T::ResetButton), (!self.busy).then_some(Msg::AskReset))
        };
        Group::new(t)
            .title(c.t(T::Reset))
            .push(Row::new(t, c.t(T::ResetRow)).subtitle(c.t(T::ResetHint)).suffix(suffix).build())
            .build()
    }
}
