use std::path::PathBuf;

use glide_ipc::keychain::{Draft, Hidden, ItemKind, ItemRef, Keep, Login, SendDraft, SyncState};
use serde_json::json;

use super::Bitwarden;
use super::api::{Client, Endpoints};
use super::crypto::{self, Kdf, SymmetricKey};
use crate::provider::Provider;

const PASSWORD: &str = "Glide live check 2026";

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glide-bitwarden-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn register(server: &str, email: &str, kdf: Kdf) {
    let master = crypto::master_key(PASSWORD.as_bytes(), email, kdf).expect("master key");
    let user = SymmetricKey::generate().expect("user key");
    let pair = crypto::generate_key_pair(&user).expect("key pair");
    let (kind, iterations, memory, parallelism) = match kdf {
        Kdf::Pbkdf2 { iterations } => (0, iterations, None, None),
        Kdf::Argon2id { iterations, memory_mib, parallelism } => (1, iterations, Some(memory_mib), Some(parallelism)),
    };
    let body = json!({
        "email": email,
        "masterPasswordHash": crypto::master_password_hash(&master, PASSWORD.as_bytes()),
        "masterPasswordHint": null,
        "userSymmetricKey": SymmetricKey::stretch(&master).encrypt(&user.to_bytes()).expect("wrap"),
        "userAsymmetricKeys": { "publicKey": pair.public, "encryptedPrivateKey": pair.private_encrypted },
        "kdf": kind,
        "kdfIterations": iterations,
        "kdfMemory": memory,
        "kdfParallelism": parallelism,
    });
    Client::new(Endpoints::for_server(server)).register(email, "Glide", &body).expect("registration");
}

fn login(server: &str, email: &str) -> Login {
    Login {
        server: server.into(),
        email: email.into(),
        password: Hidden::new(PASSWORD.into()),
        name: "Vaultwarden".into(),
        two_factor: None,
        device_code: None,
        api_key: None,
    }
}

fn draft(vault: Option<&Bitwarden>, id: Option<&str>, name: &str, password: &str) -> Draft {
    Draft {
        item: vault.zip(id).map(|(vault, id)| ItemRef::new(vault.vault().id, id)),
        kind: ItemKind::Login,
        name: name.into(),
        username: "artem".into(),
        password: Some(Hidden::new(password.into())),
        folder: "Робота".into(),
        ..Draft::default()
    }
}

fn sync(vault: &mut Bitwarden) {
    let at = now();
    let job = vault.prepare_sync(true, at).expect("an open vault syncs");
    vault.apply_sync(job.run(at), at);
}

fn password_of(vault: &Bitwarden, id: &str) -> String {
    vault.records().expect("records").iter().find(|record| record.id == id).expect("item").password_text()
}

