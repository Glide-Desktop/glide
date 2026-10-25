mod pages;

use std::time::Duration;

use glide_sdk::iced::widget::text_editor;
use glide_sdk::prelude::*;
use glide_sdk::system::{self, Dock, Notification, NotificationEvent, Osd, QuickSettings, Urgency};
use glide_sdk::theme::{Accent, Appearance};
use glide_sdk::widget::Check;

pub const APP_ID: &str = "glide-catalog";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Buttons,
    Inputs,
    Selection,
    Feedback,
    Layout,
    Cards,
    Secrets,
    Overlays,
    Type,
    Motion,
    Notifications,
    System,
}

impl Page {
    pub const ALL: [Self; 13] = [
        Self::Overview,
        Self::Buttons,
        Self::Inputs,
        Self::Selection,
        Self::Feedback,
        Self::Layout,
        Self::Cards,
        Self::Secrets,
        Self::Overlays,
        Self::Type,
        Self::Motion,
        Self::Notifications,
        Self::System,
    ];

    #[must_use]
    pub fn title(self, lang: Lang) -> &'static str {
        let (uk, en) = match self {
            Self::Overview => ("Огляд", "Overview"),
            Self::Buttons => ("Кнопки", "Buttons"),
            Self::Inputs => ("Поля вводу", "Inputs"),
            Self::Selection => ("Вибір", "Selection"),
            Self::Feedback => ("Стан і відгук", "Feedback"),
            Self::Layout => ("Розмітка", "Layout"),
            Self::Cards => ("Картки й списки", "Cards and lists"),
            Self::Secrets => ("Секрети", "Secrets"),
            Self::Overlays => ("Оверлеї", "Overlays"),
            Self::Type => ("Типографіка й кольори", "Type and colour"),
            Self::Motion => ("Рух", "Motion"),
            Self::Notifications => ("Сповіщення", "Notifications"),
            Self::System => ("Система", "System"),
        };
        lang.pick(uk, en)
    }

    #[must_use]
    pub fn icon(self) -> Icon {
        match self {
            Self::Overview => Icon::Sidebar,
            Self::Buttons | Self::Selection => Icon::Check,
            Self::Inputs => Icon::Search,
            Self::Secrets => Icon::Eye,
            Self::Feedback => Icon::Refresh,
            Self::Layout | Self::Cards => Icon::Folder,
            Self::Overlays => Icon::More,
            Self::Type => Icon::File,
            Self::Motion => Icon::Right,
            Self::Notifications => Icon::Bell,
            Self::System => Icon::Menu,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fruit {
    Apple,
    Pear,
    Plum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeout {
    Server,
    Short,
    Long,
    Never,
}

impl Timeout {
    pub const ALL: [Self; 4] = [Self::Server, Self::Short, Self::Long, Self::Never];

    const fn milliseconds(self) -> i32 {
        match self {
            Self::Server => -1,
            Self::Short => 3000,
            Self::Long => 10_000,
            Self::Never => 0,
        }
    }
}

impl std::fmt::Display for Timeout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Server => "Server default",
            Self::Short => "3 s",
            Self::Long => "10 s",
            Self::Never => "Never",
        })
    }
}

#[allow(clippy::struct_excessive_bools)] // One flag per notification hint toggled independently in the form
#[derive(Debug, Clone)]
pub struct Draft {
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub actions: bool,
    pub reply: bool,
    pub resident: bool,
    pub transient: bool,
    pub sound: bool,
    pub progress: Option<f32>,
    pub timeout: Timeout,
}

impl Draft {
    fn new(lang: Lang) -> Self {
        Self {
            summary: lang.pick("Олена надіслала фото", "Olena sent a photo").into(),
            body: lang.pick("Дивись, який захід сонця сьогодні!", "Look at tonight's sunset!").into(),
            urgency: Urgency::Normal,
            actions: true,
            reply: false,
            resident: false,
            transient: false,
            sound: true,
            progress: None,
            timeout: Timeout::Server,
        }
    }

