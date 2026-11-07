use std::collections::HashMap;
use std::time::Duration;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use glide_theme::{Accent, Appearance};
use zbus::zvariant::OwnedValue;

pub mod activation;
pub mod chooser;
pub mod drop_target;
pub mod i18n;
pub mod integration;
pub mod menubar;
pub mod quicklist;
pub mod search_provider;

pub const BUS_NAME: &str = "org.glide.Shell";
pub const PATH: &str = "/org/glide/Shell1";
pub const INTERFACE: &str = "org.glide.Shell1";

const RETRY: Duration = Duration::from_secs(3);
const MAX_RETRY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub appearance: Appearance,
    pub accent: Accent,
    pub language: Option<String>,
    pub animation_scale: f32,
    pub reduced_motion: bool,
    pub font_family: Option<String>,
    pub icon_theme: Option<String>,
    pub locked: bool,
    pub active_workspace: u32,
}

fn get<T: TryFrom<OwnedValue>>(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
    T::try_from(properties.get(key)?.try_clone().ok()?).ok()
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
}

impl Look {
    #[must_use]
    pub fn from_properties(properties: &HashMap<String, OwnedValue>) -> Option<Self> {
        Some(Self {
            appearance: Appearance::from_key(&get::<String>(properties, "Appearance")?)?,
            accent: Accent::from_key(&get::<String>(properties, "Accent")?).unwrap_or_default(),
            language: non_empty(get(properties, "Language")),
            animation_scale: get::<f64>(properties, "AnimationScale").unwrap_or(1.0) as f32,
            reduced_motion: get(properties, "ReducedMotion").unwrap_or(false),
            font_family: non_empty(get(properties, "FontFamily")),
            icon_theme: non_empty(get(properties, "IconTheme")),
            locked: get(properties, "Locked").unwrap_or(false),
            active_workspace: get(properties, "ActiveWorkspace").unwrap_or(0),
        })
    }
}

#[must_use]
pub fn look_changes() -> UnboundedReceiver<Look> {
    let (sender, receiver) = unbounded();
    let spawned = std::thread::Builder::new().name("glide-shell-look".into()).spawn(move || {
        let mut backoff = RETRY;
        while !sender.is_closed() {
            let started = std::time::Instant::now();
            match zbus::blocking::Connection::session() {
                Ok(connection) => match follow(&connection, &sender) {
                    Ok(()) => return,
                    Err(err) => tracing::debug!("stopped following the Glide shell look: {err}"),
                },
                Err(err) => tracing::debug!("no session bus for the Glide shell look: {err}"),
            }
            if started.elapsed() >= MAX_RETRY {
                backoff = RETRY;
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(MAX_RETRY);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("cannot follow the Glide shell look: {err}");
    }
    receiver
}

const DBUS: &str = "org.freedesktop.DBus";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

pub(crate) fn owner_rule(name: &str) -> zbus::Result<zbus::MatchRule<'_>> {
    Ok(zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(DBUS)?
        .interface(DBUS)?
        .member("NameOwnerChanged")?
        .arg(0, name)?
        .build())
}

pub(crate) fn owner_change(message: &zbus::Message) -> Option<(String, bool)> {
    let header = message.header();
    let from_bus = header.sender().is_some_and(|sender| sender.as_str() == DBUS);
    let member = header.member().is_some_and(|member| member.as_str() == "NameOwnerChanged");
    if header.message_type() != zbus::message::Type::Signal || !from_bus || !member {
        return None;
    }
    let (name, _, owner) = message.body().deserialize::<(String, String, String)>().ok()?;
    Some((name, !owner.is_empty()))
}

struct Follower<'a> {
    properties: zbus::blocking::fdo::PropertiesProxy<'a>,
    interface: zbus::names::InterfaceName<'a>,
    sender: &'a UnboundedSender<Look>,
    last: Option<Look>,
}

impl Follower<'_> {
    fn refresh(&mut self) -> bool {
        let all = match self.properties.get_all(self.interface.clone()) {
            Ok(all) => all,
            Err(err) => {
                tracing::debug!("cannot read the Glide shell look: {err}");
                return true;
            }
        };
        let look = Look::from_properties(&all);
        if look.is_none() || look == self.last {
            return true;
        }
        self.last.clone_from(&look);
        look.is_none_or(|look| self.sender.unbounded_send(look).is_ok())
    }
}

