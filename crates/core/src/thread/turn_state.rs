use std::time::Duration;

/// How long a hook-less agent's output may stay still before its turn ends.
pub(crate) const QUIET_OUTPUT: Duration = Duration::from_secs(15);
/// How long a hook-run turn may stay silent before it ends, for a `Stop` that never came.
pub(crate) const HOOK_SILENCE: Duration = Duration::from_secs(120);

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
    /// The window title's spinner, after the plain grace.
    Title {
        busy: bool,
    },
    /// The person sent text to an agent that has no hook.
    Submit,
    /// The program's visible output has not changed for this long.
    Silence(Duration),
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
}

impl TurnState {
    pub(crate) fn new(source: Source) -> Self {
        Self {
            open: false,
            source,
        }
    }

    pub(crate) fn open(self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub(crate) fn source(self) -> Source {
        self.source
    }

    pub(crate) fn apply(&mut self, signal: Signal) -> Option<Change> {
        let was = self.open;
        match signal {
            Signal::HookStart => {
                self.source = Source::Hook;
                self.open = true;
            }
            Signal::HookEnd | Signal::Ended => self.open = false,
            Signal::Title { busy: true } => match self.source {
                Source::Undecided | Source::Quiet => {
                    self.source = Source::Title;
                    self.open = true;
                }
                Source::Title => self.open = true,
                Source::Hook => {}
            },
            Signal::Title { busy: false } => match self.source {
                Source::Title | Source::Hook => self.open = false,
                Source::Undecided | Source::Quiet => {}
            },
            Signal::Submit => {
                if !self.open && matches!(self.source, Source::Undecided | Source::Quiet) {
                    self.source = Source::Quiet;
                    self.open = true;
                }
            }
            Signal::Silence(for_) => match self.source {
                Source::Quiet if for_ >= QUIET_OUTPUT => self.open = false,
                Source::Hook if for_ >= HOOK_SILENCE => self.open = false,
                _ => {}
            },
        }
        match (was, self.open) {
            (false, true) => Some(Change::Opened),
            (true, false) => Some(Change::Closed),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "turn_state_test.rs"]
mod tests;
