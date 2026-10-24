use std::sync::Arc;

use zbus::{
    blocking::{Connection, Proxy, connection::Builder},
    zvariant::{ObjectPath, OwnedObjectPath},
};

use crate::{
    agent_prompt::{Answer, Prompt, fill},
    i18n::{Key, t},
    oneshot,
};

pub const OWNER: &str = "bluetooth";
const AGENT_PATH: &str = "/org/glide/BluetoothAgent";
const CAPABILITY: &str = "KeyboardDisplay";
const BLUEZ: &str = "org.bluez";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Confirm { device: String, passkey: u32 },
    Authorize { device: String },
    AuthorizeService { device: String, service: String },
    PinCode { device: String },
    Passkey { device: String },
    ShowPinCode { device: String, pin: String },
    ShowPasskey { device: String, passkey: u32 },
}

#[derive(Debug)]
pub enum Event {
    Ask(Request, oneshot::Sender<Answer>),
    Show(Request),
    Cancel,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}

fn code(passkey: u32) -> String {
    format!("{passkey:06}")
}

impl Request {
    #[must_use]
    pub fn prompt(&self) -> Prompt {
        let ask = |summary: Key, body: Key, device: &str, code: &str, accept: Option<Key>| Prompt {
            icon: "bluetooth".into(),
            summary: fill(summary, device, code),
            body: fill(body, device, code),
            accept: accept.map(|key| t(key).to_string()),
            reply: None,
            secret: false,
        };
        let enter = |body: Key, device: &str, placeholder: &str| Prompt {
            reply: Some((t(Key::BtEnterCode).to_string(), placeholder.to_string())),
            ..ask(Key::BtCodeSummary, body, device, "", None)
        };
        match self {
            Self::Confirm { device, passkey } => {
                ask(Key::BtConfirmSummary, Key::BtConfirmBody, device, &code(*passkey), Some(Key::BtPair))
            }
            Self::Authorize { device } => {
                ask(Key::BtAuthorizeSummary, Key::BtAuthorizeBody, device, "", Some(Key::BtAllow))
            }
            Self::AuthorizeService { device, service } => {
                ask(Key::BtAuthorizeSummary, Key::BtServiceBody, device, service, Some(Key::BtAllow))
            }
            Self::PinCode { device } => enter(Key::BtPinBody, device, "0000"),
            Self::Passkey { device } => enter(Key::BtPasskeyBody, device, "123456"),
            Self::ShowPinCode { device, pin } => ask(Key::BtTypeSummary, Key::BtTypeBody, device, pin, None),
            Self::ShowPasskey { device, passkey } => {
                ask(Key::BtTypeSummary, Key::BtTypeBody, device, &code(*passkey), None)
            }
        }
    }
}

