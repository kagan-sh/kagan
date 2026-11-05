use kg_domain::{
    Conversation, EventKind, Order, PluginId, ProjectId, Thread, ThreadStatus, Timestamp,
};
use serde_json::json;

use super::{id, owned, sorted, store, thread};
use crate::{Error, Store};

#[test]
fn a_created_thread_arrives_with_its_events() {
    let mut store = store();
    let events = store
        .create_thread(
            &thread("thr_1", 7),
            [EventKind::Created, EventKind::TurnStarted],
        )
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].at, Timestamp(7));
    assert!(store.thread(&id("thr_1")).unwrap().is_some());
    assert!(
        store
            .create_thread(&owned("thr_2", "missing", 8), [EventKind::Created])
            .is_err()
    );
    assert!(store.thread(&id("thr_2")).unwrap().is_none());
}

#[test]
fn put_thread_updates_in_place_and_keeps_history() {
    let mut store = store();
    let mut record = thread("thr_1", 1);
    store.put_thread(&record).unwrap();
    store
        .append_events(&record.id, Timestamp(2), [EventKind::Created])
        .unwrap();
    record.status = ThreadStatus::Active;
    record.title = "renamed".into();
    store.put_thread(&record).unwrap();
    assert_eq!(store.thread(&record.id).unwrap(), Some(record.clone()));
    assert_eq!(
        store
            .events_after(&record.id, None, None, Order::Asc)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn pinned_read_and_attention() {
    let store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    store.set_pinned(&id("thr_1"), true).unwrap();
    store.mark_read(&id("thr_1"), Timestamp(5)).unwrap();
    let read = store.thread(&id("thr_1")).unwrap().unwrap();
    assert!(read.pinned);
    assert!(!read.unread());
    store.set_attention(&id("thr_1"), Timestamp(6)).unwrap();
    assert!(store.thread(&id("thr_1")).unwrap().unwrap().unread());
    assert!(matches!(
        store.set_pinned(&id("thr_missing"), true),
        Err(Error::NotFound { what: "thread", .. })
    ));
}

#[test]
fn reopening_a_file_keeps_done() {
    let path = std::env::temp_dir().join(format!("kg-store-done-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let project = store()
        .project(&ProjectId("prj_1".into()))
        .unwrap()
        .unwrap();
    let store = Store::open(&path).unwrap();
    store.put_project(&project).unwrap();
    store.put_thread(&thread("thr_done", 1)).unwrap();
    store.mark_done(&id("thr_done")).unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.thread(&id("thr_done")).unwrap().unwrap().mark,
        kg_domain::ThreadMark::Done
    );
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn set_title_stores_the_title() {
    let store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    let before = store.thread(&id("thr_1")).unwrap().unwrap();
    store.set_title(&id("thr_1"), "Notes").unwrap();
    let after = store.thread(&id("thr_1")).unwrap().unwrap();
    assert_eq!(after.title, "Notes");
    assert!(after.title_locked);
    assert_eq!(after.provider, before.provider);
    assert_eq!(after.environment_id, before.environment_id);
    assert_eq!(after.updated_at, before.updated_at);
    assert!(matches!(
        store.set_title(&id("thr_missing"), "Notes"),
        Err(Error::NotFound { what: "thread", .. })
    ));
}

#[test]
fn a_thread_keeps_its_conversation() {
    let store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    assert_eq!(store.conversation(&id("thr_1")).unwrap(), None);
    let conversation = Conversation::fresh();
    store.set_conversation(&id("thr_1"), conversation).unwrap();
    assert_eq!(
        store.conversation(&id("thr_1")).unwrap(),
        Some(conversation)
    );
    assert!(matches!(
        store.conversation(&id("thr_missing")),
        Err(Error::NotFound { what: "thread", .. })
    ));
    assert!(matches!(
        store.set_conversation(&id("thr_missing"), conversation),
        Err(Error::NotFound { what: "thread", .. })
    ));
}
#[test]
fn a_locked_title_is_kept() {
    let store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    assert!(store.set_title_once(&id("thr_1"), "Once").unwrap());
    assert!(!store.set_title_once(&id("thr_1"), "Twice").unwrap());
    assert_eq!(store.thread(&id("thr_1")).unwrap().unwrap().title, "Once");
}
#[test]
fn threads_filter_and_page_newest_first() {
    let mut store = store();
    for (name, at) in [("thr_1", 1), ("thr_2", 2), ("thr_3", 3)] {
        store.put_thread(&thread(name, at)).unwrap();
    }
    store.archive_thread(&id("thr_2"), Timestamp(4)).unwrap();
    let ids = |threads: Vec<Thread>| threads.into_iter().map(|t| t.id).collect::<Vec<_>>();
    assert_eq!(
        ids(store.threads(None, None, None, None).unwrap()),
        [id("thr_3"), id("thr_2"), id("thr_1")]
    );
    assert_eq!(
        ids(store.threads(None, Some(false), None, None).unwrap()),
        [id("thr_3"), id("thr_1")]
    );
    assert_eq!(
        ids(store.threads(None, Some(true), None, None).unwrap()),
        [id("thr_2")]
    );
    assert_eq!(
        ids(store
            .threads(None, None, Some(1), Some(Timestamp(3)))
            .unwrap()),
        [id("thr_2")]
    );
    assert!(
        store
            .threads(Some(&ProjectId("prj_2".into())), None, None, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn archive_takes_lifecycle_dependents_and_unarchive_does_not() {
    let mut store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    store.put_thread(&owned("thr_2", "thr_1", 2)).unwrap();
    store.put_thread(&owned("thr_3", "thr_2", 3)).unwrap();
    store
        .put_thread(&Thread {
            parent_thread_id: Some(id("thr_1")),
            ..thread("thr_4", 4)
        })
        .unwrap();
    let archived = store.archive_thread(&id("thr_1"), Timestamp(9)).unwrap();
    assert!(
        archived
            .iter()
            .all(|event| event.kind == EventKind::Archived)
    );
    assert_eq!(
        sorted(archived.into_iter().map(|event| event.thread_id).collect()),
        [id("thr_1"), id("thr_2"), id("thr_3")]
    );
    let last = |store: &Store, name| {
        store
            .events_after(&id(name), None, None, Order::Asc)
            .unwrap()
            .pop()
            .map(|event| event.kind)
    };
    assert_eq!(last(&store, "thr_3"), Some(EventKind::Archived));
    assert!(
        store
            .archive_thread(&id("thr_1"), Timestamp(10))
            .unwrap()
            .is_empty()
    );
    let archived_at = |name| store.thread(&id(name)).unwrap().unwrap().archived_at;
    assert_eq!(archived_at("thr_3"), Some(Timestamp(9)));
    assert_eq!(archived_at("thr_4"), None);
    assert_eq!(
        store.unarchive_thread(&id("thr_1"), Timestamp(11)).unwrap()[0].kind,
        EventKind::Unarchived
    );
    assert!(
        store
            .unarchive_thread(&id("thr_1"), Timestamp(12))
            .unwrap()
            .is_empty()
    );
    assert_eq!(last(&store, "thr_1"), Some(EventKind::Unarchived));
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().archived_at,
        None
    );
    assert_eq!(
        store.thread(&id("thr_2")).unwrap().unwrap().archived_at,
        Some(Timestamp(9))
    );
    assert!(matches!(
        store.archive_thread(&id("thr_missing"), Timestamp(1)),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn delete_removes_dependents_and_history() {
    let mut store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    store.put_thread(&owned("thr_2", "thr_1", 2)).unwrap();
    store
        .put_thread(&Thread {
            parent_thread_id: Some(id("thr_1")),
            ..thread("thr_3", 3)
        })
        .unwrap();
    store
        .append_events(&id("thr_2"), Timestamp(2), [EventKind::Created])
        .unwrap();
    store
        .conn
        .execute(
            "INSERT INTO pending_interactions (id, thread_id, title, options, created_at) VALUES ('q1', 'thr_2', 'Allow', '[]', 2)",
            [],
        )
        .unwrap();
    assert_eq!(
        sorted(
            store
                .delete_thread(&id("thr_1"), Timestamp(3), &[])
                .unwrap()
                .into_iter()
                .map(|(event, _)| event.thread_id)
                .collect()
        ),
        [id("thr_1"), id("thr_2")]
    );
    assert_eq!(store.thread(&id("thr_2")).unwrap(), None);
    assert!(
        store
            .events_after(&id("thr_2"), None, None, Order::Asc)
            .unwrap()
            .is_empty()
    );
    let pending: i64 = store
        .conn
        .query_row(
            "SELECT COUNT(*) FROM pending_interactions WHERE thread_id = 'thr_2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0);
    assert_eq!(
        store
            .thread(&id("thr_3"))
            .unwrap()
            .unwrap()
            .parent_thread_id,
        None
    );
}
#[test]
fn only_a_settled_turn_or_a_failure_needs_the_person() {
    let store = store();
    store.put_thread(&thread("thr_1", 0)).unwrap();
    let thread_id = id("thr_1");
    store.mark_read(&thread_id, Timestamp(1)).unwrap();
    let unread = |store: &Store| store.thread(&thread_id).unwrap().unwrap().unread();
    for (status, at) in [
        (ThreadStatus::Starting, 2),
        (ThreadStatus::Idle, 3),
        (ThreadStatus::Active, 4),
        (ThreadStatus::Stopping, 5),
        (ThreadStatus::Idle, 6),
    ] {
        store
            .set_thread_status(&thread_id, status, Timestamp(at))
            .unwrap();
    }
    assert!(!unread(&store), "starting, stopping and stop are quiet");
    store
        .set_thread_status(&thread_id, ThreadStatus::Active, Timestamp(7))
        .unwrap();
    store
        .set_thread_status(&thread_id, ThreadStatus::Idle, Timestamp(8))
        .unwrap();
    assert!(
        !unread(&store),
        "a turn that ends without attention is quiet"
    );
    store.set_attention(&thread_id, Timestamp(10)).unwrap();
    assert!(unread(&store));
    store.mark_read(&thread_id, Timestamp(11)).unwrap();
    store
        .set_thread_status(&thread_id, ThreadStatus::Starting, Timestamp(12))
        .unwrap();
    store
        .set_thread_status(&thread_id, ThreadStatus::Error, Timestamp(13))
        .unwrap();
    assert!(unread(&store), "a failed start needs the person");
}

#[test]
fn plugin_thread_metadata_is_separate_and_deleted_with_its_thread() {
    let store = store();
    let thread = thread("thr_1", 0);
    let one = PluginId("one".into());
    let two = PluginId("two".into());
    store.put_thread(&thread).unwrap();
    store
        .set_thread_metadata(&thread.id, &one, &json!({"role":"master"}))
        .unwrap();
    store
        .set_thread_metadata(&thread.id, &two, &json!({"role":"child"}))
        .unwrap();
    assert_eq!(
        store.thread_metadata(&thread.id, &one).unwrap(),
        Some(json!({"role":"master"}))
    );
    assert_eq!(
        store.thread_metadata(&thread.id, &two).unwrap(),
        Some(json!({"role":"child"}))
    );
    store.delete_thread(&thread.id, Timestamp(1), &[]).unwrap();
    assert_eq!(store.thread_metadata(&thread.id, &one).unwrap(), None);
}
