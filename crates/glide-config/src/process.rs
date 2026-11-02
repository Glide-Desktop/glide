use std::io::{Read, Write};
use std::os::fd::AsRawFd;
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
    let deadline = Instant::now() + timeout;
    let (stdout, stderr) = (drain(child.stdout.take(), deadline), drain(child.stderr.take(), deadline));
    let stdin = child.stdin.take();
    let status = std::thread::scope(|scope| {
        let writer = input.zip(stdin).map(|(mut input, mut stdin)| {
            scope.spawn(move || {
                nonblocking(&stdin).map_err(Failure::Spawn)?;
                while !input.is_empty() {
                    if Instant::now() >= deadline {
                        return Err(Failure::Timeout);
                    }
                    match stdin.write(input) {
                        Ok(0) => break,
                        Ok(count) => input = &input[count..],
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL_STEP),
                        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                Ok(())
            })
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_STEP),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Failure::Timeout);
                }
            }
        };
        if let Some(writer) = writer {
            writer.join().unwrap_or(Err(Failure::Timeout))?;
        }
        status
    });
    let stdout = stdout.join().unwrap_or(Err(Failure::Timeout));
    let stderr = stderr.join().unwrap_or(Err(Failure::Timeout));
    Ok(Output { status: status?, stdout: stdout?, stderr: stderr? })
}

fn nonblocking(pipe: &impl AsRawFd) -> std::io::Result<()> {
    // SAFETY: fcntl operates on the live owned pipe descriptor without transferring ownership.
    let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the descriptor is live and O_NONBLOCK is a valid file status flag.
    if unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn drain(
    pipe: Option<impl Read + AsRawFd + Send + 'static>, deadline: Instant,
) -> JoinHandle<Result<Vec<u8>, Failure>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            nonblocking(&pipe).map_err(Failure::Spawn)?;
            let mut buffer = [0; 8192];
            loop {
                if Instant::now() >= deadline {
                    return Err(Failure::Timeout);
                }
                match pipe.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => bytes.extend_from_slice(&buffer[..count]),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL_STEP),
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(err) => return Err(Failure::Spawn(err)),
                }
            }
        }
        Ok(bytes)
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

    #[test]
    fn descendants_holding_output_pipes_cannot_outlive_the_deadline() {
        let started = Instant::now();
        let result = run(Command::new("sh").args(["-c", "sleep 1 &"]), None, Duration::from_millis(100));
        assert!(matches!(result, Err(Failure::Timeout)), "{result:?}");
        assert!(started.elapsed() < Duration::from_millis(700));
    }

    #[test]
    fn descendants_holding_stdin_cannot_block_the_writer_after_parent_exit() {
        let input = vec![b'x'; 1024 * 1024];
        let started = Instant::now();
        let result = run(
            Command::new("sh").args(["-c", "sleep 1 <&0 >/dev/null 2>&1 &"]),
            Some(&input),
            Duration::from_millis(100),
        );
        assert!(matches!(result, Err(Failure::Timeout)), "{result:?}");
        assert!(started.elapsed() < Duration::from_millis(700));
    }
}
