use std::path::PathBuf;

use glide_configd::import;
use glide_configd::store::{Defaults, Store};
use serde_json::json;

fn store() -> Store {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults");
    Store::in_memory(Defaults::load(&[dir])).expect("store")
}

#[test]
fn the_installed_shell_defaults_take_what_settings_writes() {
    let mut store = store();
    let text = "[topbar]\naccent = \"teal\"\nappearance = \"light\"\nclock-24h = true\n\n\
                [general]\nterminal = \"foot\"\nwallpaper = \"/tmp/w.png\"\n\n\
                [input]\nxkb-layout = \"us,ua\"\nxkb-options = \"grp:win_space_toggle\"\naccel-speed = 0.3\n\n\
                [dock]\nautohide = \"always\"\n\n[night-light]\ntemperature = 3500\n";
    let imported = import::toml_text(&mut store, "shell", text).expect("imports");
    assert!(imported.dropped.is_empty(), "dropped {:?}", imported.dropped);
    let all = store.get_all("shell").expect("read");
    assert_eq!(all["topbar.accent"], json!("teal"));
    assert_eq!(all["general.terminal"], json!("foot"));
    assert_eq!(all["input.xkb-options"], json!("grp:win_space_toggle"));
    assert_eq!(all["night-light.temperature"], json!(3500));
}

#[test]
#[ignore = "reads $GLIDE_IMPORT_CHECK, a copy of a real glide.toml"]
fn a_real_settings_file_imports() {
    let path = std::env::var_os("GLIDE_IMPORT_CHECK").expect("GLIDE_IMPORT_CHECK names a glide.toml copy");
    let text = std::fs::read_to_string(path).expect("readable");
    let imported = import::toml_text(&mut store(), "shell", &text).expect("imports");
    eprintln!("imported {} keys, dropped {:?}", imported.keys, imported.dropped);
}
