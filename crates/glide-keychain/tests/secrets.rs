use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use glide_ipc::keychain::{Hidden, LOCAL, Request, Response};
use glide_keychain::daemon::Daemon;
use glide_keychain::secrets::{self, BUS_NAME, ROOT, Secrets};
use glide_keychain::service::Service;
use glide_keychain::vault::Params;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const FAST: Params = Params { memory_kib: 64, passes: 1, lanes: 1 };
const SERVICE: &str = "org.freedesktop.Secret.Service";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const ITEM: &str = "org.freedesktop.Secret.Item";
const DH: &str = "dh-ietf1024-sha256-aes128-cbc-pkcs7";

struct Bus {
    child: Child,
    address: String,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn private_bus() -> Option<Bus> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|err| eprintln!("no dbus-daemon ({err}); skipping"))
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut address).ok()?;
    Some(Bus { child, address: address.trim().to_string() })
}

fn connect(bus: &Bus) -> zbus::blocking::Connection {
    zbus::blocking::connection::Builder::address(bus.address.as_str()).unwrap().build().unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glide-keychain-secrets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("keychain")
}

type Found = (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>);
type SecretValue = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

fn proxy<'a>(client: &'a zbus::blocking::Connection, at: &'a str, interface: &'a str) -> zbus::blocking::Proxy<'a> {
    zbus::blocking::Proxy::new(client, BUS_NAME, at, interface).unwrap()
}

fn chrome() -> HashMap<&'static str, &'static str> {
    HashMap::from([("application", "chrome"), ("xdg:schema", "chrome_libsecret_os_crypt_password_v2")])
}

fn create(
    collection: &zbus::blocking::Proxy<'_>, session: &OwnedObjectPath, value: &[u8],
) -> (OwnedObjectPath, OwnedObjectPath) {
    let mut properties: HashMap<&str, Value<'_>> = HashMap::new();
    properties.insert("org.freedesktop.Secret.Item.Label", Value::from("Chrome Safe Storage"));
    properties.insert("org.freedesktop.Secret.Item.Attributes", Value::from(chrome()));
    let secret = (session.clone(), Vec::<u8>::new(), value.to_vec(), "text/plain");
    collection.call("CreateItem", &(properties, secret, true)).unwrap()
}

