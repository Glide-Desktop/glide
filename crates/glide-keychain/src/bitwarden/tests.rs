use std::path::PathBuf;

use glide_ipc::keychain::{Draft, Hidden, ItemKind, ItemRef, Keep, Login, SyncState};

use super::Bitwarden;
use super::api::text;
use super::fake::{EMAIL, PASSWORD, Server};
use crate::provider::{Fault, Provider};

const NOW: u64 = 1_800_000_000;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glide-bitwarden-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn login(server: &Server) -> Login {
    Login {
        server: server.url.clone(),
        email: EMAIL.into(),
        password: Hidden::new(PASSWORD.into()),
        name: "Робота".into(),
        two_factor: None,
        device_code: None,
        api_key: None,
    }
}

fn draft(name: &str, password: &str) -> Draft {
    Draft {
        kind: ItemKind::Login,
        name: name.into(),
        username: "artem".into(),
        password: Some(Hidden::new(password.into())),
        ..Draft::default()
    }
}

fn edit(vault: &Bitwarden, id: &str, name: &str, password: &str) -> Draft {
    Draft { item: Some(ItemRef::new(vault.vault().id, id)), ..draft(name, password) }
}

fn names(vault: &Bitwarden) -> Vec<String> {
    let mut names: Vec<String> = vault
        .records()
        .expect("records")
        .iter()
        .filter(|record| record.deleted.is_none())
        .map(|record| record.name.clone())
        .collect();
    names.sort();
    names
}

fn sync(vault: &mut Bitwarden) {
    let job = vault.prepare_sync(true, NOW).expect("an open vault syncs");
    vault.apply_sync(job.run(NOW), NOW);
}

