pub use glide_sdk::Lang;

use crate::library::Date;

glide_sdk::catalogue! {
    AppName => "Photos", "Фото";
    Maximize => "Maximize", "Розгорнути";
    CloseWindow => "Close", "Закрити";
    Photos => "Photos", "Фото";
    Library => "Library", "Бібліотека";
    Favourites => "Favourites", "Улюблені";
    Albums => "Albums", "Альбоми";
    Folders => "Folders", "Теки";
    Devices => "Devices", "Пристрої";
    AddFolder => "Add Folder…", "Додати теку…";
    Trash => "Trash", "Смітник";
    Eject => "Eject", "Вийняти";
    Years => "Years", "Роки";
    Months => "Months", "Місяці";
    AllPhotos => "All Photos", "Усі фото";
    TimeScale => "Time scale", "Масштаб часу";
    SearchPhotos => "Search photos", "Пошук у фото";
    SearchAlbums => "Search albums", "Пошук альбомів";
    ClearSearch => "Clear search", "Очистити пошук";
    Select => "Select", "Вибрати";
    Done => "Done", "Готово";
    SelectAll => "Select All", "Вибрати всі";
    SelectDay => "Select day", "Вибрати день";
    DeselectDay => "Deselect", "Зняти вибір";
    MoreActions => "More actions", "Інші дії";
    ZoomIn => "Zoom in", "Збільшити";
    ZoomOut => "Zoom out", "Зменшити";
    Today => "Today", "Сьогодні";
    Yesterday => "Yesterday", "Вчора";
    PhotosOne => "photo", "фото";
    PhotosFew => "photos", "фото";
    PhotosMany => "photos", "фото";
    MonthsOne => "month", "місяць";
    MonthsFew => "months", "місяці";
    MonthsMany => "months", "місяців";
    YearsOne => "year", "рік";
    YearsFew => "years", "роки";
    YearsMany => "years", "років";
    AlbumsOne => "album", "альбом";
    AlbumsFew => "albums", "альбоми";
    AlbumsMany => "albums", "альбомів";
    Smart => "smart", "розумних";
    CardNew => "{n} new photos on {card}", "На картці {card} — {n} нових фото";
    CopyToLibrary => "Copy to Library", "Скопіювати в бібліотеку";
    Hide => "Hide", "Сховати";
    Copying => "Copying photos from the card…", "Копіювання фото з картки…";
    Copied => "{n} photos copied", "Скопійовано фото: {n}";
    Indexing => "Reading the library…", "Читання бібліотеки…";
    SelectedPhotos => "{n} selected", "Вибрано {n}";
    SelectedOf => "{n} of {total} selected", "Вибрано {n} з {total}";
    SelectionHint => "Shift — range, Ctrl — separate", "Shift — діапазон, Ctrl — окремі";
    Share => "Share", "Поділитися";
    ToAlbum => "To Album", "В альбом";
    Delete => "Delete", "Видалити";
    Open => "Open", "Відкрити";
    AddToAlbum => "Add to Album…", "Додати в альбом…";
    MarkFavourite => "Mark as Favourite", "Позначити улюбленими";
    UnmarkFavourite => "Remove from Favourites", "Прибрати з улюблених";
    RotateLeft => "Rotate Left", "Повернути ліворуч";
    RotateRight => "Rotate Right", "Повернути праворуч";
    Export => "Export…", "Експортувати…";
    ShowInFiles => "Show in Files", "Показати у Файлах";
    MoveToTrash => "Move to Trash", "Перемістити до смітника";
    NewAlbum => "New Album", "Новий альбом";
    NewAlbumDots => "New Album…", "Новий альбом…";
    AlbumName => "Album name", "Назва альбому";
    Create => "Create", "Створити";
    Cancel => "Cancel", "Скасувати";
    MyAlbums => "My Albums", "Мої альбоми";
    SmartAlbums => "Smart Albums", "Розумні альбоми";
    UpdateThemselves => "Update themselves", "Оновлюються самі";
    Recent => "Recently Added", "Нещодавно додані";
    RecentHint => "In the last 30 days", "За останні 30 днів";
    Raw => "RAW", "RAW" as written;
    RawHint => "CR3, NEF, ARW, DNG", "CR3, NEF, ARW, DNG" as written;
    Animated => "Animated", "Анімовані";
    AnimatedHint => "GIF, APNG, WebP", "GIF, APNG, WebP";
    Vector => "Vector", "Векторні";
    VectorHint => "SVG, SVGZ", "SVG, SVGZ" as written;
    Screenshots => "Screenshots", "Знімки екрана";
    ScreenshotsHint => "From Glide and other apps", "З Glide та програм";
    Hdr => "HDR", "HDR" as written;
    HdrHint => "OpenEXR, Radiance HDR", "OpenEXR, Radiance HDR";
    Panoramas => "Panoramas", "Панорами";
    PanoramasHint => "Wide and 360°", "Широкі й 360°";
    Paged => "Multi-page", "Багатосторінкові";
    PagedHint => "TIFF", "TIFF" as written;
    Everything => "Everything", "Усе";
    ThisYear => "This year", "Цей рік";
    Place => "folder", "тека";
    Album => "album", "альбом";
    Matches => "{n} matches in names, folders and captions", "{n} збігів у назвах, теках і підписах";
    NothingFound => "Nothing found", "Нічого не знайдено";
    NothingFoundBody => "Nothing matches “{q}” here.", "За запитом «{q}» тут немає збігів.";
    SearchEverywhere => "Search all photos", "Шукати в усіх фото";
    EmptyTitle => "Your photos will appear here", "Тут з'являться ваші фото";
    EmptyBody => "Photos shows the pictures in folders on disk without copying anything. Start with Pictures or choose your own folder.", "Фото показує зображення з тек на диску, нічого не копіюючи. Почніть із «Зображень» або виберіть свою теку.";
    UsePictures => "Use Pictures", "Використати «Зображення»";
    OtherFolder => "Other Folder…", "Інша тека…";
    EmptyAlbum => "This album is empty", "Цей альбом порожній";
    EmptyAlbumBody => "Select photos in the library and choose To Album.", "Виберіть фото в бібліотеці й натисніть «В альбом».";
    EmptyTrash => "The Trash has no photos", "У смітнику немає фото";
    EmptyTrashBody => "Deleted photos stay here until the Trash is emptied.", "Видалені фото лежать тут, доки смітник не очистять.";
    Restore => "Restore", "Відновити";
    EmptyTrashAction => "Empty Trash…", "Очистити смітник…";
    EmptyTrashAsk => "Delete the photos in the Trash forever?", "Видалити фото зі смітника назавжди?";
    EmptyTrashNote => "This cannot be undone.", "Цю дію не можна скасувати.";
    DeleteForever => "Delete", "Видалити";
    Back => "Library", "Бібліотека";
    Favourite => "Favourite", "Улюблене";
    Info => "Info", "Інформація";
    Edit => "Edit", "Редагувати";
    Previous => "Previous", "Попереднє";
    Next => "Next", "Наступне";
    Fit => "Fit", "Вписати";
    Fullscreen => "Fullscreen", "На весь екран";
    Slideshow => "Slideshow", "Слайд-шоу";
    StopSlideshow => "Stop Slideshow", "Зупинити слайд-шоу";
    Copy => "Copy", "Копіювати";
    OfTotal => "{n} of {total}", "{n} з {total}";
    AddCaption => "Add a caption…", "Додати підпис…";
    Caption => "Caption", "Підпис";
    SizeLabel => "Size", "Розмір";
    BytesLabel => "File", "Обсяг";
    ColourLabel => "Colour", "Колір";
    FolderLabel => "Folder", "Тека";
    Megapixels => "Mp", "Мп";
    Embedded => "embedded preview", "вбудоване прев'ю";
    ToneMapped => "tone-mapped", "тональне відображення";
    Converted => "through a system decoder", "через системний декодер";
    Pages => "{n} pages", "{n} с.";
    PageOf => "Page {n} of {total}", "Сторінка {n} з {total}";
    PreviousPage => "Previous page", "Попередня сторінка";
    NextPage => "Next page", "Наступна сторінка";
    Pause => "Pause", "Пауза";
    Play => "Play", "Грати";
    NextFrame => "Next frame", "Наступний кадр";
    ExposureView => "View exposure", "Експозиція перегляду";
    Checker => "Checks", "Клітинки";
    White => "White", "Біле";
    Dark => "Dark", "Темне";
    CannotOpen => "This format could not be opened", "Цей формат не вдалося відкрити";
    OpenElsewhere => "Open in Another App", "Відкрити в іншій програмі";
    Unsupported => "{f} is not supported yet", "{f} поки не підтримується";
    Loading => "Opening…", "Відкриття…";
    Adjust => "Adjust", "Налаштувати";
    Filters => "Filters", "Фільтри";
    Crop => "Crop", "Обрізати";
    Tools => "Tools", "Інструменти";
    Undo => "Undo", "Скасувати дію";
    Redo => "Redo", "Повторити дію";
    Auto => "Auto", "Автоматично";
    ResetAll => "Reset All", "Скинути все";
    Reset => "Reset", "Скинути";
    Light => "Light", "Світло";
    Colour => "Colour", "Колір";
    Effects => "Effects", "Ефекти";
    Exposure => "Exposure", "Експозиція";
    Contrast => "Contrast", "Контраст";
    Shadows => "Shadows", "Тіні";
    Saturation => "Saturation", "Насиченість";
    Warmth => "Warmth", "Теплота";
    Vignette => "Vignette", "Віньєтка";
    FilterNone => "None", "Без фільтра";
    FilterVivid => "Vivid", "Яскравий";
    FilterWarm => "Warm", "Теплий";
    FilterCool => "Cool", "Холодний";
    FilterMono => "Mono", "Ч/Б";
    FilterNoir => "Noir", "Нуар";
    FilterFilm => "Film", "Плівка";
    FilterDrama => "Drama", "Драма";
    FilterFade => "Faded", "Вицвілий";
    Strength => "Filter strength", "Сила фільтра";
    FilterNote => "The filter goes over the adjustments and keeps them.", "Фільтр накладається поверх налаштувань і не стирає їх.";
    Compare => "Hold to compare", "Утримуйте, щоб порівняти";
    Or => "or", "або";
    Space => "Space", "Пробіл";
    Original => "Original", "Оригінал";
    Ratios => "Aspect ratio", "Пропорції";
    Landscape => "Landscape", "Альбомна";
    Portrait => "Portrait", "Книжкова";
    Free => "Free", "Вільно";
    RotateFlip => "Rotate and flip", "Поворот і відображення";
    FlipH => "Flip horizontally", "Віддзеркалити по горизонталі";
    FlipV => "Flip vertically", "Віддзеркалити по вертикалі";
    Level => "Level the horizon", "Вирівняти за горизонтом";
    Tilt => "Tilt", "Нахил";
    ResetCrop => "Reset crop", "Скинути обрізання";
    CropNote => "The crop is applied to every frame of the copy.", "Обрізання застосовується до копії, оригінал не змінюється.";
    SaveTitle => "Save changes?", "Зберегти зміни?";
    SaveCopyBody => "The original stays as it is and the changes go into a copy.", "Оригінал лишиться як був, а зміни підуть у копію.";
    SaveReplaceBody => "Choose whether to keep the original.", "Виберіть, чи лишати оригінал.";
    SaveCopy => "Save a copy beside it", "Зберегти копію поруч";
    SaveCopyHint => "Same folder · added to the same albums", "Та сама тека · додається в ті ж альбоми";
    ReplaceOriginal => "Replace the original", "Замінити оригінал";
    ReplaceHint => "The original file is overwritten", "Файл оригіналу буде перезаписано";
    ReplaceUnavailable => "Not available for {f}", "Недоступно для {f}";
    CopyName => "Copy name", "Назва копії";
    CopyFormat => "Copy format", "Формат копії";
    Discard => "Don't Save", "Не зберігати";
    SaveCopyAction => "Save Copy", "Зберегти копію";
    ReplaceAction => "Replace", "Замінити";
    EditedSuffix => "edited", "змінено";
    SavedCopy => "Copy saved", "Копію збережено";
    Saved => "Changes saved", "Зміни збережено";
    Show => "Show", "Показати";
    SaveFailed => "Could not save: {e}", "Не вдалося зберегти: {e}";
    Trashed => "{n} moved to the Trash", "До смітника переміщено: {n}";
    TrashFailed => "Could not move to the Trash", "Не вдалося перемістити до смітника";
    Restored => "Restored", "Відновлено";
    AddedToAlbum => "Added to “{a}”", "Додано в «{a}»";
    Rotated => "Rotated", "Повернуто";
    RotateFailed => "Some photos can't be rotated in place", "Деякі фото не можна повернути на місці";
    ExportTitle => "Export to Folder", "Експортувати в теку";
    Exported => "{n} exported", "Експортовано: {n}";
    ChooseFolder => "Choose", "Вибрати";
    FileMenu => "File", "Файл";
    EditMenu => "Edit", "Правка";
    ViewMenu => "View", "Вигляд";
    ImageMenu => "Image", "Зображення";
    InfoPanel => "Info Panel", "Панель інформації";
    Quit => "Quit", "Вийти";
    RemoveFolder => "Remove from Library", "Прибрати з бібліотеки";
    DeleteAlbum => "Delete Album", "Видалити альбом";
    RemoveFromAlbum => "Remove from Album", "Прибрати з альбому";
    Jan => "January", "січня";
    Feb => "February", "лютого";
    Mar => "March", "березня";
    Apr => "April", "квітня";
    May => "May", "травня";
    Jun => "June", "червня";
    Jul => "July", "липня";
    Aug => "August", "серпня";
    Sep => "September", "вересня";
    Oct => "October", "жовтня";
    Nov => "November", "листопада";
    Dec => "December", "грудня";
    JanN => "January", "Січень";
    FebN => "February", "Лютий";
    MarN => "March", "Березень";
    AprN => "April", "Квітень";
    MayN => "May", "Травень";
    JunN => "June", "Червень";
    JulN => "July", "Липень";
    AugN => "August", "Серпень";
    SepN => "September", "Вересень";
    OctN => "October", "Жовтень";
    NovN => "November", "Листопад";
    DecN => "December", "Грудень";
}

