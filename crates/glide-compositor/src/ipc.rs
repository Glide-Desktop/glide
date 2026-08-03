//! IPC server for the Glide compositor.
//!
//! Listens on a Unix domain socket in `$XDG_RUNTIME_DIR`, speaking
//! newline-delimited JSON: one [`Request`] in, one [`Response`] out, then the
//! connection is closed.
//!
//! # Why everything here is non-blocking
//!
//! The compositor is single-threaded: every IPC callback runs on the same
//! calloop event loop that drives rendering, input and the Wayland socket. A
//! blocking `read`/`write` on a client connection would therefore stall the
//! *entire session* — and on a bare tty there is no second compositor to
//! switch to, because even `Ctrl+Alt+Fn` handling lives on that same loop.
//! A client that connects and then sends nothing (`nc -U <socket>`) would be
//! enough to wedge the machine.
//!
//! So: the listener and every accepted connection are non-blocking, each
//! connection is its own event source with its own buffer, and a connection
//! that misbehaves (oversized request, unreadable, or not draining its
//! response) is dropped rather than waited on.

use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

use glide_ipc::{Request, Response, WindowInfo, WorkspaceInfo};
use smithay::{
    reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, generic::Generic},
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

use crate::GlideState;

/// Hard cap on a single request. Requests are a few hundred bytes at most;
/// this only exists so a client streaming endless bytes without a newline
/// cannot grow the buffer until the compositor is OOM-killed.
const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Owns the listening socket's path and unlinks it on shutdown.
///
/// Held by [`GlideState`] rather than dropped immediately, so the socket file
/// lives exactly as long as the compositor does.
#[derive(Debug)]
pub struct IpcServer {
    socket_path: PathBuf,
}

impl IpcServer {
    /// Path of the socket this server is listening on.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.socket_path)
            && err.kind() != ErrorKind::NotFound
        {
            tracing::warn!(
                path = %self.socket_path.display(),
                ?err,
                "failed to remove the IPC socket on shutdown"
            );
        }
    }
}

/// Directory to place the IPC socket in.
///
/// `$XDG_RUNTIME_DIR` only. There is deliberately **no** `/tmp` fallback: a
/// predictable path in a world-writable directory lets any other local user
/// pre-create the socket and intercept `glidectl` traffic — which includes
/// [`Request::Exec`], i.e. arbitrary command execution. Without a runtime dir
/// we would rather not offer IPC at all.
fn socket_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// Socket file name for a Wayland socket name, e.g. `glide-wayland-1.sock`.
fn socket_file_name(socket_name: &std::ffi::OsStr) -> String {
    format!("glide-{}.sock", socket_name.to_string_lossy())
}

/// Socket path for a given Wayland socket name, e.g. `.../glide-wayland-1.sock`.
#[must_use]
pub fn socket_path_for(socket_name: &std::ffi::OsStr) -> Option<PathBuf> {
    Some(socket_dir()?.join(socket_file_name(socket_name)))
}

/// Maps the wire-level workspace target onto the layout engine's own.
///
/// Written out rather than derived via `From`: both types are foreign to this
/// crate, so the orphan rule forbids the impl, and making `glide-ipc` depend
/// on `glide-layout` just for this would drag the layout engine into every
/// IPC client.
fn to_layout_target(target: glide_ipc::WorkspaceTarget) -> glide_layout::WorkspaceTarget {
    match target {
        glide_ipc::WorkspaceTarget::Index(idx) => glide_layout::WorkspaceTarget::Index(idx),
        glide_ipc::WorkspaceTarget::Next => glide_layout::WorkspaceTarget::Next,
        glide_ipc::WorkspaceTarget::Previous => glide_layout::WorkspaceTarget::Previous,
    }
}

