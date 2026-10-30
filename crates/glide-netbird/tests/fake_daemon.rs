use std::{
    future::pending,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, SystemTime},
};

use bytes::{BufMut, Bytes, BytesMut};
use glide_netbird::{Daemon, Feed, State, pb};
use h2::server::SendResponse;
use http::{HeaderMap, Request, Response};
use prost::Message;
use tokio::net::UnixListener;

const UNIMPLEMENTED: i32 = 12;
const SUBSCRIBE_STATUS: &str = "/daemon.DaemonService/SubscribeStatus";
const SUBSCRIBE_EVENTS: &str = "/daemon.DaemonService/SubscribeEvents";
const STATUS: &str = "/daemon.DaemonService/Status";
const GET_EVENTS: &str = "/daemon.DaemonService/GetEvents";
const LONGER_THAN_ANY_SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(11);

enum Reply {
    Refuse(i32),
    Messages { after: Duration, messages: Vec<Bytes>, close: bool },
    Hold,
}

type Script = Arc<dyn Fn(&str, usize) -> Reply + Send + Sync>;
type Calls = Arc<Mutex<Vec<String>>>;

struct FakeDaemon {
    path: PathBuf,
    calls: Calls,
    _runtime: tokio::runtime::Runtime,
}

impl FakeDaemon {
    fn start(script: impl Fn(&str, usize) -> Reply + Send + Sync + 'static) -> Self {
        static SOCKETS: AtomicUsize = AtomicUsize::new(0);
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("rt");
        let path = std::env::temp_dir().join(format!(
            "glide-netbird-fake-{}-{}.sock",
            std::process::id(),
            SOCKETS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let listener = {
            let _enter = runtime.enter();
            UnixListener::bind(&path).expect("bind the fake daemon socket")
        };
        let calls = Calls::default();
        runtime.spawn(accept(listener, Arc::new(script), Arc::clone(&calls)));
        Self { path, calls, _runtime: runtime }
    }

    fn calls_to(&self, path: &str) -> usize {
        self.calls.lock().expect("calls").iter().filter(|call| *call == path).count()
    }
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn accept(listener: UnixListener, script: Script, calls: Calls) {
    while let Ok((socket, _)) = listener.accept().await {
        let (script, calls) = (Arc::clone(&script), Arc::clone(&calls));
        tokio::spawn(async move {
            let Ok(mut connection) = h2::server::handshake(socket).await else {
                return;
            };
            while let Some(Ok((request, respond))) = connection.accept().await {
                tokio::spawn(serve(request, respond, Arc::clone(&script), Arc::clone(&calls)));
            }
        });
    }
}

async fn serve(request: Request<h2::RecvStream>, respond: SendResponse<Bytes>, script: Script, calls: Calls) {
    let path = request.uri().path().to_string();
    let nth = {
        let mut calls = calls.lock().expect("calls");
        let nth = calls.iter().filter(|call| **call == path).count();
        calls.push(path.clone());
        nth
    };
    let mut body = request.into_body();
    while let Some(chunk) = body.data().await {
        let Ok(chunk) = chunk else {
            return;
        };
        let _ = body.flow_control().release_capacity(chunk.len());
    }
    send(respond, script(&path, nth)).await;
}

fn head(status: Option<i32>) -> Response<()> {
    let mut head = Response::builder().status(200).header("content-type", "application/grpc");
    if let Some(status) = status {
        head = head.header("grpc-status", status.to_string());
    }
    head.body(()).expect("response head")
}

async fn send(mut respond: SendResponse<Bytes>, reply: Reply) {
    match reply {
        Reply::Refuse(code) => {
            let _ = respond.send_response(head(Some(code)), true);
        }
        Reply::Messages { after, messages, close } => {
            tokio::time::sleep(after).await;
            let Ok(mut stream) = respond.send_response(head(None), false) else {
                return;
            };
            for message in messages {
                if stream.send_data(message, false).is_err() {
                    return;
                }
            }
            if close {
                let mut trailers = HeaderMap::new();
                trailers.insert("grpc-status", "0".parse().expect("header value"));
                let _ = stream.send_trailers(trailers);
            } else {
                pending::<()>().await;
            }
        }
        Reply::Hold => pending::<()>().await,
    }
}

fn frame(message: &impl Message) -> Bytes {
    let body = message.encode_to_vec();
    let mut framed = BytesMut::with_capacity(5 + body.len());
    framed.put_u8(0);
    framed.put_u32(u32::try_from(body.len()).expect("a small message"));
    framed.put_slice(&body);
    framed.freeze()
}

fn status(text: &str) -> Bytes {
    frame(&pb::StatusResponse { status: text.to_string(), ..pb::StatusResponse::default() })
}

fn event(id: &str, at: SystemTime) -> pb::SystemEvent {
    let since_epoch = at.duration_since(SystemTime::UNIX_EPOCH).expect("after the epoch");
    pb::SystemEvent {
        id: id.to_string(),
        timestamp: Some(prost_types::Timestamp {
            seconds: i64::try_from(since_epoch.as_secs()).expect("seconds fit"),
            nanos: 0,
        }),
        ..pb::SystemEvent::default()
    }
}

fn watch(daemon: &FakeDaemon) -> (mpsc::Receiver<Feed>, glide_netbird::Watch) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let watch = Daemon::at(&daemon.path)
        .expect("a lazy channel")
        .watch(move |feed| {
            let _ = tx.lock().expect("sink").send(feed);
        })
        .expect("watch");
    (rx, watch)
}

fn next_status(feeds: &mpsc::Receiver<Feed>, within: Duration) -> glide_netbird::Status {
    let deadline = std::time::Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match feeds.recv_timeout(left).expect("a status arrives") {
            Feed::Status(status) => return *status,
            Feed::Event(_) => {}
        }
    }
}

fn next_event(feeds: &mpsc::Receiver<Feed>, within: Duration) -> glide_netbird::Event {
    let deadline = std::time::Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match feeds.recv_timeout(left).expect("an event arrives") {
            Feed::Event(event) => return event,
            Feed::Status(_) => {}
        }
    }
}

