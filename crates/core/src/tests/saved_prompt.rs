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
    op(Op::SavedPrompt(SavedPromptOp::Delete { id: other.id }), cx)
        .await
        .unwrap();
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
async fn a_saved_prompt_title_is_refused_when_another_has_it(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    added("Review", "first", cx).await;
    let error = op(add("review", "second"), cx).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "a saved prompt named review already exists."
    );
}
