//! Spawn `pkexec /usr/sbin/secureboot-fix` (or a mock replacement) and stream
//! parsed progress events to the UI.
//!
//! Output lines from `secureboot-fix`:
//!   `[STEPS] a|b|c`     — preamble listing the steps for this run
//!   `[STEP] a`          — entering step `a`
//!   `+ <command>`       — `run()` wrapper trace
//!   anything else       — passed through as a log line
//!
//! Cancel is a SIGTERM on the returned `Subprocess` handle.

use anyhow::Result;
use futures::channel::mpsc::{unbounded, UnboundedReceiver};
use gio::prelude::*;
use glib::MainContext;
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
pub enum FixEvent {
    /// First event — the full step list announced by the fix script.
    Steps(Vec<String>),
    /// Entered named step.
    Step(String),
    /// Log line (`+ cmd`, plain output, or stderr).
    LogLine(String),
    /// Subprocess exited with this status code.
    Done(i32),
    /// Spawn or IO error before exit.
    SpawnError(String),
}

/// How the fix subprocess should be launched.
pub enum FixMode<'a> {
    /// Real: spawn `pkexec /usr/sbin/secureboot-fix --code CODE`.
    Pkexec { code: &'a str },
    /// Mock: spawn `sh -c <embedded canned-output script>`.
    Mock { script: &'a str },
}

pub struct FixHandle {
    pub subprocess: gio::Subprocess,
    pub events: UnboundedReceiver<FixEvent>,
}

pub fn spawn(mode: FixMode<'_>, password: Option<Zeroizing<Vec<u8>>>) -> Result<FixHandle> {
    let (argv, env_passthrough): (Vec<String>, bool) = match mode {
        FixMode::Pkexec { code } => (
            vec![
                "pkexec".into(),
                "/usr/sbin/secureboot-fix".into(),
                "--code".into(),
                code.into(),
            ],
            false,
        ),
        FixMode::Mock { script } => (vec!["sh".into(), "-c".into(), script.into()], true),
    };
    let _ = env_passthrough;

    let argv_ref: Vec<&str> = argv.iter().map(String::as_str).collect();
    let subprocess = gio::Subprocess::newv(
        &argv_ref
            .iter()
            .map(std::ffi::OsStr::new)
            .collect::<Vec<_>>(),
        gio::SubprocessFlags::STDIN_PIPE
            | gio::SubprocessFlags::STDOUT_PIPE
            | gio::SubprocessFlags::STDERR_MERGE,
    )?;

    let (tx, rx) = unbounded::<FixEvent>();
    let proc_for_writer = subprocess.clone();
    let proc_for_reader = subprocess.clone();
    let proc_for_wait = subprocess.clone();
    let tx_writer = tx.clone();
    let tx_reader = tx.clone();
    let tx_wait = tx;

    // Writer: send password if any, then close stdin so secureboot-fix's
    // `IFS= read` doesn't hang for codes that don't need a secret.
    MainContext::default().spawn_local(async move {
        let Some(stdin) = proc_for_writer.stdin_pipe() else {
            return;
        };
        if let Some(pw) = password {
            let mut buf: Vec<u8> = pw.iter().copied().collect();
            buf.push(b'\n');
            if let Err(e) = stdin.write_all_future(buf, glib::Priority::DEFAULT).await {
                let _ = tx_writer
                    .unbounded_send(FixEvent::SpawnError(format!("writing stdin: {}", e.1)));
            }
            // The Zeroizing<Vec<u8>> in `pw` will zero on drop; our `buf`
            // copy was consumed by write_all_future.
        }
        let _ = stdin.close_future(glib::Priority::DEFAULT).await;
    });

    // Reader: line-by-line stdout parser → events.
    MainContext::default().spawn_local(async move {
        let Some(stdout) = proc_for_reader.stdout_pipe() else {
            return;
        };
        let data_in = gio::DataInputStream::new(&stdout);
        loop {
            match data_in.read_line_future(glib::Priority::DEFAULT).await {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        break;
                    }
                    let line = String::from_utf8_lossy(&bytes).into_owned();
                    let event = parse_line(&line);
                    if tx_reader.unbounded_send(event).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ =
                        tx_reader.unbounded_send(FixEvent::SpawnError(format!("read stdout: {e}")));
                    break;
                }
            }
        }
    });

    // Waiter: emit Done with exit code.
    MainContext::default().spawn_local(async move {
        match proc_for_wait.wait_check_future().await {
            Ok(()) => {
                let _ = tx_wait.unbounded_send(FixEvent::Done(0));
            }
            Err(e) => {
                // wait_check fails on any non-zero. Use wait() + exit_status() for
                // the actual code; if signal-killed, code is wait_status & 0xff.
                let _ = proc_for_wait.wait_future().await;
                let rc = if let Some(status) = proc_for_wait.exit_status().into() {
                    let _ = status;
                    // exit_status is GLib's wait-status int; pull out the high byte.
                    let raw = proc_for_wait.exit_status();
                    if raw == 0 {
                        let _ = e;
                        -1
                    } else {
                        // POSIX: high 8 bits are exit code when normal exit.
                        (raw >> 8) & 0xff
                    }
                } else {
                    -1
                };
                let _ = tx_wait.unbounded_send(FixEvent::Done(rc));
            }
        }
    });

    Ok(FixHandle {
        subprocess,
        events: rx,
    })
}

fn parse_line(line: &str) -> FixEvent {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    if let Some(rest) = trimmed.strip_prefix("[STEPS] ") {
        let steps: Vec<String> = rest.split('|').map(|s| s.trim().to_string()).collect();
        FixEvent::Steps(steps)
    } else if let Some(rest) = trimmed.strip_prefix("[STEP] ") {
        FixEvent::Step(rest.trim().to_string())
    } else {
        FixEvent::LogLine(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_steps_preamble() {
        let e = parse_line("[STEPS] clear-lockout|wipe-old-slot|seal-new-slot\n");
        match e {
            FixEvent::Steps(v) => {
                assert_eq!(v, vec!["clear-lockout", "wipe-old-slot", "seal-new-slot"]);
            }
            _ => panic!("expected Steps"),
        }
    }

    #[test]
    fn parse_step_advance() {
        let e = parse_line("[STEP] seal-new-slot\n");
        assert!(matches!(e, FixEvent::Step(s) if s == "seal-new-slot"));
    }

    #[test]
    fn parse_log_line() {
        let e = parse_line("+ systemd-cryptenroll --wipe-slot=tpm2 /dev/foo\n");
        assert!(matches!(e, FixEvent::LogLine(s) if s.contains("systemd-cryptenroll")));
    }
}
