use std::{
    collections::VecDeque,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
};

const PROGRAM: &str = "xwayland-satellite";
const MAX_RESTARTS: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_mins(1);
const RESTART_DELAY: Duration = Duration::from_secs(1);
const READY_WAIT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Default)]
pub struct Restarts {
    recent: VecDeque<Instant>,
}

impl Restarts {
    pub fn allow(&mut self, now: Instant) -> bool {
        while self.recent.front().is_some_and(|at| now.saturating_duration_since(*at) > RESTART_WINDOW) {
            self.recent.pop_front();
        }
        if self.recent.len() >= MAX_RESTARTS {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

fn socket(number: u32) -> PathBuf {
    Path::new("/tmp/.X11-unix").join(format!("X{number}"))
}

fn lock(number: u32) -> PathBuf {
    Path::new("/tmp").join(format!(".X{number}-lock"))
}

fn pick_display() -> Option<u32> {
    (10..64).find(|number| !lock(*number).exists() && !socket(*number).exists())
}

fn clear_stale(number: u32) {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    for path in [socket(number), lock(number)] {
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.uid() == uid) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn wait_ready(child: &mut std::process::Child, number: u32) -> bool {
    let deadline = Instant::now() + READY_WAIT;
    while Instant::now() < deadline {
        if socket(number).exists() {
            return true;
        }
        if !matches!(child.try_wait(), Ok(None)) {
            return false;
        }
        std::thread::sleep(POLL);
    }
    true
}

fn supervise(program: &str, number: u32, ready: mpsc::Sender<bool>) {
    let x11 = format!(":{number}");
    let mut ready = Some(ready);
    let mut restarts = Restarts::default();
    loop {
        let mut child = match Command::new(program).arg(&x11).spawn() {
            Ok(child) => child,
            Err(err) => {
                tracing::info!("{program} is not available ({err}); X11 apps will not run");
                if let Some(ready) = ready.take() {
                    let _ = ready.send(false);
                }
                return;
            }
        };
        if let Some(ready) = ready.take() {
            let up = wait_ready(&mut child, number);
            tracing::info!(pid = child.id(), x11, up, "started {program}");
            let _ = ready.send(up);
        }
        let status = child.wait();
        tracing::warn!(?status, x11, "{program} exited");
        if !restarts.allow(Instant::now()) {
            tracing::error!(x11, "{program} keeps exiting; X11 apps stay unavailable");
            return;
        }
        std::thread::sleep(RESTART_DELAY);
        clear_stale(number);
    }
}

#[must_use]
pub fn start() -> Option<String> {
    let Some(number) = pick_display() else {
        tracing::warn!("no free X11 display number; XWayland support disabled");
        return None;
    };
    let (ready, started) = mpsc::channel();
    let spawned =
        std::thread::Builder::new().name("glide-xwayland".into()).spawn(move || supervise(PROGRAM, number, ready));
    if let Err(err) = spawned {
        tracing::warn!(?err, "could not start the XWayland supervisor");
        return None;
    }
    started.recv_timeout(READY_WAIT * 2).unwrap_or(false).then(|| format!(":{number}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_are_allowed_five_times_a_minute() {
        let mut restarts = Restarts::default();
        let start = Instant::now();
        for second in 0..5 {
            assert!(restarts.allow(start + Duration::from_secs(second)));
        }
        assert!(!restarts.allow(start + Duration::from_secs(10)), "a crash loop gives up");
        assert!(restarts.allow(start + Duration::from_secs(62)), "the budget refills as old restarts age out");
    }

    #[test]
    fn a_missing_satellite_reports_not_ready_at_once() {
        let (ready, started) = mpsc::channel();
        let began = Instant::now();
        supervise("glide-test-no-such-satellite", 63, ready);
        assert_eq!(started.try_recv(), Ok(false));
        assert!(began.elapsed() < READY_WAIT, "DISPLAY is not set for a satellite that never ran");
    }
}
