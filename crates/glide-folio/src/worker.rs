use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::doc::{self, Engine, Link, OpenError, Opened, PageText};

pub const FLAG: &str = "--render";
const MAX_SIDE: u32 = 12_000;

#[derive(Debug, Serialize, Deserialize)]
enum Request {
    Open { path: PathBuf, password: Option<String> },
    Render { page: usize, scale: f32 },
    Text { page: usize },
    Links { page: usize },
}

#[derive(Debug, Serialize, Deserialize)]
enum Reply {
    Opened(Box<Result<Opened, OpenError>>),
    Image { w: u32, h: u32 },
    Blank,
    Text(PageText),
    Links(Vec<Link>),
}

fn send<T: Serialize>(out: &mut impl Write, value: &T) -> std::io::Result<()> {
    let line = serde_json::to_string(value).map_err(std::io::Error::other)?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

fn receive<T: for<'de> Deserialize<'de>>(input: &mut impl BufRead) -> std::io::Result<T> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "renderer closed"));
    }
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

fn scale_within(engine: &dyn Engine, page: usize, scale: f32) -> f32 {
    let Some(size) = engine.opened().pages.get(page) else {
        return scale;
    };
    let longest = size.w.max(size.h) * scale;
    if longest > MAX_SIDE as f32 { MAX_SIDE as f32 / size.w.max(size.h) } else { scale }
}

