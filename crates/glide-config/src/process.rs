use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const POLL_STEP: Duration = Duration::from_millis(25);

#[derive(Debug)]
pub enum Failure {
    Spawn(std::io::Error),
    Timeout,
}

pub fn run(command: &mut Command, input: Option<&[u8]>, timeout: Duration) -> Result<Output, Failure> {
    let mut child = command
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Failure::Spawn)?;
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let deadline = Instant::now() + timeout;
    let stdin = child.stdin.take();
    let status = std::thread::scope(|scope| {
        if let (Some(input), Some(mut stdin)) = (input, stdin) {
            scope.spawn(move || {
                let _ = stdin.write_all(input);
            });
        }
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_STEP),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Failure::Timeout);
                }
            }
        }
    })?;
    Ok(Output { status, stdout: stdout.join().unwrap_or_default(), stderr: stderr.join().unwrap_or_default() })
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_reaches_the_program_and_a_hung_one_is_killed_at_the_deadline() {
        let echoed = run(&mut Command::new("cat"), Some(b"hunter22\n"), Duration::from_secs(5)).expect("cat runs");
        assert!(echoed.status.success());
        assert_eq!(echoed.stdout, b"hunter22\n");
        let started = Instant::now();
        let hung = run(Command::new("sleep").arg("5"), None, Duration::from_millis(100));
        assert!(matches!(hung, Err(Failure::Timeout)) && started.elapsed() < Duration::from_secs(2));
        assert!(matches!(
            run(&mut Command::new("/nonexistent/glide"), None, Duration::from_secs(1)),
            Err(Failure::Spawn(_))
        ));
    }

    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_stall_the_child() {
        let output = run(Command::new("head").args(["-c", "200000", "/dev/zero"]), None, Duration::from_secs(10))
            .expect("head finishes before the timeout");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 200_000);
    }
}