#[must_use]
pub fn parse_passkey(text: &str) -> Option<u32> {
    let text = text.trim();
    (!text.is_empty() && text.len() <= 6 && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

#[must_use]
pub fn parse_pin(text: &str) -> Option<String> {
    let text = text.trim();
    (1..=16).contains(&text.chars().count()).then(|| text.to_string())
}

fn address(path: &str) -> String {
    let leaf = path.rsplit('/').next().unwrap_or(path);
    leaf.strip_prefix("dev_").unwrap_or(leaf).replace('_', ":")
}

async fn device_name(connection: &zbus::Connection, path: &OwnedObjectPath) -> String {
    let alias = async {
        let proxy = zbus::Proxy::new(connection, BLUEZ, path.as_str(), "org.bluez.Device1").await?;
        proxy.get_property::<String>("Alias").await
    };
    alias.await.ok().filter(|alias| !alias.trim().is_empty()).unwrap_or_else(|| address(path.as_str()))
}

struct Agent {
    sink: Arc<dyn Fn(Event) + Send + Sync>,
}

impl Agent {
    async fn ask(&self, request: Request) -> Result<Answer, AgentError> {
        let (sender, receiver) = oneshot::channel();
        (self.sink)(Event::Ask(request, sender));
        match receiver.await {
            Some(Answer::Decline) => Err(AgentError::Rejected("declined by the user".into())),
            None => Err(AgentError::Canceled("the request went away".into())),
            Some(answer) => Ok(answer),
        }
    }

    async fn allow(&self, request: Request) -> Result<(), AgentError> {
        match self.ask(request).await? {
            Answer::Accept => Ok(()),
            _ => Err(AgentError::Rejected("not allowed".into())),
        }
    }
}

// Signatures are fixed by the org.bluez.Agent1 D-Bus interface.
#[allow(clippy::unused_self)]
#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {
        tracing::info!("bluez released the pairing agent");
    }

    async fn request_pin_code(
        &self, device: OwnedObjectPath, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<String, AgentError> {
        let device = device_name(connection, &device).await;
        match self.ask(Request::PinCode { device }).await? {
            Answer::Text(text) => parse_pin(&text).ok_or_else(|| AgentError::Rejected("not a PIN code".into())),
            _ => Err(AgentError::Rejected("no PIN code".into())),
        }
    }

    async fn display_pin_code(
        &self, device: OwnedObjectPath, pincode: String, #[zbus(connection)] connection: &zbus::Connection,
    ) {
        let device = device_name(connection, &device).await;
        (self.sink)(Event::Show(Request::ShowPinCode { device, pin: pincode }));
    }

    async fn request_passkey(
        &self, device: OwnedObjectPath, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<u32, AgentError> {
        let device = device_name(connection, &device).await;
        match self.ask(Request::Passkey { device }).await? {
            Answer::Text(text) => parse_passkey(&text).ok_or_else(|| AgentError::Rejected("not a passkey".into())),
            _ => Err(AgentError::Rejected("no passkey".into())),
        }
    }

    async fn display_passkey(
        &self, device: OwnedObjectPath, passkey: u32, entered: u16, #[zbus(connection)] connection: &zbus::Connection,
    ) {
        if entered == 0 {
            let device = device_name(connection, &device).await;
            (self.sink)(Event::Show(Request::ShowPasskey { device, passkey }));
        }
    }

    async fn request_confirmation(
        &self, device: OwnedObjectPath, passkey: u32, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<(), AgentError> {
        let device = device_name(connection, &device).await;
        self.allow(Request::Confirm { device, passkey }).await
    }

    async fn request_authorization(
        &self, device: OwnedObjectPath, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<(), AgentError> {
        let device = device_name(connection, &device).await;
        self.allow(Request::Authorize { device }).await
    }

    async fn authorize_service(
        &self, device: OwnedObjectPath, uuid: String, #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<(), AgentError> {
        let device = device_name(connection, &device).await;
        self.allow(Request::AuthorizeService { device, service: uuid }).await
    }

    fn cancel(&self) {
        (self.sink)(Event::Cancel);
    }
}

fn serve(builder: Builder<'_>, sink: Arc<dyn Fn(Event) + Send + Sync>) -> zbus::Result<Connection> {
    builder.serve_at(AGENT_PATH, Agent { sink })?.build()
}

fn register(connection: &Connection) -> zbus::Result<()> {
    let manager = Proxy::new(connection, BLUEZ, "/org/bluez", "org.bluez.AgentManager1")?;
    let path = ObjectPath::from_static_str_unchecked(AGENT_PATH);
    manager.call_method("RegisterAgent", &(&path, CAPABILITY))?;
    manager.call_method("RequestDefaultAgent", &(&path,))?;
    tracing::info!("registered the Bluetooth pairing agent");
    Ok(())
}

pub fn start(sink: impl Fn(Event) + Send + Sync + 'static) -> anyhow::Result<()> {
    let connection = serve(Builder::system()?, Arc::new(sink))?;
    crate::agent_prompt::keep_registered(connection, BLUEZ, "glide-bt-agent", register)
}

impl crate::GlideState {
    pub fn handle_bluetooth_agent(&mut self, event: Event) {
        match event {
            Event::Ask(request, answer) => {
                tracing::info!(?request, "Bluetooth pairing needs an answer");
                self.show_prompt(OWNER, &request.prompt(), answer);
            }
            Event::Show(request) => {
                let mut notice = request.prompt().notification(SHOW_NOTICE_ID);
                notice.actions.clear();
                notice.resident = false;
                self.notifications.push(&mut self.dock.icons, notice, true);
                self.refresh_topbar();
                self.queue_redraw();
            }
            Event::Cancel => {
                self.cancel_prompts(OWNER);
                self.notifications.dismiss(SHOW_NOTICE_ID);
            }
        }
    }
}

pub const SHOW_NOTICE_ID: u32 = u32::MAX - 5;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Mutex, PoisonError, mpsc},
        time::Duration,
    };

    #[test]
    fn passkeys_are_six_digits_and_pins_one_to_sixteen_characters() {
        assert_eq!(parse_passkey(" 012345 "), Some(12345));
        assert_eq!(parse_passkey("999999"), Some(999_999));
        for bad in ["", "1234567", "12a456", "-1", "+12"] {
            assert_eq!(parse_passkey(bad), None, "{bad}");
        }
        assert_eq!(parse_pin(" 0000 ").as_deref(), Some("0000"));
        assert_eq!(parse_pin(""), None);
        assert_eq!(parse_pin(&"1".repeat(17)), None);
        assert_eq!(code(42), "000042", "passkeys are always shown with six digits");
    }

    #[test]
    fn an_unnamed_device_is_shown_by_its_address() {
        assert_eq!(address("/org/bluez/hci0/dev_AA_BB_CC_00_11_22"), "AA:BB:CC:00:11:22");
    }

    #[test]
    fn each_request_becomes_a_prompt_with_the_right_controls() {
        let device = String::from("K380");
        let confirm = Request::Confirm { device: device.clone(), passkey: 7 }.prompt();
        assert!(confirm.summary.contains("K380") && confirm.body.contains("000007"));
        assert!(confirm.accept.is_some() && confirm.reply.is_none());
        let pin = Request::PinCode { device: device.clone() }.prompt();
        assert!(pin.accept.is_none() && pin.reply.is_some());
        let show = Request::ShowPasskey { device, passkey: 123_456 }.prompt();
        assert!(show.body.contains("123456") && show.accept.is_none() && show.reply.is_none());
        assert!(!show.body.contains('{'), "every placeholder is filled");
    }

    #[test]
    fn bluez_calls_wait_for_the_users_answer_and_get_bluez_errors_on_refusal() {
        let Some(daemon) = crate::test_bus::private_bus() else { return };
        let (sender, events) = mpsc::channel();
        let sender = Mutex::new(sender);
        let sink: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| {
            let _ = sender.lock().unwrap_or_else(PoisonError::into_inner).send(event);
        });
        let agent = serve(daemon.builder(), sink).expect("served");
        let destination = agent.unique_name().expect("a unique name").to_string();
        let bluez = daemon.client();
        let device = ObjectPath::from_static_str_unchecked("/org/bluez/hci0/dev_AA_BB_CC_00_11_22");
        let call = |method: &'static str, body: (ObjectPath<'static>, u32)| {
            let bluez = bluez.clone();
            let destination = destination.clone();
            std::thread::spawn(move || {
                bluez.call_method(Some(destination.as_str()), AGENT_PATH, Some("org.bluez.Agent1"), method, &body)
            })
        };
        let next = || events.recv_timeout(Duration::from_secs(5)).expect("the agent asks the shell");

        let pending = call("RequestConfirmation", (device.clone(), 42));
        let Event::Ask(Request::Confirm { device: name, passkey: 42 }, answer) = next() else {
            panic!("expected a confirmation prompt");
        };
        assert_eq!(name, "AA:BB:CC:00:11:22", "no bluetoothd here, so the address stands in");
        answer.send(Answer::Accept);
        assert!(pending.join().expect("joined").is_ok());

        let pending = call("RequestConfirmation", (device.clone(), 7));
        let Event::Ask(_, answer) = next() else { panic!("expected a prompt") };
        answer.send(Answer::Decline);
        let err = pending.join().expect("joined").expect_err("declined");
        assert!(err.to_string().contains("org.bluez.Error.Rejected"), "{err}");

        let pending = call("RequestConfirmation", (device, 9));
        let Event::Ask(_, answer) = next() else { panic!("expected a prompt") };
        let canceller = daemon.client();
        canceller
            .call_method(Some(destination.as_str()), AGENT_PATH, Some("org.bluez.Agent1"), "Cancel", &())
            .expect("Cancel is served while another call still waits for the user");
        assert!(matches!(next(), Event::Cancel), "the shell hears the cancellation at once");
        drop(answer);
        let err = pending.join().expect("joined").expect_err("cancelled");
        assert!(err.to_string().contains("org.bluez.Error.Canceled"), "{err}");
    }
}
