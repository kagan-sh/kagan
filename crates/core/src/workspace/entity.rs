use crate::thread::turn_state::{Source, TurnState};

pub struct Project {
    pub(crate) record: kg_domain::Project,
}

impl Project {
    #[must_use]
    pub fn record(&self) -> &kg_domain::Project {
        &self.record
    }
}

pub struct Environment {
    pub(crate) record: kg_domain::Environment,
}

impl Environment {
    #[must_use]
    pub fn record(&self) -> &kg_domain::Environment {
        &self.record
    }
}

pub struct Thread {
    pub(crate) record: kg_domain::Thread,
    pub(crate) busy: bool,
    /// The turn of the current run. Stays open after Stop clears `busy`, until the turn's end is
    /// written, so a spinner that is still up does not open a second turn.
    pub(crate) turn: TurnState,
    pub(crate) instruct: Option<String>,
}

impl Thread {
    #[must_use]
    pub fn record(&self) -> &kg_domain::Thread {
        &self.record
    }

    #[must_use]
    pub fn busy(&self) -> bool {
        self.busy
    }

    pub(crate) fn new(record: kg_domain::Thread) -> Self {
        Self {
            record,
            busy: false,
            turn: TurnState::new(Source::Undecided),
            instruct: None,
        }
    }

    #[must_use]
    pub fn running(&self) -> bool {
        self.record.status.running()
    }
}
