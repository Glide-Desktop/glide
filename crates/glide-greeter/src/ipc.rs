use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
};

use serde::{Deserialize, Serialize};

const MAX_MESSAGE_LEN: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "create_session")]
    CreateSession { username: String },
    #[serde(rename = "post_auth_message_response")]
    PostAuthMessageResponse {
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<String>,
    },
    #[serde(rename = "start_session")]
    StartSession {
        cmd: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<Vec<String>>,
    },
    #[serde(rename = "cancel_session")]
    CancelSession,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "error")]
    Error { error_type: String, description: String },
    #[serde(rename = "auth_message")]
    AuthMessage { auth_message_type: AuthMessageType, auth_message: String },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMessageType {
    Visible,
    Secret,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub enum GreetdCommand {
    CreateSession(String),
    Respond(Option<String>),
    StartSession { cmd: Vec<String>, env: Vec<String> },
    CancelSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GreetdReply {
    Connected,
    Prompt { message_type: AuthMessageType, message: String },
    Authenticated,
    SessionStarted,
    Error { auth: bool, message: String },
    Disconnected,
}

pub struct GreetdHandle {
    sender: mpsc::Sender<GreetdCommand>,
}

impl GreetdHandle {
    pub fn send(&self, cmd: GreetdCommand) {
        let _ = self.sender.send(cmd);
    }
}

pub fn encode_request(req: &Request) -> anyhow::Result<Vec<u8>> {
    let json = serde_json::to_vec(req)?;
    let len = u32::try_from(json.len())?.to_ne_bytes();
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&json);
    Ok(out)
}

pub fn decode_response(mut reader: impl Read) -> anyhow::Result<Response> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = usize::try_from(u32::from_ne_bytes(len_bytes))?;
    anyhow::ensure!(len <= MAX_MESSAGE_LEN, "greetd message of {len} bytes");
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    Ok(serde_json::from_slice(&payload)?)
}

fn round_trip(stream: &mut UnixStream, request: &Request) -> anyhow::Result<Response> {
    stream.write_all(&encode_request(request)?)?;
    decode_response(&mut *stream)
}

#[must_use]
pub fn reply_for(request: &Request, response: Response) -> GreetdReply {
    match response {
        Response::Success => match request {
            Request::StartSession { .. } => GreetdReply::SessionStarted,
            _ => GreetdReply::Authenticated,
        },
        Response::AuthMessage { auth_message_type, auth_message } => {
            GreetdReply::Prompt { message_type: auth_message_type, message: auth_message }
        }
        Response::Error { error_type, description } => {
            GreetdReply::Error { auth: error_type == "auth_error", message: description }
        }
    }
}

