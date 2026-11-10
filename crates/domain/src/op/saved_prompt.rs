use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::SavedPromptId;

pub(crate) const TITLE_MAX: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SavedPromptOp {
    /// Save text the person can send later
    Add { title: String, body: String },
    /// Saved prompts whose title or body contains this text. An empty search lists them all
    Search {
        #[arg(default_value = "")]
        text: String,
    },
    /// Star or unstar a saved prompt
    Star {
        id: SavedPromptId,
        #[arg(action = clap::ArgAction::Set)]
        starred: bool,
    },
    /// Delete a saved prompt
    Delete { id: SavedPromptId },
    /// Paste a saved prompt into the shown thread
    Send { id: SavedPromptId },
}
