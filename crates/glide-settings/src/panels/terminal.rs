use std::path::PathBuf;

use glide_terminal::config::{Cursor, OnExit, Preferences};
use glide_terminal::profile::{Profile, color};
use iced::widget::{Column, container, row, text};
use iced::{Alignment, Element, Fill, Length, Task};
use toml_edit::{DocumentMut, Item, value};

use super::common::error_banner;
use super::{Ctx, Pane};
use crate::services::apps;
use crate::services::config::ConfigStore;
use crate::services::run::blocking;
use crate::theme::{self, ThemeState};
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Profile => "Profile", "Профіль";
        ProfileHint => "Colours of new tabs and of tabs that still use the old default", "Кольори нових вкладок і тих, що досі мають попередній типовий профіль";
        Text => "Text", "Текст";
        TextHint => "The face, size and spacing of the terminal grid", "Шрифт, розмір і відстань між рядками сітки термінала";
        Font => "Font", "Шрифт";
        Bundled => "JetBrains Mono (built in)", "JetBrains Mono (вбудований)";
        Size => "Size", "Розмір";
        LineHeight => "Line spacing", "Міжрядковий інтервал";
        BoldBright => "Bright colours for bold text", "Яскраві кольори для жирного тексту";
        BoldBrightHint => "As in Apple's Terminal, bold text in the first eight colours uses their bright pair", "Як в Apple Terminal: жирний текст у перших восьми кольорах бере їхню яскраву пару";
        Cursor => "Cursor", "Курсор";
        Shape => "Shape", "Форма";
        Block => "Block", "Блок";
        Beam => "Bar", "Риска";
        Underline => "Underline", "Підкреслення";
        Blink => "Blink", "Блимання";
        BlinkHint => "Only while the window is focused and animations are on", "Лише коли вікно у фокусі й анімації ввімкнено";
        Bell => "Bell", "Дзвінок";
        VisualBell => "Flash the pane", "Блимати панеллю";
        AudibleBell => "Play a sound", "Відтворювати звук";
        AudibleBellHint => "Muted together with notification sounds", "Вимикається разом зі звуками сповіщень";
        Shell => "Shell", "Оболонка";
        ShellHint => "Command for new tabs and splits", "Команда для нових вкладок і панелей";
        ShellPlaceholder => "Login shell ($SHELL)", "Оболонка входу ($SHELL)";
        OnExit => "When the shell exits", "Коли оболонка завершується";
        CloseAlways => "Close the pane", "Закрити панель";
        CloseClean => "Close if it exited cleanly", "Закрити, якщо без помилки";
        Keep => "Keep it open", "Лишити відкритою";
        Behaviour => "Behaviour", "Поведінка";
        ConfirmClose => "Ask before closing running processes", "Питати перед закриттям запущених процесів";
        MarkPrompts => "Mark prompt lines automatically", "Автоматично позначати рядки запрошення";
        MarkPromptsHint => "Return marks its line; jump between marks with Ctrl+Shift+↑ and ↓", "Return позначає свій рядок; переходьте між позначками через Ctrl+Shift+↑ і ↓";
        AltMeta => "Alt sends Escape", "Alt надсилає Escape";
        AltMetaHint => "Like Apple's \"Use Option as Meta key\", for readline and Emacs", "Як «Use Option as Meta key» в Apple, для readline та Emacs";
        CopyOnSelect => "Copy selected text to the primary selection", "Копіювати виділення в основний буфер";
        Scrollback => "Scrollback", "Історія прокрутки";
        Lines => "lines", "рядків";
        Unwritable => "terminal.toml could not be read, so it is left untouched", "Не вдалося прочитати terminal.toml, тож його не змінено";
    }
}

const SIZES: [u8; 13] = [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24];
const SPACINGS: [u8; 7] = [100, 110, 120, 125, 130, 140, 150];
const SCROLLBACK: [u32; 6] = [1_000, 5_000, 10_000, 50_000, 100_000, 200_000];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labelled<T> {
    value: T,
    label: String,
}

