use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use smithay::reexports::rustix::event::{PollFd, PollFlags, poll};
use smithay::reexports::rustix::fs::inotify;
use smithay::reexports::rustix::io::Errno;
use smithay::reexports::udev;
use zbus::blocking::{Connection, MessageIterator, fdo::DBusProxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::system_info::{self, Privacy, Topics};

const RETRY_FIRST: Duration = Duration::from_secs(2);
const RETRY_LAST: Duration = Duration::from_mins(1);
const HEALTHY_RUN: Duration = Duration::from_secs(30);

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const NETWORK_MANAGER: &str = "org.freedesktop.NetworkManager";
const BLUEZ: &str = "org.bluez";
const POWER_PROFILES: [&str; 2] = ["net.hadess.PowerProfiles", "org.freedesktop.UPower.PowerProfiles"];
const NETWORK_NOISE: [&str; 4] = ["Bitrate", "LastSeen", "LastScan", "AccessPoints"];
const DCONF_RULE: &str = "type='signal',interface='ca.desrt.dconf.Writer',member='Notify'";
const ANIMATIONS_KEY: &str = "/org/gnome/desktop/interface/enable-animations";
const PIPEWIRE_TRACKED: [&str; 2] = ["PipeWire:Interface:Node", "PipeWire:Interface:Link"];

pub fn start() {
    watch_child("glide-hook-pactl", Topics::AUDIO, &["pactl", "subscribe"], read_pactl);
    watch_child("glide-hook-pipewire", Topics::PRIVACY, &["pw-dump", "--monitor", "--no-colors"], read_pipewire);
    watch_kernel();
    watch_system_bus();
    watch_session_bus();
}

enum End {
    Retry,
    GiveUp,
}

#[must_use]
pub fn next_delay(previous: Duration, ran: Duration) -> Duration {
    if ran >= HEALTHY_RUN { RETRY_FIRST } else { (previous * 2).clamp(RETRY_FIRST, RETRY_LAST) }
}

fn keep_running(name: &str, mut body: impl FnMut() -> End + Send + 'static) {
    let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || {
        let mut delay = Duration::ZERO;
        loop {
            let started = Instant::now();
            if let End::GiveUp = body() {
                return;
            }
            delay = next_delay(delay, started.elapsed());
            std::thread::sleep(delay);
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("{name} did not start: {err}");
    }
}

fn watch_child(name: &str, topics: Topics, argv: &'static [&'static str], read: fn(ChildStdout)) {
    keep_running(name, move || {
        let mut command = Command::new(argv[0]);
        command.args(&argv[1..]).env("LC_ALL", "C").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        // SAFETY: the closure runs in the forked child before exec and only calls prctl, which is
        // async-signal-safe and touches no memory shared with the parent.
        unsafe {
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                tracing::info!("{} is not installed; {topics:?} stays polled", argv[0]);
                return End::GiveUp;
            }
            Err(err) => {
                tracing::warn!("{} did not start ({err}); {topics:?} stays polled", argv[0]);
                return End::Retry;
            }
        };
        let started = Instant::now();
        if let Some(stdout) = child.stdout.take() {
            system_info::hook(topics);
            read(stdout);
            system_info::unhook(topics);
        }
        let _ = child.kill();
        let _ = child.wait();
        if started.elapsed() >= HEALTHY_RUN {
            tracing::info!("{} ended; {topics:?} is polled until it is back", argv[0]);
        } else {
            tracing::debug!("{} ended at once; {topics:?} stays polled", argv[0]);
        }
        End::Retry
    });
}

#[must_use]
pub fn touches_levels(line: &str) -> bool {
    [" on sink #", " on source #", " on server", " on card #"].into_iter().any(|kind| line.contains(kind))
}

fn read_pactl(stdout: ChildStdout) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if touches_levels(&line) {
            system_info::poke(Topics::AUDIO);
        }
    }
}

#[derive(Debug, Default)]
pub struct Graph {
    objects: HashMap<u64, serde_json::Value>,
}

