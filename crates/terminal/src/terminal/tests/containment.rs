use std::ffi::OsString;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use super::{Reap, alive, quit_within, within};
use crate::{Containment, Launch, Terminal};

const WAIT: Duration = Duration::from_secs(30);

fn grandchild_launch() -> Launch {
    let script = "$p = Start-Process -FilePath ping.exe -ArgumentList '-n 60 127.0.0.1' \
                  -NoNewWindow -PassThru; Write-Output ('grandchild=' + $p.Id); Start-Sleep -Seconds 60";
    Launch {
        program: OsString::from("powershell.exe"),
        args: ["-NoProfile", "-Command", script]
            .into_iter()
            .map(OsString::from)
            .collect(),
        cwd: None,
        env: Vec::new(),
    }
}

fn ping_launch() -> Launch {
    Launch {
        program: OsString::from("ping.exe"),
        args: ["-n", "60", "127.0.0.1"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        cwd: None,
        env: Vec::new(),
    }
}

// The grandchild starts after PowerShell has started, well after the pty began the program.
fn grandchild_pid(terminal: &Terminal) -> u32 {
    let deadline = Instant::now() + WAIT;
    loop {
        let frame = terminal.view().screen();
        if let Some(rest) = frame.split("grandchild=").nth(1) {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(pid) = digits.parse() {
                return pid;
            }
        }
        assert!(Instant::now() < deadline, "no grandchild printed: {frame}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_until_gone(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) {
        assert!(Instant::now() < deadline, "grandchild {pid} survived");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn stop_ends_a_grandchild_the_program_started() {
    let terminal = Terminal::spawn(grandchild_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let grandchild = grandchild_pid(&terminal);
    let _grandchild_reap = Reap(grandchild);
    assert!(alive(grandchild), "grandchild was not alive");
    terminal.stop();
    wait_until_gone(grandchild);
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn dropping_the_terminal_ends_a_grandchild_the_program_started() {
    let terminal = Terminal::spawn(grandchild_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let grandchild = grandchild_pid(&terminal);
    let _grandchild_reap = Reap(grandchild);
    assert!(alive(grandchild), "grandchild was not alive");
    within(Duration::from_secs(5), move || drop(terminal)).expect("drop did not return");
    wait_until_gone(grandchild);
    drop(reap);
}

#[test]
fn a_failed_job_is_kept_as_unavailable_and_stop_still_ends_the_program() {
    let terminal = Terminal::start(ping_launch(), 80, 24, |pid| {
        super::super::contain(pid, |_| Err(io::Error::other("forced assign failure")))
    })
    .unwrap();
    let reap = Reap(terminal.process_id());
    match terminal.containment() {
        Containment::Unavailable(reason) => assert!(reason.contains("forced assign failure")),
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

#[test]
fn finish_settles_a_submit_queued_behind_a_program_that_never_reads() {
    let launch = Launch {
        program: OsString::from("cmd.exe"),
        args: ["/C", "ping -n 60 127.0.0.1 >NUL"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        cwd: None,
        env: Vec::new(),
    };
    let terminal = Terminal::spawn(launch, 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let chunk = "x".repeat(64 * 1024);
    let pending: Vec<_> = (0..4)
        .map(|n| terminal.submit(&format!("{n}-{chunk}")).unwrap().1)
        .collect();
    thread::sleep(Duration::from_millis(100));
    let finished = terminal.finish();
    for written in pending {
        assert!(
            written.recv_timeout(Duration::from_secs(10)).is_ok(),
            "a queued submit never settled"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.settled() {
        assert!(Instant::now() < deadline, "finish did not settle");
        thread::sleep(Duration::from_millis(10));
    }
    drop(reap);
}
