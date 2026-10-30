use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const PAM_SUCCESS: c_int = 0;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_REFRESH_CRED: c_int = 0x0010;

const LIBRARY: &CStr = c"libpam.so.0";
const SERVICE: &str = "glide-lock";
const FALLBACK_SERVICE: &str = "login";
const PAM_DIRS: [&str; 3] = ["/etc/pam.d", "/usr/lib/pam.d", "/usr/etc/pam.d"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Granted,
    Denied(Option<String>),
    Unavailable,
    Ask { secret: bool, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub generation: u64,
    pub verdict: Verdict,
}

const ANSWER_WAIT: Duration = Duration::from_mins(2);

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type ConverseFn = unsafe extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int;

#[repr(C)]
struct PamConv {
    conv: Option<ConverseFn>,
    appdata_ptr: *mut c_void,
}

type StartFn = unsafe extern "C" fn(*const c_char, *const c_char, *const PamConv, *mut *mut c_void) -> c_int;
type HandleFn = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;

type Ask<'a> = &'a dyn Fn(bool, String) -> Option<String>;

struct Conversation<'a> {
    secret: Vec<u8>,
    answered: bool,
    errors: Vec<String>,
    notices: Vec<String>,
    ask: Option<Ask<'a>>,
}

impl<'a> Conversation<'a> {
    fn new(secret: Vec<u8>) -> Self {
        Self { secret, answered: false, errors: Vec::new(), notices: Vec::new(), ask: None }
    }

    fn with_ask(mut self, ask: Option<Ask<'a>>) -> Self {
        self.ask = ask;
        self
    }

    fn denial(&mut self) -> Option<String> {
        if let Some(error) = self.errors.pop().filter(|text| !text.is_empty()) {
            return Some(error);
        }
        let notice = self.notices.join(" ");
        (!notice.is_empty()).then_some(notice)
    }
}

unsafe fn free_replies(replies: *mut PamResponse, count: usize) {
    for index in 0..count {
        // SAFETY: the caller passes a `calloc`ed array of at least `count`
        // entries whose `resp` fields are null or `strdup`ed strings.
        unsafe {
            let resp = (*replies.add(index)).resp;
            if !resp.is_null() {
                std::ptr::write_bytes(resp, 0, libc::strlen(resp));
                libc::free(resp.cast());
            }
        }
    }
    // SAFETY: `replies` came from `calloc` and is freed exactly once.
    unsafe { libc::free(replies.cast()) };
}

