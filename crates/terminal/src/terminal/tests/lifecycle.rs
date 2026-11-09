use std::ffi::OsString;
use std::thread;
use std::time::{Duration, Instant};

use super::{Reap, alive, marker_command, quit_within, sh, shell_launch, wait_for, within};
use crate::{Exit, Launch, Terminal};

#[cfg(unix)]
#[test]
fn env_reaches_the_program() {
    let launch = Launch {
        env: vec![(OsString::from("KAGAN_PROBE"), OsString::from("probe-value"))],
        ..sh("printf 'got=%s\\n' \"$KAGAN_PROBE\"; sleep 5")
    };
    let terminal = Terminal::spawn(launch, 80, 24).expect("spawn");
    let frame = wait_for(&terminal, "got=probe-value");
    assert!(frame.contains("got=probe-value"), "{frame}");
}
#[test]
fn the_screen_shows_what_the_shell_prints() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.write(marker_command()).unwrap();
    let frame = wait_for(&terminal, "kagan-marker");
    assert!(frame.contains("kagan-marker"), "{frame:?}");
    assert!(terminal.view().take_changed());
    assert!(!terminal.view().take_changed());
    let grid = terminal.view().grid();
    assert_eq!((grid.lines.len(), grid.lines[0].len()), (24, 80));
    assert!(grid.lines.iter().any(|line| {
        line.iter()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains("kagan-marker")
    }));
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn writing_nothing_is_not_a_close() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.write(Vec::new()).unwrap();
    terminal.write(marker_command()).unwrap();
    let frame = wait_for(&terminal, "kagan-marker");
    assert!(frame.contains("kagan-marker"), "{frame:?}");
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn dropping_the_terminal_reaps_the_shell() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let id = terminal.process_id();
    let reap = Reap(id);
    assert!(alive(id), "shell was not alive");
    within(Duration::from_secs(3), move || drop(terminal)).expect("drop did not return");
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(id) {
        assert!(Instant::now() < deadline, "shell {id} survived drop");
        thread::sleep(Duration::from_millis(20));
    }
    drop(reap);
}

#[test]
fn quit_reaps_the_shell() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let id = terminal.process_id();
    let reap = Reap(id);
    assert!(alive(id), "shell was not alive");
    let started = Instant::now();
    quit_within(terminal, Duration::from_secs(3));
    assert!(started.elapsed() < Duration::from_secs(3));
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(id) {
        assert!(Instant::now() < deadline, "shell {id} survived quit");
        thread::sleep(Duration::from_millis(20));
    }
    drop(reap);
}

