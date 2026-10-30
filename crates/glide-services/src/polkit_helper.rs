use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};

use zeroize::Zeroizing;

pub const SOCKET: &str = "/run/polkit/agent-helper.socket";
const HELPERS: &[&str] = &[
    "/usr/lib/polkit-1/polkit-agent-helper-1",
    "/usr/libexec/polkit-agent-helper-1",
    "/usr/lib/policykit-1/polkit-agent-helper-1",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Ask { echo: bool },
    Error(String),
    Info(String),
    Success,
    Failure,
}

#[must_use]
pub fn parse(line: &str) -> Option<Line> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    match line {
        "SUCCESS" => return Some(Line::Success),
        "FAILURE" => return Some(Line::Failure),
        _ => {}
    }
    let (kind, rest) = line.split_once(' ')?;
    Some(match kind {
        "PAM_PROMPT_ECHO_OFF" => Line::Ask { echo: false },
        "PAM_PROMPT_ECHO_ON" => Line::Ask { echo: true },
        "PAM_ERROR_MSG" => Line::Error(compress(rest)),
        "PAM_TEXT_INFO" => Line::Info(compress(rest)),
        _ => return None,
    })
}

#[must_use]
pub fn compress(escaped: &str) -> String {
    let bytes = escaped.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let Some(&next) = bytes.get(index) else {
            break;
        };
        index += 1;
        match next {
            b'0'..=b'7' => {
                let mut value = u32::from(next - b'0');
                for _ in 0..2 {
                    match bytes.get(index) {
                        Some(&digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            index += 1;
                        }
                        _ => break,
                    }
                }
                out.push((value & 0xff) as u8);
            }
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
    Broken(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub outcome: Outcome,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

enum Abort {
    Socket(UnixStream),
    Process(u32),
}

#[derive(Default)]
struct Shared {
    cancelled: bool,
    abort: Option<Abort>,
}

#[derive(Clone, Default)]
pub struct Cancel(Arc<Mutex<Shared>>);

impl Cancel {
    pub fn cancel(&self) {
        let mut shared = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.cancelled = true;
        match shared.abort.take() {
            Some(Abort::Socket(stream)) => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
            Some(Abort::Process(pid)) => {
                if let Ok(pid) = libc::pid_t::try_from(pid) {
                    // SAFETY: `kill` takes plain integers; the pid belongs to a child this thread has not reaped yet,
                    // because the abort handle is removed under the same lock before it is waited for.
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                }
            }
            None => {}
        }
    }

    fn arm(&self, abort: Abort) -> bool {
        let mut shared = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.cancelled {
            return false;
        }
        shared.abort = Some(abort);
        true
    }

    fn disarm(&self) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).abort = None;
    }
}

struct Link {
    reader: Box<dyn BufRead + Send>,
    writer: Box<dyn Write + Send>,
    child: Option<Child>,
    socket: Option<UnixStream>,
}

fn helper() -> Option<&'static str> {
    HELPERS.iter().copied().find(|path| Path::new(path).exists())
}

fn connect(user: &str, cookie: &str, cancel: &Cancel) -> std::io::Result<Link> {
    if Path::new(SOCKET).exists() {
        match UnixStream::connect(SOCKET) {
            Ok(stream) => {
                let mut writer = stream.try_clone()?;
                if !cancel.arm(Abort::Socket(stream.try_clone()?)) {
                    return Err(std::io::Error::other("cancelled"));
                }
                let socket = stream.try_clone()?;
                writer.write_all(format!("{user}\n{cookie}\n").as_bytes())?;
                return Ok(Link {
                    reader: Box::new(BufReader::new(stream)),
                    writer: Box::new(writer),
                    child: None,
                    socket: Some(socket),
                });
            }
            Err(err) => tracing::info!(%err, "the polkit helper socket refused; spawning the helper instead"),
        }
    }
    let path = helper().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no polkit-agent-helper-1"))?;
    let mut child =
        Command::new(path).arg(user).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other("the helper has no pipes"));
    };
    if !cancel.arm(Abort::Process(child.id())) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other("cancelled"));
    }
    if let Err(err) = stdin.write_all(format!("{cookie}\n").as_bytes()) {
        cancel.disarm();
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }
    Ok(Link { reader: Box::new(BufReader::new(stdout)), writer: Box::new(stdin), child: Some(child), socket: None })
}

