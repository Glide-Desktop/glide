use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};

#[repr(C)]
struct PamHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Reply {
    text: *mut c_char,
    code: c_int,
}

#[repr(C)]
struct Conversation {
    conv: Option<unsafe extern "C" fn(c_int, *mut *const Message, *mut *mut Reply, *mut c_void) -> c_int>,
    data: *mut c_void,
}

const PAM_SUCCESS: c_int = 0;

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start_confdir(
        service: *const c_char, user: *const c_char, conversation: *const Conversation, confdir: *const c_char,
        pamh: *mut *mut PamHandle,
    ) -> c_int;
    fn pam_putenv(pamh: *mut PamHandle, name_value: *const c_char) -> c_int;
    fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_open_session(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut PamHandle, status: c_int) -> c_int;
}

unsafe extern "C" fn answer(
    count: c_int, _: *mut *const Message, replies: *mut *mut Reply, data: *mut c_void,
) -> c_int {
    let count = usize::try_from(count).unwrap_or(0);
    // SAFETY: libpam frees the replies with free(), so they come from calloc and strdup; data is the password CString.
    unsafe {
        let array = libc::calloc(count, std::mem::size_of::<Reply>()).cast::<Reply>();
        for at in 0..count {
            (*array.add(at)).text = libc::strdup(data.cast::<c_char>());
        }
        *replies = array;
    }
    PAM_SUCCESS
}

fn module() -> Option<PathBuf> {
    let deps = std::env::current_exe().ok()?.parent()?.parent()?.join("libpam_glide_keychain.so");
    deps.exists().then_some(deps).or_else(|| {
        eprintln!("the module is not built next to this test; skipping");
        None
    })
}

fn me() -> String {
    // SAFETY: getpwuid returns static storage that is read here before any other passwd lookup.
    let entry = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!entry.is_null());
    // SAFETY: pw_name of a non-null entry is a NUL-terminated string.
    unsafe { CStr::from_ptr((*entry).pw_name) }.to_string_lossy().into_owned()
}

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("glide-pam-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("conf")).unwrap();
    std::fs::create_dir_all(dir.join("runtime")).unwrap();
    dir
}

struct Session {
    pamh: *mut PamHandle,
}

impl Session {
    fn start(dir: &Path, stack: &str, password: &str) -> Self {
        std::fs::write(dir.join("conf").join("glide-test"), stack).unwrap();
        let (service, user) = (CString::new("glide-test").unwrap(), CString::new(me()).unwrap());
        let confdir = CString::new(dir.join("conf").to_str().unwrap()).unwrap();
        let password = CString::new(password).unwrap().into_raw().cast::<c_void>();
        let conversation = Box::leak(Box::new(Conversation { conv: Some(answer), data: password }));
        let mut pamh = std::ptr::null_mut();
        // SAFETY: every pointer outlives the call; the conversation is leaked so it outlives the handle.
        let started = unsafe {
            pam_start_confdir(service.as_ptr(), user.as_ptr(), conversation, confdir.as_ptr(), &raw mut pamh)
        };
        assert_eq!(started, PAM_SUCCESS);
        let runtime = CString::new(format!("XDG_RUNTIME_DIR={}", dir.join("runtime").display())).unwrap();
        // SAFETY: pamh is the live handle from pam_start_confdir, and libpam copies the string.
        assert_eq!(unsafe { pam_putenv(pamh, runtime.as_ptr()) }, PAM_SUCCESS);
        Self { pamh }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: pamh is the live handle from pam_start_confdir and is ended exactly once.
        unsafe { pam_end(self.pamh, PAM_SUCCESS) };
    }
}

#[test]
fn a_login_leaves_the_stretched_password_for_the_keychain_and_never_the_password() {
    let Some(module) = module() else {
        return;
    };
    let dir = temp("login");
    let stack = format!(
        "auth required pam_exec.so expose_authtok /bin/true\nauth optional {0}\n\
         session required pam_permit.so\nsession optional {0}\n",
        module.display()
    );
    let session = Session::start(&dir, &stack, "correct horse");
    // SAFETY: pamh is live.
    assert_eq!(unsafe { pam_authenticate(session.pamh, 0) }, PAM_SUCCESS);
    // SAFETY: pamh is live.
    assert_eq!(unsafe { pam_open_session(session.pamh, 0) }, PAM_SUCCESS);
    let written = std::fs::read(dir.join("runtime").join(glide_login_key::KEY_FILE)).unwrap();
    let expected = glide_login_key::wrapping_key(b"correct horse", &me(), uid()).unwrap();
    assert_eq!(written, expected.as_ref());
    assert!(!written.windows(5).any(|window| window == b"horse"));
    drop(session);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_login_without_a_password_writes_nothing() {
    let Some(module) = module() else {
        return;
    };
    let dir = temp("nopassword");
    let stack = format!(
        "auth required pam_permit.so\nauth optional {0}\nsession required pam_permit.so\nsession optional {0}\n",
        module.display()
    );
    let session = Session::start(&dir, &stack, "unused");
    // SAFETY: pamh is live.
    assert_eq!(unsafe { pam_authenticate(session.pamh, 0) }, PAM_SUCCESS);
    // SAFETY: pamh is live.
    assert_eq!(unsafe { pam_open_session(session.pamh, 0) }, PAM_SUCCESS, "the module never fails a login");
    assert!(!dir.join("runtime").join(glide_login_key::KEY_FILE).exists());
    drop(session);
    std::fs::remove_dir_all(dir).unwrap();
}
