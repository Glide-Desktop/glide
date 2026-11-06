pub use glide_sdk::Lang;

glide_sdk::catalogue! {
    AppName => "Folio", "Фоліо";
    Maximize => "Maximize", "Розгорнути";
    CloseWindow => "Close", "Закрити";
    Library => "Library", "Бібліотека";
    Recent => "Recent", "Нещодавні";
    Reading => "Reading Now", "Читаю зараз";
    WithNotes => "With Notes", "З нотатками";
    Favourites => "Favourites", "Улюблені";
    AwaitingSignature => "Awaiting Signature", "Чекають на підпис";
    Folders => "Folders", "Теки";
    WatchFolder => "Watch a Folder…", "Стежити за текою…";
    ChooseFolder => "Choose", "Вибрати";
    LibraryNote => "The library is your recent files and the folders you watch. Nothing is imported or copied.", "Бібліотека — це нещодавні файли системи й теки, за якими ви стежите. Нічого не імпортується і не копіюється.";
    SearchLibrary => "Name or text in a document", "Назва або текст у документі";
    ClearSearch => "Clear search", "Очистити пошук";
    OpenFile => "Open…", "Відкрити…";
    OpenTitle => "Open a document", "Відкрити документ";
    MoreActions => "More actions", "Інші дії";
    ContinueReading => "Continue Reading", "Продовжити читання";
    AllDocuments => "All Documents", "Усі документи";
    All => "All", "Усі";
    Pdf => "PDF", "PDF";
    Books => "Books", "Книжки";
    Comics => "Comics", "Комікси";
    Scans => "Scans", "Скани";
    PageShort => "p. {n}", "с. {n}";
    DropHint => "Open documents from Files or drop them into the window", "Відкривайте документи з Файлів або перетягніть їх у вікно";
    PageOf => "p. {n} of {total}", "с. {n} з {total}";
    Read => "Read", "Прочитано";
    NeverOpened => "Not opened yet", "Не відкривали";
    NeedsSignature => "Signature needed", "Потрібен підпис";
    FieldsFilled => "{n} of {total} fields filled", "Заповнено {n} з {total} полів";
    ScanNoText => "Scan · no text", "Скан · без тексту";
    DocumentsOne => "document", "документ";
    DocumentsFew => "documents", "документи";
    DocumentsMany => "documents", "документів";
    Today => "today", "сьогодні";
    Yesterday => "yesterday", "учора";
    EmptyLibrary => "No documents yet", "Документів поки немає";
    EmptyLibraryBody => "Open a PDF, book or comic, or watch a folder, and it will show up here.", "Відкрийте PDF, книжку чи комікс або стежте за текою — і вони з’являться тут.";
    NothingFound => "Nothing found", "Нічого не знайдено";
    NothingFoundBody => "No document name matches this search.", "Жодна назва документа не збігається з пошуком.";
    BackToLibrary => "Library", "Бібліотека";
    SidePanel => "Sidebar", "Бічна панель";
    Thumbnails => "Thumbnails", "Мініатюри";
    Outline => "Contents", "Зміст";
    Notes => "Notes", "Нотатки";
    Bookmarks => "Bookmarks", "Закладки";
    PageField => "Page", "Сторінка";
    Of => "of {total}", "з {total}";
    Find => "Find in document", "Знайти в документі";
    ReadingSettings => "Reading settings", "Параметри читання";
    Progress => "{n} % read", "{n} % прочитано";
    LayoutSingle => "Single page", "Одна сторінка";
    LayoutContinuous => "Continuous", "Суцільно";
    LayoutSpread => "Two pages", "Розворот";
    ZoomIn => "Zoom in", "Збільшити";
    ZoomOut => "Zoom out", "Зменшити";
    FitWidth => "Fit width", "По ширині";
    FitPage => "Whole page", "Уся сторінка";
    FitHeight => "Fit height", "По висоті";
    ActualSize => "Actual size", "Фактичний розмір";
    NoOutline => "This document has no contents", "Документ не має змісту";
    NoOutlineBody => "Use thumbnails or search to move around.", "Переходьте мініатюрами або пошуком.";
    NotesAll => "All · {n}", "Усі · {n}";
    NotesCommented => "With comment", "З коментарем";
    ExportNotes => "Export…", "Експортувати…";
    ExportMarkdown => "Export to Markdown…", "Експортувати в Markdown…";
    NoNotes => "No notes yet", "Нотаток поки немає";
    NoNotesBody => "Select text and highlight it, or leave a comment on the page.", "Виділіть текст кольором або залиште коментар на сторінці.";
    AddBookmark => "Add Bookmark", "Додати закладку";
    BookmarksNote => "Bookmarks are kept by Folio for this file.", "Закладки Фоліо зберігає для цього файлу.";
    BookmarkName => "Page {n}", "Сторінка {n}";
    RemoveBookmark => "Remove Bookmark", "Вилучити закладку";
    Matches => "{n} matches", "{n} збігів";
    MatchesOne => "{n} match", "{n} збіг";
    MatchesFew => "{n} matches", "{n} збіги";
    InSections => "in {n} sections", "у {n} розділах";
    Searching => "Searching…", "Шукаємо…";
    NoMatches => "No matches", "Збігів немає";
    MatchOf => "{n} of {total}", "{n} з {total}";
    Search => "Find", "Шукати";
    CaseSensitive => "Match case", "Враховувати регістр";
    WholeWord => "Whole word", "Ціле слово";
    PreviousMatch => "Previous match", "Попередній збіг";
    NextMatch => "Next match", "Наступний збіг";
    CloseFind => "Close search", "Закрити пошук";
    ScopeText => "Text", "Текст";
    ScopeNotes => "Notes", "Нотатки";
    FindHint => "Enter — next, Shift+Enter — previous", "Enter — наступний, Shift+Enter — попередній";
    FindClosed => "Search closed. Ctrl+F opens it again, F3 finds the next match.", "Пошук закрито. Ctrl+F відкриє його знову, F3 — наступний збіг.";
    Reopen => "Open", "Відкрити";
    Annotate => "Notes", "Нотатки";
    ToolSelect => "Select and text", "Вибір і текст";
    ToolHighlight => "Highlight", "Виділити";
    ToolUnderline => "Underline", "Підкреслити";
    ToolStrike => "Strike out", "Закреслити";
    ToolComment => "Comment", "Коментар";
    ToolInk => "Freehand", "Від руки";
    ToolText => "Text on the page", "Текст на сторінці";
    HintSelect => "Select text or click a note to change it", "Виділяйте текст або клацайте нотатку, щоб її змінити";
    HintHighlight => "Drag across text to highlight it", "Ведіть по тексту, щоб виділити його кольором";
    HintUnderline => "Drag across text to underline it", "Ведіть по тексту, щоб підкреслити";
    HintStrike => "Drag across text to strike it out", "Ведіть по тексту, щоб закреслити";
    HintComment => "Click on the page to leave a comment", "Клацніть на сторінці, щоб залишити коментар";
    HintInk => "Draw with a mouse, pen or finger", "Малюйте мишею, пером чи пальцем";
    HintText => "Click to add text on top of the page", "Клацніть, щоб додати напис поверх сторінки";
    NotesStored => "Notes are written into the file as standard PDF annotations", "Нотатки пишуться у файл як звичайні PDF-анотації";
    NotesInCopy => "This format keeps notes only in a PDF copy", "Цей формат зберігає нотатки лише в копії PDF";
    Done => "Done", "Готово";
    Undo => "Undo", "Скасувати";
    Redo => "Redo", "Повторити";
    Save => "Save", "Зберегти";
    Changed => "Changed · {n} new notes", "Змінено · нових нотаток: {n}";
    Yellow => "Yellow", "Жовтий";
    Green => "Green", "Зелений";
    Blue => "Blue", "Синій";
    Red => "Red", "Червоний";
    Purple => "Purple", "Фіолетовий";
    Colour => "Colour", "Колір";
    You => "You", "Ви";
    JustNow => "just now", "щойно";
    New => "new", "нова";
    Edit => "Edit", "Змінити";
    Remove => "Remove", "Вилучити";
    CommentPlaceholder => "Comment…", "Коментар…";
    Copy => "Copy", "Копіювати";
    Copied => "Copied", "Скопійовано";
    HighlightAndComment => "Highlight and comment", "Виділити й коментувати";
    FindSelection => "Find “{text}” in the document", "Знайти «{text}» у документі";
    SearchWeb => "Search the web", "Шукати в інтернеті";
    BookmarkPage => "Add bookmark on p. {n}", "Додати закладку на с. {n}";
    CopyPageLink => "Copy link to page", "Копіювати посилання на сторінку";
    ViewHeading => "View", "Вигляд";
    DocumentHeading => "Document", "Документ";
    PageColour => "Page", "Сторінка";
    PaperWhite => "Paper", "Папір";
    PaperSepia => "Sepia", "Сепія";
    PaperNight => "Night", "Ніч";
    RotateLeft => "Rotate left", "Повернути ліворуч";
    RotateRight => "Rotate right", "Повернути праворуч";
    Presentation => "Presentation", "Презентація";
    Fullscreen => "Full screen", "На весь екран";
    Print => "Print…", "Друкувати…";
    SaveCopy => "Save a Copy…", "Зберегти копію…";
    Organise => "Organise Pages…", "Упорядкувати сторінки…";
    FillSign => "Fill and Sign…", "Заповнити й підписати…";
    Recognise => "Recognise Text", "Розпізнати текст";
    HasText => "has text already", "текст уже є";
    Properties => "Properties", "Властивості";
    ShowInFiles => "Show in Files", "Показати у Файлах";
    OpenWith => "Open With Another App", "Відкрити в іншій програмі";
    Shortcuts => "Keyboard Shortcuts", "Гарячі клавіші";
    KeepImages => "Don’t invert images", "Не інвертувати зображення";
    KeepImagesHint => "Photos and diagrams stay as they are", "Фото й схеми лишаються як є";
    LayoutHeading => "Layout", "Розкладка";
    One => "Single", "Одна";
    Continuous => "Continuous", "Суцільно";
    Spread => "Two pages", "Розворот";
    CoverAlone => "Cover on its own", "Обкладинка окремо";
    CropMargins => "Crop margins", "Обрізати поля";
    CropMarginsHint => "The page takes more space", "Сторінка займає більше місця";
    RightToLeft => "Right to left", "Справа наліво";
    RememberedHere => "Remembered for this file. Defaults live in Preferences.", "Запам’ятовується для цього файлу. Типові значення — у Параметрах.";
    LinkHint => "Click to go there. Back — Alt+←", "Клацніть, щоб перейти. Назад — Alt+←";
    PageNumber => "Page {n}", "Сторінка {n}";
    ExternalLink => "Opens in the browser. The document opens nothing by itself: it needs your click.", "Відкриється у браузері. Документ сам нічого не відкриває: потрібне ваше клацання.";
    CopyAddress => "Copy address", "Копіювати адресу";
    BackTo => "Back to p. {n}", "Назад до с. {n}";
    Back => "Back", "Назад";
    Forward => "Forward", "Уперед";
    ToReading => "Back to reading", "До читання";
    OrganiseTitle => "Organise Pages", "Упорядкувати сторінки";
    PagesInCopy => "{name} · pages in the copy: {n}", "{name} · сторінок у копії: {n}";
    InsertFromFile => "Insert from File…", "Вставити з файлу…";
    SelectedPages => "{n} selected", "Вибрано: {n}";
    Left => "Left", "Ліворуч";
    Right => "Right", "Праворуч";
    Extract => "Extract to a New File…", "Витягти в новий файл…";
    Delete => "Remove", "Вилучити";
    ClearSelection => "Clear selection", "Зняти вибір";
    Removed => "removed", "вилучено";
    FromOtherFile => "from another file", "з іншого файлу";
    OrganiseHint => "Click to select pages. Shift — range, Ctrl — several.", "Клацайте, щоб вибрати сторінки. Shift — діапазон, Ctrl — окремі.";
    OriginalKept => "The original stays as it is: the result is a new file beside it", "Оригінал не змінюється: результат — новий файл поруч";
    SavedCopy => "Copy saved: {name}", "Копію збережено: {name}";
    SaveFailed => "Couldn’t save the file", "Не вдалося зберегти файл";
    EncryptedSave => "A password-protected file can’t be changed in place", "Файл із паролем не можна змінити на місці";
    Fields => "Fields", "Поля";
    Signatures => "Signatures", "Підписи";
    Required => "required", "обов’язкове";
    ClearForm => "Clear Form", "Очистити форму";
    FormEmpty => "Form · {n} required fields empty", "Форма · порожніх обов’язкових полів: {n}";
    FormReady => "Form · all required fields filled", "Форма · обов’язкові поля заповнено";
    SignedIntact => "Digitally signed. The document has not changed since it was signed.", "Підписано цифровим підписом. Документ не змінювався після підписання.";
    SignedChanged => "Digitally signed, but the document changed after signing.", "Підписано цифровим підписом, але документ змінено після підписання.";
    CertificateNotChecked => "The certificate has not been checked.", "Сертифікат не перевірено.";
    AwaitingYou => "Waiting for your signature", "Чекає на ваш підпис";
    SignHere => "Click to sign", "Клацніть, щоб підписати";
    YourSignature => "Your signature", "Ваш підпис";
    Draw => "Draw", "Намалювати";
    Type => "Type", "Ввести";
    Picture => "Picture", "Зображення";
    Erase => "Erase", "Стерти";
    Cancel => "Cancel", "Скасувати";
    PlaceSignature => "Place Signature", "Поставити підпис";
    NotDigital => "A drawn signature is a picture, not a digital signature", "Намальований підпис — зображення, не цифровий підпис";
    TabHint => "Tab — next field, Shift+Tab — previous", "Tab — наступне поле, Shift+Tab — попереднє";
    TypedName => "Your name", "Ваше ім’я";
    PreviousSlide => "Previous slide", "Попередній слайд";
    NextSlide => "Next slide", "Наступний слайд";
    PauseTimer => "Pause timer", "Пауза таймера";
    ResumeTimer => "Resume timer", "Продовжити таймер";
    Exit => "Exit", "Вийти";
    Next => "Next", "Далі";
    ChromeHides => "The bar hides after 2 s without movement", "Панель ховається через 2 с без руху";
    EscPanels => "Esc — show the panels again", "Esc — повернути панелі";
    Locked => "This document is protected by a password", "Документ захищено паролем";
    LockedBody => "Its author encrypted it. Enter the password to open it; without it the content can’t be read.", "Його зашифрував автор. Введіть пароль, щоб відкрити; без нього вміст не прочитати.";
    Password => "Document password", "Пароль документа";
    ShowPassword => "Show password", "Показати пароль";
    WrongPassword => "The password doesn’t match. Check the keyboard layout.", "Пароль не підходить. Перевірте розкладку клавіатури.";
    Open => "Open", "Відкрити";
    LockedSub => "Password protected", "Захищено паролем";
    LockedFoot => "The password is checked only on this computer", "Пароль перевіряється лише на цьому комп’ютері";
    Damaged => "The file is damaged", "Файл пошкоджено";
    DamagedBody => "Read {n} of {total} pages. The download may not have finished.", "Прочитано {n} з {total} сторінок. Можливо, завантаження не завершилось.";
    DamagedFull => "Folio couldn’t read this file. It may be incomplete or not a document at all.", "Фоліо не змогло прочитати цей файл. Можливо, він неповний або це взагалі не документ.";
    PageBroken => "Page {n} couldn’t be read", "Сторінку {n} не вдалося прочитати";
    Missing => "{package} is needed to open this file", "Щоб відкрити цей файл, потрібен {package}";
    MissingBody => "Install it and open the file again. Folio never sends documents anywhere to convert them.", "Встановіть його й відкрийте файл знову. Фоліо ніколи не надсилає документи для перетворення.";
    Unsupported => "Folio can’t open this kind of file", "Фоліо не відкриває такі файли";
    NoText => "There is no text in this scan", "У скані немає тексту";
    NoTextBody => "Search, selection and copying don’t work. Folio can recognise {n} pages — it takes a few minutes.", "Пошук, виділення і копіювання не працюють. Фоліо може розпізнати {n} сторінок — це займе кілька хвилин.";
    NotNow => "Not now", "Не зараз";
    Recognising => "Recognising text…", "Розпізнавання тексту…";
    KeepReading => "You can keep reading. Finished pages are searchable already.", "Можна читати далі. Готові сторінки вже шукаються.";
    Stop => "Stop", "Зупинити";
    Recognised => "Text recognised. The original is unchanged.", "Текст розпізнано. Оригінал не змінено.";
    NoTesseract => "Text recognition needs tesseract", "Для розпізнавання потрібен tesseract";
    Languages => "Ukrainian, English", "Українська, англійська";
    General => "General", "Загальне";
    Fonts => "Fonts", "Шрифти";
    Security => "Security", "Безпека";
    Title => "Title", "Назва";
    Author => "Author", "Автор";
    File => "File", "Файл";
    Pages => "Pages", "Сторінки";
    Size => "Size", "Обсяг";
    Created => "Created", "Створено";
    Modified => "Modified", "Змінено";
    Producer => "Application", "Програма";
    Version => "PDF version", "Версія PDF";
    Encryption => "Encryption", "Шифрування";
    Printing => "Printing", "Друк";
    Copying => "Copying", "Копіювання";
    JavaScript => "JavaScript", "JavaScript";
    Attachments => "Attachments", "Вкладення";
    None => "none", "немає";
    Allowed => "allowed", "дозволено";
    Forbidden => "not allowed", "заборонено";
    Present => "present", "є";
    JsOff => "none · always off in Folio", "немає · у Фоліо завжди вимкнено";
    JsFound => "present · always off in Folio", "є · у Фоліо завжди вимкнено";
    AttachmentsSaveAs => "{n} · opened only through “Save As”", "{n} · відкриваються лише через «Зберегти як»";
    Embedded => "embedded subset · {kind}", "вбудовано підмножину · {kind}";
    NotEmbedded => "not embedded · {kind}", "не вбудовано · {kind}";
    AllEmbedded => "Every font is embedded: the document looks the same on any computer.", "Усі шрифти вбудовано: документ виглядатиме однаково на будь-якому комп’ютері.";
    SomeMissing => "Some fonts aren’t embedded: the document may look different elsewhere.", "Деякі шрифти не вбудовано: деінде документ може виглядати інакше.";
    PropertiesFoot => "Alt+Enter — properties", "Alt+Enter — властивості";
    ReadOnlyCopy => "read only", "лише читання";
    OfficeCopy => "LibreOffice made a copy for viewing. The original is unchanged.", "LibreOffice зробив копію для перегляду. Оригінал не змінено.";
    EditInLibreOffice => "Edit in LibreOffice", "Редагувати в LibreOffice";
    PrintFailed => "Printing isn’t available: no print portal answered", "Друк недоступний: портал друку не відповів";
    MinutesLeft => "about {n} min left", "близько {n} хв до кінця";
    FileMenu => "File", "Файл";
    EditMenu => "Edit", "Правка";
    ViewMenu => "View", "Вигляд";
    GoMenu => "Go", "Перейти";
    NotesMenu => "Notes", "Нотатки";
    HelpMenu => "Help", "Довідка";
    Close => "Close", "Закрити";
    Quit => "Quit", "Вийти";
    SelectAll => "Select All", "Вибрати все";
    FirstPage => "First Page", "Перша сторінка";
    LastPage => "Last Page", "Остання сторінка";
    PreviousPage => "Previous Page", "Попередня сторінка";
    NextPage => "Next Page", "Наступна сторінка";
    UnsavedTitle => "Save notes in “{name}”?", "Зберегти нотатки у «{name}»?";
    UnsavedBody => "Notes you added will be lost if you close without saving.", "Додані нотатки зникнуть, якщо закрити без збереження.";
    DontSave => "Don’t Save", "Не зберігати";
    Loading => "Opening…", "Відкриваємо…";
}

