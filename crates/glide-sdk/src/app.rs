use std::borrow::Cow;
use std::ffi::OsString;
use std::time::{Duration, Instant, SystemTime};

use glide_app::menubar::{self, Handles, Publisher};
use glide_iced::Paint;
use glide_iced::style::{self, SP2};
use glide_theme::{Accent, Appearance, component, motion};
use iced::border::Radius;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{column, container};
use iced::{Element, Fill, Point, Size, Subscription, Task};

use crate::look::{self, Lang, Look};
use crate::menu::{self, Menu, Popup, Source};
use crate::overlay::{self, Answer, Dialog, Toast};
use crate::widget::separator;
use crate::window::{self, Action, Header, Window};

const TICK: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chrome {
    #[default]
    Standard,
    Own,
    Server,
}

#[derive(Clone)]
pub struct Settings {
    pub app_id: String,
    pub size: Size,
    pub min_size: Option<Size>,
    pub fonts: Vec<Cow<'static, [u8]>>,
    pub chrome: Chrome,
    pub single_instance: Option<String>,
}

impl Settings {
    #[must_use]
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            size: Size::new(960.0, 640.0),
            min_size: Some(Size::new(420.0, 300.0)),
            fonts: Vec::new(),
            chrome: Chrome::Standard,
            single_instance: None,
        }
    }

    #[must_use]
    pub fn single_instance(mut self, bus_name: impl Into<String>) -> Self {
        self.single_instance = Some(bus_name.into());
        self
    }

    #[must_use]
    pub fn chrome(mut self, chrome: Chrome) -> Self {
        self.chrome = chrome;
        self
    }

    #[must_use]
    pub fn fonts(mut self, fonts: impl IntoIterator<Item = &'static [u8]>) -> Self {
        self.fonts.extend(fonts.into_iter().map(Cow::Borrowed));
        self
    }

    #[must_use]
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    #[must_use]
    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = Some(Size::new(width, height));
        self
    }

    #[must_use]
    pub fn font(mut self, bytes: impl Into<Cow<'static, [u8]>>) -> Self {
        self.fonts.push(bytes.into());
        self
    }
}

pub struct Shell<M> {
    app_id: String,
    look: Look,
    paint: Paint,
    lang: Lang,
    window: Window,
    pointer: crate::pointer::PointerCell,
    toast: Option<Toast<M>>,
    dialog: Option<Dialog<M>>,
    closing: Option<Dialog<M>>,
    popup: Option<Popup<M>>,
    actions: Vec<Action>,
    exit: bool,
    top_bar_menu: bool,
    preview: Option<(Appearance, Accent)>,
}

impl<M: Clone> Shell<M> {
    fn new(app_id: String, look: Look) -> Self {
        Self {
            app_id,
            paint: look.paint(),
            lang: look.lang(),
            look,
            window: Window::default(),
            pointer: crate::pointer::PointerCell::default(),
            toast: None,
            dialog: None,
            closing: None,
            popup: None,
            actions: Vec::new(),
            exit: false,
            top_bar_menu: false,
            preview: None,
        }
    }

    #[must_use]
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    #[must_use]
    pub fn look(&self) -> &Look {
        &self.look
    }

    #[must_use]
    pub fn paint(&self) -> &Paint {
        &self.paint
    }

    #[must_use]
    pub fn lang(&self) -> Lang {
        self.lang
    }

    #[must_use]
    pub fn t<'a>(&self, uk: &'a str, en: &'a str) -> &'a str {
        self.lang.pick(uk, en)
    }

    #[must_use]
    pub fn window(&self) -> &Window {
        &self.window
    }

    #[must_use]
    pub fn pointer(&self) -> Point {
        self.pointer.get()
    }

    #[must_use]
    pub fn menu_in_top_bar(&self) -> bool {
        self.top_bar_menu
    }

