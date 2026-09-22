use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    #[default]
    Uk,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_language(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

#[must_use]
pub fn language() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Uk,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Open,
    NewWindow,
    PinToDock,
    UnpinFromDock,
    CloseAllWindows,
    Applications,
    InputSettings,
    Notifications,
    ClearAll,
    NoNewNotifications,
    Caffeine,
    CaffeineOn,
    CaffeineOff,
    Connected,
    On,
    Off,
    Connecting,
    NightLight,
    LessBlue,
    Screenshot,
    WholeScreen,
    User,
    JustNow,
    MinutesAgo,
    HoursAgo,
    Sent,
    Search,
    CategoryAll,
    CategoryInternet,
    CategoryOffice,
    CategoryMedia,
    CategoryUtilities,
    CategorySystem,
    CategoryOther,
    Minimize,
    Maximize,
    Restore,
    Close,
    PowerOffConfirm,
    Configure,
}

#[must_use]
pub fn t(key: Key) -> &'static str {
    match language() {
        Lang::Uk => uk(key),
        Lang::En => en(key),
    }
}

const fn uk(key: Key) -> &'static str {
    match key {
        Key::Open => "Відкрити",
        Key::NewWindow => "Нове вікно",
        Key::PinToDock => "Закріпити в доку",
        Key::UnpinFromDock => "Відкріпити від дока",
        Key::CloseAllWindows => "Закрити всі вікна",
        Key::Applications => "Застосунки",
        Key::InputSettings => "Налаштування вводу",
        Key::Notifications => "Сповіщення",
        Key::ClearAll => "Очистити все",
        Key::NoNewNotifications => "Немає нових сповіщень",
        Key::Caffeine => "Кофеїн",
        Key::CaffeineOn => "Екран не засинає",
        Key::CaffeineOff => "Не присипляти екран",
        Key::Connected => "Підключено",
        Key::On => "Увімкнено",
        Key::Off => "Вимкнено",
        Key::Connecting => "Підключення…",
        Key::NightLight => "Нічне світло",
        Key::LessBlue => "Менше синього",
        Key::Screenshot => "Знімок",
        Key::WholeScreen => "Увесь екран",
        Key::User => "Користувач",
        Key::JustNow => "щойно",
        Key::MinutesAgo => "{} хв",
        Key::HoursAgo => "{} год",
        Key::Sent => "Надіслано",
        Key::Search => "Пошук",
        Key::CategoryAll => "Усі",
        Key::CategoryInternet => "Інтернет",
        Key::CategoryOffice => "Офіс",
        Key::CategoryMedia => "Медіа",
        Key::CategoryUtilities => "Утиліти",
        Key::CategorySystem => "Система",
        Key::CategoryOther => "Інше",
        Key::Minimize => "Згорнути",
        Key::Maximize => "Розгорнути",
        Key::Restore => "Відновити",
        Key::Close => "Закрити",
        Key::PowerOffConfirm => "Вимкнути?",
        Key::Configure => "Налаштувати",
    }
}

const fn en(key: Key) -> &'static str {
    match key {
        Key::Open => "Open",
        Key::NewWindow => "New window",
        Key::PinToDock => "Pin to dock",
        Key::UnpinFromDock => "Unpin from dock",
        Key::CloseAllWindows => "Close all windows",
        Key::Applications => "Applications",
        Key::InputSettings => "Input settings",
        Key::Notifications => "Notifications",
        Key::ClearAll => "Clear all",
        Key::NoNewNotifications => "No new notifications",
        Key::Caffeine => "Caffeine",
        Key::CaffeineOn => "Screen stays awake",
        Key::CaffeineOff => "Keep the screen awake",
        Key::Connected => "Connected",
        Key::On => "On",
        Key::Off => "Off",
        Key::Connecting => "Connecting…",
        Key::NightLight => "Night light",
        Key::LessBlue => "Less blue light",
        Key::Screenshot => "Screenshot",
        Key::WholeScreen => "Whole screen",
        Key::User => "User",
        Key::JustNow => "just now",
        Key::MinutesAgo => "{} min",
        Key::HoursAgo => "{} h",
        Key::Sent => "Sent",
        Key::Search => "Search",
        Key::CategoryAll => "All",
        Key::CategoryInternet => "Internet",
        Key::CategoryOffice => "Office",
        Key::CategoryMedia => "Media",
        Key::CategoryUtilities => "Utilities",
        Key::CategorySystem => "System",
        Key::CategoryOther => "Other",
        Key::Minimize => "Minimize",
        Key::Maximize => "Maximize",
        Key::Restore => "Restore",
        Key::Close => "Close",
        Key::PowerOffConfirm => "Power off?",
        Key::Configure => "Configure",
    }
}

#[must_use]
pub fn elapsed_label(minutes: u64) -> String {
    if minutes < 1 {
        t(Key::JustNow).to_string()
    } else if minutes < 60 {
        t(Key::MinutesAgo).replace("{}", &minutes.to_string())
    } else {
        t(Key::HoursAgo).replace("{}", &(minutes / 60).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[Key] = &[
        Key::Open,
        Key::NewWindow,
        Key::PinToDock,
        Key::UnpinFromDock,
        Key::CloseAllWindows,
        Key::Applications,
        Key::InputSettings,
        Key::Notifications,
        Key::ClearAll,
        Key::NoNewNotifications,
        Key::Caffeine,
        Key::CaffeineOn,
        Key::CaffeineOff,
        Key::Connected,
        Key::On,
        Key::Off,
        Key::Connecting,
        Key::NightLight,
        Key::LessBlue,
        Key::Screenshot,
        Key::WholeScreen,
        Key::User,
        Key::JustNow,
        Key::MinutesAgo,
        Key::HoursAgo,
        Key::Sent,
        Key::Search,
        Key::CategoryAll,
        Key::CategoryInternet,
        Key::CategoryOffice,
        Key::CategoryMedia,
        Key::CategoryUtilities,
        Key::CategorySystem,
        Key::CategoryOther,
        Key::Minimize,
        Key::Maximize,
        Key::Restore,
        Key::Close,
        Key::PowerOffConfirm,
        Key::Configure,
    ];

    #[test]
    fn every_key_has_a_non_empty_translation_in_both_languages() {
        for key in ALL {
            assert!(!uk(*key).is_empty(), "{key:?} uk");
            assert!(!en(*key).is_empty(), "{key:?} en");
        }
    }

    #[test]
    fn only_micro_labels_are_uppercase_in_the_catalogue() {
        for key in ALL {
            for text in [uk(*key), en(*key)] {
                let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
                assert!(
                    letters.len() < 2 || letters.iter().any(|c| c.is_lowercase()),
                    "{key:?}: {text} is shouting; uppercase belongs to the MICRO style"
                );
            }
        }
    }

    #[test]
    fn elapsed_labels_pick_the_unit_from_the_magnitude() {
        assert_eq!(elapsed_label(0), uk(Key::JustNow));
        assert_eq!(elapsed_label(5), "5 хв");
        assert_eq!(elapsed_label(125), "2 год");
    }
}
