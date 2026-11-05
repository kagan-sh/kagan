use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::PluginId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfacePlacement {
    Left,
    Right,
    Center,
}

/// A part of the window a plugin may replace: the thread list or the file review.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    Threads,
    Review,
}

impl Slot {
    pub const ALL: [Self; 2] = [Self::Threads, Self::Review];

    /// The manifest and CLI name: `threads` or `review`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Threads => "threads",
            Self::Review => "review",
        }
    }

    /// The preference that holds the plugin id filling this slot, or `kagan`.
    #[must_use]
    pub const fn preference(self) -> &'static str {
        match self {
            Self::Threads => "slot.threads",
            Self::Review => "slot.review",
        }
    }

    /// What a person reads: "replaceable sidebar" or "replaceable review".
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Threads => "replaceable sidebar",
            Self::Review => "replaceable review",
        }
    }
}

/// Who fills a slot: the built-in, or one plugin. Written `kagan` or the plugin id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum SlotChoice {
    Kagan,
    Plugin(PluginId),
}

impl std::str::FromStr for SlotChoice {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = PluginId(value.to_owned());
        match value {
            "kagan" => Ok(Self::Kagan),
            _ if id.valid() => Ok(Self::Plugin(id)),
            _ => Err(format!("{value:?} must be a plugin id or kagan")),
        }
    }
}

impl TryFrom<String> for SlotChoice {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<SlotChoice> for String {
    fn from(choice: SlotChoice) -> Self {
        choice.to_string()
    }
}

impl std::fmt::Display for SlotChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kagan => f.write_str("kagan"),
            Self::Plugin(id) => f.write_str(&id.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceItem {
    pub plugin: PluginId,
    pub id: String,
    pub title: String,
    pub placement: SurfacePlacement,
    /// Selected or expanded. A change still waiting keeps the previous selection.
    pub open: bool,
    /// The body is painted at a non-zero size. While a change waits, this stays with the
    /// frame from before that change, unless a newer mount replaced the body.
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SurfaceOp {
    /// Declared plugin surfaces, and which are on screen
    List,
    /// Show the threads list in the left sidebar
    Threads,
    /// Show a declared surface. Succeeds only when it is on screen.
    Open { plugin: PluginId, id: String },
    /// Close a left tool, a center page, or a right tab
    Close { plugin: PluginId, id: String },
}
