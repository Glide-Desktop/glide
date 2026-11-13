use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glide_app::chooser::{CANCEL_EXIT, FLAG, Filter, Mode, REQUEST_ENV, Request, Response};
use zbus::object_server::ObjectServer;
use zbus::zvariant::{DeserializeDict, OwnedObjectPath, OwnedValue, SerializeDict, Type};

const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.glide";
const FRONTEND: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SUCCESS: u32 = 0;
const CANCELLED: u32 = 1;
const FAILED: u32 = 2;
const IDLE_EXIT: Duration = Duration::from_mins(5);
const IDLE_CHECK: Duration = Duration::from_secs(20);
const DRAIN: Duration = Duration::from_secs(1);

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static LAST_CALL: Mutex<Option<Instant>> = Mutex::new(None);

fn touch() {
    if let Ok(mut last) = LAST_CALL.lock() {
        *last = Some(Instant::now());
    }
}

fn idle() -> bool {
    IN_FLIGHT.load(Ordering::SeqCst) == 0
        && LAST_CALL.lock().is_ok_and(|last| last.is_none_or(|at| at.elapsed() >= IDLE_EXIT))
}

struct Busy;

impl Busy {
    fn start() -> Self {
        IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        touch();
        Self
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        touch();
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn from_frontend(header: &zbus::message::Header<'_>, connection: &zbus::Connection) -> bool {
    let Some(sender) = header.sender() else {
        return false;
    };
    let Ok(proxy) = zbus::fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let frontend = zbus::names::WellKnownName::from_static_str_unchecked(FRONTEND);
    proxy.get_name_owner(frontend.into()).await.is_ok_and(|owner| owner.as_str() == sender.as_str())
}

type PortalFilter = (String, Vec<(u32, String)>);

#[derive(DeserializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
struct Options {
    accept_label: Option<String>,
    multiple: Option<bool>,
    directory: Option<bool>,
    filters: Option<Vec<PortalFilter>>,
    current_filter: Option<PortalFilter>,
    current_name: Option<String>,
    current_folder: Option<Vec<u8>>,
    current_file: Option<Vec<u8>>,
    files: Option<Vec<Vec<u8>>>,
    choices: Option<Vec<PortalChoice>>,
}

type PortalChoice = (String, String, Vec<(String, String)>, String);

#[derive(SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
struct Results {
    uris: Vec<String>,
    current_filter: Option<PortalFilter>,
    choices: Option<Vec<(String, String)>>,
}

fn path_bytes(bytes: &[u8]) -> Option<PathBuf> {
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    (!bytes.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(bytes)))
}

fn request(mode: Mode, parent_window: String, title: String, options: Options) -> Request {
    let mut filters: Vec<Filter> =
        options.filters.unwrap_or_default().into_iter().map(|(name, list)| Filter::from_portal(name, list)).collect();
    let current_filter = options.current_filter.and_then(|(name, list)| {
        let wanted = Filter::from_portal(name, list);
        if filters.is_empty() {
            filters.push(wanted);
            return Some(0);
        }
        filters.iter().position(|filter| *filter == wanted)
    });
    let current_file = options.current_file.as_deref().and_then(path_bytes);
    let current_folder = options
        .current_folder
        .as_deref()
        .and_then(path_bytes)
        .or_else(|| current_file.as_deref().and_then(std::path::Path::parent).map(std::path::Path::to_path_buf))
        .filter(|folder| folder.to_str().is_some());
    let current_name = options.current_name.or_else(|| {
        current_file.as_deref().and_then(std::path::Path::file_name).map(|name| name.to_string_lossy().into_owned())
    });
    let files = options
        .files
        .unwrap_or_default()
        .iter()
        .filter_map(|bytes| path_bytes(bytes))
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .collect();
    let choices = options
        .choices
        .unwrap_or_default()
        .into_iter()
        .map(|(id, label, options, default)| glide_app::chooser::Choice { id, label, options, default })
        .collect();
    Request {
        choices,
        mode,
        title,
        accept_label: options.accept_label.filter(|label| !label.trim().is_empty()),
        multiple: options.multiple.unwrap_or(false),
        directory: options.directory.unwrap_or(false),
        filters,
        current_filter,
        current_name,
        current_folder,
        files,
        parent: (!parent_window.is_empty()).then_some(parent_window),
    }
}

fn files_binary() -> PathBuf {
    if let Some(chooser) = std::env::var_os("GLIDE_PORTAL_CHOOSER").filter(|path| !path.is_empty()) {
        return PathBuf::from(chooser);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("glide-files")))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("glide-files"))
}

