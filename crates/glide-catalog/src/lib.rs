mod pages;

use std::time::Duration;

use glide_sdk::iced::widget::text_editor;
use glide_sdk::prelude::*;
use glide_sdk::system::{self, Closed, Dock, Notification, NotificationEvent, Osd, QuickSettings, Urgency};
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
    Insight,
    Gallery,
    Secrets,
    Overlays,
    Type,
    Motion,
    Notifications,
    System,
}

impl Page {
    pub const ALL: [Self; 15] = [
        Self::Overview,
        Self::Buttons,
        Self::Inputs,
        Self::Selection,
        Self::Feedback,
        Self::Layout,
        Self::Cards,
        Self::Insight,
        Self::Gallery,
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
            Self::Insight => ("Стан і огляди", "Status and summaries"),
            Self::Gallery => ("Галерея", "Gallery"),
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
            Self::Feedback | Self::Insight => Icon::Refresh,
            Self::Gallery => Icon::Inspector,
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

impl Fruit {
    pub const ALL: [Self; 3] = [Self::Apple, Self::Pear, Self::Plum];

    fn label(self, lang: Lang) -> &'static str {
        let (uk, en) = match self {
            Self::Apple => ("🍎 Яблуко", "🍎 Apple"),
            Self::Pear => ("🍐 Груша", "🍐 Pear"),
            Self::Plum => ("🟣 Слива", "🟣 Plum"),
        };
        lang.pick(uk, en)
    }
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

    fn label(self, lang: Lang) -> &'static str {
        let (uk, en) = match self {
            Self::Server => ("Типово для сервера", "Server default"),
            Self::Short => ("3 с", "3 s"),
            Self::Long => ("10 с", "10 s"),
            Self::Never => ("Ніколи", "Never"),
        };
        lang.pick(uk, en)
    }
}

#[derive(Debug, Clone, Copy)]
struct Labelled<T> {
    value: T,
    label: &'static str,
}

impl<T> Labelled<T> {
    const fn new(value: T, label: &'static str) -> Self {
        Self { value, label }
    }
}

impl<T: PartialEq> PartialEq for Labelled<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> std::fmt::Display for Labelled<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label)
    }
}

fn fruit_option(fruit: Fruit, lang: Lang) -> Labelled<Fruit> {
    Labelled::new(fruit, fruit.label(lang))
}

fn fruit_options(lang: Lang) -> Vec<Labelled<Fruit>> {
    Fruit::ALL.iter().map(|fruit| fruit_option(*fruit, lang)).collect()
}

fn timeout_option(timeout: Timeout, lang: Lang) -> Labelled<Timeout> {
    Labelled::new(timeout, timeout.label(lang))
}

fn timeout_options(lang: Lang) -> Vec<Labelled<Timeout>> {
    Timeout::ALL.iter().map(|timeout| timeout_option(*timeout, lang)).collect()
}

fn closed_label(reason: Closed, lang: Lang) -> &'static str {
    let (uk, en) = match reason {
        Closed::Expired => ("минув час", "expired"),
        Closed::Dismissed => ("закрито користувачем", "dismissed"),
        Closed::Withdrawn => ("відкликано", "withdrawn"),
        Closed::Other => ("інша причина", "other"),
    };
    lang.pick(uk, en)
}

fn event_line(event: &NotificationEvent, lang: Lang) -> String {
    match event {
        NotificationEvent::Action { id, key } => format!("#{id} {} «{key}»", lang.pick("дія", "action")),
        NotificationEvent::Replied { id, text } => format!("#{id} {} «{text}»", lang.pick("відповідь", "reply")),
        NotificationEvent::Closed { id, reason } => {
            format!("#{id} {} ({})", lang.pick("закрито", "closed"), closed_label(*reason, lang))
        }
    }
}

fn send_failed(error: &str, lang: Lang) -> String {
    format!("{}: {error}", lang.pick("Не вдалося надіслати сповіщення", "Could not send the notification"))
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
    pub secret_revealed: bool,
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
    pub issue_open: bool,
    pub info_open: bool,
    pub plan: Fruit,
    pub gallery: Gallery,
}

pub struct Gallery {
    pub pictures: Vec<glide_sdk::iced::widget::image::Handle>,
    pub picked: [bool; 2],
    pub favourite: bool,
    pub exposure: i32,
    pub crop: glide_sdk::widget::CropRect,
    pub tilt: f32,
    pub frame: usize,
}

