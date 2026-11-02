use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const POLL_STEP: Duration = Duration::from_millis(25);

#[derive(Debug)]
pub enum Failure {
    Spawn(std::io::Error),
    Timeout,
}

struct Drained {
    bytes: Vec<u8>,
    complete: bool,
}

fn kill_group(leader: u32) {
    let Ok(group) = libc::pid_t::try_from(leader) else {
        return;
    };
    // SAFETY: killpg only sends a signal; the group was created for this child by process_group(0).
    unsafe { libc::killpg(group, libc::SIGKILL) };
}

pub fn run(command: &mut Command, input: Option<&[u8]>, timeout: Duration) -> Result<Output, Failure> {
    let mut child = command
        .process_group(0)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Failure::Spawn)?;
    let leader = child.id();
    let deadline = Instant::now() + timeout;
    let (stdout, stderr) = (drain(child.stdout.take(), deadline), drain(child.stderr.take(), deadline));
    let stdin = child.stdin.take();
    let (status, wrote) = std::thread::scope(|scope| {
        let writer = input.zip(stdin).map(|(mut input, mut stdin)| {
            scope.spawn(move || {
                nonblocking(&stdin).map_err(Failure::Spawn)?;
                while !input.is_empty() {
                    if Instant::now() >= deadline {
                        return Ok(false);
                    }
                    match stdin.write(input) {
                        Ok(0) => break,
                        Ok(count) => input = &input[count..],
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(POLL_STEP),
                        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                Ok(true)
            })
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_STEP),
                _ => {
                    kill_group(leader);
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let wrote = writer.map_or(Ok(true), |writer| writer.join().unwrap_or(Ok(false)));
        (status, wrote)
    });
    let lost = || Drained { bytes: Vec::new(), complete: false };
    let stdout = stdout.join().unwrap_or_else(|_| Ok(lost()));
    let stderr = stderr.join().unwrap_or_else(|_| Ok(lost()));
    let finished = matches!((&wrote, &stdout, &stderr), (Ok(true), Ok(out), Ok(err)) if out.complete && err.complete);
    if status.is_some() && !finished {
        kill_group(leader);
    }
    let status = status.ok_or(Failure::Timeout)?;
    wrote?;
    Ok(Output { status, stdout: stdout?.bytes, stderr: stderr?.bytes })
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
) -> JoinHandle<Result<Drained, Failure>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            nonblocking(&pipe).map_err(Failure::Spawn)?;
            let mut buffer = [0; 8192];
            loop {
                if Instant::now() >= deadline {
                    return Ok(Drained { bytes, complete: false });
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
        Ok(Drained { bytes, complete: true })
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

    fn gone(pid: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                Err(_) => return true,
                Ok(stat) if stat.rsplit(')').next().is_some_and(|rest| rest.trim_start().starts_with('Z')) => {
                    return true;
                }
                Ok(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        false
    }

    #[test]
    fn descendants_holding_output_pipes_are_killed_at_the_deadline_and_the_exited_parents_output_is_kept() {
        let started = Instant::now();
        let result = run(Command::new("sh").args(["-c", "sleep 5 & echo $!"]), None, Duration::from_millis(300));
        let output = result.expect("the direct child exited, so its output counts");
        assert!(output.status.success());
        assert!(started.elapsed() < Duration::from_secs(1));
        let pid = String::from_utf8(output.stdout).expect("a pid");
        assert!(gone(pid.trim()), "the background sleep {pid} shares the process group and is killed");
    }

    #[test]
    fn descendants_holding_stdin_cannot_block_the_writer_after_the_parent_exits() {
        let input = vec![b'x'; 1024 * 1024];
        let started = Instant::now();
        let result = run(
            Command::new("sh").args(["-c", "sleep 1 <&0 >/dev/null 2>&1 &"]),
            Some(&input),
            Duration::from_millis(100),
        );
        assert!(result.is_ok_and(|output| output.status.success()));
        assert!(started.elapsed() < Duration::from_millis(700));
    }

    #[test]
    fn a_program_still_running_at_the_deadline_is_a_timeout_and_its_children_die_with_it() {
        let marker = std::env::temp_dir().join(format!("glide-process-group-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let script = format!("(sleep 0.5; touch {}) & sleep 5", marker.display());
        let result = run(Command::new("sh").args(["-c", &script]), None, Duration::from_millis(100));
        assert!(matches!(result, Err(Failure::Timeout)), "{result:?}");
        std::thread::sleep(Duration::from_millis(800));
        assert!(!marker.exists(), "the grandchild was killed with the group");
    }
}
