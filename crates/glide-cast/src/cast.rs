use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use zbus::zvariant::OwnedObjectPath;

use crate::abort::Abort;
use crate::formats::{self, AudioSupport, Mode};
use crate::p2p::{P2p, Peer, Subnet};
use crate::pipeline::{Encoder, Gate, Probed, Spec, Stream};
use crate::rtsp::{Event, Message, Negotiated, REPLY_WAIT, Reader, Session};
use crate::wfd::CONTROL_PORT;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(40);
const ADDRESS_WAIT: Duration = Duration::from_secs(15);
const ACCEPT_WAIT: Duration = Duration::from_secs(30);
const KEEPALIVE: Duration = Duration::from_secs(20);
const FIND_POLL: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_millis(100);
const SERVER_PORT: u16 = 19000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Connecting(String),
    Negotiated(Mode),
    Streaming(Mode),
    Paused,
    Ended(Option<String>),
}

#[derive(Default)]
struct Shared {
    stream: Mutex<Option<(Stream, Mode)>>,
    paused: AtomicBool,
}

impl Shared {
    fn gate(&self) -> Option<Gate> {
        self.stream.lock().unwrap_or_else(PoisonError::into_inner).as_ref().map(|(stream, _)| stream.gate())
    }
}

#[derive(Clone)]
pub struct Handle {
    stop: mpsc::Sender<()>,
    shared: Arc<Shared>,
}

impl Handle {
    #[must_use]
    pub fn mode(&self) -> Option<Mode> {
        self.shared.stream.lock().unwrap_or_else(PoisonError::into_inner).as_ref().map(|(_, mode)| *mode)
    }

    #[must_use]
    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::Acquire)
    }

    pub fn push_frame(&self, pixels: Vec<u8>, size: (u32, u32)) {
        if self.paused() {
            return;
        }
        if let Some((stream, _)) = self.shared.stream.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
            let _ = stream.push_frame(pixels, size);
        }
    }

    pub fn stop(&self) {
        let _ = self.stop.send(());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    Peers(Vec<Peer>),
    Finished,
    Failed(String),
}

#[derive(Default)]
struct Flag {
    cancelled: Mutex<bool>,
    changed: Condvar,
}

impl Flag {
    fn cancel(&self) {
        *self.cancelled.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.changed.notify_all();
    }

