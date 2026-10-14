use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
};

use glide_ipc::{Event, OutputInfo, Request, Response, WindowInfo, WorkspaceInfo};
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, generic::Generic};

use crate::GlideState;

const MAX_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct IpcServer {
    socket_path: PathBuf,
}

impl IpcServer {
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

#[derive(Debug, Default)]
pub struct IpcSubscribers {
    streams: Vec<UnixStream>,
}

impl IpcSubscribers {
    fn add(&mut self, stream: UnixStream) {
        self.streams.push(stream);
        tracing::info!(subscriber_count = self.streams.len(), "IPC event subscriber connected");
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.streams.is_empty()
    }

    pub fn prune_closed(&mut self) {
        self.streams.retain(|stream| {
            let mut probe = [0_u8; 64];
            let mut reader = stream;
            match reader.read(&mut probe) {
                Ok(0) => false,
                Ok(_) => true,
                Err(err) => matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
            }
        });
    }

    pub fn broadcast(&mut self, event: &Event) {
        if self.streams.is_empty() {
            return;
        }
        let Ok(mut json) = serde_json::to_string(event) else {
            tracing::error!(?event, "failed to serialize an IPC event");
            return;
        };
        json.push('\n');
        let bytes = json.as_bytes();

        self.streams.retain_mut(|stream| match stream.write(bytes) {
            Ok(n) if n == bytes.len() => true,
            Ok(_) | Err(_) => false,
        });
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

fn socket_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()).map(PathBuf::from)
}

fn socket_file_name(socket_name: &std::ffi::OsStr) -> String {
    format!("glide-{}.sock", socket_name.to_string_lossy())
}

#[must_use]
pub fn socket_path_for(socket_name: &std::ffi::OsStr) -> Option<PathBuf> {
    Some(socket_dir()?.join(socket_file_name(socket_name)))
}

fn to_layout_target(target: glide_ipc::WorkspaceTarget) -> glide_layout::WorkspaceTarget {
    match target {
        glide_ipc::WorkspaceTarget::Index(idx) => glide_layout::WorkspaceTarget::Index(idx),
        glide_ipc::WorkspaceTarget::Next => glide_layout::WorkspaceTarget::Next,
        glide_ipc::WorkspaceTarget::Previous => glide_layout::WorkspaceTarget::Previous,
    }
}

pub fn init_ipc(
    event_loop: &mut EventLoop<'static, GlideState>, socket_name: &std::ffi::OsStr,
) -> anyhow::Result<Option<IpcServer>> {
    let Some(socket_path) = socket_path_for(socket_name) else {
        tracing::warn!("XDG_RUNTIME_DIR is unset; IPC socket disabled");
        return Ok(None);
    };

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

    if let Err(err) = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)) {
        tracing::warn!(?err, "failed to restrict IPC socket permissions to 0600");
    }

    let server = IpcServer { socket_path: socket_path.clone() };

    let loop_handle = event_loop.handle();
    event_loop
        .handle()
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), move |_, listener, _state| {
            accept_pending(listener, &loop_handle);
            Ok(PostAction::Continue)
        })
        .map_err(|e| e.error)
        .map_err(|e| anyhow::anyhow!("failed to register the IPC listener: {e}"))?;

    tracing::info!(socket = %socket_path.display(), "IPC server started");
    Ok(Some(server))
}

fn accept_pending(listener: &UnixListener, loop_handle: &LoopHandle<'static, GlideState>) {
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                if let Err(err) = register_connection(stream, loop_handle) {
                    tracing::warn!(?err, "failed to register an IPC connection");
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => return,
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                tracing::warn!(?err, "failed to accept an IPC connection");
                return;
            }
        }
    }
}

