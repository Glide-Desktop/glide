pub mod app;
mod chrome;
pub mod config;
pub mod detect;
pub mod document;
pub mod edit;
pub mod find;
pub mod finder;
pub mod history;
pub mod i18n;
mod icons;
pub mod syntax;
pub mod table;
pub mod tree;

pub use app::App;

pub const MONO: iced::Font = iced::Font::with_name("JetBrains Mono");

pub const FONTS: [&[u8]; 4] = glide_sdk::theme::MONO_FAMILY;
