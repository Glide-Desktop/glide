use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Structure, StructureBuilder, Value};

pub const PATH: &str = "/org/glide/Quicklist";
const ENTRY_PATH: &str = "/com/canonical/unity/launcherentry/glide";
const ENTRY_INTERFACE: &str = "com.canonical.Unity.LauncherEntry";

type Sink = Box<dyn Fn(usize) + Send + Sync>;
type Layout = (i32, HashMap<String, Value<'static>>, Vec<Value<'static>>);

pub struct QuickMenu {
    labels: Vec<String>,
    sink: Sink,
}

impl QuickMenu {
    #[must_use]
    pub fn new(labels: Vec<String>, sink: impl Fn(usize) + Send + Sync + 'static) -> Self {
        Self { labels, sink: Box::new(sink) }
    }

    fn properties(&self, id: i32) -> HashMap<String, Value<'static>> {
        let mut props = HashMap::new();
        if id == 0 {
            props.insert("children-display".to_string(), Value::from("submenu"));
            return props;
        }
        if let Some(label) = usize::try_from(id - 1).ok().and_then(|index| self.labels.get(index)) {
            props.insert("label".to_string(), Value::from(label.clone()));
            props.insert("enabled".to_string(), Value::from(true));
            props.insert("visible".to_string(), Value::from(true));
        }
        props
    }

    fn node(&self, id: i32) -> Option<Structure<'static>> {
        StructureBuilder::new()
            .add_field(id)
            .add_field(self.properties(id))
            .add_field(Vec::<Value<'static>>::new())
            .build()
            .ok()
    }
}

// Signatures are fixed by com.canonical.dbusmenu.
#[allow(clippy::needless_pass_by_value, clippy::unused_self)]
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl QuickMenu {
    fn get_layout(&self, parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> (u32, Layout) {
        drop((recursion_depth, property_names));
        if parent_id != 0 {
            return (1, (parent_id, self.properties(parent_id), Vec::new()));
        }
        let children = (1..=i32::try_from(self.labels.len()).unwrap_or(0))
            .filter_map(|id| self.node(id))
            .map(Value::from)
            .collect();
        (1, (0, self.properties(0), children))
    }

    fn get_group_properties(
        &self, ids: Vec<i32>, property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, Value<'static>>)> {
        drop(property_names);
        ids.into_iter().map(|id| (id, self.properties(id))).collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        self.properties(id)
            .remove(&name)
            .and_then(|value| value.try_to_owned().ok())
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name}")))
    }

    fn event(&self, id: i32, event_id: String, data: OwnedValue, timestamp: u32) {
        drop((data, timestamp));
        if event_id == "clicked"
            && let Ok(index) = usize::try_from(id - 1)
            && index < self.labels.len()
        {
            (self.sink)(index);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event_id, data, timestamp) in events {
            self.event(id, event_id, data, timestamp);
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        drop(ids);
        (Vec::new(), Vec::new())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn status(&self) -> String {
        String::from("normal")
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        String::from("ltr")
    }
}

pub fn announce(connection: &zbus::blocking::Connection, desktop_id: &str) -> zbus::Result<()> {
    let path = ObjectPath::try_from(PATH)?;
    let props = HashMap::from([("quicklist", Value::from(path))]);
    connection.emit_signal(
        None::<zbus::names::BusName<'_>>,
        ENTRY_PATH,
        ENTRY_INTERFACE,
        "Update",
        &(format!("application://{desktop_id}.desktop"), props),
    )
}

pub fn keep_announced(connection: &zbus::blocking::Connection, desktop_id: &str) {
    let _ = announce(connection, desktop_id);
    let rule = format!(
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{}'",
        crate::BUS_NAME
    );
    let Ok(rule) = zbus::MatchRule::try_from(rule.as_str()) else {
        return;
    };
    let watched = zbus::blocking::fdo::DBusProxy::new(connection).map(|bus| bus.add_match_rule(rule).is_ok());
    if !matches!(watched, Ok(true)) {
        return;
    }
    let (connection, desktop_id) = (connection.clone(), desktop_id.to_string());
    let _ = std::thread::Builder::new().name("glide-quicklist-announce".into()).spawn(move || {
        for message in zbus::blocking::MessageIterator::from(&connection).map_while(Result::ok) {
            let header = message.header();
            if header.member().is_none_or(|member| member.as_str() != "NameOwnerChanged") {
                continue;
            }
            let Ok((name, _, owner)) = message.body().deserialize::<(String, String, String)>() else {
                continue;
            };
            if name == crate::BUS_NAME && !owner.is_empty() {
                let _ = announce(&connection, &desktop_id);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_lists_one_child_per_label_and_clicks_reach_the_sink() {
        let clicked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = std::sync::Arc::clone(&clicked);
        let menu = QuickMenu::new(vec!["New Tab".into(), "New Window".into()], move |index| {
            seen.lock().expect("lock").push(index);
        });
        let (_, (root, props, children)) = menu.get_layout(0, -1, Vec::new());
        assert_eq!(root, 0);
        assert!(props.contains_key("children-display"));
        assert_eq!(children.len(), 2);
        menu.event(2, "clicked".into(), Value::from(0_i32).try_to_owned().expect("owned"), 0);
        menu.event(9, "clicked".into(), Value::from(0_i32).try_to_owned().expect("owned"), 0);
        menu.event(1, "hovered".into(), Value::from(0_i32).try_to_owned().expect("owned"), 0);
        assert_eq!(*clicked.lock().expect("lock"), [1]);
    }
}