const GENITIVE: [Key; 12] = [
    Key::Jan,
    Key::Feb,
    Key::Mar,
    Key::Apr,
    Key::May,
    Key::Jun,
    Key::Jul,
    Key::Aug,
    Key::Sep,
    Key::Oct,
    Key::Nov,
    Key::Dec,
];
const NOMINATIVE: [Key; 12] = [
    Key::JanN,
    Key::FebN,
    Key::MarN,
    Key::AprN,
    Key::MayN,
    Key::JunN,
    Key::JulN,
    Key::AugN,
    Key::SepN,
    Key::OctN,
    Key::NovN,
    Key::DecN,
];

#[must_use]
pub fn month(lang: Lang, month: u32) -> &'static str {
    NOMINATIVE[(month.clamp(1, 12) - 1) as usize].text(lang)
}

#[must_use]
pub fn date(lang: Lang, date: Date, today: Date) -> String {
    let gap = crate::library::days_between(date, today);
    if gap == 0 {
        return Key::Today.text(lang).into();
    }
    if gap == 1 {
        return Key::Yesterday.text(lang).into();
    }
    let name = GENITIVE[(date.month.clamp(1, 12) - 1) as usize].text(lang);
    let day = match lang {
        Lang::Uk => format!("{} {name}", date.day),
        Lang::En => format!("{name} {}", date.day),
    };
    if date.year == today.year { day } else { format!("{day} {}", date.year) }
}

