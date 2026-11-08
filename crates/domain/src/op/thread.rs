use clap::{
    Args, Subcommand, ValueEnum,
    builder::{StringValueParser, TypedValueParser},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AskField, InteractionOption, PluginId, ProjectId, ThreadId, Timestamp, Visibility};

use super::EnvironmentSpec;
use super::environment::environment_spec;
use super::json;
use super::terminal::{SelectMode, TerminalOp};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ThreadOp {
    /// Show a thread
    Get {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// List threads
    List {
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
        #[arg(long)]
        archived: Option<bool>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        before: Option<Timestamp>,
    },
    /// Start a thread that runs a program in its folder
    Run(Run),
    /// Stop the program
    Stop {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Type into the program's terminal
    Input {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        text: String,
    },
    /// Resize the program's terminal
    Resize {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        cols: u16,
        rows: u16,
    },
    /// The program's screen as ANSI escape sequences; use `--scrollback` for plain text
    Screen {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// Every kept line, as plain text
        #[arg(long)]
        #[serde(default)]
        scrollback: bool,
        /// Last rows of that scrollback, from 1 to 10000. Refused without `--scrollback`
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<u32>,
    },
    /// Scroll the program's view back into its history, or forward with a negative count
    Scroll {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// Lines up into the history; negative moves back down
        #[arg(allow_negative_numbers = true)]
        lines: i32,
    },
    /// Select text on the program's screen from a cell, counted from the top left
    Select {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        mode: SelectMode,
        /// From the left, starting at 0
        column: u16,
        /// From the top, starting at 0
        row: u16,
        /// The right half of the cell
        #[arg(long)]
        right: bool,
    },
    /// The text selected on the program's screen
    Copy {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
    },
    /// Send mouse-report bytes to the program, as decimal byte values such as `27 91 77 32 128 33`.
    /// Unlike `input`, it keeps a scrolled view where it is.
    Report {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// Decimal byte values, 0 to 255, in order
        bytes: Vec<u8>,
    },
    /// Paste text into the program, bracketed when the program asks for that
    Paste {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// An extra terminal, by its number; the program's terminal when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<u32>,
        /// The text. Omit it when `--file` is set
        #[arg(required_unless_present = "file", conflicts_with = "file")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// Read the paste from this path, or `-` for stdin
        #[arg(long, value_name = "PATH")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// Type text into the program and press Enter
    Send {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        text: String,
    },
    /// Extra terminals: login shells in the thread's folder
    #[command(subcommand)]
    Terminal(TerminalOp),
    /// The thread's folder, program, branch, base branch and last commits, read from git without fetching
    Details {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// http and https links printed in the thread's terminals: the program's, then each extra one.
    /// A link printed again moves to the end
    Links {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Archive a thread
    Archive {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Bring back an archived thread
    Unarchive {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Delete a thread and its history for good
    Delete {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// The thread where a person approves this delete
        #[arg(long, env = "KG_THREAD_ID")]
        #[serde(default)]
        approval_thread_id: Option<ThreadId>,
    },
    /// Pin or unpin a thread
    SetPinned {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(action = clap::ArgAction::Set)]
        pinned: bool,
    },
    /// Rename a thread
    Rename {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        title: String,
    },
    /// Mark a thread read
    MarkRead {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// A plugin's metadata on a thread
    Metadata {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(long)]
        plugin_id: PluginId,
    },
    /// Set a plugin's metadata on a thread
    SetMetadata {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// The plugin whose metadata this is
        #[arg(long, value_name = "PLUGIN_ID")]
        plugin_id: PluginId,
        /// The metadata as JSON
        #[arg(long, value_name = "JSON", value_parser = StringValueParser::new().map(json))]
        value: Value,
    },
    /// A thread's events
    Events {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(long)]
        after_seq: Option<u64>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long, value_enum)]
        order: Option<Order>,
    },
    /// Wait for an event
    WaitEvent {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        after_seq: Option<u64>,
        #[arg(long)]
        timeout_ms: u64,
    },
    /// Wait until a reply ends, the program ends, or the time is up
    Wait {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        /// How long to wait, in milliseconds
        #[arg(long)]
        timeout_ms: u64,
    },
    /// Raise a question for the person. The answer arrives as `answered`
    Ask {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        title: String,
        /// An option, as id=label. Repeat for at least two
        #[arg(long = "option", value_name = "ID=LABEL", value_parser = parse_option)]
        options: Vec<InteractionOption>,
        /// Host-drawn form fields (JSON). Omitted or empty still marks a plugin ask for a popup.
        #[arg(skip)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fields: Option<Vec<AskField>>,
    },
    /// Run a stopped or recovered thread's program again, in its folder. Only a request does this.
    ///
    /// A claude, cursor-agent, or agent thread, and a plain codex thread, resume their conversation;
    /// when kagan cannot find it, the agent starts a new one and the thread says so.
    StartAgain {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Agent tools offered to the thread, with plugin instructions
    Tools {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Call an agent tool as the thread
    CallTool {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        name: String,
        #[arg(long, value_parser = StringValueParser::new().map(json), default_value = "{}")]
        input: Value,
    },
    /// Pending interactions waiting for an answer
    Interactions {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Threads whose title or kept terminal lines hold the text, ignoring case. Hidden threads are left out
    Search {
        text: String,
        /// Most threads to return, from 1 to 200; 50 when left out
        #[arg(long)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
        /// Also search archived threads. Each result says whether it is archived
        #[arg(long)]
        #[serde(default)]
        archived: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Args)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[arg(long, env = "KG_PROJECT_ID")]
    pub project_id: ProjectId,
    /// The program to run, such as `claude`, `codex`, or a command the person types. Words split on spaces; single or double quotes keep spaces together
    #[arg(long)]
    pub program: String,
    #[arg(long)]
    pub title: Option<String>,
    /// project-default, `project-on:checkout:<branch>`, `project-on:new:<base>`, personal, `worktree[:<base>[:<branch>]]`, or `reuse:<environment-id>`
    #[arg(long, value_parser = environment_spec, default_value = "worktree")]
    pub environment: EnvironmentSpec,
    #[arg(long, value_enum)]
    #[serde(default)]
    pub visibility: Option<Visibility>,
    #[arg(long)]
    #[serde(default)]
    pub parent_thread_id: Option<ThreadId>,
    #[arg(long)]
    #[serde(default)]
    pub lifecycle_owner_thread_id: Option<ThreadId>,
    #[command(flatten)]
    #[serde(default)]
    pub metadata: Option<Metadata>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Args)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// The plugin whose metadata this is
    #[arg(
        long = "plugin-id",
        id = "metadata_plugin",
        value_name = "PLUGIN_ID",
        required = false,
        requires = "metadata_value"
    )]
    pub plugin_id: PluginId,
    /// The metadata as JSON
    #[arg(
        long = "metadata",
        id = "metadata_value",
        value_name = "JSON",
        required = false,
        requires = "metadata_plugin",
        value_parser = StringValueParser::new().map(json)
    )]
    pub value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Order {
    Asc,
    Desc,
}

/// A branch name taken from a thread title: lowercase, hyphenated, at most 48 characters.
#[must_use]
pub fn branch_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in title.chars() {
        if slug.len() >= 48 {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !slug.is_empty() {
            slug.push('-');
            dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Why a wait returned. `kg thread wait` prints these words, and so does the plugin API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaitOutcome {
    Waiting,
    Exited,
    Failed,
    NotBusy,
    Timeout,
}

impl std::fmt::Display for WaitOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Waiting => "waiting",
            Self::Exited => "exited",
            Self::Failed => "failed",
            Self::NotBusy => "not-busy",
            Self::Timeout => "timeout",
        })
    }
}

/// The most threads `kg thread search` returns. Its help and the refusal name this number.
pub const SEARCH_LIMIT: u32 = 200;
/// How many threads `kg thread search` and the window return when no limit is given.
pub const SEARCH_DEFAULT: u32 = 50;
/// The longest search text, in characters. Its refusal names this number.
pub const SEARCH_TEXT_MAX: usize = 256;

fn parse_option(raw: &str) -> Result<InteractionOption, &'static str> {
    let (id, label) = raw.split_once('=').ok_or("an option is id=label")?;
    if id.is_empty() || label.is_empty() {
        return Err("an option is id=label");
    }
    Ok(InteractionOption {
        id: id.to_owned(),
        label: label.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{InteractionOption, parse_option};

    #[test]
    fn an_option_is_a_nonempty_id_and_label() {
        assert_eq!(
            parse_option("allow=Allow once"),
            Ok(InteractionOption {
                id: "allow".into(),
                label: "Allow once".into(),
            })
        );
        for raw in ["allow", "=label", "id=", "="] {
            assert_eq!(parse_option(raw), Err("an option is id=label"), "{raw:?}");
        }
    }
}
