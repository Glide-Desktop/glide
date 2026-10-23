use glide_settings::App;

fn requested_size() -> Option<(f32, f32)> {
    let value = std::env::var("GLIDE_SETTINGS_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(glide_settings::services::netbird::APPLY_FLAG) {
        std::process::exit(glide_settings::services::netbird::apply_privileged(&args[1..]));
    }
    let config = glide_settings::services::config::ConfigStore::load();
    glide_settings::theme::use_ui_family(config.optional_str("general", "font-family").as_deref());
    let (width, height) = requested_size().unwrap_or((1000.0, 700.0));
    let settings = glide_sdk::Settings::new("glide-settings")
        .size(width, height)
        .min_size(760.0, 480.0)
        .chrome(glide_sdk::Chrome::Server);
    glide_sdk::run::<App>(settings)
}