#[must_use]
pub fn plural(lang: Lang, n: usize, one: Key, few: Key, many: Key) -> String {
    let word = match lang {
        Lang::En => {
            if n == 1 {
                one
            } else {
                many
            }
        }
        Lang::Uk => {
            let (ten, hundred) = (n % 10, n % 100);
            if ten == 1 && hundred != 11 {
                one
            } else if (2..=4).contains(&ten) && !(12..=14).contains(&hundred) {
                few
            } else {
                many
            }
        }
    };
    format!("{n} {}", word.text(lang))
}

#[must_use]
pub fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    pairs.iter().fold(template.to_string(), |text, (key, value)| text.replace(&format!("{{{key}}}"), value))
}

#[must_use]
pub fn matches(lang: Lang, n: usize) -> String {
    let key = match lang {
        Lang::En => {
            if n == 1 {
                Key::MatchesOne
            } else {
                Key::Matches
            }
        }
        Lang::Uk => {
            let (ten, hundred) = (n % 10, n % 100);
            if ten == 1 && hundred != 11 {
                Key::MatchesOne
            } else if (2..=4).contains(&ten) && !(12..=14).contains(&hundred) {
                Key::MatchesFew
            } else {
                Key::Matches
            }
        }
    };
    key.text(lang).replace("{n}", &n.to_string())
}

