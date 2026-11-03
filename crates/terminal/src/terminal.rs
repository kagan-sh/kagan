use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::screen::{Edit, MIN_COLS, SNAPSHOT_MAX, Screen, SelectMode};

mod ending;
mod finished;
mod lines;

use ending::Ending;
pub use finished::Finished;
pub use lines::{Kept, KeptLine, Lines};

pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
}

/// Whether a process has finished, given whether its handle has signalled and the code it reads.
/// A running Windows process reads `259` (`STILL_ACTIVE`), and a program can exit with that code too,
/// so a code counts only once the handle has signalled.
#[cfg(any(windows, test))]
fn finished_exit(signalled: bool, code: u32) -> Option<Exit> {
    signalled.then_some(Exit::Code(code))
}

/// How a program ended: its exit code, or the signal that ended it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Code(u32),
    Signal(i32),
}

enum Outgoing {
    Bytes(Vec<u8>),
    Checked {
        bytes: Vec<u8>,
        done: Sender<io::Result<()>>,
    },
}

pub struct Terminal {
    master: Option<Box<dyn MasterPty + Send>>,
    input: Sender<Outgoing>,
    child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    shared: Shared,
    ended: Arc<AtomicBool>,
    /// Finish sets this so a non-blocking write waiting on a full pty can exit.
    write_stop: Arc<AtomicBool>,
    process_id: u32,
    epoch: Instant,
    observed: OnceLock<(Exit, Duration)>,
    ended_with: OnceLock<Exit>,
    // Holds the program and what it starts; closing the handle kills them all.
    #[cfg(windows)]
    job: Option<kg_process::Job>,
    #[cfg(test)]
    clock: Option<Arc<AtomicU64>>,
    #[cfg(test)]
    fail_write: Arc<AtomicBool>,
}

/// How long output may keep arriving after the program exits. A background job that keeps the pty
/// open after its tree is killed cannot hold the exit back past this.
const DRAIN: Duration = Duration::from_millis(500);

