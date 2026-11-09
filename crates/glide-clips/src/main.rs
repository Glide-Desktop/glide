use glide_clips::{APP_ID, App};

const EIGHT_BIT_SWAPCHAIN: &str = "vk_wsi_force_bgra8_unorm_first";

fn main() -> iced::Result {
    if std::env::var_os(EIGHT_BIT_SWAPCHAIN).is_none() {
        // SAFETY: the first statement of `main`, before any thread exists that could read the environment.
        unsafe { std::env::set_var(EIGHT_BIT_SWAPCHAIN, "true") };
    }
    let settings = glide_sdk::Settings::new(APP_ID)
        .size(1280.0, 720.0)
        .min_size(480.0, 270.0)
        .fonts(glide_sdk::theme::MONO_FONTS)
        .single_instance("org.glide.Clips")
        .chrome(glide_sdk::Chrome::Own);
    glide_sdk::run::<App>(settings)
}
