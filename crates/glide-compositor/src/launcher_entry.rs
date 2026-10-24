use std::collections::HashMap;

use glide_appmenu::strip_mnemonic;
use zbus::zvariant::OwnedValue;

pub const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";
const RULE: &str = "type='signal',interface='com.canonical.Unity.LauncherEntry',member='Update'";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quicklist {
    pub bus: String,
    pub path: String,
    pub items: Vec<(i32, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntryUpdate {
    pub count: Option<i64>,
    pub count_visible: Option<bool>,
    pub progress: Option<f64>,
    pub progress_visible: Option<bool>,
    pub urgent: Option<bool>,
    pub quicklist: Option<Quicklist>,
}

const DBUSMENU: &str = "com.canonical.dbusmenu";
const OWNER_RULE: &str =
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'";

fn vanished(message: &zbus::Message) -> Option<String> {
    let header = message.header();
    let relevant = header.interface().is_some_and(|name| name.as_str() == "org.freedesktop.DBus")
        && header.member().is_some_and(|name| name.as_str() == "NameOwnerChanged");
    if !relevant {
        return None;
    }
    let (name, _, owner) = message.body().deserialize::<(String, String, String)>().ok()?;
    (name.starts_with(':') && owner.is_empty()).then_some(name)
}

#[must_use]
pub fn gone(bus: String) -> EntryUpdate {
    EntryUpdate { quicklist: Some(Quicklist { bus, path: String::new(), items: Vec::new() }), ..EntryUpdate::default() }
}

type MenuNode = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[must_use]
pub fn quick_items(children: Vec<OwnedValue>) -> Vec<(i32, String)> {
    children
        .into_iter()
        .filter_map(|child| {
            let (id, props, _): MenuNode = child.try_into().ok()?;
            let get_bool = |key: &str| props.get(key).and_then(|value| bool::try_from(value).ok());
            let get_text = |key: &str| {
                props.get(key).and_then(|value| String::try_from(value.try_clone().ok()?).ok()).unwrap_or_default()
            };
            let hidden = get_bool("visible") == Some(false) || get_bool("enabled") == Some(false);
            let separator = get_text("type") == "separator";
            let label = strip_mnemonic(&get_text("label"));
            (!hidden && !separator && !label.trim().is_empty()).then_some((id, label))
        })
        .collect()
}

fn fetch_quicklist(connection: &zbus::blocking::Connection, bus: &str, path: &str) -> Option<Vec<(i32, String)>> {
    let reply = connection
        .call_method(Some(bus), path, Some(DBUSMENU), "GetLayout", &(0_i32, 1_i32, Vec::<String>::new()))
        .ok()?;
    let (_, root): (u32, MenuNode) = reply.body().deserialize().ok()?;
    Some(quick_items(root.2))
}

fn quicklist_path(props: &HashMap<String, OwnedValue>) -> Option<String> {
    let value = props.get("quicklist")?;
    String::try_from(value.try_clone().ok()?).ok().or_else(|| {
        zbus::zvariant::OwnedObjectPath::try_from(value.try_clone().ok()?).ok().map(|path| path.to_string())
    })
}

pub fn click(bus: String, path: String, id: i32) {
    let _ = std::thread::Builder::new().name("glide-quicklist".into()).spawn(move || {
        if let Ok(connection) = zbus::blocking::Connection::session() {
            let _ = connection.call_method(
                Some(bus.as_str()),
                path.as_str(),
                Some(DBUSMENU),
                "Event",
                &(id, "clicked", zbus::zvariant::Value::from(0_i32), 0_u32),
            );
        }
    });
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EntryState {
    pub count: i64,
    pub count_visible: bool,
    pub progress: f64,
    pub progress_visible: bool,
    pub urgent: bool,
}

impl EntryState {
    pub fn merge(&mut self, update: &EntryUpdate) {
        self.count = update.count.unwrap_or(self.count);
        self.count_visible = update.count_visible.unwrap_or(self.count_visible);
        self.progress = update.progress.unwrap_or(self.progress);
        self.progress_visible = update.progress_visible.unwrap_or(self.progress_visible);
        self.urgent = update.urgent.unwrap_or(self.urgent);
    }

    #[must_use]
    pub fn badge(&self) -> crate::dock::Badge {
        crate::dock::Badge {
            count: (self.count_visible && self.count > 0).then_some(self.count),
            progress: (self.progress_visible && self.progress.is_finite()).then(|| self.progress.clamp(0.0, 1.0)),
        }
    }
}

#[must_use]
pub fn desktop_id(app_uri: &str) -> Option<String> {
    let id = app_uri.strip_prefix("application://").unwrap_or(app_uri);
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    let id = id.rsplit('/').next().unwrap_or(id).trim();
    (!id.is_empty()).then(|| id.to_string())
}

fn integer(value: &OwnedValue) -> Option<i64> {
    i64::try_from(value)
        .ok()
        .or_else(|| i32::try_from(value).ok().map(i64::from))
        .or_else(|| u32::try_from(value).ok().map(i64::from))
        .or_else(|| u64::try_from(value).ok().map(|v| i64::try_from(v).unwrap_or(i64::MAX)))
}

#[must_use]
pub fn parse_update(props: &HashMap<String, OwnedValue>) -> EntryUpdate {
    let boolean = |key: &str| props.get(key).and_then(|v| bool::try_from(v).ok());
    EntryUpdate {
        count: props.get("count").and_then(integer),
        count_visible: boolean("count-visible"),
        progress: props.get("progress").and_then(|v| f64::try_from(v).ok()),
        progress_visible: boolean("progress-visible"),
        urgent: boolean("urgent"),
        quicklist: None,
    }
}

pub fn start(sink: impl Fn(String, EntryUpdate) + Send + 'static) -> anyhow::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let bus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
    bus.add_match_rule(zbus::MatchRule::try_from(RULE)?)?;
    bus.add_match_rule(zbus::MatchRule::try_from(OWNER_RULE)?)?;
    std::thread::Builder::new().name("glide-launcher-entry".into()).spawn(move || {
        for message in zbus::blocking::MessageIterator::from(&connection) {
            let Ok(message) = message else {
                continue;
            };
            if let Some(name) = vanished(&message) {
                sink(String::new(), gone(name));
                continue;
            }
            let header = message.header();
            let relevant = header.message_type() == zbus::message::Type::Signal
                && header.interface().is_some_and(|name| name.as_str() == INTERFACE)
                && header.member().is_some_and(|name| name.as_str() == "Update");
            if !relevant {
                continue;
            }
            let Ok((uri, props)) = message.body().deserialize::<(String, HashMap<String, OwnedValue>)>() else {
                continue;
            };
            let Some(id) = desktop_id(&uri) else {
                continue;
            };
            let mut update = parse_update(&props);
            if let Some(path) = quicklist_path(&props) {
                let bus = header.sender().map(ToString::to_string).unwrap_or_default();
                let items = if path.is_empty() || path == "/" || bus.is_empty() {
                    Vec::new()
                } else {
                    fetch_quicklist(&connection, &bus, &path).unwrap_or_default()
                };
                update.quicklist = Some(Quicklist { bus, path, items });
            }
            sink(id, update);
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glide_quicklist_reads_back_as_labelled_items() {
        let Some(bus) = crate::test_bus::private_bus() else {
            return;
        };
        let menu = glide_app::quicklist::QuickMenu::new(vec!["_New Tab".into(), "New Window".into()], |_| {});
        let server = bus
            .builder()
            .serve_at(glide_app::quicklist::PATH, menu)
            .and_then(zbus::blocking::connection::Builder::build)
            .expect("serve");
        let owner = server.unique_name().expect("unique name").to_string();
        let client = bus.client();
        let items = fetch_quicklist(&client, &owner, glide_app::quicklist::PATH).expect("layout");
        assert_eq!(items, [(1, "New Tab".to_string()), (2, "New Window".to_string())]);
    }
    use zbus::zvariant::Value;

    fn owned(value: &Value<'_>) -> OwnedValue {
        value.try_to_owned().expect("plain values convert")
    }

    #[test]
    fn the_desktop_id_is_taken_from_the_application_uri() {
        assert_eq!(desktop_id("application://firefox.desktop").as_deref(), Some("firefox"));
        assert_eq!(desktop_id("application://org.gnome.Nautilus.desktop").as_deref(), Some("org.gnome.Nautilus"));
        assert_eq!(desktop_id("application://").as_deref(), None);
    }

    #[test]
    fn a_partial_update_only_touches_the_keys_it_carries() {
        let mut props = HashMap::new();
        props.insert("count".to_string(), owned(&Value::I64(7)));
        props.insert("count-visible".to_string(), owned(&Value::Bool(true)));
        let mut state = EntryState::default();
        state.merge(&parse_update(&props));
        assert_eq!(state.badge().count, Some(7));
        assert_eq!(state.badge().progress, None);

        let mut props = HashMap::new();
        props.insert("progress".to_string(), owned(&Value::F64(0.25)));
        props.insert("progress-visible".to_string(), owned(&Value::Bool(true)));
        state.merge(&parse_update(&props));
        assert_eq!(state.badge().count, Some(7), "the count survives");
        assert_eq!(state.badge().progress, Some(0.25));
    }

    #[test]
    fn hidden_or_empty_counts_draw_no_badge_and_progress_is_clamped() {
        let state = EntryState { count: 0, count_visible: true, progress: 1.7, progress_visible: true, urgent: false };
        assert_eq!(state.badge().count, None);
        assert_eq!(state.badge().progress, Some(1.0));
        let hidden = EntryState { count: 3, ..EntryState::default() };
        assert_eq!(hidden.badge(), crate::dock::Badge::default());
    }

    #[test]
    fn counts_sent_as_narrower_integers_are_still_read() {
        let mut props = HashMap::new();
        props.insert("count".to_string(), owned(&Value::I32(3)));
        props.insert("urgent".to_string(), owned(&Value::Bool(true)));
        let update = parse_update(&props);
        assert_eq!(update.count, Some(3));
        assert_eq!(update.urgent, Some(true));
    }
}
