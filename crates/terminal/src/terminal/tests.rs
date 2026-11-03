use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

fn shell_launch() -> Launch {
    Launch {
        program: shell_program().to_os_string(),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
    }
}

fn shell_program() -> &'static OsStr {
    if cfg!(windows) {
        OsStr::new("cmd.exe")
    } else {
        OsStr::new("/bin/sh")
    }
}

fn marker_command() -> Vec<u8> {
    if cfg!(windows) {
        b"echo kagan-marker\r\n".to_vec()
    } else {
        b"printf 'kagan-marker\\n'\n".to_vec()
    }
}

fn sh(script: &str) -> Launch {
    Launch {
        program: OsString::from("/bin/sh"),
        args: vec![OsString::from("-c"), OsString::from(script)],
        cwd: None,
        env: Vec::new(),
    }
}

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

fn wait_for(terminal: &Terminal, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = terminal.screen();
        if frame.contains(needle) || Instant::now() > deadline {
            return frame;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn quit_within(terminal: Terminal, limit: Duration) {
    within(limit, move || drop(terminal)).expect("quit did not return");
}

fn within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.recv_timeout(limit).ok()
}

fn alive(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    #[cfg(unix)]
    {
        // A zombie has exited and waits only to be reaped, so it is not alive.
        let signalable = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        signalable
            && !Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .is_ok_and(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .trim_start()
                        .starts_with('Z')
                })
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|word| word == pid.to_string())
            })
    }
}

struct Reap(u32);