impl Graph {
    pub fn apply(&mut self, update: serde_json::Value) -> bool {
        let serde_json::Value::Array(objects) = update else {
            return false;
        };
        let mut changed = false;
        for mut object in objects {
            let Some(id) = object.get("id").and_then(serde_json::Value::as_u64) else {
                continue;
            };
            if let Some(info) = object.get_mut("info").and_then(serde_json::Value::as_object_mut) {
                info.remove("params");
            }
            if object.get("info").is_none_or(serde_json::Value::is_null) {
                changed |= self.objects.remove(&id).is_some();
                continue;
            }
            let kind = object.get("type").and_then(serde_json::Value::as_str).unwrap_or_default();
            if PIPEWIRE_TRACKED.contains(&kind) {
                self.objects.insert(id, object);
                changed = true;
            }
        }
        changed
    }

    #[must_use]
    pub fn privacy(&self) -> Privacy {
        system_info::privacy_from(self.objects.values())
    }
}

fn read_pipewire(stdout: ChildStdout) {
    let mut graph = Graph::default();
    let mut shown: Option<Privacy> = None;
    let updates = serde_json::Deserializer::from_reader(BufReader::new(stdout)).into_iter::<serde_json::Value>();
    for update in updates.map_while(Result::ok) {
        if !graph.apply(update) && shown.is_some() {
            continue;
        }
        let privacy = graph.privacy();
        if shown.as_ref() != Some(&privacy) {
            system_info::publish_pipewire(privacy.clone());
            shown = Some(privacy);
        }
    }
}

#[must_use]
pub fn subsystem_topics(subsystem: &str) -> Topics {
    match subsystem {
        "power_supply" => Topics::BATTERY,
        "backlight" | "leds" => Topics::BACKLIGHT,
        "video4linux" => Topics::CAMERA,
        "bluetooth" => Topics::BLUETOOTH,
        _ => Topics::empty(),
    }
}

const fn rearms(subsystem_watched: bool, kind: udev::EventType) -> bool {
    subsystem_watched && matches!(kind, udev::EventType::Add | udev::EventType::Remove)
}

fn kernel_monitor() -> std::io::Result<udev::MonitorSocket> {
    ["power_supply", "backlight", "leds", "video4linux", "bluetooth"]
        .into_iter()
        .try_fold(udev::MonitorBuilder::new()?, udev::MonitorBuilder::match_subsystem)?
        .listen()
}

#[must_use]
pub fn video_nodes(dev: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dev) else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("video").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|entry| entry.path())
        .collect();
    nodes.sort();
    nodes
}

#[must_use]
pub fn keyboard_backlight_files(leds: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(leds) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("::kbd_backlight"))
        .flat_map(|entry| ["brightness", "brightness_hw_changed"].map(|file| entry.path().join(file)))
        .filter(|path| path.exists())
        .collect();
    files.sort();
    files
}

struct Watches {
    fd: OwnedFd,
    topics: HashMap<i32, Topics>,
}

impl Watches {
    fn new() -> std::io::Result<Self> {
        let fd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)?;
        let mut watches = Self { fd, topics: HashMap::new() };
        watches.arm();
        Ok(watches)
    }

    fn arm(&mut self) {
        for (wd, _) in self.topics.drain() {
            let _ = inotify::remove_watch(&self.fd, wd);
        }
        let camera = inotify::WatchFlags::OPEN | inotify::WatchFlags::CLOSE_WRITE | inotify::WatchFlags::CLOSE_NOWRITE;
        for node in video_nodes(Path::new("/dev")) {
            self.add(&node, camera, Topics::CAMERA);
        }
        for file in keyboard_backlight_files(Path::new("/sys/class/leds")) {
            self.add(&file, inotify::WatchFlags::MODIFY, Topics::BACKLIGHT);
        }
    }

    fn add(&mut self, path: &Path, flags: inotify::WatchFlags, topics: Topics) {
        match inotify::add_watch(&self.fd, path, flags) {
            Ok(wd) => {
                self.topics.insert(wd, topics);
            }
            Err(err) => tracing::debug!("{} is not watched: {err}", path.display()),
        }
    }

    fn drain(&self) -> Topics {
        let mut buffer = [MaybeUninit::<u8>::uninit(); 4096];
        let mut reader = inotify::Reader::new(&self.fd, &mut buffer);
        let mut topics = Topics::empty();
        while let Ok(event) = reader.next() {
            topics |= self.topics.get(&event.wd()).copied().unwrap_or_default();
        }
        topics
    }
}