#[must_use]
pub fn bytes(lang: Lang, n: u64) -> String {
    let (unit, value) = if n >= 1 << 30 {
        (if lang == Lang::Uk { "ГБ" } else { "GB" }, n as f64 / f64::from(1u32 << 30))
    } else if n >= 1 << 20 {
        (if lang == Lang::Uk { "МБ" } else { "MB" }, n as f64 / f64::from(1u32 << 20))
    } else {
        (if lang == Lang::Uk { "КБ" } else { "KB" }, n as f64 / 1024.0)
    };
    let text = format!("{value:.1}");
    let text = if lang == Lang::Uk { text.replace('.', ",") } else { text };
    format!("{text} {unit}")
}

const MONTHS_UK: [&str; 12] = [
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
const MONTHS_EN: [&str; 12] = [
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

#[must_use]
pub fn date(lang: Lang, iso: &str, with_time: bool) -> String {
    let part = |range: std::ops::Range<usize>| iso.get(range).and_then(|s| s.parse::<u32>().ok());
    let (Some(year), Some(month), Some(day)) = (part(0..4), part(5..7), part(8..10)) else {
        return String::new();
    };
    let month_index = (month.clamp(1, 12) - 1) as usize;
    let time = match (with_time, part(11..13), part(14..16)) {
        (true, Some(h), Some(m)) => format!(", {h:02}:{m:02}"),
        _ => String::new(),
    };
    match lang {
        Lang::Uk => format!("{day} {} {year}{time}", MONTHS_UK[month_index]),
        Lang::En => format!("{day} {} {year}{time}", MONTHS_EN[month_index]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ukrainian_plurals_follow_the_last_digits() {
        assert_eq!(plural(Lang::Uk, 1, Key::DocumentsOne, Key::DocumentsFew, Key::DocumentsMany), "1 документ");
        assert_eq!(plural(Lang::Uk, 3, Key::DocumentsOne, Key::DocumentsFew, Key::DocumentsMany), "3 документи");
        assert_eq!(plural(Lang::Uk, 11, Key::DocumentsOne, Key::DocumentsFew, Key::DocumentsMany), "11 документів");
        assert_eq!(matches(Lang::Uk, 12), "12 збігів");
        assert_eq!(matches(Lang::Uk, 22), "22 збіги");
    }

    #[test]
    fn dates_read_in_both_languages() {
        assert_eq!(date(Lang::Uk, "2026-11-06T10:42", true), "6 листопада 2026, 10:42");
        assert_eq!(date(Lang::En, "2026-11-06T10:42", false), "6 November 2026");
        assert_eq!(bytes(Lang::Uk, 3_984_588), "3,8 МБ");
    }

    #[test]
    fn every_key_has_both_languages() {
        for key in Key::ALL {
            assert!(!key.en().is_empty() && !key.uk().is_empty(), "{key:?}");
        }
    }
}
