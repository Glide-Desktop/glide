use glide_folio::{APP_ID, App};

fn main() -> std::process::ExitCode {
    if std::env::args().nth(1).as_deref() == Some(glide_folio::worker::FLAG) {
        return glide_folio::worker::run();
    }
    let settings = glide_sdk::Settings::new(APP_ID)
        .size(1280.0, 800.0)
        .min_size(720.0, 480.0)
        .fonts(glide_sdk::theme::MONO_FONTS)
        .single_instance("org.glide.Folio")
        .chrome(glide_sdk::Chrome::Own);
    match glide_sdk::run::<App>(settings) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
