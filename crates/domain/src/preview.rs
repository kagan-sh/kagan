use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::ThreadId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum PreviewOp {
    /// Show one file in the Preview tab
    Show {
        /// File to show. A relative name joins the thread's folder; without a thread it must be absolute
        path: String,
        /// Thread whose folder a relative path joins (defaults to `KG_THREAD_ID`)
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: Option<ThreadId>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewImageFormat {
    Png,
    Jpeg,
}

impl PreviewImageFormat {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
        }
    }
}

/// How much of a long text the preview kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCut {
    pub shown: usize,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum PreviewKind {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cut: Option<PreviewCut>,
    },
    Markdown {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cut: Option<PreviewCut>,
    },
    Csv {
        rows: Vec<Vec<String>>,
        rows_total: usize,
        columns_total: usize,
        truncated: bool,
    },
    Image {
        format: PreviewImageFormat,
        width: u32,
        height: u32,
    },
    Unavailable {
        reason: String,
    },
}

/// One file as the Preview tab shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFile {
    /// The path as the person named it, or relative to the environment root.
    pub path: String,
    /// The resolved path an image is read from.
    pub absolute: PathBuf,
    pub bytes: u64,
    #[serde(flatten)]
    pub kind: PreviewKind,
}