impl Terminal {
    /// # Errors
    ///
    /// The screen is too small, or the program did not start.
    pub fn spawn(launch: Launch, cols: u16, rows: u16) -> io::Result<Self> {
        let size = pty_size(cols, rows)?;
        let pair = native_pty_system().openpty(size).map_err(io_error)?;
        let reader = pair.master.try_clone_reader().map_err(io_error)?;
        let writer = pair.master.take_writer().map_err(io_error)?;
        // take_writer clones the master fd. Non-blocking writes let finish cancel a
        // checked send that filled the buffer, instead of hanging in write_all.
        #[cfg(unix)]
        let wait_fd = pair.master.as_raw_fd();
        #[cfg(unix)]
        if let Some(fd) = wait_fd {
            // SAFETY: fd is the live pty master; O_NONBLOCK is per open file description.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags == -1 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
                return Err(io::Error::last_os_error());
            }
        }
        let mut command = CommandBuilder::new(launch.program);
        command.args(launch.args);
        if let Some(cwd) = launch.cwd {
            command.cwd(cwd);
        }
        for (key, value) in launch.env {
            command.env(key, value);
        }
        // Without TERM a shell cannot move the cursor back, so its redraws print after the text.
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let child = pair.slave.spawn_command(command).map_err(io_error)?;
        drop(pair.slave);
        let process_id = child
            .process_id()
            .ok_or_else(|| io::Error::other("program has no process id"))?;
        let screen = Arc::new(Mutex::new(Screen::new(cols, rows)));
        let frame = Arc::new(Mutex::new(Frame {
            version: 1,
            grid: Arc::new(lock(&screen).grid()),
        }));
        let shared = Shared {
            screen,
            frame,
            title: Arc::default(),
            versions: Arc::new(AtomicU64::new(1)),
            wanted: Arc::new(AtomicBool::new(false)),
            edits: Arc::new(Mutex::new(Vec::new())),
            changed: Arc::new(AtomicBool::new(false)),
            settled: Arc::new(AtomicBool::new(false)),
            snapshot_bound: Arc::new(AtomicUsize::new(SNAPSHOT_MAX)),
            #[cfg(test)]
            builds: Arc::default(),
            #[cfg(test)]
            eof_gate: Arc::default(),
            #[cfg(test)]
            publish_gate: Arc::default(),
        };
        let ended = Arc::new(AtomicBool::new(false));
        let write_stop = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let fail_write = Arc::new(AtomicBool::new(false));
        let (input, queued) = mpsc::channel();
        Ok(Self {
            master: Some(pair.master),
            reader: Some(thread_read(
                reader,
                shared.clone(),
                input.clone(),
                Arc::clone(&ended),
                #[cfg(unix)]
                wait_fd,
            )),
            writer: Some(thread_write(
                writer,
                queued,
                Arc::clone(&write_stop),
                #[cfg(unix)]
                wait_fd,
                #[cfg(test)]
                Arc::clone(&fail_write),
            )),
            input,
            child: Mutex::new(Some(child)),
            shared,
            ended,
            write_stop,
            process_id,
            epoch: Instant::now(),
            observed: OnceLock::new(),
            ended_with: OnceLock::new(),
            // The pty starts the program, so it runs briefly before it joins the job. Without a
            // job, closing the console still ends the programs attached to it.
            #[cfg(windows)]
            job: kg_process::Job::assign(process_id).ok(),
            #[cfg(test)]
            clock: None,
            #[cfg(test)]
            fail_write,
        })
    }

    #[must_use]
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn set_snapshot_bound(&self, bound: usize) {
        self.shared.snapshot_bound.store(bound, Ordering::Relaxed);
    }

    /// Typing goes to the program and brings the view back to the bottom.
    ///
    /// # Errors
    ///
    /// The program has already been reaped.
    pub fn write(&self, bytes: Vec<u8>) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.edit(Edit::Bottom);
        self.send(bytes)
    }

    /// Bytes a mouse report sends. Unlike typing, a report leaves a scrolled view where it is.
    ///
    /// # Errors
    ///
    /// The program has already been reaped.
    pub fn report(&self, bytes: Vec<u8>) -> io::Result<()> {
        self.send(bytes)
    }

    fn send(&self, bytes: Vec<u8>) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.input
            .send(Outgoing::Bytes(bytes))
            .map_err(|_| io::Error::other("terminal is closed"))
    }

    /// # Errors
    ///
    /// The screen is too small, or the pty rejects the size.
    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let size = pty_size(cols, rows)?;
        let master = self
            .master
            .as_ref()
            .ok_or_else(|| io::Error::other("terminal is closed"))?;
        master.resize(size).map_err(io_error)?;
        self.edit(Edit::Resize(cols, rows));
        Ok(())
    }

    /// Text for the program, bracketed when it asked for that.
    ///
    /// # Errors
    ///
    /// The program has already been reaped.
    pub fn paste(&self, text: &str) -> io::Result<()> {
        let bracketed = lock(&self.shared.screen).bracketed_paste();
        self.write(paste_text(text, bracketed).into_bytes())
    }

    /// Types `text` the way [`paste`](Self::paste) does, then presses Enter, as one write.
    ///
    /// The returned receiver settles when that write finishes. The caller records a message only
    /// after it succeeds.
    ///
    /// # Errors
    ///
    /// The program has ended, the terminal is closed, or nothing non-control remains after the filter.
    pub fn submit(&self, text: &str) -> io::Result<(String, mpsc::Receiver<io::Result<()>>)> {
        if self.exited() {
            return Err(io::Error::other("the program has ended"));
        }
        let typed: String = text.chars().filter(|c| !is_unsafe_control(*c)).collect();
        if !typed.chars().any(|c| !c.is_control()) {
            return Err(io::Error::other("send needs text"));
        }
        let bracketed = lock(&self.shared.screen).bracketed_paste();
        let mut bytes = paste_text(&typed, bracketed).into_bytes();
        bytes.push(b'\r');
        self.edit(Edit::Bottom);
        let (done, written) = mpsc::channel();
        self.input
            .send(Outgoing::Checked { bytes, done })
            .map_err(|_| io::Error::other("terminal is closed"))?;
        Ok((typed, written))
    }

    #[cfg(test)]
    pub(crate) fn fail_next_write(&self) {
        self.fail_write.store(true, Ordering::Release);
    }

    /// Moves the view back into history by `lines`, or forward when negative.
    pub fn scroll(&self, lines: i32) {
        self.edit(Edit::Scroll(lines));
    }

    /// Starts a selection at a view cell, or moves the end of the one there is. `right` is the right
    /// half of the cell.
    pub fn select(&self, mode: SelectMode, column: u16, row: u16, right: bool) {
        self.edit(Edit::Select {
            mode,
            column,
            row,
            right,
        });
    }

    /// The selected text, if any.
    #[must_use]
    pub fn selection(&self) -> Option<String> {
        self.shared.drained(Screen::selection_text)
    }

    /// Every line the program printed that is still kept, as plain text.
    #[must_use]
    pub fn transcript(&self) -> String {
        self.shared.drained(Screen::transcript)
    }

    /// An ANSI redraw of the screen as it is now.
    #[must_use]
    pub fn screen(&self) -> String {
        lock(&self.shared.screen).draw()
    }

    /// The title the program set, if any. A program that never sets one has none.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        lock(&self.shared.title).clone()
    }

    #[must_use]
    pub fn grid(&self) -> crate::Grid {
        self.shared.grid()
    }

    /// Whether the screen changed since the last call. A hold on the screen that ran out ends here.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        self.settle();
        self.shared.changed.swap(false, Ordering::AcqRel)
    }

    /// Queues a change to the screen and applies it now if the reader lets go. It never waits for the
    /// reader: if the reader holds the screen, the change waits for the reader's next chunk or the
    /// next call here.
    fn edit(&self, edit: Edit) {
        lock(&self.shared.edits).push(edit);
        self.settle();
    }

    /// Applies queued changes and ends a finished hold. It never waits for the reader.
    fn settle(&self) {
        let Some(mut screen) = try_lock(&self.shared.screen) else {
            return;
        };
        self.shared.drain(&mut screen);
        let Some(replies) = screen.expire_hold() else {
            return;
        };
        self.shared.publish(&screen);
        if !replies.is_empty() {
            let _ = self.input.send(Outgoing::Bytes(replies));
        }
    }

    /// Whether the program has ended. A background job it left can keep the pty open, so the
    /// output may not be at its end.
    /// How the program ended, once it has. `None` while it runs.
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
            crate::tree::kill_tree(self.process_id);
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
            crate::tree::kill_leader(self.process_id);
            crate::tree::kill_tree(self.process_id);
        }
        #[cfg(windows)]
        {
            if let Some(job) = &self.job {
                job.terminate();
            }
            if let Some(child) = lock(&self.child).as_mut() {
                let _ = child.kill();
            }
        }
    }

    /// Ends the program's IO and keeps its screen. Call once `exit` has returned `Some`.
    #[must_use]
    pub fn finish(mut self) -> Finished {
        let shared = self.shared.clone();
        {
            // A synchronized update the program never closed would hide its last text.
            let mut screen = lock(&shared.screen);
            shared.drain(&mut screen);
            screen.end_hold();
            shared.publish(&screen);
        }
        if let Some(ending) = self.ending() {
            // Teardown joins the IO threads, so it runs off the caller's thread.
            let _ = std::thread::Builder::new().spawn(move || ending.finish());
        }
        Finished { shared }
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

    #[must_use]
    pub fn exited(&self) -> bool {
        // ConPTY keeps the output open after the program exits, so ask the process too.
        self.observe().is_some() || self.ended.load(Ordering::Acquire)
    }
}