/// Starts the IPC server.
///
/// Returns `Ok(None)` when `$XDG_RUNTIME_DIR` is unset — IPC is then simply
/// unavailable, which is not fatal to the compositor.
///
/// # Errors
///
/// Returns an error if the socket cannot be bound or registered with the
/// event loop.
pub fn init_ipc(
    event_loop: &mut EventLoop<'static, GlideState>,
    socket_name: &std::ffi::OsStr,
) -> anyhow::Result<Option<IpcServer>> {
    let Some(socket_path) = socket_path_for(socket_name) else {
        tracing::warn!("XDG_RUNTIME_DIR is unset; IPC socket disabled");
        return Ok(None);
    };

    // A leftover socket from a crashed run would make bind() fail with
    // EADDRINUSE, so clear it first. Anything else (including a *live*
    // compositor's socket) is not distinguishable here; the socket name is
    // derived from our own freshly-allocated wayland-N name, so a collision
    // means a stale file, not a running peer.
    match std::fs::remove_file(&socket_path) {
        Ok(()) => tracing::debug!(path = %socket_path.display(), "removed a stale IPC socket"),
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(anyhow::anyhow!(err))
                .map_err(|e: anyhow::Error| e.context("failed to clear the stale IPC socket"));
        }
    }

    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    // `Request::Exec` runs arbitrary shell commands, so the socket must not be
    // reachable by other users even if XDG_RUNTIME_DIR is misconfigured.
    if let Err(err) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
    {
        tracing::warn!(?err, "failed to restrict IPC socket permissions to 0600");
    }

    let server = IpcServer {
        socket_path: socket_path.clone(),
    };

    let loop_handle = event_loop.handle();
    event_loop
        .handle()
        .insert_source(
            Generic::new(listener, Interest::READ, Mode::Level),
            move |_, listener, _state| {
                accept_pending(listener, &loop_handle);
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.error)
        .map_err(|e| anyhow::anyhow!("failed to register the IPC listener: {e}"))?;

    tracing::info!(socket = %socket_path.display(), "IPC server started");
    Ok(Some(server))
}

/// Drains the listener's backlog, giving each connection its own event source.
fn accept_pending(listener: &UnixListener, loop_handle: &LoopHandle<'static, GlideState>) {
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                if let Err(err) = register_connection(stream, loop_handle) {
                    tracing::warn!(?err, "failed to register an IPC connection");
                }
            }
            // Backlog drained: this is the normal exit, not an error.
            Err(err) if err.kind() == ErrorKind::WouldBlock => return,
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                tracing::warn!(?err, "failed to accept an IPC connection");
                return;
            }
        }
    }
}

fn register_connection(
    stream: UnixStream,
    loop_handle: &LoopHandle<'static, GlideState>,
) -> anyhow::Result<()> {
    stream.set_nonblocking(true)?;

    let mut buffer: Vec<u8> = Vec::new();
    loop_handle
        .insert_source(
            Generic::new(stream, Interest::READ, Mode::Level),
            move |_, stream, state| Ok(read_request(stream, &mut buffer, state)),
        )
        .map_err(|e| e.error)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

/// Reads whatever is available without blocking, and once a full line has
/// arrived, dispatches it and answers.
///
/// Always returns [`PostAction::Remove`] after answering: the protocol is one
/// request per connection, so there is nothing more to wait for.
///
/// Takes `&UnixStream` rather than `&mut` because calloop hands out a shared
/// reference to the source's fd; `std` implements `Read`/`Write` for
/// `&UnixStream` precisely for this case.
fn read_request(stream: &UnixStream, buffer: &mut Vec<u8>, state: &mut GlideState) -> PostAction {
    let mut chunk = [0_u8; 4096];
    let mut stream = stream;

    loop {
        match stream.read(&mut chunk) {
            // Peer closed without sending a complete request.
            Ok(0) => return PostAction::Remove,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);

                if let Some(newline) = buffer.iter().position(|&b| b == b'\n') {
                    let line = buffer[..newline].to_vec();
                    respond(stream, state, &line);
                    return PostAction::Remove;
                }

                if buffer.len() > MAX_REQUEST_BYTES {
                    tracing::warn!(
                        bytes = buffer.len(),
                        "IPC request exceeded the size limit; dropping the connection"
                    );
                    return PostAction::Remove;
                }
            }
            // Nothing more to read yet; stay registered and wait for the rest.
            Err(err) if err.kind() == ErrorKind::WouldBlock => return PostAction::Continue,
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                tracing::warn!(?err, "failed to read from an IPC connection");
                return PostAction::Remove;
            }
        }
    }
}

/// Parses `line`, dispatches it and writes the response back.
fn respond(stream: &UnixStream, state: &mut GlideState, line: &[u8]) {
    let mut stream = stream;

    let response = match std::str::from_utf8(line) {
        Ok(text) => match serde_json::from_str::<Request>(text.trim()) {
            Ok(request) => dispatch_request(state, request),
            Err(err) => Response::Error {
                message: format!("invalid IPC request JSON: {err}"),
            },
        },
        Err(err) => Response::Error {
            message: format!("IPC request was not valid UTF-8: {err}"),
        },
    };

    let Ok(mut json) = serde_json::to_string(&response) else {
        tracing::error!("failed to serialize an IPC response");
        return;
    };
    json.push('\n');

    // Best-effort, non-blocking. Responses are small (a few KB for the
    // largest listing) and comfortably fit a socket buffer, so a short write
    // means the client is not reading — its problem, not ours. We must never
    // block the event loop waiting for it.
    if let Err(err) = stream.write_all(json.as_bytes()) {
        tracing::debug!(
            ?err,
            "failed to write an IPC response; client likely hung up"
        );
    }
}

