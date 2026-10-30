use glide_config::passwords::Prefs;
use glide_sdk::widget::{self as sdk, mock};
use iced::widget::{Column, column, row};
use iced::{Alignment, Element, Fill, Task};

use super::common::{Choice, gated, presets, toggle};
use super::{Ctx, Pane, Panel};
use crate::icons::Icon;
use crate::services::config::{ConfigStore, DEFAULT_BLANK_AFTER};
use crate::services::keybinds;
use crate::services::run::blocking;
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        Preview => "On the lock screen", "На екрані блокування";
        PreviewHint => "The clock, your picture and the password field. Notification text is never shown.", "Годинник, ваше фото й поле пароля. Текст сповіщень не показується ніколи.";
        PreviewDate => "Monday, 26 October", "понеділок, 26 жовтня";
        PreviewMedia => "Now playing", "Зараз грає";
        PreviewNotes => "3 notifications · hidden", "3 сповіщення · приховано";
        When => "When to lock", "Коли блокувати";
        WhenHint => "Counted from the last key press, click or touch", "Відлік від останнього натискання клавіші, клацання чи дотику";
        Blank => "Turn off the screen", "Гасити екран";
        BlankHint => "After this much inactivity", "Після такого часу бездіяльності";
        Lock => "Lock", "Блокувати";
        LockHint => "This long after the screen turns off", "Через такий час після того, як екран згас";
        Never => "Never", "Ніколи";
        AtOnce => "Immediately", "Одразу";
        Minutes => "min", "хв";
        Seconds => "s", "с";
        BeforeSleep => "Before sleep", "Перед сном";
        BeforeSleepHint => "The lock is drawn before the computer sleeps", "Замок з’являється до того, як комп’ютер засне";
        Always => "Always", "Завжди";
        Vaults => "Lock password vaults with the screen", "Блокувати сховища паролів разом з екраном";
        VaultsHint => "Saved passwords need the vault password again after unlocking", "Після розблокування збережені паролі знову попросять пароль сховища";
        Shortcut => "Shortcut", "Сполучення клавіш";
        ShortcutHint => "Also from quick settings and the power menu", "Також зі швидких налаштувань і меню живлення";
        SleepLink => "Sleep and what keeps the screen on", "Сон і що не дає гасити екран";
        SleepLinkHint => "Set in Power & Battery", "Налаштовується в розділі «Живлення й батарея»";
        Protection => "Protection from guessing", "Захист від підбору";
        ProtectionHint => "System values, shown for reference", "Системні значення, лише для перегляду";
        Attempts => "After {n} wrong passwords the account is locked for {t}", "Після {n} хибних паролів обліковий запис блокується на {t}";
        AttemptsHint => "Set in /etc/security/faillock.conf", "Задається в /etc/security/faillock.conf";
        Pam => "Password check is set up", "Перевірку пароля налаштовано";
        PamHint => "PAM service {s}", "Служба PAM {s}";
        PamMissing => "Glide's own password check is not installed", "Власну перевірку пароля Glide не встановлено";
        PamMissingHint => "The lock screen uses the login service instead", "Екран блокування користується службою входу";
        System => "system", "системне";
        Ready => "Ready", "Готово";
    }
}

use tr::Key as T;

