use std::process::{Command, Stdio};
use std::time::Duration;

use glide_config::process::Failure;

use iced::Task;
use iced::futures::channel::oneshot;

pub const SETTLE_DELAY: Duration = Duration::from_millis(400);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Debounce(Generation);

impl Debounce {
    pub fn schedule<M, F>(&mut self, message: F) -> Task<M>
    where
        M: Send + 'static,
        F: FnOnce(u64) -> M + Send + 'static,
    {
        let generation = self.0.bump();
        Task::perform(blocking(|| std::thread::sleep(SETTLE_DELAY)), move |_| message(generation))
    }

    #[must_use]
    pub const fn is_current(self, generation: u64) -> bool {
        self.0.is_current(generation)
    }

    pub fn cancel(&mut self) {
        self.0.bump();
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Generation(u64);

impl Generation {
    pub fn bump(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }

    #[must_use]
    pub const fn current(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_current(self, generation: u64) -> bool {
        self.0 == generation
    }
}

#[derive(Debug, Default)]
pub(crate) struct Jobs {
    pub(crate) loading: bool,
    pub(crate) pending: u32,
    pub(crate) generation: Generation,
    pub(crate) busy: Option<String>,
    pub(crate) error: Option<String>,
}

impl Jobs {
    pub(crate) fn begin(&mut self, busy: Option<String>) {
        self.busy = busy;
        self.error = None;
        self.generation.bump();
        self.loading = false;
        self.pending += 1;
    }

    pub(crate) fn finish(&mut self, result: Option<Result<String, String>>) {
        self.pending = self.pending.saturating_sub(1);
        if self.pending == 0 {
            self.busy = None;
        }
        if let Some(Err(err)) = result {
            self.error = Some(err);
        }
        self.generation.bump();
        self.loading = false;
    }

    pub(crate) fn start_loading(&mut self) -> Option<u64> {
        if self.loading || self.pending > 0 {
            return None;
        }
        self.loading = true;
        Some(self.generation.current())
    }
}

pub async fn blocking<T, F>(job: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(job());
    });
    receiver.await.ok()
}

#[must_use]
pub fn output(program: &str, args: &[&str]) -> Option<String> {
    let result = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    result.status.success().then(|| String::from_utf8_lossy(&result.stdout).into_owned())
}

pub fn status(program: &str, args: &[&str]) -> Result<String, String> {
    let result =
        Command::new(program).args(args).stdin(Stdio::null()).output().map_err(|err| format!("{program}: {err}"))?;
    settle(result.status.success(), &String::from_utf8_lossy(&result.stdout), &String::from_utf8_lossy(&result.stderr))
}

fn settle(success: bool, stdout: &str, stderr: &str) -> Result<String, String> {
    let stdout = stdout.trim().to_string();
    if success {
        return Ok(stdout);
    }
    let stderr = stderr.trim();
    Err(if stderr.is_empty() { stdout } else { stderr.to_string() })
}

struct Finished {
    success: bool,
    stdout: String,
    stderr: String,
}

fn run_within(program: &str, args: &[&str], deadline: Duration) -> Result<Finished, String> {
    let output =
        glide_config::process::run(Command::new(program).args(args), None, deadline).map_err(
            |failure| match failure {
                Failure::Spawn(err) => format!("{program}: {err}"),
                Failure::Timeout => format!("{program}: no answer in {}s", deadline.as_secs()),
            },
        )?;
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    Ok(Finished { success: output.status.success(), stdout: text(&output.stdout), stderr: text(&output.stderr) })
}

#[must_use]
pub fn output_within(program: &str, args: &[&str], deadline: Duration) -> Option<String> {
    run_within(program, args, deadline).ok().and_then(|finished| finished.success.then_some(finished.stdout))
}

pub fn status_within(program: &str, args: &[&str], deadline: Duration) -> Result<String, String> {
    let finished = run_within(program, args, deadline)?;
    settle(finished.success, &finished.stdout, &finished.stderr)
}

pub use glide_config::on_path as available;
