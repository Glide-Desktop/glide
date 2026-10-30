use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::protocol::{wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_plasma::appmenu::client::{org_kde_kwin_appmenu, org_kde_kwin_appmenu_manager};
use zbus::zvariant::{OwnedValue, Structure, StructureBuilder, Value};

pub const PATH: &str = "/MenuBar";
const INTERFACE: &str = "com.canonical.dbusmenu";
const MANAGER: &str = "org_kde_kwin_appmenu_manager";

type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Action { id: u32, label: String, enabled: bool, check: Option<bool>, shortcut: Option<String> },
    Submenu { label: String, enabled: bool, children: Vec<Entry> },
    Separator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handles {
    pub(crate) display: usize,
    pub(crate) surface: usize,
}

impl Handles {
    pub const NONE: Self = Self { display: 0, surface: 0 };

    #[allow(clippy::missing_safety_doc)] // Callers vouch that both are a live wl_display and wl_surface for the whole process
    #[must_use]
    pub const unsafe fn new(display: usize, surface: usize) -> Self {
        Self { display, surface }
    }

    #[must_use]
    pub const fn display(self) -> usize {
        self.display
    }

    #[must_use]
    pub const fn surface(self) -> usize {
        self.surface
    }
}

#[derive(Clone)]
pub struct Publisher {
    tree: Arc<Mutex<Tree>>,
    connection: zbus::blocking::Connection,
}

impl std::fmt::Debug for Publisher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Publisher").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Publisher),
    Shown(bool),
    Activated(u32),
}

#[derive(Debug, Default)]
struct Node {
    properties: HashMap<String, OwnedValue>,
    children: Vec<i32>,
    action: Option<u32>,
}

#[derive(Debug, Default)]
struct Tree {
    menu: Vec<Entry>,
    nodes: Vec<Node>,
    revision: u32,
}

fn owned<'a>(value: impl Into<Value<'a>>) -> Option<OwnedValue> {
    value.into().try_to_owned().ok()
}

#[must_use]
pub fn shortcut_chord(label: &str) -> Option<Vec<String>> {
    let (modifiers, key) = match label.strip_suffix("++") {
        Some(modifiers) => (modifiers, "+"),
        None => label.rsplit_once('+').unwrap_or(("", label)),
    };
    if key.is_empty() {
        return None;
    }
    let mut chord: Vec<String> = modifiers
        .split('+')
        .filter(|modifier| !modifier.is_empty())
        .map(|modifier| if modifier == "Ctrl" { String::from("Control") } else { modifier.to_string() })
        .collect();
    chord.push(key.to_string());
    Some(chord)
}

impl Tree {
    fn rebuild(&mut self, menu: Vec<Entry>) -> bool {
        if menu == self.menu && !self.nodes.is_empty() {
            return false;
        }
        let mut nodes = vec![Node::default()];
        if let Some(display) = owned("submenu") {
            nodes[0].properties.insert("children-display".into(), display);
        }
        let children = Self::add(&mut nodes, &menu);
        nodes[0].children = children;
        self.nodes = nodes;
        self.menu = menu;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn add(nodes: &mut Vec<Node>, entries: &[Entry]) -> Vec<i32> {
        let mut ids = Vec::with_capacity(entries.len());
        for entry in entries {
            let id = nodes.len();
            nodes.push(Node::default());
            let mut properties = HashMap::new();
            let mut insert = |key: &str, value: Option<OwnedValue>| {
                if let Some(value) = value {
                    properties.insert(key.to_string(), value);
                }
            };
            match entry {
                Entry::Separator => insert("type", owned("separator")),
                Entry::Action { id: action, label, enabled, check, shortcut } => {
                    insert("label", owned(label.as_str()));
                    insert("enabled", owned(*enabled));
                    if let Some(on) = check {
                        insert("toggle-type", owned("checkmark"));
                        insert("toggle-state", owned(i32::from(*on)));
                    }
                    if let Some(chord) = shortcut.as_deref().and_then(shortcut_chord) {
                        insert("shortcut", owned(vec![chord]));
                    }
                    nodes[id].action = Some(*action);
                }
                Entry::Submenu { label, enabled, children } => {
                    insert("label", owned(label.as_str()));
                    insert("enabled", owned(*enabled));
                    insert("children-display", owned("submenu"));
                    let children = Self::add(nodes, children);
                    nodes[id].children = children;
                }
            }
            nodes[id].properties = properties;
            ids.push(i32::try_from(id).unwrap_or(i32::MAX));
        }
        ids
    }

    fn node(&self, id: i32) -> Option<&Node> {
        self.nodes.get(usize::try_from(id).ok()?)
    }

    fn properties(&self, id: i32) -> HashMap<String, OwnedValue> {
        self.node(id)
            .map(|node| {
                node.properties.iter().filter_map(|(key, value)| Some((key.clone(), value.try_clone().ok()?))).collect()
            })
            .unwrap_or_default()
    }

    fn layout(&self, id: i32, depth: i32) -> Layout {
        let children: Vec<OwnedValue> = if depth == 0 {
            Vec::new()
        } else {
            self.node(id)
                .map(|node| node.children.clone())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|child| owned(self.structure(child, depth - 1)?))
                .collect()
        };
        (id, self.properties(id), children)
    }

