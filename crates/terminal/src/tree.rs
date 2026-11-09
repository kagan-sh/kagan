use kg_process::pid_of;
use rustix::process::{WaitId, WaitIdOptions, waitid};

use crate::terminal::Exit;

// How the program ended, without reaping it: its pid stays ours until the tree kill has
// run, and a reused pid would be someone else's.
pub(crate) fn exit(raw: u32) -> Option<Exit> {
    let status = waitid(
        WaitId::Pid(pid_of(raw)?),
        WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
    )
    .ok()??;
    status
        .exit_status()
        .and_then(|code| u32::try_from(code).ok())
        .map(Exit::Code)
        .or_else(|| status.terminating_signal().map(Exit::Signal))
}

#[cfg(test)]
mod tests;
