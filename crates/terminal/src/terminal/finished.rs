use crate::screen::{Edit, MAX_COLS, MAX_ROWS, MIN_COLS, SNAPSHOT_MAX, SelectMode, Snapshot};

use std::sync::atomic::Ordering;

use super::shared::{Shared, lock};

/// The screen of a program that has ended. It keeps no pty, so input is not possible.
#[derive(Clone)]
pub struct Finished {
    pub(super) shared: Shared,
}

impl Finished {
    /// Every line the program printed that is still kept, as plain text.
    #[must_use]
    pub fn transcript(&self) -> String {
        self.shared.drained(super::Screen::transcript)
    }

    /// An ANSI redraw of the screen as it was when the program ended.
    #[must_use]
    pub fn screen(&self) -> String {
        lock(&self.shared.screen).draw()
    }

    /// The screen as the window draws it. The program has ended, so it no longer asks for the mouse:
    /// wheel and selection stay with the window.
    #[must_use]
    pub fn grid(&self) -> crate::Grid {
        let mut grid = self.shared.grid();
        grid.mouse = None;
        grid
    }

    /// Whether the screen changed since the last call. Output can still arrive until `settled`.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        self.shared.changed.swap(false, Ordering::AcqRel)
    }

    /// Whether the screen changed since the last poll, and whether output can still arrive. The
    /// settlement is read first: a poll that sees it settled also sees every change before it.
    #[must_use]
    pub fn poll(&self) -> (bool, bool) {
        self.poll_between(|| {})
    }

    // `between` runs after the settlement is read and before the change is taken, so a test can
    // let the teardown finish in that gap.
    pub(crate) fn poll_between(&self, between: impl FnOnce()) -> (bool, bool) {
        let following = !self.settled();
        between();
        (self.take_changed(), following)
    }

    /// Whether the program's output has ended for good. Until then, late output can still arrive.
    #[must_use]
    pub fn settled(&self) -> bool {
        self.shared.settled.load(Ordering::Acquire)
    }

    /// History and the visible screen. The bound starts at [`SNAPSHOT_MAX`].
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let bound = self.shared.snapshot_bound.load(Ordering::Relaxed);
        lock(&self.shared.screen).snapshot_within(bound)
    }

    /// Returns nothing when the geometry or the payload is outside the bound, and does not
    /// allocate a terminal grid in that case.
    #[must_use]
    pub fn restore(cols: u16, rows: u16, replay: &[u8]) -> Option<Self> {
        if !(MIN_COLS..=MAX_COLS).contains(&cols)
            || !(1..=MAX_ROWS).contains(&rows)
            || replay.len() > SNAPSHOT_MAX
        {
            return None;
        }
        let mut screen = crate::screen::Screen::new(cols, rows);
        let _ = screen.feed(replay);
        Some(Self {
            shared: Shared::settled(screen),
        })
    }

    /// Moves the view back into history by `lines`, or forward when negative.
    pub fn scroll(&self, lines: i32) {
        self.edit(Edit::Scroll(lines));
    }

    /// Starts a selection at a view cell, or moves the end of the one there is.
    pub fn select(&self, mode: SelectMode, column: u16, row: u16, right: bool) {
        self.edit(Edit::Select {
            mode,
            column,
            row,
            right,
        });
    }

    /// The selected text, if any.
    #[must_use]
    pub fn selection(&self) -> Option<String> {
        self.shared.drained(super::Screen::selection_text)
    }

    // No reader is left to apply queued edits, so this applies them itself.
    fn edit(&self, edit: Edit) {
        lock(&self.shared.edits).push(edit);
        let mut screen = lock(&self.shared.screen);
        self.shared.drain(&mut screen);
    }
}
