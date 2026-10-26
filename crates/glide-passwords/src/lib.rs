pub mod app;
pub mod client;
pub mod conflict;
pub mod editor;
pub mod generator;
pub mod i18n;
pub mod icons;
pub mod model;
pub mod prefs;
pub mod security;
pub mod sends;
pub mod settings;
pub mod transfer;
pub mod view;
pub mod wizard;

pub use app::App;

pub const APP_ID: &str = "glide-passwords";
