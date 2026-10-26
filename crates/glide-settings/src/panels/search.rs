use std::collections::HashMap;
use std::path::PathBuf;

use glide_config::search_providers::{self, same_app};
use glide_ipc::{Request, Response};
use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::Column;
use iced::{Element, Task};

use super::apps::app_icon;
use super::common::{gated, switch_row, toggle};
use super::{Ctx, Pane, Panel};
use crate::i18n::Lang;
use crate::icons::Icon;
use crate::services::apps;
use crate::services::config::ConfigStore;
use crate::services::ipc;
use crate::services::keybinds;
use crate::services::run::blocking;
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Hero => "Search and launch", "Пошук і запуск";
        HeroHint => "Start typing in the launcher to find apps, windows, files and settings. Apps, their actions and system commands are always found.", "Почніть друкувати в запускачі, щоб знайти програми, вікна, файли й налаштування. Програми, їхні дії та системні команди знаходяться завжди.";
        Try => "Open search", "Відкрити пошук";
        Results => "What search finds", "Що знаходить пошук";
        Windows => "Open windows", "Відкриті вікна";
        WindowsHint => "By title or by the app they belong to", "За заголовком або програмою, якій вони належать";
        Files => "Recent files", "Нещодавні файли";
        FilesHint => "Files you opened lately, by name", "Файли, які ви нещодавно відкривали, за назвою";
        Settings => "Settings pages", "Сторінки налаштувань";
        SettingsHint => "Type “wi-fi” or “volume” to jump straight to the page", "Введіть «wi-fi» чи «гучність», щоб одразу перейти на сторінку";
        Providers => "Results from apps", "Результати з програм";
        ProvidersHint => "Apps that answer the search themselves, such as folders from Files", "Програми, що самі відповідають на пошук, як-от теки з «Файлів»";
        FromApps => "Apps that answer", "Програми, що відповідають";
        FromAppsHint => "Each app is asked only while it is running", "Програму питають лише тоді, коли вона запущена";
        NoProviders => "No installed app offers search results", "Жодна встановлена програма не пропонує результатів пошуку";
        Loading => "Looking for apps…", "Шукаємо програми…";
        Opening => "Opening search", "Як відкрити пошук";
        SuperTap => "Tap Super to open search", "Натискання Super відкриває пошук";
        SuperTapHint => "Press and release Super on its own; holding it for a shortcut never opens search", "Натисніть і відпустіть Super окремо; утримання для сполучення пошук не відкриває";
        Launcher => "Show all apps", "Показати всі програми";
        RunCommand => "Run a command", "Виконати команду";
        RunHint => "Type a command and press Enter to run it", "Введіть команду й натисніть Enter, щоб виконати її";
        Unbound => "Not set", "Не призначено";
        Change => "Change shortcuts", "Змінити сполучення";
        Tips => "Good to know", "Корисно знати";
        Calculator => "Calculator", "Калькулятор";
        CalculatorHint => "Type 12*7 or (3+4)/2: the answer comes first, Enter copies it", "Введіть 12*7 або (3+4)/2: відповідь буде першою, Enter копіює її";
        Commands => "System commands", "Системні команди";
        CommandsHint => "Lock, Sleep, Restart and Shut Down are found by name", "«Заблокувати», «Сон», «Перезапустити» й «Вимкнути» знаходяться за назвою";
    }
}

use tr::Key as T;

const TABLE: &str = "launcher";
const DISABLED: &str = "disabled-providers";

#[derive(Debug, Clone, Default)]
pub struct Provider {
    pub desktop_id: String,
    pub name: String,
    pub icon: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Lang, Vec<Provider>),
    Section(&'static str, bool),
    Provider(String, bool),
    SuperTap(bool),
    Try,
    Tried(Option<Result<(), String>>),
    Open(Panel),
}

#[derive(Debug, Default)]
pub struct Page {
    providers: Option<Vec<Provider>>,
    lang: Option<Lang>,
}

