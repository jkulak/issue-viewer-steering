//! Running a helper program (git, gh) with a time limit, reading everything it writes.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Finished {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub enum Failed {
    /// The program could not be started, most often because it is not installed.
    Start(std::io::Error),
    /// It ran longer than the limit and was stopped.
    TimedOut,
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut out);
        }
        out
    })
}

/// Run `command` with nothing on stdin, stopping it after `limit`.
pub fn run(command: &mut Command, limit: Duration) -> Result<Finished, Failed> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Failed::Start)?;
    // Read while waiting, so a long answer cannot fill a pipe and stall the program.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Finished {
                    success: status.success(),
                    stdout: stdout.join().unwrap_or_default(),
                    stderr: stderr.join().unwrap_or_default(),
                });
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failed::TimedOut);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_outputs_and_the_exit_status_are_kept() {
        let finished = run(
            Command::new("sh").args(["-c", "echo out; echo err >&2; exit 3"]),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(!finished.success);
        assert_eq!(finished.stdout, b"out\n");
        assert_eq!(finished.stderr, b"err\n");
    }

    #[test]
    fn a_program_that_takes_too_long_is_stopped() {
        let started = Instant::now();
        let result = run(
            Command::new("sh").args(["-c", "sleep 5"]),
            Duration::from_millis(100),
        );
        assert!(matches!(result, Err(Failed::TimedOut)));
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn a_missing_program_cannot_start() {
        let result = run(
            &mut Command::new("/nonexistent/program"),
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(Failed::Start(_))));
    }
}
