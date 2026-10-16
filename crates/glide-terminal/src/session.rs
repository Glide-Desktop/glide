use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options, Shell};
use iced::futures::channel::mpsc::{self, UnboundedSender};
use iced::futures::{SinkExt, Stream, StreamExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

#[derive(Debug, Clone)]
pub struct Bus(pub(crate) UnboundedSender<(PaneId, Event)>);

#[derive(Clone)]
pub struct Listener {
    pane: PaneId,
    bus: Bus,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.bus.0.unbounded_send((self.pane, event));
    }
}

#[derive(Debug, Clone)]
pub enum Feed {
    Connected(Bus),
    Events(Vec<(PaneId, Event)>),
}

pub fn feed() -> impl Stream<Item = Feed> {
    iced::stream::channel(16, async |mut output| {
        let (sender, mut receiver) = mpsc::unbounded();
        if output.send(Feed::Connected(Bus(sender))).await.is_err() {
            return;
        }
        while let Some(first) = receiver.next().await {
            let mut batch = vec![first];
            while let Ok(next) = receiver.try_recv() {
                batch.push(next);
            }
            if output.send(Feed::Events(batch)).await.is_err() {
                return;
            }
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cells {
    pub columns: usize,
    pub lines: usize,
}

impl Dimensions for Cells {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    pub command: Option<(String, Vec<String>)>,
    pub directory: Option<PathBuf>,
}

pub struct Pane {
    pub id: PaneId,
    pub term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    master: File,
    shell: i32,
    pub cells: Cells,
    pub cell_size: iced::Size,
    pub title: Option<String>,
    pub process: String,
    pub directory: Option<PathBuf>,
    pub busy: bool,
    pub exit: Option<ExitStatus>,
}

const DEFAULT_CELLS: Cells = Cells { columns: 80, lines: 24 };

fn window_size(cells: Cells, cell: iced::Size) -> WindowSize {
    let px = |value: f32| value.round().clamp(1.0, f32::from(u16::MAX)) as u16;
    WindowSize {
        num_lines: cells.lines.min(usize::from(u16::MAX)) as u16,
        num_cols: cells.columns.min(usize::from(u16::MAX)) as u16,
        cell_width: px(cell.width),
        cell_height: px(cell.height),
    }
}

impl Pane {
    pub fn spawn(id: PaneId, bus: &Bus, launch: &Launch, config: Config) -> io::Result<Self> {
        let env = HashMap::from([
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
            ("TERM_PROGRAM".to_string(), "glide-terminal".to_string()),
            ("TERM_PROGRAM_VERSION".to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ]);
        let options = Options {
            shell: launch.command.clone().map(|(program, args)| Shell::new(program, args)),
            working_directory: launch.directory.clone().filter(|directory| directory.is_dir()),
            drain_on_exit: true,
            env,
        };
        let cell_size = iced::Size::new(8.0, 16.0);
        let pty = tty::new(&options, window_size(DEFAULT_CELLS, cell_size), id.0)?;
        let master = pty.file().try_clone()?;
        let shell = i32::try_from(pty.child().id()).unwrap_or(-1);
        let listener = Listener { pane: id, bus: bus.clone() };
        let term = Arc::new(FairMutex::new(Term::new(config, &DEFAULT_CELLS, listener.clone())));
        let event_loop = EventLoop::new(Arc::clone(&term), listener, pty, true, false)?;
        let notifier = Notifier(event_loop.channel());
        drop(event_loop.spawn());
        let mut pane = Self {
            id,
            term,
            notifier,
            master,
            shell,
            cells: DEFAULT_CELLS,
            cell_size,
            title: None,
            process: String::new(),
            directory: launch.directory.clone(),
            busy: false,
            exit: None,
        };
        pane.refresh();
        Ok(pane)
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        if self.exit.is_none() {
            self.notifier.notify(bytes);
        }
    }

    pub fn resize(&mut self, cells: Cells, cell_size: iced::Size) {
        if cells == self.cells && cell_size == self.cell_size {
            return;
        }
        self.cells = cells;
        self.cell_size = cell_size;
        self.term.lock().resize(cells);
        self.notifier.on_resize(window_size(cells, cell_size));
    }

    #[must_use]
    pub fn window_size(&self) -> WindowSize {
        window_size(self.cells, self.cell_size)
    }

    fn foreground(&self) -> Option<i32> {
        rustix::termios::tcgetpgrp(&self.master).ok().map(|pid| pid.as_raw_nonzero().get())
    }

    pub fn refresh(&mut self) {
        if self.exit.is_some() {
            self.busy = false;
            return;
        }
        let pid = self.foreground().unwrap_or(self.shell);
        if let Ok(name) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            self.process = name.trim().to_string();
        }
        if let Ok(directory) = std::fs::read_link(format!("/proc/{}/cwd", self.shell)) {
            self.directory = Some(directory);
        }
        self.busy = pid != self.shell;
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line, Point};
    use std::time::{Duration, Instant};

    #[test]
    fn a_command_runs_in_its_directory_and_its_output_reaches_the_grid() {
        let (sender, mut receiver) = mpsc::unbounded();
        let launch = Launch {
            command: Some(("sh".into(), vec!["-c".into(), "printf 'glide\\n'; pwd".into()])),
            directory: Some(PathBuf::from("/tmp")),
        };
        let pane = Pane::spawn(PaneId(7), &Bus(sender), &launch, Config::default()).expect("sh starts");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut exit = None;
        while exit.is_none() && Instant::now() < deadline {
            match receiver.try_recv() {
                Ok((id, Event::ChildExit(status))) => exit = Some((id, status)),
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let (id, status) = exit.expect("the child exits");
        assert_eq!(id, PaneId(7));
        assert!(status.success());
        let text = pane.term.lock().bounds_to_string(Point::new(Line(0), Column(0)), Point::new(Line(1), Column(79)));
        assert!(text.starts_with("glide\n/tmp"), "{text:?}");
    }
}
