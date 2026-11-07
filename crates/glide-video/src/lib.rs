pub mod export;
pub mod frames;
pub mod info;
pub mod media;
pub mod player;
pub mod sink;
pub mod surface;

pub use gstreamer;

pub fn init() -> Result<(), String> {
    gstreamer::init().map_err(|err| err.to_string())
}