impl Terminal {
    fn ending(&mut self) -> Option<Ending> {
        Some(Ending {
            child: self
                .child
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner)
                .take()?,
            master: self.master.take(),
            input: self.input.clone(),
            writer: self.writer.take(),
            reader: self.reader.take(),
            shared: self.shared.clone(),
            write_stop: Arc::clone(&self.write_stop),
        })
    }
}

// The program and what it started die at once. Only the waiting happens on another
// thread: the host's request loop drops terminals, and it must not wait on a program.
impl Drop for Terminal {
    fn drop(&mut self) {
        let Some(ending) = self.ending() else {
            return;
        };
        #[cfg(unix)]
        crate::tree::kill_tree(self.process_id);
        let _ = std::thread::Builder::new().spawn(move || ending.finish());
    }
}

fn thread_read(
    mut reader: Box<dyn Read + Send>,
    shared: Shared,
    replies: Sender<Outgoing>,
    ended: Arc<AtomicBool>,
    #[cfg(unix)] wait_fd: Option<std::os::fd::RawFd>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        // Keep reading until the pty closes: stopping early leaves the host's last frame unread.
        // The master is non-blocking so a full-buffer write can be cancelled; WouldBlock is not EOF.
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // The reply is sent after the lock is gone, so a client asking for a frame never waits on the pty.
                    let reply = shared.feed(&buf[..n]);
                    if !reply.is_empty() {
                        let _ = replies.send(Outgoing::Bytes(reply));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    // O_NONBLOCK is shared across the master's dups; sleep would spin an idle pty.
                    #[cfg(unix)]
                    if let Some(fd) = wait_fd {
                        wait_ready(fd, libc::POLLIN);
                    } else {
                        std::thread::sleep(Duration::from_micros(100));
                    }
                    #[cfg(not(unix))]
                    std::thread::sleep(Duration::from_micros(100));
                }
                Err(_) => break,
            }
        }
        #[cfg(test)]
        if let Some(gate) = lock(&shared.eof_gate).take() {
            let _ = gate.recv();
        }
        ended.store(true, Ordering::Release);
    })
}