unsafe extern "C" fn converse(
    count: c_int, messages: *mut *const PamMessage, out: *mut *mut PamResponse, data: *mut c_void,
) -> c_int {
    let Ok(count) = usize::try_from(count) else {
        return PAM_CONV_ERR;
    };
    if count == 0 || messages.is_null() || out.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    // SAFETY: `calloc` either fails (null, handled) or returns zeroed storage
    // for `count` responses, which libpam takes ownership of and frees.
    let replies: *mut PamResponse = unsafe { libc::calloc(count, std::mem::size_of::<PamResponse>()) }.cast();
    if replies.is_null() {
        return PAM_BUF_ERR;
    }
    // SAFETY: `data` is the `appdata_ptr` set in `authenticate`, pointing at a
    // live `Conversation` that outlives the PAM handle.
    let conversation = unsafe { &mut *data.cast::<Conversation<'_>>() };
    for index in 0..count {
        // SAFETY: Linux-PAM passes an array of `count` message pointers.
        let message = unsafe { *messages.add(index) };
        if message.is_null() {
            // SAFETY: `replies` holds `count` entries, the first `index` filled.
            unsafe { free_replies(replies, index) };
            return PAM_CONV_ERR;
        }
        // SAFETY: `message` is non-null and points at a libpam-owned message.
        let (style, text) = unsafe { ((*message).msg_style, (*message).msg) };
        match style {
            PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON if conversation.answered || style == PAM_PROMPT_ECHO_ON => {
                let question = if text.is_null() {
                    String::new()
                } else {
                    // SAFETY: non-null PAM message text is a C string.
                    unsafe { CStr::from_ptr(text) }.to_string_lossy().trim().to_string()
                };
                let answer = conversation.ask.and_then(|ask| ask(style == PAM_PROMPT_ECHO_OFF, question.clone()));
                let copy = answer.and_then(|answer| {
                    let text = CString::new(answer).map_err(|err| err.into_vec().fill(0)).ok()?;
                    // SAFETY: `text` is a NUL-terminated C string alive across the call.
                    let copy = unsafe { libc::strdup(text.as_ptr()) };
                    text.into_bytes_with_nul().fill(0);
                    (!copy.is_null()).then_some(copy)
                });
                let Some(copy) = copy else {
                    conversation.errors.push(question);
                    // SAFETY: as above, `index` entries are filled.
                    unsafe { free_replies(replies, index) };
                    return PAM_CONV_ERR;
                };
                // SAFETY: `index < count`, inside the `calloc`ed array.
                unsafe { (*replies.add(index)).resp = copy };
            }
            PAM_PROMPT_ECHO_OFF => {
                conversation.answered = true;
                // SAFETY: `secret` is NUL-terminated with no interior NUL.
                let copy = unsafe { libc::strdup(conversation.secret.as_ptr().cast()) };
                if copy.is_null() {
                    // SAFETY: as above, `index` entries are filled.
                    unsafe { free_replies(replies, index) };
                    return PAM_BUF_ERR;
                }
                // SAFETY: `index < count`, inside the `calloc`ed array.
                unsafe { (*replies.add(index)).resp = copy };
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO => {
                if !text.is_null() {
                    // SAFETY: non-null PAM message text is a C string.
                    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().trim().to_string();
                    if style == PAM_ERROR_MSG {
                        conversation.errors.push(text);
                    } else if !text.is_empty() {
                        conversation.notices.push(text);
                    }
                }
            }
            _ => {
                // SAFETY: as above, `index` entries are filled.
                unsafe { free_replies(replies, index) };
                return PAM_CONV_ERR;
            }
        }
    }
    // SAFETY: `out` is non-null and owned by libpam for this call.
    unsafe { *out = replies };
    PAM_SUCCESS
}

struct Library {
    start: StartFn,
    authenticate: HandleFn,
    setcred: HandleFn,
    end: HandleFn,
}

unsafe fn symbol<T: Copy>(handle: *mut c_void, name: &CStr) -> Option<T> {
    // SAFETY: `handle` is a live `dlopen` handle and `name` a C string.
    let pointer = unsafe { libc::dlsym(handle, name.as_ptr()) };
    if pointer.is_null() {
        return None;
    }
    assert_eq!(
        std::mem::size_of::<T>(),
        std::mem::size_of::<*mut c_void>(),
        "a PAM symbol is read back as a function pointer"
    );
    // SAFETY: the caller names a libpam function whose C signature matches `T`,
    // and `T` is a function pointer the size of a data pointer.
    Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&pointer) })
}

impl Library {
    fn open() -> Option<Self> {
        // SAFETY: `LIBRARY` is a C string; the handle is intentionally never
        // closed, so the resolved function pointers stay valid.
        let handle = unsafe { libc::dlopen(LIBRARY.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            tracing::warn!("libpam.so.0 could not be loaded; the lock screen cannot unlock");
            return None;
        }
        // SAFETY: each name is a libpam export with the matching signature.
        unsafe {
            Some(Self {
                start: symbol(handle, c"pam_start")?,
                authenticate: symbol(handle, c"pam_authenticate")?,
                setcred: symbol(handle, c"pam_setcred")?,
                end: symbol(handle, c"pam_end")?,
            })
        }
    }

    fn authenticate(&self, service: &CStr, user: &CStr, password: String, ask: Option<Ask<'_>>) -> Verdict {
        let mut password = password.into_bytes();
        let mut secret = Vec::with_capacity(password.len() + 1);
        secret.extend_from_slice(&password);
        password.fill(0);
        drop(password);
        if secret.contains(&0) {
            secret.fill(0);
            return Verdict::Denied(None);
        }
        secret.push(0);
        let mut conversation = Box::new(Conversation::new(secret).with_ask(ask));
        let conv = PamConv { conv: Some(converse), appdata_ptr: std::ptr::from_mut(conversation.as_mut()).cast() };
        let mut handle: *mut c_void = std::ptr::null_mut();
        // SAFETY: `service` and `user` are C strings, `conv` and the boxed
        // conversation outlive the handle, which `pam_end` releases below.
        let started = unsafe { (self.start)(service.as_ptr(), user.as_ptr(), &raw const conv, &raw mut handle) };
        let verdict = if started != PAM_SUCCESS || handle.is_null() {
            tracing::warn!(started, "pam_start failed");
            Verdict::Unavailable
        } else {
            // SAFETY: `handle` is the live handle `pam_start` returned.
            let status = unsafe { (self.authenticate)(handle, 0) };
            if status == PAM_SUCCESS {
                // SAFETY: as above.
                let _ = unsafe { (self.setcred)(handle, PAM_REFRESH_CRED) };
            }
            // SAFETY: as above; the handle is not used afterwards.
            let _ = unsafe { (self.end)(handle, status) };
            if status == PAM_SUCCESS { Verdict::Granted } else { Verdict::Denied(conversation.denial()) }
        };
        conversation.secret.fill(0);
        verdict
    }
}

#[must_use]
pub fn available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| Library::open().is_some())
}

