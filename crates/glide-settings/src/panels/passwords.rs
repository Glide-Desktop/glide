use glide_config::passwords::{CLEAR_CHOICES, ClickAction, LOCK_CHOICES, Prefs};
use glide_sdk::widget as sdk;
use iced::widget::column;
use iced::{Element, Task};

use super::common::{Choice, presets, toggle};
use super::{Ctx, Pane, Panel};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::run::{self, blocking};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Hero => "Passwords and autofill", "Паролі й автозаповнення";
        HeroHint => "How Glide offers saved passwords and codes, and how it keeps them safe. Items and vaults live in the Passwords app.", "Як Glide пропонує збережені паролі й коди та як береже їх. Записи й сховища — у програмі «Паролі».";
        OpenApp => "Open Passwords", "Відкрити «Паролі»";
        Autofill => "Autofill", "Автозаповнення";
        Offer => "Offer a password in password fields", "Пропонувати пароль у полях пароля";
        OfferHint => "The island at the top shows the matching login for that app", "Острів угорі покаже логін, що підходить цій програмі";
        Typing => "Type it when a field refuses pasting", "Вводити клавішами, якщо поле не приймає вставку";
        TypingHint => "For older apps that do not support modern text input", "Для старих програм без сучасного введення тексту";
        Click => "Clicking the key in the top bar", "Клацання по ключу у верхній панелі";
        ClickPassword => "Insert the password", "Вставити пароль";
        ClickUsername => "Insert the user name", "Вставити ім’я користувача";
        ClickBoth => "Insert name, Tab, password", "Ім’я, Tab, пароль";
        ClickCode => "Insert the one-time code", "Вставити одноразовий код";
        FillShortcut => "Fill password shortcut", "Комбінація «Заповнити пароль»";
        FillShortcutHint => "Offers the window's logins even where the field does not say it wants a password", "Пропонує логіни вікна навіть там, де поле не повідомляє, що чекає пароль";
        Unbound => "Not set", "Не призначено";
        Clipboard => "Clipboard", "Буфер обміну";
        ClearAfter => "Clear a copied password", "Очищати скопійований пароль";
        ClearAfterHint => "Only while the clipboard still holds it", "Лише якщо в буфері досі він";
        Never => "Never", "Ніколи";
        Seconds => "s", "с";
        Minutes => "min", "хв";
        Private => "Hide passwords from clipboard managers", "Приховувати паролі від менеджерів буфера";
        PrivateHint => "CopyQ, Klipper and others leave them out of their history", "CopyQ, Klipper та інші не запам’ятають їх в історії";
        Locking => "Locking", "Блокування";
        LockAfter => "Lock after inactivity", "Блокувати після бездіяльності";
        LockAfterHint => "No key press, click or touch for this long", "Стільки часу без натискань, клацань і дотиків";
        WithScreen => "Lock with the screen", "Блокувати разом з екраном";
        WithScreenHint => "Set on the Lock Screen page", "Налаштовується на сторінці «Екран блокування»";
        On => "On", "Увімкнено";
        Off => "Off", "Вимкнено";
        Service => "Password service for other apps", "Служба паролів для інших програм";
        ServiceHint => "Chromium browsers and GNOME apps keep their passwords in the local vault through Secret Service", "Браузери Chromium і GNOME-програми зберігають паролі в локальному сховищі через Secret Service";
        NextSignIn => "after signing in again", "після повторного входу";
        Leaks => "Check passwords against known leaks", "Перевіряти паролі на відомі витоки";
        LeaksHint => "Sends only the first five characters of a hash, never the password", "Надсилає лише перші п’ять символів хешу, ніколи не пароль";
    }
}

use tr::Key as T;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub value: ClickAction,
    label: String,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<Prefs>),
    Installed(bool),
    Offer(bool),
    Typing(bool),
    Click(Action),
    ClearAfter(Choice),
    Private(bool),
    LockAfter(Choice),
    Service(bool),
    Leaks(bool),
    OpenApp,
    Opened(Option<Result<String, String>>),
    Open(Panel),
}

