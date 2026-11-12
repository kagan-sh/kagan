use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, op};
#[cfg(unix)]
use super::{events, screen_with, sent_text, started};

fn add(title: &str, body: &str) -> Op {
    Op::SavedPrompt(SavedPromptOp::Add {
        title: title.into(),
        body: Some(body.into()),
        file: None,
    })
}

async fn added(title: &str, body: &str, cx: &mut TestAppContext) -> SavedPrompt {
    let OpResult::SavedPrompt(prompt) = op(add(title, body), cx).await.unwrap() else {
        panic!("saved prompt");
    };
    prompt
}

#[gpui_kit::test]
async fn add_search_star_and_delete_round_trip(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let notes = added("  Notes  ", "remember this", cx).await;
    assert_eq!(notes.title, "Notes");
    assert!(!notes.starred);
    let other = added("Other", "Hello body", cx).await;
    let OpResult::SavedPrompts(found) = op(
        Op::SavedPrompt(SavedPromptOp::Search {
            text: "HELLO".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("search");
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, other.id);
    let OpResult::SavedPrompt(starred) = op(
        Op::SavedPrompt(SavedPromptOp::Star {
            id: notes.id.clone(),
            starred: true,
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("star");
    };
    assert!(starred.starred);
    let OpResult::SavedPrompts(all) = op(
        Op::SavedPrompt(SavedPromptOp::Search {
            text: String::new(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("list");
    };
    assert_eq!(all[0].id, notes.id);
    let deleting = cx.update(|cx| {
        crate::Workspace::dispatch(
            Op::SavedPrompt(SavedPromptOp::Delete {
                id: other.id.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    approve_delete(cx).await;
    deleting.await.unwrap();
    let missing = op(
        Op::SavedPrompt(SavedPromptOp::Delete {
            id: SavedPromptId("svp_missing".into()),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)));
    let empty = op(add("   ", "text"), cx).await.unwrap_err();
    assert!(matches!(empty, OpError::Invalid(_)));
}

#[gpui_kit::test]
async fn add_refuses_a_body_with_no_text_and_keeps_line_breaks(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for body in ["  \n", "\u{1}\u{2}"] {
        let error = op(add("Title", body), cx).await.unwrap_err();
        assert!(matches!(error, OpError::Invalid(_)), "{body:?}");
    }
    added("Lines", "one\ntwo", cx).await;
}

#[cfg(unix)]
#[gpui_kit::test]
async fn send_uses_the_same_path_as_thread_send(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let program = super::script(
        fixture.dir.path(),
        "echo-saved",
        "while IFS= read -r line; do printf 'got:%s\\n' \"$line\"; done",
    );
    let (id, _reap) = started(&fixture, &program, cx).await;
    let prompt = added("Greet", "hello", cx).await;
    op(
        Op::SavedPrompt(SavedPromptOp::Send {
            id: prompt.id.clone(),
            thread_id: id.clone(),
        }),
        cx,
    )
    .await
    .unwrap();
    let frame = screen_with(&id, "got:hello", cx).await;
    assert!(frame.contains("got:hello"), "{frame}");
    assert_eq!(sent_text(&events(&id, cx).await), ["hello"]);
}

#[gpui_kit::test]
async fn edit_changes_the_title_and_text_and_keeps_the_star(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let notes = added("Notes", "old text", cx).await;
    op(
        Op::SavedPrompt(SavedPromptOp::Star {
            id: notes.id.clone(),
            starred: true,
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::SavedPrompt(edited) = op(
        Op::SavedPrompt(SavedPromptOp::Edit {
            id: notes.id.clone(),
            title: "  Plans  ".into(),
            body: "new text\nsecond line".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("edit");
    };
    assert_eq!(edited.id, notes.id);
    assert_eq!(edited.title, "Plans");
    assert_eq!(edited.body, "new text\nsecond line");
    assert!(edited.starred, "an edit keeps the star");
    let OpResult::SavedPrompts(found) = op(
        Op::SavedPrompt(SavedPromptOp::Search {
            text: "second line".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("search");
    };
    assert_eq!(found.len(), 1, "the new text is what search finds");
    let missing = op(
        Op::SavedPrompt(SavedPromptOp::Edit {
            id: SavedPromptId("svp_missing".into()),
            title: "Title".into(),
            body: "text".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
    let empty = op(
        Op::SavedPrompt(SavedPromptOp::Edit {
            id: notes.id,
            title: "   ".into(),
            body: "text".into(),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(empty, OpError::Invalid(_)), "{empty}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn deleting_a_saved_prompt_waits_for_the_person(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let notes = added("Notes", "text", cx).await;
    let deleting = cx.update(|cx| {
        crate::Workspace::dispatch(
            Op::SavedPrompt(SavedPromptOp::Delete {
                id: notes.id.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    assert!(
        prompt_listed(&notes.id, cx).await,
        "nothing is deleted before the person answers"
    );
    let approval = cx.read(|cx| {
        crate::Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    cx.update(|cx| crate::Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    deleting.await.unwrap();
    assert!(!prompt_listed(&notes.id, cx).await);
}

async fn approve_delete(cx: &mut TestAppContext) {
    let approval = cx.read(|cx| {
        crate::Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    cx.update(|cx| crate::Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
}

#[cfg(unix)]
async fn prompt_listed(id: &SavedPromptId, cx: &mut TestAppContext) -> bool {
    let OpResult::SavedPrompts(all) = op(
        Op::SavedPrompt(SavedPromptOp::Search {
            text: String::new(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("list");
    };
    all.iter().any(|prompt| &prompt.id == id)
}

#[gpui_kit::test]
async fn a_saved_prompt_title_is_refused_when_another_has_it(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    added("Review", "first", cx).await;
    let error = op(add("review", "second"), cx).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "a saved prompt named review already exists"
    );
}
