use std::time::{Duration, Instant};

/// A window title can drop its spinner for a frame in the middle of a reply. It counts as plain only after this.
pub(crate) const TITLE_GRACE: Duration = Duration::from_secs(1);
/// How long a hook-less agent's screen may stay the same before its turn ends.
pub(crate) const QUIET_OUTPUT: Duration = Duration::from_secs(15);
/// How long a hook-run turn may stay silent before it ends, for a `Stop` that never came.
pub(crate) const HOOK_SILENCE: Duration = Duration::from_secs(120);
/// How long a submitted line waits for output to answer it.
pub(crate) const SUBMIT_WINDOW: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Undecided,
    Hook,
    Title,
    Quiet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    HookStart,
    HookEnd,
    /// The window title shows a spinner (`busy`) or it does not.
    Title {
        busy: bool,
    },
    /// The person sent a line to a known agent. `screen` is the visible text just before it, so
    /// the program's first redraw is compared with what was on screen when the line went in.
    Submit {
        screen: u64,
    },
    /// The program's output changed.
    Output,
    /// The visible text hashes to this value. Only a change in it counts as output.
    Screen(u64),
    /// Time passed with no other event.
    Tick,
    /// The program exited, or was stopped.
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    Opened,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TurnState {
    open: bool,
    source: Source,
    /// The window title showed a spinner during this turn.
    spinner_seen: bool,
    /// When the window title went plain during this turn.
    plain_since: Option<Instant>,
    /// The last output in this turn.
    output_at: Option<Instant>,
    /// A submitted line that no output has answered yet.
    submitted_at: Option<Instant>,
    /// The visible text the last time it was read.
    screen: Option<u64>,
}

impl TurnState {
    pub(crate) fn new(source: Source) -> Self {
        Self {
            open: false,
            source,
            spinner_seen: false,
            plain_since: None,
            output_at: None,
            submitted_at: None,
            screen: None,
        }
    }

    pub(crate) fn open(self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub(crate) fn source(self) -> Source {
        self.source
    }

    #[cfg(all(test, unix))]
    pub(crate) fn plain_since(self) -> Option<Instant> {
        self.plain_since
    }

    /// Whether the visible text is worth reading this tick.
    pub(crate) fn wants_text(self) -> bool {
        (self.open && self.source == Source::Quiet) || self.submitted_at.is_some()
    }

    pub(crate) fn apply(&mut self, signal: Signal, now: Instant) -> Option<Change> {
        match signal {
            Signal::HookStart => {
                self.source = Source::Hook;
                if self.open {
                    None
                } else {
                    Some(self.begin(now))
                }
            }
            Signal::HookEnd | Signal::Ended => self.close(),
            Signal::Title { busy: true } => self.spinner(now),
            Signal::Title { busy: false } => self.plain(now),
            Signal::Submit { screen } => {
                if !self.open && matches!(self.source, Source::Undecided | Source::Quiet) {
                    self.submitted_at = Some(now);
                    self.screen = Some(screen);
                }
                None
            }
            Signal::Output => self.output(now),
            Signal::Screen(hash) => {
                if self.screen == Some(hash) {
                    None
                } else {
                    self.screen = Some(hash);
                    self.output(now)
                }
            }
            Signal::Tick => self.tick(now),
        }
    }

    fn spinner(&mut self, now: Instant) -> Option<Change> {
        self.plain_since = None;
        match (self.open, self.source) {
            (true, Source::Hook) => {
                self.spinner_seen = true;
                None
            }
            (true, _) => {
                self.source = Source::Title;
                self.spinner_seen = true;
                None
            }
            (false, Source::Hook) => None,
            (false, _) => {
                self.source = Source::Title;
                let change = self.begin(now);
                self.spinner_seen = true;
                Some(change)
            }
        }
    }

    fn plain(&mut self, now: Instant) -> Option<Change> {
        let owns = self.open
            && (self.source == Source::Title || (self.source == Source::Hook && self.spinner_seen));
        if !owns {
            return None;
        }
        let since = *self.plain_since.get_or_insert(now);
        if now.duration_since(since) >= TITLE_GRACE {
            self.close()
        } else {
            None
        }
    }

    fn output(&mut self, now: Instant) -> Option<Change> {
        if self.open {
            self.output_at = Some(now);
            return None;
        }
        let answers = self
            .submitted_at
            .is_some_and(|at| now.duration_since(at) < SUBMIT_WINDOW);
        if answers && matches!(self.source, Source::Undecided | Source::Quiet) {
            self.source = Source::Quiet;
            return Some(self.begin(now));
        }
        None
    }

    fn tick(&mut self, now: Instant) -> Option<Change> {
        if let Some(at) = self.submitted_at
            && now.duration_since(at) >= SUBMIT_WINDOW
        {
            self.submitted_at = None;
        }
        let limit = match self.source {
            Source::Quiet => QUIET_OUTPUT,
            Source::Hook => HOOK_SILENCE,
            Source::Undecided | Source::Title => return None,
        };
        let silent = self
            .output_at
            .map_or(Duration::ZERO, |at| now.duration_since(at));
        if silent >= limit { self.close() } else { None }
    }

    fn begin(&mut self, now: Instant) -> Change {
        self.open = true;
        self.spinner_seen = false;
        self.plain_since = None;
        self.output_at = Some(now);
        self.submitted_at = None;
        Change::Opened
    }

    fn close(&mut self) -> Option<Change> {
        if !self.open {
            return None;
        }
        self.open = false;
        Some(Change::Closed)
    }
}

#[cfg(test)]
#[path = "turn_state_test.rs"]
mod tests;
