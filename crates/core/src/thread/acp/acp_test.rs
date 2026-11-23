//! The transcript a burst of the agent's chunks makes, without a process behind it.

use super::{AcpEvent, joined};

/// A burst of one message's chunks is written once, in order, so the log holds one row and a
/// transcript reads as one message.
#[test]
fn a_burst_of_one_message_becomes_one_write() {
    let (events, received) = async_channel::unbounded();
    for text in ["two ", "three"] {
        events
            .try_send(AcpEvent::Message {
                text: text.into(),
                message_id: Some("msg_1".into()),
            })
            .unwrap();
    }
    events
        .try_send(AcpEvent::Message {
            text: "later".into(),
            message_id: Some("msg_2".into()),
        })
        .unwrap();
    let mut pending = None;
    let text = joined("one ".into(), Some("msg_1"), &received, &mut pending);
    assert_eq!(text, "one two three", "the burst is one row, in order");
    assert!(
        matches!(pending, Some(AcpEvent::Message { .. })),
        "another message waits for the next step"
    );
    assert!(received.try_recv().is_err(), "nothing is left behind");
}
