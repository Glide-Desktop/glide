use glide_settings::App;

fn requested_size() -> Option<iced::Size> {
    let value = std::env::var("GLIDE_SETTINGS_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some(iced::Size::new(width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn main() -> iced::Result {
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
