use std::io::{self, ErrorKind, Write};

use crate::out::write_quietly;

struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(ErrorKind::BrokenPipe, "closed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Full;

impl Write for Full {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(ErrorKind::StorageFull, "full"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_closed_reader_is_not_an_error() {
    assert!(write_quietly(&mut Closed, b"text").is_ok());
}

#[test]
fn other_write_errors_still_fail() {
    let error = write_quietly(&mut Full, b"text").expect_err("a full disk is a failure");
    assert_eq!(error.kind(), ErrorKind::StorageFull);
}
