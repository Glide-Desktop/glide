use std::path::PathBuf;

mod keyboard;
pub mod terminal;

pub use keyboard::SystemKeyboard;

fn var(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn user_dir(xdg: Option<PathBuf>, home: Option<PathBuf>, fallback: &str) -> Option<PathBuf> {
    xdg.or_else(|| home.map(|home| home.join(fallback)))
}

#[must_use]
pub fn home() -> Option<PathBuf> {
    var("HOME")
}

#[must_use]
pub fn config_home() -> Option<PathBuf> {
    user_dir(var("XDG_CONFIG_HOME"), home(), ".config")
}

#[must_use]
pub fn data_home() -> Option<PathBuf> {
    user_dir(var("XDG_DATA_HOME"), home(), ".local/share")
}

#[must_use]
pub fn cache_home() -> Option<PathBuf> {
    user_dir(var("XDG_CACHE_HOME"), home(), ".cache")
}

#[must_use]
pub fn state_home() -> Option<PathBuf> {
    user_dir(var("XDG_STATE_HOME"), home(), ".local/state")
}

fn search_path(key: &str, default: &str) -> Vec<PathBuf> {
    let list = std::env::var_os(key).filter(|value| !value.is_empty()).unwrap_or_else(|| default.into());
    std::env::split_paths(&list).collect()
}

#[must_use]
pub fn data_dirs() -> Vec<PathBuf> {
    data_home().into_iter().chain(search_path("XDG_DATA_DIRS", "/usr/local/share:/usr/share")).collect()
}

#[must_use]
pub fn system_config_dirs() -> Vec<PathBuf> {
    search_path("XDG_CONFIG_DIRS", "/etc/xdg")
}

#[must_use]
pub fn config_dirs() -> Vec<PathBuf> {
    config_home().into_iter().chain(system_config_dirs()).collect()
}

#[must_use]
pub fn xdg_path(component: &str, file_name: &str) -> Option<PathBuf> {
    Some(config_home()?.join(component).join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_wins_over_home_and_empty_values_fall_through() {
        let home = Some(PathBuf::from("/home/me"));
        assert_eq!(user_dir(Some("/xdg".into()), home.clone(), ".config"), Some(PathBuf::from("/xdg")));
        assert_eq!(user_dir(None, home, ".config"), Some(PathBuf::from("/home/me/.config")));
        assert_eq!(user_dir(None, None, ".config"), None);
    }
}
