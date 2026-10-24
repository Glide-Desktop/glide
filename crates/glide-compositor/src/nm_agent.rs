use std::{collections::HashMap, sync::Arc};

use zbus::{
    blocking::{Connection, Proxy, connection::Builder},
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use crate::{
    agent_prompt::{Answer, Prompt, fill},
    i18n::{Key, t},
    oneshot,
};

pub const OWNER: &str = "network";
const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const IDENTIFIER: &str = "org.glide.SecretAgent";
const NETWORK_MANAGER: &str = "org.freedesktop.NetworkManager";
const ALLOW_INTERACTION: u32 = 0x1;
const REQUEST_NEW: u32 = 0x2;

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    WiFi,
    Enterprise,
    Vpn,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub name: String,
    pub kind: Kind,
    pub retry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    setting: String,
    key: String,
    kind: Kind,
}

#[derive(Debug)]
pub enum Event {
    Ask(Request, oneshot::Sender<Answer>),
    Cancel,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent")]
enum SecretError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NoSecrets(String),
    UserCanceled(String),
    AgentCanceled(String),
}

fn text(settings: &Settings, setting: &str, key: &str) -> Option<String> {
    settings.get(setting)?.get(key).and_then(|value| String::try_from(value.clone()).ok())
}

fn list(settings: &Settings, setting: &str, key: &str) -> Vec<String> {
    settings
        .get(setting)
        .and_then(|map| map.get(key))
        .and_then(|value| Vec::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn wanted(setting: &str, connection: &Settings, hints: &[String]) -> Option<Wanted> {
    let hinted = hints.iter().find(|hint| !hint.is_empty() && !hint.contains(':')).cloned();
    let (key, kind) = match setting {
        "802-11-wireless-security" => {
            let key = match text(connection, setting, "key-mgmt").as_deref() {
                Some("none") => "wep-key0",
                Some("ieee8021x") if text(connection, setting, "auth-alg").as_deref() == Some("leap") => {
                    "leap-password"
                }
                Some("owe" | "wpa-eap" | "wpa-eap-suite-b-192" | "ieee8021x") => return None,
                _ => "psk",
            };
            (hinted.unwrap_or_else(|| key.into()), Kind::WiFi)
        }
        "802-1x" => {
            let eap = list(connection, setting, "eap");
            let key = if !eap.is_empty() && eap.iter().all(|method| method == "tls") {
                "private-key-password"
            } else {
                "password"
            };
            (hinted.unwrap_or_else(|| key.into()), Kind::Enterprise)
        }
        "vpn" => (hinted.unwrap_or_else(|| "password".into()), Kind::Vpn),
        "pppoe" | "gsm" | "cdma" | "adsl" => (hinted.unwrap_or_else(|| "password".into()), Kind::Other),
        _ => return None,
    };
    Some(Wanted { setting: setting.into(), key, kind })
}

fn secrets(wanted: &Wanted, secret: String) -> zbus::Result<Settings> {
    let (key, value) = if wanted.kind == Kind::Vpn {
        ("secrets".to_string(), Value::from(HashMap::from([(wanted.key.clone(), secret)])))
    } else {
        (wanted.key.clone(), Value::from(secret))
    };
    let value = OwnedValue::try_from(value).map_err(zbus::Error::Variant)?;
    Ok(HashMap::from([(wanted.setting.clone(), HashMap::from([(key, value)]))]))
}

impl Request {
    #[must_use]
    pub fn prompt(&self) -> Prompt {
        let body = match (self.retry, self.kind) {
            (true, _) => Key::NetRetryBody,
            (false, Kind::WiFi) => Key::NetWifiBody,
            (false, Kind::Enterprise) => Key::NetEnterpriseBody,
            (false, Kind::Vpn) => Key::NetVpnBody,
            (false, Kind::Other) => Key::NetOtherBody,
        };
        Prompt {
            icon: if self.kind == Kind::Vpn { "network-vpn" } else { "network-wireless" }.into(),
            summary: fill(Key::NetPasswordSummary, &self.name, ""),
            body: fill(body, &self.name, ""),
            accept: None,
            reply: Some((t(Key::NetEnterPassword).to_string(), t(Key::NetPasswordPlaceholder).to_string())),
            secret: true,
        }
    }
}

struct Agent {
    sink: Arc<dyn Fn(Event) + Send + Sync>,
}

// Signatures are fixed by the org.freedesktop.NetworkManager.SecretAgent D-Bus interface.
#[allow(clippy::needless_pass_by_value, clippy::unused_self)]
#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl Agent {
    async fn get_secrets(
        &self, connection: Settings, connection_path: OwnedObjectPath, setting_name: String, hints: Vec<String>,
        flags: u32,
    ) -> Result<Settings, SecretError> {
        let Some(wanted) = wanted(&setting_name, &connection, &hints) else {
            return Err(SecretError::NoSecrets(format!("no secret this agent can ask for in {setting_name}")));
        };
        if flags & ALLOW_INTERACTION == 0 {
            return Err(SecretError::NoSecrets("nothing is stored here and asking is not allowed".into()));
        }
        let name = text(&connection, "connection", "id").unwrap_or_else(|| connection_path.to_string());
        let request = Request { name, kind: wanted.kind, retry: flags & REQUEST_NEW != 0 };
        let (sender, receiver) = oneshot::channel();
        (self.sink)(Event::Ask(request, sender));
        match receiver.await {
            Some(Answer::Text(secret)) if !secret.is_empty() => secrets(&wanted, secret).map_err(SecretError::ZBus),
            Some(_) => Err(SecretError::UserCanceled("the user cancelled".into())),
            None => Err(SecretError::AgentCanceled("the request went away".into())),
        }
    }

    fn cancel_get_secrets(&self, connection_path: OwnedObjectPath, setting_name: String) {
        tracing::info!(path = %connection_path, setting_name, "NetworkManager withdrew a secret request");
        (self.sink)(Event::Cancel);
    }

    fn save_secrets(&self, connection: Settings, connection_path: OwnedObjectPath) {
        let name = text(&connection, "connection", "id");
        tracing::debug!(path = %connection_path, ?name, "not keeping agent-owned secrets; they will be asked again");
    }

    fn delete_secrets(&self, connection: Settings, connection_path: OwnedObjectPath) {
        let name = text(&connection, "connection", "id");
        tracing::debug!(path = %connection_path, ?name, "no agent-owned secrets to delete");
    }
}

fn serve(builder: Builder<'_>, sink: Arc<dyn Fn(Event) + Send + Sync>) -> zbus::Result<Connection> {
    builder.serve_at(AGENT_PATH, Agent { sink })?.build()
}

fn register(connection: &Connection) -> zbus::Result<()> {
    let manager = Proxy::new(
        connection,
        NETWORK_MANAGER,
        "/org/freedesktop/NetworkManager/AgentManager",
        "org.freedesktop.NetworkManager.AgentManager",
    )?;
    manager.call_method("Register", &(IDENTIFIER,))?;
    tracing::info!("registered the NetworkManager secret agent");
    Ok(())
}

pub fn start(sink: impl Fn(Event) + Send + Sync + 'static) -> anyhow::Result<()> {
    let connection = serve(Builder::system()?, Arc::new(sink))?;
    crate::agent_prompt::keep_registered(connection, NETWORK_MANAGER, "glide-nm-agent", register)
}

impl crate::GlideState {
    pub fn handle_network_agent(&mut self, event: Event) {
        match event {
            Event::Ask(request, answer) => {
                tracing::info!(name = request.name, kind = ?request.kind, retry = request.retry, "a network needs a secret");
                self.show_prompt(OWNER, &request.prompt(), answer);
            }
            Event::Cancel => self.cancel_prompts(OWNER),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Mutex, PoisonError, mpsc},
        time::Duration,
    };

    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("plain values convert")
    }

    fn connection(setting: &str, pairs: &[(&str, Value<'static>)]) -> Settings {
        let mut settings: Settings = HashMap::new();
        settings.insert("connection".into(), HashMap::from([("id".to_string(), owned(Value::from("Home")))]));
        settings.insert(
            setting.into(),
            pairs.iter().map(|(key, value)| ((*key).to_string(), owned(value.clone()))).collect(),
        );
        settings
    }

    #[test]
    fn the_secret_follows_the_kind_of_network() {
        let psk = connection("802-11-wireless-security", &[("key-mgmt", Value::from("wpa-psk"))]);
        assert_eq!(wanted("802-11-wireless-security", &psk, &[]).map(|w| w.key), Some("psk".into()));
        let wep = connection("802-11-wireless-security", &[("key-mgmt", Value::from("none"))]);
        assert_eq!(wanted("802-11-wireless-security", &wep, &[]).map(|w| w.key), Some("wep-key0".into()));
        let owe = connection("802-11-wireless-security", &[("key-mgmt", Value::from("owe"))]);
        assert_eq!(wanted("802-11-wireless-security", &owe, &[]), None, "an open network has nothing to ask");
        let peap = connection("802-1x", &[("eap", Value::from(vec!["peap"]))]);
        let w = wanted("802-1x", &peap, &[]).expect("enterprise");
        assert_eq!((w.key.as_str(), w.kind), ("password", Kind::Enterprise));
        let tls = connection("802-1x", &[("eap", Value::from(vec!["tls"]))]);
        assert_eq!(wanted("802-1x", &tls, &[]).map(|w| w.key), Some("private-key-password".into()));
        let vpn = connection("vpn", &[]);
        assert_eq!(
            wanted("vpn", &vpn, &["x-vpn-message:Hello".into(), "cert-pass".into()]).map(|w| w.key),
            Some("cert-pass".into())
        );
        assert_eq!(wanted("802-11-wireless", &psk, &[]), None);
    }

    #[test]
    fn secrets_go_back_in_the_shape_network_manager_expects() {
        let wifi = Wanted { setting: "802-11-wireless-security".into(), key: "psk".into(), kind: Kind::WiFi };
        let reply = secrets(&wifi, "hunter22".into()).expect("built");
        let psk = reply["802-11-wireless-security"]["psk"].clone();
        assert_eq!(String::try_from(psk).expect("a string"), "hunter22");
        let vpn = Wanted { setting: "vpn".into(), key: "password".into(), kind: Kind::Vpn };
        let reply = secrets(&vpn, "s3cret".into()).expect("built");
        let inner: HashMap<String, String> = HashMap::try_from(reply["vpn"]["secrets"].clone()).expect("a{ss}");
        assert_eq!(inner["password"], "s3cret", "VPN secrets travel as a string dictionary");
    }

    #[test]
    fn the_prompt_masks_the_password_and_says_when_it_was_wrong() {
        let first = Request { name: "Home".into(), kind: Kind::WiFi, retry: false }.prompt();
        let again = Request { name: "Home".into(), kind: Kind::WiFi, retry: true }.prompt();
        assert!(first.secret && first.reply.is_some() && first.accept.is_none());
        assert!(first.summary.contains("Home"));
        assert_ne!(first.body, again.body);
    }

    #[test]
    fn get_secrets_waits_for_the_user_and_refuses_without_interaction() {
        let Some(daemon) = crate::test_bus::private_bus() else { return };
        let (sender, events) = mpsc::channel();
        let sender = Mutex::new(sender);
        let sink: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| {
            let _ = sender.lock().unwrap_or_else(PoisonError::into_inner).send(event);
        });
        let agent = serve(daemon.builder(), sink).expect("served");
        let destination = agent.unique_name().expect("a unique name").to_string();
        let network_manager = daemon.client();
        let settings = connection("802-11-wireless-security", &[("key-mgmt", Value::from("wpa-psk"))]);
        let call = |flags: u32| {
            let (client, destination, settings) = (network_manager.clone(), destination.clone(), settings.clone());
            std::thread::spawn(move || {
                let path = OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/7").expect("path");
                let body = (settings, path, "802-11-wireless-security", Vec::<String>::new(), flags);
                client
                    .call_method(
                        Some(destination.as_str()),
                        AGENT_PATH,
                        Some("org.freedesktop.NetworkManager.SecretAgent"),
                        "GetSecrets",
                        &body,
                    )
                    .and_then(|reply| reply.body().deserialize::<Settings>())
            })
        };

        let err = call(0).join().expect("joined").expect_err("no interaction");
        assert!(err.to_string().contains("SecretAgent.NoSecrets"), "{err}");
        assert!(events.try_recv().is_err(), "nothing was shown");

        let pending = call(ALLOW_INTERACTION | REQUEST_NEW);
        let Ok(Event::Ask(request, answer)) = events.recv_timeout(Duration::from_secs(5)) else {
            panic!("expected a prompt");
        };
        assert_eq!(request, Request { name: "Home".into(), kind: Kind::WiFi, retry: true });
        answer.send(Answer::Text("hunter22".into()));
        let reply = pending.join().expect("joined").expect("secrets");
        assert_eq!(String::try_from(reply["802-11-wireless-security"]["psk"].clone()).expect("str"), "hunter22");

        let pending = call(ALLOW_INTERACTION);
        let Ok(Event::Ask(_, answer)) = events.recv_timeout(Duration::from_secs(5)) else {
            panic!("expected a prompt");
        };
        answer.send(Answer::Decline);
        let err = pending.join().expect("joined").expect_err("cancelled");
        assert!(err.to_string().contains("SecretAgent.UserCanceled"), "{err}");
    }
}