fn register_connection(stream: UnixStream, loop_handle: &LoopHandle<'static, GlideState>) -> anyhow::Result<()> {
    stream.set_nonblocking(true)?;

    let mut buffer: Vec<u8> = Vec::new();
    loop_handle
        .insert_source(Generic::new(stream, Interest::READ, Mode::Level), move |_, stream, state| {
            Ok(read_request(stream, &mut buffer, state))
        })
        .map_err(|e| e.error)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

fn read_request(stream: &UnixStream, buffer: &mut Vec<u8>, state: &mut GlideState) -> PostAction {
    let mut chunk = [0_u8; 4096];
    let mut stream = stream;

    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return PostAction::Remove,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);

                if let Some(newline) = buffer.iter().position(|&b| b == b'\n') {
                    respond(stream, state, &buffer[..newline]);
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
            Err(err) if err.kind() == ErrorKind::WouldBlock => return PostAction::Continue,
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                tracing::warn!(?err, "failed to read from an IPC connection");
                return PostAction::Remove;
            }
        }
    }
}

fn write_all_nonblocking(mut stream: &UnixStream, mut data: &[u8]) -> std::io::Result<()> {
    let mut retries = 0;
    while !data.is_empty() {
        match stream.write(data) {
            Ok(0) => {
                return Err(std::io::Error::new(ErrorKind::WriteZero, "failed to write response: zero bytes written"));
            }
            Ok(n) => {
                data = &data[n..];
                retries = 0;
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                if retries >= 100 {
                    return Err(err);
                }
                retries += 1;
                std::thread::yield_now();
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn respond(stream: &UnixStream, state: &mut GlideState, line: &[u8]) {
    let request = match std::str::from_utf8(line) {
        Ok(text) => {
            serde_json::from_str::<Request>(text.trim()).map_err(|err| format!("invalid IPC request JSON: {err}"))
        }
        Err(err) => Err(format!("IPC request was not valid UTF-8: {err}")),
    };

    let subscribing = matches!(request, Ok(Request::Subscribe));
    let practising = matches!(request, Ok(Request::PracticeGestures));
    let response = match request {
        Ok(request) => dispatch_request(state, request),
        Err(message) => Response::Error { message },
    };

    let Ok(mut json) = serde_json::to_string(&response) else {
        tracing::error!("failed to serialize an IPC response");
        return;
    };
    json.push('\n');

    if let Err(err) = write_all_nonblocking(stream, json.as_bytes()) {
        tracing::debug!(?err, "failed to write an IPC response; client likely hung up or buffer full");
        return;
    }

    if subscribing || practising {
        match stream.try_clone() {
            Ok(clone) if practising => {
                state.cancel_touchpad_gesture();
                state.gestures.clients.add(clone);
            }
            Ok(clone) => state.ipc_subscribers.add(clone),
            Err(err) => tracing::warn!(?err, "failed to register an IPC event subscriber"),
        }
    }
}

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
                state.layout.move_window_to_workspace(w_id, to_layout_target(target));
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
        Request::Reload => match crate::config::Config::try_load() {
            Ok(config) => {
                config.apply_to_state(state);
                state.queue_redraw();
                Response::Ok
            }
            Err(message) => {
                tracing::warn!(%message, "reload rejected; keeping the running configuration");
                Response::Error { message }
            }
        },
        Request::Exec { command } => spawn_command(&command),
        Request::Subscribe | Request::PracticeGestures => Response::Subscribed,
        Request::SetDpms { on } => {
            state.set_outputs_blanked(!on);
            Response::Ok
        }
        Request::CycleLayout => {
            state.cycle_keyboard_layout();
            match state.active_layout() {
                Some((index, name)) => Response::Layout { index, name },
                None => Response::Ok,
            }
        }
        Request::GetLayout => match state.active_layout() {
            Some((index, name)) => Response::Layout { index, name },
            None => Response::Error { message: "no keyboard".to_string() },
        },
        Request::ListOutputs => list_outputs(state),
        Request::NightLight { on } => {
            if let Some(on) = on {
                state.set_night_light(on);
            }
            Response::NightLight { on: state.topbar.system_status().night_light_on }
        }
        Request::Lock => {
            state.lock_screen();
            Response::Ok
        }
    }
}

fn list_outputs(state: &GlideState) -> Response {
    let list = state
        .space
        .outputs()
        .map(|output| {
            let physical = output.physical_properties();
            let (width, height, refresh_mhz) =
                output.current_mode().map_or((0, 0, 0), |mode| (mode.size.w, mode.size.h, mode.refresh));
            let location = output.current_location();
            let preferred = output.preferred_mode();
            let modes = output
                .modes()
                .into_iter()
                .map(|mode| glide_ipc::ModeInfo {
                    width: mode.size.w,
                    height: mode.size.h,
                    refresh_mhz: mode.refresh,
                    preferred: preferred == Some(mode),
                })
                .collect();
            OutputInfo {
                name: output.name(),
                make: physical.make,
                model: physical.model,
                width,
                height,
                refresh_mhz,
                scale: output.current_scale().fractional_scale(),
                x: location.x,
                y: location.y,
                modes,
                physical_mm: (physical.size.w, physical.size.h),
            }
        })
        .collect();
    Response::Outputs { list }
}

fn window_not_found() -> Response {
    Response::Error { message: "window not found".to_string() }
}

fn resolve_window(state: &GlideState, selector: glide_ipc::WindowSelector) -> Option<glide_layout::WindowId> {
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
    let workspace_index: HashMap<_, _> =
        state.layout.workspaces().enumerate().map(|(index, ws)| (ws.id, index)).collect();

    let list = state
        .window_map
        .iter()
        .filter_map(|(&id, window)| {
            let layout_state = state.layout.window_state(id)?;
            let (app_id, title) = window.identity();

            Some(WindowInfo {
                id: id.as_u64(),
                app_id,
                title,
                workspace_index: *workspace_index.get(&layout_state.workspace_id)?,
                focused: focused == Some(id),
                minimized: layout_state.minimized,
            })
        })
        .collect();
    Response::Windows { list }
}

fn spawn_command(command: &str) -> Response {
    match crate::spawn_shell_command(command) {
        Ok(pid) => {
            tracing::info!(command, pid, "ipc spawned process");
            Response::Ok
        }
        Err(err) => Response::Error { message: format!("failed to spawn process: {err}") },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_file_name_is_derived_from_the_wayland_socket() {
        assert_eq!(socket_file_name(std::ffi::OsStr::new("wayland-1")), "glide-wayland-1.sock");
    }

    #[test]
    fn workspace_targets_map_onto_the_layout_engine() {
        assert_eq!(to_layout_target(glide_ipc::WorkspaceTarget::Index(3)), glide_layout::WorkspaceTarget::Index(3));
        assert_eq!(to_layout_target(glide_ipc::WorkspaceTarget::Next), glide_layout::WorkspaceTarget::Next);
        assert_eq!(to_layout_target(glide_ipc::WorkspaceTarget::Previous), glide_layout::WorkspaceTarget::Previous);
    }

    #[test]
    fn write_all_nonblocking_handles_partial_writes() {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        server_stream.set_nonblocking(true).unwrap();

        let payload = vec![0xAB_u8; 64 * 1024];
        let payload_clone = payload.clone();

        let handle = std::thread::spawn(move || {
            let mut received = Vec::new();
            let mut buf = [0_u8; 1024];
            while received.len() < payload_clone.len() {
                match (&client_stream).read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        received.extend_from_slice(&buf[..n]);
                        std::thread::sleep(std::time::Duration::from_micros(50));
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_micros(50));
                    }
                    Err(e) => panic!("unexpected read error: {e:?}"),
                }
            }
            received
        });

        let write_res = write_all_nonblocking(&server_stream, &payload);
        assert!(write_res.is_ok(), "write_all_nonblocking failed: {write_res:?}");

        let received = handle.join().unwrap();
        assert_eq!(received, payload);
    }

    #[test]
    fn write_all_nonblocking_times_out_on_full_buffer() {
        let (server_stream, _client_stream) = UnixStream::pair().unwrap();
        server_stream.set_nonblocking(true).unwrap();

        let dummy = [0x55_u8; 4096];
        #[allow(clippy::unused_io_amount)]
        while (&server_stream).write(&dummy).is_ok_and(|n| n > 0) {}

        let res = write_all_nonblocking(&server_stream, b"test response");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), ErrorKind::WouldBlock);
    }

    #[test]
    fn write_all_nonblocking_handles_closed_socket() {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        server_stream.set_nonblocking(true).unwrap();
        drop(client_stream);

        let res = write_all_nonblocking(&server_stream, b"hello client");
        assert!(res.is_err());
    }

    #[test]
    fn broadcast_with_no_subscribers_is_a_no_op() {
        let mut subscribers = IpcSubscribers::default();
        subscribers.broadcast(&Event::OutputChanged);
    }

    #[test]
    fn broadcast_delivers_the_event_as_a_newline_terminated_json_line() {
        let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
        server_stream.set_nonblocking(true).unwrap();

        let mut subscribers = IpcSubscribers::default();
        subscribers.add(server_stream);
        subscribers.broadcast(&Event::WindowOpened { id: 42 });

        let mut buf = [0_u8; 256];
        let n = client_stream.read(&mut buf).unwrap();
        let line = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(line.ends_with('\n'), "{line:?}");
        let parsed: Event = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(parsed, Event::WindowOpened { id: 42 });
    }

    #[test]
    fn broadcast_never_blocks_on_a_subscriber_whose_buffer_is_full() {
        let (server_stream, _client_stream) = UnixStream::pair().unwrap();
        server_stream.set_nonblocking(true).unwrap();

        let dummy = [0x55_u8; 4096];
        #[allow(clippy::unused_io_amount)]
        while (&server_stream).write(&dummy).is_ok_and(|n| n > 0) {}

        let mut subscribers = IpcSubscribers::default();
        subscribers.add(server_stream);

        let start = std::time::Instant::now();
        subscribers.broadcast(&Event::OutputChanged);
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "broadcast must never block on a full subscriber buffer, took {elapsed:?}"
        );
        assert!(
            subscribers.streams.is_empty(),
            "a subscriber whose buffer is full must be dropped, not retried indefinitely"
        );
    }

    #[test]
    fn a_stalled_subscriber_does_not_stop_the_broadcast_reaching_others() {
        let (stalled, _stalled_client) = UnixStream::pair().unwrap();
        stalled.set_nonblocking(true).unwrap();
        let dummy = [0x55_u8; 4096];
        #[allow(clippy::unused_io_amount)]
        while (&stalled).write(&dummy).is_ok_and(|n| n > 0) {}

        let (reading, mut reading_client) = UnixStream::pair().unwrap();
        reading.set_nonblocking(true).unwrap();

        let mut subscribers = IpcSubscribers::default();
        subscribers.add(stalled);
        subscribers.add(reading);
        subscribers.broadcast(&Event::OutputChanged);

        assert_eq!(subscribers.streams.len(), 1, "exactly the stalled subscriber must have been dropped");

        let mut buf = [0_u8; 256];
        let n = reading_client.read(&mut buf).unwrap();
        let parsed: Event = serde_json::from_str(std::str::from_utf8(&buf[..n]).unwrap().trim()).unwrap();
        assert_eq!(parsed, Event::OutputChanged);
    }

    #[test]
    fn broadcast_drops_a_subscriber_whose_client_hung_up() {
        let (server_stream, client_stream) = UnixStream::pair().unwrap();
        server_stream.set_nonblocking(true).unwrap();
        drop(client_stream);

        let mut subscribers = IpcSubscribers::default();
        subscribers.add(server_stream);
        subscribers.broadcast(&Event::OutputChanged);

        assert!(subscribers.streams.is_empty());
    }

    #[test]
    fn gesture_practice_ends_when_the_settings_app_hangs_up() {
        let (alive, _alive_client) = UnixStream::pair().unwrap();
        let (gone, gone_client) = UnixStream::pair().unwrap();
        alive.set_nonblocking(true).unwrap();
        gone.set_nonblocking(true).unwrap();
        drop(gone_client);

        let mut practice = IpcSubscribers::default();
        practice.add(alive);
        practice.add(gone);
        practice.prune_closed();
        assert_eq!(practice.streams.len(), 1, "an idle open client stays");
        assert!(!practice.is_empty());
    }
}
