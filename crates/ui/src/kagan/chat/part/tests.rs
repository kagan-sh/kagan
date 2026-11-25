use super::*;
use kg_domain::{EventKind, ThreadId};

fn event(seq: u64, kind: EventKind) -> Event {
    Event {
        thread_id: ThreadId("thr".into()),
        seq,
        at: Timestamp(i64::try_from(seq).unwrap() * 1000),
        kind,
    }
}

fn tool(seq: u64, kind: ToolKind, status: ToolStatus) -> Event {
    call(seq, &format!("call-{seq}"), kind, status, None)
}

fn call(
    seq: u64,
    id: &str,
    kind: ToolKind,
    status: ToolStatus,
    approval: Option<PendingInteractionId>,
) -> Event {
    event(
        seq,
        EventKind::Tool(ToolCall {
            id: id.into(),
            name: None,
            kind: Some(kind),
            title: Some("Read file".into()),
            status: Some(status),
            input: None,
            output: None,
            diff: Vec::new(),
            locations: Vec::new(),
            approval,
        }),
    )
}

#[test]
fn three_finished_reads_fold_into_one_bundle_and_an_edit_does_not() {
    let mut transcript = Transcript::default();
    for seq in 1..=3 {
        transcript.absorb(&tool(seq, ToolKind::Read, ToolStatus::Completed));
    }
    transcript.absorb(&tool(4, ToolKind::Edit, ToolStatus::Completed));
    assert_eq!(transcript.rows.len(), 2);
    let RowKind::Bundle(members) = &transcript.rows[0].kind else {
        panic!("a bundle");
    };
    assert_eq!(members.len(), 3);
    assert_eq!(transcript.rows[0].id, 1);
    assert!(matches!(transcript.rows[1].kind, RowKind::Work(_)));
}

#[test]
fn a_single_finished_read_stays_a_row() {
    let mut transcript = Transcript::default();
    transcript.absorb(&tool(1, ToolKind::Read, ToolStatus::Completed));
    assert!(matches!(transcript.rows[0].kind, RowKind::Work(_)));
}

#[test]
fn a_delta_after_the_approval_card_folds_into_its_row() {
    let mut transcript = Transcript::default();
    let approval = PendingInteractionId("pn_1".into());
    transcript.absorb(&call(1, "t1", ToolKind::Read, ToolStatus::InProgress, None));
    transcript.absorb(&call(
        2,
        "t1",
        ToolKind::Read,
        ToolStatus::Pending,
        Some(approval),
    ));
    transcript.absorb(&call(3, "t1", ToolKind::Read, ToolStatus::Completed, None));
    assert_eq!(transcript.rows.len(), 2);
    let RowKind::Work(tool) = &transcript.rows[0].kind else {
        panic!("the call stays one work row");
    };
    assert_eq!(tool.call.title.as_deref(), Some("Read file"));
    assert_eq!(tool.call.status, Some(ToolStatus::Completed));
    assert!(matches!(transcript.rows[1].kind, RowKind::Approval(_)));
}

#[test]
fn parallel_calls_keep_a_row_each_until_both_finish() {
    let mut transcript = Transcript::default();
    transcript.absorb(&call(1, "t1", ToolKind::Read, ToolStatus::InProgress, None));
    transcript.absorb(&call(2, "t2", ToolKind::Read, ToolStatus::InProgress, None));
    transcript.absorb(&call(3, "t1", ToolKind::Read, ToolStatus::Completed, None));
    assert_eq!(transcript.rows.len(), 2);
    let RowKind::Work(first) = &transcript.rows[0].kind else {
        panic!("the first call");
    };
    assert_eq!(first.call.status, Some(ToolStatus::Completed));
    let RowKind::Work(second) = &transcript.rows[1].kind else {
        panic!("the second call");
    };
    assert_eq!(second.call.status, Some(ToolStatus::InProgress));
}

#[test]
fn reads_that_finish_after_they_start_fold_into_one_bundle() {
    let mut transcript = Transcript::default();
    for seq in 1..=6u64 {
        let id = format!("t{}", seq.div_ceil(2));
        let status = if seq % 2 == 0 {
            ToolStatus::Completed
        } else {
            ToolStatus::InProgress
        };
        transcript.absorb(&call(seq, &id, ToolKind::Read, status, None));
    }
    assert_eq!(transcript.rows.len(), 1);
    let RowKind::Bundle(members) = &transcript.rows[0].kind else {
        panic!("one bundle");
    };
    assert_eq!(members.len(), 3);
}

