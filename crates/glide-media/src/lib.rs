mod client;
mod model;

pub use client::{MediaHandle, Sink, start};
pub use model::{
    BUS_PREFIX, Command, MediaEvent, OBJECT_PATH, PLAYER_INTERFACE, Playback, Track,
    artists_to_line, better_candidate, identity_from_bus, is_player_bus, local_art_path,
    seek_target,
};
