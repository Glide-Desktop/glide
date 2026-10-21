pub mod app;
pub mod boxes;
mod chrome;
pub mod config;
pub mod dnd;
pub mod export;
pub mod find;
pub mod hints;
pub mod i18n;
mod icons;
pub mod input;
pub mod marks;
pub mod profile;
pub mod service;
pub mod session;
pub mod sound;
mod style;
pub mod terminal;
mod widgets;

pub use app::App;

pub const MONO: iced::Font = iced::Font::with_name("JetBrains Mono");

pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../fonts/JetBrainsMono-BoldItalic.ttf"),
];