pub fn serve(input: impl Read, output: impl Write) -> std::io::Result<()> {
    let mut input = BufReader::new(input);
    let mut output = BufWriter::new(output);
    let mut engine: Option<Box<dyn Engine>> = None;
    loop {
        let request: Request = match receive(&mut input) {
            Ok(request) => request,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        match request {
            Request::Open { path, password } => {
                let opened = doc::open(&path, password.as_deref());
                let reply = opened.as_ref().map(|engine| engine.opened().clone()).map_err(Clone::clone);
                engine = opened.ok();
                send(&mut output, &Reply::Opened(Box::new(reply)))?;
            }
            Request::Render { page, scale } => {
                let image = engine.as_mut().and_then(|engine| {
                    let scale = scale_within(engine.as_ref(), page, scale);
                    engine.render(page, scale)
                });
                match image {
                    Some(image) => {
                        send(&mut output, &Reply::Image { w: image.width(), h: image.height() })?;
                        output.write_all(image.as_raw())?;
                        output.flush()?;
                    }
                    None => send(&mut output, &Reply::Blank)?,
                }
            }
            Request::Text { page } => {
                let text = engine.as_mut().map(|engine| engine.text(page)).unwrap_or_default();
                send(&mut output, &Reply::Text(text))?;
            }
            Request::Links { page } => {
                let links = engine.as_mut().map(|engine| engine.links(page)).unwrap_or_default();
                send(&mut output, &Reply::Links(links))?;
            }
        }
    }
}

#[must_use]
pub fn run() -> std::process::ExitCode {
    match serve(std::io::stdin().lock(), std::io::stdout().lock()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}

struct Process {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Backend {
    Process(Process),
    Local(Box<dyn Engine>),
}

pub struct Session {
    backend: Backend,
    opened: Opened,
}

fn spawn() -> Option<Process> {
    let exe = std::env::current_exe().ok()?;
    if exe.file_stem().and_then(|s| s.to_str()) != Some(crate::APP_ID) {
        return None;
    }
    let mut child =
        Command::new(exe).arg(FLAG).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let input = BufWriter::new(child.stdin.take()?);
    let output = BufReader::new(child.stdout.take()?);
    Some(Process { child, input, output })
}

impl Session {
    pub fn open(path: &Path, password: Option<&str>) -> Result<Self, OpenError> {
        if let Some(mut process) = spawn() {
            let request = Request::Open { path: path.to_path_buf(), password: password.map(str::to_string) };
            let reply = send(&mut process.input, &request).and_then(|()| receive::<Reply>(&mut process.output));
            return match reply {
                Ok(Reply::Opened(result)) => match *result {
                    Ok(opened) => Ok(Self { backend: Backend::Process(process), opened }),
                    Err(error) => Err(error),
                },
                _ => Err(OpenError::Damaged),
            };
        }
        Self::local(path, password)
    }

    pub fn local(path: &Path, password: Option<&str>) -> Result<Self, OpenError> {
        let engine = doc::open(path, password)?;
        let opened = engine.opened().clone();
        Ok(Self { backend: Backend::Local(engine), opened })
    }

    #[must_use]
    pub fn opened(&self) -> &Opened {
        &self.opened
    }

    fn ask(&mut self, request: &Request) -> Option<Reply> {
        let Backend::Process(process) = &mut self.backend else {
            return None;
        };
        send(&mut process.input, request).ok()?;
        receive(&mut process.output).ok()
    }

    pub fn render(&mut self, page: usize, scale: f32) -> Option<RgbaImage> {
        if let Backend::Local(engine) = &mut self.backend {
            let scale = scale_within(engine.as_ref(), page, scale);
            return engine.render(page, scale);
        }
        match self.ask(&Request::Render { page, scale })? {
            Reply::Image { w, h } => {
                let Backend::Process(process) = &mut self.backend else {
                    return None;
                };
                let mut bytes = vec![0; (w as usize) * (h as usize) * 4];
                process.output.read_exact(&mut bytes).ok()?;
                RgbaImage::from_raw(w, h, bytes)
            }
            _ => None,
        }
    }

    pub fn text(&mut self, page: usize) -> PageText {
        if let Backend::Local(engine) = &mut self.backend {
            return engine.text(page);
        }
        match self.ask(&Request::Text { page }) {
            Some(Reply::Text(text)) => text,
            _ => PageText::default(),
        }
    }

    pub fn links(&mut self, page: usize) -> Vec<Link> {
        if let Backend::Local(engine) = &mut self.backend {
            return engine.links(page);
        }
        match self.ask(&Request::Links { page }) {
            Some(Reply::Links(links)) => links,
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn the_protocol_opens_renders_and_reads_text() {
        let dir = std::env::temp_dir().join(format!("glide-folio-worker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.pdf");
        std::fs::write(&path, crate::doc::pdf::tests::sample(2)).unwrap();
        let mut script = Vec::new();
        send(&mut script, &Request::Open { path, password: None }).unwrap();
        send(&mut script, &Request::Render { page: 0, scale: 0.25 }).unwrap();
        send(&mut script, &Request::Text { page: 1 }).unwrap();
        let mut out = Vec::new();
        serve(Cursor::new(script), &mut out).unwrap();
        let mut reader = BufReader::new(Cursor::new(out));
        let Reply::Opened(result) = receive(&mut reader).unwrap() else { panic!("not opened") };
        let opened = result.expect("opens");
        assert_eq!(opened.pages.len(), 2);
        let Reply::Image { w, h } = receive(&mut reader).unwrap() else { panic!("no image") };
        assert_eq!((w, h), (153, 198));
        let mut pixels = vec![0; (w * h * 4) as usize];
        reader.read_exact(&mut pixels).unwrap();
        let Reply::Text(text) = receive(&mut reader).unwrap() else { panic!("no text") };
        assert_eq!(text.plain(), "Workspaces page 2");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_local_session_serves_the_same_answers() {
        let dir = std::env::temp_dir().join(format!("glide-folio-local-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.pdf");
        std::fs::write(&path, crate::doc::pdf::tests::sample(2)).unwrap();
        let mut session = Session::open(&path, None).unwrap();
        assert_eq!(session.opened().pages.len(), 2);
        assert_eq!(session.text(0).plain(), "Workspaces page 1");
        assert_eq!(session.links(0).len(), 1);
        assert!(session.render(1, 0.1).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
