use std::fs;
use std::os::unix::fs::PermissionsExt;

use glide_face::profile::crypto::{
    build_aad, decrypt, derive_key, encrypt_with, host_bound_secret_with, template_hash,
};
use glide_face::profile::storage::{DIR_MODE, FILE_MODE, PROFILE_VERSION};
use glide_face::profile::{Profile, Store, resolve_username};

fn test_profile() -> Profile {
    Profile { embedding: (0..128).map(|i| i as f32 * 0.01).collect(), num_samples: 5, ..Default::default() }
}

#[test]
fn existing_profiles_keep_decrypting_byte_for_byte() {
    let g: serde_json::Value = serde_json::from_str(&fs::read_to_string("tests/golden/crypto.json").unwrap()).unwrap();
    let secret: Vec<u8> = (0..32u32).map(|i| (i * 7) as u8).collect();
    let salt = *b"0123456789abcdef";
    let nonce = *b"nonce-12byte";
    assert_eq!(hex::encode(derive_key(&secret, &salt)), g["deriveKey"].as_str().unwrap());
    let emb: Vec<f32> = (0..128).map(|i| i as f32 * 0.01).collect();
    assert_eq!(template_hash(&secret, &emb), g["templateHash"].as_str().unwrap());
    let aad = build_aad(1000, "alice");
    assert_eq!(String::from_utf8(aad.clone()).unwrap(), g["aad"].as_str().unwrap());
    let ct = encrypt_with(&secret, br#"{"hello":"world"}"#, &aad, &salt, &nonce).unwrap();
    assert_eq!(hex::encode(&ct), g["ciphertext"].as_str().unwrap());
    assert_eq!(decrypt(&secret, &ct, &aad).unwrap(), br#"{"hello":"world"}"#);
    let raw: Vec<u8> = (0..40u32).map(|i| (255 - i) as u8).collect();
    let mid = g["machineId"].as_str().unwrap().as_bytes();
    assert_eq!(hex::encode(host_bound_secret_with(&raw, mid).unwrap()), g["hostBound"].as_str().unwrap());

    let p: Profile = serde_json::from_str(g["profileJSON"].as_str().unwrap()).unwrap();
    assert_eq!(p.username, "alice");
    assert_eq!(p.embedding.len(), 3);
}

#[test]
fn store_save_load_delete() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new_test(dir.path()).unwrap();
    assert!(!store.exists());

    let mut p = test_profile();
    store.save(&mut p).unwrap();
    assert_eq!(p.template_hash.len(), 16);
    assert_eq!((p.user_id, p.username.as_str(), p.version), (store.uid, store.username.as_str(), PROFILE_VERSION));

    for (path, want) in
        [(store.dir.clone(), DIR_MODE), (store.profile_path(), FILE_MODE), (store.key_path(), FILE_MODE)]
    {
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, want, "{}: mode {mode:o}", path.display());
    }

    let loaded = store.load().unwrap();
    assert_eq!(loaded.template_hash, p.template_hash);
    assert_eq!(loaded.embedding.len(), p.embedding.len());

    store.delete().unwrap();
    assert!(!store.exists());
    assert!(!store.key_path().exists());
}

#[test]
fn the_enrollment_camera_is_remembered_next_to_the_profile_and_only_a_video_device_is_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new_test(dir.path()).unwrap();
    assert_eq!(store.camera(), None);
    store.save_camera("/dev/video2").unwrap();
    assert_eq!(store.camera().as_deref(), Some("/dev/video2"));
    store.save_camera("/etc/shadow").unwrap();
    assert_eq!(store.camera(), None, "anything but /dev/videoN is ignored");
}

#[test]
fn store_rejects_tampering_and_swaps() {
    let base = tempfile::tempdir().unwrap();
    let store = Store::new_test(base.path()).unwrap();
    store.save(&mut test_profile()).unwrap();

    let data = fs::read(store.profile_path()).unwrap();
    let mut tampered = data.clone();
    *tampered.last_mut().unwrap() ^= 0xFF;
    fs::write(store.profile_path(), &tampered).unwrap();
    assert!(store.load().is_err(), "a tampered ciphertext must fail authentication");
    fs::write(store.profile_path(), &data).unwrap();
    store.load().unwrap();

    let mut other = store.clone();
    other.username.push_str("-other");
    assert!(other.load().is_err(), "another account must not open the profile");

    fs::set_permissions(store.profile_path(), fs::Permissions::from_mode(0o660)).unwrap();
    assert!(store.load().unwrap_err().is_insecure(), "a group-writable profile must be refused");
    fs::set_permissions(store.profile_path(), fs::Permissions::from_mode(FILE_MODE)).unwrap();

    let real = base.path().join("elsewhere.dat");
    fs::rename(store.profile_path(), &real).unwrap();
    std::os::unix::fs::symlink(&real, store.profile_path()).unwrap();
    assert!(store.load().unwrap_err().is_insecure());
}

#[test]
fn system_store_requires_root() {
    if nix::unistd::geteuid().is_root() {
        return;
    }
    let user = resolve_username("").unwrap();
    let store = Store::open(&user).unwrap();
    assert!(!store.can_write());
    assert!(store.save(&mut test_profile()).unwrap_err().is_not_root());
    assert!(store.delete().unwrap_err().is_not_root());
    assert!(resolve_username("root").is_err());
}

#[test]
fn profiles_of_a_previous_model_are_flagged_incompatible() {
    use glide_face::biometrics::{EMBEDDING_DIM, RECOGNIZER_ID};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new_test(dir.path()).unwrap();

    let mut current = Profile { embedding: vec![0.0; EMBEDDING_DIM], ..Default::default() };
    store.save(&mut current).unwrap();
    assert_eq!(current.model_name, RECOGNIZER_ID);
    assert!(store.load().unwrap().is_compatible());

    let mut old = Profile { embedding: vec![0.0; 128], model_name: "SFace-2021Dec-128d".into(), ..Default::default() };
    store.save(&mut old).unwrap();
    assert!(!store.load().unwrap().is_compatible());
}

#[test]
fn attempts_are_appended_and_survive_trimming() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new_test(dir.path()).unwrap();
    store.save(&mut Profile { embedding: vec![0.0; 4], ..Default::default() }).unwrap();
    for i in 0..1500 {
        store
            .append_attempt(&format!("{{\"at\":{i},\"source\":\"lock\",\"success\":true,\"reason\":\"passed\"}}"))
            .unwrap();
    }
    let text = store.read_attempts();
    assert!(text.lines().count() <= 1500);
    assert!(text.ends_with("\"at\":1499,\"source\":\"lock\",\"success\":true,\"reason\":\"passed\"}\n"));
    store.delete().unwrap();
    assert!(!store.attempts_path().exists());
}