    fn structure(&self, id: i32, depth: i32) -> Option<Structure<'static>> {
        let (id, properties, children) = self.layout(id, depth);
        StructureBuilder::new().add_field(id).add_field(properties).add_field(children).build().ok()
    }
}

struct Server {
    tree: Arc<Mutex<Tree>>,
    events: UnboundedSender<Event>,
}

impl Server {
    fn tree(&self) -> std::sync::MutexGuard<'_, Tree> {
        self.tree.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// Signatures are fixed by com.canonical.dbusmenu.
#[allow(clippy::needless_pass_by_value)]
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Server {
    fn get_layout(&self, parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> (u32, Layout) {
        drop(property_names);
        let tree = self.tree();
        (tree.revision, tree.layout(parent_id, recursion_depth))
    }

    fn get_group_properties(
        &self, ids: Vec<i32>, property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        drop(property_names);
        let tree = self.tree();
        ids.into_iter().map(|id| (id, tree.properties(id))).collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        self.tree()
            .properties(id)
            .remove(&name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no property {name}")))
    }

    fn event(&self, id: i32, event_id: String, data: OwnedValue, timestamp: u32) {
        drop((data, timestamp));
        if event_id != "clicked" {
            return;
        }
        let action = self.tree().node(id).and_then(|node| node.action);
        if let Some(action) = action {
            let _ = self.events.unbounded_send(Event::Activated(action));
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

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Publisher {
    pub fn set(&self, menu: Vec<Entry>) {
        let revision = {
            let mut tree = self.tree.lock().unwrap_or_else(PoisonError::into_inner);
            if !tree.rebuild(menu) {
                return;
            }
            tree.revision
        };
        let _ = self.connection.emit_signal(
            None::<zbus::names::BusName<'_>>,
            PATH,
            INTERFACE,
            "LayoutUpdated",
            &(revision, 0_i32),
        );
    }
}

struct Registration {
    events: UnboundedSender<Event>,
    surface: wl_surface::WlSurface,
    bus: String,
    manager: Option<(u32, org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager)>,
    menu: Option<org_kde_kwin_appmenu::OrgKdeKwinAppmenu>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Registration {
    fn event(
        state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, version }
                if interface == MANAGER && state.manager.is_none() =>
            {
                let manager: org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager =
                    registry.bind(name, version.min(2), qh, ());
                let menu = manager.create(&state.surface, qh, ());
                menu.set_address(state.bus.clone(), PATH.to_string());
                state.manager = Some((name, manager));
                state.menu = Some(menu);
                let _ = state.events.unbounded_send(Event::Shown(true));
            }
            wl_registry::Event::GlobalRemove { name }
                if state.manager.as_ref().is_some_and(|(known, _)| *known == name) =>
            {
                if let Some(menu) = state.menu.take() {
                    menu.release();
                }
                state.manager = None;
                let _ = state.events.unbounded_send(Event::Shown(false));
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(Registration: ignore org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager);
wayland_client::delegate_noop!(Registration: ignore org_kde_kwin_appmenu::OrgKdeKwinAppmenu);

fn register(handles: Handles, bus: String, events: UnboundedSender<Event>) -> Result<(), String> {
    if handles.display == 0 || handles.surface == 0 {
        return Err("no wayland window".into());
    }
    // SAFETY: `display` is the live `wl_display` the toolkit window was created on, and the window lives for
    // the rest of the process, as with the drag-and-drop threads of the Glide apps.
    let backend = unsafe { Backend::from_foreign_display(handles.display as *mut _) };
    let connection = Connection::from_backend(backend);
    // SAFETY: `surface` is the live `wl_surface` of that same window.
    let id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), handles.surface as *mut _) }
        .map_err(|error| error.to_string())?;
    let surface = wl_surface::WlSurface::from_id(&connection, id).map_err(|error| error.to_string())?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = Registration { events, surface, bus, manager: None, menu: None };
    loop {
        queue.blocking_dispatch(&mut state).map_err(|error| error.to_string())?;
        if state.events.is_closed() {
            return Ok(());
        }
    }
}

fn serve(handles: Handles, events: &UnboundedSender<Event>) -> zbus::Result<()> {
    let tree = Arc::new(Mutex::new(Tree::default()));
    let server = Server { tree: Arc::clone(&tree), events: events.clone() };
    let connection = zbus::blocking::connection::Builder::session()?.serve_at(PATH, server)?.build()?;
    let bus = connection.unique_name().map(ToString::to_string).unwrap_or_default();
    let _ = events.unbounded_send(Event::Ready(Publisher { tree, connection }));
    let events = events.clone();
    let _ = std::thread::Builder::new().name("glide-menubar-wayland".into()).spawn(move || {
        if let Err(error) = register(handles, bus, events.clone()) {
            tracing::debug!("menu bar not registered with the shell: {error}");
            let _ = events.unbounded_send(Event::Shown(false));
        }
    });
    Ok(())
}

#[must_use]
pub fn events(handles: &Handles) -> UnboundedReceiver<Event> {
    let (sender, receiver) = unbounded();
    let handles = *handles;
    let _ = std::thread::Builder::new().name("glide-menubar".into()).spawn(move || {
        if let Err(error) = serve(handles, &sender) {
            tracing::debug!("no session bus for the menu bar: {error}");
            let _ = sender.unbounded_send(Event::Shown(false));
        }
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Vec<Entry> {
        vec![
            Entry::Submenu {
                label: "File".into(),
                enabled: true,
                children: vec![
                    Entry::Action {
                        id: 7,
                        label: "Save".into(),
                        enabled: true,
                        check: None,
                        shortcut: Some("Ctrl+S".into()),
                    },
                    Entry::Separator,
                    Entry::Action { id: 9, label: "Wrap".into(), enabled: false, check: Some(true), shortcut: None },
                ],
            },
            Entry::Submenu { label: "Help".into(), enabled: true, children: Vec::new() },
        ]
    }

    #[test]
    fn shortcut_labels_become_dbusmenu_chords() {
        assert_eq!(shortcut_chord("Ctrl+Shift+S"), Some(vec!["Control".into(), "Shift".into(), "S".into()]));
        assert_eq!(shortcut_chord("Ctrl++"), Some(vec!["Control".into(), "+".into()]));
        assert_eq!(shortcut_chord("F9"), Some(vec!["F9".into()]));
        assert_eq!(shortcut_chord(""), None);
    }

    #[test]
    fn the_layout_is_a_tree_that_the_shell_reader_understands() {
        let mut tree = Tree::default();
        assert!(tree.rebuild(menu()));
        assert!(!tree.rebuild(menu()), "an unchanged menu keeps its revision");
        let value = owned(tree.structure(0, -1).expect("root")).expect("owned layout");
        let root = <(i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)>::try_from(value).expect("dbusmenu node");
        let items = glide_appmenu_like(&root);
        assert_eq!(items, vec![("File".to_string(), 3), ("Help".to_string(), 0)]);
        assert_eq!(tree.node(2).and_then(|node| node.action), Some(7));
        assert_eq!(tree.node(4).and_then(|node| node.action), Some(9));
        let save = tree.properties(2);
        assert!(save.contains_key("shortcut") && save.contains_key("label"));
        assert!(tree.rebuild(Vec::new()));
        assert_eq!(tree.revision, 2);
    }

    fn glide_appmenu_like(root: &(i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) -> Vec<(String, usize)> {
        root.2
            .iter()
            .filter_map(|child| {
                <(i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)>::try_from(child.try_clone().ok()?).ok()
            })
            .map(|(_, props, children)| {
                let label = props.get("label").and_then(|value| String::try_from(value.try_clone().ok()?).ok());
                (label.unwrap_or_default(), children.len())
            })
            .collect()
    }

    #[test]
    fn clicks_reach_the_app_with_its_own_action_id() {
        let (sender, mut receiver) = unbounded();
        let tree = Arc::new(Mutex::new(Tree::default()));
        tree.lock().expect("lock").rebuild(menu());
        let server = Server { tree, events: sender };
        let data = || Value::from(0_i32).try_to_owned().expect("owned");
        server.event(2, "clicked".into(), data(), 0);
        server.event(1, "clicked".into(), data(), 0);
        server.event(4, "hovered".into(), data(), 0);
        assert!(matches!(receiver.try_recv(), Ok(Event::Activated(7))));
        assert!(receiver.try_recv().is_err(), "submenus and hovers send nothing");
    }
}
