use std::io;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};

use windows_sys::Win32::Foundation::{FALSE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenThread, PROCESS_SET_QUOTA, PROCESS_TERMINATE, ResumeThread,
    THREAD_SUSPEND_RESUME,
};

/// A Windows Job Object that kills every process in it when it is terminated or dropped.
#[derive(Debug)]
pub struct Job(OwnedHandle);

impl Job {
    /// Puts a running process, and every process it starts from now on, in a new job.
    ///
    /// # Errors
    /// If the job cannot be made or the process cannot be opened or assigned.
    pub fn assign(pid: u32) -> io::Result<Self> {
        // SAFETY: null attributes and name create an anonymous job.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: this is a valid newly owned handle.
        let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
        // SAFETY: zero initializes a valid "no limits" value.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(io::Error::other)?;
        // SAFETY: info is live and its size matches the supplied buffer.
        if unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: plain call; null is checked before taking ownership.
        let handle = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, FALSE, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: handle is newly opened and owned.
        let process = unsafe { OwnedHandle::from_raw_handle(handle) };
        // SAFETY: both handles remain open for the call.
        if unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), process.as_raw_handle()) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Kills every process in the job now.
    pub fn terminate(&self) {
        // SAFETY: the job handle is owned and open.
        unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) };
    }
}

pub(crate) fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: plain call; the invalid handle is checked before taking ownership.
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the snapshot is a valid newly owned handle.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(handle) };
    // SAFETY: all-zero fields are valid, and dwSize is set before enumeration.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = u32::try_from(size_of::<THREADENTRY32>()).map_err(io::Error::other)?;
    // SAFETY: entry is a writable buffer of the declared size.
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) };
    while found != 0 {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: plain call; null is checked before taking ownership.
            let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, FALSE, entry.th32ThreadID) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: the thread handle is newly owned.
            let thread = unsafe { OwnedHandle::from_raw_handle(handle) };
            // SAFETY: the child was created suspended and is already assigned to its job.
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            return Ok(());
        }
        // SAFETY: snapshot and writable entry remain valid.
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) };
    }
    Err(io::Error::other("the suspended child has no thread"))
}

#[cfg(test)]
mod tests {
    use std::os::windows::process::CommandExt as _;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    use super::{Job, resume};

    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const WAIT_LIMIT: Duration = Duration::from_secs(10);

    fn silent(command: &mut Command) -> &mut Command {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
    }

    fn wait_until_exit(child: &mut Child) -> ExitStatus {
        let deadline = Instant::now() + WAIT_LIMIT;
        loop {
            if let Some(status) = child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(Instant::now() < deadline, "the child did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn terminating_the_job_ends_the_process_in_it() {
        let mut child = silent(Command::new("cmd").args(["/C", "ping -n 60 127.0.0.1 >NUL"]))
            .spawn()
            .expect("spawn");
        let job = Job::assign(child.id()).expect("assign");
        job.terminate();
        let _ = wait_until_exit(&mut child);
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn resuming_a_suspended_child_lets_it_finish() {
        let mut child = silent(Command::new("cmd").args(["/C", "exit 0"]))
            .creation_flags(CREATE_SUSPENDED)
            .spawn()
            .expect("spawn");
        resume(child.id()).expect("resume");
        assert!(wait_until_exit(&mut child).success());
    }
}
