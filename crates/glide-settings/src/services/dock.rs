pub use glide_config::pins::Pin;

pub fn load() -> Result<Option<Vec<Pin>>, String> {
    glide_config::pins::load().map_err(|err| err.to_string())
}

pub fn save(pins: &[Pin]) -> Result<(), String> {
    glide_config::pins::save(pins).map_err(|err| err.to_string())
}

#[must_use]
pub fn moved(pins: &[Pin], index: usize, up: bool) -> Vec<Pin> {
    let mut pins = pins.to_vec();
    let target = if up { index.checked_sub(1) } else { Some(index + 1).filter(|next| *next < pins.len()) };
    if let Some(target) = target
        && index < pins.len()
    {
        pins.swap(index, target);
    }
    pins
}

#[must_use]
pub fn same_app(a: &str, b: &str) -> bool {
    let key = |id: &str| id.trim().trim_end_matches(".desktop").to_ascii_lowercase();
    key(a) == key(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(app_id: &str) -> Pin {
        Pin { app_id: app_id.to_string(), name: app_id.to_uppercase(), command: app_id.to_lowercase() }
    }

    #[test]
    fn moving_swaps_neighbours_and_stops_at_the_ends() {
        let pins = vec![pin("a"), pin("b"), pin("c")];
        let ids = |list: Vec<Pin>| list.into_iter().map(|p| p.app_id).collect::<Vec<_>>();
        assert_eq!(ids(moved(&pins, 1, true)), ["b", "a", "c"]);
        assert_eq!(ids(moved(&pins, 1, false)), ["a", "c", "b"]);
        assert_eq!(ids(moved(&pins, 0, true)), ["a", "b", "c"]);
        assert_eq!(ids(moved(&pins, 2, false)), ["a", "b", "c"]);
    }

    #[test]
    fn desktop_ids_and_app_ids_compare_loosely() {
        assert!(same_app("org.gnome.Nautilus.desktop", "org.gnome.nautilus"));
        assert!(!same_app("firefox", "firefox-esr"));
    }
}