fn run(server: &str, kdf: Kdf, tag: &str) {
    let email = format!("glide-{tag}-{}-{}@example.org", std::process::id(), now());
    register(server, &email, kdf);
    let (home, other) = (dir(&format!("{tag}-a")), dir(&format!("{tag}-b")));
    let mut laptop = Bitwarden::login(&home, &login(server, &email), now()).expect("login");
    assert!(matches!(laptop.vault().sync, SyncState::Synced { .. }), "{:?}", laptop.vault().sync);
    let id = laptop.save(&draft(None, None, "GitHub", "first"), now()).expect("save");
    assert!(!id.starts_with("new:"), "an online save gets the server's id");
    assert_eq!(laptop.folders().expect("folders"), ["Робота"], "the folder was created on the server");

    let mut phone = Bitwarden::login(&other, &login(server, &email), now()).expect("second device");
    assert_eq!(password_of(&phone, &id), "first", "the second device decrypts what the first saved");
    phone.save(&draft(Some(&phone), Some(&id), "GitHub", "from the phone"), now()).expect("edit");
    sync(&mut laptop);
    assert_eq!(password_of(&laptop, &id), "from the phone", "a sync brings the other device's edit");

    let job = laptop.prepare_sync(true, now()).expect("job");
    laptop.save(&draft(Some(&laptop), Some(&id), "GitHub", "from the laptop"), now()).expect("queued");
    laptop.apply_sync(job.run(now()), now());
    std::thread::sleep(std::time::Duration::from_millis(1100));
    phone.save(&draft(Some(&phone), Some(&id), "GitHub", "phone again"), now()).expect("edit");
    sync(&mut laptop);
    let conflicts = laptop.conflicts();
    assert_eq!(conflicts.len(), 1, "an edit made against an older revision is a conflict");
    assert_eq!(conflicts[0].local_password.expose(), "from the laptop");
    assert_eq!(conflicts[0].remote_password.expose(), "phone again");
    laptop.resolve(&id, Keep::Local).expect("resolve");
    sync(&mut phone);
    assert_eq!(password_of(&phone, &id), "from the laptop", "keeping mine overwrites the server");

    let mut draft = SendDraft {
        vault: laptop.vault().id,
        name: "Wi-Fi".into(),
        text: Hidden::new("guest: hunter2".into()),
        lifetime_hours: 24,
        max_access: Some(3),
        password: Some(Hidden::new("door".into())),
        hide_text: false,
        hide_email: true,
    };
    let send = laptop.create_send(&draft).expect("send");
    assert!(send.link.contains("/#/send/"), "{}", send.link);
    assert!(send.has_password);
    draft.name = "Guest Wi-Fi".into();
    draft.text = Hidden::new("guest: plum-ocean".into());
    draft.password = None;
    let edited = laptop.edit_send(&send.id, &draft, true).expect("edit send");
    assert_eq!((edited.link.as_str(), edited.name.as_str()), (send.link.as_str(), "Guest Wi-Fi"));
    assert!(edited.has_password, "an empty password field keeps the old one");
    sync(&mut laptop);
    assert_eq!(laptop.send_text(&send.id).expect("text").expose(), "guest: plum-ocean");
    assert!(!laptop.edit_send(&send.id, &draft, false).expect("drop the password").has_password);
    laptop.delete_send(&send.id).expect("delete send");

    laptop.trash(&id, now()).expect("trash");
    sync(&mut phone);
    assert!(phone.records().expect("records").iter().any(|record| record.id == id && record.deleted.is_some()));
    laptop.restore(&id).expect("restore");
    laptop.purge(&id).expect("purge");
    sync(&mut phone);
    assert!(phone.records().expect("records").iter().all(|record| record.id != id));

    laptop.lock();
    laptop.unlock(&Hidden::new(PASSWORD.into())).expect("unlock from the cache");
    assert_eq!(laptop.unlock(&Hidden::new("wrong".into())), Err(crate::provider::Fault::WrongPassword));
    let _ = (std::fs::remove_dir_all(home), std::fs::remove_dir_all(other));
}