#[test]
fn programs_store_and_read_secrets_that_stay_encrypted_and_wait_for_the_island_when_locked() {
    let Some(bus) = private_bus() else {
        return;
    };
    let path = temp("flow");
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&events);
    let service = Service::start(Daemon::new(path.clone(), FAST), Arc::new(|_| {}));
    assert_eq!(service.handle(Request::Create { password: Hidden::new("pw".into()) }), Response::Done);
    let server = connect(&bus);
    let store = Secrets::new(
        Arc::clone(&service),
        Arc::new(move |event| seen.lock().unwrap().push(event)),
        secrets::index::path_beside(&path),
    );
    secrets::serve(&server, &store).unwrap();
    secrets::claim(&server, true);
    let client = connect(&bus);
    let root = proxy(&client, ROOT, SERVICE);

    let refused = root.call::<_, _, (OwnedValue, OwnedObjectPath)>("OpenSession", &(DH, Value::from(vec![1u8])));
    assert!(refused.unwrap_err().to_string().contains("InvalidArgs"), "a public key outside the group is refused");
    let unknown = root.call::<_, _, (OwnedValue, OwnedObjectPath)>("OpenSession", &("rot13", Value::from("")));
    assert!(unknown.unwrap_err().to_string().contains("NotSupported"));
    let (_, session): (OwnedValue, OwnedObjectPath) = root.call("OpenSession", &("plain", Value::from(""))).unwrap();
    let alias: OwnedObjectPath = root.call("ReadAlias", &("default",)).unwrap();
    let collection = proxy(&client, alias.as_str(), COLLECTION);

    let (item, prompt) = create(&collection, &session, b"first-key");
    assert_eq!(prompt.as_str(), "/");
    let (again, _) = create(&collection, &session, b"chrome-key-0451");
    assert_eq!(again, item, "replace=true overwrites the item with the same attributes");
    let found: Found = root.call("SearchItems", &(chrome(),)).unwrap();
    assert_eq!(found, (vec![item.clone()], Vec::new()));
    let secrets: HashMap<OwnedObjectPath, SecretValue> =
        root.call("GetSecrets", &(vec![item.clone()], session.clone())).unwrap();
    assert_eq!(secrets[&item].2, b"chrome-key-0451");

    let (dh, public) = secrets::session::Client::start().unwrap();
    let (reply, sealed_session): (OwnedValue, OwnedObjectPath) =
        root.call("OpenSession", &(DH, Value::from(public))).unwrap();
    let key = dh.finish(&Vec::<u8>::try_from(reply).unwrap());
    let (iv, value): (Vec<u8>, Vec<u8>) = {
        let secrets: HashMap<OwnedObjectPath, SecretValue> =
            root.call("GetSecrets", &(vec![item.clone()], sealed_session.clone())).unwrap();
        (secrets[&item].1.clone(), secrets[&item].2.clone())
    };
    assert_ne!(value, b"chrome-key-0451", "a DH session carries ciphertext over the bus");
    assert_eq!(key.open(&iv, &value).unwrap().as_slice(), b"chrome-key-0451");
    let (iv, value) = key.seal(b"sealed-write").unwrap();
    let item_proxy = proxy(&client, item.as_str(), ITEM);
    let () = item_proxy.call("SetSecret", &((sealed_session, iv, value, "text/plain"),)).unwrap();
    let plain: SecretValue = item_proxy.call("GetSecret", &(session.clone(),)).unwrap();
    assert_eq!(plain.2, b"sealed-write", "a sealed write is stored as its plain text");

    let vault = std::fs::read(&path).unwrap();
    assert!(!vault.windows(15).any(|window| window == b"chrome-key-0451"), "the vault file holds ciphertext only");
    let index = std::fs::read_to_string(secrets::index::path_beside(&path)).unwrap();
    assert!(!index.contains("chrome"), "the lookup index holds salted hashes only");

    assert_eq!(service.handle(Request::Lock { vault: None }), Response::Done);
    let found: Found = root.call("SearchItems", &(chrome(),)).unwrap();
    assert_eq!(found, (Vec::new(), vec![item.clone()]), "a locked item is still found, as locked");
    let locked =
        root.call::<_, _, HashMap<OwnedObjectPath, SecretValue>>("GetSecrets", &(vec![item.clone()], session.clone()));
    assert!(locked.unwrap_err().to_string().contains("IsLocked"));

    let (unlocked, prompt): (Vec<OwnedObjectPath>, OwnedObjectPath) =
        root.call("Unlock", &(vec![item.clone()],)).unwrap();
    assert!(unlocked.is_empty() && prompt.as_str() != "/");
    let prompt_proxy = proxy(&client, prompt.as_str(), "org.freedesktop.Secret.Prompt");
    let mut completed = prompt_proxy.receive_signal("Completed").unwrap();
    let () = prompt_proxy.call("Prompt", &("",)).unwrap();
    assert!(
        events.lock().unwrap().contains(&Response::UnlockRequested { vault: Some(LOCAL.into()) }),
        "the compositor asks in the island"
    );
    assert_eq!(
        service.handle(Request::Unlock { vault: LOCAL.into(), password: Hidden::new("pw".into()) }),
        Response::Done
    );
    let signal = completed.next().unwrap();
    let (dismissed, _): (bool, OwnedValue) = signal.body().deserialize().unwrap();
    assert!(!dismissed, "the prompt completes once the island unlocked the keychain");

    let item_proxy = proxy(&client, item.as_str(), ITEM);
    let secret = (session.clone(), Vec::<u8>::new(), b"rotated".to_vec(), "text/plain");
    let () = item_proxy.call("SetSecret", &(secret,)).unwrap();
    let value: SecretValue = item_proxy.call("GetSecret", &(session,)).unwrap();
    assert_eq!(value.2, b"rotated");
    let deleted: OwnedObjectPath = item_proxy.call("Delete", &()).unwrap();
    assert_eq!(deleted.as_str(), "/");
    let found: Found = root.call("SearchItems", &(chrome(),)).unwrap();
    assert_eq!(found, (Vec::new(), Vec::new()));
    let Response::Items { list } = service.handle(Request::Items) else { panic!() };
    assert!(list.is_empty(), "program secrets never show up as logins");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