/// Dispatches a single IPC request against the compositor state.
#[must_use]
pub fn dispatch_request(state: &mut GlideState, request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong,
        Request::Quit => {
            state.loop_signal.stop();
            Response::Ok
        }
        Request::ListWorkspaces => list_workspaces(state),
        Request::ListWindows => list_windows(state),
        Request::SwitchWorkspace { target } => {
            state.layout.switch_to_workspace(to_layout_target(target));
            state.reconcile_space();
            Response::Ok
        }
        Request::MoveWindow { window, target } => match resolve_window(state, window) {
            Some(w_id) => {
                state
                    .layout
                    .move_window_to_workspace(w_id, to_layout_target(target));
                state.reconcile_space();
                Response::Ok
            }
            None => window_not_found(),
        },
        Request::FocusWindow { selector } => match resolve_window(state, selector) {
            Some(w_id) if state.layout.focus_window(w_id) => {
                state.reconcile_space();
                Response::Ok
            }
            _ => window_not_found(),
        },
        Request::Reload => {
            crate::config::Config::load_or_default().apply_to_state(state);
            Response::Ok
        }
        Request::Exec { command } => spawn_command(&command),
    }
}

fn window_not_found() -> Response {
    Response::Error {
        message: "window not found".to_string(),
    }
}

fn resolve_window(
    state: &GlideState,
    selector: glide_ipc::WindowSelector,
) -> Option<glide_layout::WindowId> {
    match selector {
        glide_ipc::WindowSelector::Focused => state.layout.focused_window(),
        glide_ipc::WindowSelector::Id(raw) => {
            let id = glide_layout::WindowId::from_u64(raw);
            state.window_map.contains_key(&id).then_some(id)
        }
    }
}

fn list_workspaces(state: &GlideState) -> Response {
    let active_id = state.layout.active_workspace();
    let list = state
        .layout
        .workspaces()
        .enumerate()
        .map(|(index, ws)| WorkspaceInfo {
            id: ws.id.as_u64(),
            index,
            active: ws.id == active_id,
            window_count: ws.windows.len(),
        })
        .collect();
    Response::Workspaces { list }
}

fn list_windows(state: &GlideState) -> Response {
    let focused = state.layout.focused_window();
    let workspace_index: HashMap<_, _> = state
        .layout
        .workspaces()
        .enumerate()
        .map(|(index, ws)| (ws.id, index))
        .collect();

    let list = state
        .window_map
        .iter()
        .filter_map(|(&id, window)| {
            let ws_id = state.layout.window_state(id)?.workspace_id;
            let (app_id, title) = window_identity(window);

            Some(WindowInfo {
                id: id.as_u64(),
                app_id,
                title,
                workspace_index: *workspace_index.get(&ws_id)?,
                focused: focused == Some(id),
            })
        })
        .collect();
    Response::Windows { list }
}

/// Reads a window's `app_id`/`title` as last set by the client.
fn window_identity(window: &smithay::desktop::Window) -> (Option<String>, Option<String>) {
    let Some(toplevel) = window.toplevel() else {
        return (None, None);
    };

    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .map_or((None, None), |data| {
                let guard = data
                    .lock()
                    .expect("single-threaded event loop, this mutex is never poisoned");
                (guard.app_id.clone(), guard.title.clone())
            })
    })
}

fn spawn_command(command: &str) -> Response {
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn()
    {
        Ok(child) => {
            tracing::info!(command, pid = child.id(), "ipc spawned process");
            Response::Ok
        }
        Err(err) => Response::Error {
            message: format!("failed to spawn process: {err}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: these deliberately test the pure helpers rather than
    // `socket_path_for`, which reads the environment. Tests run in parallel
    // threads within one process, so mutating env vars here would race.

    #[test]
    fn socket_file_name_is_derived_from_the_wayland_socket() {
        assert_eq!(
            socket_file_name(std::ffi::OsStr::new("wayland-1")),
            "glide-wayland-1.sock"
        );
    }

    #[test]
    fn workspace_targets_map_onto_the_layout_engine() {
        assert_eq!(
            to_layout_target(glide_ipc::WorkspaceTarget::Index(3)),
            glide_layout::WorkspaceTarget::Index(3)
        );
        assert_eq!(
            to_layout_target(glide_ipc::WorkspaceTarget::Next),
            glide_layout::WorkspaceTarget::Next
        );
        assert_eq!(
            to_layout_target(glide_ipc::WorkspaceTarget::Previous),
            glide_layout::WorkspaceTarget::Previous
        );
    }
}