    #[must_use]
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast::new(text));
    }

    pub fn toast_with(&mut self, text: impl Into<String>, action: impl Into<String>, message: M) {
        self.toast = Some(Toast::new(text).action(action, message));
    }

    pub fn dialog(&mut self, dialog: Dialog<M>) {
        self.popup = None;
        self.dialog = Some(dialog);
    }

    pub fn close_dialog(&mut self) {
        if let Some(mut dialog) = self.dialog.take() {
            dialog.reveal.close(motion::t2());
            self.closing = Some(dialog);
        }
    }

    pub fn context_menu(&mut self, items: Vec<menu::Item<M>>) {
        self.context_menu_at(self.pointer.get(), items);
    }

    pub fn context_menu_at(&mut self, at: Point, items: Vec<menu::Item<M>>) {
        self.popup = Some(Popup::new(Source::Context, items, at));
    }

    pub fn close_menu(&mut self) {
        if let Some(popup) = &mut self.popup {
            popup.reveal.close(motion::t2());
        }
    }

    pub fn window_action(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn preview_look(&mut self, preview: Option<(Appearance, Accent)>) {
        self.preview = preview;
        self.refresh_paint();
    }

    #[must_use]
    pub fn previewing(&self) -> Option<(Appearance, Accent)> {
        self.preview
    }

    fn refresh_paint(&mut self) {
        self.paint = match self.preview {
            Some((appearance, accent)) => Paint::new(appearance, accent),
            None => self.look.paint(),
        };
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }

    fn animating(&self) -> bool {
        let now = Instant::now();
        self.toast.as_ref().is_some_and(|toast| toast.animating(now))
            || self.popup.as_ref().is_some_and(|popup| popup.reveal.is_animating(motion::t2()))
            || self.dialog.as_ref().is_some_and(|dialog| dialog.reveal.is_animating(motion::t2()))
            || self.closing.as_ref().is_some_and(|dialog| dialog.reveal.is_animating(motion::t2()))
    }

    fn settle(&mut self) {
        let duration = motion::t2();
        for reveal in [
            self.popup.as_mut().map(|popup| &mut popup.reveal),
            self.dialog.as_mut().map(|dialog| &mut dialog.reveal),
            self.closing.as_mut().map(|dialog| &mut dialog.reveal),
        ]
        .into_iter()
        .flatten()
        {
            reveal.settle(duration);
        }
        self.popup.take_if(|popup| !popup.reveal.is_open() && !popup.reveal.is_visible(duration));
        self.closing.take_if(|dialog| !dialog.reveal.is_visible(duration));
    }
}

pub trait App: Sized + 'static {
    type Message: Clone + Send + std::fmt::Debug + 'static;

    fn new(shell: &mut Shell<Self::Message>) -> (Self, Task<Self::Message>);

    fn title(&self, shell: &Shell<Self::Message>) -> String;

    fn update(&mut self, message: Self::Message, shell: &mut Shell<Self::Message>) -> Task<Self::Message>;

    fn view<'a>(&'a self, shell: &'a Shell<Self::Message>) -> Element<'a, Self::Message>;

    fn subtitle(&self, _shell: &Shell<Self::Message>) -> Option<String> {
        None
    }

    fn menus(&self, _shell: &Shell<Self::Message>) -> Vec<Menu<Self::Message>> {
        Vec::new()
    }

    fn toolbar<'a>(&'a self, _shell: &'a Shell<Self::Message>) -> Vec<Element<'a, Self::Message>> {
        Vec::new()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn close_requested(&mut self, shell: &mut Shell<Self::Message>) -> Task<Self::Message> {
        shell.exit();
        Task::none()
    }

    fn paint(&self, shell: &Shell<Self::Message>) -> Paint {
        *shell.paint()
    }

    fn look_changed(&mut self, _shell: &mut Shell<Self::Message>) -> Task<Self::Message> {
        Task::none()
    }

    fn window_changed(&mut self, _shell: &mut Shell<Self::Message>) -> Task<Self::Message> {
        Task::none()
    }

    fn key(
        &mut self, _event: &keyboard::Event, _captured: bool, _shell: &mut Shell<Self::Message>,
    ) -> Task<Self::Message> {
        Task::none()
    }

    fn reopened(&mut self, _args: Vec<String>, _shell: &mut Shell<Self::Message>) -> Task<Self::Message> {
        Task::none()
    }
}

#[derive(Debug, Clone)]
pub enum Event<M> {
    App(M),
    Window(Action),
    Look(glide_app::Look),
    Motion(bool),
    Tick,
    Frame,
    Key(keyboard::Event, bool),
    Handles(Option<Handles>),
    MenuBar(menubar::Event),
    Bar(menu::BarEvent),
    Popup(menu::Event),
    Dialog(Answer),
    ToastAction,
    CloseRequested,
    Reopened(crate::instance::Opened),
}

struct Host<A: App> {
    app: A,
    chrome: Chrome,
    single: bool,
    shell: Shell<A::Message>,
    shell_look: Option<glide_app::Look>,
    font_family: Option<String>,
    stamp: Option<SystemTime>,
    handles: Option<Handles>,
    publisher: Option<Publisher>,
    menus: Vec<Menu<A::Message>>,
    exported: Vec<A::Message>,
}

const BAR_START: f32 = SP2 + component::SMALL_CONTROL + SP2;

