#[cfg(unix)]
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use crate::SelectMode;
use crate::Terminal;
use crate::screen::Screen;

use super::super::shared::{Frame, lock, store_frame};
use super::{Reap, quit_within, shell_launch, within};
#[cfg(unix)]
use super::{sh, wait_for};

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

#[cfg(unix)]
pub(super) fn row_text(terminal: &Terminal, row: usize) -> String {
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