fn converse(link: &mut Link, password: &Zeroizing<String>) -> Report {
    let mut report = Report { outcome: Outcome::Broken(String::new()), errors: Vec::new(), notes: Vec::new() };
    let mut answered = false;
    let mut received = String::new();
    loop {
        received.clear();
        match link.reader.read_line(&mut received) {
            Ok(0) => {
                report.outcome = Outcome::Broken("the helper closed the conversation".into());
                return report;
            }
            Ok(_) => {}
            Err(err) => {
                report.outcome = Outcome::Broken(format!("could not read from the helper: {err}"));
                return report;
            }
        }
        match parse(&received) {
            Some(Line::Ask { .. }) if !answered => {
                answered = true;
                let mut reply = Zeroizing::new(Vec::with_capacity(password.len() + 1));
                reply.extend_from_slice(password.as_bytes());
                reply.push(b'\n');
                if let Err(err) = link.writer.write_all(&reply).and_then(|()| link.writer.flush()) {
                    report.outcome = Outcome::Broken(format!("could not answer the helper: {err}"));
                    return report;
                }
            }
            Some(Line::Ask { .. }) => {
                report.outcome = Outcome::Broken("the login stack asked a second question".into());
                return report;
            }
            Some(Line::Error(text)) => report.errors.push(text),
            Some(Line::Info(text)) => report.notes.push(text),
            Some(Line::Success) => {
                report.outcome = Outcome::Success;
                return report;
            }
            Some(Line::Failure) => {
                report.outcome = Outcome::Failure;
                return report;
            }
            None => tracing::debug!("ignoring an unknown line from the polkit helper"),
        }
    }
}

fn finish(link: Link) {
    drop(link.writer);
    if let Some(socket) = &link.socket {
        let _ = socket.shutdown(std::net::Shutdown::Write);
    }
    let mut reader = link.reader;
    let _ = std::io::copy(&mut reader.by_ref().take(64 * 1024), &mut std::io::sink());
    if let Some(mut child) = link.child {
        let _ = child.wait();
    }
}