#[test]
fn an_mcp_call_reported_as_a_read_stays_a_row() {
    let mut transcript = Transcript::default();
    for call in 1..=2u64 {
        let id = format!("t{call}");
        let first = (call - 1) * 2 + 1;
        for (seq, status) in [
            (first, ToolStatus::InProgress),
            (first + 1, ToolStatus::Completed),
        ] {
            transcript.absorb(&event(
                seq,
                EventKind::Tool(ToolCall {
                    id: id.clone(),
                    name: Some("mcp__github__create_issue".into()),
                    kind: Some(ToolKind::Read),
                    title: Some("Create an issue".into()),
                    status: Some(status),
                    input: Some(serde_json::json!({"title": "one"})),
                    output: Some("ok".into()),
                    diff: Vec::new(),
                    locations: Vec::new(),
                    approval: None,
                }),
            ));
        }
    }
    assert_eq!(transcript.rows.len(), 2, "neither call folds");
    assert!(
        transcript
            .rows
            .iter()
            .all(|row| matches!(row.kind, RowKind::Work(_)))
    );
}

#[test]
fn a_running_read_never_folds() {
    let mut transcript = Transcript::default();
    transcript.absorb(&tool(1, ToolKind::Read, ToolStatus::InProgress));
    transcript.absorb(&tool(2, ToolKind::Read, ToolStatus::Completed));
    assert_eq!(transcript.rows.len(), 2, "the running row is a row");
}

#[test]
fn reads_that_finish_out_of_order_still_fold_into_one_bundle() {
    let mut transcript = Transcript::default();
    transcript.absorb(&call(1, "t1", ToolKind::Read, ToolStatus::InProgress, None));
    transcript.absorb(&call(2, "t2", ToolKind::Read, ToolStatus::InProgress, None));
    transcript.absorb(&call(3, "t2", ToolKind::Read, ToolStatus::Completed, None));
    assert_eq!(transcript.rows.len(), 2, "nothing to join yet");
    transcript.absorb(&call(4, "t1", ToolKind::Read, ToolStatus::Completed, None));
    assert_eq!(transcript.rows.len(), 1, "the two reads are one bundle");
    let RowKind::Bundle(members) = &transcript.rows[0].kind else {
        panic!("one bundle");
    };
    assert_eq!(members.len(), 2);
    let ids: Vec<_> = members.iter().map(|member| member.id).collect();
    assert_eq!(
        ids,
        [1, 2],
        "the bundle keeps the order the reads started in"
    );
}

#[test]
fn a_diff_is_counted_once_as_it_arrives() {
    let mut transcript = Transcript::default();
    let diff = vec![ToolDiff {
        path: "a.ts".into(),
        old: Some("one\ntwo".into()),
        new: "one\nthree".into(),
    }];
    transcript.absorb(&event(
        1,
        EventKind::Tool(ToolCall {
            id: "t1".into(),
            name: None,
            kind: Some(ToolKind::Edit),
            title: Some("Edited file".into()),
            status: Some(ToolStatus::InProgress),
            input: Some(serde_json::json!({"file_path": "a.ts"})),
            output: None,
            diff: diff.clone(),
            locations: Vec::new(),
            approval: None,
        }),
    ));
    let RowKind::Work(tool) = &transcript.rows[0].kind else {
        panic!("the edit");
    };
    assert_eq!(tool.counts, (1, 1));
    transcript.absorb(&event(
        2,
        EventKind::Tool(ToolCall {
            id: "t1".into(),
            name: None,
            kind: None,
            title: None,
            status: Some(ToolStatus::Completed),
            input: None,
            output: Some("done".into()),
            diff: Vec::new(),
            locations: Vec::new(),
            approval: None,
        }),
    ));
    let RowKind::Work(tool) = &transcript.rows[0].kind else {
        panic!("the edit");
    };
    assert_eq!(tool.counts, (1, 1), "a later delta keeps the count it has");
}

#[test]
fn a_failure_that_ends_the_turn_stays_open() {
    let mut transcript = Transcript::default();
    transcript.absorb(&event(1, EventKind::TurnStarted));
    transcript.absorb(&call(2, "t1", ToolKind::Execute, ToolStatus::Failed, None));
    transcript.absorb(&event(
        3,
        EventKind::TurnEnded {
            outcome: TurnOutcome::Failed,
            error: None,
        },
    ));
    transcript.absorb(&event(
        4,
        EventKind::Notice {
            severity: kg_domain::NoticeSeverity::Info,
            text: "Start again".into(),
        },
    ));
    assert!(matches!(transcript.rows[1].kind, RowKind::Divider { .. }));
    assert!(matches!(transcript.rows[2].kind, RowKind::Notice(_)));
    assert!(transcript.auto_open(0, false));
}