fn load(lang: Lang, theme: Option<&str>) -> Vec<Provider> {
    let found = search_providers::discover();
    if found.is_empty() {
        return Vec::new();
    }
    let installed = apps::installed(lang.key());
    let entry = |id: &str| installed.iter().find(|app| same_app(&app.id, id));
    let icons: HashMap<String, PathBuf> = apps::icon_paths(
        found.iter().filter_map(|provider| entry(&provider.desktop_id)).map(|app| app.icon.as_str()),
        theme,
    );
    let mut providers: Vec<Provider> = found
        .into_iter()
        .map(|provider| {
            let app = entry(&provider.desktop_id);
            Provider {
                name: app.map_or_else(|| provider.desktop_id.clone(), |app| app.name.clone()),
                icon: app.and_then(|app| icons.get(&app.icon).cloned()),
                desktop_id: provider.desktop_id,
            }
        })
        .collect();
    providers.sort_by_key(|provider| provider.name.to_lowercase());
    providers.dedup_by(|a, b| same_app(&a.desktop_id, &b.desktop_id));
    providers
}

fn open_launcher() -> Result<(), String> {
    match ipc::request(&Request::Action { name: String::from("toggle-launcher") })? {
        Response::Error { message } => Err(message),
        _ => Ok(()),
    }
}

#[must_use]
pub fn toggled(disabled: &[String], desktop_id: &str, on: bool) -> Vec<String> {
    let mut list: Vec<String> = disabled.iter().filter(|id| !same_app(id, desktop_id)).cloned().collect();
    if !on {
        list.push(desktop_id.to_string());
    }
    list
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "search",
        "launcher",
        "spotlight",
        "super",
        "calculator",
        "run",
        "пошук",
        "запускач",
        "калькулятор",
        "виконати",
    ];

    fn open(&mut self, c: Ctx, config: &ConfigStore) -> Task<Msg> {
        if self.lang == Some(c.lang) {
            return Task::none();
        }
        self.lang = Some(c.lang);
        let lang = c.lang;
        let theme = config.optional_str("dock", "icon-theme");
        Task::perform(blocking(move || load(lang, theme.as_deref())), move |providers| {
            Msg::Loaded(lang, providers.unwrap_or_default())
        })
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(lang, providers) => {
                if self.lang == Some(lang) {
                    self.providers = Some(providers);
                }
            }
            Msg::Section(key, on) => config.set_bool(TABLE, key, on),
            Msg::Provider(id, on) => {
                let list = toggled(&config.get_str_array(TABLE, DISABLED), &id, on);
                if list.is_empty() {
                    config.remove(TABLE, DISABLED);
                } else {
                    config.set_str_array(TABLE, DISABLED, &list);
                }
            }
            Msg::SuperTap(on) => config.set_bool(TABLE, "super-tap", on),
            Msg::Try => return Task::perform(blocking(open_launcher), Msg::Tried),
            Msg::Tried(_) | Msg::Open(_) => {}
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let hero = widgets::section_hero(
            t,
            Icon::Search,
            Panel::Search.color(),
            c.t(T::Hero),
            c.t(T::HeroHint),
            Some(widgets::secondary(t, c.t(T::Try), Some(Msg::Try))),
        );
        let section = |key: &'static str, title: T, hint: T| {
            switch_row(c, c.t(title), c.t(hint), config.bool_or(TABLE, key, true), move |on| Msg::Section(key, on))
        };
        let results = Group::new(t)
            .title(c.t(T::Results))
            .push(section("windows", T::Windows, T::WindowsHint))
            .push(section("recent-files", T::Files, T::FilesHint))
            .push(section("settings", T::Settings, T::SettingsHint))
            .push(section("providers", T::Providers, T::ProvidersHint));
        let mut page = Column::new().spacing(theme::SECTION_GAP).push(hero).push(results.build());
        if config.bool_or(TABLE, "providers", true) {
            page = page.push(self.providers_group(&c, config));
        }
        page.push(opening_group(&c, config)).push(tips_group(&c)).into()
    }
}

