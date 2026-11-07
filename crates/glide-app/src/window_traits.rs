use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::{BUS_NAME, INTERFACE, PATH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Aspect(u32, u32),
    KeepAbove(bool),
    PictureInPicture(bool),
    Blur(Vec<Region>),
}

impl Call {
    fn send(&self, connection: &zbus::blocking::Connection) -> zbus::Result<()> {
        let to = (Some(BUS_NAME), PATH, Some(INTERFACE));
        match self {
            Self::Aspect(width, height) => connection.call_method(to.0, to.1, to.2, "SetAspect", &(*width, *height)),
            Self::KeepAbove(keep) => connection.call_method(to.0, to.1, to.2, "SetKeepAbove", &(*keep,)),
            Self::PictureInPicture(on) => connection.call_method(to.0, to.1, to.2, "SetPictureInPicture", &(*on,)),
            Self::Blur(regions) => {
                let raw: Vec<(i32, i32, i32, i32, i32)> = regions
                    .iter()
                    .map(|region| (region.x, region.y, region.width, region.height, region.radius))
                    .collect();
                connection.call_method(to.0, to.1, to.2, "SetContentBlur", &(raw,))
            }
        }
        .map(drop)
    }
}

fn worker() -> Option<Sender<Call>> {
    static WORKER: OnceLock<Mutex<Option<Sender<Call>>>> = OnceLock::new();
    let slot = WORKER.get_or_init(|| {
        let (sender, receiver) = channel::<Call>();
        let spawned = std::thread::Builder::new().name("glide-window-traits".into()).spawn(move || {
            let mut connection: Option<zbus::blocking::Connection> = None;
            while let Ok(call) = receiver.recv() {
                if connection.is_none() {
                    connection = zbus::blocking::Connection::session().ok();
                }
                let Some(bus) = &connection else { continue };
                if let Err(err) = call.send(bus) {
                    tracing::debug!("the shell did not take {call:?}: {err}");
                    if matches!(err, zbus::Error::InputOutput(_)) {
                        connection = None;
                    }
                }
            }
        });
        Mutex::new(spawned.ok().map(|_| sender))
    });
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

pub fn request(call: Call) {
    if let Some(worker) = worker() {
        let _ = worker.send(call);
    }
}
