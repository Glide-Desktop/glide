pub mod app;
pub mod i18n;
pub mod icons;
pub mod keys;
pub mod memory;
pub mod menus;
pub mod ocr;
mod portal;
mod still;
pub mod thumbs;
pub mod timeline;
mod update;
pub mod view;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use glide_sdk::glide_theme::Appearance;
use glide_sdk::{Paint, Shell};
use iced::{Element, Subscription, Task, keyboard};

pub use app::App;
use app::{Chrome, Message, Mode};

pub const APP_ID: &str = "glide-clips";

fn argument() -> Option<(String, Option<PathBuf>)> {
    let raw = std::env::args().skip(1).find(|arg| !arg.starts_with("--"))?;
    if let Some(path) = glide_sdk::chooser::uri_path(&raw) {
        return Some((app::uri_of(&path), Some(path)));
    }
    if raw.contains("://") {
        return Some((raw, None));
    }
    let path = std::fs::canonicalize(&raw).unwrap_or_else(|_| PathBuf::from(&raw));
    Some((app::uri_of(&path), Some(path)))
}

impl App {
    fn shell_paint(&mut self, shell: &mut Shell<Message>) {
        self.paint = *shell.paint();
        self.accent = self.paint.tokens.accent;
        self.dark = Paint::new(Appearance::Dark, self.accent);
        let want = self.media.is_some().then_some((Appearance::Dark, self.accent));
        if shell.previewing() != want {
            shell.preview_look(want);
        }
    }
}

impl glide_sdk::App for App {
    type Message = Message;

    fn new(shell: &mut Shell<Message>) -> (Self, Task<Message>) {
        let memory = memory::Memory::load();
        let saver = memory::Saver::spawn(memory::path());
        let paint = *shell.paint();
        let accent = paint.tokens.accent;
        let (mpris, commands) = match glide_video::mpris::Server::start("Clips", APP_ID) {
            Ok((server, commands)) => (Some(server), Some(commands)),
            Err(err) => {
                tracing::debug!("no MPRIS: {err}");
                (None, None)
            }
        };
        let mut app = Self {
            paint,
            dark: Paint::new(Appearance::Dark, accent),
            lang: shell.lang(),
            window: *shell.window(),
            handles: None,
            surface: None,
            pending: argument(),
            memory,
            saver,
            recents: Vec::new(),
            media: None,
            mode: Mode::Play,
            chrome: Chrome { shown: true, moved: Instant::now(), pointer: shell.pointer() },
            remaining: true,
            address: None,
            exporting: None,
            inspector: false,
            ocr: None,
            mpris,
            generation: 0,
            track: 0,
            accent,
            ticks: 0,
            export_done_at: None,
            dock: glide_sdk::system::Dock::new(APP_ID),
            clipboard: None,
            dropping: false,
            hovered_clip: None,
            queued: Vec::new(),
            resized: None,
            pip: false,
            aspect_sent: None,
            blur_sent: None,
        };
        let recents = app.refresh_recents();
        let handles = glide_sdk::window::handles().map(Message::Handles);
        let remote = match commands {
            Some(commands) => Task::run(commands, Message::Mpris),
            None => Task::none(),
        };
        (app, Task::batch([recents, handles, remote]))
    }

    fn title(&self, shell: &Shell<Message>) -> String {
        match &self.media {
            Some(media) => media.title.clone(),
            None => shell.t("Clips", "Clips").to_string(),
        }
    }

    fn update(&mut self, message: Message, shell: &mut Shell<Message>) -> Task<Message> {
        let had = self.media.is_some();
        let task = self.dispatch(message, shell);
        if had != self.media.is_some() {
            self.shell_paint(shell);
        }
        self.sync_blur();
        let mut tasks = vec![task];
        tasks.append(&mut self.queued);
        if self.media.is_none() && had {
            tasks.push(self.refresh_recents());
        }
        Task::batch(tasks)
    }

    fn view<'a>(&'a self, _shell: &'a Shell<Message>) -> Element<'a, Message> {
        view::view(self)
    }

    fn paint(&self, shell: &Shell<Message>) -> Paint {
        if self.media.is_some() { self.dark } else { *shell.paint() }
    }

    fn subscription(&self) -> Subscription<Message> {
        let busy = self.media.is_some() || matches!(self.exporting, Some(app::Exporting::Done(_)));
        let cues = self.media.as_ref().is_some_and(|media| media.sidecar_on && media.playing());
        let every = Duration::from_millis(if cues { 100 } else { 250 });
        if busy { iced::time::every(every).map(|_| Message::Tick) } else { Subscription::none() }
    }

    fn close_requested(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.quit(shell)
    }

    fn look_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        self.lang = shell.lang();
        self.shell_paint(shell);
        Task::none()
    }

    fn window_changed(&mut self, shell: &mut Shell<Message>) -> Task<Message> {
        let before = self.window.size;
        self.window = *shell.window();
        if self.window.size != before {
            let first = self.resized.map_or(before, |(_, first)| first);
            self.resized = Some((std::time::Instant::now(), first));
        }
        self.place();
        self.sync_blur();
        Task::none()
    }

    fn key(&mut self, event: &keyboard::Event, captured: bool, shell: &mut Shell<Message>) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, physical_key, modifiers, .. } = event else { return Task::none() };
        if captured || self.address.is_some() || matches!(self.exporting, Some(app::Exporting::Form(_))) {
            if self.address.is_some() && *key == keyboard::Key::Named(keyboard::key::Named::Escape) {
                self.address = None;
            }
            if matches!(self.exporting, Some(app::Exporting::Form(_)))
                && *key == keyboard::Key::Named(keyboard::key::Named::Escape)
            {
                self.exporting = None;
            }
            return Task::none();
        }
        let context = keys::Context {
            playing: self.media.as_ref().is_some_and(app::Media::playing),
            editing: self.mode != Mode::Play,
            loaded: self.media.is_some(),
        };
        match keys::command(key, *physical_key, *modifiers, context) {
            Some(command) => self.update(Message::Command(command), shell),
            None => Task::none(),
        }
    }

    fn reopened(&mut self, args: Vec<String>, shell: &mut Shell<Message>) -> Task<Message> {
        let Some(raw) = args.into_iter().find(|arg| !arg.starts_with("--")) else { return Task::none() };
        let task = if let Some(path) = glide_sdk::chooser::uri_path(&raw) {
            self.open_path(path, shell)
        } else if raw.contains("://") {
            self.open_uri(raw, None)
        } else {
            self.open_path(PathBuf::from(raw), shell)
        };
        self.shell_paint(shell);
        task
    }
}
