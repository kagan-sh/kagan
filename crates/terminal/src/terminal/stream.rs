use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use portable_pty::PtySize;
#[cfg(unix)]
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};

use super::shared::Shared;
#[cfg(test)]
use super::shared::lock;
use crate::screen::MIN_COLS;

/// Most input the program may still have to read. A write past it is refused until the program reads.
pub(super) const INPUT_MAX: usize = 4 << 20;

pub(super) enum Outgoing {
    /// The program's own query answers and the writer's wake-up. Never refused.
    Reply(Vec<u8>),
    /// Input from a person or an agent, counted against `INPUT_MAX` until it is written.
    Input {
        bytes: Vec<u8>,
        done: Option<Sender<io::Result<()>>>,
        charge: Charge,
    },
}

/// Bytes queued for the program. Dropping it gives them back, so every path that ends a message frees them.
pub(super) struct Charge {
    queued: Arc<AtomicUsize>,
    bytes: usize,
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.queued.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub(super) fn reserve(queued: &Arc<AtomicUsize>, bytes: usize) -> io::Result<Charge> {
    queued
        .try_update(Ordering::AcqRel, Ordering::Acquire, |total| {
            total.checked_add(bytes).filter(|total| *total <= INPUT_MAX)
        })
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::QuotaExceeded,
                "the program is not reading its input",
            )
        })?;
    Ok(Charge {
        queued: Arc::clone(queued),
        bytes,
    })
}

pub(super) fn thread_read(
    mut reader: Box<dyn Read + Send>,
    shared: Shared,
    replies: Sender<Outgoing>,
    ended: Arc<AtomicBool>,
    #[cfg(unix)] wait_fd: Option<OwnedFd>,
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
                        // The writer is gone, so the program no longer hears replies.
                        let _ = replies.send(Outgoing::Reply(reply));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    #[cfg(test)]
                    if let Some((reached, resume)) = lock(&shared.poll_gate).take() {
                        let _ = reached.send(());
                        let _ = resume.recv();
                    }
                    // O_NONBLOCK is shared across the master's dups; sleep would spin an idle pty.
                    #[cfg(unix)]
                    if let Some(fd) = &wait_fd {
                        wait_ready(fd.as_raw_fd(), libc::POLLIN, -1);
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
pub(super) fn dup_fd(fd: std::os::fd::RawFd) -> io::Result<OwnedFd> {
    // SAFETY: the master is open while spawn runs, so fd is valid for this borrow.
    unsafe { BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()
}

#[cfg(unix)]
pub(super) fn set_nonblocking(fd: std::os::fd::RawFd) -> io::Result<()> {
    // SAFETY: fcntl only reads and sets flags on fd; a bad fd fails with EBADF.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above. O_NONBLOCK is per open file description, so every dup of the master sees it.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// A full pty never wakes a writer on its own, so writers check write_stop this often.
#[cfg(unix)]
const WRITE_WAIT_MS: libc::c_int = 10;

#[cfg(unix)]
fn wait_ready(fd: std::os::fd::RawFd, events: libc::c_short, timeout_ms: libc::c_int) {
    loop {
        let mut pfd = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        // SAFETY: pfd is one valid pollfd; fd is a live pty master.
        let n = unsafe { libc::poll(&raw mut pfd, 1, timeout_ms) };
        if n >= 0 {
            return;
        }
        if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return;
        }
    }
}

pub(super) fn thread_write(
    mut writer: Box<dyn Write + Send>,
    queued: mpsc::Receiver<Outgoing>,
    write_stop: Arc<AtomicBool>,
    #[cfg(unix)] wait_fd: Option<OwnedFd>,
    #[cfg(test)] fail_write: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(outgoing) = queued.recv() {
            let (bytes, done, charge) = match outgoing {
                Outgoing::Reply(bytes) => (bytes, None, None),
                Outgoing::Input {
                    bytes,
                    done,
                    charge,
                } => (bytes, done, Some(charge)),
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
                        wait_fd.as_ref().map(AsRawFd::as_raw_fd),
                    )
                }
                #[cfg(not(test))]
                {
                    write_bytes(
                        &mut writer,
                        &bytes,
                        &write_stop,
                        #[cfg(unix)]
                        wait_fd.as_ref().map(AsRawFd::as_raw_fd),
                    )
                }
            };
            // The budget is back before a caller hears the write finished, so its next write can fit.
            drop(charge);
            if let Some(done) = done {
                // The caller stopped waiting for the write.
                let _ = done.send(match &result {
                    Ok(()) => Ok(()),
                    Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
                });
            }
            if result.is_err() {
                // Finish may have queued several checked writes; settle each instead of
                // dropping the channel and leaving the caller with Disconnected.
                while let Ok(leftover) = queued.try_recv() {
                    if let Outgoing::Input {
                        done: Some(done), ..
                    } = leftover
                    {
                        // The caller stopped waiting for the write.
                        let _ = done.send(Err(io::Error::other("terminal is closed")));
                    }
                }
                break;
            }
        }
    })
}

pub(super) fn write_bytes(
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
                    wait_until_writable(wait_fd);
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
                    wait_until_writable(wait_fd);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

#[cfg(unix)]
fn wait_until_writable(fd: Option<std::os::fd::RawFd>) {
    if let Some(fd) = fd {
        wait_ready(fd, libc::POLLOUT, WRITE_WAIT_MS);
    } else {
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn pty_size(cols: u16, rows: u16) -> io::Result<PtySize> {
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

/// Text for the program. Escape and the other control characters go, so the text cannot end a
/// bracketed paste early or type a command. Tab, newline and carriage return stay.
pub(super) fn paste_text(text: &str, bracketed: bool) -> String {
    let text: String = text.chars().filter(|c| !is_unsafe_control(*c)).collect();
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~")
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r")
    }
}

pub(super) fn is_unsafe_control(c: char) -> bool {
    matches!(c, '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'..='\u{9f}')
}

pub(super) fn io_error(err: impl std::fmt::Display) -> io::Error {
    io::Error::other(err.to_string())
}

#[cfg(test)]
#[path = "stream_test.rs"]
mod tests;
