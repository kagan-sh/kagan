use std::{convert::Infallible, fmt, num::ParseIntError, str::FromStr};

use serde::{Deserialize, Serialize};

macro_rules! ids {
    ($($name:ident),+) => {$(
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = Infallible;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_owned()))
            }
        }
    )+};
}

ids!(
    ProjectId,
    EnvironmentId,
    ThreadId,
    SectionId,
    PendingInteractionId,
    PluginId,
    AutomationId,
    SavedPromptId,
    PreviewId
);

impl ProjectId {
    #[must_use]
    pub fn personal() -> Self {
        Self("personal".into())
    }
}

impl PluginId {
    /// Whether this is reverse-DNS, as `dev.example.name`: lowercase letters, digits and inner
    /// dashes in two or more labels. It names the plugin's folder, so nothing else is allowed.
    #[must_use]
    pub fn valid(&self) -> bool {
        let labels: Vec<&str> = self.0.split('.').collect();
        labels.len() >= 2
            && labels.iter().all(|label| {
                !label.is_empty()
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            })
    }
}

impl ThreadId {
    #[must_use]
    pub fn plain(&self) -> bool {
        plain_identifier(&self.0)
    }
}

impl SavedPromptId {
    #[must_use]
    pub fn plain(&self) -> bool {
        plain_identifier(&self.0)
    }
}

#[must_use]
fn plain_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= 128
        && first.is_ascii_alphanumeric()
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

/// The id kagan gives an agent's conversation at its first start, so Start again can resume it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Conversation(uuid::Uuid);

impl Conversation {
    #[must_use]
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl FromStr for Conversation {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl fmt::Display for Conversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

/// Milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

impl FromStr for Timestamp {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}