struct PortalRequest {
    process: Option<std::os::fd::OwnedFd>,
    pid: libc::pid_t,
    reaped: Arc<Mutex<bool>>,
    cancel: Option<Arc<dyn Fn() + Send + Sync>>,
}

fn pidfd(pid: u32) -> Option<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: pidfd_open(2) takes a pid and a flags word and returns a new descriptor or -1.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    let fd = i32::try_from(fd).ok().filter(|fd| *fd >= 0)?;
    // SAFETY: `fd` was just returned by pidfd_open and is owned by nothing else.
    Some(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl PortalRequest {
    async fn close(
        &self, #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) {
        if from_frontend(&header, connection).await {
            self.stop();
        }
    }
}

impl PortalRequest {
    fn stop(&self) {
        use std::os::fd::AsRawFd;
        if let Some(cancel) = &self.cancel {
            cancel();
            return;
        }
        if let Some(process) = &self.process {
            // SAFETY: pidfd_send_signal(2) on a pidfd this request owns; it names the chooser it started even after
            // that process has exited and its pid has been reused, in which case the call fails with ESRCH.
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    process.as_raw_fd(),
                    libc::SIGTERM,
                    std::ptr::null::<libc::siginfo_t>(),
                    0,
                );
            }
            return;
        }
        let reaped = self.reaped.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !*reaped && self.pid > 0 {
            // SAFETY: kill(2) on the chooser this request started; the lock keeps it from being reaped meanwhile,
            // so the pid still names that process (alive or a zombie) and cannot have been reused.
            unsafe {
                libc::kill(self.pid, libc::SIGTERM);
            }
        }
    }
}

fn wait_unreaped(pid: libc::pid_t) {
    loop {
        // SAFETY: siginfo_t is plain data, for which all zero bytes are a valid value.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let Ok(id) = libc::id_t::try_from(pid) else {
            return;
        };
        // SAFETY: waitid(2) with WNOWAIT on our own child waits for it to exit and leaves it unreaped; `info` outlives
        // the call.
        let status = unsafe { libc::waitid(libc::P_PID, id, &raw mut info, libc::WEXITED | libc::WNOWAIT) };
        if status == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return;
        }
    }
}

fn collect(mut child: std::process::Child, reaped: &Mutex<bool>) -> std::io::Result<(ExitStatus, Vec<u8>)> {
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take()
        && pipe.read_to_end(&mut stdout).is_err()
    {
        stdout.clear();
    }
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        wait_unreaped(pid);
    }
    let mut reaped = reaped.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *reaped = true;
    let status = child.wait()?;
    drop(reaped);
    Ok((status, stdout))
}

fn chooser_code(status: ExitStatus) -> u32 {
    if status.code() == Some(CANCEL_EXIT) || status.signal() == Some(libc::SIGTERM) { CANCELLED } else { FAILED }
}

async fn blocking<T: Send + 'static>(name: &str, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _ = sender.send(work());
        })
        .ok()?;
    receiver.await.ok()
}

async fn run(server: &ObjectServer, handle: OwnedObjectPath, request: Request) -> (u32, Results) {
    let Some(json) = request.to_json() else {
        return (FAILED, Results::default());
    };
    let spawned = Command::new(files_binary())
        .arg(FLAG)
        .env(REQUEST_ENV, json)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        return (FAILED, Results::default());
    };
    let pid = child.id();
    let reaped = Arc::new(Mutex::new(false));
    let portal_request = PortalRequest {
        process: pidfd(pid),
        pid: libc::pid_t::try_from(pid).unwrap_or(0),
        reaped: reaped.clone(),
        cancel: None,
    };
    let registered = matches!(server.at(handle.clone(), portal_request).await, Ok(true));
    if !registered {
        let _ = child.kill();
    }
    let waiting = reaped.clone();
    let output = blocking("glide-portal-wait", move || collect(child, &waiting)).await;
    if output.is_none()
        && let Ok(pid) = i32::try_from(pid)
    {
        *reaped.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        // SAFETY: kill(2) and waitpid(2) on the chooser this request started, whose `Child` was dropped unreaped
        // when its waiting thread could not start.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, std::ptr::null_mut(), 0);
        }
    }
    if registered {
        let _ = server.remove::<PortalRequest, _>(&handle).await;
    } else {
        return (FAILED, Results::default());
    }
    match output {
        Some(Ok((status, stdout))) if status.success() => match Response::parse(&String::from_utf8_lossy(&stdout)) {
            Some(response) => (
                SUCCESS,
                Results {
                    uris: response.uris,
                    current_filter: response.filter.and_then(|index| request.filters.get(index)).map(Filter::to_portal),
                    choices: (!response.choices.is_empty()).then_some(response.choices),
                },
            ),
            None => (FAILED, Results::default()),
        },
        Some(Ok((status, _))) => (chooser_code(status), Results::default()),
        _ => (FAILED, Results::default()),
    }
}