impl<A: App> Host<A> {
    fn boot(app_id: &str, chrome: Chrome, single: bool) -> (Self, Task<Event<A::Message>>) {
        let look = Look::load();
        motion::set_scale(look.motion_scale);
        let font_family = look.font_family.clone();
        glide_iced::font::use_ui_family(font_family.as_deref());
        let mut shell = Shell::new(app_id.to_string(), look);
        let (app, task) = A::new(&mut shell);
        if chrome != Chrome::Server {
            let _ = std::thread::Builder::new().name("glide-sdk-frame".into()).spawn(|| {
                if let Err(err) = crate::system::frame_windows() {
                    tracing::debug!("the shell does not frame this window: {err}");
                }
            });
        }
        let mut host = Self {
            app,
            chrome,
            single,
            shell,
            shell_look: None,
            font_family,
            stamp: look::stamp(look::shell_path().as_deref()),
            handles: None,
            publisher: None,
            menus: Vec::new(),
            exported: Vec::new(),
        };
        host.refresh_menus();
        let handles = window::handles().map(Event::Handles);
        (host, Task::batch([task.map(Event::App), handles]))
    }

    fn apply_look(&mut self) {
        let look = Look::load().with_shell(self.shell_look.as_ref());
        motion::set_scale(look.motion_scale);
        if self.font_family != look.font_family {
            glide_iced::font::use_ui_family(look.font_family.as_deref());
            self.font_family.clone_from(&look.font_family);
        }
        self.shell.lang = look.lang();
        self.shell.look = look;
        self.shell.refresh_paint();
    }

    fn refresh_menus(&mut self) {
        self.menus = self.app.menus(&self.shell);
        if let Some(publisher) = &self.publisher {
            let (entries, actions) = menu::export(&self.menus);
            self.exported = actions;
            publisher.set(entries);
        }
    }

    fn look_changed(&mut self) -> Task<Event<A::Message>> {
        self.app.look_changed(&mut self.shell).map(Event::App)
    }

    fn run(&mut self, message: A::Message) -> Task<Event<A::Message>> {
        self.app.update(message, &mut self.shell).map(Event::App)
    }

    fn update(&mut self, event: Event<A::Message>) -> Task<Event<A::Message>> {
        let quiet = matches!(event, Event::Frame | Event::Tick);
        let task = self.handle(event);
        if !quiet {
            self.refresh_menus();
        }
        let actions: Vec<Task<Event<A::Message>>> = std::mem::take(&mut self.shell.actions)
            .into_iter()
            .map(|action| self.shell.window.update(action).map(Event::Window))
            .collect();
        let exit = if self.shell.exit { iced::exit() } else { Task::none() };
        Task::batch([task, Task::batch(actions), exit])
    }

    fn handle(&mut self, event: Event<A::Message>) -> Task<Event<A::Message>> {
        match event {
            Event::App(message) => self.run(message),
            Event::Window(action) => {
                if matches!(action, Action::Drag | Action::ToggleMaximize) {
                    self.shell.close_menu();
                }
                let task = self.shell.window.update(action).map(Event::Window);
                if matches!(action, Action::Resized(_) | Action::Focused(_) | Action::Framed(_)) {
                    let changed = self.app.window_changed(&mut self.shell).map(Event::App);
                    return Task::batch([task, changed]);
                }
                task
            }
            Event::Look(look) => {
                self.shell_look = Some(look);
                self.apply_look();
                self.look_changed()
            }
            Event::Motion(_) => {
                self.apply_look();
                self.look_changed()
            }
            Event::Tick => {
                let now = Instant::now();
                self.shell.toast.take_if(|toast| toast.expired(now));
                let stamp = look::stamp(look::shell_path().as_deref());
                if stamp != self.stamp {
                    self.stamp = stamp;
                    self.apply_look();
                    return self.look_changed();
                }
                if self.shell.preview.is_none()
                    && self.shell.look.appearance_now() != self.shell.paint.tokens.appearance
                {
                    self.shell.refresh_paint();
                    return self.look_changed();
                }
                Task::none()
            }
            Event::Frame => {
                self.shell.settle();
                Task::none()
            }
            Event::Key(event, captured) => self.on_key(&event, captured),
            Event::Handles(handles) => {
                self.handles = handles;
                Task::none()
            }
            Event::MenuBar(event) => self.on_menu_bar(event),
            Event::Bar(event) => {
                self.on_bar(event);
                Task::none()
            }
            Event::Popup(event) => self.on_popup(event),
            Event::Dialog(answer) => self.on_answer(answer),
            Event::ToastAction => match self.shell.toast.take().and_then(|toast| toast.action) {
                Some((_, message)) => self.run(message),
                None => Task::none(),
            },
            Event::CloseRequested => self.app.close_requested(&mut self.shell).map(Event::App),
            Event::Reopened(opened) => {
                if let Some(handles) = self.handles.filter(|_| !opened.token.is_empty()) {
                    let token = opened.token;
                    let _ = std::thread::Builder::new().name("glide-sdk-activate".into()).spawn(move || {
                        if let Err(err) = glide_app::activation::activate(handles, &token) {
                            tracing::debug!("could not activate the window: {err}");
                        }
                    });
                }
                self.shell.window_action(Action::Activate);
                self.app.reopened(opened.args, &mut self.shell).map(Event::App)
            }
        }
    }

