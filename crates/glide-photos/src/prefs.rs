use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Album {
    pub name: String,
    pub photos: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Prefs {
    pub folders: Vec<PathBuf>,
    pub albums: Vec<Album>,
    pub favourites: Vec<PathBuf>,
    pub captions: BTreeMap<PathBuf, String>,
    pub zoom: usize,
    pub dismissed_cards: Vec<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            albums: Vec::new(),
            favourites: Vec::new(),
            captions: BTreeMap::new(),
            zoom: 3,
            dismissed_cards: Vec::new(),
        }
    }
}

#[must_use]
pub fn path() -> Option<PathBuf> {
    Some(glide_config::config_home()?.join("glide/photos.toml"))
}

#[must_use]
pub fn load() -> Prefs {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_to(file: &Path, prefs: &Prefs) -> std::io::Result<()> {
    let text = toml::to_string_pretty(prefs).map_err(std::io::Error::other)?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = file.with_extension(format!("toml.{}.part", std::process::id()));
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, file)
}

impl Prefs {
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        if self.folders.is_empty() {
            crate::library::user_dir("XDG_PICTURES_DIR").into_iter().collect()
        } else {
            self.folders.clone()
        }
    }

    #[must_use]
    pub fn is_favourite(&self, path: &Path) -> bool {
        self.favourites.iter().any(|known| known == path)
    }

    pub fn set_favourite(&mut self, path: &Path, on: bool) {
        self.favourites.retain(|known| known != path);
        if on {
            self.favourites.push(path.to_path_buf());
        }
    }

    pub fn add_to_album(&mut self, album: usize, paths: &[PathBuf]) {
        if let Some(album) = self.albums.get_mut(album) {
            for path in paths {
                if !album.photos.contains(path) {
                    album.photos.push(path.clone());
                }
            }
        }
    }

    pub fn forget(&mut self, path: &Path) {
        self.favourites.retain(|known| known != path);
        self.captions.remove(path);
        for album in &mut self.albums {
            album.photos.retain(|known| known != path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_through_toml_and_keep_albums_unique() {
        let mut prefs =
            Prefs { albums: vec![Album { name: "Карпати".into(), photos: Vec::new() }], ..Prefs::default() };
        let a = PathBuf::from("/p/a.jpg");
        prefs.add_to_album(0, &[a.clone(), a.clone()]);
        prefs.set_favourite(&a, true);
        prefs.captions.insert(a.clone(), "Озеро".into());
        let text = toml::to_string_pretty(&prefs).unwrap();
        let back: Prefs = toml::from_str(&text).unwrap();
        assert_eq!(back, prefs);
        assert_eq!(back.albums[0].photos, std::slice::from_ref(&a));
        let mut back = back;
        back.forget(&a);
        assert!(back.albums[0].photos.is_empty() && back.favourites.is_empty() && back.captions.is_empty());
    }
}
