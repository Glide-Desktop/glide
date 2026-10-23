use glide_terminal::{App, FONTS};

fn main() -> iced::Result {
    glide_terminal::service::claim();
    let settings = glide_sdk::Settings::new("glide-terminal")
        .size(760.0, 480.0)
        .min_size(420.0, 260.0)
        .fonts(FONTS)
        .chrome(glide_sdk::Chrome::Own);
    glide_sdk::run::<App>(settings)
}
