use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError, mpsc},
};

use glide_app::{BUS_NAME, INTERFACE, PATH};
use zbus::{
    blocking::{Connection, connection::Builder},
    fdo::{RequestNameFlags, RequestNameReply},
    message::Header,
    names::BusName,
    zvariant::Value,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub appearance: glide_theme::Appearance,
    pub accent: glide_theme::Accent,
    pub language: &'static str,
    pub animation_scale: f64,
    pub reduced_motion: bool,
    pub font_family: String,
    pub icon_theme: String,
    pub locked: bool,
    pub active_workspace: u32,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            appearance: glide_theme::Appearance::default(),
            accent: glide_theme::Accent::default(),
            language: "en",
            animation_scale: 1.0,
            reduced_motion: false,
            font_family: String::new(),
            icon_theme: String::new(),
            locked: false,
            active_workspace: 0,
        }
    }
}

impl Snapshot {
    fn properties(&self) -> Vec<(&'static str, Value<'static>)> {
        vec![
            ("Appearance", Value::from(self.appearance.key())),
            ("Accent", Value::from(self.accent.key())),
            ("Language", Value::from(self.language)),
            ("AnimationScale", Value::from(self.animation_scale)),
            ("ReducedMotion", Value::from(self.reduced_motion)),
            ("FontFamily", Value::from(self.font_family.clone())),
            ("IconTheme", Value::from(self.icon_theme.clone())),
            ("Locked", Value::from(self.locked)),
            ("ActiveWorkspace", Value::from(self.active_workspace)),
            ("Tokens", Value::from(self.tokens())),
        ]
    }

    #[must_use]
    pub fn tokens(&self) -> String {
        glide_theme::data::json(&glide_theme::Tokens::new(self.appearance, self.accent))
    }

    #[must_use]
    pub fn changed_since(&self, before: &Self) -> Vec<(&'static str, Value<'static>)> {
        let old = before.properties();
        self.properties().into_iter().zip(old).filter(|(new, old)| new.1 != old.1).map(|(new, _)| new).collect()
    }
}

pub enum Event {
    Token { pid: i32, reply: crate::oneshot::Sender<Option<String>> },
    Frame { pid: i32 },
    Lock,
    Osd(crate::osd::Reading),
    QuickSettings(Option<crate::topbar::Detail>),
    CopySecret { pid: i32, secret: zeroize::Zeroizing<String>, clear_after: u32, reply: crate::oneshot::Sender<bool> },
    Traits { pid: i32, change: crate::window_traits::Change },
}

#[must_use]
pub fn osd_kind(kind: &str) -> Option<crate::osd::Kind> {
    use crate::osd::Kind;
    match kind {
        "volume" => Some(Kind::Volume),
        "brightness" => Some(Kind::Brightness),
        "keyboard-brightness" => Some(Kind::KeyboardBrightness),
        "caps-lock" => Some(Kind::CapsLock),
        _ => None,
    }
}

pub fn quick_settings_view(view: &str) -> Result<Option<crate::topbar::Detail>, ()> {
    use crate::topbar::Detail;
    match view {
        "" => Ok(None),
        "wifi" => Ok(Some(Detail::Wifi)),
        "bluetooth" => Ok(Some(Detail::Bluetooth)),
        "sound" => Ok(Some(Detail::Sound)),
        "battery" => Ok(Some(Detail::Battery)),
        "netbird" => Ok(Some(Detail::Netbird)),
        _ => Err(()),
    }
}

type Sink = Arc<dyn Fn(Event) + Send + Sync>;

async fn sender_pid(header: &Header<'_>, connection: &zbus::Connection) -> zbus::fdo::Result<i32> {
    let sender = header.sender().ok_or_else(|| zbus::fdo::Error::Failed("no sender".into()))?.to_owned();
    let pid = zbus::fdo::DBusProxy::new(connection).await?.get_connection_unix_process_id(sender.into()).await?;
    i32::try_from(pid).map_err(|_| zbus::fdo::Error::Failed("bad pid".into()))
}

struct Shell {
    snapshot: Arc<Mutex<Snapshot>>,
    sink: Sink,
}

