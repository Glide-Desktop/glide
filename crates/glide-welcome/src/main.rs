use glide_sdk::{Chrome, Settings};
use glide_welcome::ui::WelcomeApp;

fn main() -> glide_sdk::iced::Result {
    glide_sdk::run::<WelcomeApp>(
        Settings::new(glide_config::welcome::APPLICATION_ID)
            .single_instance("org.glide.Welcome")
            .size(1120.0, 760.0)
            .min_size(720.0, 480.0)
            .chrome(Chrome::Own)
            .fonts(glide_sdk::theme::MONO_FONTS),
    )
}