    fn build(&self, lang: Lang) -> Notification {
        let mut notification = Notification::new(APP_ID, &self.summary)
            .body(&self.body)
            .urgency(self.urgency)
            .resident(self.resident)
            .transient(self.transient)
            .timeout(self.timeout.milliseconds())
            .icon("applications-graphics")
            .progress(self.progress.map(|value| (value * 100.0).round() as u8));
        if self.actions {
            notification = notification
                .action("default", lang.pick("Відкрити", "Open"))
                .action("later", lang.pick("Нагадати пізніше", "Remind me later"));
        }
        if self.reply {
            notification = notification.reply(lang.pick("Надіслати", "Send"), lang.pick("Відповідь…", "Reply…"));
        }
        if self.sound {
            notification = notification.sound("message-new-instant");
        } else {
            notification = notification.silent(true);
        }
        notification
    }
}

#[derive(Debug, Clone)]
pub enum Preset {
    Message,
    Download,
    Critical,
    Reminder,
}

#[allow(clippy::struct_excessive_bools)] // Each demo control owns its own independent switch
pub struct Catalog {
    pub page: Page,
    pub look: Look,
    pub accent: Option<Accent>,
    pub name: String,
    pub email: String,
    pub search: String,
    pub password: String,
    pub revealed: bool,
    pub code: String,
    pub version: usize,
    pub filter: usize,
    pub notes: text_editor::Content,
    pub quantity: u32,
    pub fruit: Option<Fruit>,
    pub wifi: bool,
    pub compact: bool,
    pub check: Check,
    pub radio: Option<Fruit>,
    pub segment: Fruit,
    pub tab: Fruit,
    pub chips: [bool; 4],
    pub volume: f32,
    pub stepped: f32,
    pub loading: f32,
    pub count: u32,
    pub expanded: [bool; 2],
    pub flipped: bool,
    pub styles: [bool; 3],
    pub dot: usize,
    pub depth: usize,
    pub draft: Draft,
    pub last: Option<u32>,
    pub download: Option<(u32, f32)>,
    pub log: Vec<String>,
    pub dock: Dock,
    pub dock_count: u32,
    pub dock_progress: f32,
    pub dock_urgent: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Page(usize),
    Look(Look),
    Accent(Option<Accent>),
    Name(String),
    Email(String),
    Search(String),
    ClearSearch,
    Password(String),
    Reveal,
    Code(String),
    Version(usize),
    Filter(usize),
    Edit(text_editor::Action),
    Quantity(u32),
    Fruit(Fruit),
    Wifi(bool),
    Compact(bool),
    Check,
    Radio(Fruit),
    Segment(Fruit),
    Tab(Fruit),
    Chip(usize),
    Volume(f32),
    Stepped(f32),
    Loading(f32),
    Count(u32),
    Expand(usize),
    Flip,
    Style(usize, bool),
    Dot(usize),
    Depth(usize),
    Said(&'static str),
    Dialog,
    Delete,
    Toast,
    ToastUndo,
    Undone,
    Context,
    Summary(String),
    Body(String),
    Urgency(Urgency),
    Actions(bool),
    Reply(bool),
    Resident(bool),
    Transient(bool),
    Sound(bool),
    WithProgress(bool),
    Progress(f32),
    Timeout(Timeout),
    Send,
    Replace,
    Withdraw,
    Preset(Preset),
    Sent(bool, Result<u32, String>),
    Event(NotificationEvent),
    Tick,
    ClearLog,
    DockCount(u32),
    DockProgress(f32),
    DockUrgent(bool),
    Osd(Osd, u32),
    Quick(QuickSettings),
    OpenLink,
    Ignored,
}

impl Catalog {
    fn log(&mut self, line: String) {
        self.log.insert(0, line);
        self.log.truncate(30);
    }

    fn send(notification: Notification, download: bool) -> Task<Message> {
        Task::perform(system::notify(notification), move |result| {
            Message::Sent(download, result.map_err(|err| err.to_string()))
        })
    }

    fn apply_preview(&self, shell: &mut Shell<Message>) {
        let current = shell.look();
        let appearance = match self.look {
            Look::System => None,
            Look::Light => Some(Appearance::Light),
            Look::Dark => Some(Appearance::Dark),
        };
        let preview = match (appearance, self.accent) {
            (None, None) => None,
            (appearance, accent) => {
                Some((appearance.unwrap_or_else(|| current.appearance_now()), accent.unwrap_or(current.accent)))
            }
        };
        shell.preview_look(preview);
    }

