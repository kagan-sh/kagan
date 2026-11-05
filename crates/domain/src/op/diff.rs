use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::ThreadId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum DiffScope {
    Uncommitted,
    Branch,
    Commit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Args)]
#[serde(rename_all = "camelCase")]
pub struct DiffQuery {
    #[arg(env = "KG_THREAD_ID")]
    pub thread_id: ThreadId,
    #[arg(long, value_enum)]
    pub scope: DiffScope,
    #[arg(long)]
    pub base: Option<String>,
    #[arg(long)]
    pub rev: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum DiffOp {
    /// Changed files in a scope
    List {
        #[command(flatten)]
        #[serde(flatten)]
        query: DiffQuery,
    },
    /// One file's diff and its new side
    Show {
        #[command(flatten)]
        #[serde(flatten)]
        query: DiffQuery,
        path: String,
    },
    /// Commits that can be reviewed, newest first
    Commits {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
}
