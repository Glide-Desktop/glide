use std::path::PathBuf;
use std::sync::Arc;

use glide_ipc::keychain::{Request, Response};
use zbus::fdo;
use zbus::message::Header;

use crate::daemon::mutates;
pub use crate::service::Events;
use crate::service::Service;

pub use glide_ipc::keychain::{BUS_INTERFACE as INTERFACE, BUS_NAME, BUS_PATH as PATH};
pub const CLIENT: &str = "glide-passwords";

pub struct Keychain {
    service: Arc<Service>,
    events: Events,
    allowed: Vec<PathBuf>,
}

impl Keychain {
    #[must_use]
    pub fn new(service: Arc<Service>, events: Events, allowed: Vec<PathBuf>) -> Self {
        Self { service, events, allowed }
    }

    async fn authorize(&self, header: &Header<'_>, connection: &zbus::Connection) -> fdo::Result<()> {
        let sender = header.sender().ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?.to_owned();
        let pid = fdo::DBusProxy::new(connection).await?.get_connection_unix_process_id(sender.into()).await?;
        let program = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        if program.is_some_and(|program| self.allowed.contains(&program)) {
            Ok(())
        } else {
            Err(fdo::Error::AccessDenied("only Glide Passwords may open the keychain".into()))
        }
    }
}

#[zbus::interface(name = "org.glide.Keychain1")]
impl Keychain {
    async fn call(
        &self, request: String, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> fdo::Result<String> {
        self.authorize(&header, connection).await?;
        let request: Request = serde_json::from_str(&request)
            .map_err(|err| fdo::Error::InvalidArgs(format!("unreadable request: {err}")))?;
        let changes = mutates(&request);
        let response = match request {
            Request::RequestUnlock { vault } => {
                (self.events)(Response::UnlockRequested { vault, requester: None });
                Response::Done
            }
            request => {
                let service = Arc::clone(&self.service);
                blocking::unblock(move || service.handle(request)).await
            }
        };
        if changes {
            (self.events)(Response::Changed);
        }
        serde_json::to_string(&response).map_err(|err| fdo::Error::Failed(err.to_string()))
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

pub fn announce(connection: &zbus::blocking::Connection) {
    let _ = connection.emit_signal(None::<()>, PATH, INTERFACE, glide_ipc::keychain::CHANGED, &());
}

#[must_use]
pub fn client_program() -> Vec<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(CLIENT)))
        .and_then(|path| path.canonicalize().ok())
        .into_iter()
        .collect()
}
