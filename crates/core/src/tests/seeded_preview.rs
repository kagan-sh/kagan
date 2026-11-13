use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::{InteractionOption, Op, OpError, Plugin, PluginId, ThreadId, ThreadOp, prefs};
use serde_json::json;

use super::{Fixture, idle_thread, op, open_at};
use crate::{NO_ONE_HERE, Workspace, seed_preview};

// The same open the app does at start, on a folder made by `seed_preview`.
#[gpui_kit::test]
async fn a_seeded_preview_folder_opens_with_its_plugin_enabled_and_approved(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let approved = json!({ "clipboard": { "write": true } });
    let plugin = Plugin {
        id: PluginId("dev.test.panel".into()),
        root: dir.path().join("panel"),
        source: dir.path().join("panel").display().to_string(),
        commit: None,
        enabled: true,
        approved: approved.clone(),
        skills: Vec::new(),
        error: None,
    };
    seed_preview(dir.path(), &plugin).unwrap();

    let workspace = open_at(&dir, cx, |_| {}).await.unwrap();
    let plugins = cx.read(|cx| workspace.read(cx).plugins()).await.unwrap();
    let preview = cx.read(|cx| workspace.read(cx).is_preview());

    assert_eq!(plugins.len(), 1, "{plugins:?}");
    assert!(plugins[0].enabled, "the seeded plugin is not enabled");
    assert_eq!(plugins[0].approved, approved);
    assert!(preview, "the folder does not say it is a preview");
}

// The preview marker is not a preference: a person's kagan cannot read or write it through `kg`.
#[gpui_kit::test]
async fn kg_preference_refuses_the_preview_marker(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let plugin = Plugin {
        id: PluginId("dev.test.panel".into()),
        root: dir.path().join("panel"),
        source: dir.path().join("panel").display().to_string(),
        commit: None,
        enabled: true,
        approved: json!({}),
        skills: Vec::new(),
        error: None,
    };
    seed_preview(dir.path(), &plugin).unwrap();
    let _workspace = open_at(&dir, cx, |_| {}).await.unwrap();

    let read = Op::Preference(kg_domain::PreferenceOp::Get {
        key: prefs::PREVIEW.into(),
    });
    let write = Op::Preference(kg_domain::PreferenceOp::Set {
        key: prefs::PREVIEW.into(),
        value: json!(false),
    });
    for op in [read, write] {
        let answer = cx.update(|cx| Workspace::dispatch(op, cx)).await;
        assert!(
            answer.is_err(),
            "kg preference reached the preview marker: {answer:?}"
        );
    }
}

// A preview with one thread in it, which a question would be about.
async fn preview_with_a_thread(cx: &mut TestAppContext) -> Fixture {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    fixture
        .restart(cx, move |store| {
            store.set_setting(prefs::PREVIEW, &json!(true)).unwrap();
            store
                .put_thread(&idle_thread("thr_preview", project))
                .unwrap();
        })
        .await
}

// Gives the answer within five seconds of test time, or None when the question is still open.
async fn at_once<T: 'static>(cx: &mut TestAppContext, task: gpui_kit::Task<T>) -> Option<T> {
    let timeout = cx.executor().timer(Duration::from_secs(5));
    match futures::future::select(Box::pin(task), Box::pin(timeout)).await {
        futures::future::Either::Left((value, _)) => Some(value),
        futures::future::Either::Right(_) => None,
    }
}

#[gpui_kit::test]
async fn a_thread_delete_in_a_preview_is_refused_at_once(cx: &mut TestAppContext) {
    let _fixture = preview_with_a_thread(cx).await;
    let deleted = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Delete {
                id: ThreadId("thr_preview".into()),
                approval_thread_id: None,
            }),
            cx,
        )
    });
    let outcome = at_once(cx, deleted).await;
    let Some(Err(OpError::Invalid(message))) = outcome else {
        panic!("a delete in a preview did not refuse at once: {outcome:?}");
    };
    assert_eq!(message, NO_ONE_HERE);
}

#[gpui_kit::test]
async fn a_thread_ask_in_a_preview_is_refused_at_once(cx: &mut TestAppContext) {
    let _fixture = preview_with_a_thread(cx).await;
    let option = |id: &str| InteractionOption {
        id: id.into(),
        label: id.into(),
        role: None,
    };
    let asked = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: ThreadId("thr_preview".into()),
                title: "Which way?".into(),
                options: vec![option("left"), option("right")],
                fields: None,
            }),
            cx,
        )
    });
    let outcome = at_once(cx, asked).await;
    let Some(Err(OpError::Invalid(message))) = outcome else {
        panic!("an ask in a preview did not refuse at once: {outcome:?}");
    };
    assert_eq!(message, NO_ONE_HERE);
}
