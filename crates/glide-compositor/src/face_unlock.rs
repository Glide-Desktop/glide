use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use glide_config::facekey::{Event, Reason};
use glide_greeter::ui::FaceState;

const GRACE: Duration = Duration::from_secs(1);
const LOCK_GRACE: Duration = Duration::from_secs(6);
const AWAY_RETRY: Duration = Duration::from_secs(2);
const MISS_RETRY: Duration = Duration::from_secs(4);
const BUSY_RETRY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Press,
    Wake,
    Retry,
    Auto,
}

#[must_use]
pub fn retry_after(reason: Reason) -> Option<Duration> {
    match reason {
        Reason::NoFace | Reason::Timeout | Reason::TooDark | Reason::TooFar | Reason::OffFrame => Some(AWAY_RETRY),
        Reason::CameraBusy => Some(BUSY_RETRY),
        Reason::TooBright
        | Reason::Blurry
        | Reason::HeadTurned
        | Reason::Moving
        | Reason::Spoof
        | Reason::Mismatch
        | Reason::OtherPerson => Some(MISS_RETRY),
        Reason::Passed
        | Reason::NoCamera
        | Reason::CameraUntrusted
        | Reason::NotEnrolled
        | Reason::StaleProfile
        | Reason::Remote
        | Reason::Error => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    Event(Event),
    Ended { exited_cleanly: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub generation: u64,
    pub note: Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Show(Option<FaceState>),
    Unlock,
}

struct Run {
    generation: u64,
    child: Arc<Mutex<Child>>,
}

impl Run {
    fn start(binary: &Path, generation: u64, sink: impl Fn(Message) + Send + 'static) -> std::io::Result<Self> {
        let mut child = Command::new(binary)
            .args(["-verify", "-stream", "-source", "lock"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("facekey has no stdout"));
        };
        let child = Arc::new(Mutex::new(child));
        let owned = Arc::clone(&child);
        let spawned = std::thread::Builder::new().name("glide-face-unlock".into()).spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(event) = serde_json::from_str::<Event>(&line) {
                    sink(Message { generation, note: Note::Event(event) });
                }
            }
            let status = owned.lock().unwrap_or_else(PoisonError::into_inner).wait();
            sink(Message { generation, note: Note::Ended { exited_cleanly: status.is_ok_and(|s| s.success()) } });
        });
        if let Err(err) = spawned {
            let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        Ok(Self { generation, child })
    }

    fn stop(&self) {
        let _ = self.child.lock().unwrap_or_else(PoisonError::into_inner).kill();
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Default)]
pub struct FaceUnlock {
    pub config: glide_config::facekey::Config,
    pub sender: Option<smithay::reexports::calloop::channel::Sender<Message>>,
    pub timer: Option<smithay::reexports::calloop::RegistrationToken>,
    generation: u64,
    run: Option<Run>,
    locked_at: Option<Instant>,
    tried_since_wake: bool,
    passed: bool,
    retry: Option<Duration>,
    auto_after: Option<Instant>,
}

impl FaceUnlock {
    pub fn begin_lock(&mut self, now: Instant) {
        self.stop();
        self.locked_at = Some(now);
        self.tried_since_wake = false;
        self.auto_after = Some(now + LOCK_GRACE);
    }

    pub fn end_lock(&mut self) {
        self.stop();
        self.locked_at = None;
        self.auto_after = None;
    }

    pub fn screen_off(&mut self) {
        self.stop();
        self.tried_since_wake = false;
        self.auto_after = None;
    }

    pub fn hold(&mut self) {
        self.stop();
        self.auto_after = None;
    }

    #[must_use]
    pub fn due(&self) -> Option<Instant> {
        self.auto_after.filter(|_| self.locked_at.is_some() && self.run.is_none())
    }

    pub fn stop(&mut self) {
        self.generation += 1;
        self.run = None;
        self.passed = false;
    }

    #[must_use]
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    #[must_use]
    pub fn wants_start(&self, now: Instant, trigger: Trigger) -> bool {
        let Some(locked_at) = self.locked_at else { return false };
        if self.run.is_some() {
            return false;
        }
        match trigger {
            Trigger::Retry => true,
            Trigger::Auto => self.auto_after.is_some_and(|at| now >= at),
            Trigger::Wake => !self.tried_since_wake,
            Trigger::Press => !self.tried_since_wake && now.saturating_duration_since(locked_at) >= GRACE,
        }
    }

    pub fn start(&mut self, binary: &Path, sink: impl Fn(Message) + Send + 'static) -> std::io::Result<()> {
        self.stop();
        self.tried_since_wake = true;
        self.auto_after = None;
        self.retry = None;
        self.run = Some(Run::start(binary, self.generation, sink)?);
        Ok(())
    }