impl Drop for Reap {
    fn drop(&mut self) {
        if self.0 <= 1 {
            return;
        }
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                // Without `--`, Linux `kill -9 -7551` signalled group 7, not 7551.
                .args(["-9", "--", &format!("-{}", self.0)])
                .stderr(Stdio::null())
                .status();
            let _ = Command::new("kill")
                .args(["-9", &self.0.to_string()])
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &self.0.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

#[test]
fn the_screen_shows_what_the_shell_prints() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.write(marker_command()).unwrap();
    let frame = wait_for(&terminal, "kagan-marker");
    assert!(frame.contains("kagan-marker"), "{frame:?}");
    assert!(terminal.take_changed());
    assert!(!terminal.take_changed());
    let grid = terminal.grid();
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
    assert!(terminal.take_changed());
    assert!(terminal.screen().contains("\x1b[40;1H"));
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
        let screen = terminal.screen();
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

#[cfg(unix)]
#[test]
fn a_hold_that_is_never_released_does_not_freeze_the_screen_or_its_queries() {
    let terminal = Terminal::spawn(
        sh("stty raw -echo; printf '\\033[?2026hheld\\033[6n'; reply=$(dd bs=1 count=6 2>/dev/null | od -An -c | tr -d ' \\n'); printf 'got:%s:end' \"$reply\"; sleep 30"),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(8);
    while !terminal.screen().contains(":end") {
        assert!(Instant::now() < deadline, "{:?}", terminal.screen());
        let _ = terminal.take_changed();
        thread::sleep(Duration::from_millis(33));
    }
    assert!(
        terminal.screen().contains("got:033[1;5R:end"),
        "{:?}",
        terminal.screen()
    );
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn a_grid_is_read_while_the_reader_holds_the_screen() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let screen = Arc::clone(&terminal.shared.screen);
    let held = lock(&screen);
    let observed = within(Duration::from_secs(2), move || {
        (terminal.take_changed(), terminal.grid().lines.len())
    });
    drop(held);
    assert_eq!(
        observed.map(|(_, rows)| rows),
        Some(24),
        "the grid waited for the screen lock"
    );
    drop(reap);
}

#[test]
fn a_resize_under_a_busy_reader_returns_at_once_and_lands_later() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let screen = Arc::clone(&terminal.shared.screen);
    let held = lock(&screen);
    let (returned_tx, returned) = mpsc::channel();
    let (go_tx, go) = mpsc::channel::<()>();
    let (landed_tx, landed) = mpsc::channel();
    thread::spawn(move || {
        let resized = terminal.resize(100, 30).is_ok();
        let _ = returned_tx.send(resized);
        let _ = go.recv();
        // A call that finds the reader still holding the screen applies nothing, so keep asking.
        let deadline = Instant::now() + Duration::from_secs(1);
        let size = loop {
            let _ = terminal.take_changed();
            let grid = terminal.grid();
            let size = (grid.lines.len(), grid.lines[0].len());
            if size == (30, 100) || Instant::now() > deadline {
                break size;
            }
            thread::sleep(Duration::from_millis(10));
        };
        let _ = landed_tx.send(size);
        drop(terminal);
    });
    assert_eq!(
        returned.recv_timeout(Duration::from_secs(2)),
        Ok(true),
        "resize waited for the reader"
    );
    drop(held);
    go_tx.send(()).unwrap();
    assert_eq!(
        landed.recv_timeout(Duration::from_secs(2)),
        Ok((30, 100)),
        "the resize never landed"
    );
    drop(reap);
}

#[test]
fn an_older_frame_never_replaces_a_newer_one() {
    let frame = Mutex::new(Frame {
        version: 0,
        grid: Arc::new(Screen::new(80, 24).grid()),
    });
    store_frame(&frame, 2, Screen::new(100, 24).grid());
    store_frame(&frame, 1, Screen::new(80, 24).grid());
    assert_eq!(lock(&frame).grid.lines[0].len(), 100);
    assert_eq!(lock(&frame).version, 2);
}

#[cfg(unix)]
#[test]
fn reads_build_no_grid_until_one_is_asked_for_and_then_it_shows_the_latest_screen() {
    let terminal = Terminal::spawn(sh("seq 1 20000; echo LAST-LINE; sleep 30"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    let screen = wait_for(&terminal, "LAST-LINE");
    assert!(screen.contains("LAST-LINE"), "{screen}");
    let reads = terminal.shared.versions.load(Ordering::Relaxed);
    assert!(reads > 10, "only {reads} reads");
    assert_eq!(terminal.shared.builds.load(Ordering::Relaxed), 0);
    let grid = terminal.grid();
    let text: Vec<String> = grid
        .lines
        .iter()
        .map(|line| line.iter().map(|cell| cell.c).collect())
        .collect();
    assert!(
        text.iter().any(|line| line.starts_with("LAST-LINE")),
        "{text:?}"
    );
    assert_eq!(terminal.grid(), grid);
    assert_eq!(terminal.shared.builds.load(Ordering::Relaxed), 1);
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_grid_asked_for_while_the_reader_is_busy_is_built_by_the_reader() {
    let terminal = Terminal::spawn(
        sh("stty -echo; echo ready; read line; echo \"got $line\"; sleep 30"),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "ready");
    let held = lock(&terminal.shared.screen);
    let stale = terminal.grid();
    drop(held);
    // A report queues no edit, so only the reader's next chunk can build the grid asked for.
    terminal.report(b"go\n".to_vec()).unwrap();
    let built = |grid: &crate::Grid| {
        grid.lines.iter().any(|line| {
            line.iter()
                .map(|cell| cell.c)
                .collect::<String>()
                .starts_with("got go")
        })
    };
    assert!(!built(&stale));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !built(&lock(&terminal.shared.frame).grid) {
        assert!(Instant::now() < deadline, "the reader never built the grid");
        thread::sleep(Duration::from_millis(5));
    }
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn a_resize_shows_in_the_grid_at_once() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.resize(100, 30).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let grid = loop {
        let _ = terminal.take_changed();
        let grid = terminal.grid();
        if (grid.lines.len(), grid.lines[0].len()) == (30, 100) || Instant::now() > deadline {
            break grid;
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!((grid.lines.len(), grid.lines[0].len()), (30, 100));
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn the_title_the_program_sets_is_kept() {
    let terminal = Terminal::spawn(
        sh("printf '\\033]2;\\342\\240\\213 work\\007'; sleep 5"),
        80,
        24,
    )
    .expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(5);
    while terminal.title().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(terminal.title().as_deref(), Some("\u{280b} work"));
    quit_within(terminal, Duration::from_secs(3));
}

fn row_text(terminal: &Terminal, row: usize) -> String {
    let grid = terminal.grid();
    grid.lines[row]
        .iter()
        .map(|cell| cell.c)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[cfg(unix)]
#[test]
fn a_scroll_waits_for_a_held_screen_then_shows_and_typing_goes_back_to_the_bottom() {
    let terminal = Terminal::spawn(sh("seq 1 40; sleep 5"), 20, 4).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "40");
    assert_eq!(row_text(&terminal, 0), "38");
    let screen = Arc::clone(&terminal.shared.screen);
    let held = lock(&screen);
    let asked = Instant::now();
    terminal.scroll(10);
    assert!(
        asked.elapsed() < Duration::from_millis(500),
        "the scroll waited for the reader"
    );
    drop(held);
    assert!(terminal.take_changed());
    assert_eq!(row_text(&terminal, 0), "28");
    terminal.write(b"x".to_vec()).unwrap();
    assert!(terminal.take_changed());
    assert_eq!(row_text(&terminal, 0), "38");
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_selection_and_its_copy_go_through_the_queue_in_order() {
    let terminal = Terminal::spawn(sh("echo 'hello world'; sleep 5"), 40, 6).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "hello world");
    terminal.select(SelectMode::Word, 1, 0, false);
    terminal.select(SelectMode::Extend, 4, 0, true);
    assert_eq!(terminal.selection().as_deref(), Some("hello"));
    assert!(terminal.take_changed());
    assert_eq!(terminal.grid().selection, [(0, 0..5)]);
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_transcript_keeps_what_scrolled_off_the_screen() {
    let terminal = Terminal::spawn(sh("seq 1 40; sleep 5"), 20, 4).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "40");
    let text = terminal.transcript();
    assert!(text.starts_with("1\n2\n3\n"), "{text:?}");
    drop(reap);
}

#[test]
fn a_paste_keeps_its_newlines_as_the_program_reads_them() {
    assert_eq!(paste_text("a\nb\r\nc", false), "a\rb\rc");
    assert_eq!(paste_text("a\nb", true), "\x1b[200~a\nb\x1b[201~");
    assert_eq!(paste_text("a\tb\r", true), "\x1b[200~a\tb\r\x1b[201~");
}

#[test]
fn a_paste_cannot_end_its_own_bracket_early() {
    // Removing a marker would join what is left into a new one. Escape is gone, so none can form.
    let pasted = paste_text("\x1b[201\x1b[201~~cmd\r", true);
    assert_eq!(pasted, "\x1b[200~[201[201~~cmd\r\x1b[201~");
    assert_eq!(pasted.matches("\x1b[201~").count(), 1);
    assert_eq!(paste_text("\x1b[31mred", false), "[31mred");
}

#[test]
fn every_control_byte_is_kept_out_of_a_paste() {
    let unsafe_chars = (0_u8..=0x1f)
        .filter(|byte| !matches!(byte, b'\t' | b'\n' | b'\r'))
        .map(char::from)
        .chain(std::iter::once('\x7f'))
        .chain((0x80_u32..=0x9f).filter_map(char::from_u32));
    for c in unsafe_chars {
        let text = format!("a{c}b");
        assert_eq!(paste_text(&text, false), "ab", "{c:?}");
        assert_eq!(paste_text(&text, true), "\x1b[200~ab\x1b[201~", "{c:?}");
    }
}

#[test]
fn submit_refuses_control_only_text() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    for text in ["\r", "\n", "\t", "\r\n\t", "\u{1}"] {
        let error = terminal.submit(text).unwrap_err();
        assert!(
            error.to_string().contains("send needs text"),
            "{text:?}: {error}"
        );
    }
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn submit_refuses_an_exited_program_still_draining() {
    let (release, gate) = std::sync::mpsc::channel();
    let clock = Arc::new(AtomicU64::new(0));
    let terminal = Terminal::spawn(sh("printf done; exit 0"), 80, 24)
        .unwrap()
        .controlled(gate, Arc::clone(&clock));
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !terminal.exited() {
        assert!(Instant::now() < deadline, "the program never ended");
        thread::sleep(Duration::from_millis(20));
    }
    let error = terminal.submit("NOPE").unwrap_err();
    assert!(
        error.to_string().contains("the program has ended"),
        "{error}"
    );
    drop(release);
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn submit_reports_a_failed_write() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    terminal.fail_next_write();
    let (_, written) = terminal.submit("hello").unwrap();
    let error = written.recv().unwrap().unwrap_err();
    assert!(error.to_string().contains("write failed"), "{error}");
    quit_within(terminal, Duration::from_secs(3));
    drop(reap);
}

#[test]
fn a_report_queues_no_scroll_reset_and_typing_queues_one() {
    let terminal = Terminal::spawn(shell_launch(), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    // With the screen held, nothing applies: what is queued is what the calls asked for.
    let screen = Arc::clone(&terminal.shared.screen);
    let held = lock(&screen);
    terminal.scroll(10);
    terminal
        .report(vec![0x1b, b'[', b'M', 0x20, 0x21, 0x21])
        .unwrap();
    assert!(matches!(queued(&terminal).as_slice(), [Edit::Scroll(10)]));
    terminal.write(b"x".to_vec()).unwrap();
    assert!(matches!(
        queued(&terminal).as_slice(),
        [Edit::Scroll(10), Edit::Bottom]
    ));
    drop(held);
    drop(reap);
}

fn queued(terminal: &Terminal) -> Vec<Edit> {
    lock(&terminal.shared.edits).clone()
}

#[cfg(unix)]
#[test]
fn a_report_reaches_the_program_as_the_bytes_it_is_given() {
    let terminal = Terminal::spawn(
        sh("stty raw -echo; echo ready; head -c 6 | od -An -tx1; sleep 2"),
        40,
        6,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "ready");
    // Legacy column 96 is the byte 0x80, which must not turn into two bytes.
    terminal
        .report(vec![0x1b, b'[', b'M', 0x20, 0x80, 0x21])
        .unwrap();
    // od pads the bytes with two spaces on macOS and one on Linux.
    let screen = wait_for(&terminal, "21");
    assert!(
        screen
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(6)
            .any(|bytes| bytes == ["1b", "5b", "4d", "20", "80", "21"]),
        "{screen:?}"
    );
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_report_leaves_a_scrolled_view_where_it_is() {
    let terminal = Terminal::spawn(sh("seq 1 40; sleep 5"), 20, 4).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for(&terminal, "40");
    terminal.scroll(10);
    assert!(terminal.take_changed());
    assert_eq!(row_text(&terminal, 0), "28");
    terminal.report(b"\x1b[<35;1;1M".to_vec()).unwrap();
    wait_for(&terminal, "35;1;1M");
    let _ = terminal.take_changed();
    assert_eq!(row_text(&terminal, 0), "28");
    drop(reap);
}

#[test]
fn a_finished_process_needs_a_signalled_handle() {
    // 259 is STILL_ACTIVE: a running process reads it, and so can a program that exits with it.
    assert_eq!(finished_exit(false, 259), None);
    assert_eq!(finished_exit(false, 7), None);
    assert_eq!(finished_exit(true, 259), Some(Exit::Code(259)));
    assert_eq!(finished_exit(true, 0), Some(Exit::Code(0)));
}

#[cfg(unix)]
#[test]
fn a_finished_program_keeps_its_last_lines_within_the_scrollback() {
    let terminal = Terminal::spawn(
        sh("i=0; while [ $i -lt 12000 ]; do echo line $i; i=$((i+1)); done"),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(10);
    while terminal.exit().is_none() {
        assert!(Instant::now() < deadline, "the program never ended");
        thread::sleep(Duration::from_millis(20));
    }
    let finished = terminal.finish();
    let transcript = finished.transcript();
    assert!(transcript.contains("line 11999"), "{}", transcript.len());
    assert!(
        transcript.lines().count() <= 10_000 + 24,
        "{}",
        transcript.lines().count()
    );
    drop(reap);
}

#[cfg(unix)]
fn wait_for_exit(terminal: &Terminal) -> Duration {
    let started = Instant::now();
    while terminal.exit().is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the program never ended"
        );
        thread::sleep(Duration::from_millis(10));
    }
    started.elapsed()
}

#[cfg(unix)]
#[test]
fn a_program_that_exits_inside_an_unfinished_update_keeps_its_text() {
    let terminal =
        Terminal::spawn(sh("printf '\\033[?2026h'; printf HELD; exit 0"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    let finished = terminal.finish();
    assert!(finished.transcript().contains("HELD"));
    assert!(finished.screen().contains("HELD"));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn the_exit_is_reported_after_the_output_it_has_not_yet_read() {
    // The program writes far more than the pty holds, then exits. When the exit is reported, the
    // last line must already be on the screen: the reader has read to the end first.
    let terminal = Terminal::spawn(
        sh("i=0; while [ $i -lt 20000 ]; do echo line$i; i=$((i+1)); done; echo LAST; exit 0"),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    let finished = terminal.finish();
    assert!(finished.transcript().contains("LAST"));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn the_drain_cap_is_read_from_a_clock_the_test_controls() {
    // The reader withholds end of file, so the exit can only come from the drain cap.
    let clock = Arc::new(AtomicU64::new(0));
    let (release, gate) = mpsc::channel();
    let terminal = Terminal::spawn(sh("printf MARK; read x; exit 0"), 80, 24)
        .unwrap()
        .controlled(gate, Arc::clone(&clock));
    let reap = Reap(terminal.process_id());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !terminal.transcript().contains("MARK") {
        assert!(Instant::now() < deadline, "the program never printed");
        thread::sleep(Duration::from_millis(10));
    }
    terminal.write(b"x\n".to_vec()).unwrap();
    while !terminal.exited() {
        assert!(Instant::now() < deadline, "the program never ended");
        thread::sleep(Duration::from_millis(5));
    }
    let cap = u64::try_from(DRAIN.as_millis()).unwrap();
    assert_eq!(terminal.exit(), None);
    clock.store(cap - 1, Ordering::Release);
    assert_eq!(terminal.exit(), None);
    clock.store(cap, Ordering::Release);
    assert_eq!(terminal.exit(), Some(Exit::Code(0)));
    // Output the program wrote before its end is on the screen when the exit is reported.
    assert!(terminal.transcript().contains("MARK"));
    release.send(()).unwrap();
    let finished = terminal.finish();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.settled() {
        assert!(Instant::now() < deadline, "the output never settled");
        thread::sleep(Duration::from_millis(5));
    }
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_poll_that_sees_the_settle_also_sees_the_final_frame() {
    // The teardown publishes its final frame and then settles. Here it is held until the poll has
    // read the settlement, so the frame must still be reported by that poll or the next one.
    let terminal = Terminal::spawn(sh("printf FINAL; exit 0"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    let release = terminal.gate_publication();
    let _ = terminal.take_changed();
    let finished = terminal.finish();
    let _ = finished.take_changed();
    let deadline = Instant::now() + Duration::from_secs(10);
    let (changed, following) = finished.poll_between(|| {
        release.send(()).unwrap();
        while !finished.settled() {
            assert!(Instant::now() < deadline, "the teardown never settled");
            thread::sleep(Duration::from_millis(1));
        }
    });
    assert!(
        following,
        "the poll saw the settle before it read the frame"
    );
    assert!(changed, "the final frame was lost");
    assert_eq!(finished.poll(), (false, false));
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_snapshot_follows_the_drained_pty() {
    let terminal = Terminal::spawn(
        sh("i=0; while [ $i -lt 20000 ]; do echo line$i; i=$((i+1)); done; echo SNAP-LAST; exit 0"),
        80,
        24,
    )
    .unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    let release = terminal.gate_publication();
    let finished = terminal.finish();
    assert!(
        !finished.settled(),
        "the snapshot must wait until the reader has been joined"
    );
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.settled() {
        assert!(Instant::now() < deadline, "the output never settled");
        thread::sleep(Duration::from_millis(5));
    }
    let snap = finished.snapshot();
    let restored = Finished::restore(snap.cols, snap.rows, &snap.replay).expect("in bound");
    assert!(
        restored.transcript().contains("SNAP-LAST"),
        "{}",
        restored.transcript()
    );
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_finished_screen_does_not_keep_its_programs_mouse_reporting() {
    let terminal = Terminal::spawn(sh("printf '\\033[?1000h'; exit 0"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    assert!(terminal.grid().mouse.is_some());
    let finished = terminal.finish();
    assert!(finished.grid().mouse.is_none());
    drop(reap);
}

impl Terminal {
    /// Makes the reader withhold end of file until `gate` is used, and reads the drain cap from `clock`.
    pub(crate) fn controlled(
        mut self,
        gate: std::sync::mpsc::Receiver<()>,
        clock: Arc<AtomicU64>,
    ) -> Self {
        *lock(&self.shared.eof_gate) = Some(gate);
        self.clock = Some(clock);
        self
    }

    /// Makes the teardown wait before it publishes; the sender lets it go.
    pub(crate) fn gate_publication(&self) -> std::sync::mpsc::Sender<()> {
        let (release, gate) = std::sync::mpsc::channel();
        *lock(&self.shared.publish_gate) = Some(gate);
        release
    }
}
