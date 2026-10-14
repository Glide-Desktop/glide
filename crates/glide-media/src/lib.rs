mod art;
mod client;
mod model;

pub use art::{ART_MAX_BYTES, is_remote_art};
pub use client::{MediaHandle, Sink, start};
pub use model::{
    BUS_PREFIX, Command, MediaEvent, OBJECT_PATH, PLAYER_INTERFACE, Playback, Track, artists_to_line, better_candidate,
    identity_from_bus, is_player_bus, local_art_path, player_icon_candidates, seek_target,
};
