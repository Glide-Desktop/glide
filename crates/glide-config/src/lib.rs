use std::path::PathBuf;

#[must_use]
pub fn xdg_path(component: &str, file_name: &str) -> Option<PathBuf> {
    resolve_xdg_path(
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
        component,
        file_name,
    )
}

fn resolve_xdg_path(
    xdg_config_home: Option<String>,
    home: Option<String>,
    component: &str,
    file_name: &str,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join(component).join(file_name));
        }
    }
    if let Some(home) = home {
        if !home.is_empty() {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join(component)
                    .join(file_name),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_xdg_config_home_when_set() {
        let path = resolve_xdg_path(
            Some("/xdg".to_string()),
            Some("/home/artem".to_string()),
            "glide",
            "glide.toml",
        );
        assert_eq!(path, Some(PathBuf::from("/xdg/glide/glide.toml")));
    }

    #[test]
    fn falls_back_to_home_dot_config_when_xdg_is_unset() {
        let path = resolve_xdg_path(None, Some("/home/artem".to_string()), "glide", "glide.toml");
        assert_eq!(
            path,
            Some(PathBuf::from("/home/artem/.config/glide/glide.toml"))
        );
    }

    #[test]
    fn falls_back_to_home_when_xdg_is_set_but_empty() {
        let path = resolve_xdg_path(
            Some(String::new()),
            Some("/home/artem".to_string()),
            "glide",
            "glide.toml",
        );
        assert_eq!(
            path,
            Some(PathBuf::from("/home/artem/.config/glide/glide.toml"))
        );
    }

    #[test]
    fn none_when_neither_variable_is_usable() {
        assert_eq!(resolve_xdg_path(None, None, "glide", "glide.toml"), None);
        assert_eq!(
            resolve_xdg_path(
                Some(String::new()),
                Some(String::new()),
                "glide",
                "glide.toml"
            ),
            None
        );
    }

    #[test]
    fn component_and_file_name_are_both_respected() {
        let path = resolve_xdg_path(Some("/xdg".to_string()), None, "glide-panel", "panel.toml");
        assert_eq!(path, Some(PathBuf::from("/xdg/glide-panel/panel.toml")));
    }
}
