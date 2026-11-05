use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::Workspace;

use super::{
    Fixture, dispatch_delete, environment_of, hold_delete, hold_removal, hold_run, op,
    pending_delete, personal_run, started_personal,
};

#[gpui_kit::test]
async fn archived_personal_files_survive_the_grace_and_a_restart(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    op(Op::Thread(ThreadOp::Archive { id: thread.clone() }), cx)
        .await
        .unwrap();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(12));
    cx.run_until_parked();
    let kept = environment_of(&environment_id, cx).await;
    assert_eq!(kept.status, EnvironmentStatus::Ready, "{kept:?}");
    assert!(kept.retire_at.is_none(), "{kept:?}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    op(Op::Thread(ThreadOp::Unarchive { id: thread }), cx)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    drop(fixture);
}

#[gpui_kit::test]
async fn an_old_destroyed_personal_folder_stays_while_its_thread_is_archived(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    op(Op::Thread(ThreadOp::Archive { id: thread.clone() }), cx)
        .await
        .unwrap();
    let fixture = fixture
        .restart(cx, |store| {
            let mut environment = store.environment(&environment_id).unwrap().unwrap();
            environment.status = EnvironmentStatus::Destroyed;
            environment.retire_at = Some(Timestamp(0));
            store.put_environment(&environment).unwrap();
        })
        .await;
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    op(Op::Thread(ThreadOp::Unarchive { id: thread }), cx)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    drop(fixture);
}

#[gpui_kit::test]
async fn deleting_a_personal_thread_names_its_folder_and_removes_it_on_approval(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: thread.clone(),
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, question) = pending_delete(cx);
    assert!(
        question.contains(&folder.display().to_string()),
        "{question}"
    );
    assert!(question.contains("valuable-note.txt"), "{question}");
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(!folder.exists());
    assert!(
        op(Op::Thread(ThreadOp::Get { id: thread }), cx)
            .await
            .is_err()
    );
}

#[gpui_kit::test]
async fn declining_a_personal_delete_keeps_the_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: thread.clone(),
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx))
        .await
        .unwrap();
    let error = deleting.await.unwrap_err();
    assert!(error.to_string().contains("declined"), "{error}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    assert!(
        op(Op::Thread(ThreadOp::Get { id: thread }), cx)
            .await
            .is_ok()
    );
}

#[gpui_kit::test]
async fn deleting_a_thread_keeps_a_personal_folder_another_thread_uses(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    op(
        personal_run(
            &fixture,
            "other",
            EnvironmentSpec::Reuse {
                environment_id: environment_id.clone(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    let deleting = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: thread,
                approval_thread_id: None,
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, question) = pending_delete(cx);
    assert!(!question.contains("permanently"), "{question}");
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
}

#[gpui_kit::test]
async fn a_run_that_read_a_folder_before_its_delete_cannot_attach_to_it(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    std::fs::write(folder.join("valuable-note.txt"), "keep me").unwrap();
    let release_removal = hold_removal(&fixture, cx);
    let release_delete = hold_delete(&fixture, cx);
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    let release_run = hold_run(&fixture, cx);
    let running = cx.update(|cx| {
        Workspace::dispatch(
            personal_run(
                &fixture,
                "late",
                EnvironmentSpec::Reuse {
                    environment_id: environment_id.clone(),
                },
            ),
            cx,
        )
    });
    cx.run_until_parked();
    release_delete.try_send(()).unwrap();
    for _ in 0..500 {
        cx.run_until_parked();
        let store = kg_store::Store::open(fixture.dir.path().join("kagan.sqlite3")).unwrap();
        let destroyed = store
            .environment(&environment_id)
            .unwrap()
            .is_some_and(|environment| environment.status == EnvironmentStatus::Destroyed);
        drop(store);
        if destroyed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    release_run.try_send(()).unwrap();
    let error = running.await.unwrap_err();
    assert!(error.to_string().contains("ready"), "{error}");
    deleting.await.unwrap();
    release_removal.try_send(()).unwrap();
    cx.run_until_parked();
    assert!(!folder.exists());
    let OpResult::Threads(threads) = op(
        Op::Thread(ThreadOp::List {
            project_id: None,
            archived: None,
            limit: None,
            before: None,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("threads")
    };
    assert!(threads.iter().all(|thread| thread.title != "late"));
}

#[gpui_kit::test]
async fn a_thread_that_reuses_the_folder_during_the_approval_keeps_it(cx: &mut TestAppContext) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    let note = folder.join("valuable-note.txt");
    std::fs::write(&note, "keep me").unwrap();
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    op(
        personal_run(
            &fixture,
            "late",
            EnvironmentSpec::Reuse {
                environment_id: environment_id.clone(),
            },
        ),
        cx,
    )
    .await
    .unwrap();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    let error = deleting.await.unwrap_err();
    assert!(error.to_string().contains("delete again"), "{error}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "keep me");
    assert!(
        op(Op::Thread(ThreadOp::Get { id: thread }), cx)
            .await
            .is_ok()
    );
}

#[gpui_kit::test]
async fn a_delete_interrupted_before_its_folder_is_removed_finishes_at_restart(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::personal(cx).await;
    let (thread, environment_id) = started_personal(&fixture, cx).await;
    let folder = environment_of(&environment_id, cx).await.path;
    std::fs::write(folder.join("valuable-note.txt"), "keep me").unwrap();
    let release = hold_removal(&fixture, cx);
    let deleting = dispatch_delete(&thread, cx);
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(folder.exists());
    let fixture = fixture.restart(cx, |_| {}).await;
    cx.run_until_parked();
    assert!(!folder.exists());
    assert!(
        op(
            Op::Environment(EnvironmentOp::Get { id: environment_id }),
            cx
        )
        .await
        .is_err()
    );
    drop(release);
    drop(fixture);
}