    fn on_menu_bar(&mut self, event: menubar::Event) -> Task<Event<A::Message>> {
        match event {
            menubar::Event::Ready(publisher) => {
                self.publisher = Some(publisher);
                self.refresh_menus();
            }
            menubar::Event::Shown(shown) => {
                self.shell.top_bar_menu = shown;
                if shown && self.shell.popup.as_ref().is_some_and(|popup| matches!(popup.source, Source::Bar(_))) {
                    self.shell.close_menu();
                }
            }
            menubar::Event::Activated(id) => {
                if let Some(message) = menu::exported(&self.exported, id) {
                    return self.run(message);
                }
            }
        }
        Task::none()
    }

    fn open_bar(&mut self, index: usize) {
        let Some(menu) = self.menus.get(index) else { return };
        let at = Point::new(menu::bar_x(&self.menus, BAR_START, index), window::HEADER - 6.0);
        self.shell.popup = Some(Popup::new(Source::Bar(index), menu.items.clone(), at));
    }

    fn on_bar(&mut self, event: menu::BarEvent) {
        let open = self.shell.popup.as_ref().filter(|popup| popup.reveal.is_open()).map(|popup| popup.source);
        match event {
            menu::BarEvent::Pressed(index) if open == Some(Source::Bar(index)) => self.shell.close_menu(),
            menu::BarEvent::Pressed(index) => self.open_bar(index),
            menu::BarEvent::Hovered(index) => {
                if matches!(open, Some(Source::Bar(current)) if current != index) {
                    self.open_bar(index);
                }
            }
        }
    }

    fn on_popup(&mut self, event: menu::Event) -> Task<Event<A::Message>> {
        let Some(popup) = self.shell.popup.as_mut() else { return Task::none() };
        match event {
            menu::Event::Hovered(index) => popup.selected = Some(index),
            menu::Event::Dismiss => self.shell.close_menu(),
            menu::Event::Picked(index) => {
                let outcome = popup.pick(index);
                return self.finish(outcome);
            }
            menu::Event::Stepped(index, more) => {
                popup.selected = Some(index);
                let step = match popup.items.get(index) {
                    Some(menu::Item::Stepper { less, more: next, .. }) => {
                        if more {
                            next.clone()
                        } else {
                            less.clone()
                        }
                    }
                    _ => None,
                };
                return step.map_or_else(Task::none, |message| self.finish(menu::Outcome::Stay(message)));
            }
            menu::Event::Swatched(index, at) => {
                let picked = match popup.items.get(index) {
                    Some(menu::Item::Swatches(swatches)) => swatches.get(at).map(|swatch| swatch.message.clone()),
                    _ => None,
                };
                return picked.map_or_else(Task::none, |message| self.finish(menu::Outcome::Stay(message)));
            }
        }
        Task::none()
    }

    fn finish(&mut self, outcome: menu::Outcome<A::Message>) -> Task<Event<A::Message>> {
        match outcome {
            menu::Outcome::None => Task::none(),
            menu::Outcome::Close => {
                self.shell.close_menu();
                Task::none()
            }
            menu::Outcome::Run(message) => {
                self.shell.close_menu();
                self.run(message)
            }
            menu::Outcome::Stay(message) => {
                let task = self.run(message);
                self.refresh_menus();
                self.refresh_popup();
                task
            }
        }
    }

    fn refresh_popup(&mut self) {
        let Some(popup) = self.shell.popup.as_mut() else { return };
        if let menu::Source::Bar(index) = popup.source
            && let Some(menu) = self.menus.get(index)
            && popup.depth() == 0
        {
            popup.items.clone_from(&menu.items);
        }
    }

    fn on_answer(&mut self, answer: Answer) -> Task<Event<A::Message>> {
        let Some(dialog) = &self.shell.dialog else { return Task::none() };
        let message = match answer {
            Answer::Chose(index) => dialog.choices.get(index).and_then(|choice| choice.message.clone()),
            Answer::Dismiss => None,
        };
        self.shell.close_dialog();
        message.map_or_else(Task::none, |message| self.run(message))
    }

