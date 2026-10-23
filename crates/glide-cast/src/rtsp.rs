use std::collections::HashMap;
use std::fmt::Write as _;

use crate::formats::{self, Audio, Video};

const VERSION: &str = "RTSP/1.0";
const WFD_URI: &str = "rtsp://localhost/wfd1.0";
const MAX_MESSAGE: usize = 64 * 1024;
pub const SESSION_TIMEOUT_SECONDS: u32 = 30;

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
    pub fn with_parameters(mut self, body: String) -> Self {
        self.headers.push(("Content-Type".into(), "text/parameters".into()));
        self.body = body;
        self
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub fn cseq(&self) -> Option<u32> {
        self.header("CSeq")?.trim().parse().ok()
    }

    #[must_use]
    pub fn parameters(&self) -> HashMap<String, String> {
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
    pub fn to_bytes(&self) -> Vec<u8> {
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
pub enum ReadError {
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

    pub fn next_message(&mut self) -> Result<Option<Message>, ReadError> {
        let Some(end) = self.buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
            return if self.buffer.len() > MAX_MESSAGE { Err(ReadError::TooLarge) } else { Ok(None) };
        };
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
        if length > MAX_MESSAGE {
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

#[derive(Debug)]
pub struct Session {
    source_ip: String,
    server_port: u16,
    preference: &'static [u8],
    id: String,
    cseq: u32,
    pending: HashMap<u32, Pending>,
    asked_capabilities: bool,
    negotiated: Option<Negotiated>,
    set_up: bool,
}

impl Session {
    #[must_use]
    pub fn new(source_ip: &str, server_port: u16, preference: &'static [u8], id: &str) -> Self {
        Self {
            source_ip: source_ip.into(),
            server_port,
            preference,
            id: id.into(),
            cseq: 0,
            pending: HashMap::new(),
            asked_capabilities: false,
            negotiated: None,
            set_up: false,
        }
    }

    fn send(&mut self, message: Message, pending: Pending) -> Message {
        self.cseq += 1;
        self.pending.insert(self.cseq, pending);
        message.with("CSeq", self.cseq.to_string())
    }

    pub fn start(&mut self) -> Message {
        self.send(Message::request("OPTIONS", "*").with("Require", "org.wfa.wfd1.0"), Pending::Options)
    }

    pub fn keepalive(&mut self) -> Message {
        let message = Message::request("GET_PARAMETER", WFD_URI).with("Session", self.id.clone());
        self.send(message, Pending::KeepAlive)
    }

    pub fn teardown(&mut self) -> Message {
        let message =
            Message::request("SET_PARAMETER", WFD_URI).with_parameters("wfd_trigger_method: TEARDOWN\r\n".into());
        self.send(message, Pending::TriggerTeardown)
    }

    pub fn receive(&mut self, message: &Message) -> (Vec<Message>, Vec<Event>) {
        match &message.start {
            Start::Request { method, .. } => self.answer(method, message),
            Start::Response { code, reason } => self.follow_up(message.cseq(), *code, reason, message),
        }
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

    fn answer(&mut self, method: &str, request: &Message) -> (Vec<Message>, Vec<Event>) {
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
                    out.push(self.send(ask, Pending::Capabilities));
                }
                (out, vec![])
            }
            "SETUP" => {
                let Some(negotiated) = &self.negotiated else {
                    return (vec![self.reply(request, 455, "Method Not Valid In This State")], vec![]);
                };
                let client = negotiated.sink_rtp_port;
                self.set_up = true;
                let transport = format!("RTP/AVP/UDP;unicast;client_port={client};server_port={}", self.server_port);
                (vec![self.reply(request, 200, "OK").with("Transport", transport)], vec![])
            }
            "PLAY" if self.set_up => (vec![self.reply(request, 200, "OK")], vec![Event::Play]),
            "PLAY" => (vec![self.reply(request, 455, "Method Not Valid In This State")], vec![]),
            "TEARDOWN" => {
                let reply = self.reply(request, 200, "OK");
                self.set_up = false;
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
        &mut self, cseq: Option<u32>, code: u16, reason: &str, message: &Message,
    ) -> (Vec<Message>, Vec<Event>) {
        let Some(pending) = cseq.and_then(|cseq| self.pending.remove(&cseq)) else {
            return (vec![], vec![]);
        };
        if code != 200 {
            return match pending {
                Pending::KeepAlive | Pending::TriggerTeardown => (vec![], vec![]),
                _ => (vec![], vec![Event::Failed(format!("the display answered {code} {reason} to {pending:?}"))]),
            };
        }
        match pending {
            Pending::Capabilities => self.negotiate(message),
            Pending::Choice => {
                let trigger =
                    Message::request("SET_PARAMETER", WFD_URI).with_parameters("wfd_trigger_method: SETUP\r\n".into());
                (vec![self.send(trigger, Pending::TriggerSetup)], vec![])
            }
            Pending::Options | Pending::TriggerSetup | Pending::KeepAlive | Pending::TriggerTeardown => {
                (vec![], vec![])
            }
        }
    }

    fn negotiate(&mut self, reply: &Message) -> (Vec<Message>, Vec<Event>) {
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
        let audio = parameters.get("wfd_audio_codecs").and_then(|value| formats::choose_audio(value));
        let mut body = format!("wfd_video_formats: {}\r\n", formats::video_formats_line(video));
        if let Some(audio) = audio {
            let _ = write!(body, "wfd_audio_codecs: {}\r\n", audio.line());
        }
        let _ = write!(body, "wfd_presentation_URL: rtsp://{}/wfd1.0/streamid=0 none\r\n", self.source_ip);
        let _ = write!(body, "wfd_client_rtp_ports: RTP/AVP/UDP;unicast {sink_rtp_port} 0 mode=play\r\n");
        let negotiated = Negotiated { video, audio, sink_rtp_port };
        self.negotiated = Some(negotiated.clone());
        let choice = Message::request("SET_PARAMETER", WFD_URI).with_parameters(body);
        (vec![self.send(choice, Pending::Choice)], vec![Event::Negotiated(negotiated)])
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
    fn a_whole_wfd_session_reaches_play_and_teardown() {
        let mut session = Session::new("192.168.49.1", 19000, &formats::SOFTWARE_PREFERENCE, "6B8B4567");
        let m1 = session.start();
        assert!(String::from_utf8(m1.to_bytes()).unwrap().starts_with("OPTIONS * RTSP/1.0\r\nRequire: org.wfa.wfd1.0"));
        assert_eq!(
            session.receive(&reply_to(&m1, "Public: org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER\r\n", "")),
            (vec![], vec![])
        );

        let m2 = parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n");
        let (out, _) = session.receive(&m2);
        assert_eq!(out[0].header("CSeq"), Some("1"));
        assert!(out[0].header("Public").unwrap().contains("SETUP"));
        let m3 = &out[1];
        assert!(m3.body.contains("wfd_video_formats"));

        let capabilities = "wfd_video_formats: 00 00 02 10 0001ffff 1fffffff 00000fff 00 0000 0000 00 none none\r\n\
                            wfd_audio_codecs: LPCM 00000003 00, AAC 00000001 00\r\n\
                            wfd_client_rtp_ports: RTP/AVP/UDP;unicast 1028 0 mode=play\r\n";
        let (out, events) = session.receive(&reply_to(m3, "Content-Type: text/parameters\r\n", capabilities));
        let Event::Negotiated(negotiated) = &events[0] else { panic!("{events:?}") };
        assert_eq!(negotiated.sink_rtp_port, 1028);
        assert_eq!(negotiated.audio, Some(Audio::Lpcm48));
        let m4 = &out[0];
        let chosen = m4.parameters();
        assert!(chosen["wfd_video_formats"].starts_with("38 00 02 10 00000080"));
        assert_eq!(chosen["wfd_presentation_url"], "rtsp://192.168.49.1/wfd1.0/streamid=0 none");
        assert_eq!(chosen["wfd_client_rtp_ports"], "RTP/AVP/UDP;unicast 1028 0 mode=play");

        let (out, _) = session.receive(&reply_to(m4, "", ""));
        let m5 = &out[0];
        assert_eq!(m5.parameters()["wfd_trigger_method"], "SETUP");
        assert_eq!(session.receive(&reply_to(m5, "", "")), (vec![], vec![]));

        let m6 = parse(
            "SETUP rtsp://192.168.49.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port=1028\r\n\r\n",
        );
        let (out, _) = session.receive(&m6);
        assert_eq!(out[0].header("Session"), Some("6B8B4567;timeout=30"));
        assert_eq!(out[0].header("Transport"), Some("RTP/AVP/UDP;unicast;client_port=1028;server_port=19000"));

        let m7 = parse("PLAY rtsp://192.168.49.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 3\r\nSession: 6B8B4567\r\n\r\n");
        assert_eq!(session.receive(&m7).1, vec![Event::Play]);

        let keepalive = session.keepalive();
        assert_eq!(keepalive.header("Session"), Some("6B8B4567"));
        assert_eq!(session.receive(&reply_to(&keepalive, "", "")), (vec![], vec![]));

        let idr = parse(
            "SET_PARAMETER rtsp://localhost/wfd1.0 RTSP/1.0\r\nCSeq: 4\r\nContent-Length: 17\r\n\r\nwfd_idr_request\r\n",
        );
        assert_eq!(session.receive(&idr).1, vec![Event::KeyFrame]);

        let trigger = session.teardown();
        assert_eq!(trigger.parameters()["wfd_trigger_method"], "TEARDOWN");
        let m8 = parse("TEARDOWN rtsp://192.168.49.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 5\r\n\r\n");
        assert_eq!(session.receive(&m8).1, vec![Event::Teardown]);
    }

    #[test]
    fn setup_and_play_before_negotiation_are_refused() {
        let mut session = Session::new("10.0.0.1", 19000, &formats::SOFTWARE_PREFERENCE, "1");
        let setup = parse("SETUP rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 9\r\n\r\n");
        assert!(matches!(session.receive(&setup).0[0].start, Start::Response { code: 455, .. }));
        let play = parse("PLAY rtsp://x/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 10\r\n\r\n");
        assert_eq!(session.receive(&play).1, vec![]);
    }

    #[test]
    fn a_display_without_a_usable_mode_fails_the_session() {
        let mut session = Session::new("10.0.0.1", 19000, &formats::SOFTWARE_PREFERENCE, "1");
        let _ = session.start();
        let (out, _) = session.receive(&parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n"));
        let m3 = &out[1];
        let (_, events) = session.receive(&reply_to(
            m3,
            "",
            "wfd_video_formats: none\r\nwfd_client_rtp_ports: RTP/AVP/UDP;unicast 1028 0 mode=play\r\n",
        ));
        assert!(matches!(events[0], Event::Failed(_)));
    }

    #[test]
    fn an_error_reply_fails_negotiation_but_not_a_keepalive() {
        let mut session = Session::new("10.0.0.1", 19000, &formats::SOFTWARE_PREFERENCE, "1");
        let keepalive = session.keepalive();
        let refused = parse(&format!("RTSP/1.0 404 Not Found\r\nCSeq: {}\r\n\r\n", keepalive.cseq().unwrap()));
        assert_eq!(session.receive(&refused), (vec![], vec![]));
        let m1 = session.start();
        let refused = parse(&format!("RTSP/1.0 551 Option not supported\r\nCSeq: {}\r\n\r\n", m1.cseq().unwrap()));
        assert!(matches!(session.receive(&refused).1[0], Event::Failed(_)));
    }

    #[test]
    fn options_twice_asks_for_capabilities_once() {
        let mut session = Session::new("10.0.0.1", 19000, &formats::SOFTWARE_PREFERENCE, "1");
        let options = parse("OPTIONS * RTSP/1.0\r\nCSeq: 1\r\n\r\n");
        assert_eq!(session.receive(&options).0.len(), 2);
        assert_eq!(session.receive(&options).0.len(), 1);
    }
}