impl<T> std::fmt::Display for Labelled<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    BoldBright,
    Blink,
    VisualBell,
    AudibleBell,
    ConfirmClose,
    MarkPrompts,
    AltMeta,
    CopyOnSelect,
}

impl Switch {
    const fn key(self) -> &'static str {
        match self {
            Self::BoldBright => "bold-bright",
            Self::Blink => "cursor-blink",
            Self::VisualBell => "visual-bell",
            Self::AudibleBell => "audible-bell",
            Self::ConfirmClose => "confirm-close",
            Self::MarkPrompts => "mark-prompts",
            Self::AltMeta => "alt-sends-escape",
            Self::CopyOnSelect => "copy-on-select",
        }
    }

    const fn read(self, prefs: &Preferences) -> bool {
        match self {
            Self::BoldBright => prefs.bold_bright,
            Self::Blink => prefs.cursor_blink,
            Self::VisualBell => prefs.visual_bell,
            Self::AudibleBell => prefs.audible_bell,
            Self::ConfirmClose => prefs.confirm_close,
            Self::MarkPrompts => prefs.mark_prompts,
            Self::AltMeta => prefs.alt_sends_escape,
            Self::CopyOnSelect => prefs.copy_on_select,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Fonts(Vec<String>),
    Profile(Profile),
    Font(Labelled<Option<String>>),
    Size(u8),
    Spacing(Labelled<u8>),
    Cursor(Cursor),
    Switch(Switch, bool),
    OnExit(Labelled<OnExit>),
    Scrollback(Labelled<u32>),
    ShellInput(String),
    ShellSubmit,
}

#[derive(Debug)]
struct Store {
    path: PathBuf,
    doc: Option<DocumentMut>,
    prefs: Preferences,
}

impl Store {
    fn load() -> Option<Self> {
        let path = glide_terminal::config::path()?;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        Some(Self { doc: text.parse().ok(), prefs: Preferences::parse(&text), path })
    }

    fn write(&mut self, key: &str, item: Option<Item>) -> Result<(), String> {
        let doc = self.doc.as_mut().ok_or_else(String::new)?;
        match item {
            Some(item) => doc[key] = item,
            None => {
                doc.remove(key);
            }
        }
        let text = doc.to_string();
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let target = std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        crate::services::config::write_atomic(&target, text.as_bytes()).map_err(|error| error.to_string())?;
        self.prefs = Preferences::parse(&text);
        Ok(())
    }
}

fn kebab<T: serde::Serialize>(item: T) -> String {
    serde_json::to_value(item).ok().and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default()
}

#[derive(Debug, Default)]
pub struct Page {
    store: Option<Store>,
    fonts: Vec<String>,
    shell: String,
    error: Option<String>,
}

fn monospace_families() -> Vec<String> {
    crate::services::run::output("fc-list", &[":spacing=mono", "family"])
        .map(|text| apps::parse_font_families(&text))
        .unwrap_or_default()
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] =
        &["terminal", "shell", "console", "profile", "font", "термінал", "оболонка", "консоль", "профіль", "шрифт"];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.store = Store::load();
        self.shell = self.store.as_ref().and_then(|store| store.prefs.shell.clone()).unwrap_or_default();
        Task::perform(blocking(monospace_families), |fonts| Msg::Fonts(fonts.unwrap_or_default()))
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        let change: (&str, Option<Item>) = match msg {
            Msg::Fonts(fonts) => {
                self.fonts = fonts;
                return Task::none();
            }
            Msg::ShellInput(shell) => {
                self.shell = shell;
                return Task::none();
            }
            Msg::Profile(profile) => ("profile", Some(value(kebab(profile)))),
            Msg::Font(choice) => ("font-family", choice.value.map(value)),
            Msg::Size(size) => ("font-size", Some(value(f64::from(size)))),
            Msg::Spacing(spacing) => ("line-height", Some(value(f64::from(spacing.value) / 100.0))),
            Msg::Cursor(cursor) => ("cursor", Some(value(kebab(cursor)))),
            Msg::Switch(switch, on) => (switch.key(), Some(value(on))),
            Msg::OnExit(on_exit) => ("on-exit", Some(value(kebab(on_exit.value)))),
            Msg::Scrollback(lines) => ("scrollback", Some(value(i64::from(lines.value)))),
            Msg::ShellSubmit => {
                let shell = self.shell.trim().to_string();
                ("shell", (!shell.is_empty()).then(|| value(shell)))
            }
        };
        let (key, item) = change;
        if let Some(store) = self.store.as_mut() {
            self.error = store.write(key, item).err();
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, _config: &'a ConfigStore) -> Element<'a, Msg> {
        let Some(store) = &self.store else { return Column::new().into() };
        let prefs = &store.prefs;
        let mut page = Column::new().spacing(theme::SECTION_GAP);
        if store.doc.is_none() {
            page = page.push(widgets::banner(
                c.theme,
                c.theme.danger(),
                crate::icons::Icon::Warning,
                c.t(tr::Key::Unwritable),
                None,
            ));
        }
        if let Some(banner) = error_banner(c.theme, self.error.as_deref().filter(|error| !error.is_empty())) {
            page = page.push(banner);
        }
        page.push(profiles_group(&c, prefs))
            .push(self.text_group(&c, prefs))
            .push(cursor_group(&c, prefs))
            .push(bell_group(&c, prefs))
            .push(self.shell_group(&c, prefs))
            .into()
    }
}

fn swatch<'a>(profile: Profile, t: &ThemeState) -> Element<'a, Msg> {
    let palette = profile.palette(t.tokens());
    let dots = row([1, 2, 3, 4].map(|index| {
        let tint = color(palette.ansi[index], 1.0);
        container(iced::widget::space().width(6).height(6))
            .style(move |_: &iced::Theme| theme::solid(tint, iced::border::Radius::from(3.0)))
            .into()
    }))
    .spacing(3);
    let background = color(palette.background, 1.0);
    container(
        row![
            text("Aa").size(16).font(glide_terminal::MONO).color(color(palette.foreground, 1.0)),
            iced::widget::space().width(Fill),
            dots
        ]
        .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .width(Length::Fixed(124.0))
    .height(Length::Fixed(54.0))
    .align_y(Alignment::Center)
    .style(move |_: &iced::Theme| theme::solid(background, theme::radius_r2()))
    .into()
}

fn profiles_group<'a>(c: &Ctx, prefs: &Preferences) -> Element<'a, Msg> {
    let t = c.theme;
    let rows = Profile::ALL.chunks(if t.compact { 3 } else { 4 }).map(|chunk| {
        row(chunk.iter().map(|profile| {
            widgets::preview_card(
                t,
                swatch(*profile, &t),
                profile.name(),
                *profile == prefs.profile,
                Some(Msg::Profile(*profile)),
            )
        }))
        .spacing(16)
        .into()
    });
    Group::new(t)
        .title(c.t(tr::Key::Profile))
        .description(c.t(tr::Key::ProfileHint))
        .push(container(Column::with_children(rows).spacing(14)).center_x(Fill).padding([12, 8]))
        .build()
}

impl Page {
    fn text_group<'a>(&'a self, c: &Ctx, prefs: &Preferences) -> Element<'a, Msg> {
        let t = c.theme;
        let fonts: Vec<Labelled<Option<String>>> =
            std::iter::once(Labelled { value: None, label: c.t(tr::Key::Bundled).to_string() })
                .chain(self.fonts.iter().map(|family| Labelled { value: Some(family.clone()), label: family.clone() }))
                .collect();
        let current = prefs.font_family.clone();
        let selected = fonts
            .iter()
            .find(|option| option.value == current)
            .cloned()
            .or_else(|| current.map(|family| Labelled { value: Some(family.clone()), label: family }));
        let size = SIZES.iter().copied().min_by_key(|size| (f32::from(*size) - prefs.font_size).abs() as u32);
        let spacings: Vec<Labelled<u8>> = SPACINGS
            .iter()
            .map(|percent| Labelled { value: *percent, label: format!("{:.2}", f32::from(*percent) / 100.0) })
            .collect();
        let spacing = spacings
            .iter()
            .min_by_key(|option| (f32::from(option.value) - prefs.line_height * 100.0).abs() as u32)
            .cloned();
        Group::new(t)
            .title(c.t(tr::Key::Text))
            .description(c.t(tr::Key::TextHint))
            .push(
                Row::new(t, c.t(tr::Key::Font)).suffix(widgets::dropdown(t, fonts, selected, Msg::Font, 260.0)).build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::Size))
                    .suffix(widgets::dropdown(t, SIZES.to_vec(), size, Msg::Size, 120.0))
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::LineHeight))
                    .suffix(widgets::dropdown(t, spacings, spacing, Msg::Spacing, 120.0))
                    .build(),
            )
            .push(switch(c, prefs, Switch::BoldBright, tr::Key::BoldBright, Some(tr::Key::BoldBrightHint)))
            .build()
    }

    fn shell_group<'a>(&'a self, c: &Ctx, prefs: &Preferences) -> Element<'a, Msg> {
        let t = c.theme;
        let exits: Vec<Labelled<OnExit>> = [
            (OnExit::Close, tr::Key::CloseAlways),
            (OnExit::CloseClean, tr::Key::CloseClean),
            (OnExit::Keep, tr::Key::Keep),
        ]
        .into_iter()
        .map(|(value, label)| Labelled { value, label: c.t(label).to_string() })
        .collect();
        let exit = exits.iter().find(|option| option.value == prefs.on_exit).cloned();
        let lines: Vec<Labelled<u32>> = SCROLLBACK
            .iter()
            .map(|lines| Labelled { value: *lines, label: format!("{} {}", grouped(*lines), c.t(tr::Key::Lines)) })
            .collect();
        let scrollback = lines.iter().min_by_key(|option| option.value.abs_diff(prefs.scrollback as u32)).cloned();
        let changed = self.shell.trim() != prefs.shell.as_deref().unwrap_or_default();
        let apply = changed.then_some(Msg::ShellSubmit);
        let field = row![
            widgets::field(t, c.t(tr::Key::ShellPlaceholder), &self.shell, Msg::ShellInput, apply.clone(), false)
                .width(Fill),
            widgets::secondary(t, c.t(crate::i18n::Key::Apply), apply),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        Group::new(t)
            .title(c.t(tr::Key::Behaviour))
            .push(
                Row::new(t, c.t(tr::Key::Shell))
                    .subtitle(c.t(tr::Key::ShellHint))
                    .suffix(widgets::flexible(field, 340.0))
                    .build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::OnExit)).suffix(widgets::dropdown(t, exits, exit, Msg::OnExit, 260.0)).build(),
            )
            .push(
                Row::new(t, c.t(tr::Key::Scrollback))
                    .suffix(widgets::dropdown(t, lines, scrollback, Msg::Scrollback, 180.0))
                    .build(),
            )
            .push(switch(c, prefs, Switch::ConfirmClose, tr::Key::ConfirmClose, None))
            .push(switch(c, prefs, Switch::MarkPrompts, tr::Key::MarkPrompts, Some(tr::Key::MarkPromptsHint)))
            .push(switch(c, prefs, Switch::AltMeta, tr::Key::AltMeta, Some(tr::Key::AltMetaHint)))
            .push(switch(c, prefs, Switch::CopyOnSelect, tr::Key::CopyOnSelect, None))
            .build()
    }
}