    fn on_key(&mut self, event: &keyboard::Event, captured: bool) -> Task<Event<A::Message>> {
        match self.host_key(event, captured) {
            Some(task) => task,
            None => self.app.key(event, captured, &mut self.shell).map(Event::App),
        }
    }

    fn host_key(&mut self, event: &keyboard::Event, captured: bool) -> Option<Task<Event<A::Message>>> {
        let keyboard::Event::KeyPressed { key, physical_key, modifiers, .. } = event else { return None };
        if let Some(dialog) = &self.shell.dialog {
            return Some(dialog.key(key).map_or_else(Task::none, |answer| self.on_answer(answer)));
        }
        if let Some(popup) = self.shell.popup.as_mut().filter(|popup| popup.reveal.is_open()) {
            let source = popup.source;
            if let Some(outcome) = popup.key_with(key, modifiers.shift()) {
                return Some(self.finish(outcome));
            }
            if let (Source::Bar(index), Key::Named(Named::ArrowLeft | Named::ArrowRight)) = (source, key) {
                let count = self.menus.len().max(1);
                let next = if matches!(key, Key::Named(Named::ArrowRight)) {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                };
                self.open_bar(next);
            }
            return Some(Task::none());
        }
        if *key == Key::Named(Named::F10) && modifiers.is_empty() && !self.shell.top_bar_menu && !self.menus.is_empty()
        {
            self.open_bar(0);
            if let Some(popup) = &mut self.shell.popup {
                popup.step(true);
            }
            return Some(Task::none());
        }
        if captured || self.chrome != Chrome::Standard {
            return None;
        }
        if *key == Key::Named(Named::Tab) && !modifiers.control() && !modifiers.alt() && !modifiers.logo() {
            return Some(if modifiers.shift() {
                iced::widget::operation::focus_previous()
            } else {
                iced::widget::operation::focus_next()
            });
        }
        let message = menu::shortcut(&self.menus, key, *physical_key, *modifiers)?;
        Some(self.run(message))
    }

    fn overlays<'a>(&'a self, layers: &mut Vec<Element<'a, Event<A::Message>>>) {
        let shell = &self.shell;
        let (paint, window) = (shell.paint, shell.window);
        if let Some(toast) = &shell.toast {
            layers.push(overlay::toast(&paint, toast, window.size).map(|_| Event::ToastAction));
        }
        if let Some(popup) = &shell.popup {
            layers.push(menu::view(&paint, popup, window.size).map(Event::Popup));
        }
        if let Some(dialog) = shell.dialog.as_ref().or(shell.closing.as_ref()) {
            layers.push(overlay::dialog(&paint, dialog, window.size, window.corner()).map(Event::Dialog));
        }
    }

    fn view(&self) -> Element<'_, Event<A::Message>> {
        if self.chrome != Chrome::Standard {
            let body = crate::pointer::tracked(self.shell.pointer.clone(), self.app.view(&self.shell).map(Event::App));
            let mut layers: Vec<Element<'_, Event<A::Message>>> = vec![body];
            self.overlays(&mut layers);
            return crate::wheel::smooth_wheel(iced::widget::stack(layers));
        }
        let shell = &self.shell;
        let paint = shell.paint;
        let window = shell.window;
        let lang = shell.lang;
        let mut header = Header::new(self.app.title(shell))
            .subtitle(self.app.subtitle(shell))
            .labels(lang.pick("Розгорнути", "Maximize"), lang.pick("Закрити", "Close"));
        let bar = !shell.top_bar_menu && !self.menus.is_empty();
        if bar {
            let open = shell.popup.as_ref().and_then(|popup| match popup.source {
                Source::Bar(index) if popup.reveal.is_open() => Some(index),
                _ => None,
            });
            header = header.leading(menu::bar(&paint, &self.menus, open).map(Event::Bar));
        }
        for element in self.app.toolbar(shell) {
            header = header.trailing(element.map(Event::App));
        }
        let taken = if bar { menu::bar_total(&self.menus) } else { 0.0 };
        let header = window::header(&paint, &window, header, Event::Window, Event::CloseRequested, taken);
        let corner = window.corner();
        let background = paint.win_bg();
        let content = container(self.app.view(shell).map(Event::App))
            .width(Fill)
            .height(Fill)
            .style(move |_| style::rounded(background, Radius::default().bottom_left(corner).bottom_right(corner)));
        let body = crate::pointer::tracked(shell.pointer.clone(), column![header, separator(&paint), content]);
        let mut layers: Vec<Element<'_, Event<A::Message>>> = vec![body];
        if !window.edge_to_edge() {
            layers.push(window::resize_handles(Event::Window));
        }
        self.overlays(&mut layers);
        crate::wheel::smooth_wheel(iced::widget::stack(layers))
    }

    fn subscription(&self) -> Subscription<Event<A::Message>> {
        let mut subscriptions = vec![
            iced::event::listen_with(|event, status, _| match event {
                iced::Event::Keyboard(event) => Some(Event::Key(event, status == iced::event::Status::Captured)),
                iced::Event::Window(iced::window::Event::Focused) => Some(Event::Window(Action::Focused(true))),
                iced::Event::Window(iced::window::Event::Unfocused) => Some(Event::Window(Action::Focused(false))),
                iced::Event::Window(iced::window::Event::Resized(size) | iced::window::Event::Opened { size, .. }) => {
                    Some(Event::Window(Action::Resized(size)))
                }
                _ => None,
            }),
            iced::window::close_requests().map(|_| Event::CloseRequested),
            iced::time::every(TICK).map(|_| Event::Tick),
            Subscription::run(glide_app::look_changes).map(Event::Look),
            self.app.subscription().map(Event::App),
        ];
        if self.shell_look.is_none() {
            subscriptions.push(Subscription::run(look::motion_changes).map(Event::Motion));
        }
        if self.single {
            subscriptions.push(Subscription::run(crate::instance::requests).map(Event::Reopened));
        }
        if let Some(handles) = self.handles.filter(|_| !self.menus.is_empty()) {
            subscriptions.push(Subscription::run_with(handles, menubar::events).map(Event::MenuBar));
        }
        if self.shell.animating() {
            subscriptions.push(iced::window::frames().map(|_| Event::Frame));
        }
        Subscription::batch(subscriptions)
    }
}

