use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::ThreadId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum TerminalOp {
    /// Open another terminal in the thread's folder, running the login shell
    New {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// The thread's extra terminals
    List {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
    },
    /// Close an extra terminal and end its shell
    Close {
        #[arg(env = "KG_THREAD_ID")]
        id: ThreadId,
        number: u32,
    },
}

/// A terminal kagan opened beside a thread's program. Numbers are never reused within a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraTerminal {
    pub number: u32,
    /// The title the shell set, if any
    pub title: Option<String>,
}

/// How a terminal selection starts. `Extend` moves the end of the selection already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SelectMode {
    Simple,
    Word,
    Line,
    Block,
    Extend,
}

/// The columns a terminal may have.
pub const TERMINAL_COLS: std::ops::RangeInclusive<u16> = 2..=1000;
/// The rows a terminal may have.
pub const TERMINAL_ROWS: std::ops::RangeInclusive<u16> = 1..=500;
/// The most bytes a saved screen keeps, history and the visible redraw together.
pub const SCREEN_MAX_BYTES: usize = 4 * 1024 * 1024;
