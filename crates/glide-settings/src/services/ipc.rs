use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use glide_ipc::{Event, GestureSample, OutputInfo, Request, Response};
use iced::futures::{SinkExt, Stream};

#[derive(Debug, Clone, PartialEq)]
pub enum IpcEvent {
    Connected,
    Disconnected,
    OutputChanged,
    LayoutChanged(String),
}

#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GLIDE_IPC_SOCK").filter(|path| !path.is_empty()) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let display = std::env::var("WAYLAND_DISPLAY").ok()?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let path = PathBuf::from(runtime).join(format!("glide-{display}.sock"));
    path.exists().then_some(path)
}

fn connect() -> Result<UnixStream, String> {
    let path = socket_path().ok_or_else(|| String::from("glide ipc socket not found"))?;
    let stream = UnixStream::connect(path).map_err(|err| err.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|err| err.to_string())?;
    Ok(stream)
}

fn write_request(stream: &mut UnixStream, request: &Request) -> Result<(), String> {
    let mut json = serde_json::to_string(request).map_err(|err| err.to_string())?;
    json.push('\n');
    stream.write_all(json.as_bytes()).and_then(|()| stream.flush()).map_err(|err| err.to_string())
}

pub fn request(request: &Request) -> Result<Response, String> {
    let mut stream = connect()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).map_err(|err| err.to_string())?;
    write_request(&mut stream, request)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|err| err.to_string())?;
    serde_json::from_str(&line).map_err(|err| err.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadError {
    Offline,
    Rejected(String),
}

pub fn reload_outcome(response: Response) -> Result<(), ReloadError> {
    match response {
        Response::Error { message } => Err(ReloadError::Rejected(message)),
        _ => Ok(()),
    }
}

pub fn reload() -> Result<(), ReloadError> {
    let response = request(&Request::Reload).map_err(|_| ReloadError::Offline)?;
    reload_outcome(response)
}

pub fn list_outputs() -> Result<Vec<OutputInfo>, String> {
    match request(&Request::ListOutputs)? {
        Response::Outputs { list } => Ok(list),
        Response::Error { message } => Err(message),
        other => Err(format!("unexpected response: {other:?}")),
    }
}

#[must_use]
pub fn translate(event: &Event) -> Option<IpcEvent> {
    match event {
        Event::OutputChanged => Some(IpcEvent::OutputChanged),
        Event::LayoutChanged { name, .. } => Some(IpcEvent::LayoutChanged(name.clone())),
        _ => None,
    }
}

fn pump(sender: &mut iced::futures::channel::mpsc::Sender<IpcEvent>) -> bool {
    let Ok(mut stream) = connect() else {
        return false;
    };
    if write_request(&mut stream, &Request::Subscribe).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || !matches!(serde_json::from_str(&line), Ok(Response::Subscribed)) {
        return false;
    }
    let _ = sender.try_send(IpcEvent::Connected);
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return true,
            Ok(_) => {}
        }
        if let Ok(event) = serde_json::from_str::<Event>(line.trim())
            && let Some(event) = translate(&event)
            && sender.try_send(event).is_err()
            && sender.is_closed()
        {
            return true;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PracticeEvent {
    Started,
    Sample(GestureSample),
    Stopped,
}

#[must_use]
pub fn practice_sample(line: &str) -> Option<GestureSample> {
    match serde_json::from_str::<Event>(line.trim()) {
        Ok(Event::Gesture { sample }) => Some(sample),
        _ => None,
    }
}

fn practise(sender: &mut iced::futures::channel::mpsc::Sender<PracticeEvent>) {
    let Ok(mut stream) = connect() else {
        return;
    };
    if stream.set_read_timeout(Some(Duration::from_millis(400))).is_err()
        || write_request(&mut stream, &Request::PracticeGestures).is_err()
    {
        return;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {}
            Err(err) if matches!(err.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                if sender.is_closed() {
                    return;
                }
                continue;
            }
            Err(_) => return,
        }
        let event = if matches!(serde_json::from_str(line.trim()), Ok(Response::Subscribed)) {
            Some(PracticeEvent::Started)
        } else {
            practice_sample(&line).map(PracticeEvent::Sample)
        };
        line.clear();
        if let Some(event) = event
            && sender.try_send(event).is_err()
            && sender.is_closed()
        {
            return;
        }
    }
}

pub fn practice() -> impl Stream<Item = PracticeEvent> {
    iced::stream::channel(64, async |mut output| {
        let mut sender = output.clone();
        std::thread::spawn(move || {
            practise(&mut sender);
            let _ = sender.try_send(PracticeEvent::Stopped);
        });
        let _ = output.flush().await;
        std::future::pending::<()>().await;
    })
}

pub fn events() -> impl Stream<Item = IpcEvent> {
    iced::stream::channel(32, async |mut output| {
        let mut sender = output.clone();
        std::thread::spawn(move || {
            loop {
                let was_connected = pump(&mut sender);
                if sender.is_closed() {
                    return;
                }
                if was_connected {
                    let _ = sender.try_send(IpcEvent::Disconnected);
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        });
        let _ = output.flush().await;
        std::future::pending::<()>().await;
    })
}