    pub fn receive(&mut self, message: &Message) -> Outcome {
        if self.run.as_ref().is_none_or(|run| run.generation != message.generation) {
            return Outcome::Nothing;
        }
        match &message.note {
            Note::Event(Event::Started { .. }) => Outcome::Show(Some(FaceState::Looking)),
            Note::Event(Event::Finished { success: true, .. }) => {
                self.passed = true;
                Outcome::Nothing
            }
            Note::Event(Event::Finished { success: false, reason } | Event::Failed { reason, .. }) => {
                self.retry = retry_after(*reason);
                Outcome::Show(shown_for(*reason))
            }
            Note::Event(_) => Outcome::Nothing,
            Note::Ended { exited_cleanly } => {
                let passed = std::mem::take(&mut self.passed) && *exited_cleanly;
                self.run = None;
                if passed {
                    return Outcome::Unlock;
                }
                self.auto_after =
                    self.retry.take().filter(|_| self.locked_at.is_some()).map(|gap| Instant::now() + gap);
                Outcome::Nothing
            }
        }
    }
}

#[must_use]
pub fn shown_for(reason: Reason) -> Option<FaceState> {
    match reason {
        Reason::CameraBusy => Some(FaceState::CameraBusy),
        Reason::NoCamera
        | Reason::CameraUntrusted
        | Reason::NotEnrolled
        | Reason::StaleProfile
        | Reason::Remote
        | Reason::Error => None,
        Reason::Passed => Some(FaceState::Recognised),
        Reason::NoFace | Reason::Timeout | Reason::TooFar | Reason::OffFrame => Some(FaceState::Looking),
        Reason::TooDark
        | Reason::TooBright
        | Reason::Blurry
        | Reason::HeadTurned
        | Reason::Moving
        | Reason::Spoof
        | Reason::Mismatch
        | Reason::OtherPerson => Some(FaceState::Rejected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(unlock: &mut FaceUnlock) -> u64 {
        let script = std::env::temp_dir().join(format!("glide-face-unlock-test-{}", std::process::id()));
        let _ = std::fs::write(&script, "#!/bin/sh\nsleep 30\n");
        let _ = std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755));
        unlock.start(&script, |_| {}).expect("a stand-in facekey starts");
        let _ = std::fs::remove_file(&script);
        unlock.generation
    }

    fn event(generation: u64, event: Event) -> Message {
        Message { generation, note: Note::Event(event) }
    }

    #[test]
    fn the_camera_waits_for_the_person_to_come_back_instead_of_starting_with_the_lock() {
        let mut unlock = FaceUnlock::default();
        let locked = Instant::now();
        assert!(!unlock.wants_start(locked, Trigger::Press), "not locked yet");
        unlock.begin_lock(locked);
        assert!(!unlock.wants_start(locked, Trigger::Press), "a key right after Super+L does not look");
        assert!(unlock.wants_start(locked + GRACE, Trigger::Press));
        assert!(unlock.wants_start(locked, Trigger::Wake), "waking a blank screen looks at once");
        assert!(unlock.wants_start(locked, Trigger::Retry));

        running(&mut unlock);
        assert!(!unlock.wants_start(locked + GRACE * 5, Trigger::Press), "one run at a time");
        unlock.stop();
        assert!(!unlock.wants_start(locked + GRACE * 5, Trigger::Press), "a press does not look twice per wake");
        assert!(unlock.wants_start(locked + GRACE * 5, Trigger::Retry), "the glyph always retries");
        unlock.screen_off();
        assert!(unlock.wants_start(locked + GRACE * 5, Trigger::Wake), "the next wake looks again");
        unlock.end_lock();
        assert!(!unlock.wants_start(locked + GRACE * 5, Trigger::Retry));
    }

    #[test]
    fn only_a_clean_pass_of_the_current_run_unlocks() {
        let mut unlock = FaceUnlock::default();
        unlock.begin_lock(Instant::now());
        let generation = running(&mut unlock);

        let stale = event(generation - 1, Event::Finished { success: true, reason: Reason::Passed });
        assert_eq!(unlock.receive(&stale), Outcome::Nothing);
        let stale_end = Message { generation: generation - 1, note: Note::Ended { exited_cleanly: true } };
        assert_eq!(unlock.receive(&stale_end), Outcome::Nothing, "an earlier lock's pass never opens this one");
        assert!(unlock.running());

        let started = event(generation, Event::Started { camera: "cam".into() });
        assert_eq!(unlock.receive(&started), Outcome::Show(Some(FaceState::Looking)));
        let pass = event(generation, Event::Finished { success: true, reason: Reason::Passed });
        assert_eq!(unlock.receive(&pass), Outcome::Nothing, "the exit status confirms the line");
        let failed_exit = Message { generation, note: Note::Ended { exited_cleanly: false } };
        assert_eq!(unlock.receive(&failed_exit), Outcome::Nothing, "a pass line with a failing exit is refused");
        assert!(!unlock.running());

        let generation = running(&mut unlock);
        unlock.receive(&event(generation, Event::Finished { success: true, reason: Reason::Passed }));
        let ended = Message { generation, note: Note::Ended { exited_cleanly: true } };
        assert_eq!(unlock.receive(&ended), Outcome::Unlock);

        let generation = running(&mut unlock);
        let ended = Message { generation, note: Note::Ended { exited_cleanly: true } };
        assert_eq!(unlock.receive(&ended), Outcome::Nothing, "an exit without a pass line does not unlock");

        let generation = running(&mut unlock);
        unlock.receive(&event(generation, Event::Finished { success: true, reason: Reason::Passed }));
        unlock.stop();
        let ended = Message { generation, note: Note::Ended { exited_cleanly: true } };
        assert_eq!(unlock.receive(&ended), Outcome::Nothing, "a run stopped by a password submit is forgotten");
    }

    fn run_script(body: &str) -> Vec<Outcome> {
        let script = std::env::temp_dir().join(format!("glide-face-unlock-run-{}-{}", std::process::id(), body.len()));
        std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).expect("write the stand-in");
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("chmod");
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut unlock = FaceUnlock::default();
        unlock.begin_lock(Instant::now());
        unlock.start(&script, move |message| drop(sender.send(message))).expect("the stand-in starts");
        let mut outcomes = Vec::new();
        while let Ok(message) = receiver.recv_timeout(Duration::from_secs(10)) {
            let ended = matches!(message.note, Note::Ended { .. });
            outcomes.push(unlock.receive(&message));
            if ended {
                break;
            }
        }
        let _ = std::fs::remove_file(&script);
        outcomes
    }

    #[test]
    fn a_stand_in_facekey_streams_its_lines_and_its_exit_into_the_outcome() {
        let pass = run_script(
            r#"echo '{"kind":"started","camera":"cam"}'
echo 'not json'
echo '{"kind":"finished","success":true,"reason":"passed"}'
exit 0"#,
        );
        assert_eq!(pass, vec![Outcome::Show(Some(FaceState::Looking)), Outcome::Nothing, Outcome::Unlock]);

        let lie = run_script(
            r#"echo '{"kind":"finished","success":true,"reason":"passed"}'
exit 1"#,
        );
        assert_eq!(lie, vec![Outcome::Nothing, Outcome::Nothing], "a failing exit cancels a pass line");

        let miss = run_script(
            r#"echo '{"kind":"started","camera":"cam"}'
echo '{"kind":"finished","success":false,"reason":"mismatch"}'
exit 1"#,
        );
        assert_eq!(
            miss,
            vec![Outcome::Show(Some(FaceState::Looking)), Outcome::Show(Some(FaceState::Rejected)), Outcome::Nothing]
        );
    }

    #[test]
    fn the_lock_screen_keeps_looking_on_its_own_until_the_screen_goes_dark() {
        let mut unlock = FaceUnlock::default();
        let locked = Instant::now();
        unlock.begin_lock(locked);
        assert_eq!(unlock.due(), Some(locked + LOCK_GRACE), "after Super+L there is time to walk away");
        assert!(!unlock.wants_start(locked, Trigger::Auto));
        assert!(unlock.wants_start(locked + LOCK_GRACE, Trigger::Auto), "then it looks without a key press");

        let generation = running(&mut unlock);
        assert_eq!(unlock.due(), None, "no second look while one runs");
        unlock.receive(&event(generation, Event::Finished { success: false, reason: Reason::NoFace }));
        let ended = Message { generation, note: Note::Ended { exited_cleanly: false } };
        assert_eq!(unlock.receive(&ended), Outcome::Nothing);
        let next = unlock.due().expect("nobody was there, so it looks again soon");
        assert!(next <= Instant::now() + AWAY_RETRY);

        let generation = running(&mut unlock);
        unlock.receive(&event(generation, Event::Finished { success: false, reason: Reason::NotEnrolled }));
        unlock.receive(&Message { generation, note: Note::Ended { exited_cleanly: false } });
        assert_eq!(unlock.due(), None, "a face that cannot work is not retried");

        unlock.begin_lock(locked);
        unlock.screen_off();
        assert_eq!(unlock.due(), None, "a dark screen keeps the camera off");
        unlock.begin_lock(locked);
        unlock.hold();
        assert_eq!(unlock.due(), None, "a typed password stops the looking");
    }

    #[test]
    fn reasons_choose_what_the_lock_screen_shows() {
        assert_eq!(shown_for(Reason::Mismatch), Some(FaceState::Rejected));
        assert_eq!(shown_for(Reason::Spoof), Some(FaceState::Rejected));
        assert_eq!(shown_for(Reason::Timeout), Some(FaceState::Looking), "nobody there: keep looking quietly");
        assert_eq!(shown_for(Reason::NoFace), Some(FaceState::Looking));
        assert_eq!(shown_for(Reason::CameraBusy), Some(FaceState::CameraBusy));
        assert_eq!(shown_for(Reason::NotEnrolled), None, "no face added, nothing to show");
        assert_eq!(shown_for(Reason::NoCamera), None);
    }
}
