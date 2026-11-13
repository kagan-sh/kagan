use std::process::Command;

use rustix::process::{Pid, Signal, kill_process};

// Pid 0 and 1 are never ours, and `kill -9 -1` would signal everything the user owns.
#[must_use]
pub fn pid_of(raw: u32) -> Option<Pid> {
    i32::try_from(raw)
        .ok()
        .filter(|raw| *raw > 1)
        .and_then(Pid::from_raw)
}

// Kills the leader without reaping it, so its exit can still be read from the same pid.
pub fn kill_leader(raw: u32) {
    if let Some(pid) = pid_of(raw) {
        // The process may already be gone.
        let _ = kill_process(pid, Signal::KILL);
    }
}

// A shell's background jobs sit in their own process groups and keep the pty open after
// the shell dies, so everything that started under the program ends, found with `getsid`.
// A pid is checked right before the kill: a wrong one ends the user's other programs.
pub fn kill_tree(leader: u32) {
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
        .for_each(|pid| {
            if sid_of(pid) == Some(leader) {
                // The process may already be gone.
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

// Every process under `leader`, by parent pid, as it is now. A program that starts its own
// session or group leaves its parent's group, so this list is what catches it.
#[must_use]
pub fn descendants(leader: u32) -> Vec<u32> {
    let Ok(listed) = Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "ppid="])
        .output()
    else {
        return Vec::new();
    };
    let pairs: Vec<(u32, u32)> = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace().map(str::parse::<u32>);
            Some((words.next()?.ok()?, words.next()?.ok()?))
        })
        .collect();
    let mut found = Vec::new();
    let mut parents = vec![leader];
    while let Some(parent) = parents.pop() {
        for &(pid, ppid) in &pairs {
            if ppid == parent && pid != leader && !found.contains(&pid) {
                found.push(pid);
                parents.push(pid);
            }
        }
    }
    found
}

// Kills each listed pid that still exists. A pid is listed only while its process is alive, so a
// reused number needs the old process to exit and a new one to take it within the wait.
pub fn kill_listed(pids: &[u32]) {
    for pid in pids.iter().filter_map(|raw| pid_of(*raw)) {
        // The process may already be gone.
        let _ = kill_process(pid, Signal::KILL);
    }
}
