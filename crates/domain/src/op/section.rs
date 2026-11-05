use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{SectionId, ThreadId};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SectionOp {
    /// Create a section, and file a thread under it
    Create {
        name: String,
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
    /// Rename a section
    Rename { id: SectionId, name: String },
    /// Remove a section. Threads in it stay, unfiled.
    Delete { id: SectionId },
    /// File a thread under a section. Omit the section to clear it.
    File {
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: ThreadId,
        #[arg(long)]
        section_id: Option<SectionId>,
    },
}
