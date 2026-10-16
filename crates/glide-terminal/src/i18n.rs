#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "uk" | "ua" | "ukrainian" => Some(Self::Uk),
            "en" | "english" => Some(Self::En),
            _ => None,
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
            .unwrap_or_default();
        if locale.starts_with("uk") || locale.is_empty() { Self::Uk } else { Self::En }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    AppName,
    NewWindow,
    NewTab,
    SplitDown,
    SplitRight,
    ClosePane,
    Copy,
    Paste,
    SelectAll,
    Find,
    FindPlaceholder,
    NoMatches,
    Done,
    ClearScrollback,
    Reset,
    MarkLine,
    Bookmark,
    PreviousMark,
    NextMark,
    Profiles,
    MakeDefault,
    DefaultProfile,
    FontSize,
    FullScreen,
    OpenLink,
    CopyLink,
    ManPage,
    OpenHints,
    ExportText,
    ExportName,
    SavedTo,
    SaveFailed,
    ConfirmTitle,
    ConfirmBody,
    Terminate,
    Cancel,
    Completed,
    CompletedWithCode,
    SpawnFailed,
}

impl Key {
    #[must_use]
    pub const fn text(self, lang: Lang) -> &'static str {
        let (uk, en) = match self {
            Self::AppName => ("Термінал", "Terminal"),
            Self::NewWindow => ("Нове вікно", "New Window"),
            Self::NewTab => ("Нова вкладка", "New Tab"),
            Self::SplitDown => ("Розділити вниз", "Split Down"),
            Self::SplitRight => ("Розділити праворуч", "Split Right"),
            Self::ClosePane => ("Закрити панель", "Close Pane"),
            Self::Copy => ("Копіювати", "Copy"),
            Self::Paste => ("Вставити", "Paste"),
            Self::SelectAll => ("Вибрати все", "Select All"),
            Self::Find => ("Знайти…", "Find…"),
            Self::FindPlaceholder => ("Знайти в терміналі", "Find in terminal"),
            Self::NoMatches => ("Немає збігів", "No matches"),
            Self::Done => ("Готово", "Done"),
            Self::ClearScrollback => ("Очистити до початку", "Clear to Start"),
            Self::Reset => ("Скинути термінал", "Reset Terminal"),
            Self::MarkLine => ("Позначити рядок", "Mark Line"),
            Self::Bookmark => ("Додати закладку", "Mark as Bookmark"),
            Self::PreviousMark => ("До попередньої позначки", "Jump to Previous Mark"),
            Self::NextMark => ("До наступної позначки", "Jump to Next Mark"),
            Self::Profiles => ("Профілі…", "Profiles…"),
            Self::MakeDefault => ("Зробити типовим", "Use as Default"),
            Self::DefaultProfile => ("типовий", "default"),
            Self::FontSize => ("Розмір шрифту", "Font Size"),
            Self::FullScreen => ("На весь екран", "Full Screen"),
            Self::OpenLink => ("Відкрити посилання", "Open Link"),
            Self::CopyLink => ("Копіювати посилання", "Copy Link"),
            Self::ManPage => ("Відкрити сторінку man", "Open man Page"),
            Self::OpenHints => ("Відкрити посилання чи шлях…", "Open Link or Path…"),
            Self::ExportText => ("Експортувати текст як…", "Export Text As…"),
            Self::ExportName => ("Вивід терміналу.txt", "Terminal Saved Output.txt"),
            Self::SavedTo => ("Збережено в", "Saved to"),
            Self::SaveFailed => ("Не вдалося зберегти", "Couldn't save"),
            Self::ConfirmTitle => ("Завершити запущені процеси?", "Terminate running processes?"),
            Self::ConfirmBody => ("Закриття завершить", "Closing will terminate"),
            Self::Terminate => ("Завершити", "Terminate"),
            Self::Cancel => ("Скасувати", "Cancel"),
            Self::Completed => ("[Процес завершено]", "[Process completed]"),
            Self::CompletedWithCode => ("[Процес завершено з кодом", "[Process completed with code"),
            Self::SpawnFailed => ("Не вдалося запустити оболонку", "Couldn't start the shell"),
        };
        match lang {
            Lang::Uk => uk,
            Lang::En => en,
        }
    }
}
