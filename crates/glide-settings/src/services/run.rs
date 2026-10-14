use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::channel::oneshot;

pub const SETTLE_DELAY: Duration = Duration::from_millis(400);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Debounce(Generation);

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
pub struct Generation(u64);

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

fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn run_within(program: &str, args: &[&str], deadline: Duration) -> Result<Finished, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{program}: {err}"))?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    let status = status.ok_or_else(|| format!("{program}: no answer in {}s", deadline.as_secs()))?;
    Ok(Finished { success: status.success(), stdout, stderr })
}

#[must_use]
pub fn output_within(program: &str, args: &[&str], deadline: Duration) -> Option<String> {
    run_within(program, args, deadline).ok().and_then(|finished| finished.success.then_some(finished.stdout))
}

pub fn status_within(program: &str, args: &[&str], deadline: Duration) -> Result<String, String> {
    let finished = run_within(program, args, deadline)?;
    settle(finished.success, &finished.stdout, &finished.stderr)
}

#[must_use]
pub fn available(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}
