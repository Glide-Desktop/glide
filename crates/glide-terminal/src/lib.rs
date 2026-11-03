pub mod app;
mod chrome;
pub mod config;
pub mod dnd;
pub mod export;
pub mod find;
pub mod hints;
pub mod i18n;
pub mod input;
pub mod login_env;
pub mod marks;
pub mod profile;
pub mod service;
pub mod session;
pub mod sound;
pub mod terminal;

pub use app::App;

pub const MONO: iced::Font = iced::Font::with_name("JetBrains Mono");

pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../fonts/JetBrainsMono-BoldItalic.ttf"),
];
