use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

const SETTLE: Duration = Duration::from_millis(120);
const POLL_MS: i32 = 500;
const MASK: u32 = libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO
    | libc::IN_CLOSE_WRITE
    | libc::IN_ATTRIB
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF;

fn inotify(dir: &Path) -> Option<OwnedFd> {
    // SAFETY: `inotify_init1` takes no pointers; a negative return is checked below.
    let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if raw < 0 {
        return None;
    }
    // SAFETY: `raw` is a freshly created descriptor nobody else owns.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let path = CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `fd` is a live inotify descriptor and `path` is NUL-terminated.
    let watch = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), path.as_ptr(), MASK) };
    (watch >= 0).then_some(fd)
}

fn wait_readable(fd: &OwnedFd, timeout_ms: i32) -> bool {
    let mut poll = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // SAFETY: `poll` points to one valid `pollfd` for the duration of the call.
    unsafe { libc::poll(&raw mut poll, 1, timeout_ms) > 0 }
}

fn drain(fd: &OwnedFd) {
    let mut buffer = [0_u8; 4096];
    loop {
        // SAFETY: `buffer` is writable for its full length and `fd` is non-blocking.
        let read = unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if read <= 0 {
            return;
        }
    }
}

fn watch(fd: &OwnedFd, sender: &UnboundedSender<()>) {
    while !sender.is_closed() {
        if !wait_readable(fd, POLL_MS) {
            continue;
        }
        std::thread::sleep(SETTLE);
        drain(fd);
        if sender.unbounded_send(()).is_err() {
            return;
        }
    }
}

#[must_use]
#[allow(clippy::ptr_arg)] // Subscription::run_with hands the keyed data back as `&PathBuf`
pub fn changes(dir: &PathBuf) -> UnboundedReceiver<()> {
    let (sender, receiver) = unbounded();
    if let Some(fd) = inotify(dir) {
        let _ = std::thread::Builder::new().name(String::from("glide-files-watch")).spawn(move || watch(&fd, &sender));
    }
    receiver
}
