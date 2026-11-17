use glide_text::{App, FONTS};

fn main() -> iced::Result {
    glide_text::config::import_legacy();
    let mut settings = glide_sdk::Settings::new("glide-text")
        .size(960.0, 640.0)
        .min_size(420.0, 300.0)
        .fonts(FONTS)
        .chrome(glide_sdk::Chrome::Own);
    if !glide_text::app::parse_args(std::env::args_os().skip(1)).0 {
        settings = settings.single_instance("org.glide.Text").path_arguments();
    }
    glide_sdk::run::<App>(settings)
}
