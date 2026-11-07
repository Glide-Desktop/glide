pub use glide_sdk::Lang;

glide_sdk::catalogue! {
    AppName => "Clips", "Clips";
    Maximize => "Maximize", "Розгорнути";
    CloseWindow => "Close", "Закрити";
    LeaveFullscreen => "Leave full screen", "Вийти з повноекранного режиму";
    PictureInPicture => "Picture in picture", "Картинка в картинці";
    OpenVideo => "Open a video", "Відкрийте відео";
    OpenHint => "Drag a file into this window or onto the Clips icon in the dock. Clips plays MP4, MKV, WebM, MOV, AVI and dozens of other formats, as well as sound and streams by address.", "Перетягніть файл у це вікно чи на значок Clips у доку. Clips відтворює MP4, MKV, WebM, MOV, AVI і десятки інших форматів, а також звук і потоки за адресою.";
    OpenFile => "Open File…", "Відкрити файл…";
    OpenAddress => "Open Address…", "Відкрити адресу…";
    ContinueWatching => "Continue watching", "Продовжити перегляд";
    MemoryNote => "Clips remembers the position, tracks and subtitles of every file", "Clips пам'ятає позицію, доріжки й субтитри кожного файлу";
    PhotosNote => "Videos from the Photos library open here too", "Відео з бібліотеки Фото відкриваються тут само";
    ForgetHistory => "Forget History", "Забути історію";
    LeftMinutes => "{n} min left", "Залишилось {n} хв";
    LeftSeconds => "{n} s left", "Залишилось {n} с";
    Today => "today", "сьогодні";
    Yesterday => "yesterday", "вчора";
    DropToPlay => "Drop to play", "Відпустіть, щоб відтворити";
    DropAspect => "The window will take the shape of this video", "Вікно розгорнеться в пропорціях цього відео";
    ChooseVideo => "Open a video", "Відкрити відео";
    Choose => "Open", "Відкрити";
    AddressTitle => "Open an address", "Відкрити адресу";
    AddressBody => "A link to a file or a stream: HTTP, HLS (.m3u8), DASH (.mpd) or RTSP.", "Посилання на файл чи потік: HTTP, HLS (.m3u8), DASH (.mpd) або RTSP.";
    AddressPlaceholder => "https://…", "https://…";
    Cancel => "Cancel", "Скасувати";
    Play => "Play", "Відтворити";
    Pause => "Pause", "Пауза";
    Back10 => "Back 10 seconds", "Назад на 10 секунд";
    Forward10 => "Forward 10 seconds", "Уперед на 10 секунд";
    Mute => "Mute", "Вимкнути звук";
    Unmute => "Unmute", "Увімкнути звук";
    VolumeLabel => "Volume", "Гучність";
    SpeedLabel => "Speed", "Швидкість";
    KeepPitch => "Keep the pitch", "Зберігати висоту звуку";
    SubtitlesSound => "Subtitles and sound", "Субтитри й звук";
    MoreActions => "More actions", "Інші дії";
    FullScreen => "Full screen", "На весь екран";
    Position => "Position", "Позиція";
    RemainingToggle => "Switch between time left and length", "Перемкнути: залишилось чи тривалість";
    ChapterLine => "Chapter {n} · {title}", "Розділ {n} · {title}";
    ChapterNumber => "Chapter {n}", "Розділ {n}";
    Subtitles => "Subtitles", "Субтитри";
    Sound => "Sound", "Звук";
    Off => "Off", "Вимкнено";
    Embedded => "embedded", "вбудовані";
    NextToFile => "next to the file · picked up automatically", "поруч із файлом · підхоплено самі";
    AddFromFile => "Add from File…", "Додати з файлу…";
    Shift => "Delay", "Зсув";
    Seconds => "{n} s", "{n} с";
    Track => "Track {n}", "Доріжка {n}";
    Now => "now", "зараз";
    AspectMenu => "Aspect ratio", "Пропорції";
    AsInFile => "As in the file", "Як у файлі";
    FillWindow => "Fill the window", "Заповнити вікно";
    CropsEdges => "crops the edges", "обрізає краї";
    Cinema => "2.39 : 1 without bars", "2,39 : 1 без смуг";
    View => "View", "Вигляд";
    OnTop => "On top of other windows", "Поверх інших вікон";
    Repeat => "Repeat", "Повторювати";
    Editing => "Editing", "Редагування";
    TrimEllipsis => "Trim…", "Обрізати…";
    SplitClip => "Split Clip", "Розділити кліп";
    RotateLeft => "Rotate Left", "Повернути ліворуч";
    RotateRight => "Rotate Right", "Повернути праворуч";
    FlipHorizontal => "Flip", "Віддзеркалити";
    FlipVertical => "Flip Vertically", "Віддзеркалити вертикально";
    ExportCopy => "Export a Copy…", "Експортувати копію…";
    Inspector => "Inspector", "Інспектор";
    ShowInFiles => "Show in Files", "Показати у Файлах";
    Shortcuts => "Keyboard Shortcuts", "Гарячі клавіші";
    FrameBack => "Frame Back", "Кадр назад";
    CopyFrame => "Copy Frame", "Копіювати кадр";
    SaveFrame => "Save Frame to Photos", "Зберегти кадр у Фото";
    TextOnFrame => "Text on Frame", "Текст на кадрі";
    CopyTime => "Copy Time {time}", "Копіювати час {time}";
    OpenWith => "Open in Another App", "Відкрити в іншій програмі";
    Space => "Space", "Пробіл";
    Trimming => "Trim", "Обрізання";
    WillRemain => "{n} of {total} will remain", "залишиться {n} з {total}";
    Reset => "Reset", "Скинути";
    HintMarks => "set start and end at the playhead", "початок і кінець у курсорі";
    HintFrame => "frame", "кадр";
    HintTrim => "trim", "обрізати";
    HintCancel => "cancel", "скасувати";
    PlaySelection => "Play the selection", "Відтворити вибране";
    TrimButton => "Trim", "Обрізати";
    StartAt => "Start: {time}", "Початок: {time}";
    EndAt => "End: {time}", "Кінець: {time}";
    ClipsTitle => "Clips", "Кліпи";
    ClipLabel => "Clip {n}", "Кліп {n}";
    Turned => "turned {n}° clockwise", "повернуто на {n}° праворуч";
    Flipped => "flipped", "віддзеркалено";
    ClipsHint => "Drag a clip to move it · Delete removes it · Ctrl+Y splits at the playhead", "Перетягніть кліп, щоб переставити · Delete — вилучити · Ctrl+Y — розділити в курсорі";
    Split => "Split", "Розділити";
    AddClip => "Add Clip at the End…", "Додати кліп у кінець…";
    RemoveClip => "Remove clip", "Вилучити кліп";
    ExportTitle => "Export a copy", "Експортувати копію";
    ExportBody => "The original «{name}» stays as it is.", "Оригінал «{name}» лишиться як є.";
    TrimmedTo => "Trimmed to {time}.", "Обрізано до {time}.";
    ClipsTurned => "Some clips are turned.", "Деякі кліпи повернуто.";
    Name => "Name", "Назва";
    Where => "Where", "Куди";
    Videos => "Videos", "Відео";
    Quality => "Quality", "Якість";
    AudioOnly => "Sound only", "Лише звук";
    Codec => "Codec", "Кодек";
    H264 => "H.264 · opens everywhere", "H.264 · відкриється всюди";
    Hevc => "HEVC · smaller", "HEVC · менший";
    Av1 => "AV1 · smallest", "AV1 · найменший";
    EncoderNote => "The graphics card encodes through VA-API; without it the processor does, slower.", "Кодує відеокарта через VA-API; без неї — процесор, довше.";
    Lossless => "Without re-encoding, where possible", "Без перекодування, коли можна";
    LosslessNote => "Instant and lossless; the edges move to the nearest keyframes", "Миттєво і без втрат; межі стануть на найближчі ключові кадри";
    LosslessUnavailable => "Not possible when clips are turned differently, flipped or come from different files", "Неможливо, коли кліпи повернуто по-різному, віддзеркалено чи взято з різних файлів";
    VideosFolder => "Videos", "Відео";
    OtherFolder => "Other Folder…", "Інша папка…";
    SpanSeconds => "{n} s", "{n} с";
    SpanMinutes => "{n} min", "{n} хв";
    SpanHours => "{h} h {m} min", "{h} год {m} хв";
    KeepHdr => "Keep HDR", "Зберегти HDR";
    KeepHdrNote => "HEVC and AV1 only; H.264 turns the colours into SDR", "Лише для HEVC і AV1; у H.264 кольори перетворяться на SDR";
    Estimate => "About {size} · about {time}", "Приблизно {size} · близько {time}";
    ExportButton => "Export", "Експортувати";
    Exporting => "Exporting «{name}»", "Експорт «{name}»";
    ExportProgress => "{n} % · {left} left · {who}", "{n} % · залишилось {left} · {who}";
    ByCard => "the graphics card encodes", "кодує відеокарта";
    ByProcessor => "the processor encodes", "кодує процесор";
    DockProgress => "progress is on the dock icon too", "прогрес також на значку в доку";
    Saved => "The copy is saved in «{folder}»", "Копію збережено в «{folder}»";
    OpenSaved => "Open", "Відкрити";
    ExportFailed => "The copy could not be made", "Копію не вдалося зробити";
    Trimmed => "trimmed", "обрізано";
    InspectorTitle => "Inspector", "Інспектор";
    File => "File", "Файл";
    Size => "Size", "Розмір";
    Video => "Video", "Відео";
    Colour => "Colour", "Колір";
    Bitrate => "Bitrate", "Бітрейт";
    Decoder => "Decoder", "Декодер";
    Dropped => "Dropped", "Відкинуто";
    Frames => "{n} frames", "{n} кадрів";
    Hardware => "hardware", "апаратно";
    Software => "software", "програмно";
    CopyAll => "Copy All", "Скопіювати все";
    CloseInspector => "Close the inspector", "Закрити інспектор";
    Podcast => "audio", "звук";
    Recognised => "{n} lines recognised · tesseract, on this computer", "Розпізнано рядків: {n} · tesseract, локально";
    NothingRecognised => "No text on this frame", "На кадрі немає тексту";
    Recognising => "Looking for text…", "Шукаємо текст…";
    NoTesseract => "Text recognition needs tesseract", "Для розпізнавання тексту потрібен tesseract";
    Copy => "Copy", "Копіювати";
    SearchWeb => "Search the Web", "Шукати в інтернеті";
    Done => "Done", "Готово";
    MissingTitle => "No decoder for {what}", "Немає декодера {what}";
    MissingSome => "A part of this file needs a plugin that is not installed", "Частина цього файлу потребує модуля, який не встановлено";
    MissingBody => "The sound will play, but the picture will not show until the package with the decoder is installed.", "Звук гратиме, а відео не покажеться, доки не встановлено пакет із декодером.";
    SoundOnly => "Play Sound Only", "Відтворити лише звук";
    Details => "Details", "Детальніше";
    CopyCommand => "Copy Command", "Скопіювати команду";
    Live => "Live", "Наживо";
    Buffering => "Buffering · {n} %", "Буферизація · {n} %";
    Slow => "The network is slow: quality lowered", "Мережа повільна: якість знижено";
    ToLive => "Back to Live", "До прямого ефіру";
    BrokenTitle => "The rest of the file cannot be read", "Далі файл не читається";
    BrokenBody => "After {time} the data is damaged, most likely the download broke off. The video plays up to that point.", "Після {time} дані пошкоджені, найімовірніше, завантаження обірвалось. До цього місця відео грає.";
    BrokenStart => "This file cannot be read", "Цей файл не читається";
    BrokenStartBody => "It is damaged or it is not a video.", "Він пошкоджений або це не відео.";
    FromStart => "From the Start", "З початку";
    NotFoundTitle => "The file is gone", "Файлу більше немає";
    NotFoundBody => "It was moved or deleted after it was opened.", "Його перемістили або видалили.";
    NetworkTitle => "The address does not answer", "Адреса не відповідає";
    NetworkBody => "Check the link and the network connection.", "Перевірте посилання і з'єднання з мережею.";
    TryAgain => "Try Again", "Спробувати ще";
    OtherTitle => "The video cannot be played", "Відео не вдається відтворити";
    CopiedFrame => "The frame is copied", "Кадр скопійовано";
    SavedFrame => "The frame is saved in Pictures", "Кадр збережено в «Зображеннях»";
    CopiedTime => "The time is copied", "Час скопійовано";
    CopiedText => "The text is copied", "Текст скопійовано";
    CopiedInfo => "The details are copied", "Відомості скопійовано";
    PipUnavailable => "Glide cannot keep a window on top yet", "Glide поки не вміє тримати вікно поверх інших";
    ShortcutsTitle => "Keyboard shortcuts", "Гарячі клавіші";
    KeyPlay => "Play or pause", "Відтворити / пауза";
    KeyShuttle => "Back / forward, faster on each press", "Назад / уперед, щоразу швидше";
    KeySkip => "5 s, one frame on pause; Shift — 30 s", "5 с, на паузі — кадр; Shift — 30 с";
    KeyVolume => "Volume; M mutes", "Гучність; M — без звуку";
    KeyFullscreen => "Full screen", "На весь екран";
    KeyTracks => "Subtitles on or off; Shift+C, A — next track", "Субтитри; Shift+C, A — наступна доріжка";
    KeySpeed => "Speed one step; Backspace — 1×", "Швидкість на крок; Backspace — 1×";
    KeyChapters => "Previous / next chapter", "Попередній / наступний розділ";
    KeyEdit => "Trim, split, export a copy", "Обрізати, розділити, експортувати";
    KeyInspector => "Inspector, picture in picture", "Інспектор, картинка в картинці";
    KeyOpen => "Open a file, an address", "Відкрити файл, адресу";
    Close => "Close", "Закрити";
    DropUnsupported => "This is not a video or a sound", "Це не відео і не звук";
}

