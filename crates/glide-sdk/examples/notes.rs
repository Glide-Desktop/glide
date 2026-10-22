use glide_sdk::iced::widget::{column, row};
use glide_sdk::prelude::*;
use glide_sdk::system::{self, Dock, Notification};
use glide_sdk::widget::{self as ui};

const APP_ID: &str = "org.example.Notes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    Recent,
    Title,
}

struct Note {
    title: String,
    pinned: bool,
}

struct Notebook {
    notes: Vec<Note>,
    selected: usize,
    sort: Sort,
    font_size: f32,
    sidebar: bool,
    dock: Dock,
}

#[derive(Debug, Clone)]
enum Message {
    New,
    Save,
    Select(usize),
    Rename(String),
    Pin(bool),
    Sort(Sort),
    FontSize(f32),
    ToggleSidebar,
    AskDelete,
    Delete,
    Share,
    Notified(Result<u32, String>),
    Quit,
}

impl Notebook {
    fn current(&self) -> Option<&Note> {
        self.notes.get(self.selected)
    }

    fn sidebar<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        let items = self
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let glyph = if note.pinned { Icon::Check } else { Icon::File };
                ui::sidebar_item(paint, Some(glyph), &note.title, index == self.selected, Message::Select(index))
            })
            .collect();
        ui::sidebar(paint, 240.0, items)
    }

    fn detail<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let paint = shell.paint();
        let Some(note) = self.current() else {
            return ui::empty_state(
                paint,
                Icon::File,
                shell.t("Немає нотаток", "No notes"),
                shell.t("Створіть першу кнопкою «+».", "Create one with the + button."),
            );
        };
        let options = [(Sort::Recent, shell.t("Нові", "Recent")), (Sort::Title, shell.t("За назвою", "By title"))];
        let rows = vec![
            ui::list_row(
                paint,
                shell.t("Закріпити", "Pin"),
                Some(shell.t("Показувати першою в списку", "Keep it at the top of the list")),
                ui::switch(paint, note.pinned, Some(Message::Pin)),
            ),
            ui::list_row(
                paint,
                shell.t("Сортування", "Sort"),
                None,
                ui::segmented(paint, &options, self.sort, Message::Sort),
            ),
            ui::list_row(
                paint,
                shell.t("Розмір шрифту", "Font size"),
                None,
                container(ui::slider(paint, 10.0..=24.0, self.font_size, Message::FontSize)).width(180),
            ),
        ];
        let actions = row![
            ui::danger(paint, shell.t("Видалити…", "Delete…"), Message::AskDelete),
            Space::new().width(Fill),
            ui::secondary(paint, shell.t("Поділитися", "Share"), Message::Share),
            ui::primary(paint, shell.t("Зберегти", "Save"), Message::Save),
        ]
        .spacing(SP2);
        ui::scrollable(
            paint,
            column![
                text(&note.title, TextStyle::T2),
                ui::text_field(paint, shell.t("Назва", "Title"), &note.title, Message::Rename),
                ui::section(paint, shell.t("Параметри", "Options"), ui::list(paint, rows)),
                actions,
            ]
            .spacing(SP4)
            .padding(SP5)
            .max_width(640),
        )
        .into()
    }
}

