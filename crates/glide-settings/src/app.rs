use std::time::Duration;

use iced::widget::{column, container, row};
use iced::{Element, Fill, Subscription, Task};

use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::panels::{
    Category, Ctx, PageEvent, Pane, Panel, about, accessibility, appearance, apps, bluetooth, date_time, desktop,
    displays, gestures, home, keyboard, lock_screen, miracast, mouse, multitasking, netbird, network, night_light,
    notifications, passwords, power, printers, privacy, region, screen_share, search, shortcuts, sound, startup,
    storage, topbar, users, vpn, wallpaper, windows, with_panels,
};
use crate::services::config::ConfigFault;
use crate::services::config::ConfigStore;
use crate::services::ipc::{self, IpcEvent, ReloadError};
use crate::services::run::blocking;
use crate::services::system;
use crate::sidebar;
use crate::theme::{self, ThemeState};
use crate::widgets;

pub const SEARCH_ID: &str = "glide-settings-search";
pub(crate) const CONTENT_SCROLL_ID: &str = "glide-settings-content";

macro_rules! declare_pages {
    ($($variant:ident => $module:ident,)*) => {
        #[derive(Debug, Clone)]
        pub enum Message {
            PanelSelected(Panel),
            CategorySelected(Category),
            SearchChanged(String),
            SearchSubmitted,
            FocusSearch,
            ClearSearch,
            SelectNext,
            SelectPrevious,
            Reloaded(Option<Result<(), ReloadError>>),
            Ipc(IpcEvent),
            Tick,
            Clock,
            DismissNotice,
            $($variant($module::Msg),)*
        }

        #[derive(Default)]
        struct Pages {
            $($module: $module::Page,)*
        }

        impl App {
            fn open_page(&mut self, panel: Panel) -> Task<Message> {
                let c = self.ctx();
                match panel {
                    $(Panel::$variant => Pane::open(&mut self.pages.$module, c, &self.config).map(Message::$variant),)*
                }
            }

            fn leave_page(&mut self, panel: Panel) {
                match panel {
                    $(Panel::$variant => Pane::leave(&mut self.pages.$module, &mut self.config),)*
                }
            }

            fn tick_page(&mut self, panel: Panel) -> Task<Message> {
                let c = self.ctx();
                match panel {
                    $(Panel::$variant => Pane::tick(&mut self.pages.$module, c).map(Message::$variant),)*
                }
            }

            fn broadcast(&mut self, event: PageEvent) -> Task<Message> {
                let c = self.ctx();
                Task::batch([
                    $(Pane::event(&mut self.pages.$module, event, c).map(Message::$variant),)*
                ])
            }

            fn update_page(&mut self, message: Message) -> Task<Message> {
                let c = self.ctx();
                match message {
                    $(Message::$variant(msg) => Pane::update(&mut self.pages.$module, msg, c, &mut self.config).map(Message::$variant),)*
                    _ => Task::none(),
                }
            }

            fn view_panel(&self, panel: Panel, c: Ctx) -> Element<'_, Message> {
                match panel {
                    $(Panel::$variant => Pane::view(&self.pages.$module, c, &self.config).map(Message::$variant),)*
                }
            }

            fn panel_subscription(&self, panel: Panel) -> Subscription<Message> {
                match panel {
                    $(Panel::$variant => Pane::subscription(&self.pages.$module).map(Message::$variant),)*
                }
            }

            #[must_use]
            pub(crate) fn panel_status(&self, panel: Panel) -> Option<String> {
                let c = self.ctx();
                match panel {
                    $(Panel::$variant => Pane::sidebar_status(&self.pages.$module, c),)*
                }
            }
        }
    };
}

with_panels!(declare_pages);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

pub struct App {
    pub theme: ThemeState,
    lang: Lang,
    config: ConfigStore,
    active_panel: Panel,
    search_query: String,
    online: bool,
    notice: Option<Notice>,
    user_name: String,
    login: String,
    pages: Pages,
    netbird: bool,
    width: f32,
}

pub const COMPACT_BELOW: f32 = 940.0;

const CLOCK: Duration = Duration::from_secs(2);

fn appearance_status(config: &ConfigStore, c: &Ctx) -> String {
    let key = if config.appearance_scheduled() {
        crate::i18n::Key::StatusAuto
    } else {
        match config.appearance() {
            crate::theme::Appearance::Light => crate::i18n::Key::Light,
            crate::theme::Appearance::Dark => crate::i18n::Key::Dark,
        }
    };
    c.t(key).to_string()
}