#[test]
fn a_new_turn_clears_the_outcome_the_last_one_ended_with() {
    let mut transcript = Transcript::default();
    transcript.absorb(&event(1, EventKind::TurnStarted));
    transcript.absorb(&call(2, "t1", ToolKind::Execute, ToolStatus::Failed, None));
    transcript.absorb(&event(
        3,
        EventKind::TurnEnded {
            outcome: TurnOutcome::Failed,
            error: None,
        },
    ));
    transcript.absorb(&event(4, EventKind::TurnStarted));
    assert!(!transcript.auto_open(0, false));
}

#[test]
fn the_live_bundle_opens_while_the_turn_runs_and_closes_after_it() {
    let mut transcript = Transcript::default();
    transcript.absorb(&event(1, EventKind::TurnStarted));
    transcript.absorb(&tool(2, ToolKind::Read, ToolStatus::Completed));
    transcript.absorb(&tool(3, ToolKind::Read, ToolStatus::Completed));
    assert!(transcript.is_open(0, true));
    transcript.absorb(&event(
        4,
        EventKind::TurnEnded {
            outcome: TurnOutcome::Completed,
            error: None,
        },
    ));
    assert!(!transcript.is_open(0, false));
}

#[test]
fn a_live_thought_opens_only_while_the_turn_runs() {
    let mut transcript = Transcript::default();
    transcript.absorb(&event(1, EventKind::TurnStarted));
    transcript.absorb(&event(
        2,
        EventKind::Thought {
            text: "look".into(),
            message_id: None,
        },
    ));
    assert!(transcript.is_open(0, true));
    assert!(!transcript.is_open(0, false));
}

#[test]
fn a_person_collapsing_an_open_row_keeps_it_closed() {
    let mut transcript = Transcript::default();
    transcript.absorb(&event(1, EventKind::TurnStarted));
    transcript.absorb(&call(2, "t1", ToolKind::Execute, ToolStatus::Failed, None));
    transcript.absorb(&event(
        3,
        EventKind::TurnEnded {
            outcome: TurnOutcome::Failed,
            error: None,
        },
    ));
    assert!(transcript.is_open(0, false));
    let key = transcript.rows[0].key();
    assert_eq!(transcript.toggle(key, false), Some(0));
    assert!(!transcript.is_open(0, false));
    transcript.absorb(&event(
        4,
        EventKind::Notice {
            severity: kg_domain::NoticeSeverity::Info,
            text: "hi".into(),
        },
    ));
    assert!(!transcript.is_open(0, false), "the next event reopened it");
}

#[test]
fn a_plan_is_the_card_over_the_box_and_never_a_transcript_row() {
    let mut transcript = Transcript::default();
    transcript.absorb(&event(1, EventKind::TurnStarted));
    transcript.absorb(&event(
        2,
        EventKind::Plan {
            entries: vec![PlanEntry {
                text: "one".into(),
                status: kg_domain::PlanStatus::Pending,
            }],
        },
    ));
    transcript.absorb(&event(
        3,
        EventKind::Plan {
            entries: vec![
                PlanEntry {
                    text: "one".into(),
                    status: kg_domain::PlanStatus::Completed,
                },
                PlanEntry {
                    text: "two".into(),
                    status: kg_domain::PlanStatus::InProgress,
                },
            ],
        },
    ));
    assert!(
        transcript.rows.is_empty(),
        "the plan draws as the card over the box, never as a row"
    );
    let (id, entries) = transcript.plan().expect("the card's plan");
    assert_eq!(id, 2, "the first plan event keeps the card's id");
    assert_eq!(entries.len(), 2);
}

#[test]
fn an_old_event_is_not_folded_twice() {
    let mut transcript = Transcript::default();
    assert_eq!(
        transcript.absorb(&event(2, EventKind::Started)),
        Fold::Ignored
    );
    assert_eq!(
        transcript.absorb(&event(1, EventKind::Started)),
        Fold::Ignored
    );
    assert_eq!(
        transcript.absorb(&event(
            3,
            EventKind::Notice {
                severity: kg_domain::NoticeSeverity::Info,
                text: "hi".into(),
            }
        )),
        Fold::Dirty(0)
    );
    assert_eq!(
        transcript.absorb(&event(2, EventKind::Started)),
        Fold::Ignored
    );
}
