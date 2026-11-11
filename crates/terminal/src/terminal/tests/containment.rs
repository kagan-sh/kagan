use std::ffi::OsString;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use super::{Reap, alive, quit_within, within};
use crate::{Containment, Launch, Terminal};

const WAIT: Duration = Duration::from_secs(30);

fn cmd_launch(script: &str) -> Launch {
    Launch {
        program: OsString::from("cmd.exe"),
        args: ["/C", script].into_iter().map(OsString::from).collect(),
        cwd: None,
        env: Vec::new(),
    }
}

// One ping opens its own console, so closing the pty's console cannot end it; only the job can.
// The second ping shares the pty's console. Both are children of cmd.exe, the program.
fn two_pings_launch() -> Launch {
    cmd_launch("start /min ping -n 60 127.0.0.1 & ping -n 60 127.0.0.1 >NUL")
}

fn pings_of(parent: u32) -> Vec<u32> {
    children_of(parent)
        .into_iter()
        .filter(|(_, name)| name.eq_ignore_ascii_case("PING.EXE"))
        .map(|(pid, _)| pid)
        .collect()
}

fn wait_for_pings(terminal: &Terminal, count: usize) -> Vec<Reap> {
    let deadline = Instant::now() + WAIT;
    loop {
        let pids = pings_of(terminal.process_id());
        if pids.len() >= count {
            return pids.into_iter().map(Reap).collect();
        }
        assert!(
            Instant::now() < deadline,
            "only {} grandchildren started",
            pids.len()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn children_of(parent: u32) -> Vec<(u32, String)> {
    let mut children = Vec::new();
    // SAFETY: the snapshot handle is closed below; the entry is a writable buffer whose size is set first.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return children;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(0);
        let mut more = Process32FirstW(snapshot, &raw mut entry) != 0;
        while more {
            if entry.th32ParentProcessID == parent {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                children.push((entry.th32ProcessID, name));
            }
            more = Process32NextW(snapshot, &raw mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    children
}

fn wait_until_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) {
        assert!(Instant::now() < deadline, "grandchild {pid} survived");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn stop_ends_every_grandchild_through_the_job() {
    let terminal = Terminal::spawn(two_pings_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let grandchildren = wait_for_pings(&terminal, 2);
    assert!(matches!(terminal.containment(), Containment::Job(_)));
    for grandchild in &grandchildren {
        assert!(alive(grandchild.0), "grandchild was not alive");
    }
    terminal.stop();
    for grandchild in &grandchildren {
        wait_until_gone(grandchild.0);
    }
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn dropping_the_terminal_ends_every_grandchild_through_the_job() {
    let terminal = Terminal::spawn(two_pings_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let grandchildren = wait_for_pings(&terminal, 2);
    assert!(matches!(terminal.containment(), Containment::Job(_)));
    for grandchild in &grandchildren {
        assert!(alive(grandchild.0), "grandchild was not alive");
    }
    within(Duration::from_secs(5), move || drop(terminal)).expect("drop did not return");
    for grandchild in &grandchildren {
        wait_until_gone(grandchild.0);
    }
    drop(reap);
}

// Proves the drop test can only pass through the job: without one, the console close leaves the
// ping that has its own console alive.
#[test]
fn without_a_job_dropping_leaves_the_own_console_grandchild_alive() {
    let terminal = Terminal::start(two_pings_launch(), 80, 24, |pid| {
        super::super::contain(pid, |_| Err(io::Error::other("forced assign failure")))
    })
    .unwrap();
    let reap = Reap(terminal.process_id());
    let grandchildren = wait_for_pings(&terminal, 2);
    within(Duration::from_secs(5), move || drop(terminal)).expect("drop did not return");
    thread::sleep(Duration::from_secs(1));
    assert!(
        grandchildren.iter().any(|grandchild| alive(grandchild.0)),
        "every grandchild died without a job"
    );
    drop(grandchildren);
    drop(reap);
}

#[test]
fn a_failed_job_is_kept_as_unavailable_and_stop_still_ends_the_program() {
    let terminal = Terminal::start(
        Launch {
            program: OsString::from("ping.exe"),
            args: ["-n", "60", "127.0.0.1"]
                .into_iter()
                .map(OsString::from)
                .collect(),
            cwd: None,
            env: Vec::new(),
        },
        80,
        24,
        |pid| super::super::contain(pid, |_| Err(io::Error::other("forced assign failure"))),
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    match terminal.containment() {
        Containment::Unavailable(error) => {
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert!(
                error.to_string().contains("forced assign failure"),
                "{error}"
            );
        }
        other => panic!("expected unavailable containment, got {other}"),
    }
    terminal.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while terminal.exit().is_none() {
        assert!(Instant::now() < deadline, "stop did not end the program");
        thread::sleep(Duration::from_millis(20));
    }
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

// Three 1 MiB submits to a program that never reads, under the 4 MiB input cap (decision 252). Each must settle before finish runs, and
// finish must settle afterwards, so a blocked writer shows up as a timeout here.
#[test]
fn finish_settles_large_submits_to_a_program_that_never_reads() {
    let terminal = Terminal::spawn(cmd_launch("ping -n 60 127.0.0.1 >NUL"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let chunk = "x".repeat(1024 * 1024);
    let pending: Vec<_> = (0..3)
        .map(|n| terminal.submit(&format!("{n}-{chunk}")).unwrap().1)
        .collect();
    for written in &pending {
        let result = written.recv_timeout(Duration::from_secs(10));
        assert!(
            matches!(result, Ok(Ok(()))),
            "a submit did not complete before finish: {result:?}"
        );
    }
    let finished = terminal.finish();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.settled() {
        assert!(Instant::now() < deadline, "finish did not settle");
        thread::sleep(Duration::from_millis(10));
    }
    drop(reap);
}