const fn language_status(lang: crate::i18n::Lang) -> &'static str {
    match lang {
        crate::i18n::Lang::Uk => "Українська",
        crate::i18n::Lang::En => "English",
    }
}

#[must_use]
pub fn minute_now() -> Option<u16> {
    let now = crate::services::tz::Zone::system().local(crate::services::tz::now());
    Some(u16::from(now.hour) * 60 + u16::from(now.minute))
}

impl Default for App {
    fn default() -> Self {
        Self::with_config(ConfigStore::default())
    }
}

impl App {
    #[must_use]
    pub fn with_config(config: ConfigStore) -> Self {
        let theme = ThemeState::new(config.appearance_at(minute_now()), config.accent());
        Self {
            theme,
            lang: config.language(),
            config,
            active_panel: Panel::Home,
            search_query: String::new(),
            online: false,
            notice: None,
            user_name: String::new(),
            login: String::new(),
            pages: Pages::default(),
            netbird: false,
            width: 1000.0,
        }
    }

    #[must_use]
    pub const fn compact(&self) -> bool {
        self.width < COMPACT_BELOW
    }

    #[must_use]
    pub fn with_netbird(mut self, installed: bool) -> Self {
        self.netbird = installed;
        self
    }

    pub fn new() -> (Self, Task<Message>) {
        let mut app = Self::with_config(ConfigStore::load()).with_netbird(glide_netbird::installed());
        app.online = ipc::socket_path().is_some();
        app.user_name = system::user_name();
        app.login = std::env::var("USER").unwrap_or_default();
        let args: Vec<String> = std::env::args().skip(1).collect();
        let task = app.open_args(&args);
        let task = if app.active_panel == Panel::Users {
            task
        } else {
            let c = app.ctx();
            Task::batch([task, Pane::open(&mut app.pages.users, c, &app.config).map(Message::Users)])
        };
        (app, task)
    }

    #[must_use]
    pub fn title(&self) -> String {
        format!("{} — {}", self.active_category().label(self.lang), K::Settings.text(self.lang))
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        self.theme.paint.iced_theme()
    }

    #[must_use]
    pub const fn active_panel(&self) -> Panel {
        self.active_panel
    }

    #[must_use]
    pub const fn lang(&self) -> Lang {
        self.lang
    }

    #[must_use]
    pub const fn config(&self) -> &ConfigStore {
        &self.config
    }

    #[must_use]
    pub(crate) fn search_query(&self) -> &str {
        &self.search_query
    }

    #[must_use]
    pub const fn notice(&self) -> Option<&Notice> {
        self.notice.as_ref()
    }

    #[must_use]
    pub const fn online(&self) -> bool {
        self.online
    }

    #[must_use]
    pub const fn is_recording(&self) -> bool {
        matches!(self.active_panel, Panel::Shortcuts) && self.pages.shortcuts.is_recording()
    }

    #[must_use]
    pub(crate) fn current_account(&self) -> Option<&crate::services::accounts::Account> {
        self.pages.users.state().me.as_ref()
    }

    #[must_use]
    pub fn active_category(&self) -> Category {
        Category::of(self.active_panel)
    }

    #[must_use]
    pub fn category_panels(&self, category: Category) -> Vec<Panel> {
        category.panels().iter().copied().filter(|panel| *panel != Panel::Netbird || self.netbird).collect()
    }

    #[must_use]
    pub fn visible_categories(&self) -> Vec<Category> {
        Category::GROUPS
            .iter()
            .flat_map(|group| group.categories.iter().copied())
            .filter(|category| category.matches(&self.search_query))
            .collect()
    }

    #[must_use]
    pub(crate) fn category_status(&self, category: Category) -> Option<String> {
        let c = self.ctx();
        self.category_panels(category)
            .into_iter()
            .find_map(|panel| self.panel_status(panel).or_else(|| self.pages.home.status_of(panel, c)))
            .or_else(|| match category {
                Category::Appearance => Some(appearance_status(&self.config, &c)),
                Category::Region => Some(language_status(self.config.language()).to_string()),
                _ => None,
            })
    }

    fn search_target(&self, category: Category) -> Panel {
        category
            .matched_panel(&self.search_query)
            .filter(|panel| self.category_panels(category).contains(panel))
            .unwrap_or_else(|| category.first())
    }