#[derive(Debug, Default)]
pub struct Page {
    prefs: Option<Prefs>,
    installed: bool,
}

fn click_label(c: &Ctx, action: ClickAction) -> String {
    c.t(match action {
        ClickAction::Password => T::ClickPassword,
        ClickAction::Username => T::ClickUsername,
        ClickAction::Both => T::ClickBoth,
        ClickAction::Code => T::ClickCode,
    })
    .to_string()
}

fn seconds_label(c: &Ctx, seconds: i64) -> String {
    if seconds <= 0 { c.t(T::Never).to_string() } else { format!("{seconds} {}", c.t(T::Seconds)) }
}

fn minutes_label(c: &Ctx, minutes: i64) -> String {
    if minutes <= 0 { c.t(T::Never).to_string() } else { format!("{minutes} {}", c.t(T::Minutes)) }
}

fn launch() -> Result<String, String> {
    std::process::Command::new("glide-passwords")
        .stdin(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            String::new()
        })
        .map_err(|err| format!("glide-passwords: {err}"))
}

impl Page {
    fn change(&mut self, edit: impl Fn(&mut Prefs)) {
        let prefs = Prefs::update(&edit).unwrap_or_else(|_| {
            let mut prefs = self.prefs.clone().unwrap_or_else(Prefs::load);
            edit(&mut prefs);
            prefs
        });
        self.prefs = Some(prefs);
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "passwords",
        "autofill",
        "keychain",
        "vault",
        "clipboard",
        "secret service",
        "bitwarden",
        "паролі",
        "автозаповнення",
        "сховище",
        "буфер обміну",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        Task::batch([
            Task::perform(blocking(Prefs::load), |prefs| Msg::Loaded(Box::new(prefs.unwrap_or_default()))),
            Task::perform(blocking(|| run::available("glide-passwords")), |found| {
                Msg::Installed(found.unwrap_or(false))
            }),
        ])
    }

    fn update(&mut self, msg: Msg, _c: Ctx, _config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(prefs) => self.prefs = Some(*prefs),
            Msg::Installed(installed) => self.installed = installed,
            Msg::Offer(on) => self.change(|prefs| prefs.offer = on),
            Msg::Typing(on) => self.change(|prefs| prefs.type_fallback = on),
            Msg::Click(action) => self.change(|prefs| prefs.click = action.value),
            Msg::ClearAfter(choice) => {
                self.change(|prefs| prefs.clear_after = u32::try_from(choice.value).unwrap_or(0));
            }
            Msg::Private(on) => self.change(|prefs| prefs.private_clipboard = on),
            Msg::LockAfter(choice) => self.change(|prefs| prefs.lock_after = u32::try_from(choice.value).unwrap_or(0)),
            Msg::Service(on) => self.change(|prefs| prefs.secret_service = on),
            Msg::Leaks(on) => self.change(|prefs| prefs.leak_check = on),
            Msg::OpenApp => return Task::perform(blocking(launch), Msg::Opened),
            Msg::Opened(_) | Msg::Open(_) => {}
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let Some(prefs) = &self.prefs else { return widgets::spinner_label(t, crate::i18n::Key::Loading.text(c.lang)) };
        let defaults = Prefs::default();
        let open = self.installed.then_some(Msg::OpenApp);
        let hero = widgets::section_hero(
            t,
            Icon::Key,
            Panel::Passwords.color(),
            c.t(T::Hero),
            c.t(T::HeroHint),
            Some(widgets::secondary(t, c.t(T::OpenApp), open)),
        );
        let actions: Vec<Action> =
            ClickAction::ALL.iter().map(|value| Action { value: *value, label: click_label(&c, *value) }).collect();
        let selected = actions.iter().find(|action| action.value == prefs.click).cloned();
        let autofill = Group::new(t)
            .title(c.t(T::Autofill))
            .push(
                Row::new(t, c.t(T::Offer))
                    .subtitle(c.t(T::OfferHint))
                    .changed(prefs.offer != defaults.offer)
                    .suffix(toggle(c, prefs.offer, Msg::Offer))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Typing))
                    .subtitle(c.t(T::TypingHint))
                    .changed(prefs.type_fallback != defaults.type_fallback)
                    .suffix(toggle(c, prefs.type_fallback, Msg::Typing))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Click))
                    .changed(prefs.click != defaults.click)
                    .suffix(widgets::dropdown(t, actions, selected, Msg::Click, 220.0))
                    .build(),
            )
            .push(fill_shortcut(&c, config));
        let clear: Vec<i64> = CLEAR_CHOICES.iter().map(|value| i64::from(*value)).collect();
        let (clear_options, clear_selected) =
            presets(&clear, i64::from(prefs.clear_after), |value| seconds_label(&c, value));
        let clipboard = Group::new(t)
            .title(c.t(T::Clipboard))
            .push(
                Row::new(t, c.t(T::ClearAfter))
                    .subtitle(c.t(T::ClearAfterHint))
                    .changed(prefs.clear_after != defaults.clear_after)
                    .suffix(widgets::dropdown(t, clear_options, clear_selected, Msg::ClearAfter, 160.0))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Private))
                    .subtitle(c.t(T::PrivateHint))
                    .changed(prefs.private_clipboard != defaults.private_clipboard)
                    .suffix(toggle(c, prefs.private_clipboard, Msg::Private))
                    .build(),
            );
        column![hero, autofill.build(), clipboard.build(), locking(&c, prefs, &defaults)]
            .spacing(theme::SECTION_GAP)
            .into()
    }
}