fn open_rfkill() -> Option<File> {
    std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open("/dev/rfkill").ok()
}

fn drain_rfkill(mut file: &File) -> bool {
    let mut event = [0u8; 64];
    let mut any = false;
    while file.read(&mut event).is_ok_and(|read| read > 0) {
        any = true;
    }
    any
}

fn pump_kernel(monitor: &udev::MonitorSocket, watches: &mut Watches, rfkill: Option<&File>) -> std::io::Result<()> {
    loop {
        let mut fds = vec![PollFd::new(monitor, PollFlags::IN), PollFd::new(&watches.fd, PollFlags::IN)];
        fds.extend(rfkill.map(|file| PollFd::new(file, PollFlags::IN)));
        match poll(&mut fds, None) {
            Ok(_) => {}
            Err(Errno::INTR) => continue,
            Err(err) => return Err(err.into()),
        }
        let ready: Vec<bool> = fds.iter().map(|fd| !fd.revents().is_empty()).collect();
        drop(fds);
        let mut topics = Topics::empty();
        if ready[0] {
            let mut rearm = false;
            for event in monitor.iter() {
                let subsystem = event.subsystem().and_then(OsStr::to_str).unwrap_or_default();
                topics |= subsystem_topics(subsystem);
                rearm |= rearms(matches!(subsystem, "leds" | "video4linux"), event.event_type());
            }
            if rearm {
                watches.arm();
            }
        }
        if ready[1] {
            topics |= watches.drain();
        }
        if ready.get(2) == Some(&true)
            && let Some(file) = rfkill
            && drain_rfkill(file)
        {
            topics |= Topics::RADIOS;
        }
        if !topics.is_empty() {
            system_info::poke(topics);
        }
    }
}

fn watch_kernel() {
    keep_running("glide-hook-kernel", || {
        let (monitor, mut watches) = match kernel_monitor().and_then(|monitor| Ok((monitor, Watches::new()?))) {
            Ok(sources) => sources,
            Err(err) => {
                tracing::info!("kernel events are unavailable ({err}); battery, backlight and camera stay polled");
                return End::GiveUp;
            }
        };
        let rfkill = open_rfkill();
        let mut hooked = Topics::BATTERY | Topics::BACKLIGHT | Topics::CAMERA;
        if rfkill.is_some() {
            hooked |= Topics::RADIOS;
        }
        system_info::hook(hooked);
        let ended = pump_kernel(&monitor, &mut watches, rfkill.as_ref());
        system_info::unhook(hooked);
        if let Err(err) = ended {
            tracing::warn!("kernel events stopped ({err}); {hooked:?} is polled until they are back");
        }
        End::Retry
    });
}

#[must_use]
pub fn name_topics(name: &str) -> Topics {
    match name {
        NETWORK_MANAGER => Topics::NETWORK,
        BLUEZ => Topics::BLUETOOTH,
        name if POWER_PROFILES.contains(&name) => Topics::POWER,
        _ => Topics::empty(),
    }
}

fn network_interface(interface: &str) -> Topics {
    if interface.starts_with(NETWORK_MANAGER) && !interface.ends_with(".Statistics") {
        Topics::NETWORK
    } else {
        Topics::empty()
    }
}

#[must_use]
pub fn property_topics(interface: &str, keys: &[&str]) -> Topics {
    let has = |key: &str| keys.contains(&key);
    match interface {
        "org.bluez.Adapter1" if has("Powered") => Topics::BLUETOOTH,
        "org.bluez.Device1" if has("Connected") => Topics::BLUETOOTH,
        name if POWER_PROFILES.contains(&name) => {
            if has("ActiveProfile") {
                Topics::POWER
            } else {
                Topics::empty()
            }
        }
        _ if !keys.is_empty() && keys.iter().all(|key| NETWORK_NOISE.contains(key)) => Topics::empty(),
        name => network_interface(name),
    }
}

#[must_use]
pub fn object_topics(interfaces: &[&str]) -> Topics {
    interfaces
        .iter()
        .map(|interface| match *interface {
            "org.bluez.Adapter1" => Topics::BLUETOOTH,
            other => network_interface(other),
        })
        .collect()
}