#[must_use]
pub fn when(lang: Lang, opened: u64, now: u64) -> String {
    let day = |seconds: u64| seconds / 86_400;
    let ago = day(now).saturating_sub(day(opened));
    match ago {
        0 => Key::Today.text(lang).to_string(),
        1 => Key::Yesterday.text(lang).to_string(),
        _ => {
            let (_, month, day) = civil(opened / 86_400);
            format!("{day} {}", lang.month_of_date(month as usize - 1))
        }
    }
}

#[must_use]
pub fn civil(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[must_use]
pub fn counted(lang: Lang, n: usize, en: [&str; 2], uk: [&str; 3]) -> String {
    let word = match lang {
        Lang::En => {
            if n == 1 {
                en[0]
            } else {
                en[1]
            }
        }
        Lang::Uk => {
            let (tens, ones) = (n % 100, n % 10);
            if ones == 1 && tens != 11 {
                uk[0]
            } else if (2..=4).contains(&ones) && !(12..=14).contains(&tens) {
                uk[1]
            } else {
                uk[2]
            }
        }
    };
    format!("{n} {word}")
}

#[must_use]
pub fn span(lang: Lang, span: std::time::Duration) -> String {
    let seconds = span.as_secs();
    if seconds < 60 {
        return Key::SpanSeconds.text(lang).replace("{n}", &seconds.max(1).to_string());
    }
    let minutes = seconds.div_ceil(60);
    if minutes < 60 {
        Key::SpanMinutes.text(lang).replace("{n}", &minutes.to_string())
    } else {
        Key::SpanHours
            .text(lang)
            .replace("{h}", &(minutes / 60).to_string())
            .replace("{m}", &(minutes % 60).to_string())
    }
}

#[must_use]
pub fn chapters(lang: Lang, n: usize) -> String {
    counted(lang, n, ["chapter", "chapters"], ["розділ", "розділи", "розділів"])
}

#[must_use]
pub fn clips(lang: Lang, n: usize) -> String {
    counted(lang, n, ["clip", "clips"], ["кліп", "кліпи", "кліпів"])
}

#[must_use]
pub fn left(lang: Lang, left: std::time::Duration) -> String {
    let minutes = left.as_secs() / 60;
    if minutes == 0 {
        Key::LeftSeconds.text(lang).replace("{n}", &left.as_secs().to_string())
    } else {
        Key::LeftMinutes.text(lang).replace("{n}", &minutes.to_string())
    }
}

#[must_use]
pub fn decimal(lang: Lang, value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    let text = if digits > 0 { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text };
    if lang == Lang::Uk { text.replace('.', ",") } else { text }
}

#[must_use]
pub fn speed(lang: Lang, rate: f64) -> String {
    format!("{}×", decimal(lang, rate, 2))
}

#[must_use]
pub fn bytes(lang: Lang, bytes: u64) -> String {
    let units: &[(&str, &str)] = &[("B", "Б"), ("KB", "КБ"), ("MB", "МБ"), ("GB", "ГБ"), ("TB", "ТБ")];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < units.len() {
        value /= 1000.0;
        unit += 1;
    }
    let digits = usize::from((value < 10.0 && unit > 0) || (unit >= 3 && value < 100.0));
    let name = if lang == Lang::Uk { units[unit].1 } else { units[unit].0 };
    format!("{}\u{a0}{name}", decimal(lang, value, digits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_follow_the_language() {
        assert_eq!(speed(Lang::Uk, 1.25), "1,25×");
        assert_eq!(speed(Lang::En, 0.75), "0.75×");
        assert_eq!(speed(Lang::Uk, 2.0), "2×");
        assert_eq!(decimal(Lang::Uk, -0.4, 1), "-0,4");
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(bytes(Lang::Uk, 14_200_000_000), "14,2\u{a0}ГБ");
        assert_eq!(bytes(Lang::En, 360_000_000), "360\u{a0}MB");
        assert_eq!(bytes(Lang::En, 512), "512\u{a0}B");
    }

    #[test]
    fn time_left_counts_whole_minutes_like_a_clock() {
        assert_eq!(left(Lang::Uk, std::time::Duration::from_secs(2866)), "Залишилось 47 хв");
        assert_eq!(left(Lang::En, std::time::Duration::from_secs(20)), "20 s left");
    }

    #[test]
    fn counts_take_the_ukrainian_plural() {
        assert_eq!(chapters(Lang::Uk, 1), "1 розділ");
        assert_eq!(chapters(Lang::Uk, 3), "3 розділи");
        assert_eq!(chapters(Lang::Uk, 5), "5 розділів");
        assert_eq!(chapters(Lang::Uk, 12), "12 розділів");
        assert_eq!(chapters(Lang::Uk, 22), "22 розділи");
        assert_eq!(clips(Lang::Uk, 4), "4 кліпи");
        assert_eq!(clips(Lang::En, 1), "1 clip");
        assert_eq!(clips(Lang::En, 4), "4 clips");
    }

    #[test]
    fn spans_read_in_minutes_rounded_up() {
        assert_eq!(span(Lang::Uk, std::time::Duration::from_secs(5)), "5 с");
        assert_eq!(span(Lang::Uk, std::time::Duration::from_secs(61)), "2 хв");
        assert_eq!(span(Lang::Uk, std::time::Duration::from_secs(800)), "14 хв");
        assert_eq!(span(Lang::En, std::time::Duration::from_mins(65)), "1 h 5 min");
    }

    #[test]
    fn days_since_the_epoch_become_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_764), (2026, 11, 7));
        assert_eq!(when(Lang::Uk, 20_760 * 86_400, 20_764 * 86_400), "3 листопада");
    }

    #[test]
    fn recent_days_have_names() {
        assert_eq!(when(Lang::Uk, 86_400 * 10 + 5, 86_400 * 10 + 60), "сьогодні");
        assert_eq!(when(Lang::En, 86_400 * 9, 86_400 * 10 + 60), "yesterday");
    }
}
