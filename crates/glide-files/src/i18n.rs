use std::io::ErrorKind;

use crate::fs::DiskError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let lower = key.trim().to_lowercase();
        if lower.starts_with("uk") {
            Some(Self::Uk)
        } else if lower.starts_with("en") {
            Some(Self::En)
        } else {
            None
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok())
            .find_map(|value| Self::from_key(&value))
            .unwrap_or_default()
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

            #[must_use]
            pub const fn en(self) -> &'static str {
                match self {
                    $(Self::$key => $en,)*
                }
            }

            #[must_use]
            pub const fn uk(self) -> &'static str {
                match self {
                    $(Self::$key => $uk,)*
                }
            }

            #[must_use]
            pub const fn text(self, lang: Lang) -> &'static str {
                match lang {
                    Lang::Uk => self.uk(),
                    Lang::En => self.en(),
                }
            }
        }
    };
}

catalogue! {
    AppName => "Files", "Файли";
    QuitTitle => "Stop operations and quit?", "Зупинити операції й закрити?";
    QuitPrompt => "Some operations are still running. Stopped now, they stay unfinished.", "Деякі операції ще тривають. Якщо зупинити їх зараз, вони залишаться незавершеними.";
    QuitConfirm => "Stop and Quit", "Зупинити й закрити";
    Home => "Home", "Домівка";
    Documents => "Documents", "Документи";
    Downloads => "Downloads", "Завантаження";
    Pictures => "Pictures", "Зображення";
    Music => "Music", "Музика";
    Videos => "Videos", "Відео";
    Desktop => "Desktop", "Стільниця";
    Trash => "Trash", "Смітник";
    Root => "File System", "Файлова система";
    Places => "Places", "Місця";
    Bookmarks => "Bookmarks", "Закладки";
    Devices => "Devices", "Пристрої";
    ColName => "Name", "Назва";
    ColSize => "Size", "Розмір";
    ColModified => "Modified", "Змінено";
    ColType => "Kind", "Тип";
    ColLocation => "Location", "Розташування";
    Search => "Search", "Пошук";
    Back => "Back", "Назад";
    Forward => "Forward", "Уперед";
    MoreActions => "More", "Інші дії";
    Maximize => "Maximize", "Розгорнути";
    ClearSearch => "Clear", "Очистити";
    PauseJob => "Pause", "Призупинити";
    ResumeJob => "Resume", "Продовжити";
    ViewGrid => "Icons", "Значки";
    ViewList => "List", "Список";
    Open => "Open", "Відкрити";
    ShowInFolder => "Show in Folder", "Показати в папці";
    OpenInTerminal => "Open in Terminal", "Відкрити в терміналі";
    NewFolder => "New Folder", "Нова папка";
    NewFile => "New File", "Новий файл";
    UntitledFile => "Untitled", "Без назви";
    Rename => "Rename…", "Перейменувати…";
    Copy => "Copy", "Копіювати";
    Cut => "Cut", "Вирізати";
    Paste => "Paste", "Вставити";
    MoveToTrash => "Move to Trash", "Перемістити до смітника";
    DeletePermanently => "Delete Permanently", "Видалити назавжди";
    Restore => "Restore", "Відновити";
    EmptyTrash => "Empty Trash", "Очистити смітник";
    Properties => "Properties", "Властивості";
    SelectAll => "Select All", "Вибрати все";
    CopyPath => "Copy Path", "Копіювати шлях";
    AddBookmark => "Add to Bookmarks", "Додати до закладок";
    RemoveBookmark => "Remove from Bookmarks", "Прибрати із закладок";
    ViewAs => "View", "Вигляд";
    Reload => "Reload", "Оновити";
    SortBy => "Sort by", "Упорядкувати";
    Reversed => "Reverse Order", "У зворотному порядку";
    ShowHidden => "Show Hidden Files", "Приховані файли";
    PathPlaceholder => "Go to folder", "Перейти до папки";
    Create => "Create", "Створити";
    Cancel => "Cancel", "Скасувати";
    Close => "Close", "Закрити";
    Undo => "Undo", "Скасувати";
    NewFolderTitle => "New Folder", "Нова папка";
    NewFileTitle => "New File", "Новий файл";
    RenameTitle => "Rename", "Перейменування";
    RenameConfirm => "Rename", "Перейменувати";
    NamePrompt => "Name", "Назва";
    DeleteTitle => "Delete permanently?", "Видалити назавжди?";
    DeletePrompt => "These items will be erased and can’t be restored.", "Ці об’єкти буде стерто без можливості відновлення.";
    DeletePromptOne => "This item will be erased and can’t be restored.", "Цей об’єкт буде стерто без можливості відновлення.";
    EmptyTrashTitle => "Empty the Trash?", "Очистити смітник?";
    EmptyTrashPrompt => "All items in the Trash will be erased and can’t be restored.", "Усі об’єкти в смітнику буде стерто без можливості відновлення.";
    TrashBanner => "Items in the Trash can be restored or erased.", "Об’єкти в смітнику можна відновити або стерти.";
    Location => "Location", "Розташування";
    OriginalLocation => "Original location", "Початкове розташування";
    Size => "Size", "Розмір";
    Contents => "Contents", "Вміст";
    Permissions => "Permissions", "Права доступу";
    CreatedDate => "Created", "Створено";
    ModifiedDate => "Modified", "Змінено";
    SymlinkTarget => "Points to", "Вказує на";
    Today => "Today", "Сьогодні";
    Yesterday => "Yesterday", "Учора";
    Free => "free", "вільно";
    Searching => "Searching…", "Пошук…";
    MovedToTrash => "Moved to Trash", "Переміщено до смітника";
    OpenWith => "Open With", "Відкрити за допомогою";
    TotalSize => "Total size", "Загальний розмір";
    UnlockTitle => "Unlock encrypted drive", "Розблокувати зашифрований диск";
    UnlockPrompt => "Enter the passphrase for this drive", "Введіть пароль до цього диска";
    Unlock => "Unlock", "Розблокувати";
    Calculating => "Calculating…", "Обчислення…";
    Owner => "Owner", "Власник";
    Group => "Group", "Група";
    Others => "Others", "Інші";
    AlwaysOpenWith => "Always Open With", "Завжди відкривати в";
    Copied => "Copied", "Скопійовано";
    Moved => "Moved", "Переміщено";
    Deleted => "Deleted", "Видалено";
    OperationCancelled => "Operation cancelled", "Операцію скасовано";
    Copying => "Copying", "Копіювання";
    Moving => "Moving", "Переміщення";
    Deleting => "Deleting", "Видалення";
    StopJob => "Stop", "Зупинити";
    Trashing => "Moving to Trash", "Переміщення до смітника";
    Preparing => "Preparing…", "Підготовка…";
    Paused => "Paused", "Призупинено";
    TimeLeft => "left", "залишилось";
    OfTotal => "of", "з";
    DropCopy => "Copy here", "Копіювати сюди";
    DropMove => "Move here", "Перемістити сюди";
    Duplicate => "Duplicate", "Дублювати";
    CreateLink => "Create Link", "Створити посилання";
    LinkTo => "Link to", "Посилання на";
    NewDocument => "New Document", "Новий документ";
    NoTemplates => "No templates yet", "Шаблонів поки немає";
    OpenTemplates => "Open Templates Folder", "Відкрити папку шаблонів";
    NewDocumentTitle => "New Document", "Новий документ";
    Tags => "Tags", "Мітки";
    EditTags => "Edit Tags…", "Змінити мітки…";
    TagsTitle => "Tags", "Мітки";
    TagsPrompt => "Words separated by commas", "Слова через кому";
    Save => "Save", "Зберегти";
    TagRed => "Red", "Червоний";
    TagOrange => "Orange", "Помаранчевий";
    TagYellow => "Yellow", "Жовтий";
    TagGreen => "Green", "Зелений";
    TagBlue => "Blue", "Синій";
    TagPurple => "Purple", "Фіолетовий";
    TagGray => "Gray", "Сірий";
    TagsUnsupported => "This file system can’t store tags", "Ця файлова система не зберігає мітки";
    PastedImage => "Pasted image", "Вставлене зображення";
    PastedText => "Pasted text", "Вставлений текст";
    Columns => "Columns", "Колонки";
    ColCreated => "Created", "Створено";
    ColPermissions => "Permissions", "Права";
    ColTags => "Tags", "Мітки";
    ZoomIn => "Zoom In", "Збільшити";
    ZoomOut => "Zoom Out", "Зменшити";
    ZoomReset => "Normal Size", "Звичайний розмір";
    QueueOperations => "One Operation at a Time", "Операції по черзі";
    Queued => "Queued", "У черзі";
    Phone => "Phone", "Телефон";
    Camera => "Camera", "Камера";
    PhoneHint => "Unlock the phone and allow file transfer", "Розблокуйте телефон і дозвольте передавання файлів";
    NoMtpTool => "Install gvfs-mtp, jmtpfs or simple-mtpfs to open phones", "Встановіть gvfs-mtp, jmtpfs або simple-mtpfs, щоб відкривати телефони";
    CantOpenPhone => "Can’t open the device", "Не вдалося відкрити пристрій";
    FileExists => "already exists here", "вже є в цій папці";
    FolderExists => "A folder with this name already exists here", "Папка з такою назвою вже є тут";
    FileExistsHint => "Replace it, keep both, or skip this one?", "Замінити, залишити обидва чи пропустити?";
    FolderExistsHint => "Merge the contents, keep both, or skip this one?", "Об’єднати вміст, залишити обидві чи пропустити?";
    Replace => "Replace", "Замінити";
    Merge => "Merge", "Об’єднати";
    Skip => "Skip", "Пропустити";
    KeepBoth => "Keep both", "Залишити обидва";
    ApplyToAll => "Do this for every conflict", "Так для всіх конфліктів";
    Restored => "Restored", "Відновлено";
    PathNotFound => "Folder not found", "Папку не знайдено";
    CantOpen => "Can’t open", "Не вдалося відкрити";
    NoTerminal => "No terminal emulator found", "Не знайдено емулятора термінала";
    CantRun => "Can’t run", "Не вдалося запустити";
    Compress => "Compress…", "Стиснути…";
    ExtractHere => "Extract Here", "Розпакувати сюди";
    ExtractTo => "Extract To…", "Розпакувати до…";
    ErrExists => "An item with this name already exists", "Об’єкт із такою назвою вже існує";
    ErrName => "This name can’t be used", "Цю назву використати не можна";
    ErrIntoItself => "A folder can’t be placed inside itself", "Папку не можна вкласти саму в себе";
    ErrPermission => "Permission denied", "Недостатньо прав доступу";
    ErrNotFound => "The item no longer exists", "Об’єкта більше не існує";
    ErrNoSpace => "Not enough disk space", "Недостатньо місця на диску";
    ErrReadOnly => "The file system is read-only", "Файлова система доступна лише для читання";
    ErrNotInTrash => "Only top-level Trash items can be restored", "Відновити можна лише об’єкти верхнього рівня смітника";
    ErrUnsupported => "Pipes, sockets and devices can’t be copied", "Канали, сокети й пристрої не копіюються";
    CatFolder => "Folder", "Папка";
    CatDocument => "Document", "Документ";
    CatText => "Plain Text", "Текстовий файл";
    CatCode => "Source Code", "Вихідний код";
    CatImage => "Image", "Зображення";
    CatAudio => "Audio", "Аудіо";
    CatVideo => "Video", "Відео";
    CatArchive => "Archive", "Архів";
    CatPdf => "PDF Document", "Документ PDF";
    CatBinary => "Program", "Програма";
    CatUnknown => "File", "Файл";
    EmptyFolder => "Folder is empty", "Папка порожня";
    NoSearchResults => "No results", "Нічого не знайдено";
    UnitB => "B", "Б";
    UnitKb => "kB", "кБ";
    UnitMb => "MB", "МБ";
    UnitGb => "GB", "ГБ";
    UnitTb => "TB", "ТБ";
    VolumeFallbackName => "{} Volume", "Том {}";
    MountReadOnly => "Mount Read-Only", "Змонтувати лише для читання";
    Unmount => "Unmount", "Відмонтувати";
    Eject => "Safely Remove", "Безпечно вилучити";
    Repair => "Repair…", "Виправити…";
    RepairAction => "Repair", "Виправити";
    RepairTitle => "Repair the file system?", "Виправити файлову систему?";
    RepairPrompt => "The device will be unmounted, checked and mounted again. Don’t unplug it until the repair finishes.", "Пристрій буде відмонтовано, перевірено й змонтовано знову. Не від’єднуйте його, доки виправлення не завершиться.";
    MountedFallback => "wasn’t unplugged safely — mounted with a compatible driver", "не було безпечно вилучено — змонтовано сумісним драйвером";
    MountedReadOnly => "has file system errors — mounted read-only", "має помилки файлової системи — змонтовано лише для читання";
    Repaired => "repaired", "виправлено";
    SafeToRemove => "can be unplugged now", "тепер можна від’єднати";
    CantMount => "Can’t mount", "Не вдалося змонтувати";
    CantUnmount => "Can’t unmount", "Не вдалося відмонтувати";
    CantEject => "Can’t remove", "Не вдалося вилучити";
    CantRepair => "Can’t repair", "Не вдалося виправити";
    ErrBusy => "the device is in use", "пристрій використовується";
    ErrNotAuthorized => "not authorized", "немає дозволу";
    ErrNoDisks => "the disk service (udisks2) is unavailable", "служба дисків (udisks2) недоступна";
    ErrMissingTool => "this tool isn’t installed:", "не встановлено програму:";
    ErrUnrepaired => "the file system couldn’t be fully repaired", "файлову систему не вдалося повністю виправити";
}

