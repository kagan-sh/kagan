use std::io;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use async_process::Child;

/// Owns a child and kills its entire tree on drop, including after the child exits.
pub struct Process {
    child: Child,
    killed: bool,
    #[cfg(windows)]
    job: Option<crate::Job>,
}

impl Process {
    /// Starts `command` in its own process group or Windows Job Object.
    ///
    /// `stdin`, `stdout`, and `stderr` are the child's stdio. On Windows the child is created
    /// suspended and resumed only after it is in the job, so a grandchild cannot start outside it.
    ///
    /// # Errors
    /// If the child cannot start or its tree cannot be owned.
    pub fn spawn(
        mut command: std::process::Command,
        stdin: std::process::Stdio,
        stdout: std::process::Stdio,
        stderr: std::process::Stdio,
    ) -> io::Result<Self> {
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(
            &mut command,
            windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
                | windows_sys::Win32::System::Threading::CREATE_SUSPENDED,
        );
        let child = async_process::Command::from(command)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()?;
        let process = Self {
            child,
            killed: false,
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        let mut process = process;
        #[cfg(windows)]
        {
            process.job = Some(crate::Job::assign(process.child.id())?);
            crate::job::resume(process.child.id())?;
        }
        Ok(process)
    }

    /// The owned child's pipes and exit status.
    pub fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Kills every process in the tree.
    pub fn kill(&mut self) {
        if std::mem::replace(&mut self.killed, true) {
            return;
        }
        // The tree may already have exited; nothing is left to report to.
        #[cfg(unix)]
        if let Some(pid) = rustix::process::Pid::from_raw(self.child.id().cast_signed()) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        #[cfg(windows)]
        if let Some(job) = self.job.take() {
            job.terminate();
        }
        let _ = self.child.kill();
    }

    /// Waits up to `limit` for the child to exit, then kills the tree and returns `None`. When
    /// `stop` returns true first, it kills the tree and fails.
    ///
    /// # Errors
    /// When the child's status cannot be read, or `stop` returns true (`Interrupted`).
    pub fn wait_within(
        &mut self,
        limit: Duration,
        stop: impl Fn() -> bool,
    ) -> io::Result<Option<ExitStatus>> {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_status()? {
                return Ok(Some(status));
            }
            if stop() {
                self.kill();
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "the program was stopped",
                ));
            }
            if started.elapsed() > limit {
                self.kill();
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}
