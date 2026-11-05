use gpui_kit::TestAppContext;
use kg_domain::*;

use crate::host::database::Result;

use super::{Fixture, op, program, started};

async fn wait_event(
    id: &ThreadId,
    kind: &str,
    after_seq: Option<u64>,
    timeout_ms: u64,
    cx: &mut TestAppContext,
) -> Result<OpResult> {
    op(
        Op::Thread(ThreadOp::WaitEvent {
            id: id.clone(),
            kind: kind.into(),
            after_seq,
            timeout_ms,
        }),
        cx,
    )
    .await
}

#[gpui_kit::test]
async fn an_unknown_event_kind_is_refused(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let error = wait_event(&id, "nope", None, 10, cx).await.unwrap_err();
    assert!(
        matches!(error, OpError::Invalid(ref message) if message.contains("unknown event type")),
        "{error}"
    );
}

#[gpui_kit::test]
async fn wait_event_takes_the_prefix_and_skips_events_at_or_before_after_seq(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let OpResult::Event(Some(started)) = wait_event(&id, "thread.started", None, 1_000, cx)
        .await
        .unwrap()
    else {
        panic!("started")
    };
    assert_eq!(started.kind, EventKind::Started);
    let OpResult::Event(None) = wait_event(&id, "started", Some(started.seq), 0, cx)
        .await
        .unwrap()
    else {
        panic!("an event at after_seq must not repeat")
    };
}

#[gpui_kit::test]
async fn a_timeout_returns_no_event(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, program(), cx).await;
    let OpResult::Event(None) = wait_event(&id, "answered", None, 0, cx).await.unwrap() else {
        panic!("timeout")
    };
}

#[gpui_kit::test]
async fn wait_event_for_a_missing_thread_is_not_found(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let error = wait_event(&ThreadId("thr_missing".into()), "started", None, 0, cx)
        .await
        .unwrap_err();
    assert!(matches!(error, OpError::NotFound(_)), "{error}");
}
