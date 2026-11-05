use std::time::Instant;

use kg_domain::ThreadStatus;

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
    /// Stays set after Stop clears `busy`, until the turn's end is written.
    /// The watcher uses it so a spinner that is still up does not open a second turn.
    pub(crate) turn_open: bool,
    pub(crate) plain_since: Option<Instant>,
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
            turn_open: false,
            plain_since: None,
            instruct: None,
        }
    }

    #[must_use]
    pub fn running(&self) -> bool {
        match self.record.status {
            ThreadStatus::Pending | ThreadStatus::Idle | ThreadStatus::Error => false,
            ThreadStatus::Starting | ThreadStatus::Active | ThreadStatus::Stopping => true,
        }
    }
}
