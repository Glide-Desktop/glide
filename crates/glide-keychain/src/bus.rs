use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use glide_ipc::keychain::{Hidden, Request, Response};
use zbus::fdo;
use zbus::message::Header;

use crate::daemon::{APP_ATTRIBUTE, Daemon, Login, PROGRAM_ATTRIBUTE};

pub const BUS_NAME: &str = "org.glide.Keychain";
pub const PATH: &str = "/org/glide/Keychain1";

pub type Events = Arc<dyn Fn(Response) + Send + Sync>;

pub struct Keychain {
    daemon: Arc<Mutex<Daemon>>,
    events: Events,
    allowed: Vec<PathBuf>,
}

impl Keychain {
    #[must_use]
    pub fn new(daemon: Arc<Mutex<Daemon>>, events: Events, allowed: Vec<PathBuf>) -> Self {
        Self { daemon, events, allowed }
    }

    async fn authorize(&self, header: &Header<'_>, connection: &zbus::Connection) -> fdo::Result<()> {
        let sender = header.sender().ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?.to_owned();
        let pid = fdo::DBusProxy::new(connection).await?.get_connection_unix_process_id(sender.into()).await?;
        let program = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        if program.is_some_and(|program| self.allowed.contains(&program)) {
            Ok(())
        } else {
            Err(fdo::Error::AccessDenied("only Glide Settings may manage the keychain".into()))
        }
    }

    fn run(&self, request: Request) -> Response {
        self.daemon.lock().unwrap_or_else(PoisonError::into_inner).handle(request)
    }

    fn with<R>(&self, work: impl FnOnce(&mut Daemon) -> R) -> R {
        work(&mut self.daemon.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

fn failed(response: Response) -> fdo::Error {
    match response {
        Response::Locked => fdo::Error::Failed("locked".into()),
        Response::WrongPassword => fdo::Error::AuthFailed("wrong password".into()),
        Response::Error { message } => fdo::Error::Failed(message),
        other => fdo::Error::Failed(format!("{other:?}")),
    }
}

fn done(response: Response) -> fdo::Result<()> {
    match response {
        Response::Done => Ok(()),
        other => Err(failed(other)),
    }
}

type Row = (u64, String, String, String, String, u64);

#[zbus::interface(name = "org.glide.Keychain1")]
impl Keychain {
    async fn status(
        &self, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<(bool, bool)> {
        self.authorize(&header, connection).await?;
        match self.run(Request::Status) {
            Response::Status { exists, unlocked } => Ok((exists, unlocked)),
            other => Err(failed(other)),
        }
    }

    async fn create(
        &self, password: String, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        self.authorize(&header, connection).await?;
        done(self.run(Request::Create { password: Hidden::new(password) }))
    }

    async fn request_unlock(
        &self, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        self.authorize(&header, connection).await?;
        (self.events)(Response::UnlockRequested);
        Ok(())
    }

    async fn lock(
        &self, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        self.authorize(&header, connection).await?;
        done(self.run(Request::Lock))
    }

    async fn items(
        &self, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<Vec<Row>> {
        self.authorize(&header, connection).await?;
        self.with(|daemon| {
            let contents = daemon.contents().ok_or_else(|| failed(Response::Locked))?;
            Ok(contents
                .items
                .iter()
                .map(|item| {
                    let attribute = |key: &str| item.attributes.get(key).cloned().unwrap_or_default();
                    (
                        item.id,
                        item.label.clone(),
                        crate::daemon::username(item),
                        attribute(APP_ATTRIBUTE),
                        attribute(PROGRAM_ATTRIBUTE),
                        item.modified,
                    )
                })
                .collect())
        })
    }

    #[allow(clippy::too_many_arguments)] // D-Bus method arguments plus the two injected header and connection
    async fn add(
        &self, label: String, username: String, password: String, app: String, program: String,
        #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<u64> {
        self.authorize(&header, connection).await?;
        let login = Login { label, username, password: Hidden::new(password), app, program };
        self.with(|daemon| daemon.add_login(login)).map_err(failed)
    }

    #[allow(clippy::too_many_arguments)] // D-Bus method arguments plus the two injected header and connection
    async fn update(
        &self, id: u64, label: String, username: String, password: String, app: String, program: String,
        #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        self.authorize(&header, connection).await?;
        let login = Login { label, username, password: Hidden::new(password), app, program };
        done(self.with(|daemon| daemon.update_login(id, login)))
    }

    async fn delete(
        &self, id: u64, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<()> {
        self.authorize(&header, connection).await?;
        done(self.with(|daemon| daemon.delete(id)))
    }

    async fn password(
        &self, id: u64, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<String> {
        self.authorize(&header, connection).await?;
        match self.run(Request::Secret { id }) {
            Response::Secret { secret } => Ok(secret.expose().to_string()),
            other => Err(failed(other)),
        }
    }
}

pub fn serve(keychain: Keychain) -> zbus::Result<zbus::blocking::Connection> {
    serve_on(zbus::blocking::connection::Builder::session()?, keychain)
}

pub fn serve_on(
    builder: zbus::blocking::connection::Builder<'_>, keychain: Keychain,
) -> zbus::Result<zbus::blocking::Connection> {
    builder.name(BUS_NAME)?.serve_at(PATH, keychain)?.build()
}

#[must_use]
pub fn settings_program() -> Vec<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("glide-settings")))
        .and_then(|path| path.canonicalize().ok())
        .into_iter()
        .collect()
}
