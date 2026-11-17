use std::path::Path;

use glide_config::service::flatten;
use serde_json::Value;

fn check(name: &str, defaults: serde_json::Result<Value>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults").join(name);
    let values = defaults.map(flatten).expect("defaults serialize");
    let expected = serde_json::to_string_pretty(&values).expect("JSON") + "\n";
    if std::env::var_os("GLIDE_WRITE_DEFAULTS").is_some() {
        std::fs::write(&path, &expected).expect("write the defaults");
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap_or_default(),
        expected,
        "data/defaults/{name} is generated from glide-config; rerun with GLIDE_WRITE_DEFAULTS=1"
    );
}

#[test]
fn the_installed_password_defaults_are_the_prefs_defaults() {
    check("passwords.json", serde_json::to_value(glide_config::passwords::Prefs::default()));
}

#[test]
fn the_installed_permission_defaults_are_the_permissions_defaults() {
    check("permissions.json", serde_json::to_value(glide_config::permissions::Permissions::default()));
}

#[test]
fn the_installed_pin_defaults_mean_nothing_was_pinned_yet() {
    check("pins.json", serde_json::to_value(glide_config::pins::Stored::default()));
}