#[must_use]
pub fn service_name(exists: impl Fn(&Path) -> bool) -> &'static str {
    let installed = PAM_DIRS.iter().any(|dir| exists(&Path::new(dir).join(SERVICE)));
    if installed { SERVICE } else { FALLBACK_SERVICE }
}

enum Input {
    Password { generation: u64, text: String },
    Answer { generation: u64, text: String },
    Wake,
}

fn forget(text: String) {
    let mut bytes = text.into_bytes();
    bytes.fill(0);
}

fn forget_input(input: Input) {
    if let Input::Password { text, .. } | Input::Answer { text, .. } = input {
        forget(text);
    }
}

fn wait_for_answer(
    incoming: &mpsc::Receiver<Input>, current: &AtomicU64, generation: u64, wait: Duration,
    carry: &RefCell<Option<Input>>,
) -> Option<String> {
    let deadline = Instant::now() + wait;
    loop {
        if current.load(Ordering::Acquire) != generation {
            return None;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match incoming.recv_timeout(left) {
            Ok(Input::Answer { generation: given, text })
                if given == generation && current.load(Ordering::Acquire) == generation =>
            {
                return Some(text);
            }
            Ok(Input::Answer { text, .. }) => forget(text),
            Ok(Input::Password { generation: given, text }) => {
                if current.load(Ordering::Acquire) == given {
                    *carry.borrow_mut() = Some(Input::Password { generation: given, text });
                    return None;
                }
                forget(text);
            }
            Ok(Input::Wake) => {}
            Err(_) => return None,
        }
    }
}

fn serve(
    incoming: &mpsc::Receiver<Input>, current: &AtomicU64, reply: &dyn Fn(Reply),
    authenticate: &dyn for<'a> Fn(String, Ask<'a>) -> Verdict, wait: Duration,
) {
    let mut carried: Option<Input> = None;
    loop {
        let input = match carried.take() {
            Some(input) => input,
            None => match incoming.recv() {
                Ok(input) => input,
                Err(_) => return,
            },
        };
        let Input::Password { generation, text } = input else {
            forget_input(input);
            continue;
        };
        if current.load(Ordering::Acquire) != generation {
            forget(text);
            continue;
        }
        let carry = RefCell::new(None);
        let ask = |secret: bool, question: String| {
            if current.load(Ordering::Acquire) != generation {
                return None;
            }
            reply(Reply { generation, verdict: Verdict::Ask { secret, text: question } });
            wait_for_answer(incoming, current, generation, wait, &carry)
        };
        let verdict = authenticate(text, &ask);
        reply(Reply { generation, verdict });
        carried = carry.into_inner();
    }
}

pub struct Authenticator {
    requests: mpsc::Sender<Input>,
    current: Arc<AtomicU64>,
    issued: u64,
}

