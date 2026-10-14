use glide_files::App;

fn requested_size() -> Option<iced::Size> {
    let value = std::env::var("GLIDE_FILES_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some(iced::Size::new(width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn main() -> iced::Result {
    let target = std::env::args().nth(1).and_then(|arg| glide_files::fs::path_from_arg(&arg));
    if let glide_files::service::Claim::Forwarded = glide_files::service::claim(target.as_deref()) {
        return Ok(());
    }
    let window = iced::window::Settings {
        size: requested_size().unwrap_or(iced::Size::new(1000.0, 650.0)),
        min_size: Some(iced::Size::new(720.0, 480.0)),
        decorations: false,
        transparent: true,
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: String::from("glide-files"),
            ..iced::window::settings::PlatformSpecific::default()
        },
        ..iced::window::Settings::default()
    };

    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::iced_theme)
        .style(App::style)
        .subscription(App::subscription)
        .window(window)
        .antialiasing(true)
        .run()
}