#[cfg(unix)]
fn wait_ready(fd: std::os::fd::RawFd, events: libc::c_short) {
    loop {
        let mut pfd = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        // SAFETY: pfd is one valid pollfd; fd is a live pty master.
        let n = unsafe { libc::poll(&raw mut pfd, 1, -1) };
        if n >= 0 {
            return;
        }
        if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return;
        }
    }
}

fn thread_write(
    mut writer: Box<dyn Write + Send>,
    queued: mpsc::Receiver<Outgoing>,
    write_stop: Arc<AtomicBool>,
    #[cfg(unix)] wait_fd: Option<std::os::fd::RawFd>,
    #[cfg(test)] fail_write: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        for outgoing in queued {
            let (bytes, done) = match outgoing {
                Outgoing::Bytes(bytes) => (bytes, None),
                Outgoing::Checked { bytes, done } => (bytes, Some(done)),
            };
            let result = if bytes.is_empty() {
                Err(io::Error::other("terminal is closed"))
            } else {
                #[cfg(test)]
                if fail_write.swap(false, Ordering::AcqRel) {
                    Err(io::Error::other("write failed"))
                } else {
                    write_bytes(
                        &mut writer,
                        &bytes,
                        &write_stop,
                        #[cfg(unix)]
                        wait_fd,
                    )
                }
                #[cfg(not(test))]
                {
                    write_bytes(
                        &mut writer,
                        &bytes,
                        &write_stop,
                        #[cfg(unix)]
                        wait_fd,
                    )
                }
            };
            if let Some(done) = done {
                let _ = done.send(match &result {
                    Ok(()) => Ok(()),
                    Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
                });
            }
            if result.is_err() {
                // Finish may have queued several checked writes; settle each instead of
                // dropping the channel and leaving the caller with Disconnected.
                while let Ok(leftover) = queued.try_recv() {
                    if let Outgoing::Checked { done, .. } = leftover {
                        let _ = done.send(Err(io::Error::other("terminal is closed")));
                    }
                }
                break;
            }
        }
    })
}

