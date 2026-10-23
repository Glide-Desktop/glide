use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use crate::formats::{self, Mode};
use crate::p2p::{P2p, Peer};
use crate::pipeline::{Spec, Stream};
use crate::rtsp::{Event, Message, Negotiated, Reader, Session};
use crate::wfd::CONTROL_PORT;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(40);
const ADDRESS_WAIT: Duration = Duration::from_secs(15);
const ACCEPT_WAIT: Duration = Duration::from_secs(30);
const KEEPALIVE: Duration = Duration::from_secs(20);
const TICK: Duration = Duration::from_millis(100);
const SERVER_PORT: u16 = 19000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Connecting(String),
    Negotiated(Mode),
    Streaming(Mode),
    Ended(Option<String>),
}

#[derive(Clone)]
pub struct Handle {
    stop: mpsc::Sender<()>,
    stream: Arc<Mutex<Option<(Stream, Mode)>>>,
}

impl Handle {
    #[must_use]
    pub fn mode(&self) -> Option<Mode> {
        self.stream.lock().unwrap_or_else(PoisonError::into_inner).as_ref().map(|(_, mode)| *mode)
    }

    pub fn push_frame(&self, pixels: Vec<u8>) {
        if let Some((stream, _)) = self.stream.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            let _ = stream.push_frame(pixels);
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

pub fn discover(seconds: u32, found: impl Fn(Discovery) + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new().name("glide-cast-find".into()).spawn(move || {
        let p2p = match P2p::open() {
            Ok(p2p) => p2p,
            Err(err) => return found(Discovery::Failed(err)),
        };
        if let Err(err) = p2p.start_find(i32::try_from(seconds).unwrap_or(30)) {
            return found(Discovery::Failed(err));
        }
        let deadline = Instant::now() + Duration::from_secs(u64::from(seconds));
        let mut last = Vec::new();
        while Instant::now() < deadline {
            if let Ok(peers) = p2p.peers()
                && peers != last
            {
                last.clone_from(&peers);
                found(Discovery::Peers(peers));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        let _ = p2p.stop_find();
        found(Discovery::Finished);
    })?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub frame_format: &'static str,
    pub size: (u32, u32),
}

pub fn start(peer: Peer, source: Source, status: impl Fn(Status) + Send + 'static) -> std::io::Result<Handle> {
    let (stop, stopped) = mpsc::channel();
    let stream = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&stream);
    std::thread::Builder::new().name("glide-cast".into()).spawn(move || {
        status(Status::Connecting(peer.name.clone()));
        let outcome = run(&peer, source, &stopped, &slot, &status);
        slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        status(Status::Ended(outcome.err()));
    })?;
    Ok(Handle { stop, stream })
}

fn run(
    peer: &Peer, source: Source, stopped: &mpsc::Receiver<()>, slot: &Arc<Mutex<Option<(Stream, Mode)>>>,
    status: &impl Fn(Status),
) -> Result<(), String> {
    gstreamer::init().map_err(|err| format!("GStreamer did not start: {err}"))?;
    let preference = preference(crate::pipeline::hardware_encoder(crate::pipeline::element_exists));
    let p2p = P2p::open()?;
    let active = p2p.connect(peer, CONNECT_TIMEOUT)?;
    let served = (|| {
        let local = wait_for(ADDRESS_WAIT, stopped, || p2p.local_address(&active))?
            .ok_or("the Wi-Fi Direct link got no address")?;
        let (tcp, display) = accept(local, stopped)?.ok_or("the display did not open its control connection")?;
        let link = Link { tcp, display, local, source, preference };
        link.serve(stopped, slot, status)
    })();
    let _ = p2p.disconnect(&active);
    served
}

fn wait_for<T>(
    limit: Duration, stopped: &mpsc::Receiver<()>, mut probe: impl FnMut() -> Option<T>,
) -> Result<Option<T>, String> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if stopped.try_recv().is_ok() {
            return Err("stopped".into());
        }
        if let Some(value) = probe() {
            return Ok(Some(value));
        }
        std::thread::sleep(TICK * 2);
    }
    Ok(None)
}

fn accept(local: Ipv4Addr, stopped: &mpsc::Receiver<()>) -> Result<Option<(TcpStream, SocketAddr)>, String> {
    let listener = TcpListener::bind((local, CONTROL_PORT)).map_err(|err| format!("port {CONTROL_PORT}: {err}"))?;
    listener.set_nonblocking(true).map_err(|err| err.to_string())?;
    wait_for(ACCEPT_WAIT, stopped, || match listener.accept() {
        Ok((tcp, address)) => Some((tcp, address)),
        Err(_) => None,
    })
}

struct Link {
    tcp: TcpStream,
    display: SocketAddr,
    local: Ipv4Addr,
    source: Source,
    preference: &'static [u8],
}

impl Link {
    fn write(&mut self, message: &Message) -> Result<(), String> {
        self.tcp.write_all(&message.to_bytes()).map_err(|err| format!("the display connection broke: {err}"))
    }

