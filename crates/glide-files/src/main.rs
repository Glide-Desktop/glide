use glide_files::App;

fn requested_size() -> Option<iced::Size> {
    let value = std::env::var("GLIDE_FILES_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some(iced::Size::new(width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn main() -> iced::Result {
    let choosing = std::env::args().nth(1).is_some_and(|arg| arg == glide_app::chooser::FLAG);
    if choosing {
        let Some(request) = glide_files::chooser::Request::from_env() else {
            eprintln!("glide-files: --chooser needs a request in ${}", glide_app::chooser::REQUEST_ENV);
            std::process::exit(2);
        };
        glide_files::chooser::install(request);
    } else {
        let target = std::env::args().nth(1).and_then(|arg| glide_files::fs::path_from_arg(&arg));
        if let glide_files::service::Claim::Forwarded = glide_files::service::claim(target.as_deref()) {
            return Ok(());
        }
    }
    glide_files::theme::use_ui_family(glide_files::config::load().font_family.as_deref());
    let default_size = if choosing { iced::Size::new(900.0, 600.0) } else { iced::Size::new(1000.0, 650.0) };
    let window = iced::window::Settings {
        size: requested_size().unwrap_or(default_size),
        min_size: Some(iced::Size::new(720.0, 480.0)),
        decorations: false,
        transparent: true,
        exit_on_close_request: false,
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: String::from("glide-files"),
            ..iced::window::settings::PlatformSpecific::default()
        },
        ..iced::window::Settings::default()
    };

    let ran = iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::iced_theme)
        .style(App::style)
        .subscription(App::subscription)
        .window(window)
        .antialiasing(true)
        .run();
    if choosing && !glide_files::chooser::accepted() {
        std::process::exit(1);
    }
    ran
}