fn fill_shortcut<'a>(c: &Ctx, config: &ConfigStore) -> Element<'a, Msg> {
    let t = c.theme;
    let combo: Element<'a, Msg> =
        match crate::services::keybinds::combo_for(&crate::services::keybinds::current(config), "fill-password") {
            Some(combo) => widgets::key_combo(t, &combo),
            None => sdk::text(c.t(T::Unbound), glide_sdk::theme::TextStyle::CALL).color(t.fg_3()).into(),
        };
    Row::new(t, c.t(T::FillShortcut))
        .subtitle(c.t(T::FillShortcutHint))
        .suffix(iced::widget::row![combo, widgets::disclosure(t, None)].spacing(8).align_y(iced::Alignment::Center))
        .on_press(Msg::Open(Panel::Shortcuts))
        .build()
}

fn locking<'a>(c: &Ctx, prefs: &Prefs, defaults: &Prefs) -> Element<'a, Msg> {
    let t = c.theme;
    let lock: Vec<i64> = LOCK_CHOICES.iter().map(|value| i64::from(*value)).collect();
    let (lock_options, lock_selected) = presets(&lock, i64::from(prefs.lock_after), |value| minutes_label(c, value));
    let paint = widgets::paint(&t);
    Group::new(t)
        .title(c.t(T::Locking))
        .push(
            Row::new(t, c.t(T::LockAfter))
                .subtitle(c.t(T::LockAfterHint))
                .changed(prefs.lock_after != defaults.lock_after)
                .suffix(widgets::dropdown(t, lock_options, lock_selected, Msg::LockAfter, 160.0))
                .build(),
        )
        .push(
            Row::new(t, c.t(T::WithScreen))
                .subtitle(c.t(T::WithScreenHint))
                .suffix(widgets::disclosure(
                    t,
                    Some(c.t(if prefs.lock_with_screen { T::On } else { T::Off }).to_string()),
                ))
                .on_press(Msg::Open(Panel::LockScreen))
                .build(),
        )
        .push(
            Row::new(t, c.t(T::Service))
                .subtitle(c.t(T::ServiceHint))
                .changed(prefs.secret_service != defaults.secret_service)
                .suffix(
                    iced::widget::row![
                        sdk::applies_chip(&paint, c.t(T::NextSignIn), true),
                        toggle(*c, prefs.secret_service, Msg::Service),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                )
                .build(),
        )
        .push(
            Row::new(t, c.t(T::Leaks))
                .subtitle(c.t(T::LeaksHint))
                .changed(prefs.leak_check != defaults.leak_check)
                .suffix(toggle(*c, prefs.leak_check, Msg::Leaks))
                .build(),
        )
        .build()
}