#[must_use]
pub fn authenticate(user: &str, cookie: &str, password: &Zeroizing<String>, cancel: &Cancel) -> Report {
    let mut link = match connect(user, cookie, cancel) {
        Ok(link) => link,
        Err(err) => {
            return Report {
                outcome: Outcome::Broken(format!("could not reach polkit-agent-helper-1: {err}")),
                errors: Vec::new(),
                notes: Vec::new(),
            };
        }
    };
    let report = converse(&mut link, password);
    cancel.disarm();
    finish(link);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_lines_parse_and_their_messages_are_unescaped() {
        assert_eq!(parse("PAM_PROMPT_ECHO_OFF Password: \n"), Some(Line::Ask { echo: false }));
        assert_eq!(parse("PAM_PROMPT_ECHO_ON Login:"), Some(Line::Ask { echo: true }));
        assert_eq!(parse("SUCCESS\n"), Some(Line::Success));
        assert_eq!(parse("FAILURE"), Some(Line::Failure));
        assert_eq!(
            parse("PAM_TEXT_INFO The account is locked due to 3 failed logins.\\n(10 minutes left to unlock)\n"),
            Some(Line::Info("The account is locked due to 3 failed logins.\n(10 minutes left to unlock)".into()))
        );
        assert_eq!(parse("PAM_ERROR_MSG say \\\"hi\\\"\\t\\\\"), Some(Line::Error("say \"hi\"\t\\".into())));
        assert_eq!(parse("SUCCESSFUL"), None);
        assert_eq!(parse("HELLO there"), None);
    }

    #[test]
    fn compress_undoes_g_strescape_including_octal_utf8() {
        assert_eq!(compress("\\320\\237\\321\\200\\320\\270"), "При");
        assert_eq!(compress("a\\nb\\rc\\bd\\fe\\vf"), "a\nb\rc\u{8}d\u{c}e\u{b}f");
        assert_eq!(compress("trailing\\"), "trailing");
        assert_eq!(compress("\\101BC"), "ABC");
    }

    struct Script(std::io::Cursor<Vec<u8>>);

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }

    #[derive(Clone, Default)]
    struct Written(Arc<Mutex<Vec<u8>>>);

    impl Write for Written {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap_or_else(PoisonError::into_inner).extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn scripted(lines: &str) -> (Link, Written) {
        let written = Written::default();
        let reader = BufReader::new(Script(std::io::Cursor::new(lines.as_bytes().to_vec())));
        (Link { reader: Box::new(reader), writer: Box::new(written.clone()), child: None, socket: None }, written)
    }

    #[test]
    fn a_helper_socket_left_waiting_for_a_second_answer_is_released_and_the_attempt_ends() {
        let (ours, theirs) = UnixStream::pair().expect("a socket pair");
        let helper = std::thread::spawn(move || {
            let mut theirs = theirs;
            theirs.write_all(b"PAM_PROMPT_ECHO_OFF Password: \n").expect("first question");
            let mut answer = [0u8; 64];
            let _ = theirs.read(&mut answer);
            theirs.write_all(b"PAM_PROMPT_ECHO_ON Code: \n").expect("second question");
            let _ = std::io::copy(&mut theirs, &mut std::io::sink());
        });
        let link = Link {
            reader: Box::new(BufReader::new(ours.try_clone().expect("reader"))),
            writer: Box::new(ours.try_clone().expect("writer")),
            child: None,
            socket: Some(ours),
        };
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut link = link;
            let outcome = converse(&mut link, &Zeroizing::new(String::from("secret"))).outcome;
            finish(link);
            let _ = done.send(outcome);
        });
        let outcome = finished.recv_timeout(std::time::Duration::from_secs(5)).expect("the attempt ends");
        assert!(matches!(outcome, Outcome::Broken(_)));
        helper.join().expect("the helper sees the end of the conversation");
    }

    #[test]
    fn the_password_is_sent_raw_once_and_the_last_line_decides() {
        let password = Zeroizing::new(String::from("pa\\ss word"));
        let (mut link, written) = scripted("PAM_TEXT_INFO hello\nPAM_PROMPT_ECHO_OFF Password: \nSUCCESS\n");
        let report = converse(&mut link, &password);
        assert_eq!(report.outcome, Outcome::Success);
        assert_eq!(report.notes, ["hello"]);
        assert_eq!(written.0.lock().unwrap_or_else(PoisonError::into_inner).as_slice(), b"pa\\ss word\n");

        let (mut link, _) = scripted("PAM_PROMPT_ECHO_OFF Password: \nPAM_ERROR_MSG nope\nFAILURE\n");
        let report = converse(&mut link, &password);
        assert_eq!((report.outcome, report.errors), (Outcome::Failure, vec!["nope".to_string()]));

        let (mut link, written) = scripted("PAM_PROMPT_ECHO_OFF Password: \nPAM_PROMPT_ECHO_OFF Code: \n");
        assert!(
            matches!(converse(&mut link, &password).outcome, Outcome::Broken(_)),
            "a second question is not guessed"
        );
        assert_eq!(written.0.lock().unwrap_or_else(PoisonError::into_inner).as_slice(), b"pa\\ss word\n");

        let (mut link, _) = scripted("PAM_PROMPT_ECHO_OFF Password: \n");
        assert!(matches!(converse(&mut link, &password).outcome, Outcome::Broken(_)), "a closed helper is no success");
    }

    #[test]
    #[ignore = "talks to the system polkit helper socket"]
    fn the_system_helper_takes_the_user_then_the_cookie() {
        let mut stream = UnixStream::connect(SOCKET).expect("the polkit helper socket");
        stream.write_all(b"glide-probe-nobody\nglide-probe-cookie\n").expect("written");
        let mut first = String::new();
        BufReader::new(stream).read_line(&mut first).expect("a first line");
        assert!(
            matches!(parse(&first), Some(Line::Ask { .. } | Line::Failure | Line::Error(_) | Line::Info(_))),
            "{first:?}"
        );
    }
}