#[test]
fn a_login_keeps_only_ciphertext_on_the_server_and_on_disk() {
    let server = Server::start();
    let home = dir("login");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    assert_eq!(vault.vault().name, "Робота");
    let id = vault.save(&draft("GitHub", "c7#Vq2!mWx9tLp4Rz8"), NOW).expect("save");
    assert!(!id.starts_with("new:"), "an online save gets the server's id");
    let stored = serde_json::to_string(&server.state.lock().expect("state").ciphers).expect("json");
    assert!(!stored.contains("GitHub") && !stored.contains("c7#Vq2"), "the server sees only ciphertext");
    let on_disk = std::fs::read_to_string(home.join(format!("{}.json", vault.vault().id))).expect("account file");
    assert!(!on_disk.contains("GitHub") && !on_disk.contains("c7#Vq2") && !on_disk.contains(PASSWORD));
    vault.lock();
    assert_eq!(vault.unlock(&Hidden::new("wrong".into())), Err(Fault::WrongPassword));
    vault.unlock(&Hidden::new(PASSWORD.into())).expect("unlock");
    assert_eq!(names(&vault), ["GitHub"], "unlocking reads the cached copy without the network");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_wrong_master_password_is_told_apart() {
    let server = Server::start();
    let home = dir("wrong");
    let mut bad = login(&server);
    bad.password = Hidden::new("nope".into());
    assert_eq!(Bitwarden::login(&home, &bad, NOW).map(|_| ()), Err(super::LoginError::WrongPassword));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_new_device_is_confirmed_once_and_keeps_its_id() {
    let server = Server::start();
    server.state.lock().expect("state").verify_devices = true;
    let home = dir("device");
    assert_eq!(Bitwarden::login(&home, &login(&server), NOW).map(|_| ()), Err(super::LoginError::DeviceCode));
    let mut confirmed = login(&server);
    confirmed.device_code = Some(super::fake::DEVICE_OTP.into());
    let first = Bitwarden::login(&home, &confirmed, NOW).expect("a confirmed device signs in");
    let second = Bitwarden::login(&home, &login(&server), NOW).expect("the same device needs no code again");
    assert_eq!(first.account().device, second.account().device);
    Bitwarden::send_login_email(&home, &login(&server)).expect("email");
    assert_eq!(server.state.lock().expect("state").emails, [first.account().device.clone()]);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn offline_changes_queue_and_reach_the_server_with_its_ids() {
    let server = Server::start();
    let home = dir("offline");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    let kept = vault.save(&draft("Router", "admin123"), NOW).expect("save");
    server.go_offline(true);
    let fresh = vault.save(&draft("Hetzner", "Hetzner2024"), NOW).expect("an offline save is queued");
    assert!(fresh.starts_with("new:"));
    vault.save(&edit(&vault, &fresh, "Hetzner Cloud", "Hetzner2025"), NOW).expect("a second edit is queued");
    vault.trash(&kept, NOW).expect("an offline trash is queued");
    assert_eq!(names(&vault), ["Hetzner Cloud"], "queued changes show at once");
    assert!(matches!(vault.vault().sync, SyncState::Offline { pending: 2 }), "{:?}", vault.vault().sync);
    sync(&mut vault);
    assert!(matches!(vault.vault().sync, SyncState::Offline { .. }), "an offline sync keeps the queue");
    server.go_offline(false);
    sync(&mut vault);
    assert_eq!(vault.vault().sync, SyncState::Synced { at: NOW });
    assert_eq!(names(&vault), ["Hetzner Cloud"]);
    assert!(vault.records().expect("records").iter().all(|record| !record.id.starts_with("new:")));
    let state = server.state.lock().expect("state");
    assert_eq!(state.ciphers.len(), 2, "the two saves of one new item make one item");
    let trashed = state.ciphers.iter().find(|cipher| text(cipher, "id") == Some(kept.as_str())).expect("kept");
    assert!(text(trashed, "deletedDate").is_some());
    drop(state);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_new_item_trashed_and_purged_offline_never_reaches_the_server() {
    let server = Server::start();
    let home = dir("purged");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    server.go_offline(true);
    let fresh = vault.save(&draft("Draft", "x"), NOW).expect("queued");
    vault.trash(&fresh, NOW).expect("trash");
    vault.purge(&fresh).expect("purge");
    assert!(vault.account().queue.is_empty(), "{:?}", vault.account().queue);
    server.go_offline(false);
    sync(&mut vault);
    assert!(server.state.lock().expect("state").ciphers.is_empty());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_clash_survives_locking_and_resolves_either_way() {
    let server = Server::start();
    let home = dir("clash");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    let id = vault.save(&draft("GitLab", "first"), NOW).expect("save");
    server.go_offline(true);
    vault.save(&edit(&vault, &id, "GitLab", "mine"), NOW).expect("queued edit");
    server.state.lock().expect("state").touch(&id);
    server.go_offline(false);
    sync(&mut vault);
    assert!(vault.account().queue.is_empty());
    vault.lock();
    vault.unlock(&Hidden::new(PASSWORD.into())).expect("unlock");
    let conflicts = vault.conflicts();
    assert_eq!(conflicts.len(), 1, "a clash is kept on disk, not only in memory");
    assert_eq!(conflicts[0].local_password.expose(), "mine");
    assert_eq!(conflicts[0].remote_password.expose(), "first");
    vault.resolve(&id, Keep::Local).expect("resolve");
    assert!(vault.conflicts().is_empty());
    sync(&mut vault);
    let record = vault.records().expect("records").iter().find(|record| record.id == id).cloned().expect("item");
    assert_eq!(record.password_text(), "mine");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_send_keeps_its_key_in_the_link_and_only_ciphertext_on_the_server() {
    let server = Server::start();
    let home = dir("send");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    let draft = glide_ipc::keychain::SendDraft {
        vault: vault.vault().id,
        name: "Wi-Fi".into(),
        text: Hidden::new("guest: hunter2".into()),
        lifetime_hours: 24,
        max_access: Some(3),
        password: None,
        hide_text: true,
        hide_email: false,
    };
    let send = vault.create_send(&draft).expect("send");
    assert_eq!(send.name, "Wi-Fi");
    assert!(send.link.starts_with(&format!("{}/#/send/access-", server.url)), "{}", send.link);
    let key = send.link.rsplit('/').next().expect("key part");
    assert_eq!(key.len(), 22, "16 secret bytes, URL-safe base64 without padding");
    let stored = serde_json::to_string(&server.state.lock().expect("state").sends).expect("json");
    assert!(!stored.contains("Wi-Fi") && !stored.contains("hunter2") && !stored.contains(key));
    assert_eq!(vault.sends().expect("sends").len(), 1);
    vault.delete_send(&send.id).expect("delete");
    assert!(vault.sends().expect("sends").is_empty());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn an_edited_send_keeps_its_link_and_password_unless_told_otherwise() {
    let server = Server::start();
    let home = dir("send-edit");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    let mut draft = glide_ipc::keychain::SendDraft {
        vault: vault.vault().id,
        name: "Wi-Fi".into(),
        text: Hidden::new("guest: hunter2".into()),
        lifetime_hours: 24,
        max_access: Some(3),
        password: Some(Hidden::new("door".into())),
        hide_text: true,
        hide_email: false,
    };
    let send = vault.create_send(&draft).expect("send");
    assert!(send.has_password);
    assert_eq!(vault.send_text(&send.id).expect("text").expose(), "guest: hunter2");

    draft.name = "Guest Wi-Fi".into();
    draft.text = Hidden::new("guest: plum-ocean".into());
    draft.max_access = Some(5);
    draft.password = None;
    let edited = vault.edit_send(&send.id, &draft, true).expect("edit");
    assert_eq!(edited.link, send.link);
    assert_eq!((edited.name.as_str(), edited.max_access, edited.has_password), ("Guest Wi-Fi", Some(5), true));
    assert_eq!(vault.send_text(&send.id).expect("text").expose(), "guest: plum-ocean");
    let stored = serde_json::to_string(&server.state.lock().expect("state").sends).expect("json");
    assert!(!stored.contains("Guest") && !stored.contains("plum-ocean"));

    let open = vault.edit_send(&send.id, &draft, false).expect("drop the password");
    assert!(!open.has_password);
    assert_eq!(vault.sends().expect("sends").len(), 1);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn changes_made_while_a_sync_runs_wait_for_the_next_one() {
    let server = Server::start();
    let home = dir("during");
    let mut vault = Bitwarden::login(&home, &login(&server), NOW).expect("login");
    let job = vault.prepare_sync(true, NOW).expect("job");
    assert!(vault.prepare_sync(true, NOW).is_none(), "one sync at a time");
    assert_eq!(vault.vault().sync, SyncState::Syncing);
    let fresh = vault.save(&draft("Steam", "s"), NOW).expect("queued while syncing");
    assert!(fresh.starts_with("new:"));
    let done = job.run(NOW);
    vault.apply_sync(done, NOW);
    assert_eq!(vault.account().queue.len(), 1, "the change made during the sync stays queued");
    assert_eq!(names(&vault), ["Steam"]);
    sync(&mut vault);
    assert!(vault.account().queue.is_empty());
    assert_eq!(server.state.lock().expect("state").ciphers.len(), 1);
    let _ = std::fs::remove_dir_all(home);
}
