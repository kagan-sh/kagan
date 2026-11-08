use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::Launch;
use super::Terminal;
#[cfg(unix)]
use super::shared::lock;

fn shell_launch() -> Launch {
    Launch {
        program: shell_program().to_os_string(),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
    }
}

fn shell_program() -> &'static OsStr {
    if cfg!(windows) {
        OsStr::new("cmd.exe")
    } else {
        OsStr::new("/bin/sh")
    }
}

fn marker_command() -> Vec<u8> {
    if cfg!(windows) {
        b"echo kagan-marker\r\n".to_vec()
    } else {
        b"printf 'kagan-marker\\n'\n".to_vec()
    }
}

fn sh(script: &str) -> Launch {
    Launch {
        program: OsString::from("/bin/sh"),
        args: vec![OsString::from("-c"), OsString::from(script)],
        cwd: None,
        env: Vec::new(),
    }
}
fn wait_for(terminal: &Terminal, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = terminal.view().screen();
        if frame.contains(needle) || Instant::now() > deadline {
            return frame;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn quit_within(terminal: Terminal, limit: Duration) {
    within(limit, move || drop(terminal)).expect("quit did not return");
}

fn within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.recv_timeout(limit).ok()
}

fn alive(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    #[cfg(unix)]
    {
        // A zombie has exited and waits only to be reaped, so it is not alive.
        let signalable = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        signalable
            && !Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .is_ok_and(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .trim_start()
                        .starts_with('Z')
                })
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|word| word == pid.to_string())
            })
    }
}

struct Reap(u32);

impl Drop for Reap {
    fn drop(&mut self) {
        if self.0 <= 1 {
            return;
        }
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                // Without `--`, Linux `kill -9 -7551` signalled group 7, not 7551.
                .args(["-9", "--", &format!("-{}", self.0)])
                .stderr(Stdio::null())
                .status();
            let _ = Command::new("kill")
                .args(["-9", &self.0.to_string()])
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &self.0.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}
#[cfg(unix)]
impl Terminal {
    /// Makes the reader withhold end of file until `gate` is used, and reads the drain cap from `clock`.
    pub(crate) fn controlled(
        mut self,
        gate: std::sync::mpsc::Receiver<()>,
        clock: Arc<AtomicU64>,
    ) -> Self {
        *lock(&self.view.shared.eof_gate) = Some(gate);
        self.clock = Some(clock);
        self
    }

    /// Makes the teardown wait before it publishes; the sender lets it go.
    pub(crate) fn gate_publication(&self) -> std::sync::mpsc::Sender<()> {
        let (release, gate) = std::sync::mpsc::channel();
        *lock(&self.view.shared.publish_gate) = Some(gate);
        release
    }
}

mod finish;
mod frames;
mod input;
mod lifecycle;
mod pty;
