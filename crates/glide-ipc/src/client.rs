use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use zeroize::Zeroizing;

use crate::{Request, Response};

const ACTIVATION_TIMEOUT: Duration = Duration::from_millis(400);

#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    let named = std::env::var_os("GLIDE_IPC_SOCK").filter(|path| !path.is_empty()).map(PathBuf::from);
    let derived = || {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty())?;
        let display = std::env::var("WAYLAND_DISPLAY").ok().filter(|display| !display.is_empty())?;
        Some(PathBuf::from(runtime).join(format!("glide-{display}.sock")))
    };
    match named {
        Some(path) if path.exists() => Some(path),
        named => derived().filter(|path| path.exists()).or(named).or_else(derived),
    }
}

const REQUEST_CAPACITY: usize = 4096;

pub fn exchange(request: &Request, timeout: Duration) -> std::io::Result<Response> {
    let path = socket_path().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no Glide session"))?;
    let stream = bounded(timeout, move || UnixStream::connect(path))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut json = Zeroizing::new(Vec::with_capacity(REQUEST_CAPACITY));
    serde_json::to_writer(&mut *json, request).map_err(std::io::Error::other)?;
    json.push(b'\n');
    let mut reader = BufReader::new(stream);
    reader.get_mut().write_all(&json)?;
    reader.get_mut().flush()?;
    let mut line = Zeroizing::new(String::new());
    read_line(&mut reader, &mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

fn bounded<T: Send + 'static>(
    timeout: Duration, work: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new().name("glide-ipc-connect".into()).spawn(move || {
        let _ = sender.send(work());
    })?;
    receiver.recv_timeout(timeout).unwrap_or_else(|err| match err {
        RecvTimeoutError::Timeout => {
            Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the Glide IPC socket did not accept in time"))
        }
        RecvTimeoutError::Disconnected => Err(std::io::Error::other("the Glide IPC connect thread failed")),
    })
}

pub const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

pub fn read_line(reader: &mut impl BufRead, line: &mut String) -> std::io::Result<usize> {
    read_line_with_limit(reader, line, MAX_RESPONSE_BYTES)
}

fn read_line_with_limit(reader: &mut impl BufRead, line: &mut String, limit: u64) -> std::io::Result<usize> {
    line.clear();
    let count = std::io::Read::take(reader, limit + 1).read_line(line)?;
    if count as u64 > limit {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "IPC message exceeds size limit"));
    }
    if count > 0 && !line.ends_with('\n') {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "incomplete IPC message"));
    }
    Ok(count)
}

#[must_use]
pub fn activation_token(app_id: Option<&str>) -> Option<String> {
    let request = Request::ActivationToken { app_id: app_id.map(str::to_string) };
    match exchange(&request, ACTIVATION_TIMEOUT).ok()? {
        Response::ActivationToken { token } if !token.is_empty() => Some(token),
        _ => None,
    }
}

#[must_use]
pub fn activation_env(app_id: Option<&str>) -> Vec<(&'static str, String)> {
    activation_token(app_id)
        .map(|token| vec![("XDG_ACTIVATION_TOKEN", token.clone()), ("DESKTOP_STARTUP_ID", token)])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_lines_preserve_buffered_messages_and_accept_the_limit() {
        let mut reader = BufReader::new(&b"123\nxy\n"[..]);
        let mut line = String::from("previous");
        assert_eq!(read_line_with_limit(&mut reader, &mut line, 4).unwrap(), 4);
        assert_eq!(line, "123\n");
        assert_eq!(read_line_with_limit(&mut reader, &mut line, 4).unwrap(), 3);
        assert_eq!(line, "xy\n");
        assert_eq!(read_line_with_limit(&mut reader, &mut line, 4).unwrap(), 0);
    }

    #[test]
    fn a_connect_that_never_returns_times_out() {
        let (hold, wait) = std::sync::mpsc::channel::<()>();
        let started = std::time::Instant::now();
        let result = bounded(Duration::from_millis(20), move || {
            let _ = wait.recv();
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(hold);
        assert_eq!(bounded(Duration::from_secs(5), || Ok(7)).unwrap(), 7);
    }

    #[test]
    fn a_request_is_one_json_line_from_the_zeroizing_writer() {
        let mut json = Zeroizing::new(Vec::with_capacity(REQUEST_CAPACITY));
        let request = Request::LoginPasswordChanged { password: crate::keychain::Hidden::new("hunter2".into()) };
        serde_json::to_writer(&mut *json, &request).unwrap();
        assert_eq!(json.capacity(), REQUEST_CAPACITY, "a short request never reallocates its buffer");
        assert_eq!(serde_json::from_slice::<Request>(&json).unwrap(), request);
    }

    #[test]
    fn oversized_and_incomplete_messages_are_rejected() {
        let mut line = String::new();
        let mut oversized = &b"1234\n"[..];
        assert_eq!(
            read_line_with_limit(&mut oversized, &mut line, 4).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        let mut incomplete = &b"{}"[..];
        assert_eq!(
            read_line_with_limit(&mut incomplete, &mut line, 4).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
}