fn two_step(server: &str) {
    let email = format!("glide-2fa-{}-{}@example.org", std::process::id(), now());
    let kdf = Kdf::Pbkdf2 { iterations: 600_000 };
    register(server, &email, kdf);
    let home = dir("2fa");
    let mut vault = Bitwarden::login(&home, &login(server, &email), now()).expect("login");
    let master = crypto::master_key(PASSWORD.as_bytes(), &email, kdf).expect("master key");
    let hash = crypto::master_password_hash(&master, PASSWORD.as_bytes());
    let request = json!({ "masterPasswordHash": hash });
    let offered = vault
        .call(now(), |client, token| client.post("/two-factor/get-authenticator", token, &request))
        .expect("authenticator key");
    let key = super::api::text(&offered, "key").expect("key").to_string();
    let totp = crate::totp::Totp::parse(&key).expect("a base32 key");
    let enable = json!({ "key": key, "token": totp.code_at(now()), "masterPasswordHash": hash });
    vault
        .call(now(), |client, token| client.put("/two-factor/authenticator", token, Some(&enable)))
        .expect("two-step login turned on");

    let again = dir("2fa-again");
    let asked = Bitwarden::login(&again, &login(server, &email), now()).map(|_| ());
    assert_eq!(asked, Err(super::LoginError::TwoFactor(vec![glide_ipc::keychain::TwoFactorMethod::Authenticator])));
    let mut wrong = login(server, &email);
    wrong.two_factor = Some(glide_ipc::keychain::TwoFactor {
        method: glide_ipc::keychain::TwoFactorMethod::Authenticator,
        code: "000000".into(),
        remember: false,
    });
    let rejected = Bitwarden::login(&again, &wrong, now()).map(|_| ());
    assert!(matches!(rejected, Err(super::LoginError::TwoFactor(_))), "a wrong code asks again: {rejected:?}");
    let used = now() / 30;
    while now() / 30 < used + 2 {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let mut with_code = login(server, &email);
    with_code.two_factor = Some(glide_ipc::keychain::TwoFactor {
        method: glide_ipc::keychain::TwoFactorMethod::Authenticator,
        code: totp.code_at(now()),
        remember: true,
    });
    let entered = Bitwarden::login(&again, &with_code, now()).expect("the code lets the login through");
    assert!(entered.account().remember.is_some(), "a remembered device keeps the server's token, sealed");
    let _ = (std::fs::remove_dir_all(home), std::fs::remove_dir_all(again));
}

fn through_the_service(server: &str) {
    use glide_ipc::keychain::{Request, Response};
    let email = format!("glide-service-{}-{}@example.org", std::process::id(), now());
    register(server, &email, Kdf::Pbkdf2 { iterations: 600_000 });
    let home = dir("service");
    let daemon =
        crate::daemon::Daemon::new(home.join("keychain"), crate::vault::Params { memory_kib: 64, passes: 1, lanes: 1 });
    let service = crate::service::Service::start(daemon, std::sync::Arc::new(|_| {}));
    let Response::VaultAdded { vault } =
        service.handle(Request::AddBitwarden { login: Box::new(login(server, &email)) })
    else {
        panic!("the account was not added");
    };
    let draft = Draft { vault: vault.clone(), ..draft(None, None, "Hetzner", "Hx8$pQ2m") };
    let Response::Saved { item } = service.handle(Request::Save { draft: Box::new(draft) }) else {
        panic!("the item was not saved");
    };
    assert!(!item.id.starts_with("new:"));
    assert_eq!(service.handle(Request::Lock { vault: None }), Response::Done);
    let unlock = Request::Unlock { vault: vault.clone(), password: Hidden::new(PASSWORD.into()) };
    assert_eq!(service.handle(unlock), Response::Done);
    let Response::Items { list } = service.handle(Request::Items) else { panic!() };
    assert!(list.iter().any(|summary| summary.item == item && summary.name == "Hetzner"));
    let started = std::time::Instant::now();
    loop {
        let Response::Vaults { list, .. } = service.handle(Request::Vaults) else { panic!() };
        let state = list.into_iter().find(|known| known.id == vault).expect("vault").sync;
        if matches!(state, SyncState::Synced { .. }) {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the background sync did not finish: {state:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let secret = Request::Secret { item, secret: glide_ipc::keychain::SecretKind::Password };
    assert_eq!(service.handle(secret), Response::Secret { secret: Hidden::new("Hx8$pQ2m".into()) });
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "needs a Vaultwarden or Bitwarden server in GLIDE_VAULTWARDEN"]
fn a_real_server_through_the_service() {
    let server = std::env::var("GLIDE_VAULTWARDEN").expect("GLIDE_VAULTWARDEN=http://127.0.0.1:8765");
    through_the_service(&server);
}

#[test]
#[ignore = "needs a Vaultwarden or Bitwarden server in GLIDE_VAULTWARDEN"]
fn a_real_server_round_trip() {
    let server = std::env::var("GLIDE_VAULTWARDEN").expect("GLIDE_VAULTWARDEN=http://127.0.0.1:8765");
    run(&server, Kdf::Pbkdf2 { iterations: 600_000 }, "pbkdf2");
    run(&server, Kdf::Argon2id { iterations: 3, memory_mib: 64, parallelism: 4 }, "argon2");
    two_step(&server);
}
