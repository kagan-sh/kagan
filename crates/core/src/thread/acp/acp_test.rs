//! The transcript a burst of the agent's chunks makes, without a process behind it.

use kg_domain::{ThreadOption, ThreadOptionValue};

use super::{AcpEvent, joined, kept};

fn option(id: &str, value: &str, values: &[&str]) -> ThreadOption {
    ThreadOption {
        id: id.into(),
        name: id.into(),
        category: None,
        description: None,
        value: value.into(),
        values: values
            .iter()
            .map(|value| ThreadOptionValue {
                value: (*value).into(),
                name: (*value).into(),
                description: None,
            })
            .collect(),
    }
}

#[test]
fn a_start_again_asks_only_for_a_value_that_differs_and_is_still_offered() {
    let remembered = [option("mode", "plan", &["default", "plan"])];
    let differs = [option("mode", "default", &["default", "plan"])];
    assert_eq!(
        kept(&remembered, &differs),
        vec![("mode".to_owned(), "plan".to_owned())]
    );

    let same = [option("mode", "plan", &["default", "plan"])];
    assert!(kept(&remembered, &same).is_empty(), "the value stands");

    let gone = [option("mode", "default", &["default"])];
    assert!(
        kept(&remembered, &gone).is_empty(),
        "an agent that dropped the value is not asked for it"
    );

    let no_mode = [option("model", "opus", &["opus"])];
    assert!(
        kept(&remembered, &no_mode).is_empty(),
        "an option the agent no longer offers is left alone"
    );
}

#[test]
fn a_burst_of_one_message_becomes_one_write() {
    let (events, received) = async_channel::unbounded();
    for text in ["two ", "three"] {
        events
            .try_send(AcpEvent::Chunk {
                thought: false,
                text: text.into(),
                message_id: Some("msg_1".into()),
            })
            .unwrap();
    }
    events
        .try_send(AcpEvent::Chunk {
            thought: false,
            text: "later".into(),
            message_id: Some("msg_2".into()),
        })
        .unwrap();
    let mut pending = None;
    let text = joined(false, "one ".into(), Some("msg_1"), &received, &mut pending);
    assert_eq!(text, "one two three", "the burst is one row, in order");
    assert!(
        matches!(&pending, Some(AcpEvent::Chunk { text, .. }) if text == "later"),
        "another message waits for the next step"
    );
    assert!(received.try_recv().is_err(), "nothing is left behind");
}