const BLANK_CHOICES: [i64; 7] = [60, 120, 300, 600, 900, 1800, 0];
const LOCK_CHOICES: [i64; 6] = [0, 30, 60, 300, 900, -1];
const PAM_DIRS: [&str; 2] = ["/etc/pam.d", "/usr/lib/pam.d"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Faillock {
    pub deny: u32,
    pub unlock_seconds: u32,
}

impl Default for Faillock {
    fn default() -> Self {
        Self { deny: 3, unlock_seconds: 600 }
    }
}

#[must_use]
pub fn parse_faillock(text: &str) -> Faillock {
    let mut found = Faillock::default();
    for line in text.lines().map(str::trim).filter(|line| !line.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let Ok(value) = value.trim().parse::<u32>() else { continue };
        match key.trim() {
            "deny" => found.deny = value,
            "unlock_time" => found.unlock_seconds = value,
            _ => {}
        }
    }
    found
}

#[derive(Debug, Clone, Default)]
pub struct System {
    faillock: Faillock,
    pam: Option<String>,
    vaults: Option<bool>,
}

fn read_system() -> System {
    let faillock =
        std::fs::read_to_string("/etc/security/faillock.conf").map(|text| parse_faillock(&text)).unwrap_or_default();
    let pam = PAM_DIRS
        .iter()
        .map(|dir| std::path::Path::new(dir).join("glide-lock"))
        .find(|path| path.exists())
        .map(|_| "glide-lock".to_string());
    System { faillock, pam, vaults: Some(Prefs::load().lock_with_screen) }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Box<System>),
    Blank(Choice),
    LockAfter(Choice),
    Vaults(bool),
    VaultsSaved,
    Open(Panel),
}

#[derive(Debug, Default)]
pub struct Page {
    system: Option<System>,
}

#[must_use]
pub fn timeout_label(c: &Ctx, seconds: i64) -> String {
    match seconds {
        0 => c.t(T::AtOnce).to_string(),
        seconds if seconds < 0 => c.t(T::Never).to_string(),
        seconds if seconds % 60 == 0 => format!("{} {}", seconds / 60, c.t(T::Minutes)),
        seconds => format!("{seconds} {}", c.t(T::Seconds)),
    }
}

#[must_use]
pub fn blank_label(c: &Ctx, seconds: i64) -> String {
    if seconds == 0 { c.t(T::Never).to_string() } else { timeout_label(c, seconds) }
}