    fn serve(
        mut self, stopped: &mpsc::Receiver<()>, slot: &Arc<Mutex<Option<(Stream, Mode)>>>, status: &impl Fn(Status),
    ) -> Result<(), String> {
        self.tcp.set_nonblocking(false).map_err(|err| err.to_string())?;
        self.tcp.set_read_timeout(Some(TICK)).map_err(|err| err.to_string())?;
        let id = format!("{:08X}", std::process::id() ^ (Instant::now().elapsed().subsec_nanos()));
        let mut session = Session::new(&self.local.to_string(), SERVER_PORT, self.preference, &id);
        let mut reader = Reader::default();
        let mut negotiated: Option<Negotiated> = None;
        let mut last_keepalive = Instant::now();
        let first = session.start();
        self.write(&first)?;
        let mut buffer = [0; 4096];
        loop {
            if stopped.try_recv().is_ok() {
                let teardown = session.teardown();
                let _ = self.write(&teardown);
                return Ok(());
            }
            match self.tcp.read(&mut buffer) {
                Ok(0) => return Err("the display closed the connection".into()),
                Ok(read) => reader.push(&buffer[..read]),
                Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(err) => return Err(format!("the display connection broke: {err}")),
            }
            while let Some(message) = reader.next_message().map_err(|err| format!("{err:?}"))? {
                let (out, events) = session.receive(&message);
                for reply in &out {
                    self.write(reply)?;
                }
                for event in events {
                    match event {
                        Event::Negotiated(choice) => {
                            status(Status::Negotiated(choice.video.mode));
                            negotiated = Some(choice);
                        }
                        Event::Play => {
                            let choice = negotiated.clone().ok_or("PLAY came before negotiation")?;
                            let spec = Spec {
                                video: choice.video,
                                audio: choice.audio,
                                host: display_host(self.display.ip()),
                                port: choice.sink_rtp_port,
                                frame_format: self.source.frame_format,
                                input: self.source.size,
                            };
                            let stream = Stream::start(&spec)?;
                            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some((stream, choice.video.mode));
                            status(Status::Streaming(choice.video.mode));
                            last_keepalive = Instant::now();
                        }
                        Event::KeyFrame => {
                            if let Some((stream, _)) = slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
                                stream.request_keyframe();
                            }
                        }
                        Event::Teardown => return Ok(()),
                        Event::Failed(reason) => return Err(reason),
                    }
                }
            }
            if let Some(failure) =
                slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref().and_then(|(stream, _)| stream.failure())
            {
                return Err(failure);
            }
            if negotiated.is_some() && last_keepalive.elapsed() >= KEEPALIVE {
                let keepalive = session.keepalive();
                self.write(&keepalive)?;
                last_keepalive = Instant::now();
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
pub const fn preference(hardware: bool) -> &'static [u8] {
    if hardware { &formats::HARDWARE_PREFERENCE } else { &formats::SOFTWARE_PREFERENCE }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(wait_for(Duration::from_secs(5), &stopped, || None::<u8>).is_err());
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
    }

    #[test]
    fn a_display_gets_rtp_after_the_full_rtsp_exchange() {
        if gstreamer::init().is_err() || gstreamer::ElementFactory::find("openh264enc").is_none() {
            eprintln!("no openh264enc; skipping");
            return;
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let rtp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        rtp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let rtp_port = rtp.local_addr().unwrap().port();
        let slot = Arc::new(Mutex::new(None));
        let (stop, stopped) = mpsc::channel::<()>();
        let source_slot = Arc::clone(&slot);
        let source = std::thread::spawn(move || {
            let (tcp, display) = listener.accept().unwrap();
            let source = Source { frame_format: "BGRx", size: (800, 500) };
            let preference = &formats::SOFTWARE_PREFERENCE;
            let link = Link { tcp, display, local: Ipv4Addr::LOCALHOST, source, preference };
            let statuses = Mutex::new(Vec::new());
            let outcome = link.serve(&stopped, &source_slot, &|status| statuses.lock().unwrap().push(status));
            (outcome, statuses.into_inner().unwrap())
        });

        let mut sink = Sink { tcp: TcpStream::connect(address).unwrap(), reader: Reader::default() };
        let m1 = sink.next();
        sink.ok(&m1, "");
        let m3 = sink.send("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n");
        assert!(matches!(m3.start, crate::rtsp::Start::Response { code: 200, .. }));
        let m3 = sink.next();
        sink.ok(
            &m3,
            &format!(
                "wfd_video_formats: 00 00 01 01 00000001 00000000 00000000 00 0000 0000 00 none none\r\n\
                 wfd_audio_codecs: none\r\n\
                 wfd_client_rtp_ports: RTP/AVP/UDP;unicast {rtp_port} 0 mode=play\r\n"
            ),
        );
        let m4 = sink.next();
        assert!(m4.parameters()["wfd_video_formats"].starts_with("00 00 01 01 00000001"));
        sink.ok(&m4, "");
        let m5 = sink.next();
        sink.ok(&m5, "");
        let setup = sink.send(&format!(
            "SETUP rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port={rtp_port}\r\n\r\n"
        ));
        assert!(setup.header("Session").is_some());
        let play = sink.send("PLAY rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 3\r\n\r\n");
        assert!(matches!(play.start, crate::rtsp::Start::Response { code: 200, .. }));

        let deadline = Instant::now() + Duration::from_secs(5);
        while slot.lock().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let handle = Handle { stop, stream: Arc::clone(&slot) };
        assert_eq!(handle.mode().map(|mode| (mode.width, mode.height)), Some((640, 480)));
        for _ in 0..15 {
            handle.push_frame(vec![0x40; 800 * 500 * 4]);
            std::thread::sleep(Duration::from_millis(33));
        }
        let mut packet = [0; 2048];
        rtp.recv(&mut packet).unwrap();
        assert_eq!(packet[1] & 0x7f, 33);
        assert_eq!(packet[12], 0x47);

        handle.stop();
        let teardown = sink.next();
        assert_eq!(teardown.parameters()["wfd_trigger_method"], "TEARDOWN");
        let (outcome, statuses) = source.join().unwrap();
        assert_eq!(outcome, Ok(()));
        assert!(matches!(statuses.as_slice(), [Status::Negotiated(_), Status::Streaming(_)]), "{statuses:?}");
    }
}
