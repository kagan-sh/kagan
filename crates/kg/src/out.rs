//! Standard output for `kg`. A reader that closes early (`kg ... | head`) is not a failure: the
//! operation already ran, so the rest of the text is dropped and the exit code stays as it was.

use std::fmt::Display;
use std::io::{self, ErrorKind, Write};

pub(crate) fn line(text: impl Display) -> io::Result<()> {
    bytes(format!("{text}\n").as_bytes())
}

pub(crate) fn bytes(data: &[u8]) -> io::Result<()> {
    write_quietly(&mut io::stdout().lock(), data)
}

fn write_quietly(writer: &mut impl Write, data: &[u8]) -> io::Result<()> {
    match writer.write_all(data) {
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, ErrorKind, Write};

    use super::write_quietly;

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
}