    const fn ctx(&self) -> Ctx {
        Ctx {
            theme: self.theme.compact(self.compact()),
            lang: self.lang,
            writable: self.config.writable(),
            online: self.online,
        }
    }

    pub fn follow_look(&mut self, look: &glide_sdk::look::Look, paint: glide_sdk::Paint) {
        self.theme.paint = paint;
        self.lang = look.language.unwrap_or_else(|| self.config.language());
    }

    pub fn follow_window(&mut self, window: glide_sdk::window::Window) {
        self.width = window.size.width;
    }

    fn apply_edited_look(&mut self) {
        self.theme.set_appearance(self.config.appearance_at(minute_now()));
        self.theme.set_accent(self.config.accent());
        self.lang = self.config.language();
        if self.config.path().is_some() {
            theme::use_ui_family(self.config.optional_str("general", "font-family").as_deref());
        }
    }

    fn shown_panels(&self) -> Vec<Panel> {
        let category = self.active_category();
        if category.stacked() { self.category_panels(category) } else { vec![self.active_panel] }
    }

    fn select(&mut self, panel: Panel) -> Task<Message> {
        let switched = self.active_panel != panel;
        let category_switched = Category::of(panel) != self.active_category();
        if switched {
            for shown in self.shown_panels() {
                if category_switched || !self.active_category().stacked() {
                    self.leave_page(shown);
                }
            }
        }
        self.active_panel = panel;
        let open = Task::batch(self.shown_panels().into_iter().map(|shown| self.open_page(shown)).collect::<Vec<_>>());
        let open = self.commit(open);
        if switched {
            Task::batch([
                open,
                iced::widget::operation::snap_to(CONTENT_SCROLL_ID, iced::widget::scrollable::RelativeOffset::START),
            ])
        } else {
            open
        }
    }

    pub fn open_args(&mut self, args: &[String]) -> Task<Message> {
        let panel = args.first().and_then(|arg| Panel::from_slug(arg)).unwrap_or(self.active_panel);
        if panel == Panel::Netbird
            && let Some(section) = args.get(1)
        {
            self.pages.netbird.show_tab(section);
        }
        self.select(panel)
    }

    fn step(&mut self, forward: bool) -> Task<Message> {
        let visible = self.visible_categories();
        if visible.is_empty() {
            return Task::none();
        }
        let current = self.active_category();
        let next = match visible.iter().position(|category| *category == current) {
            Some(index) if forward => visible[(index + 1) % visible.len()],
            Some(index) => visible[(index + visible.len() - 1) % visible.len()],
            None => visible[0],
        };
        self.select(self.search_target(next))
    }

