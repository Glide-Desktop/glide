use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct HungPlayer;

#[allow(clippy::unused_self)] // zbus requires `&self` on interface properties even when stateless.
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl HungPlayer {
    #[zbus(property)]
    fn metadata(&self) -> std::collections::HashMap<String, zbus::zvariant::OwnedValue> {
        std::thread::sleep(Duration::from_secs(10));
        std::collections::HashMap::new()
    }
}

#[test]
fn a_hung_player_never_blocks_the_calling_thread() {
    if zbus::blocking::Connection::session().is_err() {
        eprintln!("no session bus; skipping");
        return;
    }

    let bus_name = format!("org.mpris.MediaPlayer2.glidehung{}", std::process::id());
    let _player = zbus::blocking::connection::Builder::session()
        .expect("session bus")
        .name(bus_name.as_str())
        .expect("a free name")
        .serve_at("/org/mpris/MediaPlayer2", HungPlayer)
        .expect("serve")
        .build()
        .expect("player connection");

    let handle = glide_media::start(Arc::new(|_| {})).expect("the media client starts");
    std::thread::sleep(Duration::from_millis(200));

    let began = Instant::now();
    for step in 0..50u8 {
        handle.seek(f32::from(step) / 50.0);
        handle.send(glide_media::Command::PlayPause);
    }
    assert!(
        began.elapsed() < Duration::from_millis(100),
        "seek and send must only enqueue work, took {:?}",
        began.elapsed()
    );
}
