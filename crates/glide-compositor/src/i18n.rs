use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    #[must_use]
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        Self::from_locale_vars(var("LC_ALL").as_deref(), var("LC_MESSAGES").as_deref(), var("LANG").as_deref())
    }

    #[must_use]
    pub fn from_locale_vars(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Self {
        let locale = [lc_all, lc_messages, lang]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        let base = locale.split(['_', '.', '@']).next().unwrap_or_default();
        if base.eq_ignore_ascii_case("uk") { Self::Uk } else { Self::En }
    }
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

macro_rules! catalogue {
    ($($key:ident => $en:literal, $uk:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Key {
            $($key,)*
        }

        impl Key {
            pub const ALL: &'static [Self] = &[$(Self::$key,)*];
        }

        #[allow(clippy::too_many_lines)]
        const fn uk(key: Key) -> &'static str {
            match key {
                $(Key::$key => $uk,)*
            }
        }

        #[allow(clippy::too_many_lines)]
        const fn en(key: Key) -> &'static str {
            match key {
                $(Key::$key => $en,)*
            }
        }
    };
}

catalogue! {
    Open => "Open", "Відкрити";
    NewWindow => "New Window", "Нове вікно";
    PinToDock => "Pin to Dock", "Закріпити в доку";
    UnpinFromDock => "Unpin from Dock", "Відкріпити від дока";
    CloseAllWindows => "Close All Windows", "Закрити всі вікна";
    Applications => "Applications", "Застосунки";
    InputSettings => "Input Settings", "Налаштування вводу";
    Notifications => "Notifications", "Сповіщення";
    ClearAll => "Clear All", "Очистити все";
    NoNewNotifications => "No new notifications", "Немає нових сповіщень";
    Caffeine => "Caffeine", "Кофеїн";
    CaffeineOn => "Screen stays awake", "Екран не засинає";
    CaffeineOff => "Keep the screen awake", "Не присипляти екран";
    Connected => "Connected", "Підключено";
    On => "On", "Увімкнено";
    Off => "Off", "Вимкнено";
    Connecting => "Connecting…", "Підключення…";
    NotConnected => "Not connected", "Не підключено";
    NightLight => "Night Light", "Нічне світло";
    LessBlue => "Less blue light", "Менше синього";
    Screenshot => "Screenshot", "Знімок";
    WholeScreen => "Whole screen", "Увесь екран";
    User => "User", "Користувач";
    JustNow => "just now", "щойно";
    MinutesAgo => "{} min", "{} хв";
    HoursAgo => "{} h", "{} год";
    Yesterday => "yesterday", "вчора";
    Sent => "Sent", "Надіслано";
    Search => "Search", "Пошук";
    SearchApps => "Search apps…", "Пошук застосунків…";
    CategoryAll => "All", "Усі";
    CategoryInternet => "Internet", "Інтернет";
    CategoryOffice => "Office", "Офіс";
    CategoryMedia => "Media", "Медіа";
    CategoryUtilities => "Utilities", "Утиліти";
    CategorySystem => "System", "Система";
    CategoryOther => "Other", "Інше";
    Minimize => "Minimize", "Згорнути";
    Maximize => "Maximize", "Розгорнути";
    Restore => "Restore", "Відновити";
    Close => "Close", "Закрити";
    ForceQuit => "Force Quit", "Завершити примусово";
    Configure => "Configure", "Налаштувати";
    MenuFile => "File", "Файл";
    MenuView => "View", "Вигляд";
    MenuWorkspaces => "Workspaces", "Робочі столи";
    SystemSettings => "System Settings…", "Системні налаштування…";
    LockScreen => "Lock Screen", "Заблокувати екран";
    NewTerminal => "New Terminal", "Новий термінал";
    OpenHome => "Open Home Folder", "Відкрити домашню теку";
    Overview => "Overview", "Огляд";
    DoNotDisturb => "Do Not Disturb", "Не турбувати";
    PreviousWorkspace => "Previous Workspace", "Попередній робочий стіл";
    NextWorkspace => "Next Workspace", "Наступний робочий стіл";
    Reply => "Reply", "Відповісти";
    NoResultsFor => "No results for \u{201c}{}\u{201d}", "Немає результатів для \u{201c}{}\u{201d}";
    WiFi => "Wi-Fi", "Wi-Fi";
    Bluetooth => "Bluetooth", "Bluetooth";
    SectionCalculator => "Calculator", "Калькулятор";
    SectionActions => "Actions", "Дії";
    SectionCommands => "Commands", "Команди";
    SectionSettings => "Settings", "Налаштування";
    CommandLogOut => "Log Out", "Вийти з сеансу";
    CommandRestart => "Restart", "Перезавантажити";
    CommandShutDown => "Shut Down", "Вимкнути";
    CommandSleep => "Sleep", "Режим сну";
    CalculatorCopyHint => "Enter to copy", "Enter \u{2014} скопіювати";
    SettingsNetwork => "Network & Wi-Fi", "Мережа та Wi-Fi";
    SettingsAppearance => "Appearance", "Зовнішній вигляд";
    SettingsDesktop => "Desktop & Dock", "Стільниця і док";
    SettingsWindows => "Windows", "Вікна";
    SettingsDisplays => "Displays", "Дисплеї";
    SettingsSound => "Sound", "Звук";
    SettingsPower => "Power", "Живлення";
    SettingsKeyboard => "Keyboard", "Клавіатура";
    SettingsMouse => "Mouse & Touchpad", "Миша і тачпад";
    SettingsApps => "Apps", "Програми за замовчуванням";
    SettingsAbout => "About", "Про систему";
    SettingsVpn => "VPN connections", "З\u{2019}єднання VPN";
    SettingsNotifications => "Notifications", "Сповіщення";
    SettingsWallpaper => "Wallpaper", "Шпалери";
    SettingsTopBar => "Top Bar", "Верхня панель";
    SettingsMultitasking => "Multitasking", "Багатозадачність";
    SettingsNightLight => "Night Light", "Нічне світло";
    SettingsPrinters => "Printers", "Принтери";
    SettingsShortcuts => "Keyboard Shortcuts", "Сполучення клавіш";
    SettingsGestures => "Gestures", "Жести";
    SettingsAccessibility => "Accessibility", "Доступність";
    SettingsPrivacy => "Privacy & Security", "Конфіденційність і безпека";
    SettingsUsers => "Users", "Користувачі";
    SettingsDateTime => "Date & Time", "Дата й час";
    SettingsRegion => "Region & Language", "Регіон і мова";
    SettingsStartup => "Startup Apps", "Автозапуск";
    SettingsStorage => "Storage", "Сховище";
    DockShowAllWindows => "Show All Windows", "Показати всі вікна";
    DockHide => "Hide", "Сховати";
    DockQuit => "Quit", "Завершити";
    PowerLock => "Lock", "Заблокувати";
    PowerCancel => "Cancel", "Скасувати";
    PowerCountdownLogOut => "You will be logged out automatically in {n} s.", "Сеанс буде завершено автоматично через {n} с.";
    PowerCountdownRestart => "The computer will restart automatically in {n} s.", "Комп\u{2019}ютер буде перезавантажено автоматично через {n} с.";
    PowerCountdownShutDown => "The computer will shut down automatically in {n} s.", "Комп\u{2019}ютер буде вимкнено автоматично через {n} с.";
    PowerAskLock => "Are you sure you want to lock the screen?", "Ви точно хочете заблокувати екран?";
    PowerAskSleep => "Are you sure you want to put the computer to sleep?", "Ви точно хочете перевести комп\u{2019}ютер у режим сну?";
    PowerAskRestart => "Are you sure you want to restart the computer?", "Ви точно хочете перезапустити комп\u{2019}ютер?";
    PowerAskShutDown => "Are you sure you want to shut down the computer?", "Ви точно хочете вимкнути комп\u{2019}ютер?";
    PowerAskLogOut => "Are you sure you want to log out?", "Ви точно хочете вийти із системи?";
    PowerConfirmSleep => "Sleep", "Приспати";
    PowerConfirmRestart => "Restart", "Перезапустити";
    PowerConfirmLogOut => "Log Out", "Вийти";
    QsNetworks => "Networks", "Мережі";
    QsDevices => "Devices", "Пристрої";
    QsWifiSettings => "Wi-Fi Settings…", "Налаштування Wi-Fi…";
    QsBluetoothSettings => "Bluetooth Settings…", "Налаштування Bluetooth…";
    QsWifiIsOff => "Wi-Fi is off", "Wi-Fi вимкнено";
    QsBluetoothIsOff => "Bluetooth is off", "Bluetooth вимкнено";
    QsSearching => "Searching…", "Пошук…";
    QsNoNetworks => "No networks found", "Мереж не знайдено";
    QsNoDevices => "No paired devices", "Немає спарених пристроїв";
    QsPassword => "Password", "Пароль";
    QsWrongPassword => "Wrong password", "Неправильний пароль";
    QsCouldNotConnect => "Couldn\u{2019}t connect", "Не вдалося підключитися";
    QsDisconnecting => "Disconnecting…", "Відключення…";
    TbBattery => "Battery", "Акумулятор";
    TbBatteryCharging => "Charging", "Заряджається";
    TbBatteryOnBattery => "On battery", "Від акумулятора";
    TbBatteryFull => "Fully charged", "Повністю заряджено";
    TbBatteryNotCharging => "Not charging", "Не заряджається";
    TbBatteryTimeLeft => "{} remaining", "Залишилось {}";
    TbBatteryTimeToFull => "{} until full", "До повного заряду {}";
    TbDurationHoursMinutes => "{h} h {m} min", "{h} год {m} хв";
    TbDurationMinutes => "{m} min", "{m} хв";
    TbPowerMode => "Power Mode", "Режим живлення";
    TbPowerSaver => "Power Saver", "Енергозбереження";
    TbPowerBalanced => "Balanced", "Збалансований";
    TbPowerPerformance => "Performance", "Продуктивність";
    TbBatterySettings => "Battery Settings…", "Налаштування акумулятора…";
    TbPrivacy => "Privacy", "Конфіденційність";
    TbMicrophone => "Microphone", "Мікрофон";
    TbCamera => "Camera", "Камера";
    TbScreenRecording => "Screen Recording", "Запис екрана";
    TbUnknownApp => "Unknown app", "Невідомий застосунок";
    GreeterPassword => "Enter Password", "Введіть пароль";
    GreeterHint => "Press Enter to log in", "Натисніть Enter, щоб увійти";
    GreeterChecking => "Checking…", "Перевірка…";
    GreeterLoggingIn => "Logging in…", "Вхід…";
    GreeterWrongPassword => "Incorrect password. Try again", "Невірний пароль. Спробуйте ще раз";
    GreeterUnavailable => "greetd is not reachable", "Немає з\u{2019}єднання з greetd";
    GreeterSleep => "Sleep", "Сон";
    GreeterRestart => "Restart", "Перезапуск";
    GreeterAskSleep => "Put the computer to sleep?", "Перевести комп\u{2019}ютер у сон?";
    GreeterAskRestart => "Restart the computer?", "Перезавантажити комп\u{2019}ютер?";
    GreeterAskShutDown => "Shut down the computer?", "Вимкнути комп\u{2019}ютер?";
    GreeterConfirmSleep => "Sleep", "Сон";
    LockHint => "Press Enter to unlock", "Натисніть Enter, щоб розблокувати";
    CapsLockOn => "Caps Lock is on", "Увімкнено Caps Lock";
    DiskLowSummary => "The disk is almost full", "Диск майже заповнений";
    DiskLowBody => "Only {code} left in your home folder. Apps may fail to save files.", "У домашній теці лишилося {code}. Програми можуть не змогти зберегти файли.";
    DiskOpenStorage => "Free up space", "Звільнити місце";
    NetPasswordSummary => "Password for {device}", "Пароль для {device}";
    NetWifiBody => "The Wi-Fi network {device} needs its password.", "Мережа Wi-Fi {device} потребує пароля.";
    NetEnterpriseBody => "Network {device} needs your account password.", "Мережа {device} потребує пароля вашого облікового запису.";
    NetVpnBody => "The VPN {device} needs a password.", "VPN {device} потребує пароля.";
    NetOtherBody => "The connection {device} needs a password.", "З\u{2019}єднання {device} потребує пароля.";
    NetRetryBody => "The password for {device} was not accepted. Try again.", "Пароль для {device} не підійшов. Спробуйте ще раз.";
    NetEnterPassword => "Enter password", "Ввести пароль";
    NetPasswordPlaceholder => "Password", "Пароль";
    BtConfirmSummary => "Pair with {device}?", "Створити пару з {device}?";
    BtConfirmBody => "Check that {device} shows {code}.", "Перевірте, що {device} показує {code}.";
    BtPair => "Pair", "Створити пару";
    BtAuthorizeSummary => "Allow {device} to connect?", "Дозволити {device} під\u{2019}єднатися?";
    BtAuthorizeBody => "The device asks to connect to this computer.", "Пристрій хоче під\u{2019}єднатися до цього комп\u{2019}ютера.";
    BtServiceBody => "The device asks for the service {code}.", "Пристрій запитує службу {code}.";
    BtAllow => "Allow", "Дозволити";
    BtCodeSummary => "Pairing with {device}", "Створення пари з {device}";
    BtPinBody => "Enter the PIN code of {device}.", "Введіть PIN-код пристрою {device}.";
    BtPasskeyBody => "Enter the 6-digit code shown on {device}.", "Введіть шестизначний код, який показує {device}.";
    BtEnterCode => "Enter code", "Ввести код";
    BtTypeSummary => "Pairing with {device}", "Створення пари з {device}";
    BtTypeBody => "Type {code} on {device}, then press Enter.", "Наберіть {code} на {device} і натисніть Enter.";
    ConfigBroken => "glide.toml has an error; the last working settings are in use", "У glide.toml помилка; діють останні робочі налаштування";
    ConfigIgnoredKeys => "glide.toml: settings this version does not know were ignored", "glide.toml: проігноровано налаштування, яких ця версія не знає";
    BatteryLow => "Battery is low", "Низький заряд батареї";
    BatteryCritical => "Battery is almost empty", "Батарея майже розряджена";
    BatteryPlugIn => "plug in the charger", "під\u{2019}єднайте зарядний пристрій";
    SessionEndRefused => "Some apps are still open", "Деякі програми ще відкриті";
    SessionEndRefusedHint => "They may have unsaved changes. Choose the same action again within a minute to end the session anyway.", "У них можуть бути незбережені зміни. Оберіть ту саму дію ще раз протягом хвилини, щоб завершити сеанс попри це.";
    SessionEndRefusedLocked => "Apps with unsaved changes kept the session open. Unlock to see them.", "Програми з незбереженими змінами не дали завершити сеанс. Розблокуйте, щоб побачити їх.";
    LockUnlocking => "Unlocking…", "Розблокування…";
    LockUnavailable => "Authentication is unavailable", "Автентифікація недоступна";
}

