use std::path::{Path, PathBuf};

fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn every_hidden_text_field_goes_through_the_sdk_so_it_always_has_a_reveal_button() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let allowed = Path::new("glide-sdk").join("src").join("widget").join("inputs.rs");
    let mut offenders = Vec::new();
    for member in std::fs::read_dir(&crates).expect("the crates folder").flatten() {
        if member.file_name() == "glide-compositor" {
            continue;
        }
        let mut files = Vec::new();
        sources(&member.path().join("src"), &mut files);
        for file in files {
            let relative = file.strip_prefix(&crates).unwrap_or(&file).to_path_buf();
            if relative == allowed {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            if text.contains(".secure(") {
                offenders.push(relative.display().to_string());
            }
        }
    }
    assert!(offenders.is_empty(), "use glide_sdk::widget::secret_input instead of .secure(): {offenders:?}");
}
