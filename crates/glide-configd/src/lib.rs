pub mod bus;
pub mod store;

use std::path::PathBuf;

#[must_use]
pub fn defaults_dirs() -> Vec<PathBuf> {
    let extra = std::env::var_os("GLIDE_DEFAULTS_DIR").map(PathBuf::from);
    extra.into_iter().chain(glide_config::data_dirs().into_iter().map(|dir| dir.join("glide/defaults"))).collect()
}

#[must_use]
pub fn database_path() -> Option<PathBuf> {
    glide_config::xdg_path("glide", "settings.db")
}
