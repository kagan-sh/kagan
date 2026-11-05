use kg_domain::{EventKind, Order, Role, Timestamp, TurnOutcome};
use serde_json::json;

use super::{id, store, thread};

#[test]
fn last_started_is_the_newest_started_event() {
    let mut store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    assert_eq!(store.last_started(&id("thr_1")).unwrap(), None);
    store
        .append_events(&id("thr_1"), Timestamp(5), [EventKind::Started])
        .unwrap();
    store
        .append_events(
            &id("thr_1"),
            Timestamp(9),
            [EventKind::Started, EventKind::Idle],
        )
        .unwrap();
    store
        .append_events(&id("thr_1"), Timestamp(12), [EventKind::Idle])
        .unwrap();
    assert_eq!(
        store.last_started(&id("thr_1")).unwrap(),
        Some(Timestamp(9))
    );
}
#[test]
fn events_number_per_thread_and_read_back() {
    let mut store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    store.put_thread(&thread("thr_2", 1)).unwrap();
    let first = store
        .append_events(
            &id("thr_1"),
            Timestamp(1),
            [
                EventKind::Created,
                EventKind::Message {
                    role: Role::User,
                    text: "hi".into(),
                    message_id: None,
                    attachments: Vec::new(),
                },
            ],
        )
        .unwrap();
    let second = store
        .append_events(
            &id("thr_1"),
            Timestamp(2),
            [EventKind::TurnEnded {
                outcome: TurnOutcome::Completed,
                error: None,
            }],
        )
        .unwrap();
    let other = store
        .append_events(&id("thr_2"), Timestamp(2), [EventKind::Created])
        .unwrap();
    assert_eq!(
        first
            .iter()
            .chain(&second)
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(other[0].seq, 1);

    let all = store
        .events_after(&id("thr_1"), None, None, Order::Asc)
        .unwrap();
    assert_eq!(all, [first, second.clone()].concat());
    assert_eq!(
        store
            .events_after(&id("thr_1"), None, Some(1), Order::Desc)
            .unwrap(),
        second
    );
    assert_eq!(
        store
            .events_after(&id("thr_1"), Some(1), None, Order::Asc)
            .unwrap(),
        all[1..]
    );
}

#[test]
fn events_are_append_only() {
    let mut store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    store
        .append_events(&id("thr_1"), Timestamp(1), [EventKind::Created])
        .unwrap();
    for sql in [
        "UPDATE events SET at = 2",
        "DELETE FROM events WHERE seq = 1",
    ] {
        assert!(store.conn.execute(sql, []).is_err(), "{sql}");
    }
    assert!(
        store
            .append_events(&id("thr_missing"), Timestamp(1), [EventKind::Created])
            .is_err()
    );
}
#[test]
fn rewind_keeps_the_log_append_only_and_never_resurrects_discarded_turns() {
    let mut store = store();
    let thread = thread("thr_1", 1);
    store.put_thread(&thread).unwrap();
    store
        .append_events(
            &thread.id,
            Timestamp(1),
            [
                EventKind::Created,
                EventKind::TurnStarted,
                EventKind::TurnEnded {
                    outcome: kg_domain::TurnOutcome::Completed,
                    error: None,
                },
                EventKind::TurnStarted,
            ],
        )
        .unwrap();
    store
        .append_events(
            &thread.id,
            Timestamp(2),
            [EventKind::Rewound { before_seq: 4 }],
        )
        .unwrap();
    store
        .append_events(&thread.id, Timestamp(3), [EventKind::TurnStarted])
        .unwrap();
    store
        .append_events(
            &thread.id,
            Timestamp(4),
            [EventKind::Rewound { before_seq: 2 }],
        )
        .unwrap();
    let events = store
        .events_after(&thread.id, None, None, kg_domain::Order::Asc)
        .unwrap();
    assert_eq!(
        events.iter().map(|event| event.seq).collect::<Vec<_>>(),
        [1, 7]
    );
    let raw: u64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(raw, 7);
    assert!(
        store
            .conn
            .execute("DELETE FROM events WHERE seq = 4", [])
            .is_err()
    );
}
#[test]
fn old_experiment_rows_and_the_predict_setting_still_load() {
    let mut store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    store.set_setting("predict", &json!(true)).unwrap();
    assert_eq!(store.setting("predict").unwrap(), Some(json!(true)));

    let predicted = EventKind::FilePredicted {
        path: "a.rs".into(),
        line: "renames it".into(),
    };
    store
        .append_events(&id("thr_1"), Timestamp(1), [predicted])
        .unwrap();
    let answered: EventKind = serde_json::from_value(json!({
        "type": "answered",
        "data": { "title": "Allow?", "option": "allow", "line": "because" }
    }))
    .unwrap();
    store
        .append_events(&id("thr_1"), Timestamp(2), [answered])
        .unwrap();

    let events = store
        .events_after(&id("thr_1"), None, None, Order::Asc)
        .unwrap();
    assert!(matches!(
        &events[0].kind,
        EventKind::FilePredicted { path, line } if path == "a.rs" && line == "renames it"
    ));
    assert_eq!(
        events[1].kind,
        EventKind::Answered {
            title: "Allow?".into(),
            option: "allow".into(),
            fields: std::collections::BTreeMap::new(),
        }
    );
}
