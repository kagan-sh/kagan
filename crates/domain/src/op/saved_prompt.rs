use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{SavedPromptId, ThreadId};

pub(crate) const TITLE_MAX: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SavedPromptOp {
    /// Save text the person can send later. Give the text, or --file (`-` for stdin)
    Add {
        title: String,
        /// The text to save. Omit it when `--file` is set
        #[arg(required_unless_present = "file", conflicts_with = "file")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Read the text from this path, or `-` for stdin
        #[arg(long, value_name = "PATH")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<PathBuf>,
    },
    /// Change a saved prompt's title and text. Its star stays
    Edit {
        id: SavedPromptId,
        title: String,
        body: String,
    },
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
    /// Delete a saved prompt after the person approves it in kagan
    Delete { id: SavedPromptId },
    /// Type a saved prompt into a thread and press Enter
    Send {
        id: SavedPromptId,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
}
