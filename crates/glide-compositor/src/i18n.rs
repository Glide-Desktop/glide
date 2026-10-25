use std::sync::atomic::{AtomicU8, Ordering};

pub use glide_app::i18n::Lang;

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

glide_app::catalogue! {
    Open => "Open", "Відкрити";
    NewWindow => "New Window", "Нове вікно";
    PinToDock => "Pin to Dock", "Закріпити в доку";
    UnpinFromDock => "Unpin from Dock", "Відкріпити від дока";
    CloseAllWindows => "Close All Windows", "Закрити всі вікна";
    Applications => "Applications", "Застосунки";
    InputSettings => "Input Settings", "Налаштування вводу";
    Notifications => "Notifications", "Сповіщення";
    ClearAll => "Clear All", "Очистити все";
    NewNotification => "New notification", "Нове сповіщення";
    HeadsetConnected => "Connected", "Підключено";
    HeadsetBattery => "battery {pct}%", "заряд {pct}%";
    HsBattery => "Battery", "Заряд";
    HsVolume => "Volume", "Гучність";
    HsMode => "Mode", "Режим";
    HsCodec => "Codec", "Кодек";
    HsMusic => "Music", "Музика";
    HsCalls => "Calls", "Дзвінки";
    HsPlayingHere => "sound plays here", "звук іде сюди";
    HsUseOutput => "Play Sound Here", "Виводити звук сюди";
    NeedsAnswer => "Needs your answer", "Потрібна відповідь";
    PkWants => "wants administrator rights", "просить права адміністратора";
    PkNeedsRights => "Administrator rights are needed", "Потрібні права адміністратора";
    PkPassword => "Password", "Пароль";
    PkAllowKey => "allow", "дозволити";
    PkCancelKey => "cancel", "скасувати";
    PkChecking => "Checking the password…", "Перевірка пароля…";
    PkWrong => "The password didn't match.", "Пароль не підійшов.";
    PkLeftOne => "{n} attempt left.", "Залишилася {n} спроба.";
    PkLeftFew => "{n} attempts left.", "Залишилось {n} спроби.";
    PkLeftMany => "{n} attempts left.", "Залишилось {n} спроб.";
    PkBroken => "Couldn't check the password. Try again.", "Не вдалося перевірити пароль. Спробуйте ще раз.";
    PkCapsLock => "Caps Lock is on", "Caps Lock увімкнено";
    PkDetails => "Details", "Подробиці";
    PkHideDetails => "Hide details", "Сховати подробиці";
    PkAction => "Action", "Дія";
    PkProgram => "Program", "Програма";
    PkAsUser => "As", "Від імені";
    PkProtected => "Protected", "Захищено";
    PkCancel => "Cancel", "Скасувати";
    PkAllowed => "Allowed", "Дозволено";
    PkAllowedBody => "Permission granted. This closes by itself.", "Дозвіл надано. Запит закриється сам.";
    PkLockedTitle => "Too many failed attempts", "Забагато невдалих спроб";
    PkLockedBody => "Nothing was changed. Try again later.", "Нічого не змінено. Спробуйте пізніше.";
    PkUnderstood => "Got it", "Зрозуміло";
    PkShieldTitle => "While this request is open", "Поки відкрито цей запит";
    PkShieldSub => "Glide restricts the system so the password can't be intercepted.", "Glide обмежує систему, щоб пароль не можна було перехопити.";
    PkShieldKeys => "The keyboard is for this field only", "Клавіатура — лише для поля";
    PkShieldKeysBody => "No app, virtual keyboard or input method gets the keys. Shortcuts are off, except switching the layout.", "Натискань не отримує жодна програма, віртуальна клавіатура чи метод вводу. Гарячі клавіші вимкнено, крім перемикання розкладки.";
    PkShieldScreen => "The screen isn't recorded", "Екран не потрапляє в запис";
    PkShieldScreenBody => "Screenshots, recordings and screen casts show an empty panel in its place.", "Знімки, запис і трансляція екрана показують на місці запиту порожню панель.";
    PkShieldClipboard => "The clipboard is closed", "Буфер обміну закрито";
    PkShieldClipboardBody => "The password can't be copied out of the field or pasted into it.", "Пароль не можна скопіювати з поля чи вставити в нього.";
    PkShieldTop => "The request stays on top", "Запит завжди зверху";
    PkShieldTopBody => "The shell draws it itself: windows can't cover it, fake it or take the focus.", "Його малює сама оболонка: вікна не можуть його перекрити, підробити чи забрати фокус.";
    PkShieldDirect => "The password goes only to the system", "Пароль іде лише до системи";
    PkShieldDirectBody => "Straight to polkit-agent-helper-1, never over D-Bus or into logs. It's wiped from memory after the check.", "Напряму в polkit-agent-helper-1, без D-Bus і журналів. Після перевірки стирається з пам’яті.";
    NoNewNotifications => "No new notifications", "Немає нових сповіщень";
    CalendarAndNotifications => "Calendar & Notifications", "Календар та сповіщення";
    Media => "Media", "Медіа";
    Today => "Today", "Сьогодні";
    Caffeine => "Caffeine", "Кофеїн";
    Connected => "Connected", "Підключено";
    Off => "Off", "Вимкнено";
    Connecting => "Connecting…", "Підключення…";
    NotConnected => "Not connected", "Не підключено";
    NightLight => "Night Light", "Нічне світло";
    Screenshot => "Screenshot", "Знімок";
    WholeScreen => "Whole screen", "Увесь екран";
    ShotArea => "Area", "Область";
    ShotWindow => "Window", "Вікно";
    ShotScreen => "Screen", "Екран";
    ShotShadow => "Shadow", "Тінь";
    ShotCapture => "Capture", "Зняти";
    ShotCancel => "Cancel", "Скасувати";
    ShotMove => "Move and resize", "Перемістити й змінити розмір";
    ShotPen => "Pen", "Олівець";
    ShotArrow => "Arrow", "Стрілка";
    ShotRect => "Frame", "Рамка";
    ShotHighlight => "Highlight", "Маркер";
    ShotBlur => "Blur", "Розмиття";
    ShotText => "Text", "Текст";
    ShotStep => "Numbered step", "Нумерований крок";
    ShotUndo => "Undo", "Скасувати дію";
    ShotRedo => "Redo", "Повторити";
    ShotCopy => "Copy to clipboard", "Копіювати в буфер";
    ShotSave => "Save", "Зберегти";
    ShotCopied => "Screenshot copied", "Знімок скопійовано";
    ShotCopiedBody => "Paste it anywhere with Ctrl+V", "Вставте його будь-де через Ctrl+V";
    ShotOpen => "Open", "Відкрити";
    ShotShowInFolder => "Show in Folder", "Показати в теці";
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
    SectionWindows => "Windows", "Вікна";
    SectionRecentFiles => "Recent Files", "Недавні файли";
    SectionRun => "Command", "Команда";
    SectionProvided => "From Apps", "У програмах";
    RunPrefix => "Run", "Виконати";
    RunHint => "Press Enter to run", "Натисніть Enter, щоб виконати";
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
    SettingsTerminal => "Terminal", "Термінал";
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
    ShortcutsAsk => "{app} wants to use keyboard shortcuts", "{app} хоче перехоплювати клавіатурні скорочення";
    ShortcutsAskBody => "While it is focused, Glide's shortcuts go to it. Super+Esc takes them back.", "Поки вона у фокусі, скорочення Glide надходять їй. Super+Esc поверне їх.";
    ShortcutsAllow => "Allow", "Дозволити";
    ShortcutsRestored => "Keyboard shortcuts restored", "Клавіатурні скорочення повернуто";
    PermissionClipboard => "{app} wants to read and change the clipboard", "{app} хоче читати й змінювати буфер обміну";
    PermissionInput => "{app} wants to type and read keys in other apps", "{app} хоче вводити текст і читати клавіші в інших програмах";
    PermissionScreen => "{app} wants to record the screen", "{app} хоче записувати екран";
    PermissionBody => "Allow it only if you trust this program, then start it again.\n{path}", "Дозвольте, лише якщо довіряєте цій програмі, і запустіть її знову.\n{path}";
    PermissionAllow => "Allow", "Дозволити";
    PermissionMany => "{app} asks for access", "{app} просить доступ";
    AccessClipboard => "clipboard", "буфер обміну";
    AccessInput => "keys in other apps", "клавіші в інших програмах";
    AccessScreen => "screen recording", "запис екрана";
    CodeOffer => "Code from {app}: {code}", "Код від {app}: {code}";
    CodeOfferBody => "Insert it into the field you are typing in", "Вставити його в поле, де ви зараз вводите";
    CodeInsert => "Insert", "Вставити";
    KeychainUnlock => "Unlock the Glide keychain", "Розблокувати зв'язку ключів Glide";
    KeychainUnlockBody => "Enter the keychain password to fill the saved password", "Введіть пароль зв'язки ключів, щоб вставити збережений пароль";
    KeychainWrong => "That password did not open the keychain. Try again.", "Цей пароль не відкрив зв'язку ключів. Спробуйте ще раз.";
    KeychainUnlockAction => "Unlock", "Розблокувати";
    KeychainPlaceholder => "Keychain password", "Пароль зв'язки ключів";
    PasswordOffer => "Saved password for {app}", "Збережений пароль для {app}";
    PasswordOfferLocked => "The keychain is locked", "Зв'язка ключів заблокована";
    PasswordOfferLockedBody => "Unlock it to fill a saved password for {app}", "Розблокуйте її, щоб вставити збережений пароль для {app}";
    PasswordFill => "Fill {name}", "Вставити {name}";
    PasswordNoneSaved => "No saved password matches this app. Add one in Passwords.", "Для цієї програми немає збереженого пароля. Додайте його в застосунку «Паролі».";
    CastTitle => "Screen mirroring", "Дублювання екрана";
    CastSearching => "Looking for displays…", "Шукаю дисплеї поблизу…";
    CastSearchingBody => "Turn on screen mirroring (Miracast) on the TV", "Увімкніть дублювання екрана (Miracast) на телевізорі";
    CastPick => "Mirror the screen to", "Транслювати екран на";
    CastPickBody => "Choose a display nearby", "Виберіть дисплей поруч";
    CastNone => "No display found", "Дисплеїв не знайдено";
    CastNoneBody => "Turn on screen mirroring on the TV and try again", "Увімкніть дублювання екрана на телевізорі й спробуйте ще раз";
    CastConnecting => "Connecting to {name}…", "Підключення до {name}…";
    CastStreaming => "Mirroring to {name}", "Екран транслюється на {name}";
    CastStop => "Stop", "Зупинити";
    CastEnded => "Mirroring ended", "Трансляцію завершено";
    CastFailed => "Mirroring stopped: {reason}", "Трансляцію зупинено: {reason}";
    CastUnavailable => "Screen mirroring is unavailable: {reason}", "Дублювання екрана недоступне: {reason}";
    CastIndicator => "Mirroring to {name}", "Трансляція на {name}";
    PasswordNoKeychain => "Create a vault in Passwords first.", "Спершу створіть сховище в застосунку «Паролі».";
    SomeApp => "An app", "Програма";
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
    QsWifiSettings => "Wi-Fi Settings…", "Налаштування Wi-Fi…";
    QsBluetoothSettings => "Bluetooth Settings…", "Налаштування Bluetooth…";
    QsSoundSettings => "Sound Settings…", "Налаштування звуку…";
    QsEnterprise => "Set up in Settings", "Налаштувати в Settings";
    QsNoOutputs => "No output devices", "Немає пристроїв виводу";
    QsWifiIsOff => "Wi-Fi is off", "Wi-Fi вимкнено";
    QsBluetoothIsOff => "Bluetooth is off", "Bluetooth вимкнено";
    QsSearching => "Searching…", "Пошук…";
    QsNoNetworks => "No networks found", "Мереж не знайдено";
    QsNoDevices => "No paired devices", "Немає спарених пристроїв";
    QsPassword => "Password", "Пароль";
    QsWrongPassword => "Wrong password", "Неправильний пароль";
    QsCouldNotConnect => "Couldn\u{2019}t connect", "Не вдалося підключитися";
    QsDisconnecting => "Disconnecting…", "Відключення…";
    QsOn => "On", "Увімк.";
    QsOff => "Off", "Вимк.";
    QsDarkTheme => "Dark Style", "Темна тема";
    QsScreenshot => "Screenshot", "Знімок екрана";
    QsDisplay => "Display", "Дисплей";
    QsProfileSaver => "Saver", "Економія";
    QsProfileBalanced => "Balanced", "Баланс";
    QsProfilePerformance => "Performance", "Швидкодія";
    QsAskLock => "Lock the screen?", "Заблокувати екран комп\u{2019}ютера?";
    QsAskSleep => "Put the computer to sleep?", "Перевести комп\u{2019}ютер у режим сну?";
    QsPowerCountdown => "Automatically in {n} s.", "Автоматично через {n} с.";
    QsDefaultDevice => "Default device", "Типовий пристрій";
    QsKindSpeakers => "Speakers", "Динаміки";
    QsKindHeadphones => "Headphones", "Навушники";
    QsKindHeadset => "Headset", "Гарнітура";
    QsKindWebcam => "Webcam", "Вебкамера";
    QsNoInputs => "No input devices", "Немає пристроїв вводу";
    QsConnectedNetwork => "Connected network", "Підключена мережа";
    QsAvailableNetworks => "Available networks", "Доступні мережі";
    QsConnectedDevices => "Connected devices", "Підключені пристрої";
    QsPairedDevices => "Paired devices", "Спарені пристрої";
    QsOutputDevice => "Output device", "Пристрій виводу";
    QsInputDevice => "Input device (microphone)", "Пристрій вводу (мікрофон)";
    QsBatteryTelemetry => "Battery telemetry", "Телеметрія акумулятора";
    QsPerformanceProfile => "Performance profile", "Профіль продуктивності";
    QsConnect => "Connect", "Підключити";
    QsDisconnect => "Disconnect", "Від\u{2019}єднати";
    QsSecured => "Secured", "Захищена";
    QsOpenNetwork => "Open network", "Відкрита мережа";
    QsSaved => "Saved", "Збережена";
    QsPaired => "Paired", "Спарений";
    QsBatteryLevel => "Battery {}%", "Заряд {}%";
    QsPasswordFor => "Password for \u{201c}{ssid}\u{201d}", "Введіть пароль для \u{ab}{ssid}\u{bb}";
    QsHealth => "Health", "Здоров\u{2019}я";
    QsHealthy => "Normal", "Нормальний";
    QsWorn => "Worn", "Зношений";
    QsCycles => "Cycles", "Цикли";
    QsCyclesUnit => "cycles", "циклів";
    QsDraw => "Power draw", "Споживання";
    QsWatts => "W", "Вт";
    QsCapacity => "Full capacity", "Повна ємність";
    QsOfWattHours => "of {} Wh", "з {} Вт\u{b7}год";
    QsProfileSaverHint => "Lower clocks for longer battery life", "Знижує частоти для довшої роботи від батареї";
    QsProfileBalancedHint => "Balances power use and responsiveness", "Збалансовує споживання та швидкість відгуку";
    QsProfilePerformanceHint => "Top clocks for heavy work", "Найвищі частоти для важких задач";
    QsChargeLimit => "Limit charging to {n}%", "Обмежити заряджання до {n}%";
    QsChargeLimitHint => "Keeps the battery healthy on mains power", "Береже акумулятор, поки ноутбук від мережі";
    QsWaitingForAuth => "Waiting for permission…", "Очікування дозволу…";
    QsCouldNotChange => "Couldn\u{2019}t change it", "Не вдалося змінити";
    TbBattery => "Battery", "Акумулятор";
    TbBatteryCharging => "Charging", "Заряджається";
    TbBatteryOnBattery => "On battery", "Від акумулятора";
    TbBatteryFull => "Fully charged", "Повністю заряджено";
    TbBatteryNotCharging => "Not charging", "Не заряджається";
    TbBatteryTimeLeft => "{} remaining", "Залишилось {}";
    TbBatteryTimeToFull => "{} until full", "До повного заряду {}";
    TbDurationHoursMinutes => "{h} h {m} min", "{h} год {m} хв";
    TbDurationMinutes => "{m} min", "{m} хв";
    TbPowerSaver => "Power Saver", "Енергозбереження";
    AirplaneMode => "Airplane Mode", "Режим польоту";
    WelcomeTitle => "Welcome to Glide", "Ласкаво просимо до Glide";
    WelcomeBody => "Tap Super for the launcher, Super+O for the overview and Super+V for notifications. Swipe three fingers sideways to switch workspaces and up for the overview.", "Натисніть Super для запуску програм, Super+O для огляду, Super+V для сповіщень. Проведіть трьома пальцями вбік, щоб перемкнути робочий простір, і вгору — для огляду.";
    WelcomeShortcuts => "Shortcuts", "Комбінації клавіш";
    WelcomeSettings => "Settings", "Налаштування";
    Unavailable => "Unavailable", "Недоступно";
    TbPowerBalanced => "Balanced", "Збалансований";
    TbPowerPerformance => "Performance", "Продуктивність";
    TbBatterySettings => "Battery Settings…", "Налаштування акумулятора…";
    TbPrivacy => "Privacy", "Конфіденційність";
    TbMicrophone => "Microphone", "Мікрофон";
    TbCamera => "Camera", "Камера";
    TbScreenRecording => "Screen Recording", "Запис екрана";
    TbUnknownApp => "Unknown app", "Невідомий застосунок";
    GreeterPassword => "Enter Password", "Введіть пароль";
    GreeterHint => "Type your password and press Enter", "Введіть пароль і натисніть Enter";
    GreeterWrongShort => "Incorrect password.", "Невірний пароль.";
    GreeterLockedFor => "Locked for", "Заблоковано на";
    GreeterLockedHint => "Too many failed attempts. You can choose another user.", "Забагато невдалих спроб. Можна вибрати іншого користувача.";
    GreeterLockedHintLock => "Too many failed attempts.", "Забагато невдалих спроб.";
    GreeterEntering => "Signing in to {session}…", "Вхід у {session}…";
    GreeterAgainSleep => "Again to sleep", "Ще раз — сон";
    GreeterAgainRestart => "Again to restart", "Ще раз — перезапуск";
    GreeterAgainShutDown => "Again to shut down", "Ще раз — вимкнути";
    GreeterToastSleep => "Going to sleep…", "Перехід у сон…";
    GreeterToastRestart => "Restarting…", "Перезапуск…";
    GreeterToastShutDown => "Shutting down…", "Вимкнення…";
    GreeterPowerNote =>"The action runs on the second press.", "Дія спрацює з другого натискання.";
    LockNoticesOne => "{n} new notification", "{n} нове сповіщення";
    LockNoticesFew => "{n} new notifications", "{n} нових сповіщення";
    LockNoticesMany => "{n} new notifications", "{n} нових сповіщень";
    LockNoticesHidden => "hidden", "приховано";
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
    LockHint => "Type your password to unlock", "Введіть пароль, щоб розблокувати";
    CapsLockOn => "Caps Lock is on", "Увімкнено Caps Lock";
    CapsLockOff => "Caps Lock is off", "Вимкнено Caps Lock";
    RemovableSummary => "{device} connected", "Під\u{2019}єднано {device}";
    RemovableDrive => "A drive", "Накопичувач";
    RemovableOpen => "Open", "Відкрити";
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
    Netbird => "NetBird", "NetBird";
    NbSettings => "NetBird Settings…", "Налаштування NetBird…";
    NbSettingsAction => "Open NetBird", "Відкрити NetBird";
    NbLoading => "Loading…", "Завантаження…";
    NbDisconnected => "Disconnected", "Відключено";
    NbLoginRequired => "Login required", "Потрібен вхід";
    NbStateLoginFailed => "Login failed", "Не вдалося увійти";
    NbSessionExpired => "Session expired", "Сесія завершилася";
    NbNotRunning => "Not running", "Не запущено";
    NbServiceDown => "NetBird service is not running", "Служба NetBird не запущена";
    NbServiceDownHint => "Glide reconnects as soon as the service starts", "Glide під\u{2019}єднається, щойно служба запуститься";
    NbConnectingHint => "Joining your network", "Під\u{2019}єднання до вашої мережі";
    NbOffHint => "Turn on to join your network", "Увімкніть, щоб під\u{2019}єднатися до мережі";
    NbPeersLine => "{ip} \u{b7} {n} of {total} peers", "{ip} \u{b7} {n} з {total} вузлів";
    NbSignIn => "Sign in", "Увійти";
    NbSignInHint => "Sign in with your browser to connect", "Увійдіть через браузер, щоб підключитися";
    NbExpiresIn => "Session expires in {time}", "Сесія спливає через {time}";
    NbRenew => "Renew", "Подовжити";
    NbRenewHint => "A browser sign-in keeps this device connected", "Вхід через браузер збереже підключення пристрою";
    NbLessThanMinute => "less than a minute", "менш ніж хвилину";
    NbMinutes => "{n} min", "{n} хв";
    NbHours => "{n} h", "{n} год";
    NbDays => "{n} d", "{n} дн.";
    NbProfiles => "Profiles", "Профілі";
    NbNoEmail => "Other", "Інший";
    NbSwitching => "Switching…", "Перемикання…";
    NbExitNode => "Exit node", "Вихідний вузол";
    NbExitNone => "None", "Немає";
    NbExitNoneHint => "Direct connection without an exit node", "Пряме з\u{2019}єднання без вихідного вузла";
    NbActive => "Active", "Активний";
    NbInactive => "Inactive", "Неактивний";
    NbNetwork => "Network", "Мережа";
    NbPeers => "Peers", "Вузли";
    NbPeersCount => "{n} of {total} connected", "{n} з {total} на зв\u{2019}язку";
    NbResources => "Resources", "Ресурси";
    NbResourcesCount => "{n} of {total} active", "{n} з {total} активні";
    NbAuthentication => "Authentication", "Автентифікація";
    NbConnectivity => "Connectivity", "З\u{2019}єднання";
    NbSystem => "System", "Система";
    NbInfo => "Info", "Інформація";
    NbWarning => "Warning", "Попередження";
    NbError => "Error", "Помилка";
    NbCritical => "Critical", "Критично";
    NbEventTitle => "{severity}: {category}", "{severity}: {category}";
    NbErrorTitle => "NetBird error", "Помилка NetBird";
    NbConnectFailed => "Couldn\u{2019}t connect", "Не вдалося підключитися";
    NbDisconnectFailed => "Couldn\u{2019}t disconnect", "Не вдалося відключитися";
    NbSwitchFailed => "Couldn\u{2019}t switch to {profile}", "Не вдалося перемкнутися на {profile}";
    NbExitFailed => "Couldn\u{2019}t update the exit node {name}", "Не вдалося змінити вихідний вузол {name}";
    NbLoginFailed => "Login failed: {reason}", "Не вдалося увійти: {reason}";
    NbLoginTitle => "Complete login in your browser", "Завершіть вхід у браузері";
    NbLoginBody => "A browser tab is open so you can finish signing in to NetBird. Not seeing it?", "Відкрито вкладку браузера, щоб завершити вхід у NetBird. Не бачите її?";
    NbOpenAgain => "Open again", "Відкрити знову";
    NbExpiredTitle => "NetBird session expired", "Сесія NetBird завершилася";
    NbExpiredBody => "Your NetBird session has expired. Please sign in again.", "Сесія NetBird завершилася. Увійдіть знову.";
    NbDeadlineTitle => "Session deadline rejected", "Термін сесії відхилено";
    NbDeadlineBody => "The server sent an invalid session deadline. Please sign in again.", "Сервер надіслав некоректний термін сесії. Увійдіть знову.";
    NbWarningTitle => "Session expires soon", "Сесія скоро спливе";
    NbWarningBody => "Your NetBird session expires in {time}. Extend it now to stay connected.", "Сесія NetBird спливає через {time}. Подовжте її, щоб лишатися на зв\u{2019}язку.";
    NbWarningGeneric => "Your NetBird session is about to expire. Extend it now to stay connected.", "Сесія NetBird ось-ось спливе. Подовжте її, щоб лишатися на зв\u{2019}язку.";
    NbExtendNow => "Extend now", "Подовжити";
    NbDismiss => "Dismiss", "Пізніше";
    NbFinalTitle => "This device will disconnect soon", "Пристрій скоро від\u{2019}єднається";
    NbFinalBody => "Renew the NetBird session with a browser sign-in to stay connected.", "Подовжте сесію NetBird входом через браузер, щоб лишатися на зв\u{2019}язку.";
    NbRenewSession => "Renew session", "Подовжити сесію";
    NbExtendedTitle => "NetBird session extended", "Сесію NetBird подовжено";
    NbExtendedBody => "Your session has been refreshed.", "Сесію оновлено.";
    NbExtendFailed => "Couldn\u{2019}t extend the NetBird session", "Не вдалося подовжити сесію NetBird";
    NbUpdateTitle => "NetBird update available", "Доступне оновлення NetBird";
    NbUpdateBody => "NetBird {version} is available.", "Доступна версія NetBird {version}.";
    NbUpdateEnforced => " Your administrator requires this update.", " Адміністратор вимагає це оновлення.";
    NbInstallNow => "Install now", "Встановити";
    NbDownload => "Download", "Завантажити";
    NbUpdatingTitle => "Updating NetBird", "Оновлення NetBird";
    NbUpdatingBody => "NetBird {version} is being installed; the service restarts on its own.", "Встановлюється NetBird {version}; служба перезапуститься сама.";
    NbUpdateFailed => "Couldn\u{2019}t install the NetBird update.", "Не вдалося встановити оновлення NetBird.";
    NbPolicyTitle => "NetBird settings updated", "Налаштування NetBird оновлено";
    NbPolicyBody => "Your NetBird configuration was updated by your IT policy.", "Конфігурацію NetBird оновлено політикою вашої організації.";
    NbOutdatedTitle => "NetBird service is outdated", "Служба NetBird застаріла";
    NbOutdatedBody => "Update the NetBird service to manage it from Glide.", "Оновіть службу NetBird, щоб керувати нею з Glide.";
    NbErrClockSkew => "This device\u{2019}s clock is out of sync with the server. Sync the clock and try again.", "Годинник цього пристрою розходиться із сервером. Синхронізуйте час і спробуйте знову.";
    NbErrTokenExpired => "Your sign-in token has expired. Please sign in again.", "Термін дії токена входу минув. Увійдіть знову.";
    NbErrSignature => "The token signature is invalid. Please contact your administrator.", "Підпис токена недійсний. Зверніться до адміністратора.";
    NbErrSessionExpired => "Your session has expired. Please sign in again.", "Сесія завершилася. Увійдіть знову.";
    NbErrSetup =>"The setup key is missing or invalid.", "Ключ налаштування відсутній або недійсний.";
    NbErrPermission => "Sign-in was rejected by the server.", "Сервер відхилив вхід.";
    NbErrUnreachable => "The NetBird service is not responding.", "Служба NetBird не відповідає.";
    NbErrUnknown => "The operation failed.", "Операція не вдалася.";
}

