use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub desktop_id: String,
    pub bus: String,
    pub path: String,
}

fn parse_entry(text: &str) -> Option<(Provider, bool)> {
    let mut in_section = false;
    let mut keys: HashMap<&str, &str> = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line == "[Shell Search Provider]";
        } else if in_section && let Some((key, value)) = line.split_once('=') {
            keys.insert(key.trim(), value.trim());
        }
    }
    if keys.get("Version").and_then(|version| version.parse::<u32>().ok()).is_none_or(|version| version < 2) {
        return None;
    }
    let enabled = keys.get("DefaultDisabled").is_none_or(|value| *value != "true");
    let desktop_id = *keys.get("DesktopId")?;
    let desktop_id = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id).to_string();
    let provider =
        Provider { desktop_id, bus: (*keys.get("BusName")?).to_string(), path: (*keys.get("ObjectPath")?).to_string() };
    Some((provider, enabled))
}

#[must_use]
pub fn parse_ini(text: &str) -> Option<Provider> {
    parse_entry(text).filter(|(_, enabled)| *enabled).map(|(provider, _)| provider)
}

fn merge<S: AsRef<str>>(texts: impl IntoIterator<Item = S>) -> Vec<Provider> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut found = Vec::new();
    for text in texts {
        let Some((provider, enabled)) = parse_entry(text.as_ref()) else {
            continue;
        };
        if seen.insert((provider.bus.clone(), provider.path.clone())) && enabled {
            found.push(provider);
        }
    }
    found
}

#[must_use]
pub fn discover() -> Vec<Provider> {
    let mut texts: Vec<String> = Vec::new();
    for dir in crate::data_dirs() {
        let Ok(entries) = std::fs::read_dir(dir.join("gnome-shell/search-providers")) else {
            continue;
        };
        let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        files.sort();
        texts.extend(
            files
                .iter()
                .filter(|file| file.extension().is_some_and(|ext| ext == "ini"))
                .filter_map(|file| std::fs::read_to_string(file).ok()),
        );
    }
    merge(texts)
}

#[must_use]
pub fn same_app(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
        || a.strip_suffix(".desktop").is_some_and(|stem| stem.eq_ignore_ascii_case(b))
        || b.strip_suffix(".desktop").is_some_and(|stem| stem.eq_ignore_ascii_case(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_file(file_id: &str, bus: &str, extra: &str) -> String {
        format!(
            "[Shell Search Provider]\nDesktopId={file_id}\nBusName={bus}\nObjectPath=/{}\nVersion=2\n{extra}",
            bus.replace('.', "/")
        )
    }

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
    fn only_the_file_extension_is_cut_from_the_desktop_id() {
        let desktop_id =
            |id: &str| parse_ini(&provider_file(id, "org.telegram.Search", "")).map(|provider| provider.desktop_id);
        assert_eq!(desktop_id("org.telegram.desktop.desktop").as_deref(), Some("org.telegram.desktop"));
        assert_eq!(desktop_id("org.telegram.desktop").as_deref(), Some("org.telegram"), "a file name, so its stem");
        assert_eq!(desktop_id("firefox.desktop").as_deref(), Some("firefox"));
        assert_eq!(desktop_id("firefox").as_deref(), Some("firefox"));
        assert_eq!(desktop_id("a.desktop.desktop.desktop").as_deref(), Some("a.desktop.desktop"));
    }

    #[test]
    fn a_disabled_user_file_hides_the_system_provider_it_copies() {
        let system = provider_file("foo.desktop", "org.foo.Search", "");
        let other = provider_file("bar.desktop", "org.bar.Search", "");
        let user_off = provider_file("foo.desktop", "org.foo.Search", "DefaultDisabled=true\n");
        let ids = |texts: &[&str]| {
            merge(texts.iter().copied()).into_iter().map(|provider| provider.desktop_id).collect::<Vec<_>>()
        };
        assert_eq!(ids(&[system.as_str(), other.as_str()]), ["foo", "bar"]);
        assert_eq!(
            ids(&[user_off.as_str(), system.as_str(), other.as_str()]),
            ["bar"],
            "the user copy shadows the system one"
        );
        assert_eq!(ids(&[system.as_str(), user_off.as_str()]), ["foo"], "a later copy never overrides an earlier one");
    }

    #[test]
    fn an_enabled_user_file_replaces_the_system_one_with_the_same_bus_and_path() {
        let system = provider_file("foo.desktop", "org.foo.Search", "");
        let user = provider_file("foo-custom.desktop", "org.foo.Search", "");
        let found = merge([user.as_str(), system.as_str()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].desktop_id, "foo-custom");
    }

    #[test]
    fn unusable_files_neither_provide_nor_shadow() {
        let system = provider_file("foo.desktop", "org.foo.Search", "");
        let old = "[Shell Search Provider]\nDesktopId=foo.desktop\nBusName=org.foo.Search\nObjectPath=/org/foo/Search\nVersion=1\n";
        let broken = "[Shell Search Provider]\nBusName=org.foo.Search\n";
        assert_eq!(merge([old, broken, system.as_str()]).len(), 1);
    }

    #[test]
    fn a_provider_must_declare_version_two_or_later_as_an_integer() {
        let base =
            "[Shell Search Provider]\nDesktopId=foo.desktop\nBusName=org.foo.Search\nObjectPath=/org/foo/Search\n";
        assert!(parse_ini(base).is_none(), "GNOME Shell skips a file without a version");
        assert!(parse_ini(&format!("{base}Version=two\n")).is_none());
        assert!(parse_ini(&format!("{base}Version=1\n")).is_none());
        assert!(parse_ini(&format!("{base}Version=2\n")).is_some());
        assert!(parse_ini(&format!("{base}Version=3\n")).is_some());
    }

    #[test]
    fn app_ids_compare_with_or_without_the_desktop_suffix_on_either_side() {
        assert!(same_app("org.gnome.Nautilus.desktop", "org.gnome.nautilus"));
        assert!(same_app("org.gnome.nautilus", "org.gnome.Nautilus.desktop"));
        assert!(!same_app("org.gnome.Nautilus", "org.gnome.Calculator"));
    }

    #[test]
    fn an_app_whose_id_ends_in_desktop_matches_its_file_name_and_itself() {
        assert!(same_app("org.telegram.desktop.desktop", "org.telegram.desktop"));
        assert!(same_app("org.telegram.desktop", "org.telegram.desktop.desktop"));
        assert!(same_app("org.telegram.desktop", "org.telegram.desktop"));
        assert!(same_app("org.telegram.desktop.desktop", "org.telegram.desktop.desktop"));
        assert!(!same_app("org.telegram.desktop.desktop", "org.signal.desktop"));
    }
}
