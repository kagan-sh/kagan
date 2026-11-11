use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};

use crate::screen::{Edit, Screen};
use shared::Shared;

mod ending;
mod exit;
mod finished;
mod shared;
mod stream;
#[cfg(test)]
mod tests;
mod view;

use ending::Ending;
pub use exit::Exit;
pub use finished::Finished;
use shared::lock;
use stream::{Outgoing, io_error, is_unsafe_control, paste_text, pty_size};
#[cfg(unix)]
use stream::{dup_fd, set_nonblocking};
use stream::{thread_read, thread_write};
pub use view::{Kept, KeptLine, View};

pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
}

/// What ends the program's processes when it is stopped or its terminal closes.
#[derive(Debug)]
pub enum Containment {
    /// The program's process tree, found by its pid (macOS and Linux).
    Tree,
    /// A Job Object: closing or terminating it ends the program and what it started.
    #[cfg(windows)]
    Job(kg_process::Job),
    /// The job could not be made or assigned, so only the program is ended. The text is the OS error.
    #[cfg(windows)]
    Unavailable(io::Error),
}

impl std::fmt::Display for Containment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tree => write!(f, "tree"),
            #[cfg(windows)]
            Self::Job(_) => write!(f, "job"),
            #[cfg(windows)]
            Self::Unavailable(error) => write!(f, "unavailable: {error}"),
        }
    }
}

pub struct Terminal {
    master: Option<Box<dyn MasterPty + Send>>,
    input: Sender<Outgoing>,
    child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    view: View,
    ended: Arc<AtomicBool>,
    /// Finish sets this so a non-blocking write waiting on a full pty can exit.
    write_stop: Arc<AtomicBool>,
    process_id: u32,
    epoch: Instant,
    observed: OnceLock<(Exit, std::time::Duration)>,
    ended_with: OnceLock<Exit>,
    containment: Containment,
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
        Self::start(launch, cols, rows, contain_program)
    }

    fn start(
        launch: Launch,
        cols: u16,
        rows: u16,
        contain: impl FnOnce(u32) -> Containment,
    ) -> io::Result<Self> {
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
        let containment = contain(process_id);
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
            view: View {
                shared,
                replies: Some(input.clone()),
            },
            input,
            child: Mutex::new(Some(child)),
            ended,
            write_stop,
            process_id,
            epoch: Instant::now(),
            observed: OnceLock::new(),
            ended_with: OnceLock::new(),
            containment,
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

    /// What ends the program's processes. Windows keeps the job, or why there is none.
    #[must_use]
    pub fn containment(&self) -> &Containment {
        &self.containment
    }

    pub fn set_snapshot_bound(&self, bound: usize) {
        self.view
            .shared
            .snapshot_bound
            .store(bound, Ordering::Relaxed);
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
        self.view.edit(Edit::Bottom);
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
        self.view.edit(Edit::Resize(cols, rows));
        Ok(())
    }

    /// Text for the program, bracketed when it asked for that.
    ///
    /// # Errors
    ///
    /// The program has already been reaped.
    pub fn paste(&self, text: &str) -> io::Result<()> {
        let bracketed = lock(&self.view.shared.screen).bracketed_paste();
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
        let bracketed = lock(&self.view.shared.screen).bracketed_paste();
        let mut bytes = paste_text(&typed, bracketed).into_bytes();
        bytes.push(b'\r');
        self.view.edit(Edit::Bottom);
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

    /// The title the program set, if any. A program that never sets one has none.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        lock(&self.view.shared.title).clone()
    }

    /// The screen, its text and its view, readable and movable from any thread.
    #[must_use]
    pub fn view(&self) -> &View {
        &self.view
    }

    /// Ends the program's IO and keeps its screen. Call once `exit` has returned `Some`.
    #[must_use]
    pub fn finish(mut self) -> Finished {
        let view = View {
            replies: None,
            ..self.view.clone()
        };
        let shared = &view.shared;
        {
            // A synchronized update the program never closed would hide its last text.
            let mut screen = lock(&shared.screen);
            shared.drain(&mut screen);
            screen.end_hold();
            shared.publish(&screen);
        }
        if let Some(ending) = self.ending() {
            // Teardown joins the IO threads, so it runs off the caller's thread.
            if let Err(error) = std::thread::Builder::new().spawn(move || ending.finish()) {
                eprintln!("kg: could not end the program's pty: {error}");
            }
        }
        Finished { view }
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
            shared: self.view.shared.clone(),
            write_stop: Arc::clone(&self.write_stop),
        })
    }
}

#[cfg(unix)]
fn contain_program(_process_id: u32) -> Containment {
    Containment::Tree
}

// The pty starts the program, so it runs briefly before it joins the job. Without a job,
// closing the console still ends the programs attached to it.
#[cfg(windows)]
fn contain_program(process_id: u32) -> Containment {
    contain(process_id, kg_process::Job::assign)
}

#[cfg(windows)]
fn contain(
    process_id: u32,
    assign: impl FnOnce(u32) -> io::Result<kg_process::Job>,
) -> Containment {
    match assign(process_id) {
        Ok(job) => Containment::Job(job),
        Err(error) => {
            eprintln!(
                "kg: program {process_id} is not in a job object, so stopping it ends only the program: {error}"
            );
            Containment::Unavailable(error)
        }
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
        kg_process::kill_tree(self.process_id);
        if let Err(error) = std::thread::Builder::new().spawn(move || ending.finish()) {
            eprintln!("kg: could not end the program's pty: {error}");
        }
    }
}
