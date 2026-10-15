use std::time::{Duration, SystemTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("installed: {}", glide_netbird::installed());
    let daemon = glide_netbird::Daemon::connect()?;
    println!("compatible: {:?}", daemon.compatible());
    let status = daemon.status()?;
    println!(
        "state: {:?}, daemon {}, peers {}/{}, management {}, signal {}, relays {}, revision {}",
        status.state,
        status.daemon_version,
        status.connected_peers(),
        status.peers.len(),
        status.management.connected,
        status.signal.connected,
        status.relays.len(),
        status.networks_revision,
    );
    if let Some(deadline) = status.session_expires_at {
        println!("session: {:?}", glide_netbird::remaining_until(deadline, SystemTime::now()));
    }
    let profiles = daemon.profiles()?;
    println!(
        "profiles: {}, active: {:?}",
        profiles.len(),
        profiles.iter().find(|profile| profile.active).map(|p| &p.id)
    );
    println!("active profile: {:?}", daemon.active_profile()?.id);
    println!("config: {:?}", daemon.config()?);
    println!("features: {:?}", daemon.features()?);
    let networks = daemon.networks()?;
    println!(
        "networks: {}, selected {}, exit nodes {}",
        networks.len(),
        networks.iter().filter(|network| network.selected).count(),
        glide_netbird::exit_nodes(&networks).len()
    );
    println!("log level: {:?}", daemon.log_level()?);
    println!("events: {}", daemon.events()?.len());
    println!("forwarding rules: {}", daemon.forwarding_rules()?.len());
    let (sender, receiver) = std::sync::mpsc::channel();
    let watch = daemon.watch(move |feed| {
        let _ = sender.send(match feed {
            glide_netbird::Feed::Status(status) => format!("stream status {:?}", status.state),
            glide_netbird::Feed::Event(event) => format!("stream event {event:?}"),
        });
    })?;
    while let Ok(line) = receiver.recv_timeout(Duration::from_secs(3)) {
        println!("{}", line.chars().take(120).collect::<String>());
    }
    drop(watch);
    Ok(())
}
