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
pub mod syntax;
pub mod tree;

pub use app::App;

pub const MONO: iced::Font = iced::Font::with_name("JetBrains Mono");

pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../../glide-terminal/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../glide-terminal/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../glide-terminal/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../glide-terminal/fonts/JetBrainsMono-BoldItalic.ttf"),
];