#[must_use]
pub fn t(key: Key) -> &'static str {
    match language() {
        Lang::Uk => uk(key),
        Lang::En => en(key),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDay {
    pub minute_of_day: u64,
    pub month0: usize,
    pub day: u32,
}

impl LocalDay {
    #[must_use]
    pub fn at_unix(secs: i64) -> Option<Self> {
        let time = libc::time_t::try_from(secs).ok()?;
        // SAFETY: `tm` is a plain C struct of integers and a pointer for which
        // all-zero bytes is a valid value; `localtime_r` overwrites it below.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers address locals that outlive the call, and
        // `localtime_r` writes only the `tm` we hand it.
        let filled = unsafe { libc::localtime_r(&raw const time, &raw mut tm) };
        if filled.is_null() {
            return None;
        }
        Some(Self {
            minute_of_day: u64::try_from(tm.tm_hour * 60 + tm.tm_min).ok()?,
            month0: usize::try_from(tm.tm_mon).ok()?.min(11),
            day: u32::try_from(tm.tm_mday).ok()?,
        })
    }
}

const MONTHS_UK: [&str; 12] =
    ["січ", "лют", "бер", "квіт", "трав", "черв", "лип", "серп", "вер", "жовт", "лист", "груд"];
const MONTHS_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[must_use]
pub fn elapsed_label(elapsed_secs: u64) -> String {
    let now_secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let now = i64::try_from(now_secs).unwrap_or(i64::MAX);
    let then = now.saturating_sub(i64::try_from(elapsed_secs).unwrap_or(i64::MAX));
    let now_day = LocalDay::at_unix(now);
    let then_day = LocalDay::at_unix(then);
    relative_label(language(), elapsed_secs / 60, now_day, then_day)
}

#[must_use]
pub fn relative_label(lang: Lang, minutes: u64, now: Option<LocalDay>, then: Option<LocalDay>) -> String {
    let text = |key| match lang {
        Lang::Uk => uk(key),
        Lang::En => en(key),
    };
    if minutes < 1 {
        return text(Key::JustNow).to_string();
    }
    if minutes < 60 {
        return text(Key::MinutesAgo).replace("{}", &minutes.to_string());
    }
    let (Some(now), Some(then)) = (now, then) else {
        return text(Key::HoursAgo).replace("{}", &(minutes / 60).to_string());
    };
    if minutes <= now.minute_of_day {
        text(Key::HoursAgo).replace("{}", &(minutes / 60).to_string())
    } else if minutes <= now.minute_of_day + 24 * 60 {
        text(Key::Yesterday).to_string()
    } else {
        match lang {
            Lang::Uk => format!("{} {}", then.day, MONTHS_UK[then.month0]),
            Lang::En => format!("{} {}", MONTHS_EN[then.month0], then.day),
        }
    }
}

#[must_use]
pub fn screenshot_saved() -> &'static str {
    match language() {
        Lang::Uk => "Знімок екрана збережено",
        Lang::En => "Screenshot saved",
    }
}