fn faillock_line(c: &Ctx, faillock: Faillock) -> String {
    let minutes = faillock.unlock_seconds.div_ceil(60);
    c.t(T::Attempts)
        .replace("{n}", &faillock.deny.to_string())
        .replace("{t}", &format!("{minutes} {}", c.t(T::Minutes)))
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "lock",
        "lock screen",
        "screen lock",
        "blank",
        "screen off",
        "idle",
        "password",
        "блокування",
        "екран блокування",
        "замок",
        "гасити",
        "бездіяльність",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        Task::perform(blocking(read_system), |system| Msg::Loaded(Box::new(system.unwrap_or_default())))
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Loaded(system) => self.system = Some(*system),
            Msg::Blank(choice) => config.set_int("general", "blank-after-seconds", choice.value),
            Msg::LockAfter(choice) => config.set_int("general", "lock-after-seconds", choice.value),
            Msg::Vaults(on) => {
                if let Some(system) = &mut self.system {
                    system.vaults = Some(on);
                }
                return Task::perform(
                    blocking(move || {
                        let _ = Prefs::update(|prefs| prefs.lock_with_screen = on);
                    }),
                    |_| Msg::VaultsSaved,
                );
            }
            Msg::VaultsSaved | Msg::Open(_) => {}
        }
        Task::none()
    }

    fn sidebar_status(&self, _c: Ctx) -> Option<String> {
        None
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let paint = widgets::paint(&t);
        let preview = mock::lock_screen(
            paint,
            300.0,
            188.0,
            String::from("14:05"),
            c.t(T::PreviewDate).to_string(),
            vec![c.t(T::PreviewMedia).to_string(), c.t(T::PreviewNotes).to_string()],
        );
        let shortcut =
            keybinds::combo_for(&keybinds::current(config), "lock").map(|combo| widgets::key_combo(t, &combo));
        let mut aside = Column::new()
            .spacing(8)
            .width(Fill)
            .push(widgets::micro_label(t, c.t(T::Preview)))
            .push(sdk::text(c.t(T::PreviewHint), glide_sdk::theme::TextStyle::CALL).color(t.fg_2()));
        if let Some(shortcut) = shortcut {
            aside = aside.push(
                row![sdk::text(c.t(T::Shortcut), glide_sdk::theme::TextStyle::CALL), shortcut]
                    .spacing(8)
                    .align_y(Alignment::Center),
            );
        }
        let hero = sdk::card(&paint, row![preview, aside].spacing(20).align_y(Alignment::Center));

        let current = config.int_or("general", "blank-after-seconds", DEFAULT_BLANK_AFTER);
        let (blank_options, blank_selected) = presets(&BLANK_CHOICES, current, |value| blank_label(&c, value));
        let lock_after = config.int_or("general", "lock-after-seconds", 0).max(-1);
        let (lock_options, lock_selected) = presets(&LOCK_CHOICES, lock_after, |value| timeout_label(&c, value));
        let mut when = Group::new(t)
            .title(c.t(T::When))
            .description(c.t(T::WhenHint))
            .push(
                Row::new(t, c.t(T::Blank))
                    .subtitle(c.t(T::BlankHint))
                    .changed(current != DEFAULT_BLANK_AFTER)
                    .suffix(gated(c, widgets::dropdown(t, blank_options, blank_selected, Msg::Blank, 180.0)))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::Lock))
                    .subtitle(c.t(T::LockHint))
                    .changed(lock_after != 0)
                    .suffix(gated(c, widgets::dropdown(t, lock_options, lock_selected, Msg::LockAfter, 180.0)))
                    .build(),
            )
            .push(
                Row::new(t, c.t(T::BeforeSleep))
                    .subtitle(c.t(T::BeforeSleepHint))
                    .suffix(sdk::text(c.t(T::Always), glide_sdk::theme::TextStyle::CALL).color(t.fg_2()))
                    .build(),
            );
        if let Some(vaults) = self.system.as_ref().and_then(|system| system.vaults) {
            when = when.push(
                Row::new(t, c.t(T::Vaults)).subtitle(c.t(T::VaultsHint)).suffix(toggle(c, vaults, Msg::Vaults)).build(),
            );
        }
        when = when.push(
            Row::new(t, c.t(T::SleepLink))
                .subtitle(c.t(T::SleepLinkHint))
                .suffix(widgets::disclosure(t, None))
                .on_press(Msg::Open(Panel::Power))
                .build(),
        );
        let mut page = column![hero, when.build()].spacing(theme::SECTION_GAP);
        if let Some(system) = &self.system {
            let pam = match &system.pam {
                Some(service) => Row::new(t, c.t(T::Pam))
                    .subtitle(c.t(T::PamHint).replace("{s}", service))
                    .prefix(widgets::icon(Icon::Check, 18.0, t.ok()))
                    .suffix(widgets::status_pill(t, crate::theme::Tone::Ok, c.t(T::Ready)))
                    .build(),
                None => Row::new(t, c.t(T::PamMissing))
                    .subtitle(c.t(T::PamMissingHint))
                    .prefix(widgets::icon(Icon::Warning, 18.0, t.warn()))
                    .build(),
            };
            page = page.push(
                Group::new(t)
                    .title(c.t(T::Protection))
                    .description(c.t(T::ProtectionHint))
                    .push(
                        Row::new(t, faillock_line(&c, system.faillock))
                            .subtitle(c.t(T::AttemptsHint))
                            .prefix(widgets::icon(Icon::Lock, 18.0, t.fg_2()))
                            .suffix(widgets::pill(t, crate::theme::Tone::Neutral, c.t(T::System)))
                            .build(),
                    )
                    .push(pam)
                    .build(),
            );
        }
        page.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faillock_reads_the_uncommented_values_and_keeps_the_pam_defaults_otherwise() {
        assert_eq!(parse_faillock(""), Faillock { deny: 3, unlock_seconds: 600 });
        let text = "# deny = 9\ndeny = 5\n unlock_time = 900\nsilent\n";
        assert_eq!(parse_faillock(text), Faillock { deny: 5, unlock_seconds: 900 });
    }
}