struct FileChooser;

// Argument lists are fixed by org.freedesktop.impl.portal.FileChooser.
#[allow(clippy::too_many_arguments, clippy::unused_self)]
#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    async fn open_file(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String, title: String, options: Options,
        #[zbus(object_server)] server: &ObjectServer, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, Results) {
        let _busy = Busy::start();
        drop(app_id);
        if !from_frontend(&header, connection).await {
            return (FAILED, Results::default());
        }
        run(server, handle, request(Mode::Open, parent_window, title, options)).await
    }

    async fn save_file(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String, title: String, options: Options,
        #[zbus(object_server)] server: &ObjectServer, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, Results) {
        let _busy = Busy::start();
        drop(app_id);
        if !from_frontend(&header, connection).await {
            return (FAILED, Results::default());
        }
        run(server, handle, request(Mode::Save, parent_window, title, options)).await
    }

    async fn save_files(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String, title: String, options: Options,
        #[zbus(object_server)] server: &ObjectServer, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, Results) {
        let _busy = Busy::start();
        drop(app_id);
        if !from_frontend(&header, connection).await {
            return (FAILED, Results::default());
        }
        run(server, handle, request(Mode::SaveFiles, parent_window, title, options)).await
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        4
    }
}

#[derive(DeserializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
struct ScreenshotOptions {
    modal: Option<bool>,
    interactive: Option<bool>,
}

#[derive(SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
struct ScreenshotResults {
    uri: Option<String>,
}

#[derive(SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
struct ColorResults {
    color: Option<(f64, f64, f64)>,
}

const SCREENSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

const PICK_TIMEOUT: std::time::Duration = std::time::Duration::from_mins(5);

async fn ask(request: glide_ipc::Request, timeout: std::time::Duration) -> Option<glide_ipc::Response> {
    blocking("glide-portal-screen", move || glide_ipc::client::exchange(&request, timeout)).await?.ok()
}

fn code_for(response: Option<&glide_ipc::Response>) -> u32 {
    match response {
        Some(glide_ipc::Response::Error { message }) if message == "cancelled" => CANCELLED,
        _ => FAILED,
    }
}

struct Screenshot;

// Argument lists are fixed by org.freedesktop.impl.portal.Screenshot.
#[allow(clippy::unused_self, clippy::needless_pass_by_value)]
#[zbus::interface(name = "org.freedesktop.impl.portal.Screenshot")]
impl Screenshot {
    async fn screenshot(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String, options: ScreenshotOptions,
        #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, ScreenshotResults) {
        let _busy = Busy::start();
        drop((handle, app_id, parent_window, options.modal));
        if !from_frontend(&header, connection).await {
            return (FAILED, ScreenshotResults::default());
        }
        let (request, timeout) = if options.interactive.unwrap_or(false) {
            (glide_ipc::Request::ScreenshotArea, PICK_TIMEOUT)
        } else {
            (glide_ipc::Request::Screenshot, SCREENSHOT_TIMEOUT)
        };
        match ask(request, timeout).await {
            Some(glide_ipc::Response::Screenshot { path }) => (
                SUCCESS,
                ScreenshotResults { uri: Some(glide_app::integration::file_uri(std::path::Path::new(&path))) },
            ),
            other => (code_for(other.as_ref()), ScreenshotResults::default()),
        }
    }

