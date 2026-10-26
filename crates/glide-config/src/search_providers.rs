use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub desktop_id: String,
    pub bus: String,
    pub path: String,
}

#[must_use]
pub fn parse_ini(text: &str) -> Option<Provider> {
    let mut in_section = false;
    let mut keys: HashMap<&str, &str> = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line == "[Shell Search Provider]";
        } else if in_section && let Some((key, value)) = line.split_once('=') {
            keys.insert(key.trim(), value.trim());
        }
    }
    if keys.get("Version").is_some_and(|version| *version != "2")
        || keys.get("DefaultDisabled").is_some_and(|value| *value == "true")
    {
        return None;
    }
    let desktop_id = keys.get("DesktopId")?.trim_end_matches(".desktop").to_string();
    Some(Provider {
        desktop_id,
        bus: (*keys.get("BusName")?).to_string(),
        path: (*keys.get("ObjectPath")?).to_string(),
    })
}

#[must_use]
pub fn discover() -> Vec<Provider> {
    let mut found: Vec<Provider> = Vec::new();
    for dir in crate::data_dirs() {
        let Ok(entries) = std::fs::read_dir(dir.join("gnome-shell/search-providers")) else {
            continue;
        };
        let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        files.sort();
        for file in files.iter().filter(|file| file.extension().is_some_and(|ext| ext == "ini")) {
            let Some(provider) = std::fs::read_to_string(file).ok().as_deref().and_then(parse_ini) else {
                continue;
            };
            if !found.iter().any(|known| known.bus == provider.bus && known.path == provider.path) {
                found.push(provider);
            }
        }
    }
    found
}

#[must_use]
pub fn same_app(a: &str, b: &str) -> bool {
    a.strip_suffix(".desktop").unwrap_or(a).eq_ignore_ascii_case(b.strip_suffix(".desktop").unwrap_or(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_file_names_its_bus_path_and_app() {
        let ini = "[Shell Search Provider]\nDesktopId=org.gnome.Nautilus.desktop\nBusName=org.gnome.Nautilus\nObjectPath=/org/gnome/Nautilus/SearchProvider\nVersion=2\n";
        assert_eq!(
            parse_ini(ini),
            Some(Provider {
                desktop_id: "org.gnome.Nautilus".into(),
                bus: "org.gnome.Nautilus".into(),
                path: "/org/gnome/Nautilus/SearchProvider".into(),
            })
        );
        assert_eq!(parse_ini(&format!("{ini}DefaultDisabled=true\n")), None);
        assert_eq!(parse_ini("[Shell Search Provider]\nBusName=a\n"), None);
    }

    #[test]
    fn app_ids_compare_without_the_desktop_suffix() {
        assert!(same_app("org.gnome.Nautilus.desktop", "org.gnome.nautilus"));
        assert!(!same_app("org.gnome.Nautilus", "org.gnome.Calculator"));
    }
}
