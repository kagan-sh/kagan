use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};

use crate::screen::{Edit, Screen, SelectMode};

mod ending;
mod exit;
mod finished;
mod lines;
mod shared;
mod stream;
#[cfg(test)]
mod tests;

use ending::Ending;
pub use exit::Exit;
pub use finished::Finished;
pub use lines::{Kept, KeptLine, Lines};
use shared::{Shared, lock, try_lock};
use stream::{Outgoing, io_error, is_unsafe_control, paste_text, pty_size};
#[cfg(unix)]
use stream::{dup_fd, set_nonblocking};
use stream::{thread_read, thread_write};

pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
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
    observed: OnceLock<(Exit, std::time::Duration)>,
    ended_with: OnceLock<Exit>,
    // Holds the program and what it starts; closing the handle kills them all.
    #[cfg(windows)]
    job: Option<kg_process::Job>,
    #[cfg(test)]
    clock: Option<Arc<std::sync::atomic::AtomicU64>>,
    #[cfg(test)]
    fail_write: Arc<AtomicBool>,
}

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
        // Each polling thread owns its own dup, because teardown closes the master before it joins the reader.
        #[cfg(unix)]
        let (reader_wait, writer_wait) = match pair.master.as_raw_fd() {
            Some(fd) => {
                set_nonblocking(fd)?;
                (Some(dup_fd(fd)?), Some(dup_fd(fd)?))
            }
            None => (None, None),
        };
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
        let shared = Shared::running(Screen::new(cols, rows));
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
                reader_wait,
            )),
            writer: Some(thread_write(
                writer,
                queued,
                Arc::clone(&write_stop),
                #[cfg(unix)]
                writer_wait,
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