    fn cancelled(&self) -> bool {
        *self.cancelled.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait(&self, limit: Duration) -> bool {
        let guard = self.cancelled.lock().unwrap_or_else(PoisonError::into_inner);
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, limit, |cancelled| !*cancelled)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

#[derive(Clone)]
pub struct Search {
    flag: Arc<Flag>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Search {
    pub fn cancel(&self) {
        self.flag.cancel();
    }

    pub(crate) fn finish(&self) {
        self.flag.cancel();
        let thread = self.thread.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

fn search(
    flag: &Flag, limit: Duration, poll: Duration, mut peers: impl FnMut() -> Option<Vec<Peer>>,
    stop_find: impl FnOnce(), found: &impl Fn(Discovery),
) {
    let deadline = Instant::now() + limit;
    let mut last = Vec::new();
    let mut cancelled = flag.cancelled();
    while !cancelled && Instant::now() < deadline {
        if let Some(list) = peers()
            && list != last
        {
            last.clone_from(&list);
            found(Discovery::Peers(list));
        }
        cancelled = flag.wait(poll);
    }
    if !flag.cancelled() {
        stop_find();
    }
    found(Discovery::Finished);
}

pub fn preflight() -> Result<Encoder, String> {
    crate::pipeline::probe().map(|probed| probed.encoder)
}

pub fn discover(seconds: u32, found: impl Fn(Discovery) + Send + 'static) -> std::io::Result<Search> {
    let flag = Arc::new(Flag::default());
    let shared = Arc::clone(&flag);
    let thread = std::thread::Builder::new().name("glide-cast-find".into()).spawn(move || {
        if let Err(reason) = preflight() {
            return found(Discovery::Failed(reason));
        }
        if shared.cancelled() {
            return found(Discovery::Finished);
        }
        let p2p = match P2p::open() {
            Ok(p2p) => p2p,
            Err(err) => return found(Discovery::Failed(err)),
        };
        if let Err(err) = p2p.start_find(i32::try_from(seconds).unwrap_or(30)) {
            return found(Discovery::Failed(err));
        }
        let limit = Duration::from_secs(u64::from(seconds));
        search(
            &shared,
            limit,
            FIND_POLL,
            || p2p.peers().ok(),
            || {
                let _ = p2p.stop_find();
            },
            &found,
        );
    })?;
    Ok(Search { flag, thread: Arc::new(Mutex::new(Some(thread))) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub frame_format: &'static str,
    pub size: (u32, u32),
}

pub fn start(
    peer: Peer, source: Source, options: Options, search: Option<Search>, status: impl Fn(Status) + Send + 'static,
) -> std::io::Result<Handle> {
    let (stop, stopped) = mpsc::channel();
    let shared = Arc::new(Shared::default());
    let owned = Arc::clone(&shared);
    std::thread::Builder::new().name("glide-cast".into()).spawn(move || {
        status(Status::Connecting(peer.name.clone()));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&peer, source, options, search.as_ref(), &stopped, &owned, &status)
        }))
        .unwrap_or_else(|_| Err(Abort::from(CRASHED)));
        let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(swap(&owned.stream, None))));
        status(ending(outcome.and(dropped.map_err(|_| Abort::from(CRASHED)))));
    })?;
    Ok(Handle { stop, shared })
}

const CRASHED: &str = "screen mirroring stopped after an internal error";

struct Joined<'a> {
    p2p: &'a P2p,
    active: OwnedObjectPath,
}

impl Drop for Joined<'_> {
    fn drop(&mut self) {
        let _ = self.p2p.disconnect(&self.active);
    }
}

fn ending(outcome: Result<(), Abort>) -> Status {
    Status::Ended(outcome.err().and_then(Abort::into_reason))
}

fn swap<T>(slot: &Mutex<Option<T>>, next: Option<T>) -> Option<T> {
    let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    std::mem::replace(&mut *guard, next)
}

fn run(
    peer: &Peer, source: Source, options: Options, search: Option<&Search>, stopped: &mpsc::Receiver<()>,
    shared: &Shared, status: &impl Fn(Status),
) -> Result<(), Abort> {
    if let Some(search) = search {
        search.finish();
    }
    let encoder = crate::pipeline::probe()?;
    let preference = preference(encoder.encoder.hardware, options.quality);
    let sound = AudioSupport {
        enabled: options.audio,
        capture: crate::pipeline::audio_capture(crate::pipeline::element_exists),
        aac_encoder: crate::pipeline::aac_encoder(crate::pipeline::element_exists),
    };
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, CONTROL_PORT))
        .and_then(|listener| listener.set_nonblocking(true).map(|()| listener))
        .map_err(|err| format!("port {CONTROL_PORT}: {err}"))?;
    let p2p = P2p::open()?;
    let _ = p2p.stop_find();
    let joined = Joined { active: p2p.connect(peer, CONNECT_TIMEOUT, stopped)?, p2p: &p2p };
    let subnet = wait_for(ADDRESS_WAIT, stopped, || p2p.local_address(&joined.active))?
        .ok_or("the Wi-Fi Direct link got no address")?;
    let (tcp, display) =
        accept(&listener, subnet, stopped)?.ok_or("the display did not open its control connection")?;
    let link = Link { tcp, display, local: subnet.address, source, preference, sound, encoder };
    let served = link.serve(stopped, shared, status).map_err(Abort::from);
    drop(joined);
    drop(listener);
    served
}

