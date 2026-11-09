#![cfg(unix)]

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use portable_pty::MasterPty;

use super::super::shared::lock;
use super::super::stream::{set_nonblocking, write_bytes};
use super::{Reap, sh, wait_for};
use crate::Terminal;

#[cfg(unix)]
fn pty_pair() -> (std::fs::File, std::fs::File) {
    use std::os::fd::FromRawFd;
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: openpty writes one new fd through each out pointer; null name, termios and size mean defaults.
    let opened = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "openpty failed");
    // SAFETY: an all-zero termios is valid storage that tcgetattr overwrites.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: tcgetattr and tcsetattr take the open slave fd and a valid termios.
    unsafe {
        assert_eq!(libc::tcgetattr(slave, &raw mut term), 0);
        libc::cfmakeraw(&raw mut term);
        assert_eq!(libc::tcsetattr(slave, libc::TCSANOW, &raw const term), 0);
    }
    // SAFETY: the two fds came from openpty and nothing else owns them.
    unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    }
}

// Sends on each WouldBlock, so a test can wait until the write really blocks.
#[cfg(unix)]
struct Signalled<T> {
    inner: T,
    would_block: mpsc::Sender<()>,
}

#[cfg(unix)]
impl<T> Signalled<T> {
    fn note<R>(&self, result: io::Result<R>) -> io::Result<R> {
        if matches!(&result, Err(error) if error.kind() == io::ErrorKind::WouldBlock) {
            let _ = self.would_block.send(());
        }
        result
    }
}

#[cfg(unix)]
impl Write for Signalled<std::fs::File> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let result = self.inner.write(buf);
        self.note(result)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(unix)]
#[test]
fn a_full_pty_write_is_cancelled_by_stop() {
    use std::os::fd::AsRawFd;
    let (master, mut slave) = pty_pair();
    set_nonblocking(master.as_raw_fd()).unwrap();
    set_nonblocking(slave.as_raw_fd()).unwrap();
    let (would_block, is_blocked) = mpsc::channel();
    let mut writer = Signalled {
        inner: master.try_clone().unwrap(),
        would_block,
    };
    let fd = writer.inner.as_raw_fd();
    // Nobody reads the slave, so 1 MiB can never fit: this write ends only by stop or by error.
    let payload = vec![0_u8; 1 << 20];
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let (done, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = done.send(write_bytes(&mut writer, &payload, &flag, Some(fd)));
    });
    let blocked = is_blocked.recv_timeout(Duration::from_secs(5)).is_ok();
    stop.store(true, Ordering::Release);
    let observed = result.recv_timeout(Duration::from_secs(3));
    // Drain only after the verdict, so a failing run still lets the worker finish.
    let mut sink = [0_u8; 8192];
    while matches!(slave.read(&mut sink), Ok(n) if n > 0) {}
    worker.join().unwrap();
    assert!(blocked, "the write never reached a full pty");
    assert!(
        matches!(observed, Ok(Err(_))),
        "stop did not cancel the full write: {observed:?}"
    );
}

#[cfg(unix)]
#[test]
fn the_reader_keeps_polling_its_own_fd_after_the_master_closes() {
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    let mut terminal = Terminal::spawn(
        sh("stty -echo; read first; printf 'ready\\n'; read line; printf 'got:%s\\n' \"$line\"; sleep 30"),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    // The child prints nothing until it gets input, so the reader's next WouldBlock comes after this arms the gate.
    let (reached, at_poll) = mpsc::channel();
    let (resume, held) = mpsc::channel();
    *lock(&terminal.view.shared.poll_gate) = Some((reached, held));
    terminal.write(b"go\n".to_vec()).unwrap();
    at_poll
        .recv_timeout(Duration::from_secs(5))
        .expect("the reader never reached its poll");
    // Made before the master closes, so the socket cannot take the master's number.
    let (unrelated, _peer) = UnixStream::pair().unwrap();
    let master = terminal.master.take().unwrap();
    let master_fd = MasterPty::as_raw_fd(master.as_ref()).unwrap();
    drop(master);
    assert_eq!(
        // SAFETY: drop(master) freed master_fd, so dup2 gives it a copy of the socket. The test closes it below.
        unsafe { libc::dup2(unrelated.as_raw_fd(), master_fd) },
        master_fd
    );
    terminal.write(b"hello\n".to_vec()).unwrap();
    resume.send(()).unwrap();
    let frame = wait_for(&terminal, "got:hello");
    // SAFETY: master_fd is the copy dup2 made, and only this test owns it.
    unsafe { libc::close(master_fd) };
    drop(reap);
    assert!(
        frame.contains("got:hello"),
        "reader stopped seeing output after its fd number was reused: {frame}"
    );
}

#[cfg(unix)]
#[test]
fn a_bad_fd_is_refused_when_made_nonblocking() {
    let error = set_nonblocking(-1).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EBADF));
}

#[cfg(unix)]
#[test]
fn write_bytes_delivers_after_a_short_write() {
    struct ShortWrite {
        calls: usize,
        got: Vec<u8>,
    }

    impl Write for ShortWrite {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            match self.calls {
                1 => {
                    self.got.push(buf[0]);
                    Ok(1)
                }
                2 => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                _ => {
                    self.got.extend_from_slice(buf);
                    Ok(buf.len())
                }
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut writer = ShortWrite {
        calls: 0,
        got: Vec::new(),
    };
    let stop = AtomicBool::new(false);
    write_bytes(&mut writer, b"abc", &stop, None).unwrap();
    assert_eq!(writer.got, b"abc");
}

#[cfg(unix)]
#[test]
fn finish_settles_a_submit_blocked_on_a_full_pty() {
    let terminal =
        Terminal::spawn(sh("stty raw -echo; printf 'ready\\n'; sleep 30"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "ready");
    let chunk = "x".repeat(64 * 1024);
    let pending: Vec<_> = (0..4)
        .map(|n| {
            let text = format!("{n}-{chunk}");
            terminal.submit(&text).unwrap().1
        })
        .collect();
    thread::sleep(Duration::from_millis(100));
    let finished = terminal.finish();
    for written in pending {
        let result = written.recv_timeout(Duration::from_secs(3));
        assert!(
            matches!(result, Ok(Err(_))),
            "blocked submit must settle when finish runs: {result:?}"
        );
    }
    while !finished.settled() {
        thread::sleep(Duration::from_millis(10));
    }
    drop(reap);
}
