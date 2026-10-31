use std::fmt;

use super::{Finished, Screen, Shared, Terminal};

/// The kept text as lines, oldest first, with the grid's row count and how many of the last rows
/// are on the screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kept {
    pub lines: Vec<KeptLine>,
    pub rows: usize,
    pub on_screen: usize,
}

/// One line as the program printed it: rows the terminal wrapped are joined, and `row` is the
/// first of them, counted from the oldest kept row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeptLine {
    pub text: String,
    pub row: usize,
}

/// A terminal's kept text, read from any thread.
#[derive(Clone)]
pub struct Lines(Shared);

impl fmt::Debug for Lines {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Lines")
    }
}

impl Lines {
    #[must_use]
    pub fn kept(&self) -> Kept {
        self.0.drained(Screen::kept)
    }
}

impl Terminal {
    #[must_use]
    pub fn lines(&self) -> Lines {
        Lines(self.shared.clone())
    }
}

impl Finished {
    #[must_use]
    pub fn lines(&self) -> Lines {
        Lines(self.shared.clone())
    }
}