fn wait_for<T>(
    limit: Duration, stopped: &mpsc::Receiver<()>, mut probe: impl FnMut() -> Option<T>,
) -> Result<Option<T>, Abort> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if stopped.try_recv().is_ok() {
            return Err(Abort::Stopped);
        }
        if let Some(value) = probe() {
            return Ok(Some(value));
        }
        std::thread::sleep(TICK * 2);
    }
    Ok(None)
}

fn accept(
    listener: &TcpListener, subnet: Subnet, stopped: &mpsc::Receiver<()>,
) -> Result<Option<(TcpStream, SocketAddr)>, Abort> {
    wait_for(ACCEPT_WAIT, stopped, || match listener.accept() {
        Ok((tcp, address)) if subnet.contains(address.ip()) => Some((tcp, address)),
        Ok(_) | Err(_) => None,
    })
}

const WRITE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

fn session_id() -> u32 {
    let mut bytes = [0_u8; 4];
    let random = std::fs::File::open("/dev/urandom")
        .and_then(|mut source| std::io::Read::read_exact(&mut source, &mut bytes))
        .is_ok();
    if random {
        u32::from_ne_bytes(bytes)
    } else {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.subsec_nanos());
        std::process::id() ^ nanos
    }
}

struct Link {
    tcp: TcpStream,
    display: SocketAddr,
    local: Ipv4Addr,
    source: Source,
    preference: &'static [u8],
    sound: AudioSupport,
    encoder: Probed,
}

struct Serving {
    negotiated: Option<Negotiated>,
    last_keepalive: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continue,
    Finished,
}

impl Link {
    fn write(&mut self, message: &Message) -> Result<(), String> {
        self.tcp.write_all(&message.to_bytes()).map_err(|err| format!("the display connection broke: {err}"))
    }

