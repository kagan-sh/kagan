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

    #[must_use]
    pub fn tail(&self, cap: usize) -> String {
        newest(self.0.drained(Screen::transcript), cap)
    }
}

/// May return fewer than `cap` bytes so the slice starts on a character boundary.
#[must_use]
pub(crate) fn newest(text: String, cap: usize) -> String {
    if text.len() <= cap {
        return text;
    }
    let mut start = text.len() - cap;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
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

#[cfg(test)]
mod tests {
    use super::newest;

    #[test]
    fn newest_keeps_the_end_on_a_char_boundary() {
        let capped = newest(format!("{}y", "x".repeat(10)), 10);
        assert_eq!(capped, format!("{}y", "x".repeat(9)));

        let mut text = "a".repeat(10);
        text.push('€');
        text.push_str(&"b".repeat(8));
        let capped = newest(text, 10);
        assert!(capped.len() <= 10);
        assert!(capped.is_char_boundary(0));
        assert!(capped.starts_with('b') && capped.ends_with('b'));
        assert!(!capped.contains('a') && !capped.contains('€'));
    }
}
