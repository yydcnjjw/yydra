// SPDX-License-Identifier: MIT OR Apache-2.0

//! Capture subprocess output without pipe deadlocks; bound probes and reap cancelled children.

use std::io::{Read, Seek};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result, bail};

pub(crate) fn capture(
    mut command: Command,
    shutdown: &AtomicBool,
    timeout: Option<Duration>,
) -> Result<Output> {
    if shutdown.load(Ordering::SeqCst) {
        bail!("cancelled before starting tool");
    }
    let mut stdout = tempfile::tempfile().context("create stdout capture")?;
    let mut stderr = tempfile::tempfile().context("create stderr capture")?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = CapturedChild::spawn(command).context("start tool")?;
    let started = Instant::now();
    let status = loop {
        if shutdown.load(Ordering::SeqCst) {
            bail!("cancelled while running tool");
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            bail!("tool exceeded its diagnostic timeout");
        }
        if let Some(status) = child.child.try_wait().context("wait for tool")? {
            child.disarm();
            break status;
        }
        thread::sleep(Duration::from_millis(25));
    };
    stdout.rewind()?;
    stderr.rewind()?;
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    stdout.read_to_end(&mut output.stdout)?;
    stderr.read_to_end(&mut output.stderr)?;
    Ok(output)
}

struct CapturedChild {
    child: Child,
    armed: bool,
    #[cfg(windows)]
    job: Option<crate::WindowsJob>,
}

impl CapturedChild {
    fn spawn(mut command: Command) -> std::io::Result<Self> {
        #[allow(unused_mut)]
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let job = match crate::create_kill_on_close_job(&child) {
            Ok(job) => Some(job),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::other(format!(
                    "place child in Windows Job Object: {error:#}"
                )));
            }
        };
        Ok(Self {
            child,
            armed: true,
            #[cfg(windows)]
            job,
        })
    }

    fn disarm(&mut self) {
        #[cfg(unix)]
        if let Ok(group) = i32::try_from(self.child.id()) {
            // Match the Windows Job lifetime when a tool leaves background workers.
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(group),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        self.armed = false;
        #[cfg(windows)]
        drop(self.job.take());
    }
}

impl Drop for CapturedChild {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        #[cfg(windows)]
        drop(self.job.take());
        #[cfg(unix)]
        if let Ok(group) = i32::try_from(self.child.id()) {
            use nix::sys::signal::{Signal, killpg};
            use nix::unistd::Pid;
            let _ = killpg(Pid::from_raw(group), Signal::SIGTERM);
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
            let _ = killpg(Pid::from_raw(group), Signal::SIGKILL);
        }
        #[cfg(not(any(unix, windows)))]
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn completed_probe_reaps_background_workers() {
        let sandbox = tempfile::tempdir().unwrap();
        let marker = sandbox.path().join("leaked");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(/bin/sleep 0.2; echo leaked > \"$1\") & echo version",
                "probe",
            ])
            .arg(&marker);
        let output = capture(
            command,
            &AtomicBool::new(false),
            Some(Duration::from_secs(1)),
        )
        .unwrap();
        assert!(output.status.success());
        thread::sleep(Duration::from_millis(300));
        assert!(!marker.exists(), "a background probe survived its parent");
    }

    #[test]
    fn probe_timeout_reaps_the_child_group() {
        let sandbox = tempfile::tempdir().unwrap();
        let marker = sandbox.path().join("leaked");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(/bin/sleep 0.2; echo leaked > \"$1\") & wait",
                "probe",
            ])
            .arg(&marker);
        let error = capture(
            command,
            &AtomicBool::new(false),
            Some(Duration::from_millis(50)),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timeout"));
        thread::sleep(Duration::from_millis(300));
        assert!(!marker.exists(), "a timed-out tool left background work");
    }
}
