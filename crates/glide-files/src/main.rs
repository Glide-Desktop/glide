use glide_files::App;

fn requested_size() -> Option<(f32, f32)> {
    let value = std::env::var("GLIDE_FILES_SIZE").ok()?;
    let (width, height) = value.split_once('x')?;
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn main() -> iced::Result {
    let choosing = std::env::args().nth(1).is_some_and(|arg| arg == glide_app::chooser::FLAG);
    if choosing {
        let Some(request) = glide_files::chooser::Request::from_env() else {
            eprintln!("glide-files: --chooser needs a request in ${}", glide_app::chooser::REQUEST_ENV);
            std::process::exit(glide_app::chooser::FAILURE_EXIT);
        };
        glide_files::chooser::install(request);
    } else {
        let _ = glide_files::service::claim();
    }
    let (width, height) = requested_size().unwrap_or(if choosing { (900.0, 600.0) } else { (1000.0, 650.0) });
    let settings = glide_sdk::Settings::new("glide-files")
        .size(width, height)
        .min_size(720.0, 480.0)
        .chrome(glide_sdk::Chrome::Own);
    let ran = glide_sdk::run::<App>(settings);
    if choosing {
        std::process::exit(if glide_files::chooser::accepted() {
            0
        } else if ran.is_ok() {
            glide_app::chooser::CANCEL_EXIT
        } else {
            glide_app::chooser::FAILURE_EXIT
        });
    }
    ran
}