impl Authenticator {
    pub fn start(user: &str, reply: impl Fn(Reply) + Send + 'static) -> std::io::Result<Self> {
        let user = CString::new(user).unwrap_or_default();
        Self::spawn(reply, ANSWER_WAIT, move || {
            let library = Library::open();
            let service = service_name(Path::exists);
            tracing::info!(service, "lock screen authenticates through PAM");
            let service = CString::new(service).unwrap_or_default();
            move |password: String, ask: Ask<'_>| {
                if let Some(library) = &library {
                    library.authenticate(&service, &user, password, Some(ask))
                } else {
                    forget(password);
                    Verdict::Unavailable
                }
            }
        })
    }

    fn spawn<A>(
        reply: impl Fn(Reply) + Send + 'static, wait: Duration, build: impl FnOnce() -> A + Send + 'static,
    ) -> std::io::Result<Self>
    where
        A: for<'a> Fn(String, Ask<'a>) -> Verdict,
    {
        let (requests, incoming) = mpsc::channel::<Input>();
        let current = Arc::new(AtomicU64::new(0));
        let shared = Arc::clone(&current);
        std::thread::Builder::new().name("glide-lock-auth".into()).spawn(move || {
            let authenticate = build();
            serve(&incoming, &shared, &reply, &authenticate, wait);
        })?;
        Ok(Self { requests, current, issued: 0 })
    }

    pub fn begin(&mut self) -> u64 {
        self.issued += 1;
        self.current.store(self.issued, Ordering::Release);
        let _ = self.requests.send(Input::Wake);
        self.issued
    }

    pub fn end(&self) {
        self.current.store(0, Ordering::Release);
        let _ = self.requests.send(Input::Wake);
    }

    #[must_use]
    pub fn is_current(&self, generation: u64) -> bool {
        generation != 0 && self.current.load(Ordering::Acquire) == generation
    }

    #[must_use]
    pub fn check(&self, password: String) -> bool {
        match self.current.load(Ordering::Acquire) {
            0 => {
                forget(password);
                false
            }
            generation => self.requests.send(Input::Password { generation, text: password }).is_ok(),
        }
    }

    #[must_use]
    pub fn answer(&self, text: String) -> bool {
        match self.current.load(Ordering::Acquire) {
            0 => {
                forget(text);
                false
            }
            generation => self.requests.send(Input::Answer { generation, text }).is_ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn converse_with(messages: &[(c_int, &CStr)], secret: &str) -> (c_int, Vec<Option<String>>, Conversation<'static>) {
        converse_asking(messages, secret, None)
    }

    fn converse_asking<'a>(
        messages: &[(c_int, &CStr)], secret: &str, ask: Option<Ask<'a>>,
    ) -> (c_int, Vec<Option<String>>, Conversation<'a>) {
        let mut secret = secret.as_bytes().to_vec();
        secret.push(0);
        let mut conversation = Conversation::new(secret).with_ask(ask);
        let owned: Vec<PamMessage> =
            messages.iter().map(|(style, text)| PamMessage { msg_style: *style, msg: text.as_ptr() }).collect();
        let mut pointers: Vec<*const PamMessage> = owned.iter().map(std::ptr::from_ref).collect();
        let mut out: *mut PamResponse = std::ptr::null_mut();
        let count = c_int::try_from(messages.len()).unwrap();
        // SAFETY: the arrays and the conversation live across the call.
        let status = unsafe {
            converse(count, pointers.as_mut_ptr(), &raw mut out, std::ptr::from_mut(&mut conversation).cast())
        };
        let mut answers = Vec::new();
        if status == PAM_SUCCESS {
            for index in 0..messages.len() {
                // SAFETY: `converse` filled `out` with `count` responses.
                let resp = unsafe { (*out.add(index)).resp };
                answers.push((!resp.is_null()).then(|| {
                    // SAFETY: a non-null response is a `strdup`ed C string.
                    unsafe { CStr::from_ptr(resp) }.to_string_lossy().into_owned()
                }));
            }
            // SAFETY: `out` came from `converse` and is released once.
            unsafe { free_replies(out, messages.len()) };
        }
        (status, answers, conversation)
    }

    #[test]
    fn the_password_prompt_is_answered_and_errors_are_kept() {
        let (status, answers, conversation) = converse_with(
            &[
                (PAM_TEXT_INFO, c"Welcome"),
                (PAM_PROMPT_ECHO_OFF, c"Password: "),
                (PAM_ERROR_MSG, c"The account is locked "),
            ],
            "hunter2",
        );
        assert_eq!(status, PAM_SUCCESS);
        assert_eq!(answers, vec![None, Some("hunter2".into()), None]);
        assert_eq!(conversation.errors, vec!["The account is locked".to_string()]);
    }

    #[test]
    fn a_faillock_lockout_explains_the_denial_instead_of_a_wrong_password() {
        let (status, _, mut conversation) = converse_with(
            &[
                (PAM_TEXT_INFO, c"The account is locked due to 3 failed logins."),
                (PAM_TEXT_INFO, c"(10 minutes left to unlock)"),
                (PAM_PROMPT_ECHO_OFF, c"Password: "),
            ],
            "hunter2",
        );
        assert_eq!(status, PAM_SUCCESS);
        assert_eq!(
            conversation.denial().as_deref(),
            Some("The account is locked due to 3 failed logins. (10 minutes left to unlock)")
        );
    }

    #[test]
    fn an_error_message_outranks_notices_and_silence_means_no_reason() {
        let (_, _, mut conversation) = converse_with(
            &[(PAM_TEXT_INFO, c"Welcome"), (PAM_ERROR_MSG, c"Authentication token is no longer valid")],
            "x",
        );
        assert_eq!(conversation.denial().as_deref(), Some("Authentication token is no longer valid"));
        let (_, _, mut quiet) = converse_with(&[(PAM_PROMPT_ECHO_OFF, c"Password: ")], "x");
        assert_eq!(quiet.denial(), None);
    }

    #[test]
    fn the_password_answers_one_hidden_prompt_and_never_a_second_question() {
        let (status, _, conversation) = converse_with(
            &[(PAM_PROMPT_ECHO_OFF, c"Password: "), (PAM_PROMPT_ECHO_OFF, c"Verification code: ")],
            "hunter2",
        );
        assert_eq!(status, PAM_CONV_ERR, "a one-time code is not the password");
        assert_eq!(conversation.errors, vec!["Verification code:".to_string()]);
        let (status, _, _) = converse_with(&[(PAM_PROMPT_ECHO_ON, c"login: ")], "hunter2");
        assert_eq!(status, PAM_CONV_ERR, "a visible prompt never gets the password");
    }

    #[test]
    fn a_second_question_is_put_to_the_user_and_their_answer_goes_back() {
        let asked = std::cell::RefCell::new(Vec::new());
        let ask = |secret: bool, text: String| {
            asked.borrow_mut().push((secret, text));
            Some("482913".to_string())
        };
        let (status, answers, _) = converse_asking(
            &[(PAM_PROMPT_ECHO_OFF, c"Password: "), (PAM_PROMPT_ECHO_OFF, c"Verification code: ")],
            "hunter2",
            Some(&ask),
        );
        assert_eq!(status, PAM_SUCCESS);
        assert_eq!(answers, vec![Some("hunter2".into()), Some("482913".into())]);
        assert_eq!(*asked.borrow(), vec![(true, "Verification code:".to_string())]);
        let refuse = |_: bool, _: String| None;
        let (status, _, conversation) = converse_asking(&[(PAM_PROMPT_ECHO_ON, c"Token: ")], "hunter2", Some(&refuse));
        assert_eq!(status, PAM_CONV_ERR, "an unanswered question fails the attempt");
        assert_eq!(conversation.errors, vec!["Token:".to_string()]);
    }

    #[test]
    fn an_unknown_message_style_fails_the_conversation() {
        let (status, _, _) = converse_with(&[(99, c"?")], "x");
        assert_eq!(status, PAM_CONV_ERR);
    }

    type Log = Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>;

    const PATIENCE: Duration = Duration::from_secs(5);

    fn logged(log: &Log) -> Vec<(String, Option<String>)> {
        log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    fn code_asker(log: Log) -> impl for<'a> Fn(String, Ask<'a>) -> Verdict {
        move |password: String, ask: Ask<'_>| {
            let answer = password.starts_with("ask").then(|| ask(true, "Verification code:".into())).flatten();
            let asked = password.starts_with("ask");
            log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((password, answer.clone()));
            if asked && answer.is_none() { Verdict::Denied(None) } else { Verdict::Granted }
        }
    }

    fn collected() -> (impl Fn(Reply) + Send + 'static, mpsc::Receiver<Reply>) {
        let (sender, receiver) = mpsc::channel();
        (
            move |reply| {
                let _ = sender.send(reply);
            },
            receiver,
        )
    }

    fn question() -> Verdict {
        Verdict::Ask { secret: true, text: "Verification code:".into() }
    }

    #[test]
    fn every_reply_carries_the_generation_of_the_lock_that_asked_for_it() {
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, move || code_asker(Log::default())).expect("worker");
        let first = auth.begin();
        assert!(auth.check("pw".into()));
        let got = replies.recv_timeout(PATIENCE).expect("verdict");
        assert_eq!(got, Reply { generation: first, verdict: Verdict::Granted });
        assert!(auth.is_current(got.generation));
        let second = auth.begin();
        assert_ne!(first, second);
        assert!(!auth.is_current(first), "a new lock retires the generation of the old one");
        assert!(auth.check("pw".into()));
        assert_eq!(replies.recv_timeout(PATIENCE).expect("verdict").generation, second);
    }

    #[test]
    fn nothing_is_sent_to_pam_before_a_lock_begins_or_after_it_ends() {
        let log = Log::default();
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, {
            let log = log.clone();
            move || code_asker(log)
        })
        .expect("worker");
        assert!(!auth.check("pw".into()) && !auth.answer("1".into()));
        auth.begin();
        auth.end();
        assert!(!auth.check("pw".into()) && !auth.answer("1".into()));
        assert!(!auth.is_current(0), "generation zero is never a lock");
        assert!(replies.recv_timeout(Duration::from_millis(50)).is_err());
        assert!(logged(&log).is_empty());
    }

    #[test]
    fn ending_the_lock_wakes_a_pending_question_at_once() {
        let log = Log::default();
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, {
            let log = log.clone();
            move || code_asker(log)
        })
        .expect("worker");
        let first = auth.begin();
        assert!(auth.check("ask-1".into()));
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: first, verdict: question() }));
        auth.end();
        let denied = replies.recv_timeout(PATIENCE).expect("the question gives up long before the two-minute wait");
        assert_eq!(denied, Reply { generation: first, verdict: Verdict::Denied(None) });
        assert!(!auth.is_current(denied.generation), "the compositor ignores what the ended lock produced");
        assert_eq!(logged(&log), [("ask-1".to_string(), None)]);
        let second = auth.begin();
        assert!(auth.check("pw".into()));
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: second, verdict: Verdict::Granted }));
    }

    #[test]
    fn the_first_password_of_a_new_lock_is_never_the_answer_to_the_old_locks_question() {
        let log = Log::default();
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, {
            let log = log.clone();
            move || code_asker(log)
        })
        .expect("worker");
        let first = auth.begin();
        assert!(auth.check("ask-1".into()));
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: first, verdict: question() }));
        let second = auth.begin();
        assert!(auth.check("pw-2".into()));
        assert_eq!(
            replies.recv_timeout(PATIENCE),
            Ok(Reply { generation: first, verdict: Verdict::Denied(None) }),
            "the old attempt fails without ever seeing the new password"
        );
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: second, verdict: Verdict::Granted }));
        assert_eq!(logged(&log), [("ask-1".to_string(), None), ("pw-2".to_string(), None)]);
    }

    #[test]
    fn a_dropped_authenticator_aborts_the_pending_question() {
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, move || code_asker(Log::default())).expect("worker");
        let first = auth.begin();
        assert!(auth.check("ask-1".into()));
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: first, verdict: question() }));
        drop(auth);
        let denied = replies.recv_timeout(PATIENCE).expect("the question gives up when its owner is gone");
        assert_eq!(denied.verdict, Verdict::Denied(None));
    }

    #[test]
    fn a_verdict_that_arrives_after_its_lock_ended_is_no_longer_current() {
        let (started, begun) = mpsc::channel::<()>();
        let (release, gate) = mpsc::channel::<()>();
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, move || {
            move |_: String, _: Ask<'_>| {
                let _ = started.send(());
                let _ = gate.recv();
                Verdict::Granted
            }
        })
        .expect("worker");
        let first = auth.begin();
        assert!(auth.check("pw".into()));
        begun.recv_timeout(PATIENCE).expect("the attempt started");
        auth.end();
        let second = auth.begin();
        release.send(()).expect("release");
        let late = replies.recv_timeout(PATIENCE).expect("verdict");
        assert_eq!(late, Reply { generation: first, verdict: Verdict::Granted });
        assert!(!auth.is_current(late.generation));
        assert!(auth.is_current(second));
    }

    #[test]
    fn an_attempt_queued_under_an_ended_lock_never_reaches_pam() {
        let log = Log::default();
        let (started, begun) = mpsc::channel::<()>();
        let (release, gate) = mpsc::channel::<()>();
        let (reply, replies) = collected();
        let mut auth = Authenticator::spawn(reply, ANSWER_WAIT, {
            let log = log.clone();
            move || {
                move |password: String, _: Ask<'_>| {
                    if password == "pw-1" {
                        let _ = started.send(());
                        let _ = gate.recv();
                    }
                    log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((password, None));
                    Verdict::Granted
                }
            }
        })
        .expect("worker");
        let first = auth.begin();
        assert!(auth.check("pw-1".into()));
        begun.recv_timeout(PATIENCE).expect("the first attempt is inside PAM");
        assert!(auth.check("queued-behind-it".into()));
        auth.end();
        let second = auth.begin();
        assert!(auth.check("pw-2".into()));
        release.send(()).expect("release");
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: first, verdict: Verdict::Granted }));
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: second, verdict: Verdict::Granted }));
        assert_eq!(logged(&log), [("pw-1".to_string(), None), ("pw-2".to_string(), None)]);
    }

    #[test]
    fn an_unanswered_question_gives_up_after_the_wait() {
        let (reply, replies) = collected();
        let mut auth =
            Authenticator::spawn(reply, Duration::from_millis(30), move || code_asker(Log::default())).expect("worker");
        let first = auth.begin();
        assert!(auth.check("ask-1".into()));
        assert_eq!(replies.recv_timeout(PATIENCE), Ok(Reply { generation: first, verdict: question() }));
        assert_eq!(
            replies.recv_timeout(PATIENCE),
            Ok(Reply { generation: first, verdict: Verdict::Denied(None) }),
            "the lock is still current, so the user sees the failure"
        );
        assert!(auth.is_current(first));
    }

    fn run(inputs: Vec<Input>, current: u64, log: &Log) -> Vec<Reply> {
        let (sender, incoming) = mpsc::channel();
        for input in inputs {
            sender.send(input).expect("queued");
        }
        drop(sender);
        let current = AtomicU64::new(current);
        let replies = RefCell::new(Vec::new());
        serve(&incoming, &current, &|reply| replies.borrow_mut().push(reply), &code_asker(log.clone()), ANSWER_WAIT);
        replies.into_inner()
    }

    fn password(generation: u64, text: &str) -> Input {
        Input::Password { generation, text: text.into() }
    }

    fn answer(generation: u64, text: &str) -> Input {
        Input::Answer { generation, text: text.into() }
    }

    #[test]
    fn a_password_is_never_taken_as_the_answer_to_a_question() {
        let log = Log::default();
        let replies = run(vec![password(1, "ask-1"), password(1, "pw")], 1, &log);
        assert_eq!(logged(&log), [("ask-1".to_string(), None), ("pw".to_string(), None)]);
        assert_eq!(
            replies,
            [
                Reply { generation: 1, verdict: question() },
                Reply { generation: 1, verdict: Verdict::Denied(None) },
                Reply { generation: 1, verdict: Verdict::Granted },
            ]
        );
    }

    #[test]
    fn only_an_answer_of_the_current_lock_answers_a_question_and_a_stray_one_is_not_a_password() {
        let log = Log::default();
        let inputs = vec![
            answer(1, "stray"),
            password(1, "ask-1"),
            answer(2, "from another lock"),
            password(0, "stale password"),
            answer(1, "482913"),
        ];
        let replies = run(inputs, 1, &log);
        assert_eq!(logged(&log), [("ask-1".to_string(), Some("482913".to_string()))]);
        assert_eq!(
            replies,
            [Reply { generation: 1, verdict: question() }, Reply { generation: 1, verdict: Verdict::Granted }]
        );
    }

    #[test]
    fn a_password_of_an_older_lock_is_dropped_while_asking_and_while_idle() {
        let log = Log::default();
        let replies = run(vec![password(3, "old"), password(4, "ask-4"), password(3, "late"), answer(4, "1")], 4, &log);
        assert_eq!(logged(&log), [("ask-4".to_string(), Some("1".to_string()))]);
        assert_eq!(replies.last().map(|reply| reply.generation), Some(4));
    }

    #[test]
    fn the_dedicated_service_is_preferred_when_installed() {
        assert_eq!(service_name(|_| false), "login");
        assert_eq!(service_name(|path| path == Path::new("/usr/lib/pam.d/glide-lock")), "glide-lock");
    }
}
