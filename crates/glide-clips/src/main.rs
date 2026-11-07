use glide_clips::{APP_ID, App};

fn main() -> iced::Result {
    let settings = glide_sdk::Settings::new(APP_ID)
        .size(1280.0, 720.0)
        .min_size(480.0, 270.0)
        .fonts(glide_sdk::theme::MONO_FONTS)
        .single_instance("org.glide.Clips")
        .chrome(glide_sdk::Chrome::Own);
    glide_sdk::run::<App>(settings)
}
