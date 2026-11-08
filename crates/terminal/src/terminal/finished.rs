use crate::screen::{MAX_COLS, MAX_ROWS, MIN_COLS, SNAPSHOT_MAX, Snapshot};

use std::sync::atomic::Ordering;

use super::shared::{Shared, lock};
use super::view::View;

/// The screen of a program that has ended. It keeps no pty, so input is not possible.
#[derive(Clone)]
pub struct Finished {
    pub(super) view: View,
}

impl Finished {
    /// The screen and its text. The program has ended, so it keeps no pty and nothing it asks for is answered.
    #[must_use]
    pub fn view(&self) -> &View {
        &self.view
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
        (self.view.take_changed(), following)
    }

    /// Whether the program's output has ended for good. Until then, late output can still arrive.
    #[must_use]
    pub fn settled(&self) -> bool {
        self.view.shared.settled.load(Ordering::Acquire)
    }

    /// History and the visible screen. The bound starts at [`SNAPSHOT_MAX`].
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let bound = self.view.shared.snapshot_bound.load(Ordering::Relaxed);
        lock(&self.view.shared.screen).snapshot_within(bound)
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
            view: View {
                shared: Shared::settled(screen),
                replies: None,
            },
        })
    }
}