#[test]
fn a_daemon_without_subscribe_status_is_followed_through_the_unary_status_call() {
    let daemon = FakeDaemon::start(|path, nth| match (path, nth) {
        (SUBSCRIBE_STATUS, _) => Reply::Refuse(UNIMPLEMENTED),
        (STATUS, 0) => Reply::Messages { after: Duration::ZERO, messages: vec![status("Connecting")], close: true },
        (STATUS, _) => Reply::Messages { after: Duration::ZERO, messages: vec![status("Connected")], close: true },
        _ => Reply::Hold,
    });
    let (feeds, _watch) = watch(&daemon);

    assert_eq!(next_status(&feeds, Duration::from_secs(5)).state, State::Connecting);
    assert_eq!(
        next_status(&feeds, Duration::from_secs(8)).state,
        State::Connected,
        "the poll goes on after the first answer"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while daemon.calls_to(STATUS) < 3 {
        assert!(std::time::Instant::now() < deadline, "the poll stopped after {} calls", daemon.calls_to(STATUS));
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !std::iter::from_fn(|| feeds.try_recv().ok()).any(|feed| matches!(feed, Feed::Status(_))),
        "an unchanged answer is not fed again"
    );
    assert_eq!(daemon.calls_to(SUBSCRIBE_STATUS), 1, "an unimplemented stream is not retried while polling works");
}

#[test]
fn an_idle_events_subscription_is_not_cancelled_and_resubscribed() {
    let daemon = FakeDaemon::start(|path, _| match path {
        SUBSCRIBE_STATUS => {
            Reply::Messages { after: Duration::ZERO, messages: vec![status("Connected")], close: false }
        }
        SUBSCRIBE_EVENTS => Reply::Messages {
            after: LONGER_THAN_ANY_SUBSCRIBE_TIMEOUT,
            messages: vec![frame(&event("late", SystemTime::now()))],
            close: false,
        },
        _ => Reply::Hold,
    });
    let (feeds, _watch) = watch(&daemon);

    let late = next_event(&feeds, LONGER_THAN_ANY_SUBSCRIBE_TIMEOUT + Duration::from_secs(8));
    assert_eq!(late.id, "late");
    assert_eq!(daemon.calls_to(SUBSCRIBE_EVENTS), 1, "the subscription must wait for the daemon's first event");
}

#[test]
fn events_published_while_the_subscription_was_down_are_back_filled_once() {
    let ahead = SystemTime::now() + Duration::from_hours(1);
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let daemon = FakeDaemon::start(move |path, nth| match (path, nth) {
        (SUBSCRIBE_STATUS, _) => {
            Reply::Messages { after: Duration::ZERO, messages: vec![status("Connected")], close: false }
        }
        (SUBSCRIBE_EVENTS, 0) => {
            Reply::Messages { after: Duration::ZERO, messages: vec![frame(&event("first", long_ago))], close: true }
        }
        (GET_EVENTS, _) => Reply::Messages {
            after: Duration::ZERO,
            messages: vec![frame(&pb::GetEventsResponse {
                events: vec![event("old", long_ago), event("missed", ahead)],
            })],
            close: true,
        },
        _ => Reply::Hold,
    });
    let (feeds, _watch) = watch(&daemon);

    assert_eq!(next_event(&feeds, Duration::from_secs(5)).id, "first");
    assert_eq!(next_event(&feeds, Duration::from_secs(8)).id, "missed");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(daemon.calls_to(GET_EVENTS), 1, "the back-fill runs once per outage");
    assert_eq!(daemon.calls_to(SUBSCRIBE_EVENTS), 2);
    assert!(
        !std::iter::from_fn(|| feeds.try_recv().ok())
            .any(|feed| matches!(feed, Feed::Event(event) if event.id == "old")),
        "history from before the outage is not replayed"
    );
}
