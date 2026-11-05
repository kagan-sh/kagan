use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ProgramOp {
    #[command(skip)]
    Discover,
    /// Programs found on the PATH, the shell, and the recent threads; the first installed one is the default
    List,
}
