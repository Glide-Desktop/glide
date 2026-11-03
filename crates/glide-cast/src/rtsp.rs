use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::formats::{self, Audio, AudioSupport, Video};

const VERSION: &str = "RTSP/1.0";
const WFD_URI: &str = "rtsp://localhost/wfd1.0";
const MAX_MESSAGE: usize = 64 * 1024;
pub(crate) const SESSION_TIMEOUT_SECONDS: u32 = 30;
pub(crate) const REPLY_WAIT: Duration = Duration::from_secs(5);
pub(crate) const NEGOTIATION_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    Request { method: String, uri: String },
    Response { code: u16, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub start: Start,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Message {
    #[must_use]
    pub fn request(method: &str, uri: &str) -> Self {
        Self {
            start: Start::Request { method: method.into(), uri: uri.into() },
            headers: Vec::new(),
            body: String::new(),
        }
    }

    #[must_use]
    pub fn response(code: u16, reason: &str) -> Self {
        Self { start: Start::Response { code, reason: reason.into() }, headers: Vec::new(), body: String::new() }
    }

    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub(crate) fn with_parameters(mut self, body: String) -> Self {
        self.headers.push(("Content-Type".into(), "text/parameters".into()));
        self.body = body;
        self
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub(crate) fn cseq(&self) -> Option<u32> {
        self.header("CSeq")?.trim().parse().ok()
    }

    #[must_use]
    pub(crate) fn parameters(&self) -> HashMap<String, String> {
        self.body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                let (name, value) = line.split_once(':').unwrap_or((line, ""));
                (name.trim().to_ascii_lowercase(), value.trim().to_string())
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut text = match &self.start {
            Start::Request { method, uri } => format!("{method} {uri} {VERSION}\r\n"),
            Start::Response { code, reason } => format!("{VERSION} {code} {reason}\r\n"),
        };
        for (name, value) in &self.headers {
            let _ = write!(text, "{name}: {value}\r\n");
        }
        if !self.body.is_empty() {
            let _ = write!(text, "Content-Length: {}\r\n", self.body.len());
        }
        text.push_str("\r\n");
        text.push_str(&self.body);
        text.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadError {
    TooLarge,
    Malformed(String),
}

#[derive(Debug, Default)]
pub struct Reader {
    buffer: Vec<u8>,
}

impl Reader {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub(crate) fn next_message(&mut self) -> Result<Option<Message>, ReadError> {
        let Some(end) = self.buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
            return if self.buffer.len() > MAX_MESSAGE { Err(ReadError::TooLarge) } else { Ok(None) };
        };
        if end + 4 > MAX_MESSAGE {
            return Err(ReadError::TooLarge);
        }
        let head = String::from_utf8_lossy(&self.buffer[..end]).into_owned();
        let mut lines = head.split("\r\n");
        let start = parse_start(lines.next().unwrap_or_default())?;
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':').map(|(name, value)| (name.trim().into(), value.trim().into())))
            .collect();
        let length = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
            .map_or(Ok(0), |(_, value)| value.parse::<usize>())
            .map_err(|_| ReadError::Malformed("bad Content-Length".into()))?;
        if length > MAX_MESSAGE - end - 4 {
            return Err(ReadError::TooLarge);
        }
        let total = end + 4 + length;
        if self.buffer.len() < total {
            return Ok(None);
        }
        let body = String::from_utf8_lossy(&self.buffer[end + 4..total]).into_owned();
        self.buffer.drain(..total);
        let headers = headers.into_iter().filter(|(name, _)| !name.eq_ignore_ascii_case("Content-Length")).collect();
        Ok(Some(Message { start, headers, body }))
    }
}

fn parse_start(line: &str) -> Result<Start, ReadError> {
    let mut parts = line.splitn(3, ' ');
    let first = parts.next().unwrap_or_default();
    let second = parts.next().ok_or_else(|| ReadError::Malformed(line.into()))?;
    let third = parts.next().unwrap_or_default();
    if first.starts_with("RTSP/") {
        let code = second.parse().map_err(|_| ReadError::Malformed(line.into()))?;
        Ok(Start::Response { code, reason: third.into() })
    } else if third.starts_with("RTSP/") {
        Ok(Start::Request { method: first.to_ascii_uppercase(), uri: second.into() })
    } else {
        Err(ReadError::Malformed(line.into()))
    }
}

fn transport_client_port(transport: &str) -> Option<u16> {
    transport
        .split(';')
        .find_map(|parameter| parameter.trim().strip_prefix("client_port="))
        .and_then(|ports| ports.split('-').next())
        .and_then(|port| port.trim().parse().ok())
        .filter(|port| *port != 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    pub video: Video,
    pub audio: Option<Audio>,
    pub sink_rtp_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Negotiated(Negotiated),
    Play,
    Pause,
    Resume,
    KeyFrame,
    Teardown,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Options,
    Capabilities,
    Choice,
    TriggerSetup,
    KeepAlive,
    TriggerTeardown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Playback {
    Idle,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy)]
struct Outstanding {
    kind: Pending,
    due: Option<Instant>,
}

#[derive(Debug)]
pub struct Session {
    source_ip: String,
    server_port: u16,
    preference: &'static [u8],
    audio: AudioSupport,
    id: String,
    cseq: u32,
    pending: HashMap<u32, Outstanding>,
    asked_capabilities: bool,
    negotiated: Option<Negotiated>,
    set_up: bool,
    playback: Playback,
    play_by: Option<Instant>,
}

impl Session {
    #[must_use]
    pub fn new(source_ip: &str, server_port: u16, preference: &'static [u8], audio: AudioSupport, id: &str) -> Self {
        Self {
            source_ip: source_ip.into(),
            server_port,
            preference,
            audio,
            id: id.into(),
            cseq: 0,
            pending: HashMap::new(),
            asked_capabilities: false,
            negotiated: None,
            set_up: false,
            playback: Playback::Idle,
            play_by: None,
        }
    }

    fn send(&mut self, message: Message, kind: Pending, now: Instant) -> Message {
        self.cseq += 1;
        let due = match kind {
            Pending::KeepAlive | Pending::TriggerTeardown => Some(now + REPLY_WAIT),
            Pending::Options | Pending::Capabilities | Pending::Choice | Pending::TriggerSetup => None,
        };
        self.pending.insert(self.cseq, Outstanding { kind, due });
        message.with("CSeq", self.cseq.to_string())
    }

    pub fn start(&mut self, now: Instant) -> Message {
        self.play_by = Some(now + NEGOTIATION_WAIT);
        self.send(Message::request("OPTIONS", "*").with("Require", "org.wfa.wfd1.0"), Pending::Options, now)
    }

    pub(crate) fn keepalive(&mut self, now: Instant) -> Message {
        let message = Message::request("GET_PARAMETER", WFD_URI).with("Session", self.id.clone());
        self.send(message, Pending::KeepAlive, now)
    }

    pub(crate) fn teardown(&mut self, now: Instant) -> Message {
        let message =
            Message::request("SET_PARAMETER", WFD_URI).with_parameters("wfd_trigger_method: TEARDOWN\r\n".into());
        self.send(message, Pending::TriggerTeardown, now)
    }

    pub fn receive(&mut self, message: &Message, now: Instant) -> (Vec<Message>, Vec<Event>) {
        match &message.start {
            Start::Request { method, .. } => self.answer(method, message, now),
            Start::Response { code, reason } => self.follow_up(message.cseq(), *code, reason, message, now),
        }
    }

    pub fn poll(&mut self, now: Instant) -> Vec<Event> {
        let mut overdue: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, sent)| sent.due.is_some_and(|due| now >= due))
            .map(|(cseq, _)| *cseq)
            .collect();
        overdue.sort_unstable();
        let mut events = Vec::new();
        for cseq in overdue {
            if let Some(Outstanding { kind: Pending::KeepAlive, .. }) = self.pending.remove(&cseq) {
                events.push(Event::Failed("the display stopped answering the keep-alive".into()));
            }
        }
        if self.play_by.is_some_and(|by| now >= by) {
            self.play_by = None;
            events.push(Event::Failed("the display did not start the stream in time".into()));
        }
        events
    }

    fn playing(&mut self, resumed: bool) -> Vec<Event> {
        self.playback = Playback::Playing;
        self.play_by = None;
        self.pending.retain(|_, sent| sent.due.is_some());
        vec![if resumed { Event::Resume } else { Event::Play }]
    }

    fn reply(&self, request: &Message, code: u16, reason: &str) -> Message {
        let mut reply = Message::response(code, reason);
        if let Some(cseq) = request.header("CSeq") {
            reply = reply.with("CSeq", cseq);
        }
        if self.set_up {
            reply = reply.with("Session", format!("{};timeout={SESSION_TIMEOUT_SECONDS}", self.id));
        }
        reply
    }

    fn answer(&mut self, method: &str, request: &Message, now: Instant) -> (Vec<Message>, Vec<Event>) {
        match method {
            "OPTIONS" => {
                let mut out = vec![
                    self.reply(request, 200, "OK")
                        .with("Public", "org.wfa.wfd1.0, SETUP, TEARDOWN, PLAY, PAUSE, GET_PARAMETER, SET_PARAMETER"),
                ];
                if !self.asked_capabilities {
                    self.asked_capabilities = true;
                    let ask = Message::request("GET_PARAMETER", WFD_URI)
                        .with_parameters("wfd_video_formats\r\nwfd_audio_codecs\r\nwfd_client_rtp_ports\r\n".into());
                    out.push(self.send(ask, Pending::Capabilities, now));
                }
                (out, vec![])
            }
            "SETUP" => {
                let Some(negotiated) = self.negotiated.as_mut() else {
                    return (vec![self.reply(request, 455, "Method Not Valid In This State")], vec![]);
                };
                let mut events = Vec::new();
                if let Some(port) = request.header("Transport").and_then(transport_client_port)
                    && port != negotiated.sink_rtp_port
                {
                    negotiated.sink_rtp_port = port;
                    events.push(Event::Negotiated(negotiated.clone()));
                }
                let client = negotiated.sink_rtp_port;
                self.set_up = true;
                let transport = format!("RTP/AVP/UDP;unicast;client_port={client};server_port={}", self.server_port);
                (vec![self.reply(request, 200, "OK").with("Transport", transport)], events)
            }
            "PLAY" if self.set_up => {
                let events = match self.playback {
                    Playback::Idle => self.playing(false),
                    Playback::Paused => self.playing(true),
                    Playback::Playing => vec![],
                };
                (vec![self.reply(request, 200, "OK")], events)
            }
            "PAUSE" if self.set_up && self.playback != Playback::Idle => {
                let events = if self.playback == Playback::Playing { vec![Event::Pause] } else { vec![] };
                self.playback = Playback::Paused;
                (vec![self.reply(request, 200, "OK")], events)
            }
            "PLAY" | "PAUSE" => (vec![self.reply(request, 455, "Method Not Valid In This State")], vec![]),
            "TEARDOWN" => {
                let reply = self.reply(request, 200, "OK");
                self.set_up = false;
                self.playback = Playback::Idle;
                (vec![reply], vec![Event::Teardown])
            }
            "GET_PARAMETER" | "SET_PARAMETER" => {
                let keyframe = request.parameters().contains_key("wfd_idr_request");
                (vec![self.reply(request, 200, "OK")], keyframe.then_some(Event::KeyFrame).into_iter().collect())
            }
            _ => (vec![self.reply(request, 501, "Not Implemented")], vec![]),
        }
    }

    fn follow_up(
        &mut self, cseq: Option<u32>, code: u16, reason: &str, message: &Message, now: Instant,
    ) -> (Vec<Message>, Vec<Event>) {
        let Some(pending) = cseq.and_then(|cseq| self.pending.remove(&cseq)).map(|sent| sent.kind) else {
            return (vec![], vec![]);
        };
        if code != 200 {
            return match pending {
                Pending::KeepAlive | Pending::TriggerTeardown => (vec![], vec![]),
                _ => (vec![], vec![Event::Failed(format!("the display answered {code} {reason} to {pending:?}"))]),
            };
        }
        match pending {
            Pending::Capabilities => self.negotiate(message, now),
            Pending::Choice => {
                let trigger =
                    Message::request("SET_PARAMETER", WFD_URI).with_parameters("wfd_trigger_method: SETUP\r\n".into());
                (vec![self.send(trigger, Pending::TriggerSetup, now)], vec![])
            }
            Pending::Options | Pending::TriggerSetup | Pending::KeepAlive | Pending::TriggerTeardown => {
                (vec![], vec![])
            }
        }
    }

    fn negotiate(&mut self, reply: &Message, now: Instant) -> (Vec<Message>, Vec<Event>) {
        let parameters = reply.parameters();
        let video = parameters
            .get("wfd_video_formats")
            .and_then(|value| formats::parse_video_formats(value))
            .and_then(|formats| formats::choose_video(&formats, self.preference));
        let Some(video) = video else {
            return (vec![], vec![Event::Failed("the display offers no H.264 mode Glide can send".into())]);
        };
        let Some(sink_rtp_port) =
            parameters.get("wfd_client_rtp_ports").and_then(|value| formats::client_rtp_port(value))
        else {
            return (vec![], vec![Event::Failed("the display gave no RTP port".into())]);
        };
        let audio = parameters.get("wfd_audio_codecs").and_then(|value| formats::choose_audio(value, self.audio));
        let mut body = format!("wfd_video_formats: {}\r\n", formats::video_formats_line(video));
        if let Some(audio) = audio {
            let _ = write!(body, "wfd_audio_codecs: {}\r\n", audio.line());
        }
        let _ = write!(body, "wfd_presentation_URL: rtsp://{}/wfd1.0/streamid=0 none\r\n", self.source_ip);
        let _ = write!(body, "wfd_client_rtp_ports: RTP/AVP/UDP;unicast {sink_rtp_port} 0 mode=play\r\n");
        let negotiated = Negotiated { video, audio, sink_rtp_port };
        self.negotiated = Some(negotiated.clone());
        let choice = Message::request("SET_PARAMETER", WFD_URI).with_parameters(body);
        (vec![self.send(choice, Pending::Choice, now)], vec![Event::Negotiated(negotiated)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Message {
        let mut reader = Reader::default();
        reader.push(text.as_bytes());
        reader.next_message().unwrap().unwrap()
    }

    fn reply_to(message: &Message, extra: &str, body: &str) -> Message {
        let cseq = message.cseq().unwrap();
        let length = if body.is_empty() { String::new() } else { format!("Content-Length: {}\r\n", body.len()) };
        parse(&format!("RTSP/1.0 200 OK\r\nCSeq: {cseq}\r\n{extra}{length}\r\n{body}"))
    }

    #[test]
    fn a_message_survives_bytes_split_anywhere() {
        let message = Message::request("SET_PARAMETER", WFD_URI)
            .with("CSeq", "4")
            .with_parameters("wfd_trigger_method: SETUP\r\n".into());
        let bytes = message.to_bytes();
        let mut reader = Reader::default();
        for chunk in bytes.chunks(5) {
            assert_eq!(reader.next_message().unwrap(), None);
            reader.push(chunk);
        }
        let read = reader.next_message().unwrap().unwrap();
        assert_eq!(read.parameters()["wfd_trigger_method"], "SETUP");
        assert_eq!(read.cseq(), Some(4));
        assert_eq!(reader.next_message().unwrap(), None);
    }

    #[test]
    fn a_runaway_header_is_refused() {
        let mut reader = Reader::default();
        reader.push(&vec![b'a'; MAX_MESSAGE + 1]);
        assert_eq!(reader.next_message(), Err(ReadError::TooLarge));
    }

    #[test]
    fn message_limit_includes_terminated_headers_and_body_together() {
        let mut reader = Reader::default();
        reader.push(format!("OPTIONS * RTSP/1.0\r\nX-Padding: {}\r\n\r\n", "a".repeat(MAX_MESSAGE)).as_bytes());
        assert_eq!(reader.next_message(), Err(ReadError::TooLarge));
        let mut reader = Reader::default();
        reader.push(format!("RTSP/1.0 200 OK\r\nContent-Length: {MAX_MESSAGE}\r\n\r\n").as_bytes());
        assert_eq!(reader.next_message(), Err(ReadError::TooLarge));
    }

    const SOUND: AudioSupport = AudioSupport { enabled: true, capture: true, aac_encoder: true };

    fn new_session(audio: AudioSupport) -> Session {
        Session::new("192.168.49.1", 19000, &formats::SOFTWARE_PREFERENCE, audio, "6B8B4567")
    }

    fn secs(count: u64) -> Duration {
        Duration::from_secs(count)
    }

    const CAPABILITIES: &str = "wfd_video_formats: 00 00 02 10 0001ffff 1fffffff 00000fff 00 0000 0000 00 none none\r\n\
                                wfd_audio_codecs: LPCM 00000003 00, AAC 00000001 00\r\n\
                                wfd_client_rtp_ports: RTP/AVP/UDP;unicast 1028 0 mode=play\r\n";

    fn at_capabilities(session: &mut Session, now: Instant) -> Message {
        let _ = session.start(now);
        let (out, _) = session.receive(&parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n"), now);
        out[1].clone()
    }

    fn set_up(session: &mut Session, now: Instant) {
        let m3 = at_capabilities(session, now);
        let (out, _) = session.receive(&reply_to(&m3, "", CAPABILITIES), now);
        let _ = session.receive(&reply_to(&out[0], "", ""), now);
        let setup = parse("SETUP rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\n\r\n");
        let _ = session.receive(&setup, now);
    }

    fn request(method: &str, cseq: u32) -> Message {
        parse(&format!("{method} rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: {cseq}\r\n\r\n"))
    }

    fn code_of(message: &Message) -> u16 {
        let Start::Response { code, .. } = message.start else { panic!("{message:?}") };
        code
    }

    #[test]
    fn a_whole_wfd_session_reaches_play_and_teardown() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let m1 = session.start(now);
        assert!(String::from_utf8(m1.to_bytes()).unwrap().starts_with("OPTIONS * RTSP/1.0\r\nRequire: org.wfa.wfd1.0"));
        assert_eq!(
            session.receive(&reply_to(&m1, "Public: org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER\r\n", ""), now),
            (vec![], vec![])
        );

        let m2 = parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n");
        let (out, _) = session.receive(&m2, now);
        assert_eq!(out[0].header("CSeq"), Some("1"));
        assert!(out[0].header("Public").unwrap().contains("SETUP"));
        let m3 = &out[1];
        assert!(m3.body.contains("wfd_video_formats"));

        let (out, events) = session.receive(&reply_to(m3, "Content-Type: text/parameters\r\n", CAPABILITIES), now);
        let Event::Negotiated(negotiated) = &events[0] else { panic!("{events:?}") };
        assert_eq!(negotiated.sink_rtp_port, 1028);
        assert_eq!(negotiated.audio, Some(Audio::Lpcm48));
        let m4 = &out[0];
        let chosen = m4.parameters();
        assert!(chosen["wfd_video_formats"].starts_with("38 00 02 10 00000080"));
        assert_eq!(chosen["wfd_presentation_url"], "rtsp://192.168.49.1/wfd1.0/streamid=0 none");
        assert_eq!(chosen["wfd_client_rtp_ports"], "RTP/AVP/UDP;unicast 1028 0 mode=play");

        let (out, _) = session.receive(&reply_to(m4, "", ""), now);
        let m5 = &out[0];
        assert_eq!(m5.parameters()["wfd_trigger_method"], "SETUP");
        assert_eq!(session.receive(&reply_to(m5, "", ""), now), (vec![], vec![]));

        let m6 = parse(
            "SETUP rtsp://192.168.49.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port=1028\r\n\r\n",
        );
        let (out, _) = session.receive(&m6, now);
        assert_eq!(out[0].header("Session"), Some("6B8B4567;timeout=30"));
        assert_eq!(out[0].header("Transport"), Some("RTP/AVP/UDP;unicast;client_port=1028;server_port=19000"));

        let m7 = parse("PLAY rtsp://192.168.49.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 3\r\nSession: 6B8B4567\r\n\r\n");
        assert_eq!(session.receive(&m7, now).1, vec![Event::Play]);

        let keepalive = session.keepalive(now);
        assert_eq!(keepalive.header("Session"), Some("6B8B4567"));
        assert_eq!(session.receive(&reply_to(&keepalive, "", ""), now), (vec![], vec![]));

        let idr = parse(
            "SET_PARAMETER rtsp://localhost/wfd1.0 RTSP/1.0\r\nCSeq: 4\r\nContent-Length: 17\r\n\r\nwfd_idr_request\r\n",
        );
        assert_eq!(session.receive(&idr, now).1, vec![Event::KeyFrame]);

        let trigger = session.teardown(now);
        assert_eq!(trigger.parameters()["wfd_trigger_method"], "TEARDOWN");
        let m8 = parse("TEARDOWN rtsp://192.168.49.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 5\r\n\r\n");
        assert_eq!(session.receive(&m8, now).1, vec![Event::Teardown]);
    }

    #[test]
    fn setup_sends_to_the_client_port_the_sink_names_in_its_transport() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let m3 = at_capabilities(&mut session, now);
        let (out, _) = session.receive(&reply_to(&m3, "", CAPABILITIES), now);
        let _ = session.receive(&reply_to(&out[0], "", ""), now);
        let same = parse(
            "SETUP rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port=1028\r\n\r\n",
        );
        assert_eq!(session.receive(&same, now).1, vec![], "the agreed port needs no news");
        let moved = parse(
            "SETUP rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 3\r\nTransport: RTP/AVP/UDP;unicast;client_port=5004-5005\r\n\r\n",
        );
        let (out, events) = session.receive(&moved, now);
        assert_eq!(out[0].header("Transport"), Some("RTP/AVP/UDP;unicast;client_port=5004;server_port=19000"));
        assert!(matches!(events.as_slice(), [Event::Negotiated(choice)] if choice.sink_rtp_port == 5004), "{events:?}");
        assert_eq!(transport_client_port("RTP/AVP/UDP;unicast;client_port=0"), None);
        assert_eq!(transport_client_port("RTP/AVP/UDP;unicast"), None);
    }

    #[test]
    fn setup_and_play_before_negotiation_are_refused() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let setup = parse("SETUP rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 9\r\n\r\n");
        assert!(matches!(session.receive(&setup, now).0[0].start, Start::Response { code: 455, .. }));
        let play = parse("PLAY rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 10\r\n\r\n");
        assert_eq!(session.receive(&play, now).1, vec![]);
    }

    #[test]
    fn a_display_without_a_usable_mode_fails_the_session() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let _ = session.start(now);
        let (out, _) = session.receive(&parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n"), now);
        let m3 = &out[1];
        let (_, events) = session.receive(
            &reply_to(
                m3,
                "",
                "wfd_video_formats: none\r\nwfd_client_rtp_ports: RTP/AVP/UDP;unicast 1028 0 mode=play\r\n",
            ),
            now,
        );
        assert!(matches!(events[0], Event::Failed(_)));
    }

    #[test]
    fn an_error_reply_fails_negotiation_but_not_a_keepalive() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let keepalive = session.keepalive(now);
        let refused = parse(&format!("RTSP/1.0 404 Not Found\r\nCSeq: {}\r\n\r\n", keepalive.cseq().unwrap()));
        assert_eq!(session.receive(&refused, now), (vec![], vec![]));
        let m1 = session.start(now);
        let refused = parse(&format!("RTSP/1.0 551 Option not supported\r\nCSeq: {}\r\n\r\n", m1.cseq().unwrap()));
        assert!(matches!(session.receive(&refused, now).1[0], Event::Failed(_)));
    }

    #[test]
    fn options_twice_asks_for_capabilities_once() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let options = parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
        assert_eq!(session.receive(&options, now).0.len(), 2);
        assert_eq!(session.receive(&options, now).0.len(), 1);
    }

    #[test]
    fn an_unanswered_keepalive_fails_the_session_after_five_seconds() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        set_up(&mut session, now);
        assert_eq!(session.receive(&request("PLAY", 3), now).1, vec![Event::Play]);
        let _ = session.keepalive(now);
        assert_eq!(session.poll(now + secs(4)), vec![]);
        assert_eq!(session.poll(now + REPLY_WAIT.saturating_sub(Duration::from_millis(1))), vec![]);
        let events = session.poll(now + REPLY_WAIT);
        assert!(matches!(events.as_slice(), [Event::Failed(reason)] if reason.contains("keep-alive")), "{events:?}");
        assert_eq!(session.poll(now + secs(60)), vec![], "the expired entry is gone and reports once");
        assert!(session.pending.is_empty());
    }

    #[test]
    fn an_answered_keepalive_leaves_nothing_pending_even_when_the_reply_is_an_error() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        set_up(&mut session, now);
        let _ = session.receive(&request("PLAY", 3), now);
        let ok = session.keepalive(now);
        let refused = session.keepalive(now + secs(20));
        assert_eq!(session.receive(&reply_to(&ok, "", ""), now + secs(1)), (vec![], vec![]));
        let error = parse(&format!("RTSP/1.0 454 Session Not Found\r\nCSeq: {}\r\n\r\n", refused.cseq().unwrap()));
        assert_eq!(session.receive(&error, now + secs(21)), (vec![], vec![]));
        assert_eq!(session.poll(now + secs(600)), vec![], "a sink that answers at all is alive");
        assert!(session.pending.is_empty());
    }

    #[test]
    fn every_unanswered_keepalive_is_reported_and_dropped() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        set_up(&mut session, now);
        let _ = session.receive(&request("PLAY", 3), now);
        for step in 0..3 {
            let _ = session.keepalive(now + secs(step));
        }
        let events = session.poll(now + secs(30));
        assert_eq!(events.len(), 3);
        assert!(session.pending.is_empty());
    }

    #[test]
    fn negotiation_that_never_reaches_play_fails_after_thirty_seconds() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let _ = session.start(now);
        assert_eq!(session.poll(now + NEGOTIATION_WAIT.saturating_sub(Duration::from_millis(1))), vec![]);
        let events = session.poll(now + NEGOTIATION_WAIT);
        assert!(matches!(events.as_slice(), [Event::Failed(reason)] if reason.contains("in time")), "{events:?}");
        assert_eq!(session.poll(now + secs(120)), vec![], "it fails once");
    }

    #[test]
    fn play_ends_the_negotiation_deadline_and_forgets_requests_the_sink_never_answered() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        set_up(&mut session, now);
        assert!(!session.pending.is_empty(), "the sink never answered M5");
        assert_eq!(session.receive(&request("PLAY", 3), now + secs(2)).1, vec![Event::Play]);
        assert!(session.pending.is_empty());
        assert_eq!(session.poll(now + secs(3600)), vec![], "a healthy stream is not killed by an old M5");
    }

    #[test]
    fn an_unanswered_teardown_trigger_expires_without_a_failure() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        let _ = session.teardown(now);
        assert_eq!(session.poll(now + REPLY_WAIT + secs(1)), vec![]);
        assert!(session.pending.is_empty());
    }

    #[test]
    fn pause_is_answered_and_a_later_play_resumes_instead_of_playing_again() {
        let now = Instant::now();
        let mut session = new_session(SOUND);
        set_up(&mut session, now);
        let (out, events) = session.receive(&request("PAUSE", 3), now);
        assert_eq!((code_of(&out[0]), events), (455, vec![]), "nothing plays yet");

        assert_eq!(session.receive(&request("PLAY", 4), now).1, vec![Event::Play]);
        let (out, events) = session.receive(&request("PLAY", 5), now);
        assert_eq!((code_of(&out[0]), events), (200, vec![]), "a second PLAY while playing changes nothing");

        let (out, events) = session.receive(&request("PAUSE", 6), now);
        assert_eq!((code_of(&out[0]), events), (200, vec![Event::Pause]));
        let (out, events) = session.receive(&request("PAUSE", 7), now);
        assert_eq!((code_of(&out[0]), events), (200, vec![]), "pausing twice is harmless");

        let (out, events) = session.receive(&request("PLAY", 8), now);
        assert_eq!((code_of(&out[0]), events), (200, vec![Event::Resume]));
        assert_eq!(session.receive(&request("PLAY", 9), now).1, vec![]);

        let _ = session.receive(&request("TEARDOWN", 10), now);
        assert_eq!(code_of(&session.receive(&request("PAUSE", 11), now).0[0]), 455, "nothing is left to pause");
    }

    #[test]
    fn pause_before_setup_is_refused() {
        let mut session = new_session(SOUND);
        let (out, events) = session.receive(&request("PAUSE", 1), Instant::now());
        assert_eq!((code_of(&out[0]), events), (455, vec![]));
    }

    #[test]
    fn no_method_in_the_public_header_is_answered_with_501() {
        let mut session = new_session(SOUND);
        let (out, _) = session.receive(&parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n"), Instant::now());
        let public = out[0].header("Public").unwrap();
        for method in public.split(',').map(str::trim).skip(1) {
            let (reply, _) = session.receive(&request(method, 9), Instant::now());
            assert_ne!(code_of(&reply[0]), 501, "{method} is advertised in Public but not implemented");
        }
    }

    fn m4_audio(support: AudioSupport, offer: &str) -> (Option<String>, Negotiated) {
        let now = Instant::now();
        let mut session = new_session(support);
        let m3 = at_capabilities(&mut session, now);
        let body = format!(
            "wfd_video_formats: 00 00 02 10 0001ffff 1fffffff 00000fff 00 0000 0000 00 none none\r\n\
             wfd_audio_codecs: {offer}\r\n\
             wfd_client_rtp_ports: RTP/AVP/UDP;unicast 1028 0 mode=play\r\n"
        );
        let (out, events) = session.receive(&reply_to(&m3, "", &body), now);
        let Event::Negotiated(negotiated) = events[0].clone() else { panic!("{events:?}") };
        (out[0].parameters().get("wfd_audio_codecs").cloned(), negotiated)
    }

    #[test]
    fn m4_promises_audio_exactly_when_the_pipeline_will_send_it() {
        let offers =
            ["LPCM 00000003 00, AAC 00000001 00", "LPCM 00000001 00, AAC 00000001 00", "LPCM 00000003 00", "none"];
        for enabled in [false, true] {
            for (aac_encoder, capture) in [(false, false), (false, true), (true, false), (true, true)] {
                for offer in offers {
                    let support = AudioSupport { enabled, capture, aac_encoder };
                    let (promised, negotiated) = m4_audio(support, offer);
                    let available = |name: &str| match name {
                        "avenc_aac" | "aacparse" => aac_encoder,
                        "pipewiresrc" => capture,
                        "fdkaacenc" | "voaacenc" => false,
                        _ => true,
                    };
                    assert_eq!(crate::pipeline::aac_encoder(available), aac_encoder);
                    assert_eq!(crate::pipeline::audio_capture(available), capture);
                    let spec = crate::pipeline::Spec {
                        video: negotiated.video,
                        audio: negotiated.audio,
                        host: "127.0.0.1".into(),
                        port: 1028,
                        frame_format: "BGRx",
                        input: (640, 480),
                        encoder: crate::pipeline::Probed::assumed("openh264enc"),
                    };
                    let sent = crate::pipeline::description(&spec, available).contains("pipewiresrc");
                    assert_eq!(
                        promised.is_some(),
                        sent,
                        "enabled={enabled} aac_encoder={aac_encoder} capture={capture} offer={offer:?}: M4 says {promised:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn turning_sound_off_leaves_the_audio_line_out_of_m4() {
        let (promised, negotiated) =
            m4_audio(AudioSupport { enabled: false, ..SOUND }, "LPCM 00000003 00, AAC 00000001 00");
        assert_eq!((promised, negotiated.audio), (None, None));
        let (promised, negotiated) = m4_audio(SOUND, "LPCM 00000003 00, AAC 00000001 00");
        assert_eq!((promised.as_deref(), negotiated.audio), (Some("LPCM 00000002 00"), Some(Audio::Lpcm48)));
    }
}
