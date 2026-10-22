use glide_catalog::{APP_ID, Catalog};
use glide_sdk::Settings;

fn main() -> glide_sdk::iced::Result {
    glide_sdk::run::<Catalog>(Settings::new(APP_ID).size(1120.0, 760.0).min_size(760.0, 520.0))
}