#[must_use]
fn forwardable_args<I: IntoIterator<Item = OsString>>(args: I) -> Option<Vec<String>> {
    args.into_iter().map(|arg| arg.into_string().ok()).collect()
}

pub fn run<A: App>(settings: Settings) -> iced::Result {
    let single = match settings.single_instance.as_deref() {
        Some(name) => match forwardable_args(std::env::args_os().skip(1)) {
            Some(args) => match crate::instance::claim(name, &args, &crate::instance::launch_token()) {
                crate::instance::Claim::Forwarded => return Ok(()),
                crate::instance::Claim::First => true,
                crate::instance::Claim::Alone => false,
            },
            None => false,
        },
        None => false,
    };
    let chrome = settings.chrome;
    let server = chrome == Chrome::Server;
    let window = iced::window::Settings {
        size: settings.size,
        min_size: settings.min_size,
        decorations: server,
        transparent: !server,
        exit_on_close_request: false,
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: settings.app_id.clone(),
            ..iced::window::settings::PlatformSpecific::default()
        },
        ..iced::window::Settings::default()
    };
    let app_id = settings.app_id.clone();
    settings
        .fonts
        .into_iter()
        .fold(
            iced::application(move || Host::<A>::boot(&app_id, chrome, single), Host::<A>::update, Host::<A>::view),
            iced::Application::font,
        )
        .title(|host: &Host<A>| host.app.title(&host.shell))
        .theme(|host: &Host<A>| host.app.paint(&host.shell).iced_theme())
        .style(move |host: &Host<A>, _| {
            let paint = host.app.paint(&host.shell);
            iced::theme::Style {
                background_color: if server { paint.win_bg() } else { iced::Color::TRANSPARENT },
                text_color: paint.fg_1(),
            }
        })
        .subscription(Host::<A>::subscription)
        .window(window)
        .antialiasing(true)
        .run()
}

pub struct Harness<A: App> {
    host: Host<A>,
}

impl<A: App> Harness<A> {
    #[must_use]
    pub fn new(app_id: &str, look: Look, size: Size) -> Self {
        motion::set_scale(look.motion_scale);
        let mut shell = Shell::new(app_id.to_string(), look);
        shell.window.size = size;
        let (app, _) = A::new(&mut shell);
        let mut host = Host {
            app,
            chrome: Chrome::Standard,
            single: false,
            shell,
            shell_look: None,
            font_family: None,
            stamp: None,
            handles: None,
            publisher: None,
            menus: Vec::new(),
            exported: Vec::new(),
        };
        host.refresh_menus();
        Self { host }
    }

    pub fn send(&mut self, event: Event<A::Message>) {
        let _ = self.host.update(event);
    }

    pub fn message(&mut self, message: A::Message) {
        self.send(Event::App(message));
    }

    pub fn point_at(&mut self, at: Point) {
        self.host.shell.pointer.set(at);
    }

    #[must_use]
    pub fn app(&self) -> &A {
        &self.host.app
    }