pub fn start<F>(sink: F) -> Option<GreetdHandle>
where
    F: Fn(GreetdReply) + Send + 'static,
{
    let Ok(sock_path) = std::env::var("GREETD_SOCK") else {
        tracing::info!("no GREETD_SOCK in environment; running greeter in standalone preview mode");
        return None;
    };

    let (tx, rx) = mpsc::channel::<GreetdCommand>();

    let spawned = thread::Builder::new().name("glide-greetd-ipc".into()).spawn(move || {
        let mut stream = match UnixStream::connect(&sock_path) {
            Ok(stream) => {
                sink(GreetdReply::Connected);
                stream
            }
            Err(err) => {
                tracing::warn!(?err, %sock_path, "failed to connect to greetd socket");
                sink(GreetdReply::Disconnected);
                return;
            }
        };

        let mut session_open = false;
        while let Ok(cmd) = rx.recv() {
            if session_open
                && matches!(cmd, GreetdCommand::CreateSession(_))
                && let Err(err) = round_trip(&mut stream, &Request::CancelSession)
            {
                tracing::warn!(?err, "greetd cancel_session failed");
                sink(GreetdReply::Disconnected);
                return;
            }

            let request = match cmd {
                GreetdCommand::CreateSession(username) => Request::CreateSession { username },
                GreetdCommand::Respond(response) => Request::PostAuthMessageResponse { response },
                GreetdCommand::StartSession { cmd, env } => Request::StartSession { cmd, env: Some(env) },
                GreetdCommand::CancelSession => Request::CancelSession,
            };

            let response = match round_trip(&mut stream, &request) {
                Ok(response) => response,
                Err(err) => {
                    tracing::warn!(?err, "greetd connection failed");
                    sink(GreetdReply::Disconnected);
                    return;
                }
            };

            if matches!(response, Response::Error { .. }) && !matches!(request, Request::CancelSession) {
                let _ = round_trip(&mut stream, &Request::CancelSession);
            }
            session_open = !matches!(
                (&request, &response),
                (Request::CancelSession, _)
                    | (_, Response::Error { .. })
                    | (Request::StartSession { .. }, Response::Success)
            );

            if !matches!(request, Request::CancelSession) {
                sink(reply_for(&request, response));
            }
        }
    });

    match spawned {
        Ok(_) => Some(GreetdHandle { sender: tx }),
        Err(err) => {
            tracing::error!(?err, "failed to spawn the greetd worker");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(json: &[u8]) -> Vec<u8> {
        let mut data = u32::try_from(json.len()).unwrap().to_ne_bytes().to_vec();
        data.extend_from_slice(json);
        data
    }

    #[test]
    fn create_session_is_length_prefixed_json() {
        let req = Request::CreateSession { username: "artem".into() };
        let encoded = encode_request(&req).unwrap();
        let len = u32::from_ne_bytes(encoded[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, encoded.len() - 4);
        let s = std::str::from_utf8(&encoded[4..]).unwrap();
        assert!(s.contains("\"type\":\"create_session\""));
        assert!(s.contains("\"username\":\"artem\""));
    }

    #[test]
    fn a_null_response_to_an_info_prompt_omits_the_field() {
        let encoded = encode_request(&Request::PostAuthMessageResponse { response: None }).unwrap();
        let s = std::str::from_utf8(&encoded[4..]).unwrap();
        assert_eq!(s, r#"{"type":"post_auth_message_response"}"#);
    }

    #[test]
    fn start_session_carries_the_session_environment() {
        let req = Request::StartSession {
            cmd: vec!["glide-session".into()],
            env: Some(vec!["XDG_SESSION_TYPE=wayland".into()]),
        };
        let encoded = encode_request(&req).unwrap();
        let s = std::str::from_utf8(&encoded[4..]).unwrap();
        assert!(s.contains(r#""env":["XDG_SESSION_TYPE=wayland"]"#));
    }

    #[test]
    fn responses_decode() {
        let resp = decode_response(&framed(br#"{"type":"success"}"#)[..]).unwrap();
        assert_eq!(resp, Response::Success);

        let resp = decode_response(
            &framed(br#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#)[..],
        )
        .unwrap();
        assert_eq!(
            resp,
            Response::AuthMessage { auth_message_type: AuthMessageType::Secret, auth_message: "Password: ".into() }
        );
    }

    #[test]
    fn oversized_frames_are_rejected_instead_of_allocated() {
        let data = u32::MAX.to_ne_bytes();
        assert!(decode_response(&data[..]).is_err());
    }

    #[test]
    fn auth_errors_are_told_apart_from_other_errors() {
        let create = Request::CreateSession { username: "a".into() };
        let auth = reply_for(
            &create,
            Response::Error { error_type: "auth_error".into(), description: "pam_authenticate: AUTH_ERR".into() },
        );
        assert!(matches!(auth, GreetdReply::Error { auth: true, .. }));
        let other = reply_for(&create, Response::Error { error_type: "error".into(), description: "boom".into() });
        assert!(matches!(other, GreetdReply::Error { auth: false, .. }));
    }

    #[test]
    fn success_means_authenticated_except_after_start_session() {
        let start = Request::StartSession { cmd: vec![], env: None };
        assert_eq!(reply_for(&start, Response::Success), GreetdReply::SessionStarted);
        let respond = Request::PostAuthMessageResponse { response: None };
        assert_eq!(reply_for(&respond, Response::Success), GreetdReply::Authenticated);
    }
}
