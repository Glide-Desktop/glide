use glide_terminal::{App, FONTS};

fn main() -> iced::Result {
    let window = iced::window::Settings {
        size: iced::Size::new(760.0, 480.0),
        min_size: Some(iced::Size::new(420.0, 260.0)),
        decorations: false,
        transparent: true,
        exit_on_close_request: false,
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: String::from("glide-terminal"),
            ..iced::window::settings::PlatformSpecific::default()
        },
        ..iced::window::Settings::default()
    };

    FONTS
        .into_iter()
        .fold(iced::application(App::new, App::update, App::view), iced::Application::font)
        .title(App::title)
        .theme(App::iced_theme)
        .style(App::style)
        .subscription(App::subscription)
        .window(window)
        .antialiasing(true)
        .run()
}
