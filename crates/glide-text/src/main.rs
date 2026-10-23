use glide_text::{App, FONTS};

fn main() -> iced::Result {
    let settings = glide_sdk::Settings::new("glide-text")
        .size(960.0, 640.0)
        .min_size(420.0, 300.0)
        .fonts(FONTS)
        .chrome(glide_sdk::Chrome::Own);
    glide_sdk::run::<App>(settings)
}
