use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{column, row};
use iced::{Alignment, Element, Length, Task};

use super::apps::app_icon;
use super::common::{error_banner, gated, switch_row};
use super::{Ctx, Pane};
use crate::i18n::Lang;
use crate::icons::Icon;
use crate::services::apps::{self, DesktopApp};
use crate::services::autostart::{self, Entry};
use crate::services::config::ConfigStore;
use crate::services::run::{Generation, blocking};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Heading => "Startup apps", "Автозапуск";
        Starting => "start when you sign in", "запускаються під час входу";
        Nothing => "Nothing starts when you sign in", "Під час входу нічого не запускається";
        Commands => "Commands", "Команди";
        CommandsHint => "Glide runs them in this order every time you sign in", "Glide виконує їх у цьому порядку щоразу, коли ви входите";
        Skipped => "Skipped", "Пропущено";
        SkippedHint => "Entries that do not start in Glide, and why", "Записи, які в Glide не запускаються, і чому";
        AndMore => "and", "і ще";
        NoCommands => "No apps or commands yet", "Поки немає програм чи команд";
        AddApp => "Add an app", "Додати програму";
        ChooseApp => "Choose an app…", "Оберіть програму…";
        AddCommand => "Add a command", "Додати команду";
        CommandPlaceholder => "nm-applet --indicator", "nm-applet --indicator";
        Add => "Add", "Додати";
        Entries => "Apps", "Програми";
        EntriesHint => "Apps that start when you sign in", "Програми, що запускаються під час входу";
        Launch => "Start standard autostart entries", "Запускати стандартні записи автозапуску";
        LaunchHint => "Shared with other desktops, from ~/.config/autostart. Takes effect the next time you sign in", "Спільні з іншими середовищами, з ~/.config/autostart. Діє з наступного входу";
        PolkitAgent => "Ask for administrator passwords", "Запитувати пароль адміністратора";
        PolkitAgentHint => "Glide asks in the status island when an app needs administrator rights; turn it off only to use another polkit agent", "Glide запитує в острові, коли програмі потрібні права адміністратора; вимикайте, лише щоб користуватися іншим агентом polkit";
        NoEntries => "No autostart entries", "Немає записів автозапуску";
        OnlyIn => "Only for", "Лише для";
        NotHere => "Not for Glide", "Не для Glide";
        Replaced => "Glide does this itself. To start it anyway, add it under Commands", "Glide робить це сам. Щоб усе одно запускати, додайте його в «Команди»";
        Missing => "The program is not installed", "Програму не встановлено";
        Conditional => "Starts only when its own condition holds, checked at login", "Запускається лише коли виконується його умова; перевіряється під час входу";
        Yours => "Added by you", "Додано вами";
        System => "System", "Системний";
        Loading => "Loading…", "Завантаження…";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub apps: Vec<DesktopApp>,
    pub entries: Vec<Entry>,
    pub icons: HashMap<String, PathBuf>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(u64, Lang, Box<Catalog>),
    AddStartupApp(DesktopApp),
    Command(String),
    AddCommand,
    RemoveStartup(usize),
    MoveUp(usize),
    Launch(bool),
    PolkitAgent(bool),
    Toggle(String, bool),
    Remove(String),
    Done(Option<Result<(), String>>),
}

#[derive(Debug, Default)]
pub struct Page {
    catalog: Catalog,
    loaded: bool,
    loading: bool,
    command: String,
    error: Option<String>,
    lang: Option<Lang>,
    icon_theme: Option<String>,
    busy: bool,
    generation: Generation,
}

fn load(lang: Lang, theme: Option<&str>) -> Catalog {
    let apps = apps::installed(lang.key());
    let entries = autostart::scan(lang.key());
    let icons = apps::icon_paths(
        apps.iter().map(|app| app.icon.as_str()).chain(entries.iter().map(|entry| entry.icon.as_str())),
        theme,
    );
    Catalog { apps, entries, icons }
}

impl Page {
    fn refresh(&mut self, lang: Lang, config: &ConfigStore) -> Task<Msg> {
        self.loading = true;
        self.lang = Some(lang);
        let theme = config.optional_str("dock", "icon-theme");
        self.icon_theme.clone_from(&theme);
        let generation = self.generation.bump();
        Task::perform(blocking(move || load(lang, theme.as_deref())), move |catalog| {
            Msg::Loaded(generation, lang, Box::new(catalog.unwrap_or_default()))
        })
    }

    fn app_for(&self, command: &str) -> Option<&DesktopApp> {
        self.catalog.apps.iter().find(|app| apps::strip_field_codes(&app.exec) == command)
    }

    fn edit_entry(&mut self, id: &str, job: impl FnOnce(&Entry) -> Result<(), String> + Send + 'static) -> Task<Msg> {
        let Some(entry) = self.catalog.entries.iter().find(|entry| entry.id == id).cloned() else {
            return Task::none();
        };
        self.busy = true;
        self.error = None;
        Task::perform(blocking(move || job(&entry)), Msg::Done)
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.catalog.entries
    }
}

