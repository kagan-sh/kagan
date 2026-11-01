use std::process::Command;

use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process, waitid};

use crate::terminal::Exit;

// Pid 0 and 1 are never ours, and `kill -9 -1` would signal everything the user owns.
pub(crate) fn pid_of(raw: u32) -> Option<Pid> {
    i32::try_from(raw)
        .ok()
        .filter(|raw| *raw > 1)
        .and_then(Pid::from_raw)
}

// Whether the program has ended, without reaping it: its pid stays ours until the
// tree kill has run, and a reused pid would be someone else's.
pub(crate) fn ended(raw: u32) -> bool {
    pid_of(raw).is_some_and(|pid| {
        matches!(
            waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
            ),
            Ok(Some(_))
        )
    })
}

// How the program ended, read with the same `waitid` that `ended` uses, so the pid stays ours.
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

// A shell's background jobs sit in their own process groups and keep the pty open after
// the shell dies, so everything that started under the program ends, found with `getsid`.
// A pid is checked again right before the kill: a wrong one ends the user's other programs.
// Kills the leader without reaping it, so its exit can still be read from the same pid.
pub(crate) fn kill_leader(raw: u32) {
    if let Some(pid) = pid_of(raw) {
        let _ = kill_process(pid, Signal::KILL);
    }
}

pub(crate) fn kill_tree(leader: u32) {
    let Some(leader) = pid_of(leader) else {
        return;
    };
    let Ok(listed) = Command::new("ps").args(["-A", "-o", "pid="]).output() else {
        return;
    };
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter_map(pid_of)
        .filter(|pid| sid_of(*pid) == Some(leader))
        .for_each(|pid| {
            if sid_of(pid) == Some(leader) {
                let _ = kill_process(pid, Signal::KILL);
            }
        });
}

// Linux gives kernel threads and a container's init sid 0, and rustix's `getsid` turns
// that 0 into a `Pid` unchecked, so the call goes to libc. 0 and -1 (no access) mean none.
fn sid_of(pid: Pid) -> Option<Pid> {
    // SAFETY: getsid takes a pid by value and reads no memory of ours.
    let sid = unsafe { libc::getsid(pid.as_raw_nonzero().get()) };
    (sid > 0).then_some(sid).and_then(Pid::from_raw)
}

#[cfg(test)]
mod tests;