#[must_use]
pub fn screenshot_failed() -> &'static str {
    match language() {
        Lang::Uk => "Не вдалося зберегти знімок екрана",
        Lang::En => "Could not save the screenshot",
    }
}

const WEEKDAYS_UK: [&str; 7] = ["нд", "пн", "вт", "ср", "чт", "пт", "сб"];
const WEEKDAYS_EN: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES_UK: [&str; 12] = [
    "Січень",
    "Лютий",
    "Березень",
    "Квітень",
    "Травень",
    "Червень",
    "Липень",
    "Серпень",
    "Вересень",
    "Жовтень",
    "Листопад",
    "Грудень",
];
const MONTH_NAMES_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn pick(lang: Lang, uk: &[&'static str], en: &[&'static str], index: u32) -> &'static str {
    let table = match lang {
        Lang::Uk => uk,
        Lang::En => en,
    };
    table[usize::try_from(index).unwrap_or(0) % table.len()]
}

#[must_use]
pub fn weekday_short(lang: Lang, weekday_from_sunday: u32) -> &'static str {
    pick(lang, &WEEKDAYS_UK, &WEEKDAYS_EN, weekday_from_sunday)
}

#[must_use]
pub fn month_short(lang: Lang, month0: u32) -> &'static str {
    pick(lang, &MONTHS_UK, &MONTHS_EN, month0)
}

