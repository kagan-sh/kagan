use kg_domain::{
    Conversation, Environment, EnvironmentId, EnvironmentStatus, EventKind, InteractionOption,
    Order, PendingInteraction, PendingInteractionId, Plugin, PluginId, Project, ProjectId,
    ProjectKind, Role, Section, SectionId, Thread, ThreadId, ThreadStatus, Timestamp, TurnOutcome,
    Visibility,
};
use serde_json::json;

use crate::{Changes, Error, PluginDb, ScreenRow, Statement, Store};

fn store() -> Store {
    let store = Store::open(":memory:").unwrap();
    store
        .put_project(&Project {
            id: ProjectId("prj_1".into()),
            kind: ProjectKind::Standard,
            name: "kagan".into(),
            path: "/code/kagan".into(),
            git_remote_url: Some("https://github.com/example/repo".into()),
        })
        .unwrap();
    store
}

fn thread(id: &str, created_at: i64) -> Thread {
    Thread {
        id: ThreadId(id.into()),
        project_id: ProjectId("prj_1".into()),
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: "claude-code".into(),
        model: "opus".into(),
        reasoning_level: "high".into(),
        status: ThreadStatus::Idle,
        visibility: Visibility::Visible,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned: false,
        last_read_at: None,
        latest_attention_at: Timestamp(created_at),
        mark: kg_domain::ThreadMark::None,
        archived_at: None,
        created_at: Timestamp(created_at),
        updated_at: Timestamp(created_at),
    }
}

fn owned(id: &str, owner: &str, created_at: i64) -> Thread {
    Thread {
        lifecycle_owner_thread_id: Some(ThreadId(owner.into())),
        parent_thread_id: Some(ThreadId(owner.into())),
        ..thread(id, created_at)
    }
}

fn id(id: &str) -> ThreadId {
    ThreadId(id.into())
}

fn sorted(mut ids: Vec<ThreadId>) -> Vec<ThreadId> {
    ids.sort();
    ids
}

