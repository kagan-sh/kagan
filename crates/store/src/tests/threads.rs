use kg_domain::{
    Conversation, EventKind, Order, PluginId, ProjectId, Thread, ThreadStatus, Timestamp,
};
use serde_json::json;

use super::{id, owned, sorted, store, thread};
use crate::{Error, Store};

fn ready_environment(store: &Store) -> kg_domain::Environment {
    let environment = kg_domain::Environment {
        id: kg_domain::EnvironmentId("env_1".into()),
        project_id: ProjectId("prj_1".into()),
        path: "/code/kagan".into(),
        status: kg_domain::EnvironmentStatus::Ready,
        provider: kg_domain::EnvironmentProvider::Project,
        branch: None,
        base_branch: None,
        retire_at: Some(Timestamp(99)),
        error: None,
    };
    store.put_environment(&environment).unwrap();
    environment
}

#[test]
fn a_created_thread_arrives_with_its_metadata_and_events() {
    let mut store = store();
    let environment = ready_environment(&store);
    let record = Thread {
        environment_id: Some(environment.id.clone()),
        ..thread("thr_1", 7)
    };
    let metadata = kg_domain::Metadata {
        plugin_id: PluginId("dev.test".into()),
        value: json!({"role": "worker"}),
    };
    let events = store
        .create_thread(
            &record,
            Some(&metadata),
            [EventKind::Created, EventKind::TurnStarted],
        )
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].at, Timestamp(7));
    assert_eq!(store.thread(&record.id).unwrap(), Some(record.clone()));
    assert_eq!(
        store
            .thread_metadata(&record.id, &metadata.plugin_id)
            .unwrap(),
        Some(metadata.value)
    );
    assert_eq!(
        store
            .environment(&environment.id)
            .unwrap()
            .unwrap()
            .retire_at,
        None
    );
}

#[test]
fn a_failed_creation_rolls_back_the_thread_metadata_events_and_retirement() {
    let mut store = store();
    let environment = ready_environment(&store);
    let record = Thread {
        environment_id: Some(environment.id.clone()),
        ..thread("thr_1", 7)
    };
    let metadata = kg_domain::Metadata {
        plugin_id: PluginId("dev.test".into()),
        value: json!({"role": "worker"}),
    };
    store
        .conn
        .execute_batch(
            "CREATE TRIGGER refuse_second_event BEFORE INSERT ON events WHEN NEW.seq = 2
         BEGIN SELECT RAISE(ABORT, 'event write failed'); END;",
        )
        .unwrap();
    assert!(
        store
            .create_thread(
                &record,
                Some(&metadata),
                [EventKind::Created, EventKind::TurnStarted]
            )
            .is_err()
    );
    assert!(store.thread(&record.id).unwrap().is_none());
    assert!(
        store
            .thread_metadata(&record.id, &metadata.plugin_id)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .events_after(&record.id, None, None, Order::Asc)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.environment(&environment.id).unwrap(),
        Some(environment)
    );
    store
        .conn
        .execute_batch("DROP TRIGGER refuse_second_event")
        .unwrap();
    let events = store
        .create_thread(&record, Some(&metadata), [EventKind::Created])
        .unwrap();
    assert_eq!(events[0].seq, 1);
}

#[test]
fn creation_refuses_an_environment_that_is_not_ready_or_in_the_project() {
    let mut store = store();
    let mut environment = ready_environment(&store);
    let record = Thread {
        environment_id: Some(environment.id.clone()),
        ..thread("thr_1", 7)
    };
    environment.status = kg_domain::EnvironmentStatus::Destroyed;
    store.put_environment(&environment).unwrap();
    assert!(
        store
            .create_thread(&record, None, [EventKind::Created])
            .is_err()
    );
    environment.status = kg_domain::EnvironmentStatus::Ready;
    store.put_environment(&environment).unwrap();
    let other = Thread {
        project_id: ProjectId("prj_other".into()),
        ..record.clone()
    };
    assert!(
        store
            .create_thread(&other, None, [EventKind::Created])
            .is_err()
    );
    let missing_owner = Thread {
        lifecycle_owner_thread_id: Some(id("missing")),
        ..record
    };
    assert!(
        store
            .create_thread(&missing_owner, None, [EventKind::Created])
            .is_err()
    );
    assert_eq!(
        store.environment(&environment.id).unwrap(),
        Some(environment)
    );
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
fn claim_conversation_waits_for_this_start_and_refuses_another_threads_id() {
    let mut store = store();
    let mut running = thread("thr_1", 1);
    running.status = ThreadStatus::Active;
    let mut other = thread("thr_2", 2);
    other.status = ThreadStatus::Active;
    store.put_thread(&running).unwrap();
    store.put_thread(&other).unwrap();
    store
        .append_events(&running.id, Timestamp(10), [EventKind::Started])
        .unwrap();
    store
        .append_events(&other.id, Timestamp(11), [EventKind::Started])
        .unwrap();
    let first = Conversation::fresh();
    let second = Conversation::fresh();
    assert!(
        !store
            .claim_conversation(&running.id, first, None, Timestamp(9))
            .unwrap()
    );
    store
        .set_thread_status(&running.id, ThreadStatus::Idle, Timestamp(12))
        .unwrap();
    assert!(
        !store
            .claim_conversation(&running.id, first, None, Timestamp(10))
            .unwrap()
    );
    store
        .set_thread_status(&running.id, ThreadStatus::Active, Timestamp(13))
        .unwrap();
    assert!(
        store
            .claim_conversation(&running.id, first, None, Timestamp(10))
            .unwrap()
    );
    assert_eq!(store.conversation(&running.id).unwrap(), Some(first));
    assert!(
        !store
            .claim_conversation(&running.id, second, None, Timestamp(10))
            .unwrap()
    );
    assert!(
        store
            .claim_conversation(&running.id, second, Some(first), Timestamp(10))
            .unwrap()
    );
    assert_eq!(store.conversation(&running.id).unwrap(), Some(second));
    assert!(
        !store
            .claim_conversation(&other.id, second, None, Timestamp(11))
            .unwrap()
    );
    assert_eq!(store.conversation(&other.id).unwrap(), None);
    let third = Conversation::fresh();
    assert!(
        store
            .claim_conversation(&other.id, third, None, Timestamp(11))
            .unwrap()
    );
    assert_eq!(store.conversation_ids().unwrap().len(), 2);
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

#[test]
fn owned_threads_reaches_every_lifecycle_descendant() {
    let store = store();
    store.put_thread(&thread("thr_parent", 1)).unwrap();
    store
        .put_thread(&owned("thr_child", "thr_parent", 2))
        .unwrap();
    store
        .put_thread(&owned("thr_grandchild", "thr_child", 3))
        .unwrap();
    store.put_thread(&thread("thr_other", 4)).unwrap();

    assert_eq!(
        sorted(store.owned_threads(&id("thr_parent")).unwrap()),
        sorted(vec![
            id("thr_parent"),
            id("thr_child"),
            id("thr_grandchild"),
        ])
    );
}