    fn commit(&mut self, task: Task<Message>) -> Task<Message> {
        if self.config.is_dirty() {
            self.apply_edited_look();
        }
        match self.config.save_dirty() {
            Ok(false) => task,
            Ok(true) => Task::batch([task, Task::perform(blocking(ipc::reload), Message::Reloaded)]),
            Err(err) => {
                self.notice = Some(Notice { text: format!("{}: {err}", K::SaveFailed.text(self.lang)), error: true });
                task
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Home(home::Msg::Undo(change)) => match self.config.undo_history(&change) {
                Ok(true) => {
                    self.apply_edited_look();
                    Task::perform(blocking(ipc::reload), Message::Reloaded)
                }
                Ok(false) => Task::none(),
                Err(err) => {
                    self.notice =
                        Some(Notice { text: format!("{}: {err}", K::SaveFailed.text(self.lang)), error: true });
                    Task::none()
                }
            },
            Message::PanelSelected(panel)
            | Message::Home(home::Msg::Open(panel))
            | Message::LockScreen(lock_screen::Msg::Open(panel))
            | Message::Power(power::Msg::Open(panel))
            | Message::ScreenShare(screen_share::Msg::Open(panel))
            | Message::Miracast(miracast::Msg::Open(panel))
            | Message::Privacy(privacy::Msg::Open(panel))
            | Message::Passwords(passwords::Msg::Open(panel))
            | Message::Search(search::Msg::Open(panel))
            | Message::TopBar(topbar::Msg::Open(panel)) => self.select(panel),
            Message::CategorySelected(category) => {
                let panel =
                    if self.search_query.trim().is_empty() { category.first() } else { self.search_target(category) };
                self.select(panel)
            }
            Message::SearchChanged(query) => {
                self.search_query = query;
                Task::none()
            }
            Message::SearchSubmitted => match self.visible_categories().first() {
                Some(&category) => self.select(self.search_target(category)),
                None => Task::none(),
            },
            Message::FocusSearch => iced::widget::operation::focus(SEARCH_ID),
            Message::ClearSearch => {
                self.search_query.clear();
                Task::none()
            }
            Message::SelectNext => self.step(true),
            Message::SelectPrevious => self.step(false),
            Message::Reloaded(result) => match result {
                Some(Ok(())) => {
                    self.online = true;
                    self.notice = None;
                    self.broadcast(PageEvent::Reloaded)
                }
                Some(Err(ReloadError::Rejected(message))) => {
                    self.online = true;
                    self.notice =
                        Some(Notice { text: format!("{} {message}", K::ConfigRejected.text(self.lang)), error: true });
                    Task::none()
                }
                Some(Err(ReloadError::Offline)) | None => {
                    self.online = false;
                    self.notice = Some(Notice { text: K::SavedOffline.text(self.lang).to_string(), error: false });
                    Task::none()
                }
            },
            Message::Ipc(event) => match event {
                IpcEvent::Connected => {
                    self.online = true;
                    Task::none()
                }
                IpcEvent::Disconnected => {
                    self.online = false;
                    Task::none()
                }
                IpcEvent::OutputChanged => self.broadcast(PageEvent::OutputsChanged),
                IpcEvent::LayoutChanged(_) => Task::none(),
            },
            Message::Tick => {
                Task::batch(self.shown_panels().into_iter().map(|panel| self.tick_page(panel)).collect::<Vec<_>>())
            }
            Message::Clock => {
                let changed = self.config.refresh_if_changed();
                if changed { self.broadcast(PageEvent::Reloaded) } else { Task::none() }
            }
            Message::DismissNotice => {
                self.notice = None;
                Task::none()
            }
            page => {
                self.config.refresh_if_changed();
                let task = self.update_page(page);
                self.commit(task)
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let shown = self.shown_panels();
        let mut subscriptions =
            vec![Subscription::run(ipc::events).map(Message::Ipc), iced::time::every(CLOCK).map(|_| Message::Clock)];
        subscriptions.extend(shown.iter().map(|panel| self.panel_subscription(*panel)));
        if shown.iter().any(|panel| panel.polls()) {
            subscriptions.push(iced::time::every(Duration::from_secs(4)).map(|_| Message::Tick));
        }
        Subscription::batch(subscriptions)
    }

    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let t = self.theme;
        let paint = crate::widgets::paint(&t);
        let body = row![
            sidebar::view(self, self.ctx(), &self.user_name, &self.login),
            glide_sdk::widget::vertical_separator(&paint),
            self.view_content(),
        ];
        glide_sdk::widget::pane(t.win_bg(), 0.0, body).into()
    }

    fn view_content(&self) -> Element<'_, Message> {
        let c = self.ctx();
        let t = c.theme;
        let shown = self.shown_panels();
        let panel_view: Element<'_, Message> = if self.active_category() == Category::Appearance {
            column![
                appearance::Page::view_before_wallpaper(&c, &self.config).map(Message::Appearance),
                self.pages.wallpaper.view_primary(&c, &self.config).map(Message::Wallpaper),
                self.pages.appearance.view_after_wallpaper(&c, &self.config).map(Message::Appearance),
                self.pages.wallpaper.view_additional(&c, &self.config).map(Message::Wallpaper),
            ]
            .spacing(theme::SECTION_GAP)
            .into()
        } else if shown.len() > 1 {
            iced::widget::Column::with_children(shown.iter().map(|panel| self.view_panel(*panel, c)))
                .spacing(theme::SECTION_GAP * 2.0)
                .into()
        } else {
            self.view_panel(self.active_panel, c)
        };

        let panel = self.active_panel;
        let category = self.active_category();
        let paint = t.paint;
        let mut body = column![glide_sdk::widget::page_header(
            &paint,
            glide_sdk::widget::inked_tile(category.icon(), category.color(), category.ink(), 44.0),
            Some(c.t(category.group().title.unwrap_or(K::Settings))),
            category.label(self.lang),
            Some(category.description(self.lang)),
            None,
        )]
        .spacing(theme::SECTION_GAP);
        if let Some(error) = self.config.error() {
            let lead = match self.config.fault() {
                Some(ConfigFault::Io) => K::ConfigUnreadable,
                _ => K::ConfigReadOnly,
            };
            let message = format!("{} {error}", c.t(lead));
            let banner = widgets::banner(t, crate::theme::Tone::Danger, Icon::Warning, message, None);
            body = body.push(glide_sdk::widget::arriving_over(t.win_bg(), banner));
        }
        if let Some(notice) = &self.notice {
            let banner = widgets::banner(
                t,
                if notice.error { crate::theme::Tone::Danger } else { crate::theme::Tone::Warn },
                if notice.error { Icon::Warning } else { Icon::Info },
                notice.text.as_str(),
                Some(widgets::icon_button(t, Icon::Close, Some(Message::DismissNotice))),
            );
            body = body.push(glide_sdk::widget::arriving_over(t.win_bg(), banner));
        }
        body = body.push(glide_sdk::widget::swapped(panel.slug(), Some(t.win_bg()), panel_view));

        let clamp = container(body.max_width(theme::CONTENT_MAX_WIDTH))
            .padding(
                iced::Padding::from([theme::PAGE_PADDING_TOP, theme::PAGE_PADDING_X]).bottom(theme::PAGE_PADDING_X),
            )
            .center_x(Fill);

        let paint = crate::widgets::paint(&t);
        glide_sdk::widget::pane(
            t.win_bg(),
            0.0,
            glide_sdk::widget::scrollable(&paint, clamp).id(CONTENT_SCROLL_ID).height(Fill),
        )
        .into()
    }
}