impl App for Notebook {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let notes = [shell.t("Список покупок", "Groceries"), shell.t("Ідеї", "Ideas"), shell.t("Відпустка", "Holiday")]
            .into_iter()
            .map(|title| Note { title: title.to_string(), pinned: false })
            .collect();
        (
            Self { notes, selected: 0, sort: Sort::Recent, font_size: 14.0, sidebar: true, dock: Dock::new(APP_ID) },
            Task::none(),
        )
    }

    fn title(&self, shell: &Shell<Message>) -> String {
        self.current().map_or_else(|| shell.t("Нотатки", "Notes").to_string(), |note| note.title.clone())
    }

    fn subtitle(&self, shell: &Shell<Message>) -> Option<String> {
        Some(format!("{} {}", self.notes.len(), shell.t("нотаток", "notes")))
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Save => shell.toast(shell.t("Збережено", "Saved")),
            Message::New => {
                self.notes
                    .push(Note { title: shell.t("Нова нотатка", "New note").to_string(), pinned: false });
                self.selected = self.notes.len() - 1;
                self.dock.count(Some(self.notes.len() as i64));
            }
            Message::Select(index) => self.selected = index,
            Message::Rename(title) => {
                if let Some(note) = self.notes.get_mut(self.selected) {
                    note.title = title;
                }
            }
            Message::Pin(on) => {
                if let Some(note) = self.notes.get_mut(self.selected) {
                    note.pinned = on;
                }
            }
            Message::Sort(sort) => {
                self.sort = sort;
                if sort == Sort::Title {
                    self.notes.sort_by(|a, b| a.title.cmp(&b.title));
                }
            }
            Message::FontSize(size) => self.font_size = size.round(),
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::AskDelete => {
                let title = self.current().map(|note| note.title.clone()).unwrap_or_default();
                shell.dialog(
                    Dialog::new(
                        format!("{} «{title}»?", shell.t("Видалити", "Delete")),
                        shell.t("Це не можна скасувати.", "This cannot be undone."),
                    )
                    .choice(shell.t("Видалити", "Delete"), Tone::Danger, Message::Delete)
                    .cancel(shell.t("Скасувати", "Cancel")),
                );
            }
            Message::Delete => {
                if self.selected < self.notes.len() {
                    self.notes.remove(self.selected);
                    self.selected = self.selected.min(self.notes.len().saturating_sub(1));
                    shell.toast(shell.t("Нотатку видалено", "Note deleted"));
                }
            }
            Message::Share => {
                let title = self.current().map(|note| note.title.clone()).unwrap_or_default();
                let notification = Notification::new(APP_ID, shell.t("Нотатку надіслано", "Note shared")).body(title);
                return Task::perform(system::notify(notification), |result| {
                    Message::Notified(result.map_err(|err| err.to_string()))
                });
            }
            Message::Notified(result) => {
                if result.is_err() {
                    shell.toast(shell.t("Сповіщення недоступні", "Notifications are unavailable"));
                }
            }
            Message::Quit => shell.exit(),
        }
        Task::none()
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        let detail = container(self.detail(shell)).width(Fill).height(Fill);
        if self.sidebar {
            row![self.sidebar(shell), ui::vertical_separator(shell.paint()), detail].into()
        } else {
            detail.into()
        }
    }

    fn menus(&self, shell: &Shell<Message>) -> Vec<Menu<Message>> {
        let has_note = self.current().is_some();
        vec![
            Menu::new(
                shell.t("Файл", "File"),
                vec![
                    Item::action(shell.t("Нова нотатка", "New Note"), Message::New).shortcut("Ctrl+N"),
                    Item::action(shell.t("Зберегти", "Save"), Message::Save).shortcut("Ctrl+S").enabled(has_note),
                    Item::action(shell.t("Поділитися", "Share"), Message::Share).enabled(has_note),
                    Item::separator(),
                    Item::action(shell.t("Вийти", "Quit"), Message::Quit).shortcut("Ctrl+Q"),
                ],
            ),
            Menu::new(
                shell.t("Правка", "Edit"),
                vec![
                    Item::action(shell.t("Видалити…", "Delete…"), Message::AskDelete)
                        .shortcut("Delete")
                        .enabled(has_note),
                ],
            ),
            Menu::new(
                shell.t("Вигляд", "View"),
                vec![
                    Item::action(shell.t("Бічна панель", "Sidebar"), Message::ToggleSidebar)
                        .shortcut("Ctrl+B")
                        .checked(self.sidebar),
                    Item::submenu(
                        shell.t("Сортування", "Sort"),
                        vec![
                            Item::action(shell.t("Нові", "Recent"), Message::Sort(Sort::Recent))
                                .checked(self.sort == Sort::Recent),
                            Item::action(shell.t("За назвою", "By title"), Message::Sort(Sort::Title))
                                .checked(self.sort == Sort::Title),
                        ],
                    ),
                ],
            ),
        ]
    }

    fn toolbar<'a>(&'a self, shell: &'a Shell<Message>) -> Vec<Element<'a, Message>> {
        let paint = shell.paint();
        vec![
            ui::icon_button(
                paint,
                Icon::Sidebar,
                shell.t("Бічна панель", "Sidebar"),
                Some(Message::ToggleSidebar),
                self.sidebar,
            ),
            ui::icon_button(paint, Icon::Plus, shell.t("Нова нотатка", "New note"), Some(Message::New), false),
        ]
    }
}

fn main() -> glide_sdk::iced::Result {
    glide_sdk::run::<Notebook>(Settings::new(APP_ID).size(900.0, 600.0).min_size(520.0, 360.0))
}
