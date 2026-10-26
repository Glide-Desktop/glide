use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::api::text;
use super::crypto::{self, Kdf, SymmetricKey};
use super::dates;

pub const EMAIL: &str = "artem@example.com";
pub const PASSWORD: &str = "correct horse battery staple";
const ITERATIONS: u32 = 5_000;
const EPOCH: u64 = 1_767_225_600;

#[derive(Default)]
pub struct State {
    pub ciphers: Vec<Value>,
    pub folders: Vec<Value>,
    pub sends: Vec<Value>,
    pub requests: Vec<String>,
    pub known_devices: Vec<String>,
    pub verify_devices: bool,
    pub emails: Vec<String>,
    tokens: Vec<String>,
    next: u64,
}

pub const DEVICE_OTP: &str = "424242";

impl State {
    fn stamp(&mut self) -> (String, String) {
        self.next += 1;
        (format!("c{}", self.next), dates::format(EPOCH + self.next))
    }

    pub fn touch(&mut self, id: &str) {
        let (_, stamp) = self.stamp();
        if let Some(cipher) = self.ciphers.iter_mut().find(|cipher| text(cipher, "id") == Some(id)) {
            cipher["revisionDate"] = json!(stamp);
        }
    }
}

pub struct Server {
    pub url: String,
    pub state: Arc<Mutex<State>>,
    pub offline: Arc<AtomicBool>,
}

struct Shared {
    state: Arc<Mutex<State>>,
    offline: Arc<AtomicBool>,
    protected: String,
    hash: String,
}

impl Server {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let master =
            crypto::master_key(PASSWORD.as_bytes(), EMAIL, Kdf::Pbkdf2 { iterations: ITERATIONS }).expect("key");
        let user = SymmetricKey::generate().expect("user key");
        let shared = Arc::new(Shared {
            state: Arc::new(Mutex::new(State::default())),
            offline: Arc::new(AtomicBool::new(false)),
            protected: SymmetricKey::stretch(&master).encrypt(&user.to_bytes()).expect("wrap"),
            hash: crypto::master_password_hash(&master, PASSWORD.as_bytes()),
        });
        let server = Self { url, state: Arc::clone(&shared.state), offline: Arc::clone(&shared.offline) };
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || serve(stream, &shared));
            }
        });
        server
    }

    pub fn go_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }
}

fn decode_form(body: &str) -> Vec<(String, String)> {
    let decode = |text: &str| {
        let bytes = text.replace('+', " ").into_bytes();
        let mut out = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%'
                && let Some(value) = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(value);
                index += 3;
            } else {
                out.push(bytes[index]);
                index += 1;
            }
        }
        String::from_utf8(out).unwrap_or_default()
    };
    body.split('&').filter_map(|pair| pair.split_once('=')).map(|(key, value)| (decode(key), decode(value))).collect()
}

