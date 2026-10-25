pub use glide_sdk::Lang;

glide_sdk::catalogue! {
    AppName => "Passwords", "Паролі";
    Maximize => "Maximize", "Розгорнути";
    CloseWindow => "Close", "Закрити";
    Sync => "Sync", "Синхронізувати";
    LockAll => "Lock", "Заблокувати";
    New => "New", "Новий";
    NewLogin => "New Login", "Новий логін";
    NewNote => "New Secure Note", "Нова нотатка";
    NewCard => "New Card", "Нова картка";
    AllVaults => "All vaults", "Усі сховища";
    Categories => "Categories", "Категорії";
    All => "All", "Усі";
    Logins => "Logins", "Логіни";
    Passkeys => "Passkeys", "Passkey";
    Codes => "Codes", "Коди";
    Cards => "Cards", "Картки";
    Notes => "Notes", "Нотатки";
    Sent => "Sent", "Надіслане";
    Security => "Security", "Безпека";
    Check => "Password check", "Перевірка паролів";
    Trash => "Trash", "Кошик";
    JustNow => "just now", "щойно";
    OnDevice => "on this device", "на пристрої";
    OfflineStatus => "offline", "офлайн";
    Syncing => "syncing", "синхронізується";
    FailedStatus => "failed", "помилка";
    LockedStatus => "locked", "заблоковано";
    WaitingOne => "{n} change waits for the network", "{n} зміна чекає на мережу";
    WaitingFew => "{n} changes wait for the network", "{n} зміни чекають на мережу";
    WaitingMany => "{n} changes wait for the network", "{n} змін чекають на мережу";
    SearchIn => "Search {n} items", "Пошук у {n} записах";
    SearchLogins => "Search {n} logins", "Пошук у {n} логінах";
    SearchPasskeys => "Search {n} passkeys", "Пошук серед {n} passkey";
    SearchCodes => "Search {n} codes", "Пошук у {n} кодах";
    SearchCards => "Search {n} cards", "Пошук у {n} картках";
    SearchNotes => "Search {n} notes", "Пошук у {n} нотатках";
    SearchTrash => "Search the Trash", "Пошук у кошику";
    SearchItems => "Search", "Пошук";
    Sorting => "Sorting", "Сортування";
    Recent => "Recent", "Нещодавні";
    ByName => "By name", "За назвою";
    Changed => "Changed {date}", "Змінено {date}";
    EditItem => "Edit", "Редагувати";
    More => "More", "Ще";
    Username => "User name", "Ім'я користувача";
    Password => "Password", "Пароль";
    ShowPassword => "Show password", "Показати пароль";
    HidePassword => "Hide password", "Сховати пароль";
    CopyUsername => "Copy user name", "Скопіювати ім'я";
    CopyPassword => "Copy password", "Скопіювати пароль";
    CopyCode => "Copy code", "Скопіювати код";
    Code => "Verification code", "Код підтвердження";
    Passkey => "Passkey", "Passkey";
    PasskeySynced => "Synced", "Синхронізується";
    PasskeyCreated => "{site} · created {date}", "{site} · створено {date}";
    SitesApps => "Sites and apps", "Сайти й застосунки";
    Domain => "Domain", "Домен";
    MatchHost => "Host", "Хост";
    MatchStarts => "Starts with", "Початок адреси";
    MatchExact => "Exact", "Точна адреса";
    MatchRegex => "Regular expression", "Регулярний вираз";
    MatchNever => "Never fill", "Не підставляти";
    AppKind => "App", "Застосунок";
    OpenSite => "Open site", "Відкрити сайт";
    Bind => "Link an app or site", "Прив'язати застосунок або сайт";
    Created => "Created {date}", "Створено {date}";
    HistoryCount => "Password history ({n})", "Історія паролів ({n})";
    Weak => "Weak", "Слабкий";
    Fair => "Fair", "Середній";
    Strong => "Strong", "Надійний";
    VeryStrong => "Very strong", "Дуже надійний";
    CharsOne => "{n} character", "{n} символ";
    CharsFew => "{n} characters", "{n} символи";
    CharsMany => "{n} characters", "{n} символів";
    WordsOne => "{n} word", "{n} слово";
    WordsFew => "{n} words", "{n} слова";
    WordsMany => "{n} words", "{n} слів";
    DigitsOne => "{n} digit", "{n} цифра";
    DigitsFew => "{n} digits", "{n} цифри";
    DigitsMany => "{n} digits", "{n} цифр";
    ReusedIn => "reused in {n} items", "повторюється в {n} записах";
    CopiedUsername => "User name copied", "Ім'я користувача скопійовано";
    CopiedPassword => "Password copied", "Пароль скопійовано";
    CopiedCode => "Code copied", "Код скопійовано";
    CopiedLink => "Link copied", "Посилання скопійовано";
    ClearsIn => "clears in {n} s", "очиститься через {n} с";
    Okay => "OK", "Гаразд";
    EditTitle => "Edit item", "Редагування запису";
    NewTitle => "New item", "Новий запис";
    EditHint => "Esc to cancel · Ctrl+S to save", "Esc — скасувати · Ctrl+S — зберегти";
    Name => "Name", "Назва";
    Vault => "Vault", "Сховище";
    Folder => "Folder", "Тека";
    NoFolder => "No folder", "Без теки";
    TotpKey => "TOTP key", "Ключ TOTP";
    Generate => "Generate", "Згенерувати";
    GoesToHistory => "The previous one goes to history", "Попередній піде в історію";
    Site => "Site", "Сайт";
    AddApp => "App…", "Застосунок…";
    AddField => "Field", "Поле";
    FieldName => "Field name", "Назва поля";
    FieldValue => "Value", "Значення";
    Remove => "Remove", "Прибрати";
    ToTrash => "Move to Trash", "У кошик";
    Cancel => "Cancel", "Скасувати";
    Save => "Save", "Зберегти";
    AppId => "App ID", "Ідентифікатор застосунку";
    AppTitle => "App name", "Назва застосунку";
    NotesPlaceholder => "Notes", "Нотатки";
    CardHolder => "Card holder", "Власник картки";
    CardNumber => "Card number", "Номер картки";
    CardCode => "Security code", "Код безпеки";
    CardExpiry => "Expires", "Термін дії";
    CardBrand => "Brand", "Платіжна система";
    ModePassword => "Password", "Пароль";
    ModePhrase => "Phrase", "Фраза";
    ModePin => "PIN", "PIN";
    Length => "Length", "Довжина";
    WordCount => "Number of words", "Кількість слів";
    DigitCount => "Number of digits", "Кількість цифр";
    Upper => "Capital letters", "Великі літери";
    Digits => "Digits", "Цифри";
    Symbols => "Symbols", "Символи";
    AvoidSimilar => "No look-alike characters", "Без схожих символів";
    Capitalise => "Capitalised words", "Слова з великої літери";
    AddDigit => "Add a digit", "Додати цифру";
    HistoryButton => "History", "Історія";
    Use => "Use", "Використати";
    Another => "Another one", "Інший варіант";
    Copy => "Copy", "Скопіювати";
    GeneratorTitle => "Password generator", "Генератор паролів";
    SearchSent => "Search sent items", "Пошук у надісланому";
    NewSecret => "New secret", "Новий секрет";
    ActiveCount => "Active · {n}", "Активні · {n}";
    FinishedCount => "Finished · {n}", "Завершені · {n}";
    ActiveBadge => "Active", "Активне";
    Exhausted => "Used up", "Вичерпано";
    Expired => "Expired", "Спливло";
    DisabledBadge => "Disabled", "Вимкнено";
    ExpiresHours => "Expires in {n} h", "Спливає через {n} год";
    ExpiresDays => "Expires in {n} d", "Спливає через {n} дн";
    ExpiredAgo => "Expired {n} d ago", "Спливло {n} дн тому";
    OpenedOf => "opened {a} of {b}", "відкрито {a} з {b}";
    OpenedTimes => "opened {a} times", "відкрито {a} разів";
    WithPassword => "With a password", "З паролем";
    DeletedOn => "deleted {date}", "видалиться {date}";
    SendNote => "Links are made by a Bitwarden server, cloud or your own. The key is in the link after «#», so the server never sees the text.", "Посилання створює сервер Bitwarden, хмарний чи власний. Ключ розшифрування лежить у самому посиланні після «#», тож сервер тексту не бачить.";
    OnlyYou => "(only you see it)", "(бачите лише ви)";
    TextLabel => "Text", "Текст";
    Lifetime => "Expires after", "Термін дії";
    OneHour => "1 h", "1 год";
    OneDay => "1 day", "1 день";
    SevenDays => "7 days", "7 днів";
    ThirtyDays => "30 days", "30 днів";
    MaxOpens => "Maximum opens", "Максимум відкриттів";
    OpenPassword => "Password to open", "Пароль на відкриття";
    OtherChannel => "Pass it on another channel", "Передайте його іншим каналом";
    HideText => "Hide the text until «Show» is pressed", "Приховати текст до натискання «Показати»";
    HideEmail => "Hide my email", "Не показувати мою пошту";
    SendVia => "Send through", "Надіслати через";
    CreateLink => "Create link", "Створити посилання";
    LinkReady => "The link is ready", "Посилання готове";
    LinkSummary => "{name} · expires {date} · up to {n} opens", "{name} · спливає {date} · до {n} відкриттів";
    PointCamera => "Point a phone camera at it", "Наведіть камеру телефона";
    SendHeading => "Send", "Надіслати";
    ByMail => "By email", "Поштою";
    ToApp => "To an app…", "У застосунок…";
    LinkNote => "The link opens the text without an account. Every open counts; once the limit is used up the link stops working.", "Посилання відкриває текст без акаунта. Кожне відкриття зменшує ліміт; коли він вичерпається, посилання перестане працювати.";
    DisableLink => "Disable link", "Вимкнути посилання";
    DeleteSend => "Delete", "Видалити";
    Done => "Done", "Готово";
    NeedsServer => "Add a Bitwarden account to send secrets.", "Щоб надсилати секрети, додайте акаунт Bitwarden.";
    StepOf => "Step {n} of 4", "Крок {n} з 4";
    AddVault => "Add a vault", "Додати сховище";
    AddVaultBody => "Every vault has its own password. You can search and fill from all of them at once.", "Кожне сховище має власний пароль. Шукати й вставляти можна з усіх одразу.";
    Bitwarden => "Bitwarden", "Bitwarden";
    BitwardenBody => "bitwarden.com, .eu or your own server. Syncs both ways.", "Хмара bitwarden.com, .eu або власний сервер. Синхронізується в обидва боки.";
    OnDeviceTitle => "On this device", "На цьому пристрої";
    OnDeviceBody => "An encrypted file. No server, no sync.", "Зашифрований файл. Без сервера й синхронізації.";
    KeePass => "KeePass file", "Файл KeePass";
    KeePassBody => "Later: open a .kdbx file.", "Згодом: відкрити .kdbx.";
    Next => "Next", "Далі";
    SignInBitwarden => "Sign in to Bitwarden", "Увійти в Bitwarden";
    MasterNote => "The master password never leaves this computer: only a key made from it goes to the server.", "Майстер-пароль не залишає цей комп'ютер: на сервер іде лише ключ, похідний від нього.";
    Server => "Server", "Сервер";
    OwnServer => "Own server", "Власний сервер";
    ServerAddress => "Server address", "Адреса сервера";
    ServerHint => "Vaultwarden or Bitwarden on your own server. The address is checked before signing in.", "Vaultwarden або Bitwarden на своєму сервері. Адресу перевіримо до входу.";
    Email => "Email", "Пошта";
    MasterPassword => "Master password", "Майстер-пароль";
    Back => "Back", "Назад";
    SignIn => "Sign in", "Увійти";
    ConfirmSignIn => "Confirm the sign-in", "Підтвердьте вхід";
    ConfirmBody => "Enter the six-digit code from your authenticator app for {email}.", "Введіть шестизначний код із застосунку-автентифікатора для {email}.";
    EmailCodeBody => "Enter the code Bitwarden sent to {email}.", "Введіть код, який Bitwarden надіслав на {email}.";
    Remember => "Remember this device", "Запам'ятати цей пристрій";
    OtherMethod => "Another way: email code", "Інший спосіб: код поштою";
    Confirm => "Confirm", "Підтвердити";
    Loading => "Loading the vault", "Завантажуємо сховище";
    LoadingBody => "Items are decrypted on this device. You can close the window; syncing goes on.", "Записи розшифровуються на цьому пристрої. Можна закрити вікно: синхронізація продовжиться.";
    KeysDone => "Keys decrypted", "Ключі розшифровано";
    ItemsLoaded => "Items", "Записи";
    FoldersLoaded => "Folders and organisation collections", "Теки й колекції організацій";
    NameInApp => "Name in Passwords", "Назва в Паролях";
    CreateVault => "Create a vault", "Створити сховище";
    CreateVaultBody => "Choose a password for the vault on this device. It cannot be recovered if you forget it.", "Виберіть пароль для сховища на цьому пристрої. Якщо його забути, відновити записи не вийде.";
    VaultPassword => "Vault password", "Пароль сховища";
    RepeatPassword => "Repeat the password", "Повторіть пароль";
    Mismatch => "The passwords differ", "Паролі не збігаються";
    ConflictTitle => "{name} was changed in two places", "{name} змінено у двох місцях";
    ConflictBody => "While this device was offline the item was changed elsewhere. Choose the version to keep.", "Поки цей пристрій був офлайн, запис змінили на іншому. Оберіть, яку версію лишити.";
    ThisDevice => "This device", "Цей пристрій";
    ServerVersion => "Server · {vault}", "Сервер · {vault}";
    Newer => "Newer", "Новіша";
    PasswordDiffers => "Password · differs", "Пароль · відрізняється";
    NotesDiffer => "Notes · differ", "Нотатки · відрізняються";
    HistoryKeeps => "The password you do not keep is not lost: it goes to the item's password history.", "Невибраний пароль не зникне: він піде в історію паролів запису.";
    DecideLater => "Decide later", "Вирішити пізніше";
    KeepBoth => "Keep both", "Залишити обидві";
    KeepServer => "Keep the server version", "Залишити версію з сервера";
    KeepLocal => "Keep this device's version", "Залишити версію цього пристрою";
    CheckTitle => "Password check", "Перевірка паролів";
    CheckSub => "{n} items in {m} vaults · checked just now", "{n} записів у {m} сховищах · перевірено щойно";
    CheckAgain => "Check again", "Перевірити знову";
    WeakTitle => "Weak", "Слабкі";
    WeakSub => "Easy to guess", "Легко підібрати";
    ReusedTitle => "Reused", "Повторені";
    ReusedSubOne => "{n} shared password", "{n} однаковий пароль";
    ReusedSubFew => "{n} shared passwords", "{n} однакові паролі";
    ReusedSubMany => "{n} shared passwords", "{n} однакових паролів";
    OldTitle => "Old", "Давні";
    OldSub => "Not changed for over a year", "Не змінювались понад рік";
    LeakedTitle => "Leaked", "Злиті";
    LeakedSub => "Found in known breaches", "Знайдені у відомих витоках";
    LeakNote => "The breach check sends Have I Been Pwned only the first 5 characters of the password's SHA-1. The password itself never leaves the computer.", "Перевірка витоків надсилає в Have I Been Pwned лише перші 5 символів SHA-1 від пароля. Сам пароль не залишає комп'ютер.";
    SameIn => "One password in {n} items", "Один пароль у {n} записах";
    ChangePassword => "Change password", "Змінити пароль";
    ChangeHint => "«Change password» opens the site or app with the generator beside it; the new password is saved in the same vault.", "«Змінити пароль» відкриває сайт або застосунок і генератор поруч; новий пароль збережеться в тому самому сховищі.";
    LeakedBadge => "Leaked", "Злитий";
    NoProblems => "No problems found", "Проблем не знайдено";
    NoProblemsBody => "Every password here is strong and used once.", "Усі паролі тут надійні й не повторюються.";
    SettingsTitle => "Settings", "Налаштування";
    Vaults => "Vaults", "Сховища";
    ChangeVaultPassword => "Change password…", "Змінити пароль…";
    SyncedBadge => "Synced", "Синхронізовано";
    OfflineBadge => "Offline", "Офлайн";
    LockedBadge => "Locked", "Заблоковано";
    SaveNewIn => "Save new items in", "Нові записи зберігати в";
    AddVaultEllipsis => "Add a vault…", "Додати сховище…";
    Filling => "Filling", "Вставлення";
    ClickTopbar => "Click in the top bar", "Клік у верхній панелі";
    ClickHint => "What a click on a login does; the other actions stay in its menu", "Що робить клік по логіну; інші дії лишаються в меню";
    OfferLogin => "Offer logins in password fields", "Пропонувати логін у полі пароля";
    OfferHint => "The island offers one when an app focuses a password field", "Острів показує пропозицію, коли застосунок фокусує поле пароля";
    TypeFallback => "Type when a field takes no text", "Друкувати, якщо поле не приймає текст";
    TypeHint => "Key presses; only characters of the current layout", "Натискання клавіш; лише символи поточної розкладки";
    Clipboard => "Clipboard", "Буфер обміну";
    ClearAfter => "Clear copies after", "Очищати скопійоване через";
    NeverClear => "Never", "Ніколи";
    SecondsOne => "{n} second", "{n} секунда";
    SecondsFew => "{n} seconds", "{n} секунди";
    SecondsMany => "{n} seconds", "{n} секунд";
    MinutesOne => "{n} minute", "{n} хвилина";
    MinutesFew => "{n} minutes", "{n} хвилини";
    MinutesMany => "{n} minutes", "{n} хвилин";
    NoHistory => "Keep copies out of clipboard history", "Не зберігати в історії буфера";
    NoHistoryHint => "Marks copies as secrets for clipboard managers", "Позначає копію як секрет для менеджерів буфера";
    Locking => "Locking", "Блокування";
    LockWithScreen => "Lock with the screen", "Блокувати разом з екраном";
    LockIdle => "Lock when not used for", "Блокувати, якщо не користуватись";
    Fingerprint => "Unlock with a fingerprint", "Розблоковувати відбитком пальця";
    LaterNote => "Later", "Згодом";
    OtherApps => "Other apps", "Інші програми";
    SecretService => "Keep other apps' passwords", "Зберігати паролі інших програм";
    SecretServiceHint => "A store for Chrome, git and NetworkManager through Secret Service", "Сховище для Chrome, git, NetworkManager через Secret Service";
    ImportLabel => "Import from Bitwarden, KeePass or CSV", "Імпорт з Bitwarden, KeePass або CSV";
    ImportButton => "Import…", "Імпортувати…";
    ExportLabel => "Export to an encrypted file", "Експорт у зашифрований файл";
    ExportButton => "Export…", "Експортувати…";
    InsertPassword => "Insert password", "Вставити пароль";
    InsertUsername => "Insert user name", "Вставити ім'я користувача";
    InsertBoth => "Name, Tab, password", "Ім'я, Tab, пароль";
    InsertCode => "Insert code", "Вставити код";
    LockedTitle => "Passwords are locked", "Паролі заблоковано";
    LockedBody => "Unlock to see and fill your logins.", "Розблокуйте, щоб бачити й вставляти логіни.";
    Unlock => "Unlock…", "Розблокувати…";
    PromptAbove => "The prompt opens in the island at the top", "Запит відкриється в острові вгорі";
    NoVaultTitle => "No vault yet", "Ще немає сховища";
    NoVaultBody => "Create a vault on this device or connect Bitwarden, in the cloud or on your own server.", "Створіть сховище на цьому пристрої або під'єднайте акаунт Bitwarden: у хмарі чи на власному сервері.";
    ConnectAccount => "Connect an account", "Під'єднати акаунт";
    NothingHere => "Nothing here yet", "Тут поки порожньо";
    NothingHereBody => "Items you add appear here.", "Тут з'являться записи, які ви додасте.";
    NoMatches => "Nothing found", "Нічого не знайдено";
    SelectItem => "Choose an item", "Виберіть запис";
    Restore => "Restore", "Відновити";
    DeleteForever => "Delete for good", "Видалити назавжди";
    DeleteForeverTitle => "Delete «{name}» for good?", "Видалити «{name}» назавжди?";
    DeleteForeverBody => "It cannot be restored afterwards.", "Після цього його не можна буде відновити.";
    InTrashSince => "In the Trash since {date}", "У кошику з {date}";
    MovedToTrash => "Moved to the Trash", "Переміщено в кошик";
    Saved => "Saved", "Збережено";
    NotAvailable => "Bitwarden accounts are not supported by this build yet", "Ця збірка ще не підтримує акаунти Bitwarden";
    WrongPassword => "That password does not open the vault", "Цей пароль не відкриває сховище";
    DaemonMissing => "The keychain is not running", "Сховище паролів не запущено";
    File => "File", "Файл";
    Edit => "Edit", "Правка";
    View => "View", "Вигляд";
    Settings => "Settings…", "Налаштування…";
    Quit => "Quit", "Вийти";
    SortRecent => "Recent First", "Спершу нещодавні";
    SortName => "By Name", "За назвою";
    Today => "today", "сьогодні";
    ItemsOne => "{n} item", "{n} запис";
    ItemsFew => "{n} items", "{n} записи";
    ItemsMany => "{n} items", "{n} записів";
    RemoveVault => "Remove vault", "Прибрати сховище";
    NewVaultPassword => "New password", "Новий пароль";
    Soon => "Not ready yet", "Ще не готово";
    Yesterday => "yesterday", "вчора";
}