fn switch<'a>(c: &Ctx, prefs: &Preferences, which: Switch, title: tr::Key, hint: Option<tr::Key>) -> Element<'a, Msg> {
    let mut line = Row::new(c.theme, c.t(title))
        .suffix(widgets::switch(c.theme, which.read(prefs), move |on| Msg::Switch(which, on)));
    if let Some(hint) = hint {
        line = line.subtitle(c.t(hint));
    }
    line.build()
}

fn cursor_group<'a>(c: &Ctx, prefs: &Preferences) -> Element<'a, Msg> {
    let t = c.theme;
    let shapes =
        [(Cursor::Block, tr::Key::Block), (Cursor::Beam, tr::Key::Beam), (Cursor::Underline, tr::Key::Underline)]
            .into_iter()
            .map(|(shape, label)| (shape, c.t(label).to_string()))
            .collect();
    Group::new(t)
        .title(c.t(tr::Key::Cursor))
        .push(Row::new(t, c.t(tr::Key::Shape)).suffix(widgets::segmented(t, shapes, prefs.cursor, Msg::Cursor)).build())
        .push(switch(c, prefs, Switch::Blink, tr::Key::Blink, Some(tr::Key::BlinkHint)))
        .build()
}

fn bell_group<'a>(c: &Ctx, prefs: &Preferences) -> Element<'a, Msg> {
    Group::new(c.theme)
        .title(c.t(tr::Key::Bell))
        .push(switch(c, prefs, Switch::VisualBell, tr::Key::VisualBell, None))
        .push(switch(c, prefs, Switch::AudibleBell, tr::Key::AudibleBell, Some(tr::Key::AudibleBellHint)))
        .build()
}

