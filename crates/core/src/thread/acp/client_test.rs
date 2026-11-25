//! What the connection keeps behind its events: the output a tool call repeats, and an exit code.

use super::{Repeats, exit_code};
use kg_terminal::Exit;

#[test]
fn a_status_above_i32_max_keeps_its_number() {
    assert_eq!(exit_code(-1_073_741_819), Exit::Code(0xC000_0005));
}

fn update(output: Option<&str>, status: Option<kg_domain::ToolStatus>) -> kg_domain::ToolCall {
    kg_domain::ToolCall {
        id: "t1".into(),
        name: None,
        kind: None,
        title: None,
        status,
        input: None,
        output: output.map(str::to_owned),
        diff: Vec::new(),
        locations: Vec::new(),
        approval: None,
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