    fn preset(&mut self, preset: &Preset, shell: &Shell<Message>) -> Task<Message> {
        let t = |uk: &'static str, en: &'static str| shell.t(uk, en);
        let notification = match preset {
            Preset::Message => Notification::new(APP_ID, t("Андрій", "Andriy"))
                .body(t("Ти будеш на зустрічі о 15:00?", "Are you coming to the 3 pm meeting?"))
                .icon("mail-message-new")
                .reply(t("Надіслати", "Send"), t("Відповідь…", "Reply…"))
                .sound("message-new-instant"),
            Preset::Download => {
                self.download = Some((0, 0.0));
                Notification::new(APP_ID, t("Завантаження glide.iso", "Downloading glide.iso"))
                    .body("0 %")
                    .icon("folder-download")
                    .progress(Some(0))
                    .resident(true)
                    .silent(true)
            }
            Preset::Critical => Notification::new(APP_ID, t("Батарея майже розряджена", "Battery almost empty"))
                .body(t("Залишилось 4 %. Підключіть живлення.", "4 % left. Plug in the charger."))
                .icon("battery-caution")
                .urgency(Urgency::Critical),
            Preset::Reminder => Notification::new(APP_ID, t("Нагадування", "Reminder"))
                .body(t("Подзвонити мамі", "Call mum"))
                .icon("alarm")
                .action("done", t("Готово", "Done"))
                .action("snooze", t("Відкласти на 10 хв", "Snooze 10 min")),
        };
        Self::send(notification, matches!(preset, Preset::Download))
    }

    fn tick(&mut self, shell: &Shell<Message>) -> Task<Message> {
        let Some((id, done)) = self.download else { return Task::none() };
        if id == 0 {
            return Task::none();
        }
        let done = (done + 0.07).min(1.0);
        let percent = (done * 100.0).round() as u8;
        let finished = done >= 1.0;
        self.download = if finished { None } else { Some((id, done)) };
        let mut notification = Notification::new(APP_ID, shell.t("Завантаження glide.iso", "Downloading glide.iso"))
            .replaces(id)
            .icon("folder-download")
            .silent(true);
        notification = if finished {
            notification
                .body(shell.t("Готово — 2,1 ГБ", "Done — 2.1 GB"))
                .action("default", shell.t("Відкрити", "Open"))
        } else {
            notification.body(format!("{percent} %")).progress(Some(percent)).resident(true)
        };
        Self::send(notification, false)
    }
}

impl App for Catalog {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let lang = shell.lang();
        let catalog = Self {
            page: Page::Overview,
            look: Look::System,
            accent: None,
            name: String::new(),
            email: "olena@".into(),
            search: String::new(),
            password: "glide".into(),
            revealed: false,
            code: String::from("482"),
            version: 1,
            filter: 1,
            notes: text_editor::Content::with_text(lang.pick(
                "Багаторядкове поле на text_editor зі стилем SDK.",
                "A multi-line field: text_editor with the SDK style.",
            )),
            quantity: 2,
            fruit: Some(Fruit::Pear),
            wifi: true,
            compact: false,
            check: Check::Mixed,
            radio: Some(Fruit::Apple),
            segment: Fruit::Apple,
            tab: Fruit::Apple,
            chips: [true, false, true, false],
            volume: 60.0,
            stepped: 50.0,
            loading: 0.42,
            count: 3,
            expanded: [true, false],
            flipped: false,
            styles: [true, false, false],
            dot: 1,
            depth: 3,
            draft: Draft::new(lang),
            last: None,
            download: None,
            log: Vec::new(),
            dock: Dock::new(APP_ID),
            dock_count: 0,
            dock_progress: 0.0,
            dock_urgent: false,
        };
        (catalog, Task::none())
    }

    fn title(&self, shell: &Shell<Message>) -> String {
        shell.t("Каталог компонентів", "Component catalogue").into()
    }

    fn subtitle(&self, shell: &Shell<Message>) -> Option<String> {
        Some(self.page.title(shell.lang()).into())
    }

