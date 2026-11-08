use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use super::Screen;
use super::shared::{Shared, lock, try_lock};
use super::stream::Outgoing;
use crate::Grid;
use crate::screen::{Edit, SelectMode};

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

/// A terminal's screen, read and moved from any thread. A running program and an ended one give the same type.
#[derive(Clone)]
pub struct View {
    pub(super) shared: Shared,
    // Where a held screen's replies go. An ended program has no pty, so it has none.
    pub(super) replies: Option<Sender<Outgoing>>,
}

impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("View")
    }
}

impl View {
    #[must_use]
    pub fn kept(&self) -> Kept {
        self.shared.drained(Screen::kept)
    }

    #[must_use]
    pub fn tail(&self, cap: usize) -> String {
        newest(self.shared.drained(Screen::transcript), cap)
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

    /// The screen as the window draws it. An ended program no longer asks for the mouse, so wheel
    /// and selection stay with the window.
    #[must_use]
    pub fn grid(&self) -> Grid {
        let mut grid = self.shared.grid();
        if self.replies.is_none() {
            grid.mouse = None;
        }
        grid
    }

    /// The selected text, if any.
    #[must_use]
    pub fn selection(&self) -> Option<String> {
        self.shared.drained(Screen::selection_text)
    }

    /// Whether the screen changed since the last call. For a running program, a hold on the screen
    /// that ran out ends here.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        if let Some(replies) = &self.replies {
            self.settle(replies);
        }
        self.shared.changed.swap(false, Ordering::AcqRel)
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

    /// Queues a change to the screen. A running program applies it now if the reader lets go and
    /// never waits for the reader: otherwise the change waits for the reader's next chunk or the
    /// next call here. An ended program has no reader, so it applies the change itself.
    pub(super) fn edit(&self, edit: Edit) {
        lock(&self.shared.edits).push(edit);
        if let Some(replies) = &self.replies {
            self.settle(replies);
        } else {
            let mut screen = lock(&self.shared.screen);
            self.shared.drain(&mut screen);
        }
    }

    fn settle(&self, replies: &Sender<Outgoing>) {
        let Some(mut screen) = try_lock(&self.shared.screen) else {
            return;
        };
        self.shared.drain(&mut screen);
        let Some(held) = screen.expire_hold() else {
            return;
        };
        self.shared.publish(&screen);
        if !held.is_empty() {
            // The writer is gone, so the program no longer hears replies.
            let _ = replies.send(Outgoing::Bytes(held));
        }
    }
}

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
