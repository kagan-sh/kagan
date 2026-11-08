//! The Browser tab's page, driven by a thread.

use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use super::check::carried_text;
use super::{TEXT_MAX, ThreadId};

/// What a thread may do with the Browser tab's one page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum BrowserOp {
    /// Give this thread the Browser tab's page and load a URL
    Open {
        url: String,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// The page this thread drives: thread, url, title, and whether the tab is drawn
    Status {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Controls and text on the page, each control with a ref for the next command
    Snapshot {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Load another URL in the page
    Goto {
        url: String,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Go back in the page's history
    Back {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Go forward in the page's history
    Forward {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Load the page again
    Reload {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Click the control with this ref
    Click {
        #[arg(value_name = "ref")]
        #[serde(rename = "ref")]
        ref_id: u32,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Set a control's text, or read it from --file
    Fill {
        #[arg(value_name = "ref")]
        #[serde(rename = "ref")]
        ref_id: u32,
        /// The text. Omit it when `--file` is set
        #[arg(required_unless_present = "file", conflicts_with = "file")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// Read the text from this path, or `-` for stdin
        #[arg(long, value_name = "PATH")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<PathBuf>,
        #[arg(long, env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Insert text into the focused control
    Type {
        text: String,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Press enter, escape, tab, a letter, or an arrow
    Press {
        key: String,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Choose a <select> option by value or label
    Select {
        #[arg(value_name = "ref")]
        #[serde(rename = "ref")]
        ref_id: u32,
        option: String,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Scroll the page by dy pixels
    Scroll {
        #[arg(allow_hyphen_values = true)]
        dy: f32,
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
    /// Stop driving; the page stays as the person's
    Close {
        #[arg(env = "KG_THREAD_ID")]
        thread_id: ThreadId,
    },
}

impl BrowserOp {
    /// The thread that must own the page this verb reaches.
    #[must_use]
    pub fn thread(&self) -> &ThreadId {
        match self {
            Self::Open { thread_id, .. }
            | Self::Status { thread_id }
            | Self::Snapshot { thread_id }
            | Self::Goto { thread_id, .. }
            | Self::Back { thread_id }
            | Self::Forward { thread_id }
            | Self::Reload { thread_id }
            | Self::Click { thread_id, .. }
            | Self::Fill { thread_id, .. }
            | Self::Type { thread_id, .. }
            | Self::Press { thread_id, .. }
            | Self::Select { thread_id, .. }
            | Self::Scroll { thread_id, .. }
            | Self::Close { thread_id } => thread_id,
        }
    }
}

/// The page a thread drives, as `open` and `status` report it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserView {
    pub thread_id: ThreadId,
    pub thread: String,
    pub url: String,
    pub title: String,
    /// Whether the driven page is the tab on screen.
    pub shown: bool,
}

/// Controls and text read from the page. Text is what a person can read on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub url: String,
    pub title: String,
    pub controls: Vec<BrowserControl>,
    pub text: String,
    pub truncated: bool,
}

/// One control the snapshot found, with the ref the next command names it by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserControl {
    #[serde(rename = "ref")]
    pub ref_id: u32,
    pub role: String,
    pub name: String,
    /// Never a password's value.
    pub value: Option<String>,
    pub focused: bool,
    pub disabled: bool,
    pub checked: Option<bool>,
}

pub(super) fn browser_check(op: &BrowserOp) -> Result<(), &'static str> {
    match op {
        BrowserOp::Open { url, .. } | BrowserOp::Goto { url, .. } if url.trim().is_empty() => {
            Err("type a web address, such as example.com")
        }
        BrowserOp::Fill { text, file, .. } => carried_text(text.as_ref(), file.as_ref()),
        BrowserOp::Type { text, .. } if text.len() > TEXT_MAX => Err("text is over 1 MiB"),
        BrowserOp::Type { text, .. } if text.is_empty() => Err("text must not be empty"),
        BrowserOp::Press { key, .. } if key.trim().is_empty() => Err("key must not be empty"),
        BrowserOp::Scroll { dy, .. } if !dy.is_finite() => Err("scroll must be a number"),
        _ => Ok(()),
    }
}
