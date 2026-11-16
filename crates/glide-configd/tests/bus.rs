use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use glide_configd::bus::{self, BUS_NAME, INTERFACE, PATH};
use glide_configd::store::{Defaults, Store, Values};
use glide_services::test_bus;
use serde_json::json;

fn store() -> Arc<Mutex<Store>> {
    let shell = Values::from([("topbar.accent".to_owned(), json!("blue")), ("topbar.tray".to_owned(), json!(true))]);
    let permissions = Values::from([("allowed".to_owned(), json!([]))]);
    let defaults = Defaults::from_namespaces(BTreeMap::from([
        ("shell".to_owned(), shell),
        ("permissions".to_owned(), permissions),
    ]));
    Arc::new(Mutex::new(Store::in_memory(defaults).expect("store")))
}

fn call<T: serde::de::DeserializeOwned + zbus::zvariant::Type>(
    client: &zbus::blocking::Connection, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
) -> zbus::Result<T> {
    client.call_method(Some(BUS_NAME), PATH, Some(INTERFACE), method, body)?.body().deserialize::<T>()
}

#[test]
fn the_served_interface_matches_its_published_contract() {
    let Some(daemon) = test_bus::private_bus() else { return };
    let _server = bus::serve_on(daemon.builder(), store(), Vec::new()).expect("serve");
    let served = test_bus::introspect(&daemon, BUS_NAME, PATH);
    let contract = test_bus::members(include_str!("../../../data/dbus/org.glide.Config1.xml"), INTERFACE);
    assert_eq!(contract.len(), 7, "six methods and one signal");
    assert_eq!(
        test_bus::members(&served, INTERFACE),
        contract,
        "a change to org.glide.Config1 goes into data/dbus first; removing or altering a member needs Config2"
    );
}

#[test]
fn a_set_is_read_back_announced_and_undone() {
    let Some(daemon) = test_bus::private_bus() else { return };
    let _server = bus::serve_on(daemon.builder(), store(), Vec::new()).expect("serve");
    let client = daemon.client();
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(INTERFACE)
        .expect("interface")
        .member("Changed")
        .expect("member")
        .build();
    let mut signals = zbus::blocking::MessageIterator::for_match_rule(rule, &client, None).expect("match");

    let values = HashMap::from([("topbar.accent".to_owned(), "\"amber\"".to_owned())]);
    call::<()>(&client, "SetMany", &("shell", values)).expect("set");
    let signal = signals.next().expect("a Changed signal").expect("message");
    let (namespace, changed): (String, HashMap<String, String>) = signal.body().deserialize().expect("body");
    assert_eq!((namespace.as_str(), changed["topbar.accent"].as_str()), ("shell", "\"amber\""));

    let all: HashMap<String, String> = call(&client, "GetAll", &("shell",)).expect("get all");
    assert_eq!((all["topbar.accent"].as_str(), all["topbar.tray"].as_str()), ("\"amber\"", "true"));

    let history: String = call(&client, "History", &("shell", 10u32)).expect("history");
    let history: serde_json::Value = serde_json::from_str(&history).expect("history JSON");
    let id = history[0]["id"].as_i64().expect("an id");
    call::<()>(&client, "Undo", &(id,)).expect("undo");
    let accent: String = call(&client, "Get", &("shell", "topbar.accent")).expect("get");
    assert_eq!(accent, "\"blue\"");
}

#[test]
fn bad_writes_are_refused() {
    let Some(daemon) = test_bus::private_bus() else { return };
    let _server = bus::serve_on(daemon.builder(), store(), Vec::new()).expect("serve");
    let client = daemon.client();
    let unknown = HashMap::from([("topbar.nope".to_owned(), "1".to_owned())]);
    assert!(call::<()>(&client, "SetMany", &("shell", unknown)).is_err());
    let not_json = HashMap::from([("topbar.accent".to_owned(), "amber".to_owned())]);
    assert!(call::<()>(&client, "SetMany", &("shell", not_json)).is_err());
    let grant = HashMap::from([("allowed".to_owned(), "[\"/usr/bin/evil\"]".to_owned())]);
    let refused = call::<()>(&client, "SetMany", &("permissions", grant)).expect_err("an untrusted writer");
    assert!(refused.to_string().contains("AccessDenied"), "{refused}");
}