    fn apply(
        &self, events: Vec<Event>, serving: &mut Serving, shared: &Shared, status: &impl Fn(Status),
    ) -> Result<Flow, String> {
        for event in events {
            match event {
                Event::Negotiated(choice) => {
                    status(Status::Negotiated(choice.video.mode));
                    serving.negotiated = Some(choice);
                }
                Event::Play => {
                    let choice = serving.negotiated.clone().ok_or("PLAY came before negotiation")?;
                    let spec = Spec {
                        video: choice.video,
                        audio: choice.audio,
                        host: display_host(self.display.ip()),
                        port: choice.sink_rtp_port,
                        frame_format: self.source.frame_format,
                        input: self.source.size,
                        encoder: self.encoder.clone(),
                    };
                    let stream = Stream::start(&spec)?;
                    shared.paused.store(false, Ordering::Release);
                    drop(swap(&shared.stream, Some((stream, choice.video.mode))));
                    status(Status::Streaming(choice.video.mode));
                    serving.last_keepalive = Instant::now();
                }
                Event::Pause => {
                    shared.paused.store(true, Ordering::Release);
                    if let Some(gate) = shared.gate() {
                        gate.pause()?;
                    }
                    status(Status::Paused);
                }
                Event::Resume => {
                    let choice = serving.negotiated.as_ref().ok_or("PLAY came before negotiation")?;
                    if let Some(gate) = shared.gate() {
                        gate.resume()?;
                    }
                    if let Some((stream, _)) = shared.stream.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
                        stream.request_keyframe();
                    }
                    shared.paused.store(false, Ordering::Release);
                    status(Status::Streaming(choice.video.mode));
                }
                Event::KeyFrame => {
                    if let Some((stream, _)) = shared.stream.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
                        stream.request_keyframe();
                    }
                }
                Event::Teardown => return Ok(Flow::Finished),
                Event::Failed(reason) => return Err(reason),
            }
        }
        Ok(Flow::Continue)
    }

    fn await_teardown(&mut self, session: &mut Session, reader: &mut Reader) {
        let deadline = Instant::now() + REPLY_WAIT;
        let mut buffer = [0; 4096];
        while Instant::now() < deadline {
            match self.tcp.read(&mut buffer) {
                Ok(0) => return,
                Ok(read) => reader.push(&buffer[..read]),
                Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return,
            }
            while let Ok(Some(message)) = reader.next_message() {
                let (out, events) = session.receive(&message, Instant::now());
                for reply in &out {
                    if self.write(reply).is_err() {
                        return;
                    }
                }
                if events.contains(&Event::Teardown) {
                    return;
                }
            }
        }
    }

    fn serve(mut self, stopped: &mpsc::Receiver<()>, shared: &Shared, status: &impl Fn(Status)) -> Result<(), String> {
        self.tcp.set_nonblocking(false).map_err(|err| err.to_string())?;
        self.tcp.set_read_timeout(Some(TICK)).map_err(|err| err.to_string())?;
        self.tcp.set_write_timeout(Some(WRITE_WAIT)).map_err(|err| err.to_string())?;
        let id = format!("{:08X}", session_id());
        let mut session = Session::new(&self.local.to_string(), SERVER_PORT, self.preference, self.sound, &id);
        let mut reader = Reader::default();
        let mut serving = Serving { negotiated: None, last_keepalive: Instant::now() };
        let first = session.start(Instant::now());
        self.write(&first)?;
        let mut buffer = [0; 4096];
        loop {
            if stopped.try_recv().is_ok() {
                let teardown = session.teardown(Instant::now());
                if self.write(&teardown).is_ok() {
                    self.await_teardown(&mut session, &mut reader);
                }
                return Ok(());
            }
            match self.tcp.read(&mut buffer) {
                Ok(0) => return Err("the display closed the connection".into()),
                Ok(read) => reader.push(&buffer[..read]),
                Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(err) => return Err(format!("the display connection broke: {err}")),
            }
            while let Some(message) = reader.next_message().map_err(|err| format!("{err:?}"))? {
                let (out, events) = session.receive(&message, Instant::now());
                for reply in &out {
                    self.write(reply)?;
                }
                if self.apply(events, &mut serving, shared, status)? == Flow::Finished {
                    return Ok(());
                }
            }
            if self.apply(session.poll(Instant::now()), &mut serving, shared, status)? == Flow::Finished {
                return Ok(());
            }
            if let Some(failure) = shared
                .stream
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .and_then(|(stream, _)| stream.failure())
            {
                return Err(failure);
            }
            if serving.negotiated.is_some() && serving.last_keepalive.elapsed() >= KEEPALIVE {
                let keepalive = session.keepalive(Instant::now());
                self.write(&keepalive)?;
                serving.last_keepalive = Instant::now();
            }
        }
    }
}