const MONTHS_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTHS_UK: [&str; 12] =
    ["січ.", "лют.", "бер.", "квіт.", "трав.", "черв.", "лип.", "серп.", "вер.", "жовт.", "лист.", "груд."];

#[must_use]
pub fn month(month: u8, lang: Lang) -> &'static str {
    let table = match lang {
        Lang::Uk => &MONTHS_UK,
        Lang::En => &MONTHS_EN,
    };
    table[usize::from(month.clamp(1, 12) - 1)]
}

#[must_use]
pub fn items(count: usize, lang: Lang) -> String {
    let noun = match lang {
        Lang::En if count == 1 => "item",
        Lang::En => "items",
        Lang::Uk => match (count % 10, count % 100) {
            (1, n) if n != 11 => "об’єкт",
            (2..=4, n) if !(12..=14).contains(&n) => "об’єкти",
            _ => "об’єктів",
        },
    };
    format!("{count} {noun}")
}

#[must_use]
pub fn duration(span: std::time::Duration, lang: Lang) -> String {
    let secs = span.as_secs().max(1);
    let (hours, minutes, seconds) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let (h, m, s) = match lang {
        Lang::Uk => ("год", "хв", "с"),
        Lang::En => ("h", "min", "s"),
    };
    match (hours, minutes) {
        (0, 0) => format!("{seconds} {s}"),
        (0, _) => format!("{minutes} {m}"),
        _ => format!("{hours} {h} {minutes} {m}"),
    }
}

