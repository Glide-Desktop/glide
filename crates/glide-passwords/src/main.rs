use glide_passwords::{APP_ID, App};

fn main() -> iced::Result {
    let settings = glide_sdk::Settings::new(APP_ID)
        .size(1280.0, 800.0)
        .min_size(900.0, 560.0)
        .fonts(glide_sdk::theme::MONO_FONTS)
        .chrome(glide_sdk::Chrome::Own);
    glide_sdk::run::<App>(settings)
}
