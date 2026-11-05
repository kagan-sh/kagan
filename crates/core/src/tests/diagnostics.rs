use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{
    Fixture, assert_diagnostics_keeps_secrets_out, idle_thread, op, plant_secret_plugins, script,
    wait_for_status, wait_saved,
};

#[gpui_kit::test]
async fn diagnostics_writes_a_new_file_without_the_screen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    #[cfg(windows)]
    let line = {
        let path = fixture.dir.path().join("quiet.cmd");
        std::fs::write(&path, "@echo SECRET-MARKER\r\n").unwrap();
        path.display().to_string()
    };
    #[cfg(unix)]
    let line = script(fixture.dir.path(), "quiet", "printf 'SECRET-MARKER\\n'");
    let OpResult::Thread(started) = op(fixture.run(&line), cx).await.unwrap() else {
        panic!("thread")
    };
    wait_for_status(&started.id, ThreadStatus::Idle, cx).await;
    wait_saved(fixture.dir.path(), &started.id, cx);
    let OpResult::Diagnostics { path } =
        op(Op::Diagnostics(DiagnosticsOp::Export { path: None }), cx)
            .await
            .unwrap()
    else {
        panic!("diagnostics")
    };
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("schema "), "{text}");
    assert!(text.contains(&started.id.0), "{text}");
    assert!(text.contains("no environment variables"), "{text}");
    assert!(!text.contains("SECRET-MARKER"), "{text}");
    assert!(!text.contains("HOME="), "{text}");
    let again = op(
        Op::Diagnostics(DiagnosticsOp::Export {
            path: Some(path.clone()),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(again.to_string().contains("already exists"), "{again}");
    let missing = fixture.dir.path().join("no-such-folder").join("out.txt");
    let error = op(
        Op::Diagnostics(DiagnosticsOp::Export {
            path: Some(missing),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("does not exist"), "{error}");
    drop(fixture);
}

#[gpui_kit::test]
async fn diagnostics_omits_secrets_and_names_bad_geometry(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    let dir = fixture.dir.path().to_path_buf();
    let log_dir = dir.join("plugins").join("demo-plugin");
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(
        log_dir.join("logs.jsonl"),
        "\
{\"at\":1,\"level\":\"debug\",\"message\":\"pw-debug-9f3a\"}\n\
{\"at\":2,\"level\":\"info\",\"message\":\"pw-info-9f3a\"}\n\
{\"at\":3,\"level\":\"warn\",\"message\":\"pw-log-9f3a\"}\n\
{\"at\":4,\"level\":\"error\",\"message\":\"bearer pw-error-9f3a\"}\n\
not-json pw-raw-9f3a\n",
    )
    .unwrap();
    let db = dir.join("kagan.sqlite3");
    let root = dir.clone();
    let fixture = fixture
        .restart(cx, move |store| {
            let mut record = idle_thread("thr_secret", project);
            record.title = "kept-prose-9f3a https://dev:pw-title-9f3a@example.com/tool?token=pw-titleq-9f3a https://example.com/a,https://dev:pw-second-9f3a@github.com/org/repo".into();
            store.put_thread(&record).unwrap();
            let id = ThreadId("thr_secret".into());
            let started = store
                .append_events(&id, Timestamp(1), [EventKind::Started])
                .unwrap();
            store
                .append_events(
                    &id,
                    Timestamp(2),
                    [EventKind::Failed {
                        error: Some(
                            "https://dev:pw-end-9f3a@example.com/e?x=pw-endq-9f3a".into(),
                        ),
                    }],
                )
                .unwrap();
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "INSERT INTO terminal_screens \
                    (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
                 VALUES ('thr_secret', ?1, 9999, 24, ?2, 9, 1)",
                rusqlite::params![started[0].seq, b"pw-replay-9f3a"],
            )
            .unwrap();
            plant_secret_plugins(store, &root);
        })
        .await;
    let id = ThreadId("thr_secret".into());
    assert_eq!(
        fixture
            .workspace
            .read_with(cx, |workspace, _| workspace.screen_note(&id)),
        Some("The screen was outside the geometry or the 4 MiB bound.")
    );
    let OpResult::Diagnostics { path } =
        op(Op::Diagnostics(DiagnosticsOp::Export { path: None }), cx)
            .await
            .unwrap()
    else {
        panic!("diagnostics")
    };
    let text = std::fs::read_to_string(&path).unwrap();
    assert_diagnostics_keeps_secrets_out(&text);
    drop(fixture);
}
