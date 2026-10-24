use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

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

pub fn exchange(request: &Request, timeout: Duration) -> std::io::Result<Response> {
    let path = socket_path().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no Glide session"))?;
    let stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut json = serde_json::to_string(request).map_err(std::io::Error::other)?;
    json.push('\n');
    let mut reader = BufReader::new(stream);
    reader.get_mut().write_all(json.as_bytes())?;
    reader.get_mut().flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
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
