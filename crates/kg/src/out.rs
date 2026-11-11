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

pub(crate) fn write_quietly(writer: &mut impl Write, data: &[u8]) -> io::Result<()> {
    match writer.write_all(data) {
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}