fn add_command(config: &mut ConfigStore, command: String) {
    let mut list = config.autostart();
    if !command.is_empty() && !list.contains(&command) {
        list.push(command);
        config.set_str_array("general", "autostart", &list);
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["autostart", "startup", "login", "sign in", "session", "автозапуск", "запуск", "вхід", "сеанс"];

    fn open(&mut self, c: Ctx, config: &ConfigStore) -> Task<Msg> {
        if (self.loaded || self.loading)
            && self.lang == Some(c.lang)
            && self.icon_theme == config.optional_str("dock", "icon-theme")
        {
            return Task::none();
        }
        self.refresh(c.lang, config)
    }

    fn update(&mut self, msg: Msg, c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(generation, lang, catalog) => {
                if self.generation.is_current(generation) && self.lang == Some(lang) {
                    self.catalog = *catalog;
                    self.loaded = true;
                    self.loading = false;
                }
            }
            Msg::AddStartupApp(app) => add_command(config, apps::strip_field_codes(&app.exec)),
            Msg::Command(command) => self.command = command,
            Msg::AddCommand => {
                add_command(config, self.command.trim().to_string());
                self.command.clear();
            }
            Msg::RemoveStartup(index) => {
                let mut list = config.autostart();
                if index < list.len() {
                    list.remove(index);
                    config.set_str_array("general", "autostart", &list);
                }
            }
            Msg::MoveUp(index) => {
                let mut list = config.autostart();
                if index > 0 && index < list.len() {
                    list.swap(index - 1, index);
                    config.set_str_array("general", "autostart", &list);
                }
            }
            Msg::Launch(enabled) => config.set_bool("general", "xdg-autostart", enabled),
            Msg::PolkitAgent(enabled) => config.set_bool("general", "polkit-agent", enabled),
            Msg::Toggle(id, enabled) => {
                if let Some(entry) = self.catalog.entries.iter_mut().find(|entry| entry.id == id) {
                    entry.enabled = enabled;
                }
                return self.edit_entry(&id, move |entry| autostart::set_enabled(entry, enabled));
            }
            Msg::Remove(id) => return self.edit_entry(&id, autostart::remove),
            Msg::Done(result) => {
                self.busy = false;
                if let Some(Err(err)) = result {
                    self.error = Some(err);
                }
                return self.refresh(c.lang, config);
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let c = &c;
        column![
            self.hero(c, config),
            error_banner(c.theme, self.error.as_deref()),
            self.apps_group(c, config),
            self.commands_group(c, config),
            self.skipped_group(c, config),
        ]
        .spacing(theme::SECTION_GAP)
        .into()
    }
}

impl Page {
    fn launches_entries(config: &ConfigStore) -> bool {
        config.bool_or("general", "xdg-autostart", true)
    }

    fn hero<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let commands = config.autostart().len();
        let entries = if Self::launches_entries(config) {
            self.catalog.entries.iter().filter(|entry| entry.starts()).count()
        } else {
            0
        };
        let total = commands + entries;
        let description =
            if total == 0 { c.t(T::Nothing).to_string() } else { format!("{total} {}", c.t(T::Starting)) };
        let trailing = (self.loading || self.busy).then(|| widgets::spinner_label(t, c.t(T::Loading)));
        widgets::section_hero(t, Icon::Rocket, super::Panel::Startup.color(), c.t(T::Heading), description, trailing)
    }

    fn commands_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let list = config.autostart();
        let mut group = Group::new(t).title(c.t(T::Commands)).description(c.t(T::CommandsHint));
        if list.is_empty() {
            group = group.push(Row::new(t, c.t(T::NoCommands)).dimmed(true).build());
        }
        for (index, command) in list.iter().enumerate() {
            let app = self.app_for(command);
            let name = app.map_or_else(|| command.clone(), |app| app.name.clone());
            let prefix: Element<'a, Msg> = match app {
                Some(app) => app_icon(&t, self.catalog.icons.get(&app.icon), &app.name, 28.0),
                None => widgets::tile(Icon::Command, t.fill_3(), 28.0),
            };
            let mut actions = row![].spacing(2).align_y(Alignment::Center);
            if index > 0 {
                actions =
                    actions.push(widgets::icon_button(t, Icon::ChevronUp, c.writable.then_some(Msg::MoveUp(index))));
            }
            actions =
                actions.push(widgets::icon_button(t, Icon::Trash, c.writable.then_some(Msg::RemoveStartup(index))));
            group = group.push(Row::new(t, name).subtitle(command.clone()).prefix(prefix).suffix(actions).build());
        }
        let launchable: Vec<DesktopApp> = self
            .catalog
            .apps
            .iter()
            .filter(|app| !app.terminal && !app.no_display)
            .filter(|app| !list.contains(&apps::strip_field_codes(&app.exec)))
            .cloned()
            .collect();
        group = group
            .push(
                Row::new(t, c.t(T::AddApp))
                    .prefix(widgets::icon(Icon::Plus, 18.0, t.acc_text()))
                    .suffix(gated(*c, widgets::chooser(t, launchable, c.t(T::ChooseApp), Msg::AddStartupApp, 260.0)))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::AddCommand))
                    .prefix(widgets::icon(Icon::Command, 18.0, t.fg_2()))
                    .suffix(gated(
                        *c,
                        widgets::flexible(
                            row![
                                widgets::field(
                                    t,
                                    c.t(T::CommandPlaceholder),
                                    &self.command,
                                    Msg::Command,
                                    Some(Msg::AddCommand),
                                    false,
                                )
                                .width(Length::Fill),
                                widgets::secondary(
                                    t,
                                    c.t(T::Add),
                                    (!self.command.trim().is_empty()).then_some(Msg::AddCommand),
                                ),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            300.0,
                        ),
                    ))
                    .build(),
            );
        group.build()
    }

    fn apps_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let launching = Self::launches_entries(config);
        let group = Group::new(t).title(c.t(T::Entries)).description(c.t(T::EntriesHint));
        if !self.loaded {
            return group.push(Row::new(t, c.t(T::Loading)).dimmed(true).build()).build();
        }
        let applying: Vec<&Entry> = self.catalog.entries.iter().filter(|entry| skip_reason(entry).is_none()).collect();
        if applying.is_empty() {
            return group.push(Row::new(t, c.t(T::NoEntries)).dimmed(true).build()).build();
        }
        applying
            .into_iter()
            .fold(group, |group, entry| {
                let subtitle = if entry.conditional {
                    c.t(T::Conditional).to_string()
                } else if entry.removable() {
                    c.t(T::Yours).to_string()
                } else if entry.comment.is_empty() {
                    entry.exec.clone()
                } else {
                    entry.comment.clone()
                };
                let id = entry.id.clone();
                let mut suffix = row![widgets::inert(
                    t,
                    launching && !self.busy,
                    widgets::switch(t, entry.enabled, move |on| Msg::Toggle(id.clone(), on)),
                )]
                .spacing(8)
                .align_y(Alignment::Center);
                if entry.removable() {
                    suffix = suffix.push(widgets::icon_button(
                        t,
                        Icon::Trash,
                        (!self.busy).then_some(Msg::Remove(entry.id.clone())),
                    ));
                }
                group.push(
                    Row::new(t, entry.name.clone())
                        .subtitle(subtitle)
                        .prefix(app_icon(&t, self.catalog.icons.get(&entry.icon), &entry.name, 28.0))
                        .suffix(suffix)
                        .dimmed(!launching)
                        .build(),
                )
            })
            .build()
    }

    fn skipped_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let mut lines: Vec<(Skip, Vec<&Entry>)> = Vec::new();
        for entry in &self.catalog.entries {
            let Some(reason) = skip_reason(entry) else { continue };
            match lines.iter_mut().find(|(known, _)| matches!(known, Skip::Elsewhere(_)) && *known == reason) {
                Some((_, entries)) => entries.push(entry),
                None => lines.push((reason, vec![entry])),
            }
        }
        let mut group = Group::new(t).title(c.t(T::Skipped)).description(c.t(T::SkippedHint));
        for (reason, entries) in lines {
            let first = entries[0];
            let title = if entries.len() > 1 {
                format!("{} {} {}", first.name, c.t(T::AndMore), entries.len() - 1)
            } else {
                first.name.clone()
            };
            let subtitle = match reason {
                Skip::Replaced => c.t(T::Replaced).to_string(),
                Skip::Missing => c.t(T::Missing).to_string(),
                Skip::Elsewhere(only_in) if only_in.is_empty() => c.t(T::NotHere).to_string(),
                Skip::Elsewhere(only_in) => format!("{} {}", c.t(T::OnlyIn), only_in),
            };
            group = group.push(
                Row::new(t, title)
                    .subtitle(subtitle)
                    .prefix(app_icon(&t, self.catalog.icons.get(&first.icon), &first.name, 28.0))
                    .dimmed(true)
                    .build(),
            );
        }
        group
            .push(switch_row(*c, c.t(T::Launch), c.t(T::LaunchHint), Self::launches_entries(config), Msg::Launch))
            .push(switch_row(
                *c,
                c.t(T::PolkitAgent),
                c.t(T::PolkitAgentHint),
                config.bool_or("general", "polkit-agent", true),
                Msg::PolkitAgent,
            ))
            .build()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    Elsewhere(String),
    Replaced,
    Missing,
}

#[must_use]
pub(crate) fn skip_reason(entry: &Entry) -> Option<Skip> {
    if !entry.desktop_ok {
        Some(Skip::Elsewhere(entry.only_in.join(", ")))
    } else if entry.replaced {
        Some(Skip::Replaced)
    } else if entry.runnable {
        None
    } else {
        Some(Skip::Missing)
    }
}
