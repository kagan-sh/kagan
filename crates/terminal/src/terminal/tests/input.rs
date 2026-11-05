use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::thread;
use std::time::{Duration, Instant};

use crate::Terminal;
use crate::screen::Edit;

use super::super::shared::lock;
use super::super::stream::paste_text;
use super::frames::row_text;
use super::{Reap, quit_within, sh, shell_launch, wait_for};

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
