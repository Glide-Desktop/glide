use std::time::Duration;

use iced::widget::{column, container, row, rule, scrollable};
use iced::{Element, Fill, Length, Subscription, Task};

use crate::i18n::{Key as K, Lang};
use crate::icons::Icon;
use crate::panels::{
    Ctx, PageEvent, Pane, Panel, about, accessibility, appearance, apps, bluetooth, date_time, desktop, displays,
    gestures, keyboard, mouse, multitasking, netbird, network, night_light, notifications, power, printers, privacy,
    region, shortcuts, sound, startup, storage, topbar, users, vpn, wallpaper, windows, with_panels,
};
use crate::services::config::ConfigFault;
use crate::services::config::ConfigStore;
use crate::services::desktop as desktop_sync;
use crate::services::ipc::{self, IpcEvent, ReloadError};
use crate::services::run::blocking;
use crate::services::system;
use crate::sidebar;
use crate::theme::{self, ThemeState};
use crate::widgets;

pub const SEARCH_ID: &str = "glide-settings-search";

macro_rules! declare_pages {
    ($($variant:ident => $module:ident,)*) => {
        #[derive(Debug, Clone)]
        pub enum Message {
            PanelSelected(Panel),
            SearchChanged(String),
            SearchSubmitted,
            FocusSearch,
            ClearSearch,
            SelectNext,
            SelectPrevious,
            DesktopSynced,
            Reloaded(Option<Result<(), ReloadError>>),
            Ipc(IpcEvent),
            Tick,
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

            fn view_page(&self, c: Ctx) -> Element<'_, Message> {
                match self.active_panel {
                    $(Panel::$variant => Pane::view(&self.pages.$module, c, &self.config).map(Message::$variant),)*
                }
            }

            fn page_subscription(&self) -> Subscription<Message> {
                match self.active_panel {
                    $(Panel::$variant => Pane::subscription(&self.pages.$module).map(Message::$variant),)*
                }
            }

            #[must_use]
            pub fn panel_status(&self, panel: Panel) -> Option<String> {
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
    host_name: String,
    pages: Pages,
    synced: Option<DesktopLook>,
    netbird: bool,
}

type DesktopLook = (theme::Appearance, theme::Accent, Option<String>);

impl Default for App {
    fn default() -> Self {
        Self::with_config(ConfigStore::default())
    }
}

impl App {
    #[must_use]
    pub fn with_config(config: ConfigStore) -> Self {
        let theme = ThemeState::new(config.appearance(), config.accent());
        Self {
            theme,
            lang: config.language(),
            config,
            active_panel: Panel::Appearance,
            search_query: String::new(),
            online: false,
            notice: None,
            user_name: String::new(),
            host_name: String::new(),
            pages: Pages::default(),
            synced: None,
            netbird: false,
        }
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
        app.host_name = system::hostname();
        let initial = std::env::args().nth(1).and_then(|arg| Panel::from_slug(&arg)).unwrap_or(Panel::Appearance);
        if initial == Panel::Netbird
            && let Some(section) = std::env::args().nth(2)
        {
            app.pages.netbird.show_tab(&section);
        }
        let task = app.select(initial);
        (app, task)
    }

    #[must_use]
    pub fn title(&self) -> String {
        String::from(K::Settings.text(self.lang))
    }

    #[must_use]
    pub fn iced_theme(&self) -> iced::Theme {
        theme::meridian_theme(&self.theme)
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
    pub fn search_query(&self) -> &str {
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
    pub fn visible_panels(&self) -> Vec<Panel> {
        Panel::GROUPS
            .iter()
            .flat_map(|group| group.panels.iter().copied())
            .filter(|panel| *panel != Panel::Netbird || self.netbird)
            .filter(|panel| panel.matches(&self.search_query))
            .collect()
    }

    const fn ctx(&self) -> Ctx {
        Ctx { theme: self.theme, lang: self.lang, writable: self.config.writable(), online: self.online }
    }

    fn select(&mut self, panel: Panel) -> Task<Message> {
        self.active_panel = panel;
        self.open_page(panel)
    }

    fn step(&mut self, forward: bool) -> Task<Message> {
        let visible = self.visible_panels();
        if visible.is_empty() {
            return Task::none();
        }
        let next = match visible.iter().position(|panel| *panel == self.active_panel) {
            Some(index) if forward => visible[(index + 1) % visible.len()],
            Some(index) => visible[(index + visible.len() - 1) % visible.len()],
            None => visible[0],
        };
        self.select(next)
    }

    fn sync_desktop(&mut self) -> Task<Message> {
        let wanted: DesktopLook =
            (self.config.appearance(), self.config.accent(), self.config.optional_str("dock", "icon-theme"));
        if self.config.path().is_none() || self.synced.as_ref() == Some(&wanted) {
            return Task::none();
        }
        self.synced = Some(wanted.clone());
        let (appearance, accent, icons) = wanted;
        Task::perform(blocking(move || desktop_sync::sync(appearance, accent, icons.as_deref())), |_| {
            Message::DesktopSynced
        })
    }

    fn commit(&mut self, task: Task<Message>) -> Task<Message> {
        self.theme.set_appearance(self.config.appearance());
        self.theme.set_accent(self.config.accent());
        self.lang = self.config.language();
        if !self.config.take_dirty() {
            return task;
        }
        match self.config.save() {
            Ok(()) => Task::batch([task, Task::perform(blocking(ipc::reload), Message::Reloaded), self.sync_desktop()]),
            Err(err) => {
                self.notice = Some(Notice { text: format!("{}: {err}", K::SaveFailed.text(self.lang)), error: true });
                task
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PanelSelected(panel) => self.select(panel),
            Message::SearchChanged(query) => {
                self.search_query = query;
                Task::none()
            }
            Message::SearchSubmitted => match self.visible_panels().first() {
                Some(&panel) => self.select(panel),
                None => Task::none(),
            },
            Message::FocusSearch => iced::widget::operation::focus(SEARCH_ID),
            Message::ClearSearch => {
                self.search_query.clear();
                Task::none()
            }
            Message::SelectNext => self.step(true),
            Message::SelectPrevious => self.step(false),
            Message::DesktopSynced => Task::none(),
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
            Message::Tick => self.tick_page(self.active_panel),
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
        let mut subscriptions = vec![
            Subscription::run(ipc::events).map(Message::Ipc),
            iced::keyboard::listen().filter_map(shortcut),
            self.page_subscription(),
        ];
        if self.active_panel.polls() {
            subscriptions.push(iced::time::every(Duration::from_secs(4)).map(|_| Message::Tick));
        }
        Subscription::batch(subscriptions)
    }

    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let t = self.theme;
        let divider = rule::vertical(1).style(move |_: &iced::Theme| theme::rule_style(&t));
        container(
            row![sidebar::view(self, self.ctx(), &self.user_name, &self.host_name), divider, self.view_content(),],
        )
        .width(Fill)
        .height(Fill)
        .style(move |_: &iced::Theme| theme::window_container_style(&t))
        .into()
    }

    fn view_content(&self) -> Element<'_, Message> {
        let c = self.ctx();
        let t = c.theme;
        let panel_view = self.view_page(c);

        let panel = self.active_panel;
        let mut body = column![widgets::page_title(
            t,
            panel.icon(),
            panel.color(),
            Some(c.t(panel.group().title)),
            panel.label(self.lang),
            Some(panel.description(self.lang)),
            None,
        )]
        .spacing(theme::SECTION_GAP);
        if let Some(error) = self.config.error() {
            let lead = match self.config.fault() {
                Some(ConfigFault::Io) => K::ConfigUnreadable,
                _ => K::ConfigReadOnly,
            };
            body = body.push(widgets::banner(t, t.danger(), Icon::Warning, format!("{} {error}", c.t(lead)), None));
        }
        if let Some(notice) = &self.notice {
            body = body.push(widgets::banner(
                t,
                if notice.error { t.danger() } else { t.warn() },
                if notice.error { Icon::Warning } else { Icon::Info },
                notice.text.as_str(),
                Some(widgets::icon_button(t, Icon::Close, Some(Message::DismissNotice))),
            ));
        }
        body = body.push(panel_view);

        let clamp = container(body.max_width(theme::CONTENT_MAX_WIDTH))
            .padding(
                iced::Padding::from([theme::PAGE_PADDING_TOP, theme::PAGE_PADDING_X])
                    .bottom(theme::PAGE_PADDING_X * 2.0),
            )
            .center_x(Fill);

        container(
            scrollable(clamp).height(Fill).style(move |_: &iced::Theme, status| theme::scrollable_style(&t, status)),
        )
        .width(Fill)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| theme::content_container_style(&t))
        .into()
    }
}

#[must_use]
pub fn shortcut(event: iced::keyboard::Event) -> Option<Message> {
    use iced::keyboard::{Event, Key, key::Named};
    let Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    let forward = match key.as_ref() {
        Key::Character("f" | "k") if modifiers.command() => return Some(Message::FocusSearch),
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
