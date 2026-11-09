#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::sync::mpsc;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use super::super::exit::DRAIN;
use super::super::exit::finished_exit;
#[cfg(unix)]
use super::{Reap, sh};
use crate::Exit;
#[cfg(unix)]
use crate::{Finished, Terminal};

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
    let transcript = finished.view().transcript();
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
    assert!(finished.view().transcript().contains("HELD"));
    assert!(finished.view().screen().contains("HELD"));
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
    assert!(finished.view().transcript().contains("LAST"));
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
    while !terminal.view().transcript().contains("MARK") {
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
    assert!(terminal.view().transcript().contains("MARK"));
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
    let _ = terminal.view().take_changed();
    let finished = terminal.finish();
    let _ = finished.view().take_changed();
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

#[test]
fn a_replay_is_restored_up_to_four_mib_and_no_further() {
    const FOUR_MIB: usize = 4 * 1024 * 1024;
    assert!(Finished::restore(80, 24, &vec![b' '; FOUR_MIB]).is_some());
    assert!(Finished::restore(80, 24, &vec![b' '; FOUR_MIB + 1]).is_none());
}

#[test]
fn a_restored_screen_keeps_its_lines_and_its_tail() {
    let restored = Finished::restore(80, 24, b"one\r\ntwo\r\nthree\r\n").expect("restore");
    let view = restored.view();
    let kept = view.kept();
    assert!(kept.lines.iter().any(|line| line.text == "two"), "{kept:?}");
    let transcript = view.transcript();
    assert_eq!(view.tail(5), &transcript[transcript.len() - 5..]);
}

#[cfg(unix)]
#[test]
fn a_snapshot_bound_set_on_the_terminal_limits_the_snapshot() {
    let terminal = Terminal::spawn(sh("echo SNAP-BOUND; exit 0"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    terminal.set_snapshot_bound(1);
    let finished = terminal.finish();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.settled() {
        assert!(Instant::now() < deadline, "the output never settled");
        thread::sleep(Duration::from_millis(5));
    }
    let snap = finished.snapshot();
    assert!(!snap.screen_kept);
    assert!(snap.replay.is_empty());
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
        restored.view().transcript().contains("SNAP-LAST"),
        "{}",
        restored.view().transcript()
    );
    drop(reap);
}

#[cfg(unix)]
#[test]
fn a_finished_screen_does_not_keep_its_programs_mouse_reporting() {
    let terminal = Terminal::spawn(sh("printf '\\033[?1000h'; exit 0"), 80, 24).unwrap();
    let reap = Reap(terminal.process_id());
    wait_for_exit(&terminal);
    assert!(terminal.view().grid().mouse.is_some());
    let finished = terminal.finish();
    assert!(finished.view().grid().mouse.is_none());
    drop(reap);
}