#[must_use]
pub fn selected(count: usize, total: usize, lang: Lang) -> String {
    match lang {
        Lang::Uk => format!("Вибрано {count} з {total}"),
        Lang::En => format!("{count} of {total} selected"),
    }
}

#[must_use]
pub fn disk_error(err: &DiskError, lang: Lang) -> String {
    let key = match err {
        DiskError::Unavailable => Key::ErrNoDisks,
        DiskError::NotAuthorized | DiskError::Cancelled => Key::ErrNotAuthorized,
        DiskError::Busy => Key::ErrBusy,
        DiskError::Unrepaired => Key::ErrUnrepaired,
        DiskError::MissingTool(tool) => {
            return format!("{} {tool}", Key::ErrMissingTool.text(lang));
        }
        DiskError::Failed(detail) => return detail.clone(),
    };
    key.text(lang).to_string()
}

#[must_use]
pub fn io_error(err: &std::io::Error, lang: Lang) -> String {
    let key = match err.kind() {
        ErrorKind::AlreadyExists => Key::ErrExists,
        ErrorKind::InvalidFilename => Key::ErrName,
        ErrorKind::InvalidInput => Key::ErrIntoItself,
        ErrorKind::PermissionDenied => Key::ErrPermission,
        ErrorKind::NotFound => Key::ErrNotFound,
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Key::ErrNoSpace,
        ErrorKind::ReadOnlyFilesystem => Key::ErrReadOnly,
        ErrorKind::Unsupported => Key::ErrUnsupported,
        _ => return err.to_string(),
    };
    key.text(lang).to_string()
}
