use std::sync::atomic::Ordering;
use std::time::Duration;

#[cfg(windows)]
use super::shared::lock;

/// Whether a process has finished, given whether its handle has signalled and the code it reads.
/// A running Windows process reads `259` (`STILL_ACTIVE`), and a program can exit with that code too,
/// so a code counts only once the handle has signalled.
#[cfg(any(windows, test))]
pub(super) fn finished_exit(signalled: bool, code: u32) -> Option<Exit> {
    signalled.then_some(Exit::Code(code))
}

/// How a program ended: its exit code, or the signal that ended it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Code(u32),
    Signal(i32),
}

/// How long output may keep arriving after the program exits. A background job that keeps the pty
/// open after its tree is killed cannot hold the exit back past this.
pub(super) const DRAIN: Duration = Duration::from_millis(500);

impl super::Terminal {
    /// How the program ended, once it has. `None` while it runs and until the 500 ms drain ends.
    #[must_use]
    pub fn exit(&self) -> Option<Exit> {
        if let Some(exit) = self.ended_with.get() {
            return Some(*exit);
        }
        let (exit, observed) = if let Some(&found) = self.observed.get() {
            found
        } else {
            let exit = self.observe()?;
            // The leader is gone, so its tree is what still holds the pty open. Kill it now, so
            // the output can end.
            #[cfg(unix)]
            kg_process::kill_tree(self.process_id);
            *self.observed.get_or_init(|| (exit, self.now()))
        };
        if !self.ended.load(Ordering::Acquire) && self.now().saturating_sub(observed) < DRAIN {
            return None;
        }
        Some(*self.ended_with.get_or_init(|| exit))
    }

    // The clock the drain cap is read from. A test can drive it.
    fn now(&self) -> Duration {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return Duration::from_millis(clock.load(Ordering::Acquire));
        }
        self.epoch.elapsed()
    }

    /// Asks the program and everything it started to end. Its screen is final once `exit` returns `Some`.
    pub fn stop(&self) {
        // Unix does not use `child.kill()`: portable-pty reaps the child after its SIGHUP, and then the
        // exit could never be read. The leader and its tree are killed by pid, and reaped later.
        #[cfg(unix)]
        {
            kg_process::kill_leader(self.process_id);
            kg_process::kill_tree(self.process_id);
        }
        #[cfg(windows)]
        {
            if let Some(job) = &self.job {
                job.terminate();
            }
            if let Some(child) = lock(&self.child).as_mut() {
                // The program may already have exited.
                let _ = child.kill();
            }
        }
    }

    #[cfg(unix)]
    fn observe(&self) -> Option<Exit> {
        crate::tree::exit(self.process_id)
    }

    #[cfg(windows)]
    fn observe(&self) -> Option<Exit> {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
        };
        // portable-pty keeps the child's handle open until the terminal ends, so the pid
        // cannot be reused while this runs.
        // SAFETY: the handle is opened here and closed on every path below; the out pointer is valid for the call.
        unsafe {
            let handle = OpenProcess(
                SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                self.process_id,
            );
            if handle.is_null() {
                return None;
            }
            let signalled = WaitForSingleObject(handle, 0) == WAIT_OBJECT_0;
            let mut code = 0_u32;
            let read = signalled && GetExitCodeProcess(handle, &raw mut code) != 0;
            CloseHandle(handle);
            finished_exit(read, code)
        }
    }

    /// Whether the program has ended. A background job it left can keep the pty open, so the
    /// output may not be at its end.
    #[must_use]
    pub fn exited(&self) -> bool {
        // ConPTY keeps the output open after the program exits, so ask the process too.
        self.observe().is_some() || self.ended.load(Ordering::Acquire)
    }
}
