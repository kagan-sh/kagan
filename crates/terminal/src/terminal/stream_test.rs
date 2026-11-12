use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use proptest::prelude::*;

use super::{INPUT_MAX, reserve, write_bytes};

#[test]
fn a_dropped_charge_gives_its_bytes_back() {
    let queued = Arc::new(AtomicUsize::new(0));
    let first = reserve(&queued, INPUT_MAX).expect("the whole budget fits");
    let Err(error) = reserve(&queued, 1) else {
        panic!("a full budget refuses more")
    };
    assert_eq!(error.kind(), io::ErrorKind::QuotaExceeded);
    drop(first);
    let _second = reserve(&queued, 1).expect("the freed bytes fit");
    assert_eq!(queued.load(Ordering::Acquire), 1);
}

struct Chunks {
    data: Vec<u8>,
    chunk: usize,
    first: Option<io::ErrorKind>,
    flush_first: Option<io::ErrorKind>,
    hard_write: Option<io::ErrorKind>,
    hard_flush: Option<io::ErrorKind>,
    flushed: bool,
}

impl Chunks {
    fn new(chunk: usize) -> Self {
        Self {
            data: Vec::new(),
            chunk,
            first: None,
            flush_first: None,
            hard_write: None,
            hard_flush: None,
            flushed: false,
        }
    }
}

impl Write for Chunks {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(kind) = self.hard_write.take() {
            return Err(io::Error::from(kind));
        }
        if let Some(kind) = self.first.take() {
            return Err(io::Error::from(kind));
        }
        let n = buf.len().min(self.chunk);
        self.data.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(kind) = self.hard_flush.take() {
            return Err(io::Error::from(kind));
        }
        if let Some(kind) = self.flush_first.take() {
            return Err(io::Error::from(kind));
        }
        self.flushed = true;
        Ok(())
    }
}

fn write(writer: &mut Chunks, bytes: &[u8], stop: &AtomicBool) -> io::Result<()> {
    #[cfg(unix)]
    return write_bytes(writer, bytes, stop, None);
    #[cfg(not(unix))]
    write_bytes(writer, bytes, stop)
}

proptest! {
    #[test]
    fn every_byte_reaches_the_writer_in_any_chunk(
        bytes in prop::collection::vec(any::<u8>(), 0..256),
        chunk in 1usize..32,
    ) {
        let mut writer = Chunks {
            first: Some(io::ErrorKind::Interrupted),
            ..Chunks::new(chunk)
        };
        let stop = AtomicBool::new(false);
        write(&mut writer, &bytes, &stop).expect("write");
        prop_assert_eq!(writer.data, bytes);
        prop_assert!(writer.flushed);
    }
}

#[test]
fn a_writer_that_accepts_nothing_is_a_write_zero() {
    let mut writer = Chunks::new(0);
    let error = write(&mut writer, b"x", &AtomicBool::new(false)).expect_err("zero write");
    assert_eq!(error.kind(), io::ErrorKind::WriteZero);
}

#[cfg(unix)]
#[test]
fn a_closed_terminal_refuses_the_write() {
    let mut writer = Chunks::new(4);
    let error = write(&mut writer, b"x", &AtomicBool::new(true)).expect_err("closed");
    assert_eq!(error.kind(), io::ErrorKind::Other);
}

#[cfg(unix)]
#[test]
fn a_blocked_write_is_retried() {
    let mut writer = Chunks {
        first: Some(io::ErrorKind::WouldBlock),
        ..Chunks::new(4)
    };
    write(&mut writer, b"hello", &AtomicBool::new(false)).expect("retry");
    assert_eq!(writer.data, b"hello");
}

#[cfg(unix)]
#[test]
fn an_interrupted_flush_is_retried() {
    let mut writer = Chunks {
        flush_first: Some(io::ErrorKind::Interrupted),
        ..Chunks::new(4)
    };
    write(&mut writer, b"hello", &AtomicBool::new(false)).expect("retry");
    assert!(writer.flushed);
}

#[cfg(unix)]
#[test]
fn a_blocked_flush_is_retried() {
    let mut writer = Chunks {
        flush_first: Some(io::ErrorKind::WouldBlock),
        ..Chunks::new(4)
    };
    write(&mut writer, b"hello", &AtomicBool::new(false)).expect("retry");
    assert!(writer.flushed);
}

#[cfg(unix)]
#[test]
fn a_hard_write_error_is_returned() {
    let mut writer = Chunks {
        hard_write: Some(io::ErrorKind::PermissionDenied),
        ..Chunks::new(4)
    };
    let error = write(&mut writer, b"hello", &AtomicBool::new(false)).expect_err("hard error");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[cfg(unix)]
#[test]
fn a_hard_flush_error_is_returned() {
    let mut writer = Chunks {
        hard_flush: Some(io::ErrorKind::PermissionDenied),
        ..Chunks::new(4)
    };
    let error = write(&mut writer, b"hello", &AtomicBool::new(false)).expect_err("hard error");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}
