use std::collections::HashMap;
use std::time::Duration;

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use glide_theme::{Accent, Appearance};
use zbus::zvariant::OwnedValue;

pub mod activation;
pub mod chooser;
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
        let mut connection = None;
        while !sender.is_closed() {
            if connection.is_none() {
                connection = zbus::blocking::Connection::session()
                    .inspect_err(|err| tracing::debug!("no session bus for the Glide shell look: {err}"))
                    .ok();
            }
            if let Some(bus) = connection.as_ref() {
                match follow(bus, &sender) {
                    Ok(()) => backoff = RETRY,
                    Err(err) => tracing::debug!("the Glide shell service is unavailable: {err}"),
                }
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

fn follow(connection: &zbus::blocking::Connection, sender: &UnboundedSender<Look>) -> zbus::Result<()> {
    let properties =
        zbus::blocking::fdo::PropertiesProxy::builder(connection).destination(BUS_NAME)?.path(PATH)?.build()?;
    let interface = zbus::names::InterfaceName::try_from(INTERFACE)?;
    let changes = properties.receive_properties_changed()?;
    let mut last = None;
    let mut publish = |all: HashMap<String, OwnedValue>| {
        let look = Look::from_properties(&all);
        if look.is_some() && look != last {
            last.clone_from(&look);
            if let Some(look) = look {
                return sender.unbounded_send(look).is_ok();
            }
        }
        true
    };
    if !publish(properties.get_all(interface.clone())?) {
        return Ok(());
    }
    for _ in changes {
        if !publish(properties.get_all(interface.clone())?) {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn owned(value: Value<'_>) -> OwnedValue {
        value.try_to_owned().expect("plain values are owned")
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
