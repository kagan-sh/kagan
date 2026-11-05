use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum WindowOp {
    /// Show the application name and version
    About,
    /// Hide kagan
    Hide,
    /// Hide the other applications
    HideOthers,
    /// Show the applications hidden by Hide Others
    ShowAll,
    /// Quit. Programs still running end
    Quit,
    /// Minimize the window
    Minimize,
    /// Zoom the window
    Zoom,
    /// Close the window. Programs keep running
    Close,
    /// Open the next visible thread and focus its terminal
    NextThread,
    /// Open the previous visible thread and focus its terminal
    PreviousThread,
    /// Open the Nth visible thread and focus its terminal
    Thread {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=9))]
        index: u8,
    },
    /// Open the oldest unread thread or question and focus its terminal
    NextThreadNeedingYou,
    /// Toggle wrapping in the open diff review
    ToggleReviewWrap,
    /// Open the command list
    CommandList,
    /// Open the command list on thread search
    SearchThreads,
    /// Move the keyboard to the shown thread's next terminal
    NextTerminal,
    /// Move the keyboard to the shown thread's previous terminal
    PreviousTerminal,
    /// Open the newest local link the shown thread printed, such as `localhost:3000`, in the system browser
    OpenLocalLink,
}
