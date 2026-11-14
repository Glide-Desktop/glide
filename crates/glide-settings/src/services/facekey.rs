use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::Engine;
use glide_config::facekey::{Attempt, Camera, Event, PamService, Status};
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream};

use super::run::{output_within, status_within};

const QUERY: Duration = Duration::from_secs(5);
const CHANGE: Duration = Duration::from_mins(2);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub installed: bool,
    pub status: Option<Status>,
    pub cameras: Vec<Camera>,
    pub attempts: Vec<Attempt>,
    pub pam: BTreeMap<String, Option<bool>>,
}

impl Snapshot {
    #[must_use]
    pub fn enrolled(&self) -> bool {
        self.status.as_ref().is_some_and(|status| status.enrolled)
    }

    #[must_use]
    pub fn stale(&self) -> bool {
        self.status.as_ref().is_some_and(|status| status.enrolled && status.compatible == Some(false))
    }

    #[must_use]
    pub fn pam(&self, service: PamService) -> Option<bool> {
        self.pam.get(service.source().name()).copied().flatten()
    }
}

#[must_use]
pub fn binary() -> Option<PathBuf> {
    glide_config::facekey::binary()
}

fn query<T: serde::de::DeserializeOwned>(binary: &str, flag: &str) -> Option<T> {
    serde_json::from_str(&output_within(binary, &[flag], QUERY)?).ok()
}

#[must_use]
pub fn snapshot() -> Snapshot {
    let Some(binary) = binary() else { return Snapshot::default() };
    let binary = binary.to_string_lossy().into_owned();
    Snapshot {
        installed: true,
        status: query(&binary, "-status"),
        cameras: query(&binary, "-cameras").unwrap_or_default(),
        attempts: query(&binary, "-attempts").unwrap_or_default(),
        pam: query(&binary, "-pam-status").unwrap_or_default(),
    }
}

fn privileged(args: &[&str]) -> Result<String, String> {
    let binary = binary().ok_or_else(|| "facekey is not installed".to_string())?;
    let binary = binary.to_string_lossy().into_owned();
    let mut full = vec![binary.as_str()];
    full.extend_from_slice(args);
    status_within("pkexec", &full, CHANGE)
}

pub fn delete() -> Result<String, String> {
    privileged(&["-delete-profile"])
}

pub fn set_pam(service: PamService, on: bool) -> Result<String, String> {
    privileged(&[if on { "-pam-enable" } else { "-pam-disable" }, service.source().name()])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Run {
    Enroll,
    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Session {
    pub run: Run,
    pub camera: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub enum Update {
    Event(Event),
    Frame(iced::widget::image::Handle),
    Ended(bool),
}

fn command(session: &Session) -> Option<Command> {
    let binary = binary()?;
    let mut command = match session.run {
        Run::Enroll => {
            let mut command = Command::new("pkexec");
            command.arg(binary).args(["-enroll", "-stream"]);
            command
        }
        Run::Verify => {
            let mut command = Command::new(binary);
            command.args(["-verify", "-stream", "-source", "test"]);
            command
        }
    };
    if let Some(camera) = &session.camera {
        command.args(["-camera", camera]);
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    Some(command)
}

#[must_use]
pub fn decode(line: &str) -> Option<Update> {
    match serde_json::from_str::<Event>(line).ok()? {
        Event::Frame { jpeg, .. } => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(jpeg).ok()?;
            Some(Update::Frame(iced::widget::image::Handle::from_bytes(bytes)))
        }
        event => Some(Update::Event(event)),
    }
}

fn pump(session: &Session, sender: &mut Sender<Update>) -> bool {
    let Some(mut child) = command(session).and_then(|mut command| command.spawn().ok()) else { return false };
    let Some(stdout) = child.stdout.take() else { return false };
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if let Some(update) = decode(&line)
            && sender.try_send(update).is_err()
            && sender.is_closed()
        {
            break;
        }
    }
    child.wait().is_ok_and(|status| status.success())
}

pub fn stream(session: &Session) -> impl Stream<Item = Update> + use<> {
    let session = session.clone();
    iced::stream::channel(64, async move |mut output| {
        let mut sender = output.clone();
        std::thread::spawn(move || {
            let ok = pump(&session, &mut sender);
            let _ = sender.try_send(Update::Ended(ok));
        });
        let _ = output.flush().await;
        std::future::pending::<()>().await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glide_config::facekey::Reason;

    #[test]
    fn stream_lines_become_updates() {
        let line = r#"{"kind":"enroll","step":1,"hold":2,"required":4,"face":true,"hint":"too-dark"}"#;
        let Some(Update::Event(Event::Enroll { step, hint, .. })) = decode(line) else { panic!("not an enroll event") };
        assert_eq!((step, hint), (1, Some(Reason::TooDark)));
        assert!(matches!(decode(r#"{"kind":"frame","width":1,"height":1,"jpeg":"/9j/"}"#), Some(Update::Frame(_))));
        assert!(decode("[INFO] text").is_none());
    }

    #[test]
    fn a_stale_profile_is_enrolled_but_incompatible() {
        let snapshot = Snapshot {
            status: Some(Status { enrolled: true, compatible: Some(false), ..Status::default() }),
            ..Snapshot::default()
        };
        assert!(snapshot.enrolled() && snapshot.stale());
    }
}
