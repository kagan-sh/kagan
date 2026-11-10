use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum GuideChapter {
    Threads,
    Projects,
    Environments,
    Plugins,
    Skills,
    Ui,
    Customize,
    Json,
}