fn system_rules() -> Vec<String> {
    let mut rules = vec![
        format!("type='signal',sender='{NETWORK_MANAGER}'"),
        format!("type='signal',sender='{BLUEZ}',interface='{PROPERTIES}',member='PropertiesChanged'"),
        format!("type='signal',sender='{BLUEZ}',interface='org.freedesktop.DBus.ObjectManager'"),
    ];
    for name in POWER_PROFILES {
        rules.push(format!("type='signal',sender='{name}',interface='{PROPERTIES}',member='PropertiesChanged'"));
    }
    for name in [NETWORK_MANAGER, BLUEZ].into_iter().chain(POWER_PROFILES) {
        rules.push(format!(
            "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',\
             member='NameOwnerChanged',arg0='{name}'"
        ));
    }
    rules
}

fn listen(connection: &Connection, rules: &[String]) -> zbus::Result<DBusProxy<'static>> {
    let bus = DBusProxy::new(connection)?;
    for rule in rules {
        bus.add_match_rule(zbus::MatchRule::try_from(rule.as_str())?)?;
    }
    Ok(bus)
}

fn system_signal(message: &zbus::Message) -> Topics {
    type Added = (OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>);
    let header = message.header();
    let interface = header.interface().map(|name| name.as_str().to_string()).unwrap_or_default();
    let member = header.member().map(|member| member.as_str().to_string()).unwrap_or_default();
    let body = message.body();
    match member.as_str() {
        "NameOwnerChanged" => {
            let Ok((name, _, owner)) = body.deserialize::<(String, String, String)>() else {
                return Topics::empty();
            };
            if name == NETWORK_MANAGER {
                if owner.is_empty() {
                    system_info::unhook(Topics::NETWORK);
                } else {
                    system_info::hook(Topics::NETWORK);
                }
            }
            name_topics(&name)
        }
        "PropertiesChanged" if interface == PROPERTIES => {
            let Ok((name, changed, invalidated)) =
                body.deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                return Topics::empty();
            };
            let keys: Vec<&str> =
                changed.keys().map(String::as_str).chain(invalidated.iter().map(String::as_str)).collect();
            property_topics(&name, &keys)
        }
        "InterfacesAdded" => body
            .deserialize::<Added>()
            .map_or(Topics::empty(), |(_, added)| object_topics(&added.keys().map(String::as_str).collect::<Vec<_>>())),
        "InterfacesRemoved" => body
            .deserialize::<(OwnedObjectPath, Vec<String>)>()
            .map_or(Topics::empty(), |(_, gone)| object_topics(&gone.iter().map(String::as_str).collect::<Vec<_>>())),
        _ => network_interface(&interface),
    }
}

fn watch_system_bus() {
    keep_running("glide-hook-system-bus", || {
        let rules = system_rules();
        let connected = Connection::system().and_then(|connection| {
            let bus = listen(&connection, &rules)?;
            let network = zbus::names::BusName::try_from(NETWORK_MANAGER)
                .ok()
                .and_then(|name| bus.name_has_owner(name).ok())
                .unwrap_or(false);
            Ok((connection, network))
        });
        let (connection, network) = match connected {
            Ok(connected) => connected,
            Err(err) => {
                tracing::info!("system bus signals are unavailable ({err}); network, Bluetooth and power stay polled");
                return End::Retry;
            }
        };
        system_info::hook(Topics::BLUETOOTH | Topics::POWER);
        if network {
            system_info::hook(Topics::NETWORK);
        }
        for message in MessageIterator::from(&connection).map_while(Result::ok) {
            let topics = system_signal(&message);
            if !topics.is_empty() {
                system_info::poke(topics);
            }
        }
        system_info::unhook(Topics::BLUETOOTH | Topics::POWER | Topics::NETWORK);
        tracing::info!("the system bus connection ended; network, Bluetooth and power are polled until it is back");
        End::Retry
    });
}

#[must_use]
pub fn touches_animations(prefix: &str, keys: &[String]) -> bool {
    let touches = |path: &str| ANIMATIONS_KEY.starts_with(path);
    if keys.is_empty() { touches(prefix) } else { keys.iter().any(|key| touches(&format!("{prefix}{key}"))) }
}

