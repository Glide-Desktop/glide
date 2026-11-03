use std::{
    os::unix::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

const PROGRAM: &str = "glide-xembed";
pub const CONTAINER_CLASS: &str = "glide-xembed";
const DISPLAY_POLL: Duration = Duration::from_secs(2);
const RESTART_DELAY: Duration = Duration::from_secs(1);

fn program() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(PROGRAM)))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(PROGRAM))
}

#[derive(Debug)]
pub struct Bridge {
    stop: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
}

impl Bridge {
    #[must_use]
    pub fn start() -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(0));
        let (thread_stop, thread_pid) = (Arc::clone(&stop), Arc::clone(&pid));
        let spawned =
            std::thread::Builder::new().name("glide-xembed".into()).spawn(move || supervise(&thread_stop, &thread_pid));
        match spawned {
            Ok(_) => Some(Self { stop, pid }),
            Err(err) => {
                tracing::warn!(?err, "could not start the X11 tray bridge supervisor");
                None
            }
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let pid = self.pid.swap(0, Ordering::SeqCst);
        if let Ok(pid) = i32::try_from(pid)
            && pid > 0
        {
            // SAFETY: `kill` has no memory-safety preconditions; the pid is the bridge's own child, which the
            // supervisor thread still waits on, so it cannot have been reaped and reused yet.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
}

fn supervise(stop: &AtomicBool, pid: &AtomicU32) {
    let mut restarts = crate::xwayland::Restarts::default();
    while !stop.load(Ordering::SeqCst) {
        let Some(display) = std::env::var_os("DISPLAY") else {
            std::thread::sleep(DISPLAY_POLL);
            continue;
        };
        let mut command = Command::new(program());
        command.env("DISPLAY", display);
        // SAFETY: the closure runs in the forked child before exec and only calls `prctl`, which is
        // async-signal-safe; it ties the bridge's life to this supervisor thread.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                tracing::info!("{PROGRAM} is not available ({err}); Wine and X11 tray icons keep their own windows");
                return;
            }
        };
        pid.store(child.id(), Ordering::SeqCst);
        tracing::info!(pid = child.id(), "started the X11 tray bridge");
        let status = child.wait();
        pid.store(0, Ordering::SeqCst);
        if stop.load(Ordering::SeqCst) {
            return;
        }
        tracing::warn!(?status, "{PROGRAM} exited");
        if !restarts.allow(Instant::now()) {
            tracing::error!("{PROGRAM} keeps exiting; X11 tray icons stay in their own windows");
            return;
        }
        std::thread::sleep(RESTART_DELAY);
    }
}

#[must_use]
pub fn is_container(app_id: Option<&str>, pid: Option<i32>) -> bool {
    app_id == Some(CONTAINER_CLASS) && pid.is_some_and(crate::force_quit::is_x11_bridge)
}