impl Page {
    fn providers_group<'a>(&'a self, c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let group = Group::new(t).title(c.t(T::FromApps)).description(c.t(T::FromAppsHint));
        let Some(providers) = &self.providers else {
            return group.push(Row::new(t, c.t(T::Loading)).dimmed(true).build()).build();
        };
        if providers.is_empty() {
            return group.push(Row::new(t, c.t(T::NoProviders)).dimmed(true).build()).build();
        }
        let disabled = config.get_str_array(TABLE, DISABLED);
        providers
            .iter()
            .fold(group, |group, provider| {
                let on = !disabled.iter().any(|id| same_app(id, &provider.desktop_id));
                let id = provider.desktop_id.clone();
                group.push(
                    Row::new(t, provider.name.clone())
                        .prefix(app_icon(&t, provider.icon.as_ref(), &provider.name, 28.0))
                        .suffix(toggle(*c, on, move |on| Msg::Provider(id.clone(), on)))
                        .build(),
                )
            })
            .build()
    }
}

fn shortcut<'a>(c: &Ctx, binds: &[keybinds::Keybind], action: &str) -> Element<'a, Msg> {
    match keybinds::combo_for(binds, action) {
        Some(combo) => widgets::key_combo(c.theme, &combo),
        None => text(c.t(T::Unbound), TextStyle::CALL).color(c.theme.fg_3()).into(),
    }
}

fn opening_group<'a>(c: &Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let binds = keybinds::current(config);
    Group::new(t)
        .title(c.t(T::Opening))
        .push(
            Row::new(t, c.t(T::SuperTap))
                .subtitle(c.t(T::SuperTapHint))
                .prefix(widgets::key_combo(t, "Super"))
                .suffix(toggle(*c, config.bool_or(TABLE, "super-tap", true), Msg::SuperTap))
                .build(),
        )
        .push(Row::new(t, c.t(T::Launcher)).suffix(shortcut(c, &binds, "toggle-launcher")).build())
        .push(
            Row::new(t, c.t(T::RunCommand))
                .subtitle(c.t(T::RunHint))
                .suffix(shortcut(c, &binds, "run-command"))
                .build(),
        )
        .push(
            Row::new(t, c.t(T::Change))
                .suffix(gated(*c, widgets::disclosure(t, None)))
                .on_press(Msg::Open(Panel::Shortcuts))
                .build(),
        )
        .build()
}

fn tips_group<'a>(c: &Ctx) -> Element<'a, Msg> {
    let t = c.theme;
    Group::new(t)
        .title(c.t(T::Tips))
        .push(
            Row::new(t, c.t(T::Calculator))
                .subtitle(c.t(T::CalculatorHint))
                .prefix(widgets::icon(Icon::Grid, 18.0, t.fg_2()))
                .build(),
        )
        .push(
            Row::new(t, c.t(T::Commands))
                .subtitle(c.t(T::CommandsHint))
                .prefix(widgets::icon(Icon::Command, 18.0, t.fg_2()))
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_a_provider_off_and_on_leaves_the_others_alone() {
        let off = toggled(&["org.gnome.Calculator".into()], "org.gnome.Nautilus", false);
        assert_eq!(off, ["org.gnome.Calculator", "org.gnome.Nautilus"]);
        let on = toggled(&off, "org.gnome.Nautilus.desktop", true);
        assert_eq!(on, ["org.gnome.Calculator"]);
    }

    #[test]
    fn every_section_switch_writes_a_key_the_compositor_reads() {
        let mut config = ConfigStore::parse("");
        for key in ["windows", "recent-files", "settings", "providers", "super-tap"] {
            config.set_bool(TABLE, key, false);
        }
        config.set_str_array(TABLE, DISABLED, &["org.gnome.Nautilus".to_string()]);
        let text = config.to_text();
        assert!(text.contains("[launcher]"), "{text}");
        for key in ["windows", "recent-files", "settings", "providers", "super-tap", DISABLED] {
            assert!(text.contains(key), "{key} missing from {text}");
        }
    }
}