#[test]
fn a_program_that_ends_is_exited() {
    let launch = if cfg!(windows) {
        Launch {
            program: OsString::from("cmd.exe"),
            args: vec![OsString::from("/c"), OsString::from("echo done")],
            cwd: None,
            env: Vec::new(),
        }
    } else {
        sh("printf done")
    };
    let terminal = Terminal::spawn(launch, 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !terminal.exited() {
        assert!(Instant::now() < deadline, "the program never ended");
        thread::sleep(Duration::from_millis(20));
    }
    // The exit can be seen before the last output is read (decision 46), so wait for the output.
    assert!(wait_for(&terminal, "done").contains("done"));
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn an_exit_reports_its_code() {
    let launch = if cfg!(windows) {
        Launch {
            program: OsString::from("cmd.exe"),
            args: vec![OsString::from("/c"), OsString::from("exit 7")],
            cwd: None,
            env: Vec::new(),
        }
    } else {
        sh("exit 7")
    };
    let terminal = Terminal::spawn(launch, 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = loop {
        if let Some(exit) = terminal.exit() {
            break exit;
        }
        assert!(Instant::now() < deadline, "the program never ended");
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(exit, Exit::Code(7));
    assert_eq!(terminal.exit(), Some(Exit::Code(7)));
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn a_running_program_is_not_exited() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.write(marker_command()).unwrap();
    wait_for(&terminal, "kagan-marker");
    assert!(!terminal.exited());
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_program_is_told_it_has_a_256_colour_terminal() {
    let terminal = Terminal::spawn(sh("printf 'term=%s\\n' \"$TERM\""), 80, 24).unwrap();
    assert!(wait_for(&terminal, "term=xterm-256color").contains("term=xterm-256color"));
}

#[test]
fn the_program_starts_in_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("kagan-cwd-marker"), b"here").unwrap();
    let mut launch = shell_launch();
    launch.cwd = Some(dir.path().to_path_buf());
    let terminal = Terminal::spawn(launch, 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let list: &[u8] = if cfg!(windows) {
        b"dir /b kagan-cwd-marker\r\n"
    } else {
        b"ls kagan-cwd-marker\n"
    };
    terminal.write(list.to_vec()).unwrap();
    let frame = wait_for(&terminal, "kagan-cwd-marker");
    assert!(frame.contains("kagan-cwd-marker"), "{frame:?}");
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn a_screen_too_small_is_refused() {
    assert!(Terminal::spawn(shell_launch(), 0, 24).is_err());
    assert!(Terminal::spawn(shell_launch(), 80, 0).is_err());
    assert!(Terminal::spawn(shell_launch(), 1, 24).is_err());
    let terminal = Terminal::spawn(shell_launch(), 2, 1).unwrap();
    let reap = Reap(terminal.process_id());
    assert!(terminal.resize(0, 24).is_err());
    assert!(terminal.resize(80, 0).is_err());
    assert!(terminal.resize(1, 24).is_err());
    assert!(terminal.resize(2, 1).is_ok());
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn a_resize_changes_the_screen_and_marks_it_changed() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.resize(100, 40).unwrap();
    // The reader may still hold the screen, and then it applies the resize on its next pass.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !terminal.view().take_changed() {
        assert!(
            Instant::now() < deadline,
            "the resize never marked the screen changed"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(terminal.view().screen().contains("\x1b[40;1H"));
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_resize_reaches_the_shell() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.resize(100, 40).unwrap();
    terminal.write(b"stty size\n".to_vec()).unwrap();
    let frame = wait_for(&terminal, "40 100");
    assert!(frame.contains("40 100"), "{frame:?}");
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_cursor_query_is_answered_with_no_client_attached() {
    let terminal = Terminal::spawn(
        sh("stty raw -echo; printf 'ab\\033[6n'; reply=$(dd bs=1 count=6 2>/dev/null | od -An -c | tr -d ' \\n'); printf 'got:%s:end' \"$reply\""),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    let frame = wait_for(&terminal, ":end");
    assert!(frame.contains("got:033[1;3R:end"), "{frame:?}");
    assert!(!frame.contains("\x1b[6n"), "{frame:?}");
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn the_terminal_keeps_reading_while_nobody_asks_for_a_frame() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("drained");
    let terminal = Terminal::spawn(
        sh(&format!(
            "dd if=/dev/zero bs=1024 count=4096 2>/dev/null; printf x > '{}'",
            marker.display()
        )),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(8);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "the terminal stopped reading");
        thread::sleep(Duration::from_millis(20));
    }
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_program_that_ignores_input_does_not_block_the_caller() {
    let terminal = Terminal::spawn(sh("sleep 30"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let finished = within(Duration::from_secs(5), move || {
        for _ in 0..64 {
            terminal.write(vec![b'x'; 4096]).unwrap();
        }
        terminal
    })
    .expect("a write blocked the caller");
    quit_within(finished, Duration::from_secs(5));
    drop(reap);
}

#[cfg(unix)]
fn start_background_job(terminal: &Terminal) -> u32 {
    terminal.write(b"sleep 1000 &\n".to_vec()).unwrap();
    terminal.write(b"echo job:$! end\n".to_vec()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let screen = terminal.view().screen();
        let pid = screen.match_indices("job:").find_map(|(at, _)| {
            let digits: String = screen[at + 4..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse::<u32>().ok()
        });
        if let Some(pid) = pid {
            return pid;
        }
        assert!(Instant::now() < deadline, "no background job: {screen:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn dies_within(pid: u32, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while alive(pid) {
        if Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
    true
}

#[cfg(unix)]
#[test]
fn dropping_ends_a_background_job_the_shell_started() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let job = start_background_job(&terminal);
    let reap_job = Reap(job);
    assert!(alive(job));
    quit_within(terminal, Duration::from_secs(5));
    assert!(
        dies_within(job, Duration::from_secs(3)),
        "job {job} survived quit"
    );
    drop((reap, reap_job));
}

#[cfg(unix)]
#[test]
fn dropping_a_terminal_does_not_wait_for_a_program_that_will_not_end() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let job = start_background_job(&terminal);
    let reap_job = Reap(job);
    let started = Instant::now();
    drop(terminal);
    assert!(started.elapsed() < Duration::from_secs(1), "drop waited");
    assert!(
        dies_within(job, Duration::from_secs(3)),
        "job {job} survived drop"
    );
    drop((reap, reap_job));
}