#[must_use]
pub fn plural(lang: Lang, n: u64, [one, few, many]: [Key; 3]) -> String {
    let form = match lang.plural(n) {
        glide_sdk::i18n::Plural::One => one,
        glide_sdk::i18n::Plural::Few => few,
        glide_sdk::i18n::Plural::Many => many,
    };
    form.text(lang).replace("{n}", &n.to_string())
}

#[must_use]
pub fn date(lang: Lang, unix: u64, now: u64) -> String {
    const UK: [&str; 12] =
        ["січ.", "лют.", "бер.", "квіт.", "трав.", "черв.", "лип.", "серп.", "вер.", "жовт.", "лист.", "груд."];
    let day = |time: u64| time / 86_400;
    if day(unix) == day(now) {
        return Key::Today.text(lang).to_string();
    }
    if day(unix) + 1 == day(now) {
        return Key::Yesterday.text(lang).to_string();
    }
    let (year, month, date) = civil(unix);
    let (this_year, ..) = civil(now);
    let short: String = match lang {
        Lang::Uk => UK[month as usize - 1].to_string(),
        Lang::En => lang.month_of_date(month as usize - 1).chars().take(3).collect(),
    };
    let base = match lang {
        Lang::Uk => format!("{date} {short}"),
        Lang::En => format!("{short} {date}"),
    };
    if year == this_year { base } else { format!("{base} {year}") }
}