fn display_host(address: IpAddr) -> String {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or_else(|| v6.to_string(), |v4| v4.to_string()),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

#[must_use]
pub(crate) const fn preference(hardware: bool, quality: Quality) -> &'static [u8] {
    match (hardware, quality) {
        (true, Quality::Best) => &formats::HARDWARE_PREFERENCE,
        (false, Quality::Best) => &formats::SOFTWARE_PREFERENCE,
        (true, Quality::Smooth) => &formats::HARDWARE_PREFERENCE_720,
        (false, Quality::Smooth) => &formats::SOFTWARE_PREFERENCE_720,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    #[default]
    Best,
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub quality: Quality,
    pub audio: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { quality: Quality::Best, audio: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_smooth_quality_never_offers_more_than_720p() {
        for hardware in [false, true] {
            let modes = preference(hardware, Quality::Smooth);
            assert!(modes.iter().all(|bit| formats::CEA[usize::from(*bit)].height <= 720), "{modes:?}");
            assert_eq!(modes.last(), Some(&0), "640×480 stays as the last resort");
            assert!(
                preference(hardware, Quality::Best).iter().any(|bit| formats::CEA[usize::from(*bit)].height == 1080)
            );
        }
    }

    #[test]
    fn a_mapped_ipv6_peer_streams_to_its_ipv4_address() {
        let mapped: IpAddr = "::ffff:192.168.49.10".parse().unwrap();
        assert_eq!(display_host(mapped), "192.168.49.10");
        assert_eq!(display_host("192.168.49.20".parse().unwrap()), "192.168.49.20");
    }

    #[test]
    fn waiting_gives_up_at_its_limit_and_on_stop() {
        let (stop, stopped) = mpsc::channel();
        assert_eq!(wait_for(Duration::from_millis(50), &stopped, || None::<u8>), Ok(None));
        assert_eq!(wait_for(Duration::from_secs(5), &stopped, || Some(7)), Ok(Some(7)));
        stop.send(()).unwrap();
        assert_eq!(wait_for(Duration::from_secs(5), &stopped, || None::<u8>), Err(Abort::Stopped));
    }

    #[test]
    fn a_stop_ends_the_cast_quietly_and_a_failure_ends_it_with_its_reason() {
        assert_eq!(ending(Ok(())), Status::Ended(None));
        assert_eq!(ending(Err(Abort::Stopped)), Status::Ended(None), "pressing Stop is not a failure");
        assert_eq!(
            ending(Err(Abort::from("the display closed the connection"))),
            Status::Ended(Some("the display closed the connection".into()))
        );
    }

    #[test]
    fn a_stop_while_waiting_for_the_link_address_is_not_reported_as_a_failure() {
        let (stop, stopped) = mpsc::channel();
        stop.send(()).unwrap();
        let outcome = wait_for(ADDRESS_WAIT, &stopped, || None::<Ipv4Addr>).map(|_| ());
        assert_eq!(ending(outcome), Status::Ended(None));
    }

    fn cancelled_after(flag: &Arc<Flag>, delay: Duration) -> JoinHandle<()> {
        let flag = Arc::clone(flag);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            flag.cancel();
        })
    }

    const SHORT: Duration = Duration::from_millis(5);

    fn display(name: &str) -> Peer {
        Peer {
            path: zbus::zvariant::OwnedObjectPath::try_from(format!("/peer/{name}")).unwrap(),
            name: name.into(),
            address: "aa:bb:cc:dd:ee:ff".into(),
            strength: 50,
            display: None,
        }
    }

    #[test]
    fn a_search_that_runs_out_stops_the_find_once_and_reports_its_end() {
        let flag = Flag::default();
        let stops = Mutex::new(0);
        let found = Mutex::new(Vec::new());
        search(&flag, Duration::ZERO, SHORT, || None, || *stops.lock().unwrap() += 1, &|event| {
            found.lock().unwrap().push(event);
        });
        assert_eq!(*stops.lock().unwrap(), 1);
        assert_eq!(*found.lock().unwrap(), [Discovery::Finished]);
    }

    #[test]
    fn a_cancelled_search_skips_the_trailing_stop_find_but_still_reports_its_end() {
        let flag = Flag::default();
        flag.cancel();
        let stops = Mutex::new(0);
        let found = Mutex::new(Vec::new());
        search(&flag, Duration::from_secs(30), SHORT, || None, || *stops.lock().unwrap() += 1, &|event| {
            found.lock().unwrap().push(event);
        });
        assert_eq!(*stops.lock().unwrap(), 0, "StopFind would reset wpa_supplicant in the middle of a join");
        assert_eq!(*found.lock().unwrap(), [Discovery::Finished], "the compositor still learns the search is over");
    }

    #[test]
    fn a_cancel_that_arrives_during_the_search_also_skips_the_trailing_stop_find() {
        let flag = Flag::default();
        let mut polls = 0;
        let stops = Mutex::new(0);
        let peers = || {
            polls += 1;
            if polls == 3 {
                flag.cancel();
            }
            None
        };
        search(&flag, Duration::from_secs(30), SHORT, peers, || *stops.lock().unwrap() += 1, &|_| {});
        assert_eq!(*stops.lock().unwrap(), 0);
    }

    #[test]
    fn a_search_notices_a_cancel_without_waiting_out_its_poll_interval() {
        let flag = Arc::new(Flag::default());
        let stops = Mutex::new(0);
        let canceller = cancelled_after(&flag, Duration::from_millis(50));
        let started = Instant::now();
        search(
            &flag,
            Duration::from_mins(1),
            Duration::from_secs(30),
            || None,
            || *stops.lock().unwrap() += 1,
            &|_| {},
        );
        canceller.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(10), "took {:?}", started.elapsed());
        assert_eq!(*stops.lock().unwrap(), 0);
    }

    #[test]
    fn a_search_reports_a_list_only_when_it_changes() {
        let flag = Flag::default();
        let mut lists = vec![vec![display("tv"), display("box")], vec![display("tv")], vec![display("tv")], vec![]];
        let peers = || {
            lists.pop().or_else(|| {
                flag.cancel();
                None
            })
        };
        let found = Mutex::new(Vec::new());
        search(&flag, Duration::from_secs(30), SHORT, peers, || {}, &|event| found.lock().unwrap().push(event));
        assert_eq!(
            *found.lock().unwrap(),
            [
                Discovery::Peers(vec![display("tv")]),
                Discovery::Peers(vec![display("tv"), display("box")]),
                Discovery::Finished
            ],
            "the empty list at the start and the repeated list are not news"
        );
    }

    #[test]
    fn finishing_a_search_cancels_it_and_waits_for_its_thread() {
        let flag = Arc::new(Flag::default());
        let ended = Arc::new(AtomicBool::new(false));
        let thread = {
            let flag = Arc::clone(&flag);
            let ended = Arc::clone(&ended);
            std::thread::spawn(move || {
                while !flag.wait(Duration::from_secs(30)) {}
                std::thread::sleep(Duration::from_millis(150));
                ended.store(true, Ordering::Release);
            })
        };
        let search = Search { flag, thread: Arc::new(Mutex::new(Some(thread))) };
        let copy = search.clone();
        let started = Instant::now();
        search.finish();
        assert!(ended.load(Ordering::Acquire), "the thread was still running when finish returned");
        assert!(started.elapsed() < Duration::from_secs(5));
        copy.finish();
    }

    #[test]
    fn dropping_a_search_handle_does_not_cancel_the_search() {
        let flag = Arc::new(Flag::default());
        let search = Search { flag: Arc::clone(&flag), thread: Arc::new(Mutex::new(None)) };
        drop(search);
        assert!(!flag.cancelled());
    }

    struct UnlockProbe {
        slot: Arc<Mutex<Option<UnlockProbe>>>,
        unlocked: Arc<Mutex<Vec<bool>>>,
    }

    impl Drop for UnlockProbe {
        fn drop(&mut self) {
            let unlocked = self.slot.try_lock().is_ok();
            self.unlocked.lock().unwrap().push(unlocked);
        }
    }

    #[test]
    fn a_replaced_or_ended_stream_is_torn_down_after_the_render_path_lock_is_released() {
        let slot = Arc::new(Mutex::new(None));
        let unlocked = Arc::new(Mutex::new(Vec::new()));
        let probe = || UnlockProbe { slot: Arc::clone(&slot), unlocked: Arc::clone(&unlocked) };
        drop(swap(&slot, Some(probe())));
        drop(swap(&slot, Some(probe())));
        drop(swap(&slot, None));
        assert!(slot.lock().unwrap().is_none());
        assert_eq!(*unlocked.lock().unwrap(), [true, true]);
    }

    struct Sink {
        tcp: TcpStream,
        reader: Reader,
    }

    impl Sink {
        fn next(&mut self) -> Message {
            let mut buffer = [0; 4096];
            loop {
                if let Some(message) = self.reader.next_message().unwrap() {
                    return message;
                }
                let read = self.tcp.read(&mut buffer).unwrap();
                assert!(read > 0, "the source hung up");
                self.reader.push(&buffer[..read]);
            }
        }

        fn ok(&mut self, request: &Message, body: &str) {
            let mut reply = Message::response(200, "OK").with("CSeq", request.cseq().unwrap().to_string());
            if !body.is_empty() {
                reply = reply.with_parameters(body.into());
            }
            self.tcp.write_all(&reply.to_bytes()).unwrap();
        }

        fn send(&mut self, request: &str) -> Message {
            self.tcp.write_all(request.as_bytes()).unwrap();
            self.next()
        }

        fn handshake(&mut self, rtp_port: u16) {
            let m1 = self.next();
            self.ok(&m1, "");
            let m2 = self.send("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n");
            assert!(matches!(m2.start, crate::rtsp::Start::Response { code: 200, .. }));
            let m3 = self.next();
            self.ok(
                &m3,
                &format!(
                    "wfd_video_formats: 00 00 01 01 00000001 00000000 00000000 00 0000 0000 00 none none\r\n\
                     wfd_audio_codecs: none\r\n\
                     wfd_client_rtp_ports: RTP/AVP/UDP;unicast {rtp_port} 0 mode=play\r\n"
                ),
            );
            let m4 = self.next();
            assert!(m4.parameters()["wfd_video_formats"].starts_with("00 00 01 01 00000001"));
            self.ok(&m4, "");
            let m5 = self.next();
            self.ok(&m5, "");
            let setup = self.send(&format!(
                "SETUP rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port={rtp_port}\r\n\r\n"
            ));
            assert!(setup.header("Session").is_some());
        }

        fn answer_teardown(&mut self) {
            let trigger = self.next();
            assert_eq!(trigger.parameters()["wfd_trigger_method"], "TEARDOWN");
            self.ok(&trigger, "");
            assert_eq!(self.command("TEARDOWN", 90), 200, "the source answers the sink's TEARDOWN");
        }

        fn command(&mut self, method: &str, cseq: u32) -> u16 {
            let reply =
                self.send(&format!("{method} rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: {cseq}\r\n\r\n"));
            let crate::rtsp::Start::Response { code, .. } = reply.start else { panic!("{reply:?}") };
            code
        }
    }

    type Outcome = (Result<(), String>, Vec<Status>);

    fn spawn_source(
        listener: TcpListener, stopped: mpsc::Receiver<()>, shared: Arc<Shared>,
    ) -> std::thread::JoinHandle<Outcome> {
        std::thread::spawn(move || {
            let (tcp, display) = listener.accept().unwrap();
            let source = Source { frame_format: "BGRx", size: (800, 500) };
            let preference = &formats::SOFTWARE_PREFERENCE;
            let sound = AudioSupport { enabled: true, capture: true, aac_encoder: false };
            let encoder = crate::pipeline::probe().unwrap();
            let link = Link { tcp, display, local: Ipv4Addr::LOCALHOST, source, preference, sound, encoder };
            let statuses = Mutex::new(Vec::new());
            let outcome = link.serve(&stopped, &shared, &|status| statuses.lock().unwrap().push(status));
            (outcome, statuses.into_inner().unwrap())
        })
    }

    fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn encoder_installed() -> bool {
        crate::pipeline::probe().is_ok()
    }

    #[test]
    fn a_display_gets_rtp_after_the_full_rtsp_exchange() {
        if !encoder_installed() {
            eprintln!("no working H.264 encoder; skipping");
            return;
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let rtp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        rtp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let rtp_port = rtp.local_addr().unwrap().port();
        let shared = Arc::new(Shared::default());
        let (stop, stopped) = mpsc::channel::<()>();
        let source = spawn_source(listener, stopped, Arc::clone(&shared));

        let mut sink = Sink { tcp: TcpStream::connect(address).unwrap(), reader: Reader::default() };
        sink.handshake(rtp_port);
        assert_eq!(sink.command("PLAY", 3), 200);

        wait_until("the stream", || shared.stream.lock().unwrap().is_some());
        let handle = Handle { stop, shared: Arc::clone(&shared) };
        assert_eq!(handle.mode().map(|mode| (mode.width, mode.height)), Some((640, 480)));
        for _ in 0..15 {
            handle.push_frame(vec![0x40; 800 * 500 * 4], (800, 500));
            std::thread::sleep(Duration::from_millis(33));
        }
        let mut packet = [0; 2048];
        rtp.recv(&mut packet).unwrap();
        assert_eq!(packet[1] & 0x7f, 33);
        assert_eq!(packet[12], 0x47);

        handle.stop();
        sink.answer_teardown();
        let (outcome, statuses) = source.join().unwrap();
        assert_eq!(outcome, Ok(()));
        assert!(matches!(statuses.as_slice(), [Status::Negotiated(_), Status::Streaming(_)]), "{statuses:?}");
    }

    #[test]
    fn a_stopped_source_waits_for_the_sinks_teardown_and_answers_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = mpsc::channel::<()>();
        let source = std::thread::spawn(move || {
            let (tcp, display) = listener.accept().unwrap();
            let link = Link {
                tcp,
                display,
                local: Ipv4Addr::LOCALHOST,
                source: Source { frame_format: "BGRx", size: (640, 480) },
                preference: &formats::SOFTWARE_PREFERENCE,
                sound: AudioSupport { enabled: false, capture: false, aac_encoder: false },
                encoder: crate::pipeline::Probed::assumed("openh264enc"),
            };
            link.serve(&stopped, &Shared::default(), &|_| {})
        });
        let mut sink = Sink { tcp: TcpStream::connect(address).unwrap(), reader: Reader::default() };
        let m1 = sink.next();
        sink.ok(&m1, "");
        stop.send(()).unwrap();
        let started = Instant::now();
        sink.answer_teardown();
        assert_eq!(source.join().unwrap(), Ok(()));
        assert!(started.elapsed() < REPLY_WAIT, "the source left as soon as the sink tore down");
    }

    #[test]
    fn pause_stops_the_frames_and_a_later_play_resumes_the_same_pipeline() {
        if !encoder_installed() {
            eprintln!("no working H.264 encoder; skipping");
            return;
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let rtp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let rtp_port = rtp.local_addr().unwrap().port();
        let shared = Arc::new(Shared::default());
        let (stop, stopped) = mpsc::channel::<()>();
        let source = spawn_source(listener, stopped, Arc::clone(&shared));

        let mut sink = Sink { tcp: TcpStream::connect(address).unwrap(), reader: Reader::default() };
        sink.handshake(rtp_port);
        assert_eq!(sink.command("PLAY", 3), 200);
        wait_until("the stream", || shared.stream.lock().unwrap().is_some());
        let handle = Handle { stop, shared: Arc::clone(&shared) };
        let pipeline = shared.gate().unwrap();
        assert!(!handle.paused());

        assert_eq!(sink.command("PAUSE", 4), 200, "PAUSE is advertised, so it must be answered");
        wait_until("the pause", || handle.paused());
        assert_eq!(sink.command("PLAY", 5), 200);
        wait_until("the resume", || !handle.paused());
        assert_eq!(shared.gate().unwrap(), pipeline, "PLAY after PAUSE must not build a second pipeline");
        assert_eq!(sink.command("PLAY", 6), 200, "a PLAY while playing is harmless");

        handle.stop();
        sink.answer_teardown();
        let (outcome, statuses) = source.join().unwrap();
        assert_eq!(outcome, Ok(()));
        assert!(
            matches!(
                statuses.as_slice(),
                [Status::Negotiated(_), Status::Streaming(_), Status::Paused, Status::Streaming(_)]
            ),
            "{statuses:?}"
        );
    }
}
