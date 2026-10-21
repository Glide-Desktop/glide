use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use glide_app::chooser::{FLAG, Filter, Mode, REQUEST_ENV, Request, Response};
use zbus::object_server::ObjectServer;
use zbus::zvariant::{DeserializeDict, OwnedObjectPath, SerializeDict, Type};

const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.glide";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SUCCESS: u32 = 0;
const CANCELLED: u32 = 1;
const FAILED: u32 = 2;

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
    let filters: Vec<Filter> =
        options.filters.unwrap_or_default().into_iter().map(|(name, list)| Filter::from_portal(name, list)).collect();
    let current_filter = options.current_filter.and_then(|(name, list)| {
        let wanted = Filter::from_portal(name, list);
        filters.iter().position(|filter| *filter == wanted)
    });
    let current_file = options.current_file.as_deref().and_then(path_bytes);
    let current_folder = options
        .current_folder
        .as_deref()
        .and_then(path_bytes)
        .or_else(|| current_file.as_deref().and_then(std::path::Path::parent).map(std::path::Path::to_path_buf));
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
    pid: u32,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl PortalRequest {
    fn close(&self) {
        if let Ok(pid) = i32::try_from(self.pid) {
            // SAFETY: plain kill(2) on the chooser process this request started.
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
}

async fn run(server: &ObjectServer, handle: OwnedObjectPath, request: Request) -> (u32, Results) {
    let filters = request.filters.clone();
    let spawned = Command::new(files_binary())
        .arg(FLAG)
        .env(REQUEST_ENV, request.to_json())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(child) = spawned else {
        return (FAILED, Results::default());
    };
    let registered = server.at(handle.clone(), PortalRequest { pid: child.id() }).await.is_ok();
    let (sender, receiver) = futures_channel::oneshot::channel();
    let waiter = std::thread::Builder::new().name("glide-portal-wait".into()).spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    let output = if waiter.is_ok() { receiver.await.ok() } else { None };
    if registered {
        let _ = server.remove::<PortalRequest, _>(&handle).await;
    }
    match output {
        Some(Ok(output)) if output.status.success() => {
            match Response::parse(&String::from_utf8_lossy(&output.stdout)) {
                Some(response) => (
                    SUCCESS,
                    Results {
                        uris: response.uris,
                        current_filter: response.filter.and_then(|index| filters.get(index)).map(Filter::to_portal),
                        choices: (!response.choices.is_empty()).then_some(response.choices),
                    },
                ),
                None => (CANCELLED, Results::default()),
            }
        }
        Some(Ok(_)) => (CANCELLED, Results::default()),
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
        #[zbus(object_server)] server: &ObjectServer,
    ) -> (u32, Results) {
        drop(app_id);
        run(server, handle, request(Mode::Open, parent_window, title, options)).await
    }

    async fn save_file(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String, title: String, options: Options,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> (u32, Results) {
        drop(app_id);
        run(server, handle, request(Mode::Save, parent_window, title, options)).await
    }

    async fn save_files(
        &self, handle: OwnedObjectPath, app_id: String, parent_window: String, title: String, options: Options,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> (u32, Results) {
        drop(app_id);
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

const PICK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

async fn ask(request: glide_ipc::Request, timeout: std::time::Duration) -> Option<glide_ipc::Response> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::Builder::new()
        .name("glide-portal-screen".into())
        .spawn(move || {
            let _ = sender.send(glide_ipc::client::exchange(&request, timeout));
        })
        .ok()?;
    receiver.await.ok()?.ok()
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
    ) -> (u32, ScreenshotResults) {
        drop((handle, app_id, parent_window, options.modal));
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
    ) -> (u32, ColorResults) {
        drop((handle, app_id, parent_window, options));
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

fn serve(builder: zbus::blocking::connection::Builder<'_>) -> zbus::Result<zbus::blocking::Connection> {
    builder.name(BUS_NAME)?.serve_at(PATH, FileChooser)?.serve_at(PATH, Screenshot)?.build()
}

fn main() -> zbus::Result<()> {
    let _connection = serve(zbus::blocking::connection::Builder::session()?)?;
    loop {
        std::thread::park();
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
        let _portal = serve(builder).expect("serve");
        let client =
            zbus::blocking::connection::Builder::address(bus.1.as_str()).expect("address").build().expect("client");
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
                &(handle, "app", "", "Pick", options),
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
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_files_keeps_only_the_names() {
        let options = Options { files: Some(vec![b"a.txt\0".to_vec(), b"/x/b.txt".to_vec()]), ..Options::default() };
        let request = request(Mode::SaveFiles, String::new(), String::new(), options);
        assert_eq!(request.files, ["a.txt", "b.txt"]);
        assert_eq!(request.parent, None);
    }
}