fn grouped(number: u32) -> String {
    let digits = number.to_string();
    let chunks: Vec<&str> =
        digits.as_bytes().rchunks(3).rev().map(|chunk| std::str::from_utf8(chunk).unwrap_or_default()).collect();
    chunks.join("\u{202f}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_settings_are_written_as_the_terminal_reads_them() {
        assert_eq!(kebab(Profile::RedSands), "red-sands");
        assert_eq!(kebab(Cursor::Underline), "underline");
        assert_eq!(kebab(OnExit::CloseClean), "close-clean");
        let text = format!("profile = \"{}\"\non-exit = \"{}\"\n", kebab(Profile::ManPage), kebab(OnExit::Keep));
        let prefs = Preferences::parse(&text);
        assert_eq!((prefs.profile, prefs.on_exit), (Profile::ManPage, OnExit::Keep));
    }

    #[test]
    fn every_switch_names_a_key_the_terminal_reads() {
        let all = [
            Switch::BoldBright,
            Switch::Blink,
            Switch::VisualBell,
            Switch::AudibleBell,
            Switch::ConfirmClose,
            Switch::MarkPrompts,
            Switch::AltMeta,
            Switch::CopyOnSelect,
        ];
        for which in all {
            let prefs = Preferences::parse(&format!("{} = false\n", which.key()));
            assert!(!which.read(&prefs), "{}", which.key());
            assert!(which.read(&Preferences::default()), "{} defaults to on", which.key());
        }
    }

    #[test]
    fn line_counts_group_their_thousands() {
        assert_eq!(grouped(1_000), "1\u{202f}000");
        assert_eq!(grouped(200_000), "200\u{202f}000");
        assert_eq!(grouped(999), "999");
    }

    #[test]
    fn writing_keeps_the_users_comments() {
        let dir = std::env::temp_dir().join(format!("glide-settings-terminal-{}", std::process::id()));
        let path = dir.join("terminal.toml");
        let text = "# my terminal\nfont-size = 15.0\n";
        let mut store = Store { path: path.clone(), doc: text.parse().ok(), prefs: Preferences::parse(text) };
        store.write("profile", Some(value("ocean"))).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("# my terminal\n"));
        assert_eq!(store.prefs.profile, Profile::Ocean);
        assert!((store.prefs.font_size - 15.0).abs() < f32::EPSILON);
        store.write("profile", None).unwrap();
        assert_eq!(store.prefs.profile, Profile::Meridian);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