#[must_use]
pub fn stamp(lang: Lang, date: Date, minute: u32, today: Date) -> String {
    let day = if crate::library::days_between(date, today) <= 1 {
        date_full(lang, date)
    } else {
        let text = self::date(lang, date, today);
        if date.year == today.year { format!("{text} {}", date.year) } else { text }
    };
    format!("{day}, {:02}:{:02}", minute / 60, minute % 60)
}

fn date_full(lang: Lang, date: Date) -> String {
    let name = GENITIVE[(date.month.clamp(1, 12) - 1) as usize].text(lang);
    match lang {
        Lang::Uk => format!("{} {name} {}", date.day, date.year),
        Lang::En => format!("{name} {}, {}", date.day, date.year),
    }
}

#[must_use]
pub fn count(lang: Lang, n: usize, forms: [Key; 3]) -> String {
    glide_sdk::i18n::count(n as u64, lang, forms.map(|key| key.text(lang)))
}

#[must_use]
pub fn photos(lang: Lang, n: usize) -> String {
    count(lang, n, [Key::PhotosOne, Key::PhotosFew, Key::PhotosMany])
}

#[must_use]
pub fn number(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    out
}

#[must_use]
pub fn bytes(lang: Lang, bytes: u64) -> String {
    let (value, unit) = match bytes {
        0..1024 => (bytes as f64, lang.pick("Б", "B")),
        1024..1_048_576 => (bytes as f64 / 1024.0, lang.pick("КБ", "KB")),
        1_048_576..1_073_741_824 => (bytes as f64 / 1_048_576.0, lang.pick("МБ", "MB")),
        _ => (bytes as f64 / 1_073_741_824.0, lang.pick("ГБ", "GB")),
    };
    let text =
        if value >= 100.0 || unit == "Б" || unit == "B" { format!("{value:.0}") } else { format!("{value:.1}") };
    let text = if lang == Lang::Uk { text.replace('.', ",") } else { text };
    format!("{text} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_read_as_today_yesterday_or_a_dated_name() {
        let today = Date { year: 2026, month: 11, day: 5 };
        assert_eq!(date(Lang::Uk, today, today), "Сьогодні");
        assert_eq!(date(Lang::Uk, Date { day: 4, ..today }, today), "Вчора");
        assert_eq!(date(Lang::Uk, Date { day: 2, ..today }, today), "2 листопада");
        assert_eq!(date(Lang::En, Date { year: 2025, month: 7, day: 9 }, today), "July 9 2025");
        assert_eq!(month(Lang::Uk, 11), "Листопад");
    }

    #[test]
    fn numbers_and_sizes_use_narrow_groups_and_decimal_commas() {
        assert_eq!(number(12_486), "12\u{a0}486");
        assert_eq!(number(312), "312");
        assert_eq!(bytes(Lang::Uk, 3_565_158), "3,4 МБ");
        assert_eq!(bytes(Lang::En, 512), "512 B");
        assert_eq!(photos(Lang::Uk, 41), "41 фото");
    }
}