#[must_use]
pub fn month_name(lang: Lang, month0: u32) -> &'static str {
    pick(lang, &MONTH_NAMES_UK, &MONTH_NAMES_EN, month0)
}

const WEEKDAYS_LONG_UK: [&str; 7] =
    ["неділя", "понеділок", "вівторок", "середа", "четвер", "п\u{2019}ятниця", "субота"];
const WEEKDAYS_LONG_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS_GENITIVE_UK: [&str; 12] = [
    "січня",
    "лютого",
    "березня",
    "квітня",
    "травня",
    "червня",
    "липня",
    "серпня",
    "вересня",
    "жовтня",
    "листопада",
    "грудня",
];

#[must_use]
pub fn long_date(lang: Lang, weekday_from_sunday: u32, day: u32, month0: u32) -> String {
    let weekday = pick(lang, &WEEKDAYS_LONG_UK, &WEEKDAYS_LONG_EN, weekday_from_sunday);
    match lang {
        Lang::Uk => {
            let month = pick(lang, &MONTHS_GENITIVE_UK, &MONTH_NAMES_EN, month0);
            format!("{weekday}, {day} {month}")
        }
        Lang::En => format!("{weekday}, {} {day}", month_name(lang, month0)),
    }
}

#[must_use]
pub fn duration_label(minutes: u32) -> String {
    let (hours, rest) = (minutes / 60, minutes % 60);
    if hours == 0 {
        return t(Key::TbDurationMinutes).replace("{m}", &rest.to_string());
    }
    t(Key::TbDurationHoursMinutes).replace("{h}", &hours.to_string()).replace("{m}", &rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_a_non_empty_translation_in_both_languages() {
        for key in Key::ALL {
            assert!(!uk(*key).is_empty(), "{key:?} uk");
            assert!(!en(*key).is_empty(), "{key:?} en");
        }
    }

    #[test]
    fn long_dates_use_full_weekdays_and_the_genitive_month_in_ukrainian() {
        assert_eq!(long_date(Lang::Uk, 5, 25, 8), "п\u{2019}ятниця, 25 вересня");
        assert_eq!(long_date(Lang::En, 5, 25, 8), "Friday, September 25");
        assert_eq!(long_date(Lang::Uk, 0, 1, 0), "неділя, 1 січня");
    }

    #[test]
    fn only_micro_labels_are_uppercase_in_the_catalogue() {
        for key in Key::ALL {
            for text in [uk(*key), en(*key)] {
                let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
                assert!(
                    letters.len() < 2 || letters.iter().any(|c| c.is_lowercase()),
                    "{key:?}: {text} is shouting; uppercase belongs to the MICRO style"
                );
            }
        }
    }

    const fn day(minute_of_day: u64, month0: usize, day: u32) -> LocalDay {
        LocalDay { minute_of_day, month0, day }
    }

    #[test]
    fn elapsed_labels_pick_the_unit_from_the_magnitude() {
        let noon = Some(day(12 * 60, 2, 12));
        let label = |minutes| relative_label(Lang::Uk, minutes, noon, noon);
        assert_eq!(label(0), uk(Key::JustNow));
        assert_eq!(label(5), "5 хв");
        assert_eq!(label(125), "2 год");
    }

    #[test]
    fn something_from_before_midnight_reads_as_yesterday() {
        let now = Some(day(9 * 60, 2, 12));
        let then = Some(day(22 * 60, 2, 11));
        assert_eq!(relative_label(Lang::Uk, 11 * 60, now, then), "вчора");
        assert_eq!(relative_label(Lang::En, 11 * 60, now, then), "yesterday");
        let midnight = Some(day(0, 2, 12));
        assert_eq!(relative_label(Lang::En, 9 * 60, now, midnight), "9 h");
    }

    #[test]
    fn anything_older_than_yesterday_shows_a_short_date() {
        let now = Some(day(9 * 60, 2, 14));
        let then = Some(day(20 * 60, 2, 12));
        let minutes = 9 * 60 + 24 * 60 + 4 * 60;
        assert_eq!(relative_label(Lang::Uk, minutes, now, then), "12 бер");
        assert_eq!(relative_label(Lang::En, minutes, now, then), "Mar 12");
    }

    #[test]
    fn an_unknown_calendar_falls_back_to_hours() {
        assert_eq!(relative_label(Lang::En, 3000, None, None), "50 h");
    }

    #[test]
    fn the_locale_picks_ukrainian_only_for_uk() {
        let pick = Lang::from_locale_vars;
        assert_eq!(pick(None, None, Some("uk_UA.UTF-8")), Lang::Uk);
        assert_eq!(pick(None, None, Some("en_US.UTF-8")), Lang::En);
        assert_eq!(pick(None, None, None), Lang::En);
        assert_eq!(pick(Some("C"), None, Some("uk_UA.UTF-8")), Lang::En);
        assert_eq!(pick(Some(""), Some("uk_UA"), Some("en_GB")), Lang::Uk);
        assert_eq!(pick(None, Some("de_DE"), Some("uk_UA")), Lang::En);
        assert_eq!(pick(None, None, Some("uk")), Lang::Uk);
    }
}