const SCENES: [[[u8; 3]; 4]; 6] = [
    [[0xf2, 0xd2, 0x96], [0xfc, 0xe4, 0xd6], [0xec, 0x8b, 0x4f], [0x74, 0x34, 0x0f]],
    [[0xa7, 0xc8, 0xf6], [0xff, 0xff, 0xff], [0x5b, 0x9b, 0xec], [0x16, 0x3e, 0x79]],
    [[0xdc, 0xef, 0xd9], [0xfa, 0xec, 0xd2], [0x6c, 0xbb, 0x62], [0x1e, 0x4f, 0x1c]],
    [[0xcd, 0xb4, 0xee], [0xf6, 0xbf, 0x9c], [0x7a, 0x4d, 0xbb], [0x40, 0x25, 0x66]],
    [[0xd3, 0xeb, 0xe7], [0xff, 0xff, 0xff], [0x46, 0xb3, 0xa6], [0x08, 0x4b, 0x45]],
    [[0x16, 0x3e, 0x79], [0xfa, 0xec, 0xd2], [0x35, 0x31, 0x2c], [0x1b, 0x1a, 0x17]],
];

#[must_use]
pub fn landscape(index: usize, width: u32, height: u32) -> glide_sdk::iced::widget::image::Handle {
    let [sky, sun, far, near] = SCENES[index % SCENES.len()];
    let (w, h) = (width as f32, height as f32);
    let sun_x = w * (0.25 + (index * 37 % 50) as f32 / 100.0);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        for column in 0..width {
            let (x, y) = (column as f32, row as f32);
            let ridge = h * (0.55 + 0.08 * (x / w * 6.0 + index as f32).sin());
            let shore = h * (0.75 + 0.05 * (x / w * 9.0 + index as f32 * 2.0).cos());
            let colour = if y > shore {
                near
            } else if y > ridge {
                far
            } else if (x - sun_x).hypot(y - h * 0.25) < h * 0.09 {
                sun
            } else {
                sky
            };
            pixels.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    glide_sdk::iced::widget::image::Handle::from_rgba(width, height, pixels)
}

impl Gallery {
    fn new() -> Self {
        Self {
            pictures: (0..SCENES.len()).map(|index| landscape(index, 360, 240)).collect(),
            picked: [false, true],
            favourite: true,
            exposure: 14,
            crop: glide_sdk::widget::CropRect { x: 0.1, y: 0.1, w: 0.8, h: 0.8 },
            tilt: -3.5,
            frame: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    IssueOpen,
    InfoOpen,
    Plan(Fruit),
    Page(usize),
    Look(Look),
    Accent(Option<Accent>),
    Name(String),
    Email(String),
    Search(String),
    ClearSearch,
    Password(String),
    Reveal,
    RevealSecret,
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
    SystemResult(bool, &'static str),
    Ignored,
    GalleryPick(usize),
    GalleryFavourite,
    GalleryExposure(i32),
    GalleryCrop(glide_sdk::widget::CropRect),
    GalleryTilt(f32),
    GalleryFrame(usize),
}

const SHORTCUT_PAGES: usize = 10;

fn page_shortcut(index: usize) -> Option<String> {
    (index < SHORTCUT_PAGES).then(|| format!("Ctrl+{}", (index + 1) % SHORTCUT_PAGES))
}

struct Outgoing {
    notification: Notification,
    download: bool,
}

const DOWNLOAD_STEP: f32 = 0.07;

fn preset_request(download: &mut Option<(u32, f32)>, preset: &Preset, lang: Lang) -> Option<Outgoing> {
    let t = |uk: &'static str, en: &'static str| lang.pick(uk, en);
    let notification = match preset {
        Preset::Message => Notification::new(APP_ID, t("Андрій", "Andriy"))
            .body(t("Ти будеш на зустрічі о 15:00?", "Are you coming to the 3 pm meeting?"))
            .icon("mail-message-new")
            .reply(t("Надіслати", "Send"), t("Відповідь…", "Reply…"))
            .sound("message-new-instant"),
        Preset::Download => {
            if matches!(*download, Some((0, _))) {
                return None;
            }
            let replaces = download.map_or(0, |(id, _)| id);
            *download = Some((replaces, 0.0));
            Notification::new(APP_ID, t("Завантаження glide.iso", "Downloading glide.iso"))
                .replaces(replaces)
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
    Some(Outgoing { notification, download: matches!(preset, Preset::Download) })
}

fn download_tick(download: &mut Option<(u32, f32)>, lang: Lang) -> Option<Outgoing> {
    let (id, done) = download.filter(|&(id, _)| id != 0)?;
    let done = (done + DOWNLOAD_STEP).min(1.0);
    let percent = (done * 100.0).round() as u8;
    let finished = done >= 1.0;
    *download = if finished { None } else { Some((id, done)) };
    let notification = Notification::new(APP_ID, lang.pick("Завантаження glide.iso", "Downloading glide.iso"))
        .replaces(id)
        .icon("folder-download")
        .silent(true);
    let notification = if finished {
        notification
            .body(lang.pick("Готово — 2,1 ГБ", "Done — 2.1 GB"))
            .action("default", lang.pick("Відкрити", "Open"))
    } else {
        notification.body(format!("{percent} %")).progress(Some(percent)).resident(true)
    };
    Some(Outgoing { notification, download: true })
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

    fn dispatch(outgoing: Option<Outgoing>) -> Task<Message> {
        outgoing.map_or_else(Task::none, |outgoing| Self::send(outgoing.notification, outgoing.download))
    }

    fn on_sent(&mut self, download: bool, result: Result<u32, String>, lang: Lang) -> Option<String> {
        match result {
            Ok(id) => {
                if download {
                    if let Some((slot, _)) = &mut self.download {
                        *slot = id;
                    }
                } else {
                    self.last = Some(id);
                }
                self.log(format!("{} #{id}", lang.pick("Надіслано", "Sent")));
                None
            }
            Err(err) => {
                if download {
                    self.download = None;
                }
                self.log(format!("{}: {err}", lang.pick("Помилка", "Error")));
                Some(send_failed(&err, lang))
            }
        }
    }

    fn on_closed(&mut self, id: u32) {
        if self.last == Some(id) {
            self.last = None;
        }
        if self.download.is_some_and(|(slot, _)| slot != 0 && slot == id) {
            self.download = None;
        }
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
}

impl Catalog {
    fn initial(lang: Lang) -> Self {
        Self {
            page: Page::Overview,
            look: Look::System,
            accent: None,
            name: String::new(),
            email: "olena@".into(),
            search: String::new(),
            password: "glide".into(),
            revealed: false,
            secret_revealed: false,
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
            issue_open: false,
            info_open: false,
            plan: Fruit::Pear,
            gallery: Gallery::new(),
        }
    }
}

impl App for Catalog {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        (Self::initial(shell.lang()), Task::none())
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
            Message::IssueOpen => self.issue_open = !self.issue_open,
            Message::InfoOpen => self.info_open = !self.info_open,
            Message::Plan(fruit) => self.plan = fruit,
            Message::GalleryPick(index) => {
                if let Some(picked) = self.gallery.picked.get_mut(index) {
                    *picked = !*picked;
                }
            }
            Message::GalleryFavourite => self.gallery.favourite = !self.gallery.favourite,
            Message::GalleryExposure(value) => self.gallery.exposure = value,
            Message::GalleryCrop(rect) => self.gallery.crop = rect,
            Message::GalleryTilt(degrees) => self.gallery.tilt = degrees,
            Message::GalleryFrame(index) => self.gallery.frame = index,
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
            Message::RevealSecret => self.secret_revealed = !self.secret_revealed,
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
                .choice(
                    shell.t("Перейменувати", "Rename"),
                    Tone::Accent,
                    Message::Said(shell.t("Перейменувати", "Rename")),
                ),
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
                Item::action(shell.t("Копіювати", "Copy"), Message::Said(shell.t("Копіювати", "Copy")))
                    .shortcut("Ctrl+C"),
                Item::action(shell.t("Вставити", "Paste"), Message::Said(shell.t("Вставити", "Paste")))
                    .shortcut("Ctrl+V"),
                Item::separator(),
                Item::submenu(
                    shell.t("Поділитися", "Share"),
                    vec![
                        Item::action(shell.t("Пошта", "Email"), Message::Said(shell.t("Пошта", "Email"))),
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
            Message::Preset(preset) => {
                return Self::dispatch(preset_request(&mut self.download, &preset, shell.lang()));
            }
            Message::Sent(download, result) => {
                if let Some(text) = self.on_sent(download, result, shell.lang()) {
                    shell.toast(text);
                }
            }
            Message::Event(event) => {
                if let NotificationEvent::Closed { id, .. } = &event {
                    self.on_closed(*id);
                }
                self.log(event_line(&event, shell.lang()));
            }
            Message::Tick => return Self::dispatch(download_tick(&mut self.download, shell.lang())),
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
                let error = shell.t("Оболонка Glide недоступна", "The Glide shell is not running");
                return Task::perform(async move { system::show_osd(kind, level, true).is_ok() }, move |ok| {
                    Message::SystemResult(ok, error)
                });
            }
            Message::Quick(view) => {
                let error = shell.t("Оболонка Glide недоступна", "The Glide shell is not running");
                return Task::perform(async move { system::open_quick_settings(view).is_ok() }, move |ok| {
                    Message::SystemResult(ok, error)
                });
            }
            Message::OpenLink => {
                let error = shell.t("Не вдалося відкрити посилання", "Could not open the link");
                return Task::perform(async { system::open("https://github.com", APP_ID).is_ok() }, move |ok| {
                    Message::SystemResult(ok, error)
                });
            }
            Message::SystemResult(ok, error) => {
                if !ok {
                    shell.toast(error);
                }
            }
            Message::Ignored => {}
        }
        Task::none()
    }

    fn view<'a>(&'a self, shell: &'a Shell<Message>) -> Element<'a, Message> {
        pages::view(self, shell)
    }

    fn look_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.apply_preview(shell);
        Task::none()
    }

    fn menus(&self, shell: &Shell<Message>) -> Vec<Menu<Message>> {
        let pages = Page::ALL
            .iter()
            .enumerate()
            .map(|(index, page)| {
                let item = Item::action(page.title(shell.lang()), Message::Page(index)).checked(*page == self.page);
                match page_shortcut(index) {
                    Some(chord) => item.shortcut(chord),
                    None => item,
                }
            })
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use glide_sdk::Harness;
    use glide_sdk::app::Event;
    use glide_sdk::iced::Size;

    use super::*;

    const LANGS: [Lang; 2] = [Lang::Uk, Lang::En];

    fn bench() -> Harness<Catalog> {
        Harness::new(APP_ID, glide_sdk::Look::default(), Size::new(1120.0, 760.0))
    }

    fn shell_look(appearance: Appearance, accent: Accent) -> glide_sdk::glide_app::Look {
        glide_sdk::glide_app::Look {
            appearance,
            accent,
            language: Some("en".into()),
            animation_scale: 1.0,
            reduced_motion: false,
            font_family: None,
            icon_theme: None,
            locked: false,
            active_workspace: 0,
        }
    }

    fn opposite(appearance: Appearance) -> Appearance {
        if appearance == Appearance::Light { Appearance::Dark } else { Appearance::Light }
    }

    fn slot_id(catalog: &Catalog) -> Option<u32> {
        catalog.download.map(|(id, _)| id)
    }

    #[test]
    fn the_download_tick_replaces_the_tracked_id_and_is_sent_as_the_downloads_own() {
        let mut download = Some((5, 0.0));
        let tick = download_tick(&mut download, Lang::En).expect("a running download ticks");
        assert!(tick.download, "the reply to a tick must reach the download slot, not the draft's last id");
        assert_eq!(tick.notification.replaces, 5);
        assert_eq!(tick.notification.progress, Some(7));
        assert!(tick.notification.resident);
        assert_eq!(download.map(|(id, _)| id), Some(5));
    }

    #[test]
    fn a_download_still_waiting_for_its_first_id_or_not_running_does_not_tick() {
        let mut pending = Some((0, 0.0));
        assert!(download_tick(&mut pending, Lang::En).is_none());
        assert_eq!(pending, Some((0, 0.0)));
        let mut idle = None;
        assert!(download_tick(&mut idle, Lang::En).is_none());
        assert_eq!(idle, None);
    }

    #[test]
    fn the_download_finishes_with_a_done_notice_without_a_progress_bar() {
        let mut download = Some((5, 0.0));
        let mut sent = Vec::new();
        while let Some(tick) = download_tick(&mut download, Lang::En) {
            sent.push(tick);
            assert!(sent.len() <= 20, "the simulated download ends");
        }
        let last = sent.last().expect("at least one tick");
        assert_eq!(last.notification.progress, None);
        assert!(!last.notification.resident);
        assert_eq!(download, None);
        assert!(sent.iter().all(|tick| tick.download && tick.notification.replaces == 5));
    }

    #[test]
    fn pressing_download_again_restarts_the_tracked_notification_instead_of_orphaning_it() {
        let mut running = Some((5, 0.4));
        let again = preset_request(&mut running, &Preset::Download, Lang::En).expect("a known id restarts");
        assert!(again.download);
        assert_eq!(again.notification.replaces, 5);
        assert_eq!(running, Some((5, 0.0)));

        let mut pending = Some((0, 0.0));
        assert!(preset_request(&mut pending, &Preset::Download, Lang::En).is_none(), "no id yet: ignored");
        assert_eq!(pending, Some((0, 0.0)));

        let mut idle = None;
        let first = preset_request(&mut idle, &Preset::Download, Lang::En).expect("the first press starts it");
        assert!(first.download);
        assert_eq!(first.notification.replaces, 0);
        assert_eq!(idle, Some((0, 0.0)));
    }

    #[test]
    fn the_other_presets_are_not_the_downloads_and_leave_it_running() {
        let mut download = Some((5, 0.3));
        for preset in [Preset::Message, Preset::Critical, Preset::Reminder] {
            let outgoing = preset_request(&mut download, &preset, Lang::En).expect("plain presets always send");
            assert!(!outgoing.download, "{preset:?}");
        }
        assert_eq!(download, Some((5, 0.3)));
    }

    #[test]
    fn a_reply_with_a_new_id_moves_the_download_slot_and_leaves_the_drafts_last_alone() {
        let mut catalog = Catalog::initial(Lang::En);
        catalog.last = Some(3);
        catalog.download = Some((5, 0.07));
        catalog.on_sent(true, Ok(9), Lang::En);
        assert_eq!(catalog.download, Some((9, 0.07)));
        assert_eq!(catalog.last, Some(3));
        catalog.on_sent(false, Ok(4), Lang::En);
        assert_eq!(catalog.last, Some(4));
        assert_eq!(slot_id(&catalog), Some(9));
    }

    #[test]
    fn the_first_reply_gives_a_pending_download_its_id_and_a_late_one_after_it_ended_is_ignored() {
        let mut catalog = Catalog::initial(Lang::En);
        catalog.download = Some((0, 0.0));
        catalog.on_sent(true, Ok(5), Lang::En);
        assert_eq!(catalog.download, Some((5, 0.0)));
        assert_eq!(catalog.last, None);

        catalog.download = None;
        catalog.on_sent(true, Ok(9), Lang::En);
        assert_eq!(catalog.download, None);
        assert_eq!(catalog.last, None, "a download's reply never becomes the draft's last id");
    }

    #[test]
    fn a_failed_draft_send_keeps_the_download_and_a_failed_download_send_stops_it() {
        let reason = "created too many similar notifications in quick succession";
        let mut catalog = Catalog::initial(Lang::En);
        catalog.download = Some((5, 0.35));
        let toast = catalog.on_sent(false, Err(reason.into()), Lang::En).expect("a failed send is announced");
        assert_eq!(catalog.download, Some((5, 0.35)), "the draft's failure is not the download's");
        assert!(toast.contains(reason), "the toast carries the server's own words: {toast}");
        assert!(!toast.contains("No notification server"));
        assert!(catalog.log[0].contains(reason));

        let toast = catalog.on_sent(true, Err("the bus went away".into()), Lang::En);
        assert_eq!(catalog.download, None);
        assert!(toast.is_some_and(|text| text.contains("the bus went away")));
    }

    #[test]
    fn a_close_signal_stops_the_download_only_for_its_own_live_id() {
        let mut catalog = Catalog::initial(Lang::En);
        catalog.download = Some((0, 0.0));
        catalog.on_closed(0);
        assert_eq!(catalog.download, Some((0, 0.0)), "a download without an id is not any notification");
        catalog.download = Some((9, 0.5));
        catalog.on_closed(4);
        assert_eq!(catalog.download, Some((9, 0.5)));
        catalog.on_closed(9);
        assert_eq!(catalog.download, None);
    }

    #[test]
    fn a_closed_draft_no_longer_enables_replace_or_withdraw() {
        let mut catalog = Catalog::initial(Lang::En);
        catalog.last = Some(7);
        catalog.on_closed(8);
        assert_eq!(catalog.last, Some(7));
        catalog.on_closed(7);
        assert_eq!(catalog.last, None);
    }

    #[test]
    fn the_messages_walk_the_download_through_a_server_that_forgets_its_id() {
        let mut harness = bench();
        harness.message(Message::Preset(Preset::Download));
        assert_eq!(harness.app().download, Some((0, 0.0)));
        harness.message(Message::Tick);
        assert_eq!(harness.app().download, Some((0, 0.0)), "no tick before the first id");
        harness.message(Message::Sent(true, Ok(5)));
        harness.message(Message::Tick);
        assert_eq!(slot_id(harness.app()), Some(5));
        harness.message(Message::Sent(true, Ok(9)));
        assert_eq!(slot_id(harness.app()), Some(9));
        assert_eq!(harness.app().last, None);
        harness.message(Message::Sent(false, Ok(3)));
        assert_eq!(harness.app().last, Some(3));
        assert_eq!(slot_id(harness.app()), Some(9));
        harness.message(Message::Withdraw);
        assert_eq!(harness.app().last, None);
        assert_eq!(slot_id(harness.app()), Some(9), "withdrawing the draft leaves the download alone");
        harness.message(Message::Event(NotificationEvent::Closed { id: 9, reason: Closed::Dismissed }));
        assert_eq!(harness.app().download, None, "dismissing the download ends it");
        harness.message(Message::Tick);
        assert_eq!(harness.app().download, None);
    }

    #[test]
    fn a_system_look_with_a_chosen_accent_keeps_following_the_shells_appearance() {
        let mut harness = bench();
        let initial = harness.shell().paint().tokens.appearance;
        let other = opposite(initial);
        harness.message(Message::Accent(Some(Accent::Teal)));
        assert_eq!(harness.shell().previewing(), Some((initial, Accent::Teal)));
        harness.send(Event::Look(shell_look(other, Accent::Blue)));
        assert_eq!(harness.shell().previewing(), Some((other, Accent::Teal)));
        assert_eq!(harness.shell().paint().tokens.appearance, other);
    }

    #[test]
    fn a_chosen_appearance_with_the_system_accent_keeps_following_the_shells_accent() {
        let mut harness = bench();
        let accent = harness.shell().look().accent;
        let moved = if accent == Accent::Teal { Accent::Blue } else { Accent::Teal };
        harness.message(Message::Look(Look::Dark));
        assert_eq!(harness.shell().previewing(), Some((Appearance::Dark, accent)));
        harness.send(Event::Look(shell_look(Appearance::Light, moved)));
        assert_eq!(harness.shell().previewing(), Some((Appearance::Dark, moved)));
        assert_eq!(harness.shell().paint().tokens.appearance, Appearance::Dark);
    }

    #[test]
    fn revealing_the_secret_field_leaves_the_password_field_masked() {
        let mut harness = bench();
        harness.message(Message::RevealSecret);
        assert!(harness.app().secret_revealed && !harness.app().revealed);
        harness.message(Message::Reveal);
        harness.message(Message::RevealSecret);
        assert!(harness.app().revealed && !harness.app().secret_revealed);
    }

    #[test]
    fn without_a_preview_the_window_follows_the_shell_outright() {
        let mut harness = bench();
        let other = opposite(harness.shell().paint().tokens.appearance);
        harness.message(Message::Accent(Some(Accent::Teal)));
        harness.message(Message::Accent(None));
        assert_eq!(harness.shell().previewing(), None);
        harness.send(Event::Look(shell_look(other, Accent::Blue)));
        assert_eq!(harness.shell().previewing(), None);
        assert_eq!(harness.shell().paint().tokens.appearance, other);
    }

    #[test]
    fn the_catalogue_menu_lists_every_page_and_gives_shortcuts_to_the_first_ten() {
        let harness = bench();
        let lang = harness.shell().lang();
        let items = &harness.menus()[0].items;
        assert_eq!(items.len(), Page::ALL.len(), "the menu is also the top-bar menu: no page may be missing");
        for (index, item) in items.iter().enumerate() {
            let Item::Action { label, shortcut, message, .. } = item else {
                panic!("page entries are actions: {item:?}");
            };
            assert!(matches!(message, Message::Page(page) if *page == index), "{label} opens page {index}");
            assert_eq!(label, Page::ALL[index].title(lang));
            assert_eq!(shortcut.is_some(), index < 10, "{label}");
        }
        let chords: Vec<&str> = items
            .iter()
            .filter_map(|item| match item {
                Item::Action { shortcut, .. } => shortcut.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(
            chords,
            ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+5", "Ctrl+6", "Ctrl+7", "Ctrl+8", "Ctrl+9", "Ctrl+0"]
        );
    }

    #[test]
    fn shortcuts_stop_after_ctrl_zero() {
        assert_eq!(page_shortcut(0).as_deref(), Some("Ctrl+1"));
        assert_eq!(page_shortcut(9).as_deref(), Some("Ctrl+0"));
        assert_eq!(page_shortcut(10), None);
        assert_eq!(page_shortcut(13), None);
    }

    #[test]
    fn dropdown_labels_are_translated_and_distinct_within_a_language() {
        for lang in LANGS {
            let timeouts: HashSet<&str> = Timeout::ALL.iter().map(|timeout| timeout.label(lang)).collect();
            assert_eq!(timeouts.len(), Timeout::ALL.len());
            let fruits: HashSet<&str> = Fruit::ALL.iter().map(|fruit| fruit.label(lang)).collect();
            assert_eq!(fruits.len(), Fruit::ALL.len());
        }
        for timeout in Timeout::ALL {
            assert_ne!(timeout.label(Lang::Uk), timeout.label(Lang::En), "{timeout:?} is translated");
        }
        for fruit in Fruit::ALL {
            assert_ne!(fruit.label(Lang::Uk), fruit.label(Lang::En), "{fruit:?} is translated");
        }
    }

    #[test]
    fn dropdown_options_show_the_label_in_the_ui_language_and_match_by_value() {
        let timeouts: Vec<String> = timeout_options(Lang::Uk).iter().map(ToString::to_string).collect();
        assert_eq!(timeouts, ["Типово для сервера", "3 с", "10 с", "Ніколи"]);
        let fruits: Vec<String> = fruit_options(Lang::En).iter().map(ToString::to_string).collect();
        assert_eq!(fruits, ["🍎 Apple", "🍐 Pear", "🟣 Plum"]);
        assert_eq!(timeout_option(Timeout::Never, Lang::Uk), timeout_option(Timeout::Never, Lang::En));
        assert_ne!(timeout_option(Timeout::Short, Lang::En), timeout_option(Timeout::Long, Lang::En));
    }

    #[test]
    fn close_reasons_are_words_in_the_ui_language_not_debug_names() {
        let line = |reason, lang| event_line(&NotificationEvent::Closed { id: 7, reason }, lang);
        for reason in [Closed::Expired, Closed::Dismissed, Closed::Withdrawn, Closed::Other] {
            assert_ne!(closed_label(reason, Lang::Uk), closed_label(reason, Lang::En), "{reason:?} is translated");
            for lang in LANGS {
                assert!(!line(reason, lang).contains(&format!("{reason:?}")), "{reason:?}");
            }
        }
        assert_eq!(line(Closed::Expired, Lang::Uk), "#7 закрито (минув час)");
        assert_eq!(line(Closed::Expired, Lang::En), "#7 closed (expired)");
    }

    #[test]
    fn the_event_log_speaks_the_ui_language_for_actions_and_replies() {
        let action = NotificationEvent::Action { id: 2, key: "later".into() };
        let reply = NotificationEvent::Replied { id: 3, text: "ok".into() };
        assert_eq!(event_line(&action, Lang::Uk), "#2 дія «later»");
        assert_eq!(event_line(&action, Lang::En), "#2 action «later»");
        assert_eq!(event_line(&reply, Lang::Uk), "#3 відповідь «ok»");
        assert_eq!(event_line(&reply, Lang::En), "#3 reply «ok»");
    }

    #[test]
    fn the_failure_toast_is_translated_and_keeps_the_servers_error() {
        let error = "created too many similar notifications";
        let uk = send_failed(error, Lang::Uk);
        let en = send_failed(error, Lang::En);
        assert_ne!(uk, en);
        assert!(uk.ends_with(error) && en.ends_with(error));
    }
}
