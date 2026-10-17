use glide_settings::App;

fn requested_size() -> Option<iced::Size> {
    let value = std::env::var("GLIDE_SETTINGS_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some(iced::Size::new(width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some(glide_settings::services::netbird::APPLY_FLAG) {
        std::process::exit(glide_settings::services::netbird::apply_privileged(&args[1..]));
    }
    let config = glide_settings::services::config::ConfigStore::load();
    glide_settings::theme::use_ui_family(config.optional_str("general", "font-family").as_deref());
    let window = iced::window::Settings {
        size: requested_size().unwrap_or(iced::Size::new(1000.0, 700.0)),
        min_size: Some(iced::Size::new(760.0, 480.0)),
        decorations: true,
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: String::from("glide-settings"),
            ..iced::window::settings::PlatformSpecific::default()
        },
        ..iced::window::Settings::default()
    };

    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::iced_theme)
        .subscription(App::subscription)
        .window(window)
        .antialiasing(true)
        .run()
}