    #[must_use]
    pub fn shell(&self) -> &Shell<A::Message> {
        &self.host.shell
    }

    #[must_use]
    pub fn menus(&self) -> &[Menu<A::Message>] {
        &self.host.menus
    }

    #[must_use]
    pub fn exiting(&self) -> bool {
        self.host.shell.exit
    }

    #[must_use]
    pub fn view(&self) -> Element<'_, Event<A::Message>> {
        self.host.view()
    }

    #[must_use]
    pub fn theme(&self) -> iced::Theme {
        self.host.shell.paint.iced_theme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::Item;
    use crate::widget::Tone;

    #[derive(Default)]
    struct Notes {
        saved: usize,
        dirty: bool,
        looks: usize,
        windows: usize,
        keys: usize,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Message {
        Save,
        Edit,
        Discard,
    }

    impl App for Notes {
        type Message = Message;

        fn new(_: &mut Shell<Message>) -> (Self, Task<Message>) {
            (Self::default(), Task::none())
        }

        fn title(&self, _: &Shell<Message>) -> String {
            "Notes".into()
        }

        fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
            match message {
                Message::Save => {
                    self.saved += 1;
                    self.dirty = false;
                    shell.toast("Saved");
                }
                Message::Edit => self.dirty = true,
                Message::Discard => shell.exit(),
            }
            Task::none()
        }

        fn view<'a>(&'a self, _: &'a Shell<Message>) -> Element<'a, Message> {
            iced::widget::Space::new().into()
        }

        fn menus(&self, _: &Shell<Message>) -> Vec<Menu<Message>> {
            vec![Menu::new("File", vec![Item::action("Save", Message::Save).shortcut("Ctrl+S").enabled(self.dirty)])]
        }

        fn look_changed(&mut self, _: &mut Shell<Message>) -> Task<Message> {
            self.looks += 1;
            Task::none()
        }

        fn close_requested(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
            if self.dirty {
                shell.dialog(
                    Dialog::new("Unsaved", "").choice("Discard", Tone::Danger, Message::Discard).cancel("Cancel"),
                );
            } else {
                shell.exit();
            }
            Task::none()
        }

        fn window_changed(&mut self, _: &mut Shell<Message>) -> Task<Message> {
            self.windows += 1;
            Task::none()
        }

        fn key(&mut self, _: &keyboard::Event, _: bool, _: &mut Shell<Message>) -> Task<Message> {
            self.keys += 1;
            Task::none()
        }
    }

    fn host() -> Host<Notes> {
        Harness::<Notes>::new("org.example.Notes", Look::default(), Size::new(800.0, 600.0)).host
    }

    fn press(key: Key, physical: keyboard::key::Code, modifiers: keyboard::Modifiers) -> Event<Message> {
        pressed(key, physical, modifiers, false)
    }

    fn pressed(
        key: Key, physical: keyboard::key::Code, modifiers: keyboard::Modifiers, captured: bool,
    ) -> Event<Message> {
        Event::Key(
            keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                physical_key: keyboard::key::Physical::Code(physical),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            },
            captured,
        )
    }

    #[test]
    fn a_key_a_focused_widget_consumed_runs_no_shortcut() {
        let mut host = host();
        let _ = host.update(Event::App(Message::Edit));
        let s = || Key::Character("s".into());
        let _ = host.update(pressed(s(), keyboard::key::Code::KeyS, keyboard::Modifiers::CTRL, true));
        assert_eq!(host.app.saved, 0, "a chord a focused widget consumed never reaches the menus");
        let _ = host.update(pressed(s(), keyboard::key::Code::KeyS, keyboard::Modifiers::CTRL, false));
        assert_eq!(host.app.saved, 1);
    }

    #[test]
    fn menu_shortcuts_follow_the_state_the_menus_were_built_from() {
        let mut host = host();
        let ctrl_s = || press(Key::Character("s".into()), keyboard::key::Code::KeyS, keyboard::Modifiers::CTRL);
        let _ = host.update(ctrl_s());
        assert_eq!(host.app.saved, 0, "Save is disabled while clean");
        let _ = host.update(Event::App(Message::Edit));
        let _ = host.update(ctrl_s());
        assert_eq!(host.app.saved, 1);
        assert!(host.shell.toast.is_some());
    }

    #[test]
    fn the_app_hears_about_every_new_look() {
        let mut host = host();
        let _ = host.update(Event::Motion(true));
        let _ = host.update(Event::Motion(false));
        assert_eq!(host.app.looks, 2);
    }

    #[test]
    fn the_app_hears_when_the_window_changes_but_not_when_it_asks_for_a_change() {
        let mut host = host();
        let _ = host.update(Event::Window(Action::Resized(Size::new(640.0, 480.0))));
        let _ = host.update(Event::Window(Action::Focused(false)));
        let _ = host.update(Event::Window(Action::Framed(window::Frame::Maximized)));
        assert_eq!(host.app.windows, 3);
        let _ = host.update(Event::Window(Action::Drag));
        let _ = host.update(Event::Window(Action::ToggleMaximize));
        assert_eq!(host.app.windows, 3);
        assert_eq!(host.shell.window().size, Size::new(640.0, 480.0));
        assert!(!host.shell.window().focused);
    }

    #[test]
    fn an_app_with_its_own_chrome_keeps_its_keys() {
        let mut host = host();
        host.chrome = Chrome::Own;
        let _ = host.update(Event::App(Message::Edit));
        let ctrl_s = press(Key::Character("s".into()), keyboard::key::Code::KeyS, keyboard::Modifiers::CTRL);
        let _ = host.update(ctrl_s);
        assert_eq!(host.app.saved, 0, "the app binds its own shortcuts when it draws its own window");
        assert_eq!(host.app.keys, 1, "and the host hands it the key instead");
    }

    #[test]
    fn keys_the_menus_do_not_take_reach_the_app() {
        let mut host = host();
        let _ = host.update(Event::App(Message::Edit));
        let _ = host.update(press(Key::Character("s".into()), keyboard::key::Code::KeyS, keyboard::Modifiers::CTRL));
        assert_eq!((host.app.saved, host.app.keys), (1, 0), "a menu shortcut stays with the menu");
        let _ = host.update(press(Key::Character("q".into()), keyboard::key::Code::KeyQ, keyboard::Modifiers::empty()));
        let _ = host.update(Event::Key(keyboard::Event::ModifiersChanged(keyboard::Modifiers::SHIFT), false));
        assert_eq!(host.app.keys, 2);
    }

    #[test]
    fn a_refused_close_waits_for_the_dialog() {
        let mut host = host();
        let _ = host.update(Event::App(Message::Edit));
        let _ = host.update(Event::CloseRequested);
        assert!(!host.shell.exit);
        assert!(host.shell.has_dialog());
        let _ =
            host.update(press(Key::Named(Named::Escape), keyboard::key::Code::Escape, keyboard::Modifiers::empty()));
        assert!(!host.shell.has_dialog(), "Esc dismisses the dialog");
        let _ = host.update(Event::CloseRequested);
        let _ = host.update(Event::Dialog(Answer::Chose(0)));
        assert!(host.shell.exit, "Discard ran its message");
    }

    #[test]
    fn the_in_window_menu_opens_with_f10_and_runs_with_enter() {
        let mut host = host();
        let _ = host.update(Event::App(Message::Edit));
        let _ = host.update(press(Key::Named(Named::F10), keyboard::key::Code::F10, keyboard::Modifiers::empty()));
        assert!(matches!(host.shell.popup.as_ref().map(|popup| popup.source), Some(Source::Bar(0))));
        let _ = host.update(press(Key::Named(Named::Enter), keyboard::key::Code::Enter, keyboard::Modifiers::empty()));
        assert_eq!(host.app.saved, 1);
        assert!(!host.shell.popup.as_ref().is_some_and(|popup| popup.reveal.is_open()));
    }

    #[test]
    fn the_top_bar_menu_hides_the_in_window_one_and_runs_exported_ids() {
        let mut host = host();
        let _ = host.update(Event::App(Message::Edit));
        let _ = host.update(Event::MenuBar(menubar::Event::Shown(true)));
        assert!(host.shell.menu_in_top_bar());
        let _ = host.update(press(Key::Named(Named::F10), keyboard::key::Code::F10, keyboard::Modifiers::empty()));
        assert!(host.shell.popup.is_none(), "F10 belongs to the top bar now");
        host.exported = menu::export(&host.menus).1;
        let _ = host.update(Event::MenuBar(menubar::Event::Activated(1)));
        assert_eq!(host.app.saved, 1);
    }

    #[test]
    fn arguments_that_are_not_utf8_are_not_forwarded_to_another_instance_and_do_not_panic() {
        let text = |argument: &str| OsString::from(argument);
        assert_eq!(
            forwardable_args([text("power"), text("--flag")]),
            Some(vec!["power".to_string(), "--flag".to_string()])
        );
        assert_eq!(forwardable_args(Vec::new()), Some(Vec::new()));
        let odd: OsString = std::os::unix::ffi::OsStringExt::from_vec(b"caf\xe9".to_vec());
        assert_eq!(forwardable_args([text("power"), odd]), None);
    }
}