#[test]
fn project_and_environment_round_trip() {
    let store = store();
    let environment = Environment {
        id: EnvironmentId("env_1".into()),
        project_id: ProjectId("prj_1".into()),
        path: "/code/kagan-w2".into(),
        status: EnvironmentStatus::Ready,
        provider: kg_domain::EnvironmentProvider::Worktree,
        branch: Some("w2-store".into()),
        base_branch: Some("build".into()),
        retire_at: Some(Timestamp(9)),
        error: None,
    };
    store.put_environment(&environment).unwrap();
    assert_eq!(
        store.environment(&environment.id).unwrap(),
        Some(environment.clone())
    );
    assert_eq!(
        store.environments(&environment.project_id).unwrap(),
        [environment]
    );
    assert_eq!(store.projects().unwrap()[0].name, "kagan");
    assert_eq!(store.project(&ProjectId("prj_2".into())).unwrap(), None);
}

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
fn a_schema_one_database_gains_the_conversation_and_keeps_its_threads() {
    let path = std::env::temp_dir().join(format!("kg-store-v1-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    crate::migrate(&mut conn, crate::store::MIGRATIONS[..1].iter().copied()).unwrap();
    let store = Store { conn };
    store
        .put_project(&Project {
            id: ProjectId("prj_1".into()),
            kind: ProjectKind::Standard,
            name: "kagan".into(),
            path: "/code/kagan".into(),
            git_remote_url: None,
        })
        .unwrap();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 2);
    assert!(store.thread(&id("thr_1")).unwrap().is_some());
    assert_eq!(store.conversation(&id("thr_1")).unwrap(), None);
    drop(store);
    std::fs::remove_file(&path).unwrap();
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
fn sections_file_threads() {
    let store = store();
    let section = Section {
        id: SectionId("sec_1".into()),
        name: "Reviews".into(),
    };
    store.put_section(&section).unwrap();
    store
        .put_thread(&Thread {
            section_id: Some(section.id.clone()),
            ..thread("thr_1", 1)
        })
        .unwrap();
    assert_eq!(store.sections().unwrap(), std::slice::from_ref(&section));
    store.set_section(&id("thr_1"), None).unwrap();
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().section_id,
        None
    );
    store.set_section(&id("thr_1"), Some(&section.id)).unwrap();
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().section_id,
        Some(section.id.clone())
    );
    store.delete_section(&section.id).unwrap();
    assert!(store.sections().unwrap().is_empty());
    assert_eq!(
        store.thread(&id("thr_1")).unwrap().unwrap().section_id,
        None
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
fn pending_interactions() {
    let store = store();
    store.put_thread(&thread("thr_1", 1)).unwrap();
    let interaction = PendingInteraction {
        id: PendingInteractionId("pi_1".into()),
        thread_id: id("thr_1"),
        title: "Allow edit of src/lib.rs?".into(),
        options: vec![
            InteractionOption {
                id: "allow".into(),
                label: "Allow".into(),
            },
            InteractionOption {
                id: "reject".into(),
                label: "Reject".into(),
            },
        ],
        created_at: Timestamp(3),
    };
    store.put_pending_interaction(&interaction).unwrap();
    assert_eq!(
        store.pending_interactions(&id("thr_1")).unwrap(),
        std::slice::from_ref(&interaction)
    );
    store.remove_pending_interaction(&interaction.id).unwrap();
    assert!(store.pending_interactions(&id("thr_1")).unwrap().is_empty());
}

#[test]
fn plugin_values() {
    let store = store();
    let plugin = PluginId("dev.kagan.example".into());
    assert_eq!(store.plugin_value(&plugin, "label").unwrap(), None);
    store
        .set_plugin_value(&plugin, "label", &json!("one"))
        .unwrap();
    store
        .set_plugin_value(&plugin, "label", &json!("two"))
        .unwrap();
    assert_eq!(
        store.plugin_value(&plugin, "label").unwrap(),
        Some(json!("two"))
    );
}

#[test]
fn plugin_enabled_and_commit_update_the_same_row() {
    let store = store();
    let mut plugin = Plugin {
        id: PluginId("dev.kagan.canvas".into()),
        root: "/plugins/canvas".into(),
        source: "git:https://example.com/p.git@v1".into(),
        commit: Some("abc".into()),
        enabled: true,
        approved: json!({ "powers": { "serve": false } }),
        error: None,
    };
    store.insert_plugin(&plugin).unwrap();
    plugin.enabled = false;
    plugin.commit = None;
    store
        .set_plugin_enabled(&plugin.id, plugin.enabled)
        .unwrap();
    store
        .keep_plugin_checkout(
            &plugin.id,
            &plugin.root,
            &plugin.source,
            &plugin.commit,
            &plugin.approved,
        )
        .unwrap();
    assert_eq!(store.plugins().unwrap(), vec![plugin]);
}

#[test]
fn reopening_a_file_keeps_data() {
    let dir = std::env::temp_dir().join(format!("kg-store-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("kagan.db");
    Store::open(&path)
        .unwrap()
        .put_section(&Section {
            id: SectionId("sec_1".into()),
            name: "Kept".into(),
        })
        .unwrap();
    assert_eq!(
        Store::open(&path).unwrap().sections().unwrap()[0].name,
        "Kept"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plugin_db_migrates_once_and_round_trips_values() {
    let mut db = PluginDb::open(":memory:").unwrap();
    let mut migrations = vec![
        "CREATE TABLE docs (id INTEGER PRIMARY KEY, name TEXT, points REAL, data TEXT)".to_owned(),
    ];
    db.migrate(&migrations).unwrap();
    migrations.push("ALTER TABLE docs ADD COLUMN flag INTEGER".into());
    db.migrate(&migrations).unwrap();
    db.migrate(&migrations).unwrap();

    let changes = db
        .run(
            "INSERT INTO docs (name, points, data, flag) VALUES (?, ?, ?, ?)",
            &[json!("a"), json!(1.5), json!({ "x": 1 }), json!(true)],
        )
        .unwrap();
    assert_eq!(
        changes,
        Changes {
            changes: 1,
            last_id: 1
        }
    );
    assert_eq!(
        db.query("SELECT * FROM docs WHERE name = ?", &[json!("a")])
            .unwrap(),
        [
            json!({ "id": 1, "name": "a", "points": 1.5, "data": "{\"x\":1}", "flag": 1 })
                .as_object()
                .unwrap()
                .clone()
        ]
    );
}

#[test]
fn plugin_db_transaction_is_all_or_nothing_with_foreign_keys() {
    let mut db = PluginDb::open(":memory:").unwrap();
    db.migrate(&[
        "CREATE TABLE tickets (id INTEGER PRIMARY KEY);
         CREATE TABLE comments (ticket_id INTEGER NOT NULL REFERENCES tickets(id) ON DELETE CASCADE);"
            .to_owned(),
    ])
    .unwrap();
    let statement = |sql: &str, params: Vec<serde_json::Value>| Statement {
        sql: sql.into(),
        params,
    };
    assert!(
        db.transaction(&[
            statement("INSERT INTO tickets (id) VALUES (?)", vec![json!(1)]),
            statement(
                "INSERT INTO comments (ticket_id) VALUES (?)",
                vec![json!(2)]
            ),
        ])
        .is_err()
    );
    assert!(db.query("SELECT * FROM tickets", &[]).unwrap().is_empty());

    db.transaction(&[
        statement("INSERT INTO tickets (id) VALUES (?)", vec![json!(1)]),
        statement(
            "INSERT INTO comments (ticket_id) VALUES (?)",
            vec![json!(1)],
        ),
    ])
    .unwrap();
    db.run("DELETE FROM tickets", &[]).unwrap();
    assert!(db.query("SELECT * FROM comments", &[]).unwrap().is_empty());
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
    let mut store = store();
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
        }
    );
}

fn screen_row(id: &ThreadId, seq: u64, replay: &[u8]) -> ScreenRow {
    ScreenRow {
        thread_id: id.clone(),
        started_seq: seq,
        cols: 80,
        rows: 24,
        replay: replay.to_vec(),
        history_dropped: 0,
        screen_kept: true,
    }
}

#[test]
fn a_delayed_screen_cannot_replace_a_newer_run() {
    let mut store = store();
    let id = ThreadId("thr_screen".into());
    store.put_thread(&thread("thr_screen", 1)).unwrap();
    let first = store
        .append_events(&id, Timestamp(1), [EventKind::Started])
        .unwrap();
    assert!(
        store
            .put_screen(&screen_row(&id, first[0].seq, b"one"))
            .unwrap()
    );
    let second = store
        .append_events(&id, Timestamp(2), [EventKind::Started])
        .unwrap();
    store.delete_screen(&id).unwrap();
    assert!(
        !store
            .put_screen(&screen_row(&id, first[0].seq, b"stale"))
            .unwrap()
    );
    assert!(store.screens().unwrap().is_empty());
    assert!(
        store
            .put_screen(&screen_row(&id, second[0].seq, b"new"))
            .unwrap()
    );
    assert!(
        !store
            .put_screen(&screen_row(&id, first[0].seq, b"stale"))
            .unwrap()
    );
    let saved = store.screens().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].replay, b"new");
    assert_eq!(saved[0].started_seq, second[0].seq);
}

#[test]
fn a_screen_outside_the_bound_is_not_returned() {
    let mut store = store();
    let id = ThreadId("thr_bound".into());
    store.put_thread(&thread("thr_bound", 1)).unwrap();
    let started = store
        .append_events(&id, Timestamp(1), [EventKind::Started])
        .unwrap();
    let mut huge = screen_row(&id, started[0].seq, b"");
    huge.cols = 1001;
    assert!(matches!(store.put_screen(&huge), Err(Error::Conflict(_))));
    store
        .conn
        .execute(
            "INSERT INTO terminal_screens \
                (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
             VALUES ('thr_bound', ?1, 9999, 24, X'ABCD', 7, 1)",
            [started[0].seq],
        )
        .unwrap();
    let wide = store.screens().unwrap();
    assert_eq!(wide.len(), 1);
    assert!(!wide[0].screen_kept);
    assert!(wide[0].replay.is_empty());
    assert_eq!(wide[0].history_dropped, 0);
    assert!((2..=1000).contains(&wide[0].cols));
    let over = vec![0u8; 4 * 1024 * 1024 + 1];
    let tall = ThreadId("thr_tall".into());
    store.put_thread(&thread("thr_tall", 2)).unwrap();
    let started = store
        .append_events(&tall, Timestamp(1), [EventKind::Started])
        .unwrap();
    store
        .conn
        .execute(
            "INSERT INTO terminal_screens \
                (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
             VALUES ('thr_tall', ?1, 80, 0, ?2, 4, 1)",
            rusqlite::params![started[0].seq, over],
        )
        .unwrap();
    let rows = store.screens().unwrap();
    let tall = rows
        .iter()
        .find(|row| row.thread_id.0 == "thr_tall")
        .unwrap();
    assert!(!tall.screen_kept);
    assert!(tall.replay.is_empty(), "{}", tall.replay.len());
    assert_eq!(tall.history_dropped, 0);
    assert!((1..=500).contains(&tall.rows));
}

#[test]
fn a_future_schema_is_refused_before_anything_is_written() {
    let dir = std::env::temp_dir().join(format!("kg-store-future-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("kagan.sqlite3");
    let side = dir.join("notes.txt");
    std::fs::write(&side, "keep").unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE canary (id INTEGER); INSERT INTO canary VALUES (1);")
        .unwrap();
    conn.pragma_update(None, "application_id", crate::store::APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 999i64).unwrap();
    drop(conn);
    let Err(error) = Store::open(&path) else {
        panic!("a newer schema was opened");
    };
    let Error::Future { found, understood } = error else {
        panic!("{error}");
    };
    assert_eq!(found, 999);
    assert!(understood < found);
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 999);
    let journal: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "delete");
    let canary: i64 = conn
        .query_row("SELECT id FROM canary", [], |row| row.get(0))
        .unwrap();
    assert_eq!(canary, 1);
    assert_eq!(std::fs::read_to_string(&side).unwrap(), "keep");
    assert!(!dir.join("kagan.sqlite3-wal").exists());
    drop(conn);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_new_database_is_schema_two_and_marked_as_kagan() {
    let store = store();
    assert_eq!(store.schema_version().unwrap(), 2);
    let application: i32 = store
        .conn
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .unwrap();
    assert_eq!(application, crate::store::APPLICATION_ID);
}

#[test]
fn a_pre_release_database_is_refused_with_its_path_and_left_alone() {
    let dir = std::env::temp_dir().join(format!("kg-store-pre-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for version in [1i64, 14] {
        let path = dir.join(format!("v{version}.sqlite3"));
        let conn = rusqlite::Connection::open(&path).unwrap();
        if version == 14 {
            conn.pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()))
                .unwrap();
        }
        conn.execute_batch("CREATE TABLE threads (id TEXT); INSERT INTO threads VALUES ('thr_1');")
            .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        let Err(error) = Store::open(&path) else {
            panic!("a pre-release schema {version} was opened");
        };
        assert!(matches!(error, Error::PreRelease { .. }), "{error}");
        assert_eq!(
            error.to_string(),
            format!(
                "{} was made by a pre-release build of kagan and cannot be opened. Nothing was changed. Move it, and any -wal or -shm file beside it, out of that folder, then start kagan again for a new database",
                path.display()
            )
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn another_file_is_refused_unchanged_and_an_empty_one_is_built() {
    let dir = std::env::temp_dir().join(format!("kg-store-foreign-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let other = dir.join("other.sqlite3");
    let conn = rusqlite::Connection::open(&other).unwrap();
    conn.execute_batch("CREATE TABLE projects (id TEXT); INSERT INTO projects VALUES ('mine');")
        .unwrap();
    drop(conn);
    let text = dir.join("notes.sqlite3");
    std::fs::write(&text, "not a database").unwrap();
    for path in [&other, &text] {
        let before = std::fs::read(path).unwrap();
        let Err(error) = Store::open(path) else {
            panic!("{} was opened", path.display());
        };
        assert_eq!(
            error.to_string(),
            format!(
                "{} is not a kagan database, so kagan did not open it. Nothing was changed. Move it out of that folder, or set KG_DATA_DIR to a different folder",
                path.display()
            )
        );
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    let empty = dir.join("empty.sqlite3");
    std::fs::write(&empty, "").unwrap();
    assert_eq!(Store::open(&empty).unwrap().schema_version().unwrap(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_migration_leaves_the_previous_schema() {
    let path =
        std::env::temp_dir().join(format!("kg-store-migrate-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    let error = crate::migrate(&mut conn, ["CREATE TABLE keep_me (id INTEGER);", "NOT SQL"]);
    assert!(error.is_err());
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'keep_me'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    drop(conn);
    std::fs::remove_file(&path).unwrap();
}
