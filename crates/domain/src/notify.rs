//! Which thread events post a system notification. The global level is the `notify` preference;
//! a thread's own level overrides it.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// What a person has asked for, for every thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum NotifyLevel {
    /// A turn ended, a question or approval waits, or a program failed or exited
    #[default]
    All,
    /// Only a question or approval that waits for the person
    Asks,
    Off,
}

/// One thread's own level. `Default` follows the global level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadNotify {
    #[default]
    Default,
    All,
    Asks,
    Muted,
}

/// What happened to a thread that may need the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyReason {
    TurnEnded,
    Asked,
    Failed,
}

impl NotifyLevel {
    #[must_use]
    pub fn posts(self, reason: NotifyReason) -> bool {
        match self {
            Self::All => true,
            Self::Asks => reason == NotifyReason::Asked,
            Self::Off => false,
        }
    }
}

impl ThreadNotify {
    /// A thread's own level replaces the global one, so `All` posts even when the global level is off.
    #[must_use]
    pub fn posts(self, global: NotifyLevel, reason: NotifyReason) -> bool {
        match self {
            Self::Default => global.posts(reason),
            Self::All => NotifyLevel::All.posts(reason),
            Self::Asks => NotifyLevel::Asks.posts(reason),
            Self::Muted => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NotifyLevel, NotifyReason, ThreadNotify};

    const REASONS: [NotifyReason; 3] = [
        NotifyReason::TurnEnded,
        NotifyReason::Asked,
        NotifyReason::Failed,
    ];

    #[test]
    fn the_global_level_decides_what_posts() {
        let posted = |level: NotifyLevel| REASONS.map(|reason| level.posts(reason));
        assert_eq!(posted(NotifyLevel::All), [true, true, true]);
        assert_eq!(posted(NotifyLevel::Asks), [false, true, false]);
        assert_eq!(posted(NotifyLevel::Off), [false, false, false]);
    }

    #[test]
    fn a_thread_level_other_than_default_replaces_the_global_one() {
        let posted = |thread: ThreadNotify, global: NotifyLevel| {
            REASONS.map(|reason| thread.posts(global, reason))
        };
        assert_eq!(
            posted(ThreadNotify::Default, NotifyLevel::Asks),
            [false, true, false]
        );
        assert_eq!(
            posted(ThreadNotify::All, NotifyLevel::Off),
            [true, true, true]
        );
        assert_eq!(
            posted(ThreadNotify::Asks, NotifyLevel::All),
            [false, true, false]
        );
        assert_eq!(
            posted(ThreadNotify::Muted, NotifyLevel::All),
            [false, false, false]
        );
    }
}
