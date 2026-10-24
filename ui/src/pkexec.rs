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
use futures::AsyncBufReadExt;
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
    /// Normal exit code, or the negative signal number for signal termination.
    Done(i32),
    /// Spawn, IO, or wait error.
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
        // Read bytes so EOF is an explicit zero-length read, without the
        // nullable EOF buffer in GIO's read_line binding. Keep lossy UTF-8 logs.
        let mut data_in = stdout.into_async_buf_read(4096);
        loop {
            let mut bytes = Vec::new();
            match data_in.read_until(b'\n', &mut bytes).await {
                Ok(0) => break,
                Ok(_) => {
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

    // GIO already decodes the wait status. Only a confirmed normal exit 0
    // may report success; negative signal numbers keep termination distinct.
    MainContext::default().spawn_local(async move {
        let event = match proc_for_wait.wait_future().await {
            Err(e) => FixEvent::SpawnError(format!("waiting for subprocess: {e}")),
            Ok(()) if proc_for_wait.has_exited() => FixEvent::Done(proc_for_wait.exit_status()),
            Ok(()) if proc_for_wait.has_signaled() => FixEvent::Done(-proc_for_wait.term_sig()),
            Ok(()) => {
                FixEvent::SpawnError("subprocess ended without an exit code or signal".into())
            }
        };
        let _ = tx_wait.unbounded_send(event);
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
    use futures::{future, StreamExt};

    #[test]
    fn real_subprocess_completion_events() {
        let context = MainContext::default();
        // spawn() pumps events on the default context, just as in the UI.
        context
            .with_thread_default(|| {
                for (script, expected) in [
                    ("exit 0", 0),
                    ("exit 1", 1),
                    ("exit 126", 126),
                    ("exit 127", 127),
                    ("exit 42", 42),
                    ("kill -TERM $$", -15),
                ] {
                    context.block_on(async {
                        let output_script = format!(
                            "printf '[STEPS] check\\n[STEP] check\\n\\nlog\\377\\nlast'; {script}"
                        );
                        let handle = spawn(FixMode::Mock { script: &output_script }, None)
                            .expect("spawn shell");
                        let collect = handle.events.collect::<Vec<_>>();
                        let timeout = glib::timeout_future(std::time::Duration::from_secs(5));
                        futures::pin_mut!(collect, timeout);
                        let events = match future::select(collect, timeout).await {
                            future::Either::Left((events, _)) => events,
                            future::Either::Right(_) => {
                                handle.subprocess.force_exit();
                                handle.subprocess.wait_future().await.expect("reap shell");
                                panic!("timed out waiting for events: {script}");
                            }
                        };
                        let completion: Vec<_> = events.iter().filter(|event| {
                            matches!(event, FixEvent::Done(_) | FixEvent::SpawnError(_))
                        }).collect();
                        assert!(
                            matches!(completion.as_slice(), [FixEvent::Done(rc)] if *rc == expected),
                            "{script}: expected Done({expected}), got {events:?}"
                        );
                        let output: Vec<_> = events.iter().filter(|event| {
                            !matches!(event, FixEvent::Done(_) | FixEvent::SpawnError(_))
                        }).collect();
                        assert!(matches!(output.as_slice(), [
                            FixEvent::Steps(steps),
                            FixEvent::Step(step),
                            FixEvent::LogLine(blank),
                            FixEvent::LogLine(log),
                            FixEvent::LogLine(last),
                        ] if steps == &["check"] && step == "check" && blank.is_empty()
                            && log == "log\u{fffd}" && last == "last"), "{script}: {events:?}");
                        assert_eq!(
                            events.iter().any(|event| matches!(event, FixEvent::Done(0))),
                            expected == 0,
                            "only normal exit 0 may take the UI success path: {script}"
                        );
                    });
                }
            })
            .expect("acquire the default main context");
    }

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