    #[allow(clippy::too_many_lines)] // The message dispatcher: one short arm per Message variant
    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        match message {
            Message::Page(index) => self.page = Page::ALL.get(index).copied().unwrap_or(Page::Overview),
            Message::Look(look) => {
                self.look = look;
                self.apply_preview(shell);
            }
            Message::Accent(accent) => {
                self.accent = accent;
                self.apply_preview(shell);
            }
            Message::Name(name) => self.name = name,
            Message::Email(email) => self.email = email,
            Message::Search(search) => self.search = search,
            Message::ClearSearch => self.search.clear(),
            Message::Password(password) => self.password = password,
            Message::Reveal => self.revealed = !self.revealed,
            Message::Code(code) => self.code = code,
            Message::Version(index) => self.version = index,
            Message::Filter(index) => self.filter = index,
            Message::Edit(action) => self.notes.perform(action),
            Message::Quantity(quantity) => self.quantity = quantity,
            Message::Fruit(fruit) => self.fruit = Some(fruit),
            Message::Wifi(on) => self.wifi = on,
            Message::Compact(on) => self.compact = on,
            Message::Check => {
                self.check = match self.check {
                    Check::Off => Check::On,
                    Check::On => Check::Mixed,
                    Check::Mixed => Check::Off,
                }
            }
            Message::Radio(fruit) => self.radio = Some(fruit),
            Message::Segment(fruit) => self.segment = fruit,
            Message::Tab(fruit) => self.tab = fruit,
            Message::Chip(index) => {
                if let Some(chip) = self.chips.get_mut(index) {
                    *chip = !*chip;
                }
            }
            Message::Volume(volume) => self.volume = volume,
            Message::Stepped(value) => self.stepped = value,
            Message::Loading(value) => self.loading = value,
            Message::Count(count) => self.count = count,
            Message::Expand(index) => {
                if let Some(open) = self.expanded.get_mut(index) {
                    *open = !*open;
                }
            }
            Message::Flip => self.flipped = !self.flipped,
            Message::Style(index, on) => {
                if let Some(style) = self.styles.get_mut(index) {
                    *style = on;
                }
            }
            Message::Dot(index) => self.dot = index,
            Message::Depth(depth) => self.depth = depth,
            Message::Said(what) => shell.toast(what),
            Message::Dialog => shell.dialog(
                Dialog::new(
                    shell.t("Видалити «Ескіз.png»?", "Delete “Sketch.png”?"),
                    shell.t("Файл буде переміщено в смітник.", "The file will be moved to the Trash."),
                )
                .choice(shell.t("Видалити", "Delete"), Tone::Danger, Message::Delete)
                .cancel(shell.t("Скасувати", "Cancel"))
                .choice(shell.t("Перейменувати", "Rename"), Tone::Accent, Message::Said("Rename")),
            ),
            Message::Delete => shell.toast_with(
                shell.t("Файл видалено", "File deleted"),
                shell.t("Скасувати", "Undo"),
                Message::Undone,
            ),
            Message::Toast => shell.toast(shell.t("Простий тост на чотири секунди", "A plain four-second toast")),
            Message::ToastUndo => {
                shell.toast_with(
                    shell.t("Архівовано 3 листи", "3 messages archived"),
                    shell.t("Скасувати", "Undo"),
                    Message::Undone,
                );
            }
            Message::Undone => shell.toast(shell.t("Дію скасовано", "Undone")),
            Message::Context => shell.context_menu(vec![
                Item::action(shell.t("Копіювати", "Copy"), Message::Said("Copy")).shortcut("Ctrl+C"),
                Item::action(shell.t("Вставити", "Paste"), Message::Said("Paste")).shortcut("Ctrl+V"),
                Item::separator(),
                Item::submenu(
                    shell.t("Поділитися", "Share"),
                    vec![
                        Item::action("Email", Message::Said("Email")),
                        Item::action("Telegram", Message::Said("Telegram")),
                    ],
                ),
                Item::action(shell.t("Видалити", "Delete"), Message::Dialog),
            ]),
            Message::Summary(text) => self.draft.summary = text,
            Message::Body(text) => self.draft.body = text,
            Message::Urgency(urgency) => self.draft.urgency = urgency,
            Message::Actions(on) => self.draft.actions = on,
            Message::Reply(on) => self.draft.reply = on,
            Message::Resident(on) => self.draft.resident = on,
            Message::Transient(on) => self.draft.transient = on,
            Message::Sound(on) => self.draft.sound = on,
            Message::WithProgress(on) => self.draft.progress = on.then_some(0.4),
            Message::Progress(value) => self.draft.progress = Some(value),
            Message::Timeout(timeout) => self.draft.timeout = timeout,
            Message::Send => return Self::send(self.draft.build(shell.lang()), false),
            Message::Replace => {
                let notification = self.draft.build(shell.lang()).replaces(self.last.unwrap_or(0));
                return Self::send(notification, false);
            }
            Message::Withdraw => {
                if let Some(id) = self.last.take() {
                    return Task::perform(system::withdraw(id), |_| Message::Ignored);
                }
            }
            Message::Preset(preset) => return self.preset(&preset, shell),
            Message::Sent(download, result) => match result {
                Ok(id) => {
                    self.last = Some(id);
                    if download && let Some((slot, _)) = &mut self.download {
                        *slot = id;
                    }
                    self.log(format!("{} #{id}", shell.t("Надіслано", "Sent")));
                }
                Err(err) => {
                    self.download = None;
                    self.log(format!("{}: {err}", shell.t("Помилка", "Error")));
                    shell.toast(shell.t("Сервер сповіщень недоступний", "No notification server"));
                }
            },
            Message::Event(event) => {
                let line = match event {
                    NotificationEvent::Action { id, key } => format!("#{id} {} «{key}»", shell.t("дія", "action")),
                    NotificationEvent::Replied { id, text } => {
                        format!("#{id} {} «{text}»", shell.t("відповідь", "reply"))
                    }
                    NotificationEvent::Closed { id, reason } => {
                        format!("#{id} {} ({reason:?})", shell.t("закрито", "closed"))
                    }
                };
                self.log(line);
            }
            Message::Tick => return self.tick(shell),
            Message::ClearLog => self.log.clear(),
            Message::DockCount(count) => {
                self.dock_count = count;
                self.dock.count((count > 0).then_some(i64::from(count)));
            }
            Message::DockProgress(value) => {
                self.dock_progress = value;
                self.dock.progress((value > 0.0).then_some(f64::from(value)));
            }
            Message::DockUrgent(on) => {
                self.dock_urgent = on;
                self.dock.urgent(on);
            }
            Message::Osd(kind, level) => {
                if system::show_osd(kind, level, true).is_err() {
                    shell.toast(shell.t("Оболонка Glide недоступна", "The Glide shell is not running"));
                }
            }
            Message::Quick(view) => {
                if system::open_quick_settings(view).is_err() {
                    shell.toast(shell.t("Оболонка Glide недоступна", "The Glide shell is not running"));
                }
            }
            Message::OpenLink => {
                if system::open("https://github.com", APP_ID).is_err() {
                    shell.toast(shell.t("Не вдалося відкрити посилання", "Could not open the link"));
                }
            }
            Message::Ignored => {}
        }
        Task::none()
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        pages::view(self, shell)
    }

    fn menus(&self, shell: &Shell<Message>) -> Vec<Menu<Message>> {
        let pages = Page::ALL
            .iter()
            .enumerate()
            .map(|(index, page)| {
                Item::action(page.title(shell.lang()), Message::Page(index))
                    .shortcut(format!("Ctrl+{}", (index + 1) % 10))
                    .checked(*page == self.page)
            })
            .take(10)
            .collect();
        vec![
            Menu::new(shell.t("Каталог", "Catalogue"), pages),
            Menu::new(
                shell.t("Вигляд", "View"),
                vec![
                    Item::action(shell.t("Як у системі", "Follow the system"), Message::Look(Look::System))
                        .checked(self.look == Look::System),
                    Item::action(shell.t("Світла тема", "Light"), Message::Look(Look::Light))
                        .checked(self.look == Look::Light),
                    Item::action(shell.t("Темна тема", "Dark"), Message::Look(Look::Dark))
                        .checked(self.look == Look::Dark),
                ],
            ),
            Menu::new(
                shell.t("Сповіщення", "Notifications"),
                vec![
                    Item::action(shell.t("Надіслати чернетку", "Send the draft"), Message::Send).shortcut("Ctrl+Enter"),
                    Item::action(
                        shell.t("Повідомлення з відповіддю", "Message with reply"),
                        Message::Preset(Preset::Message),
                    ),
                    Item::action(
                        shell.t("Завантаження з прогресом", "Download with progress"),
                        Message::Preset(Preset::Download),
                    ),
                    Item::action(shell.t("Критичне", "Critical"), Message::Preset(Preset::Critical)),
                ],
            ),
        ]
    }

    fn subscription(&self) -> Subscription<Message> {
        let events = Subscription::run(system::notification_events).map(Message::Event);
        if self.download.is_some() {
            Subscription::batch([
                events,
                glide_sdk::iced::time::every(Duration::from_millis(400)).map(|_| Message::Tick),
            ])
        } else {
            events
        }
    }
}