fn serve(stream: TcpStream, shared: &Shared) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut length = 0usize;
    let mut bearer = String::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        let (name, value) = header.split_once(':').unwrap_or((&header, ""));
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().unwrap_or(0);
        }
        if name.eq_ignore_ascii_case("authorization") {
            bearer = value.trim().trim_start_matches("Bearer ").to_string();
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    if shared.offline.load(Ordering::SeqCst) {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let body = String::from_utf8_lossy(&body).into_owned();
    let (status, reply) = route(shared, method, path, &body, &bearer);
    let reply = reply.to_string();
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

fn route(shared: &Shared, method: &str, path: &str, body: &str, bearer: &str) -> (u16, Value) {
    let mut state = shared.state.lock().expect("state");
    state.requests.push(format!("{method} {path}"));
    match (method, path) {
        ("POST", "/identity/accounts/prelogin") => (200, json!({ "kdf": 0, "kdfIterations": ITERATIONS })),
        ("POST", "/identity/connect/token") => {
            let form = decode_form(body);
            let get = |key: &str| form.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
            let allowed = match get("grant_type") {
                Some("password") => get("username") == Some(EMAIL) && get("password") == Some(shared.hash.as_str()),
                Some("refresh_token") => get("refresh_token") == Some("refresh"),
                _ => false,
            };
            if !allowed {
                return (400, json!({ "error": "invalid_grant", "error_description": "invalid_username_or_password" }));
            }
            let device = get("deviceIdentifier").unwrap_or_default().to_string();
            if get("grant_type") == Some("password") && state.verify_devices && !state.known_devices.contains(&device) {
                if get("newDeviceOtp") != Some(DEVICE_OTP) {
                    return (
                        400,
                        json!({ "error": "invalid_grant", "error_description": "New device verification required." }),
                    );
                }
                state.known_devices.push(device);
            }
            let token = format!("token-{}", state.tokens.len());
            state.tokens.push(token.clone());
            (
                200,
                json!({ "access_token": token, "refresh_token": "refresh", "expires_in": 3600, "Key": shared.protected }),
            )
        }
        ("POST", "/api/two-factor/send-email-login") => {
            let request: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            if text(&request, "masterPasswordHash") != Some(shared.hash.as_str()) {
                return (400, json!({ "message": "invalid_username_or_password" }));
            }
            let device = text(&request, "deviceIdentifier").unwrap_or_default().to_string();
            state.emails.push(device);
            (200, Value::Null)
        }
        _ if !state.tokens.iter().any(|token| token == bearer) => (401, json!({ "message": "unauthorised" })),
        ("GET", "/api/sync?excludeDomains=true") => (
            200,
            json!({
                "profile": { "privateKey": null, "organizations": [] },
                "folders": state.folders,
                "ciphers": state.ciphers,
                "sends": state.sends,
            }),
        ),
        ("POST", "/api/ciphers") => {
            let (id, stamp) = state.stamp();
            let mut cipher: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            cipher["id"] = json!(id);
            cipher["revisionDate"] = json!(stamp);
            state.ciphers.push(cipher.clone());
            (200, cipher)
        }
        ("POST", "/api/folders") => {
            let (id, stamp) = state.stamp();
            let mut folder: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            folder["id"] = json!(id);
            folder["revisionDate"] = json!(stamp);
            state.folders.push(folder.clone());
            (200, folder)
        }
        ("POST", "/api/sends") => {
            let (id, stamp) = state.stamp();
            let mut send: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            send["id"] = json!(id);
            send["accessId"] = json!(format!("access-{id}"));
            send["revisionDate"] = json!(stamp);
            state.sends.push(send.clone());
            (200, send)
        }
        ("DELETE", path) if path.starts_with("/api/sends/") => {
            let id = path.trim_start_matches("/api/sends/");
            state.sends.retain(|send| text(send, "id") != Some(id));
            (200, Value::Null)
        }
        (method, path) => cipher_route(&mut state, method, path, body),
    }
}

fn cipher_route(state: &mut State, method: &str, path: &str, body: &str) -> (u16, Value) {
    let Some(rest) = path.strip_prefix("/api/ciphers/") else {
        return (404, json!({ "message": "no such route" }));
    };
    let (id, action) = rest.split_once('/').unwrap_or((rest, ""));
    let Some(index) = state.ciphers.iter().position(|cipher| text(cipher, "id") == Some(id)) else {
        return (404, json!({ "message": "no such item" }));
    };
    let (_, stamp) = state.stamp();
    match (method, action) {
        ("PUT", "") => {
            let mut cipher: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            cipher["id"] = json!(id);
            cipher["revisionDate"] = json!(stamp);
            state.ciphers[index] = cipher.clone();
            (200, cipher)
        }
        ("PUT", "delete") => {
            state.ciphers[index]["deletedDate"] = json!(stamp);
            (200, Value::Null)
        }
        ("PUT", "restore") => {
            state.ciphers[index]["deletedDate"] = Value::Null;
            (200, state.ciphers[index].clone())
        }
        ("DELETE", "") => {
            state.ciphers.remove(index);
            (200, Value::Null)
        }
        _ => (404, json!({ "message": "no such route" })),
    }
}