#[must_use]
pub const fn civil(unix: u64) -> (i64, u32, u32) {
    let days = (unix / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let date = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_short_and_local() {
        let october_12 = 1_791_763_200;
        let later = october_12 + 13 * 86_400 + 43_200;
        assert_eq!(date(Lang::Uk, october_12, later), "12 жовт.");
        assert_eq!(date(Lang::En, october_12, later), "Oct 12");
        assert_eq!(date(Lang::Uk, later - 60, later), "сьогодні");
        assert_eq!(date(Lang::Uk, later - 86_400, later), "вчора");
        assert!(date(Lang::Uk, 1_704_067_200, later).ends_with("2024"));
    }

    #[test]
    fn plurals_pick_the_ukrainian_form() {
        assert_eq!(plural(Lang::Uk, 1, [Key::CharsOne, Key::CharsFew, Key::CharsMany]), "1 символ");
        assert_eq!(plural(Lang::Uk, 3, [Key::CharsOne, Key::CharsFew, Key::CharsMany]), "3 символи");
        assert_eq!(plural(Lang::Uk, 18, [Key::CharsOne, Key::CharsFew, Key::CharsMany]), "18 символів");
        assert_eq!(plural(Lang::En, 1, [Key::CharsOne, Key::CharsFew, Key::CharsMany]), "1 character");
    }
}