fn watch_session_bus() {
    keep_running("glide-hook-session-bus", || {
        let connection = match Connection::session().and_then(|connection| {
            listen(&connection, &[DCONF_RULE.to_string()])?;
            Ok(connection)
        }) {
            Ok(connection) => connection,
            Err(err) => {
                tracing::info!("dconf signals are unavailable ({err}); reduced motion stays polled");
                return End::Retry;
            }
        };
        system_info::hook(Topics::MOTION);
        for message in MessageIterator::from(&connection).map_while(Result::ok) {
            let notify = message.body().deserialize::<(String, Vec<String>, String)>();
            if notify.is_ok_and(|(prefix, keys, _)| touches_animations(&prefix, &keys)) {
                system_info::poke(Topics::MOTION);
            }
        }
        system_info::unhook(Topics::MOTION);
        End::Retry
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watcher_that_keeps_dying_backs_off_and_a_healthy_one_retries_soon() {
        let quick = Duration::from_millis(50);
        let mut delay = next_delay(Duration::ZERO, quick);
        assert_eq!(delay, RETRY_FIRST);
        for _ in 0..10 {
            delay = next_delay(delay, quick);
        }
        assert_eq!(delay, RETRY_LAST);
        assert_eq!(next_delay(delay, HEALTHY_RUN), RETRY_FIRST);
    }

    #[test]
    fn only_device_and_server_events_reread_the_levels() {
        assert!(touches_levels("Event 'change' on sink #56"));
        assert!(touches_levels("Event 'change' on source #57"));
        assert!(touches_levels("Event 'change' on server #-1"));
        assert!(touches_levels("Event 'new' on card #45"));
        assert!(!touches_levels("Event 'new' on source-output #90"), "a recording app is privacy, not a level");
        assert!(!touches_levels("Event 'new' on client #18738"), "our own wpctl reads must not wake us");
        assert!(!touches_levels("Event 'change' on sink-input #12"));
    }

    fn node(id: u64, class: &str, state: &str, app: &str) -> serde_json::Value {
        serde_json::json!({"id": id, "type": "PipeWire:Interface:Node",
            "info": {"change-mask": ["state"], "state": state,
                     "props": {"media.class": class, "application.name": app}}})
    }

    #[test]
    fn the_pipewire_graph_follows_monitor_updates_and_removals() {
        let mut graph = Graph::default();
        let initial = serde_json::json!([
            node(40, "Stream/Input/Audio", "suspended", "Firefox"),
            {"id": 3, "type": "PipeWire:Interface:Client", "info": {"props": {}}}
        ]);
        assert!(graph.apply(initial));
        assert_eq!(graph.privacy(), Privacy::default());

        assert!(graph.apply(serde_json::json!([node(40, "Stream/Input/Audio", "running", "Firefox")])));
        assert_eq!(graph.privacy().microphone, vec!["Firefox".to_string()]);

        let client = serde_json::json!([{"id": 3, "type": "PipeWire:Interface:Client", "info": {"props": {"x": 1}}}]);
        assert!(!graph.apply(client), "clients come and go with every wpctl call and are not tracked");
        assert!(!graph.apply(serde_json::json!([{"id": 3, "info": null}])));

        assert!(graph.apply(serde_json::json!([{"id": 40, "info": null}])));
        assert_eq!(graph.privacy(), Privacy::default(), "a stream that goes away stops counting");
        assert!(!graph.apply(serde_json::json!({"not": "an update"})));
    }

    #[test]
    fn kernel_subsystems_map_to_the_topics_they_change() {
        assert_eq!(subsystem_topics("power_supply"), Topics::BATTERY);
        assert_eq!(subsystem_topics("backlight"), Topics::BACKLIGHT);
        assert_eq!(subsystem_topics("video4linux"), Topics::CAMERA);
        assert_eq!(subsystem_topics("bluetooth"), Topics::BLUETOOTH);
        assert_eq!(subsystem_topics("input"), Topics::empty());
        assert!(rearms(true, udev::EventType::Add) && rearms(true, udev::EventType::Remove));
        assert!(!rearms(true, udev::EventType::Change), "a change keeps the same device node");
        assert!(!rearms(false, udev::EventType::Add));
    }

    #[test]
    fn video_nodes_and_keyboard_backlights_are_found_by_name() {
        let root = std::env::temp_dir().join(format!("glide-hooks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dev = root.join("dev");
        std::fs::create_dir_all(&dev).expect("scratch dev");
        for name in ["video0", "video12", "videodev", "video", "media0"] {
            std::fs::write(dev.join(name), "").expect("node");
        }
        assert_eq!(video_nodes(&dev), [dev.join("video0"), dev.join("video12")]);

        let leds = root.join("leds");
        for name in ["input3::capslock", "tpacpi::kbd_backlight"] {
            std::fs::create_dir_all(leds.join(name)).expect("led");
            std::fs::write(leds.join(name).join("brightness"), "1").expect("brightness");
        }
        let backlight = leds.join("tpacpi::kbd_backlight");
        assert_eq!(keyboard_backlight_files(&leds), [backlight.join("brightness")]);
        std::fs::write(backlight.join("brightness_hw_changed"), "1").expect("hw");
        assert_eq!(keyboard_backlight_files(&leds).len(), 2, "firmware changes are reported there");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn bus_signals_wake_only_the_topic_they_change() {
        assert_eq!(property_topics("org.freedesktop.NetworkManager", &["State"]), Topics::NETWORK);
        assert_eq!(property_topics("org.freedesktop.NetworkManager.AccessPoint", &["Strength"]), Topics::NETWORK);
        assert_eq!(
            property_topics("org.freedesktop.NetworkManager.Device.Statistics", &["RxBytes"]),
            Topics::empty(),
            "traffic counters tick every second when a client asks for them"
        );
        let wireless = "org.freedesktop.NetworkManager.Device.Wireless";
        assert_eq!(property_topics(wireless, &["Bitrate"]), Topics::empty(), "the link rate is not on the bar");
        assert_eq!(property_topics(wireless, &["Bitrate", "ActiveAccessPoint"]), Topics::NETWORK);
        assert_eq!(property_topics(wireless, &[]), Topics::NETWORK, "an invalidation is read again");
        assert_eq!(property_topics("org.bluez.Adapter1", &["Powered"]), Topics::BLUETOOTH);
        assert_eq!(property_topics("org.bluez.Device1", &["RSSI"]), Topics::empty(), "discovery floods RSSI");
        assert_eq!(property_topics("org.bluez.Device1", &["Connected"]), Topics::BLUETOOTH);
        assert_eq!(property_topics("net.hadess.PowerProfiles", &["ActiveProfile"]), Topics::POWER);
        assert_eq!(property_topics("net.hadess.PowerProfiles", &["PerformanceDegraded"]), Topics::empty());
        assert_eq!(object_topics(&["org.bluez.Device1"]), Topics::empty(), "a discovered device is not connected");
        assert_eq!(object_topics(&["org.bluez.Adapter1", "org.freedesktop.NetworkManager.AccessPoint"]), {
            Topics::BLUETOOTH | Topics::NETWORK
        });
        assert_eq!(name_topics("org.freedesktop.UPower.PowerProfiles"), Topics::POWER);
        assert_eq!(name_topics("org.bluez"), Topics::BLUETOOTH);
        assert_eq!(network_interface("org.freedesktop.NetworkManager.Device"), Topics::NETWORK);
        for rule in system_rules() {
            assert!(zbus::MatchRule::try_from(rule.as_str()).is_ok(), "{rule}");
        }
        assert!(zbus::MatchRule::try_from(DCONF_RULE).is_ok());
    }

    #[test]
    fn only_dconf_writes_that_reach_enable_animations_reread_motion() {
        let keys = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(touches_animations("/org/gnome/desktop/interface/", &keys(&["enable-animations"])));
        assert!(touches_animations("/org/gnome/desktop/interface/enable-animations", &keys(&[""])));
        assert!(touches_animations("/org/gnome/", &keys(&[])), "a reset of a whole directory");
        assert!(!touches_animations("/org/gnome/desktop/interface/", &keys(&["color-scheme"])), "Glide writes it");
        assert!(!touches_animations("/org/gnome/desktop/background/", &keys(&["picture-uri"])));
    }
}