    async fn pick_color(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String,
        options: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, ColorResults) {
        let _busy = Busy::start();
        drop((handle, app_id, parent_window, options));
        if !from_frontend(&header, connection).await {
            return (FAILED, ColorResults::default());
        }
        match ask(glide_ipc::Request::PickColor, PICK_TIMEOUT).await {
            Some(glide_ipc::Response::Color { red, green, blue }) => {
                (SUCCESS, ColorResults { color: Some((red, green, blue)) })
            }
            other => (code_for(other.as_ref()), ColorResults::default()),
        }
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
struct ScreenCastSession {
    pub app_id: String,
    pub types: u32,
    pub multiple: bool,
    pub cursor_mode: u32,
    pub stream_id: Option<u64>,
}

struct PortalSession {
    session_handle: String,
    sessions: Arc<Mutex<HashMap<String, ScreenCastSession>>>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Session")]
impl PortalSession {
    async fn close(
        &self, #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) {
        if !from_frontend(&header, connection).await {
            return;
        }
        let stream_id = if let Ok(mut map) = self.sessions.lock() {
            map.remove(&self.session_handle).and_then(|s| s.stream_id)
        } else {
            None
        };
        if let Some(stream_id) = stream_id {
            let _ = blocking("glide-portal-session-close", move || {
                glide_ipc::client::exchange(
                    &glide_ipc::Request::ScreenShareStop { stream_id: Some(stream_id) },
                    Duration::from_secs(1),
                )
            })
            .await;
        }
    }
}

fn parse_select_sources_options(options: &HashMap<String, OwnedValue>) -> (u32, bool, u32) {
    let types = options
        .get("types")
        .and_then(|v| {
            u32::try_from(v)
                .ok()
                .or_else(|| i32::try_from(v).ok().map(|i| i as u32))
                .or_else(|| u64::try_from(v).ok().map(|u| u as u32))
        })
        .unwrap_or(1 | 2 | 4);
    let multiple = options.get("multiple").and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
    let cursor_mode = options
        .get("cursor_mode")
        .and_then(|v| {
            u32::try_from(v)
                .ok()
                .or_else(|| i32::try_from(v).ok().map(|i| i as u32))
                .or_else(|| u64::try_from(v).ok().map(|u| u as u32))
        })
        .unwrap_or(1);
    (types, multiple, cursor_mode)
}

type PortalStreams = Vec<(u32, HashMap<String, OwnedValue>)>;

fn format_screencast_streams(info: &glide_ipc::ShareStreamInfo) -> (PortalStreams, String) {
    let source_type_u32 = match info.source_kind {
        glide_ipc::ShareSourceKind::Screen | glide_ipc::ShareSourceKind::FilteredDesktop => 1u32,
        glide_ipc::ShareSourceKind::Window => 2u32,
        glide_ipc::ShareSourceKind::MultiWindow => 4u32,
    };
    let mut props = HashMap::new();
    let size_val = zbus::zvariant::Value::from((info.width as i32, info.height as i32));
    if let Ok(owned) = OwnedValue::try_from(size_val) {
        props.insert("size".to_string(), owned);
    }
    let type_val = zbus::zvariant::Value::from(source_type_u32);
    if let Ok(owned) = OwnedValue::try_from(type_val) {
        props.insert("source_type".to_string(), owned);
    }
    let restore_token = format!("glide-stream-{}", info.stream_id);
    (vec![(info.node_id, props)], restore_token)
}

struct ScreenCast {
    sessions: Arc<Mutex<HashMap<String, ScreenCastSession>>>,
}

impl ScreenCast {
    fn new(sessions: Arc<Mutex<HashMap<String, ScreenCastSession>>>) -> Self {
        Self { sessions }
    }
}

#[allow(clippy::too_many_arguments, clippy::unused_self)]
#[zbus::interface(name = "org.freedesktop.impl.portal.ScreenCast")]
impl ScreenCast {
    async fn create_session(
        &self, handle: OwnedObjectPath, session_handle: OwnedObjectPath, app_id: String,
        options: HashMap<String, OwnedValue>, #[zbus(object_server)] server: &ObjectServer,
        #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let _busy = Busy::start();
        drop((handle, options));
        if !from_frontend(&header, connection).await {
            return (FAILED, HashMap::new());
        }
        let session_key = session_handle.as_str().to_string();
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(
                session_key.clone(),
                ScreenCastSession { app_id, types: 1 | 2 | 4, multiple: false, cursor_mode: 1, stream_id: None },
            );
        }
        let session_obj = PortalSession { session_handle: session_key, sessions: self.sessions.clone() };
        let _ = server.at(session_handle.clone(), session_obj).await;

        let mut results = HashMap::new();
        let val = zbus::zvariant::Value::from(session_handle);
        if let Ok(owned) = OwnedValue::try_from(val) {
            results.insert("session_id".to_string(), owned);
        }
        (SUCCESS, results)
    }

    async fn select_sources(
        &self, handle: OwnedObjectPath, session_handle: OwnedObjectPath, app_id: String,
        options: HashMap<String, OwnedValue>, #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let _busy = Busy::start();
        drop(handle);
        if !from_frontend(&header, connection).await {
            return (FAILED, HashMap::new());
        }
        let (types, multiple, cursor_mode) = parse_select_sources_options(&options);
        let session_key = session_handle.as_str().to_string();
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get_mut(&session_key) {
                session.types = types;
                session.multiple = multiple;
                session.cursor_mode = cursor_mode;
            } else {
                sessions
                    .insert(session_key, ScreenCastSession { app_id, types, multiple, cursor_mode, stream_id: None });
            }
        }
        (SUCCESS, HashMap::new())
    }

    async fn start(
        &self, handle: OwnedObjectPath, session_handle: OwnedObjectPath, app_id: String, parent_window: String,
        options: HashMap<String, OwnedValue>, #[zbus(object_server)] server: &ObjectServer,
        #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let _busy = Busy::start();
        drop((parent_window, options));
        if !from_frontend(&header, connection).await {
            return (FAILED, HashMap::new());
        }
        let session_key = session_handle.as_str().to_string();
        let (multiple, cursor_mode) = if let Ok(sessions) = self.sessions.lock() {
            sessions.get(&session_key).map_or((false, 1), |s| (s.multiple, s.cursor_mode))
        } else {
            (false, 1)
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        let c = cancelled.clone();
        let portal_request = PortalRequest {
            process: None,
            pid: 0,
            reaped: Arc::default(),
            cancel: Some(Arc::new(move || {
                c.store(true, Ordering::SeqCst);
                let _ = glide_ipc::client::exchange(
                    &glide_ipc::Request::ScreenShareStop { stream_id: None },
                    Duration::from_millis(500),
                );
            })),
        };
        let registered = matches!(server.at(handle.clone(), portal_request).await, Ok(true));

        let req =
            glide_ipc::Request::ScreenShareStart { app_id: Some(app_id), multiple, cursor: Some(cursor_mode != 1) };
        let response =
            blocking("glide-portal-screen-share", move || glide_ipc::client::exchange(&req, PICK_TIMEOUT)).await;

        if registered {
            let _ = server.remove::<PortalRequest, _>(&handle).await;
        }

        if cancelled.load(Ordering::SeqCst) {
            return (CANCELLED, HashMap::new());
        }

        let Some(Ok(glide_ipc::Response::ScreenShareStarted(info))) = response else {
            return (CANCELLED, HashMap::new());
        };

        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(session) = sessions.get_mut(&session_key)
        {
            session.stream_id = Some(info.stream_id);
        }

        let (streams, restore_token) = format_screencast_streams(&info);
        let mut results = HashMap::new();
        let streams_val = zbus::zvariant::Value::from(streams);
        if let Ok(owned) = OwnedValue::try_from(streams_val) {
            results.insert("streams".to_string(), owned);
        }
        let restore_token_val = zbus::zvariant::Value::from(restore_token);
        if let Ok(owned) = OwnedValue::try_from(restore_token_val) {
            results.insert("restore_token".to_string(), owned);
        }

        (SUCCESS, results)
    }

    #[zbus(property)]
    fn available_source_types(&self) -> u32 {
        1 | 2 | 4
    }

    #[zbus(property)]
    fn available_cursor_modes(&self) -> u32 {
        1 | 2
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        4
    }
}

fn serve_with_sessions(
    builder: zbus::blocking::connection::Builder<'_>, sessions: Arc<Mutex<HashMap<String, ScreenCastSession>>>,
) -> zbus::Result<zbus::blocking::Connection> {
    builder
        .name(BUS_NAME)?
        .serve_at(PATH, FileChooser)?
        .serve_at(PATH, Screenshot)?
        .serve_at(PATH, ScreenCast::new(sessions))?
        .build()
}

fn serve(builder: zbus::blocking::connection::Builder<'_>) -> zbus::Result<zbus::blocking::Connection> {
    serve_with_sessions(builder, Arc::new(Mutex::new(HashMap::new())))
}

fn main() -> zbus::Result<()> {
    let connection = serve(zbus::blocking::connection::Builder::session()?)?;
    touch();
    while !idle() {
        std::thread::sleep(IDLE_CHECK);
    }
    let _ = connection.release_name(BUS_NAME);
    loop {
        std::thread::sleep(DRAIN);
        if IN_FLIGHT.load(Ordering::SeqCst) == 0 {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_options_become_a_chooser_request() {
        let options = Options {
            accept_label: Some("Attach".into()),
            multiple: Some(true),
            filters: Some(vec![("Images".into(), vec![(0, "*.png".into())]), ("All".into(), vec![(0, "*".into())])]),
            current_filter: Some(("All".into(), vec![(0, "*".into())])),
            current_file: Some(b"/home/me/Pictures/cat.png\0".to_vec()),
            ..Options::default()
        };
        let request = request(Mode::Save, "wayland:xyz".into(), "Save".into(), options);
        assert_eq!(request.accept_label.as_deref(), Some("Attach"));
        assert!(request.multiple);
        assert_eq!(request.current_filter, Some(1));
        assert_eq!(request.current_folder, Some(PathBuf::from("/home/me/Pictures")));
        assert_eq!(request.current_name.as_deref(), Some("cat.png"));
        assert_eq!(request.wayland_parent(), Some("xyz"));
    }

    #[test]
    fn a_current_filter_without_a_filter_list_still_applies() {
        let options =
            Options { current_filter: Some(("Text".into(), vec![(0, "*.txt".into())])), ..Options::default() };
        let request = request(Mode::Open, String::new(), String::new(), options);
        assert_eq!(request.filters, [Filter::from_portal("Text".into(), vec![(0, "*.txt".into())])]);
        assert_eq!(request.current_filter, Some(0));
    }

    struct Daemon(std::process::Child, String);

    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn private_bus() -> Option<Daemon> {
        use std::io::BufRead;
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut address = String::new();
        std::io::BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
        Some(Daemon(child, address.trim().to_string()))
    }

    #[test]
    fn a_portal_call_returns_what_the_chooser_printed() {
        let Some(bus) = private_bus() else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("glide-portal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        let script = dir.join("chooser");
        let answer = Response {
            uris: vec!["file:///tmp/picked.txt".into()],
            filter: Some(0),
            choices: vec![("encoding".into(), "latin1".into())],
        }
        .to_json();
        std::fs::write(&script, format!("#!/bin/sh\necho '{answer}'\n")).expect("script");
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("mode");
        // SAFETY: set before the portal's own threads exist and read back only by it in this test.
        unsafe { std::env::set_var("GLIDE_PORTAL_CHOOSER", &script) };

        let builder = zbus::blocking::connection::Builder::address(bus.1.as_str()).expect("address");
        let portal = serve(builder).expect("serve");
        let client = zbus::blocking::connection::Builder::address(bus.1.as_str())
            .expect("address")
            .name(FRONTEND)
            .expect("frontend name")
            .build()
            .expect("client");
        let filters: Vec<PortalFilter> = vec![("Text".into(), vec![(0, "*.txt".into())])];
        let options = std::collections::HashMap::from([
            ("filters", zbus::zvariant::Value::from(filters)),
            ("multiple", zbus::zvariant::Value::from(true)),
        ]);
        let handle =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/request/1_1/t").expect("path");
        let reply = client
            .call_method(
                Some(BUS_NAME),
                PATH,
                Some("org.freedesktop.impl.portal.FileChooser"),
                "OpenFile",
                &(handle.clone(), "app", "", "Pick", options),
            )
            .expect("reply");
        let (code, results): (u32, std::collections::HashMap<String, zbus::zvariant::OwnedValue>) =
            reply.body().deserialize().expect("body");
        assert_eq!(code, SUCCESS);
        let uris: Vec<String> = results["uris"].try_clone().expect("clone").try_into().expect("uris");
        assert_eq!(uris, ["file:///tmp/picked.txt"]);
        assert!(results.contains_key("current_filter"));
        let choices: Vec<(String, String)> =
            results["choices"].try_clone().expect("clone").try_into().expect("choices");
        assert_eq!(choices, [("encoding".to_string(), "latin1".to_string())]);
        let stale = PortalRequest { process: None, pid: 0, reaped: Arc::default(), cancel: None };
        assert!(portal.object_server().at(handle.clone(), stale).unwrap());
        let reply = client
            .call_method(
                Some(BUS_NAME),
                PATH,
                Some("org.freedesktop.impl.portal.FileChooser"),
                "OpenFile",
                &(
                    handle.clone(),
                    "app",
                    "",
                    "Pick",
                    std::collections::HashMap::<String, zbus::zvariant::Value<'_>>::new(),
                ),
            )
            .expect("duplicate handle reply");
        let (code, _): (u32, std::collections::HashMap<String, zbus::zvariant::OwnedValue>) =
            reply.body().deserialize().expect("body");
        assert_eq!(code, FAILED);
        assert!(portal.object_server().interface::<_, PortalRequest>(handle).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_the_portal_frontend_may_call_the_backend() {
        let Some(bus) = private_bus() else {
            return;
        };
        let builder = zbus::blocking::connection::Builder::address(bus.1.as_str()).expect("address");
        let portal = serve(builder).expect("serve");
        let intruder =
            zbus::blocking::connection::Builder::address(bus.1.as_str()).expect("address").build().expect("client");
        let handle =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/request/1_2/t").expect("path");
        let empty = std::collections::HashMap::<String, zbus::zvariant::Value<'_>>::new();
        for (interface, method) in [
            ("org.freedesktop.impl.portal.Screenshot", "Screenshot"),
            ("org.freedesktop.impl.portal.Screenshot", "PickColor"),
        ] {
            let reply = intruder
                .call_method(Some(BUS_NAME), PATH, Some(interface), method, &(handle.clone(), "app", "", &empty))
                .expect("reply");
            let (code, results): (u32, std::collections::HashMap<String, zbus::zvariant::OwnedValue>) =
                reply.body().deserialize().expect("body");
            assert_eq!((code, results.len()), (FAILED, 0), "{method} from a stranger must be refused");
        }
        let reply = intruder
            .call_method(
                Some(BUS_NAME),
                PATH,
                Some("org.freedesktop.impl.portal.FileChooser"),
                "OpenFile",
                &(handle, "app", "", "Pick", &empty),
            )
            .expect("reply");
        let (code, _): (u32, std::collections::HashMap<String, zbus::zvariant::OwnedValue>) =
            reply.body().deserialize().expect("body");
        assert_eq!(code, FAILED, "a stranger must not open a chooser");

        let frontend = zbus::blocking::connection::Builder::address(bus.1.as_str())
            .expect("address")
            .name(FRONTEND)
            .expect("frontend name")
            .build()
            .expect("frontend");
        let call_from = |connection: &zbus::blocking::Connection| {
            let sender = connection.unique_name().expect("unique name").to_owned();
            let message = zbus::message::Message::method_call(PATH, "Screenshot")
                .expect("call")
                .sender(sender)
                .expect("sender")
                .build(&())
                .expect("message");
            zbus::block_on(from_frontend(&message.header(), portal.inner()))
        };
        assert!(call_from(&frontend), "the owner of {FRONTEND} passes");
        assert!(!call_from(&intruder), "anyone else is refused");

        let mut chooser = Command::new("sleep").arg("60").spawn().expect("stand-in chooser");
        let open = PortalRequest {
            process: pidfd(chooser.id()),
            pid: libc::pid_t::try_from(chooser.id()).expect("pid"),
            reaped: Arc::default(),
            cancel: None,
        };
        let handle =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/request/1_2/open").expect("path");
        assert!(portal.object_server().at(handle.clone(), open).expect("request object"));
        let close = |connection: &zbus::blocking::Connection| {
            connection
                .call_method(Some(BUS_NAME), handle.clone(), Some("org.freedesktop.impl.portal.Request"), "Close", &())
                .expect("close reply");
        };
        close(&intruder);
        assert!(chooser.try_wait().expect("wait").is_none(), "a stranger cannot close someone's file dialog");
        close(&frontend);
        assert_eq!(chooser.wait().expect("wait").signal(), Some(libc::SIGTERM));
    }

    #[test]
    fn a_folder_that_is_not_utf8_is_left_out_of_the_chooser_request() {
        let options = Options {
            current_folder: Some(b"/tmp/\xff\0".to_vec()),
            current_name: Some("a.txt".into()),
            ..Options::default()
        };
        let request = request(Mode::Save, String::new(), String::new(), options);
        assert_eq!(request.current_folder, None);
        assert!(request.to_json().is_some(), "the rest of the request still reaches the chooser");
    }

    #[test]
    fn only_the_choosers_own_cancel_counts_as_cancelled() {
        assert_eq!(chooser_code(ExitStatus::from_raw(CANCEL_EXIT << 8)), CANCELLED);
        assert_eq!(chooser_code(ExitStatus::from_raw(libc::SIGTERM)), CANCELLED);
        assert_eq!(chooser_code(ExitStatus::from_raw(glide_app::chooser::FAILURE_EXIT << 8)), FAILED);
        assert_eq!(chooser_code(ExitStatus::from_raw(101 << 8)), FAILED);
        assert_eq!(chooser_code(ExitStatus::from_raw(libc::SIGSEGV)), FAILED);
    }

    #[test]
    fn save_files_keeps_only_the_names() {
        let options = Options { files: Some(vec![b"a.txt\0".to_vec(), b"/x/b.txt".to_vec()]), ..Options::default() };
        let request = request(Mode::SaveFiles, String::new(), String::new(), options);
        assert_eq!(request.files, ["a.txt", "b.txt"]);
        assert_eq!(request.parent, None);
    }

    #[test]
    fn screencast_session_creation_and_options_parsing() {
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let screencast = ScreenCast::new(sessions);

        assert_eq!(screencast.available_source_types(), 1 | 2 | 4);
        assert_eq!(screencast.available_cursor_modes(), 1 | 2);
        assert_eq!(screencast.version(), 4);

        let mut opts = HashMap::new();
        opts.insert("types".to_string(), OwnedValue::try_from(zbus::zvariant::Value::from(2u32)).unwrap());
        opts.insert("multiple".to_string(), OwnedValue::try_from(zbus::zvariant::Value::from(true)).unwrap());
        opts.insert("cursor_mode".to_string(), OwnedValue::try_from(zbus::zvariant::Value::from(2u32)).unwrap());

        let (types, multiple, cursor_mode) = parse_select_sources_options(&opts);
        assert_eq!(types, 2);
        assert!(multiple);
        assert_eq!(cursor_mode, 2);

        let empty = HashMap::new();
        let (def_types, def_multiple, def_cursor) = parse_select_sources_options(&empty);
        assert_eq!(def_types, 1 | 2 | 4);
        assert!(!def_multiple);
        assert_eq!(def_cursor, 1);
    }

    #[test]
    fn screencast_response_construction() {
        let info = glide_ipc::ShareStreamInfo {
            stream_id: 123,
            node_id: 456,
            source_kind: glide_ipc::ShareSourceKind::Window,
            width: 1920,
            height: 1080,
            window_count: 1,
            app_id: Some("org.example.App".into()),
        };
        let (streams, token) = format_screencast_streams(&info);
        assert_eq!(token, "glide-stream-123");
        assert_eq!(streams.len(), 1);
        let (node_id, props) = &streams[0];
        assert_eq!(*node_id, 456);

        let size: (i32, i32) = props["size"].try_clone().unwrap().try_into().unwrap();
        assert_eq!(size, (1920, 1080));

        let source_type: u32 = props["source_type"].try_clone().unwrap().try_into().unwrap();
        assert_eq!(source_type, 2);

        let screen_info = glide_ipc::ShareStreamInfo {
            stream_id: 7,
            node_id: 8,
            source_kind: glide_ipc::ShareSourceKind::Screen,
            width: 2560,
            height: 1440,
            window_count: 0,
            app_id: None,
        };
        let (screen_streams, _) = format_screencast_streams(&screen_info);
        let screen_source_type: u32 = screen_streams[0].1["source_type"].try_clone().unwrap().try_into().unwrap();
        assert_eq!(screen_source_type, 1);

        let multi_info = glide_ipc::ShareStreamInfo {
            stream_id: 9,
            node_id: 10,
            source_kind: glide_ipc::ShareSourceKind::MultiWindow,
            width: 1280,
            height: 720,
            window_count: 3,
            app_id: None,
        };
        let (multi_streams, _) = format_screencast_streams(&multi_info);
        let multi_source_type: u32 = multi_streams[0].1["source_type"].try_clone().unwrap().try_into().unwrap();
        assert_eq!(multi_source_type, 4);
    }

    #[test]
    fn screencast_portal_dbus_lifecycle() {
        let Some(bus) = private_bus() else {
            return;
        };
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let builder = zbus::blocking::connection::Builder::address(bus.1.as_str()).expect("address");
        let _portal = serve_with_sessions(builder, sessions.clone()).expect("serve");

        let client = zbus::blocking::connection::Builder::address(bus.1.as_str())
            .expect("address")
            .name(FRONTEND)
            .expect("frontend name")
            .build()
            .expect("client");

        let handle =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/request/1_3/sc_create").unwrap();
        let session_handle =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/session/1_3/s1").unwrap();

        let create_reply = client
            .call_method(
                Some(BUS_NAME),
                PATH,
                Some("org.freedesktop.impl.portal.ScreenCast"),
                "CreateSession",
                &(
                    handle.clone(),
                    session_handle.clone(),
                    "org.example.screen",
                    HashMap::<String, zbus::zvariant::Value<'_>>::new(),
                ),
            )
            .expect("create reply");
        let (code, results): (u32, HashMap<String, OwnedValue>) = create_reply.body().deserialize().expect("body");
        assert_eq!(code, SUCCESS);
        assert!(results.contains_key("session_id"));

        let sess_key = session_handle.as_str().to_string();
        {
            let guard = sessions.lock().unwrap();
            let sess = guard.get(&sess_key).expect("session exists");
            assert_eq!(sess.app_id, "org.example.screen");
        }

        let mut select_opts = HashMap::new();
        select_opts.insert("multiple", zbus::zvariant::Value::from(true));
        select_opts.insert("cursor_mode", zbus::zvariant::Value::from(2u32));
        select_opts.insert("types", zbus::zvariant::Value::from(2u32));

        let sel_handle =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/portal/desktop/request/1_3/sc_select").unwrap();
        let select_reply = client
            .call_method(
                Some(BUS_NAME),
                PATH,
                Some("org.freedesktop.impl.portal.ScreenCast"),
                "SelectSources",
                &(sel_handle, session_handle.clone(), "org.example.screen", select_opts),
            )
            .expect("select reply");
        let (code, _): (u32, HashMap<String, OwnedValue>) = select_reply.body().deserialize().expect("body");
        assert_eq!(code, SUCCESS);

        {
            let guard = sessions.lock().unwrap();
            let sess = guard.get(&sess_key).expect("session exists");
            assert!(sess.multiple);
            assert_eq!(sess.cursor_mode, 2);
            assert_eq!(sess.types, 2);
        }
    }
}