fn changed_properties(message: &zbus::Message) -> bool {
    let header = message.header();
    header.message_type() == zbus::message::Type::Signal
        && header.path().is_some_and(|path| path.as_str() == PATH)
        && header.interface().is_some_and(|interface| interface.as_str() == PROPERTIES)
        && header.member().is_some_and(|member| member.as_str() == "PropertiesChanged")
        && message
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            .is_ok_and(|(interface, _, _)| interface == INTERFACE)
}

fn follow(connection: &zbus::blocking::Connection, sender: &UnboundedSender<Look>) -> zbus::Result<()> {
    let messages = zbus::blocking::MessageIterator::from(connection);
    let dbus = zbus::blocking::fdo::DBusProxy::new(connection)?;
    dbus.add_match_rule(owner_rule(BUS_NAME)?)?;
    dbus.add_match_rule(
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(BUS_NAME)?
            .path(PATH)?
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?
            .arg(0, INTERFACE)?
            .build(),
    )?;
    let mut follower = Follower {
        properties: zbus::blocking::fdo::PropertiesProxy::builder(connection)
            .destination(BUS_NAME)?
            .path(PATH)?
            .build()?,
        interface: zbus::names::InterfaceName::try_from(INTERFACE)?,
        sender,
        last: None,
    };
    if dbus.name_has_owner(zbus::names::BusName::try_from(BUS_NAME)?)? && !follower.refresh() {
        return Ok(());
    }
    for message in messages {
        let message = message?;
        let alive = match owner_change(&message) {
            Some((name, true)) if name == BUS_NAME => follower.refresh(),
            Some((name, false)) if name == BUS_NAME => {
                follower.last = None;
                true
            }
            _ => !changed_properties(&message) || follower.refresh(),
        };
        if !alive || sender.is_closed() {
            return Ok(());
        }
    }
    Err(zbus::Error::Failure("the session bus connection closed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("plain values are owned")
    }

    #[test]
    fn a_full_property_set_becomes_a_look() {
        let properties = HashMap::from([
            ("Appearance".to_string(), owned(Value::from("light"))),
            ("Accent".to_string(), owned(Value::from("teal"))),
            ("Language".to_string(), owned(Value::from("uk"))),
            ("AnimationScale".to_string(), owned(Value::from(0.5_f64))),
            ("ReducedMotion".to_string(), owned(Value::from(false))),
            ("FontFamily".to_string(), owned(Value::from(""))),
            ("IconTheme".to_string(), owned(Value::from("Papirus"))),
            ("Locked".to_string(), owned(Value::from(true))),
            ("ActiveWorkspace".to_string(), owned(Value::from(2_u32))),
        ]);
        let look = Look::from_properties(&properties).expect("look");
        assert_eq!(look.appearance, Appearance::Light);
        assert_eq!(look.accent, Accent::Teal);
        assert_eq!(look.language.as_deref(), Some("uk"));
        assert!((look.animation_scale - 0.5).abs() < f32::EPSILON);
        assert_eq!(look.font_family, None, "an empty family means the default");
        assert_eq!(look.icon_theme.as_deref(), Some("Papirus"));
        assert!(look.locked);
        assert_eq!(look.active_workspace, 2);
    }

    #[test]
    fn a_property_set_without_an_appearance_is_not_a_look() {
        assert_eq!(Look::from_properties(&HashMap::new()), None);
    }
}