impl glide_sdk::App for App {
    type Message = Message;

    fn close_requested(&mut self, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        if self.pages.displays.abandon(&mut self.config) && self.config.save_dirty() == Ok(true) {
            let _ = ipc::reload();
        }
        shell.exit();
        Task::none()
    }

    fn new(_: &mut glide_sdk::Shell<Message>) -> (Self, Task<Message>) {
        Self::new()
    }

    fn look_changed(&mut self, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.follow_look(shell.look(), *shell.paint());
        Task::none()
    }

    fn window_changed(&mut self, shell: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.follow_window(*shell.window());
        Task::none()
    }

    fn key(
        &mut self, event: &iced::keyboard::Event, captured: bool, _: &mut glide_sdk::Shell<Message>,
    ) -> Task<Message> {
        let message = if self.is_recording() {
            shortcuts::recorded(event.clone()).map(Message::Shortcuts)
        } else if captured {
            None
        } else {
            shortcut(event.clone())
        };
        message.map_or_else(Task::none, |message| Self::update(self, message))
    }

    fn reopened(&mut self, args: Vec<String>, _: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        self.open_args(&args)
    }

    fn title(&self, _: &glide_sdk::Shell<Message>) -> String {
        Self::title(self)
    }

    fn update(&mut self, message: Message, _: &mut glide_sdk::Shell<Message>) -> Task<Message> {
        Self::update(self, message)
    }

    fn view<'a>(&'a self, _: &'a glide_sdk::Shell<Message>) -> Element<'a, Message> {
        Self::view(self)
    }

    fn subscription(&self) -> Subscription<Message> {
        Self::subscription(self)
    }

    fn paint(&self, _: &glide_sdk::Shell<Message>) -> glide_sdk::Paint {
        self.theme.paint
    }
}

#[must_use]
pub fn shortcut(event: iced::keyboard::Event) -> Option<Message> {
    use iced::keyboard::{
        Event, Key,
        key::{Code, Named, Physical},
    };
    let Event::KeyPressed { key, physical_key, modifiers, .. } = event else {
        return None;
    };
    let search_key = matches!(key.as_ref(), Key::Character("f" | "k" | "F" | "K"))
        || matches!(physical_key, Physical::Code(Code::KeyF | Code::KeyK));
    if search_key && modifiers.command() && !modifiers.shift() && !modifiers.alt() {
        return Some(Message::FocusSearch);
    }
    let forward = match key.as_ref() {
        Key::Named(Named::Escape) => return Some(Message::ClearSearch),
        Key::Named(Named::PageDown) if modifiers.command() => true,
        Key::Named(Named::PageUp) if modifiers.command() => false,
        Key::Named(Named::ArrowDown) if modifiers.alt() => true,
        Key::Named(Named::ArrowUp) if modifiers.alt() => false,
        Key::Named(Named::Tab) if modifiers.command() => !modifiers.shift(),
        _ => return None,
    };
    Some(if forward { Message::SelectNext } else { Message::SelectPrevious })
}