fn write_bytes(
    writer: &mut dyn Write,
    bytes: &[u8],
    write_stop: &AtomicBool,
    #[cfg(unix)] wait_fd: Option<std::os::fd::RawFd>,
) -> io::Result<()> {
    #[cfg(not(unix))]
    {
        let _ = write_stop;
        return writer.write_all(bytes).and_then(|()| writer.flush());
    }
    #[cfg(unix)]
    {
        let mut offset = 0;
        while offset < bytes.len() {
            if write_stop.load(Ordering::Acquire) {
                return Err(io::Error::other("terminal is closed"));
            }
            match writer.write(&bytes[offset..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "terminal is closed",
                    ));
                }
                Ok(n) => offset += n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(fd) = wait_fd {
                        wait_ready(fd, libc::POLLOUT);
                    } else {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                Err(error) => return Err(error),
            }
        }
        loop {
            if write_stop.load(Ordering::Acquire) {
                return Err(io::Error::other("terminal is closed"));
            }
            match writer.flush() {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(fd) = wait_fd {
                        wait_ready(fd, libc::POLLOUT);
                    } else {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn pty_size(cols: u16, rows: u16) -> io::Result<PtySize> {
    if cols < MIN_COLS || rows == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "screen is too small",
        ));
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}

struct Frame {
    version: u64,
    grid: Arc<crate::Grid>,
}

/// Text for the program, with its newlines as the program reads them.
/// Text for the program. Escape and the other control characters go, so the text cannot end a
/// bracketed paste early or type a command. Tab, newline and carriage return stay.
fn paste_text(text: &str, bracketed: bool) -> String {
    let text: String = text.chars().filter(|c| !is_unsafe_control(*c)).collect();
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~")
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r")
    }
}

fn is_unsafe_control(c: char) -> bool {
    matches!(c, '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'..='\u{9f}')
}

// What the reader thread and the terminal share. Versions move and grids are built only with the
// screen locked, so a grid's version is the screen's version when it was built.
#[derive(Clone)]
struct Shared {
    screen: Arc<Mutex<Screen>>,
    frame: Arc<Mutex<Frame>>,
    title: Arc<Mutex<Option<String>>>,
    versions: Arc<AtomicU64>,
    // A grid was asked for while the screen was busy: the reader builds one after its chunk.
    wanted: Arc<AtomicBool>,
    edits: Arc<Mutex<Vec<Edit>>>,
    changed: Arc<AtomicBool>,
    settled: Arc<AtomicBool>,
    snapshot_bound: Arc<AtomicUsize>,
    #[cfg(test)]
    builds: Arc<AtomicUsize>,
    /// Test only: the reader waits on this before it reports end of file.
    #[cfg(test)]
    eof_gate: Arc<Mutex<Option<std::sync::mpsc::Receiver<()>>>>,
    /// Test only: the teardown waits on this before it publishes the final screen.
    #[cfg(test)]
    publish_gate: Arc<Mutex<Option<std::sync::mpsc::Receiver<()>>>>,
}

impl Shared {
    pub(super) fn settled(mut screen: Screen) -> Self {
        let _ = screen.end_hold();
        let grid = screen.grid();
        let title = screen.title().map(str::to_owned);
        Self {
            screen: Arc::new(Mutex::new(screen)),
            frame: Arc::new(Mutex::new(Frame {
                version: 1,
                grid: Arc::new(grid),
            })),
            title: Arc::new(Mutex::new(title)),
            versions: Arc::new(AtomicU64::new(1)),
            wanted: Arc::new(AtomicBool::new(false)),
            edits: Arc::new(Mutex::new(Vec::new())),
            changed: Arc::new(AtomicBool::new(false)),
            settled: Arc::new(AtomicBool::new(true)),
            snapshot_bound: Arc::new(AtomicUsize::new(SNAPSHOT_MAX)),
            #[cfg(test)]
            builds: Arc::default(),
            #[cfg(test)]
            eof_gate: Arc::default(),
            #[cfg(test)]
            publish_gate: Arc::default(),
        }
    }

    fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        let mut screen = lock(&self.screen);
        self.drain(&mut screen);
        let reply = screen.feed(bytes);
        self.publish(&screen);
        reply
    }

    // Built only when the screen changed since the last grid. It never waits for the reader: while
    // the reader holds the screen, this is the last grid built.
    fn grid(&self) -> crate::Grid {
        if let Some(screen) = try_lock(&self.screen) {
            self.wanted.store(false, Ordering::Relaxed);
            self.build(&screen);
        } else {
            self.wanted.store(true, Ordering::Relaxed);
            self.changed.store(true, Ordering::Release);
        }
        // The frame lock is held only to clone the Arc. The cells are copied without it.
        let grid = Arc::clone(&lock(&self.frame).grid);
        grid.as_ref().clone()
    }

    fn drained<T>(&self, read: impl FnOnce(&Screen) -> T) -> T {
        let mut screen = lock(&self.screen);
        self.drain(&mut screen);
        read(&screen)
    }

    /// Applies the queued changes in order and publishes the result. Call with the screen locked.
    fn drain(&self, screen: &mut Screen) {
        let edits = std::mem::take(&mut *lock(&self.edits));
        if edits.is_empty() {
            return;
        }
        for edit in edits {
            screen.apply(edit);
        }
        self.publish(screen);
    }

    // Marks the screen changed; the grid waits until it is read. Call with the screen locked.
    fn publish(&self, screen: &Screen) {
        self.versions.fetch_add(1, Ordering::Relaxed);
        {
            let mut title = lock(&self.title);
            if title.as_deref() != screen.title() {
                *title = screen.title().map(str::to_owned);
            }
        }
        if self.wanted.swap(false, Ordering::Relaxed) {
            self.build(screen);
        }
        self.changed.store(true, Ordering::Release);
    }

    // Call with the screen locked.
    fn build(&self, screen: &Screen) {
        let version = self.versions.load(Ordering::Relaxed);
        if lock(&self.frame).version >= version {
            return;
        }
        #[cfg(test)]
        self.builds.fetch_add(1, Ordering::Relaxed);
        store_frame(&self.frame, version, screen.grid());
    }
}

/// Replaces the frame unless it is newer. The old grid is dropped after the frame lock is released.
fn store_frame(frame: &Mutex<Frame>, version: u64, grid: crate::Grid) {
    let previous = {
        let mut current = lock(frame);
        if version < current.version {
            return;
        }
        std::mem::replace(
            &mut *current,
            Frame {
                version,
                grid: Arc::new(grid),
            },
        )
    };
    drop(previous);
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Takes the lock only if nobody holds it now. A poisoned lock is still taken.
fn try_lock<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn io_error(err: impl std::fmt::Display) -> io::Error {
    io::Error::other(err.to_string())
}

#[cfg(test)]
mod tests;
