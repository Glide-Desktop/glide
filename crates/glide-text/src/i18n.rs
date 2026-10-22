pub use glide_sdk::Lang;

macro_rules! keys {
    ($($name:ident => ($uk:expr, $en:expr),)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Key {
            $($name,)*
        }

        impl Key {
            #[must_use]
            pub const fn text(self, lang: Lang) -> &'static str {
                match (self, lang) {
                    $((Self::$name, Lang::Uk) => $uk, (Self::$name, Lang::En) => $en,)*
                }
            }
        }
    };
}

keys! {
    AppName => ("Текстовий редактор", "Text Editor"),
    Untitled => ("Без назви", "Untitled"),
    File => ("Файл", "File"),
    Edit => ("Правка", "Edit"),
    View => ("Вигляд", "View"),
    Search => ("Пошук", "Find"),
    Format => ("Формат", "Format"),
    Help => ("Довідка", "Help"),
    Menu => ("Меню", "Menu"),
    New => ("Новий документ", "New Document"),
    NewWindow => ("Нове вікно", "New Window"),
    Open => ("Відкрити…", "Open…"),
    OpenFolder => ("Відкрити теку…", "Open Folder…"),
    OpenRecent => ("Нещодавні файли…", "Open Recent…"),
    NoRecent => ("Нещодавніх файлів немає", "No recent files"),
    ClearRecent => ("Очистити список", "Clear Menu"),
    Save => ("Зберегти", "Save"),
    SaveAs => ("Зберегти як…", "Save As…"),
    SaveAll => ("Зберегти все", "Save All"),
    Revert => ("Повернути збережене", "Revert to Saved"),
    ShowInFiles => ("Показати у Файлах", "Show in Files"),
    CopyPath => ("Копіювати шлях", "Copy Path"),
    CloseTab => ("Закрити документ", "Close Document"),
    CloseFolder => ("Закрити теку", "Close Folder"),
    CloseWindow => ("Закрити вікно", "Close Window"),
    Undo => ("Скасувати", "Undo"),
    Redo => ("Повторити", "Redo"),
    Cut => ("Вирізати", "Cut"),
    Copy => ("Копіювати", "Copy"),
    Paste => ("Вставити", "Paste"),
    SelectAll => ("Вибрати все", "Select All"),
    DuplicateLine => ("Дублювати рядок", "Duplicate Line"),
    DeleteLine => ("Видалити рядок", "Delete Line"),
    MoveLineUp => ("Перемістити рядок вгору", "Move Line Up"),
    MoveLineDown => ("Перемістити рядок вниз", "Move Line Down"),
    JoinLines => ("Об’єднати рядки", "Join Lines"),
    SortLines => ("Сортувати рядки", "Sort Lines"),
    ToggleComment => ("Закоментувати / розкоментувати", "Toggle Comment"),
    Indent => ("Збільшити відступ", "Indent"),
    Unindent => ("Зменшити відступ", "Unindent"),
    Uppercase => ("ВЕЛИКІ ЛІТЕРИ", "UPPERCASE"),
    Lowercase => ("малі літери", "lowercase"),
    TitleCase => ("Як У Заголовку", "Title Case"),
    InvertCase => ("іНВЕРТУВАТИ рЕГІСТР", "iNVERT cASE"),
    TrimTrailing => ("Прибрати пробіли в кінці рядків", "Trim Trailing Spaces"),
    InsertDate => ("Вставити дату й час", "Insert Date and Time"),
    Find => ("Знайти…", "Find…"),
    Replace => ("Знайти й замінити…", "Find and Replace…"),
    FindNext => ("Знайти наступне", "Find Next"),
    FindPrevious => ("Знайти попереднє", "Find Previous"),
    UseSelection => ("Шукати виділене", "Use Selection for Find"),
    GoToLine => ("Перейти до рядка…", "Go to Line…"),
    Sidebar => ("Бічна панель", "Sidebar"),
    StatusBar => ("Рядок стану", "Status Bar"),
    Monospace => ("Моноширинний шрифт", "Monospace Font"),
    Preview => ("Перегляд Markdown", "Markdown Preview"),
    ZoomIn => ("Збільшити", "Zoom In"),
    ZoomOut => ("Зменшити", "Zoom Out"),
    ZoomReset => ("Звичайний розмір", "Actual Size"),
    FullScreen => ("На весь екран", "Full Screen"),
    HiddenFiles => ("Показувати приховані файли", "Show Hidden Files"),
    Syntax => ("Мова підсвітки…", "Highlight Mode…"),
    Automatic => ("Автоматично", "Automatic"),
    Detected => ("визначено", "detected"),
    SpacesForTabs => ("Пробіли замість табуляції", "Insert Spaces"),
    TabWidth => ("Ширина табуляції", "Tab Width"),
    AutoIndent => ("Автоматичний відступ", "Auto Indent"),
    LineEndings => ("Закінчення рядків…", "Line Endings…"),
    UnixEndings => ("Unix / macOS (LF)", "Unix / macOS (LF)"),
    WindowsEndings => ("Windows (CRLF)", "Windows (CRLF)"),
    TrimOnSave => ("Прибирати пробіли під час збереження", "Trim Spaces on Save"),
    Statistics => ("Статистика документа…", "Document Statistics…"),
    Shortcuts => ("Комбінації клавіш", "Keyboard Shortcuts"),
    About => ("Про Текстовий редактор", "About Text Editor"),
    AboutBody => (
        "Блокнот Glide: вкладки, дерево тек, пошук і заміна, перегляд Markdown. Мову документа визначає вбудований детектор, підсвітку дає tree-sitter.",
        "The Glide notepad: tabs, a folder tree, find and replace, Markdown preview. A built-in detector picks the document’s language; tree-sitter highlights it."
    ),
    FindPlaceholder => ("Знайти", "Find"),
    ReplacePlaceholder => ("Замінити на", "Replace with"),
    ReplaceOne => ("Замінити", "Replace"),
    ReplaceAll => ("Замінити все", "Replace All"),
    MatchCase => ("Враховувати регістр", "Match Case"),
    WholeWord => ("Лише цілі слова", "Whole Words"),
    Regex => ("Регулярний вираз", "Regular Expression"),
    NoMatches => ("Немає збігів", "No matches"),
    Of => ("з", "of"),
    Done => ("Готово", "Done"),
    Replaced => ("Замінено", "Replaced"),
    ShowReplace => ("Заміна", "Replace"),
    LinePlaceholder => ("Рядок[:стовпець]", "Line[:column]"),
    Line => ("Рядок", "Ln"),
    Column => ("стовпець", "Col"),
    Selected => ("вибрано", "selected"),
    Words => ("сл.", "words"),
    Spaces => ("Пробіли", "Spaces"),
    Tabs => ("Табуляція", "Tab Width"),
    UnsavedOne => ("Зберегти зміни в документі", "Save changes to"),
    UnsavedMany => ("Зберегти зміни в документах?", "Save changes to these documents?"),
    UnsavedBody => ("Якщо не зберегти, зміни буде втрачено.", "If you don’t save, your changes will be lost."),
    DontSave => ("Не зберігати", "Don’t Save"),
    Cancel => ("Скасувати", "Cancel"),
    Close => ("Закрити", "Close"),
    Saved => ("Збережено", "Saved"),
    SaveFailed => ("Не вдалося зберегти", "Couldn’t save"),
    OpenFailed => ("Не вдалося відкрити", "Couldn’t open"),
    Binary => ("це двійковий файл", "it is a binary file"),
    ChangedOnDisk => ("Файл змінено поза редактором.", "The file changed outside the editor."),
    Reload => ("Перезавантажити", "Reload"),
    KeepMine => ("Лишити мою версію", "Keep Mine"),
    DeletedOnDisk => ("Файл видалено або переміщено.", "The file was deleted or moved."),
    Latin1Loss => (
        "Текст не вміщається в ISO-8859-1, тому збережено як UTF-8",
        "The text doesn’t fit ISO-8859-1, so it was saved as UTF-8"
    ),
    ChooserFailed => ("Не вдалося відкрити вікно вибору файлів", "Couldn’t open the file chooser"),
    NoFolder => ("Відкрийте теку, щоб бачити її файли тут.", "Open a folder to see its files here."),
    OpenFolderButton => ("Відкрити теку", "Open Folder"),
    Refresh => ("Оновити", "Refresh"),
    EmptyFolder => ("Порожня тека", "Empty folder"),
    OpenItem => ("Відкрити", "Open"),
    Lines => ("Рядки", "Lines"),
    WordsLong => ("Слова", "Words"),
    Characters => ("Символи", "Characters"),
    CharactersNoSpaces => ("Символи без пробілів", "Characters (no spaces)"),
    Bytes => ("Байти", "Bytes"),
    Encoding => ("Кодування", "Encoding"),
    Maximize => ("Розгорнути / Відновити", "Maximize / Restore"),
    Minimize => ("Згорнути", "Minimize"),
    EmptyTitle => ("Немає відкритих документів", "No open documents"),
    EmptyBody => ("Створіть новий документ або відкрийте файл чи теку.", "Create a new document or open a file or folder."),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_parse_from_glide_toml_values() {
        assert_eq!(Lang::from_key("UK"), Some(Lang::Uk));
        assert_eq!(Lang::from_key("english"), Some(Lang::En));
        assert_eq!(Lang::from_key("de"), None);
        assert_eq!(Key::Save.text(Lang::Uk), "Зберегти");
        assert_eq!(Key::Save.text(Lang::En), "Save");
    }
}
