//! The agent's lines as they arrive, and the frames the pump reads once the agent stops writing.

use super::{LINE_MAX, Lines, Output, pump};

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

#[test]
fn a_line_longer_than_kagan_keeps_is_cut_and_the_next_one_still_arrives() {
    let long = "a".repeat(LINE_MAX * 2);
    let mut lines = Lines::new(Bytes(std::io::Cursor::new(
        format!("{long}\nlast").into_bytes(),
    )));
    let first = lines.next().unwrap();
    assert_eq!(first.len(), LINE_MAX);
    assert!(lines.noted, "the person is told the line was cut");
    assert_eq!(
        lines.next().as_deref(),
        Some("last"),
        "the rest of the long line is not a line"
    );
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
    pump(read, frames, connection, &cx.background_executor)
        .await
        .unwrap();
    assert_eq!(
        seen.load(Ordering::Relaxed),
        2,
        "the frames the agent wrote before its output ended are read"
    );
}
