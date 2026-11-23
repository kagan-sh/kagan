//! The parts of the agent connection a test can reach without a process: a line read in chunks,
//! an exit code, the output a tool call repeats, and the frames the pump reads after the agent
//! stops writing.

use super::{LINE_MAX, Lines, Output, Repeats, exit_code};
use kg_terminal::Exit;

struct Bytes(std::io::Cursor<Vec<u8>>);

impl Output for Bytes {
    fn fill(&mut self, into: &mut [u8]) -> Option<usize> {
        use std::io::Read as _;
        match self.0.read(into) {
            Ok(0) | Err(_) => None,
            Ok(read) => Some(read),
        }
    }
}

fn lines(text: &str) -> Vec<String> {
    let mut lines = Lines::new(Bytes(std::io::Cursor::new(text.as_bytes().to_vec())));
    std::iter::from_fn(|| lines.next()).collect()
}

#[test]
fn a_status_above_i32_max_keeps_its_number() {
    assert_eq!(exit_code(-1_073_741_819), Exit::Code(0xC000_0005));
}

#[test]
fn a_line_longer_than_kagan_keeps_is_cut_and_the_next_one_still_arrives() {
    let long = "a".repeat(LINE_MAX * 2);
    let read = lines(&format!("{long}\nlast"));
    assert_eq!(read.len(), 2, "{read:?}");
    assert_eq!(read[0].len(), LINE_MAX);
    assert_eq!(read[1], "last", "the rest of the long line is not a line");
}

#[gpui_kit::test]
async fn a_stop_after_the_agent_spoke_still_reads_what_it_said(cx: &mut gpui_kit::TestAppContext) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};

    let (lines, read) = mpsc::sync_channel(2);
    let (frames, mut inbound) = futures::channel::mpsc::unbounded();
    lines.send("one".to_owned()).unwrap();
    lines.send("two".to_owned()).unwrap();
    drop(lines);
    let seen = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&seen);
    let connection = async move {
        use futures::StreamExt as _;
        while inbound.next().await.is_some() {
            counted.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    };
    super::pump(read, frames, connection, &cx.background_executor)
        .await
        .unwrap();
    assert_eq!(
        seen.load(Ordering::Relaxed),
        2,
        "the frames the agent wrote before its output ended are read"
    );
}

fn update(output: Option<&str>, status: Option<kg_domain::ToolStatus>) -> super::map::ToolUpdate {
    super::map::ToolUpdate {
        id: "t1".into(),
        name: None,
        kind: None,
        title: None,
        status,
        input: None,
        output: output.map(str::to_owned),
        diff: Vec::new(),
        locations: Vec::new(),
    }
}

#[test]
fn an_output_the_last_update_already_said_is_not_written_again() {
    let mut repeats = Repeats::default();
    assert!(!repeats.repeats(&update(Some("one"), None)));
    assert!(repeats.repeats(&update(Some("one"), None)));
    assert!(!repeats.repeats(&update(Some("two"), None)));
    assert!(
        !repeats.repeats(&update(Some("two"), Some(kg_domain::ToolStatus::Completed))),
        "an update that names a status is kept, whatever its output"
    );
}

#[test]
fn a_call_that_ended_keeps_no_output_in_the_connection() {
    let mut repeats = Repeats::default();
    assert!(!repeats.repeats(&update(Some("one"), None)));
    assert!(!repeats.repeats(&update(Some("one"), Some(kg_domain::ToolStatus::Completed))));
    assert!(repeats.0.is_empty(), "a finished call holds nothing");
    assert!(
        !repeats.repeats(&update(Some("one"), None)),
        "the output of a finished call is written again, not read as a repeat"
    );
}