#[must_use]
pub fn t(key: Key) -> &'static str {
    key.text(language())
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
    let text = |key: Key| key.text(lang);
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
    match lang {
        Lang::Uk => MONTH_NAMES_UK[month0 as usize % 12],
        Lang::En => lang.month_of_date(month0 as usize),
    }
}

#[must_use]
pub fn long_date(lang: Lang, weekday_from_sunday: u32, day: u32, month0: u32) -> String {
    let weekday = lang.weekday(weekday_from_sunday as usize);
    let month = lang.month_of_date(month0 as usize);
    match lang {
        Lang::Uk => format!("{weekday}, {day} {month}"),
        Lang::En => format!("{weekday}, {month} {day}"),
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
    fn long_dates_use_full_weekdays_and_the_genitive_month_in_ukrainian() {
        assert_eq!(long_date(Lang::Uk, 5, 25, 8), "п\u{2019}ятниця, 25 вересня");
        assert_eq!(long_date(Lang::En, 5, 25, 8), "Friday, September 25");
        assert_eq!(long_date(Lang::Uk, 0, 1, 0), "неділя, 1 січня");
    }

    #[test]
    fn only_micro_labels_are_uppercase_in_the_catalogue() {
        for key in Key::ALL {
            for text in [key.uk(), key.en()] {
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
        assert_eq!(label(0), Key::JustNow.uk());
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
}