impl Shell {
    fn read<R>(&self, read: impl FnOnce(&Snapshot) -> R) -> R {
        read(&self.snapshot.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[zbus::interface(name = "org.glide.Shell1")]
impl Shell {
    #[zbus(property)]
    fn appearance(&self) -> String {
        self.read(|s| s.appearance.key().to_string())
    }

    #[zbus(property)]
    fn accent(&self) -> String {
        self.read(|s| s.accent.key().to_string())
    }

    #[zbus(property)]
    fn language(&self) -> String {
        self.read(|s| s.language.to_string())
    }

    #[zbus(property)]
    fn animation_scale(&self) -> f64 {
        self.read(|s| s.animation_scale)
    }

    #[zbus(property)]
    fn reduced_motion(&self) -> bool {
        self.read(|s| s.reduced_motion)
    }

    #[zbus(property)]
    fn font_family(&self) -> String {
        self.read(|s| s.font_family.clone())
    }

    #[zbus(property)]
    fn icon_theme(&self) -> String {
        self.read(|s| s.icon_theme.clone())
    }

    #[zbus(property)]
    fn locked(&self) -> bool {
        self.read(|s| s.locked)
    }

    #[zbus(property)]
    fn active_workspace(&self) -> u32 {
        self.read(|s| s.active_workspace)
    }

    #[zbus(property)]
    fn tokens(&self) -> String {
        self.read(Snapshot::tokens)
    }

    async fn activation_token(
        &self, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<String> {
        let pid = sender_pid(&header, connection).await?;
        let (reply, answer) = crate::oneshot::channel();
        (self.sink)(Event::Token { pid, reply });
        answer.await.flatten().ok_or_else(|| {
            zbus::fdo::Error::AccessDenied("only the focused app may request an activation token".into())
        })
    }

    async fn frame_windows(
        &self, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let pid = sender_pid(&header, connection).await?;
        (self.sink)(Event::Frame { pid });
        Ok(())
    }

    fn lock(&self) {
        (self.sink)(Event::Lock);
    }

    async fn copy_secret(
        &self, secret: String, clear_after: u32, #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let secret = zeroize::Zeroizing::new(secret);
        let pid = sender_pid(&header, connection).await?;
        let (reply, answer) = crate::oneshot::channel();
        (self.sink)(Event::CopySecret { pid, secret, clear_after, reply });
        if answer.await == Some(true) {
            Ok(())
        } else {
            Err(zbus::fdo::Error::AccessDenied("only the focused app may place a secret on the clipboard".into()))
        }
    }

    fn show_osd(&self, kind: &str, level: u32, on: bool) -> zbus::fdo::Result<()> {
        let kind = osd_kind(kind).ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("unknown OSD kind {kind:?}")))?;
        let level = u8::try_from(level.min(100)).unwrap_or(100);
        (self.sink)(Event::Osd(crate::osd::Reading { kind, level, on }));
        Ok(())
    }

    fn open_quick_settings(&self, view: &str) -> zbus::fdo::Result<()> {
        let detail = quick_settings_view(view)
            .map_err(|()| zbus::fdo::Error::InvalidArgs(format!("unknown quick settings view {view:?}")))?;
        (self.sink)(Event::QuickSettings(detail));
        Ok(())
    }

    async fn set_aspect(
        &self, width: u32, height: u32, #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let pid = sender_pid(&header, connection).await?;
        let aspect = crate::window_traits::aspect(width, height);
        (self.sink)(Event::Traits { pid, change: crate::window_traits::Change::Aspect(aspect) });
        Ok(())
    }

    async fn set_keep_above(
        &self, keep: bool, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let pid = sender_pid(&header, connection).await?;
        (self.sink)(Event::Traits { pid, change: crate::window_traits::Change::KeepAbove(keep) });
        Ok(())
    }

    async fn set_picture_in_picture(
        &self, on: bool, #[zbus(header)] header: Header<'_>, #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let pid = sender_pid(&header, connection).await?;
        (self.sink)(Event::Traits { pid, change: crate::window_traits::Change::PictureInPicture(on) });
        Ok(())
    }

    async fn set_content_blur(
        &self, regions: Vec<(i32, i32, i32, i32, i32)>, #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        if regions.len() > crate::window_traits::MAX_BLUR_REGIONS {
            return Err(zbus::fdo::Error::InvalidArgs("too many blur regions".into()));
        }
        let pid = sender_pid(&header, connection).await?;
        let regions = crate::window_traits::blur_regions(&regions);
        (self.sink)(Event::Traits { pid, change: crate::window_traits::Change::Blur(regions) });
        Ok(())
    }
}

pub struct Handle {
    last: Snapshot,
    changes: mpsc::Sender<Snapshot>,
}

impl Handle {
    pub fn publish(&mut self, snapshot: Snapshot) {
        if snapshot == self.last {
            return;
        }
        self.last = snapshot.clone();
        if self.changes.send(snapshot).is_err() {
            tracing::debug!("the shell bus is gone; a state change was not announced");
        }
    }
}

fn announce(connection: &Connection, shared: &Mutex<Snapshot>, changes: mpsc::Receiver<Snapshot>) {
    for snapshot in changes {
        let before = std::mem::replace(&mut *shared.lock().unwrap_or_else(PoisonError::into_inner), snapshot.clone());
        let properties: HashMap<&str, Value<'_>> = snapshot.changed_since(&before).into_iter().collect();
        if properties.is_empty() {
            continue;
        }
        let body = (INTERFACE, properties, Vec::<&str>::new());
        if let Err(err) = connection.emit_signal(
            None::<BusName<'_>>,
            PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &body,
        ) {
            tracing::debug!(?err, "failed to announce a shell property change");
        }
    }
}

pub fn start(initial: Snapshot, sink: impl Fn(Event) + Send + Sync + 'static) -> anyhow::Result<Handle> {
    serve(Builder::session()?, initial, Arc::new(sink))
}

fn serve(builder: Builder<'_>, initial: Snapshot, sink: Sink) -> anyhow::Result<Handle> {
    let snapshot = Arc::new(Mutex::new(initial.clone()));
    let connection = builder.serve_at(PATH, Shell { snapshot: snapshot.clone(), sink })?.build()?;
    match connection.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())? {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {}
        _ => anyhow::bail!("another service already owns the name"),
    }
    let (changes, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("glide-shell-bus".into())
        .spawn(move || announce(&connection, &snapshot, receiver))?;
    Ok(Handle { last: initial, changes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osd_kinds_and_quick_settings_views_parse_by_name() {
        assert_eq!(osd_kind("volume"), Some(crate::osd::Kind::Volume));
        assert_eq!(osd_kind("caps-lock"), Some(crate::osd::Kind::CapsLock));
        assert_eq!(osd_kind("loudness"), None);
        assert!(matches!(quick_settings_view(""), Ok(None)));
        assert!(matches!(quick_settings_view("wifi"), Ok(Some(crate::topbar::Detail::Wifi))));
        assert!(quick_settings_view("printers").is_err());
    }

    #[test]
    fn only_changed_properties_are_announced() {
        let before = Snapshot::default();
        let after = Snapshot { appearance: glide_theme::Appearance::Light, locked: true, ..before.clone() };
        let names: Vec<&str> = after.changed_since(&before).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Appearance", "Locked", "Tokens"]);
        let accent = Snapshot { accent: glide_theme::Accent::Teal, ..before.clone() };
        let names: Vec<&str> = accent.changed_since(&before).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["Accent", "Tokens"], "the token sheet follows the accent too");
    }

    #[test]
    fn the_served_interface_matches_its_published_contract() {
        let Some(bus) = crate::test_bus::private_bus() else {
            return;
        };
        let sink: Sink = Arc::new(|_| {});
        let _handle = serve(bus.builder(), Snapshot::default(), sink).expect("serve");
        let served = crate::test_bus::introspect(&bus, BUS_NAME, PATH);
        let contract = crate::test_bus::members(include_str!("../../../data/dbus/org.glide.Shell1.xml"), INTERFACE);
        assert_eq!(contract.len(), 20, "ten properties and ten methods");
        assert_eq!(
            crate::test_bus::members(&served, INTERFACE),
            contract,
            "a change to org.glide.Shell1 goes into data/dbus first; removing or altering a member needs Shell2"
        );
    }

    #[test]
    fn a_secret_reaches_the_clipboard_only_for_the_focused_caller() {
        let Some(bus) = crate::test_bus::private_bus() else {
            return;
        };
        let (events, received) = mpsc::channel::<(i32, String, u32)>();
        let events = Mutex::new(events);
        let allow = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let gate = Arc::clone(&allow);
        let sink: Sink = Arc::new(move |event| {
            if let Event::CopySecret { pid, secret, clear_after, reply } = event {
                let _ = events.lock().map(|events| events.send((pid, secret.to_string(), clear_after)));
                reply.send(gate.load(std::sync::atomic::Ordering::SeqCst));
            }
        });
        let _handle = serve(bus.builder(), Snapshot::default(), sink).expect("serve");
        let client = bus.client();
        client.call_method(Some(BUS_NAME), PATH, Some(INTERFACE), "CopySecret", &("hunter2", 30u32)).expect("call");
        let (pid, secret, clear_after) = received.recv_timeout(std::time::Duration::from_secs(5)).expect("a copy");
        assert_eq!(pid, i32::try_from(std::process::id()).expect("pid fits"));
        assert_eq!((secret.as_str(), clear_after), ("hunter2", 30));
        allow.store(false, std::sync::atomic::Ordering::SeqCst);
        let refused = client.call_method(Some(BUS_NAME), PATH, Some(INTERFACE), "CopySecret", &("x", 0u32));
        assert!(refused.unwrap_err().to_string().contains("AccessDenied"), "an unfocused caller is refused");
    }

    #[test]
    fn frame_windows_reports_the_calling_process() {
        let Some(bus) = crate::test_bus::private_bus() else {
            return;
        };
        let (events, received) = mpsc::channel::<i32>();
        let events = Mutex::new(events);
        let sink: Sink = Arc::new(move |event| {
            if let Event::Frame { pid } = event {
                let _ = events.lock().map(|events| events.send(pid));
            }
        });
        let _handle = serve(bus.builder(), Snapshot::default(), sink).expect("serve");
        bus.client().call_method(Some(BUS_NAME), PATH, Some(INTERFACE), "FrameWindows", &()).expect("call");
        let pid = received.recv_timeout(std::time::Duration::from_secs(5)).expect("a frame request");
        assert_eq!(pid, i32::try_from(std::process::id()).expect("pid fits"));
    }

    #[test]
    fn window_trait_calls_reach_the_shell_with_the_callers_pid() {
        use crate::window_traits::{BlurRegion, Change};
        let Some(bus) = crate::test_bus::private_bus() else {
            return;
        };
        let (events, received) = mpsc::channel::<(i32, Change)>();
        let events = Mutex::new(events);
        let sink: Sink = Arc::new(move |event| {
            if let Event::Traits { pid, change } = event {
                let _ = events.lock().map(|events| events.send((pid, change)));
            }
        });
        let _handle = serve(bus.builder(), Snapshot::default(), sink).expect("serve");
        let client = bus.client();
        let call = |member: &str, body: &dyn erased_body::Body| body.call(&client, member);
        call("SetAspect", &(1920_u32, 1080_u32));
        call("SetKeepAbove", &(true,));
        call("SetPictureInPicture", &(true,));
        call("SetContentBlur", &(vec![(320_i32, 614_i32, 640_i32, 82_i32, 12_i32)],));
        let me = i32::try_from(std::process::id()).expect("pid fits");
        let mut seen = Vec::new();
        for _ in 0..4 {
            let (pid, change) = received.recv_timeout(std::time::Duration::from_secs(5)).expect("a trait change");
            assert_eq!(pid, me);
            seen.push(change);
        }
        assert_eq!(seen[0], Change::Aspect(Some(1920.0 / 1080.0)));
        assert_eq!(seen[1], Change::KeepAbove(true));
        assert_eq!(seen[2], Change::PictureInPicture(true));
        assert_eq!(seen[3], Change::Blur(vec![BlurRegion { x: 320, y: 614, width: 640, height: 82, radius: 12 }]));
        let too_many: Vec<(i32, i32, i32, i32, i32)> = (0..40).map(|index| (index, 0, 4, 4, 0)).collect();
        let refused =
            client.call_method(Some(BUS_NAME), PATH, Some(INTERFACE), "SetContentBlur", &(too_many,)).is_err();
        assert!(refused, "an absurd number of regions is refused");
    }

    mod erased_body {
        pub trait Body {
            fn call(&self, client: &zbus::blocking::Connection, member: &str);
        }

        impl<T: serde::Serialize + zbus::zvariant::DynamicType> Body for T {
            fn call(&self, client: &zbus::blocking::Connection, member: &str) {
                client
                    .call_method(Some(super::BUS_NAME), super::PATH, Some(super::INTERFACE), member, self)
                    .expect("call");
            }
        }
    }

    #[test]
    fn apps_read_the_published_look_and_hear_about_changes() {
        let Some(bus) = crate::test_bus::private_bus() else {
            return;
        };
        let (events, _received) = mpsc::channel::<()>();
        let events = Mutex::new(events);
        let sink: Sink = Arc::new(move |_| {
            let _ = events.lock().map(|events| events.send(()));
        });
        let mut handle = serve(bus.builder(), Snapshot::default(), sink).expect("serve");
        let client = bus.client();
        let properties = zbus::blocking::fdo::PropertiesProxy::builder(&client)
            .destination(BUS_NAME)
            .and_then(|builder| builder.path(PATH))
            .and_then(zbus::blocking::proxy::Builder::build)
            .expect("proxy");
        let interface = zbus::names::InterfaceName::try_from(INTERFACE).expect("name");
        let all = properties.get_all(interface.clone()).expect("properties");
        let look = glide_app::Look::from_properties(&all).expect("look");
        assert_eq!(look.appearance, glide_theme::Appearance::Dark);

        let mut changes = properties.receive_properties_changed().expect("signals");
        handle.publish(Snapshot { appearance: glide_theme::Appearance::Light, ..Snapshot::default() });
        let signal = changes.next().expect("a change signal");
        let args = signal.args().expect("args");
        assert!(args.changed_properties().contains_key("Appearance"));
        let all = properties.get_all(interface).expect("properties");
        assert_eq!(
            glide_app::Look::from_properties(&all).map(|look| look.appearance),
            Some(glide_theme::Appearance::Light)
        );
    }
}
