use std::fmt::Write as _;

use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, idle_thread, op};

fn keep_screen(store: &mut kg_store::Store, id: &ThreadId, replay: &[u8]) {
    let started = store
        .append_events(id, Timestamp(1), [EventKind::Started])
        .unwrap();
    store
        .append_events(id, Timestamp(2), [EventKind::Stopped])
        .unwrap();
    store
        .put_screen(&kg_store::ScreenRow {
            thread_id: id.clone(),
            started_seq: started[0].seq,
            cols: 80,
            rows: 24,
            replay: replay.to_vec(),
            history_dropped: 0,
            screen_kept: true,
        })
        .unwrap();
}

async fn seeded(cx: &mut TestAppContext) -> Fixture {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    fixture
        .restart(cx, move |store| {
            let mut titled = idle_thread("thr_titled", project.clone());
            titled.title = "Deploy staging".into();
            store.put_thread(&titled).unwrap();

            let printed = ThreadId("thr_printed".into());
            store
                .put_thread(&idle_thread("thr_printed", project.clone()))
                .unwrap();
            let mut replay = String::from("first EARLY-MARK line\r\n");
            for line in 0..100 {
                write!(replay, "filler {line}\r\n").unwrap();
            }
            replay.push_str("\x1b[31mNEED\x1b[0mLE-red tail\r\n");
            keep_screen(store, &printed, replay.as_bytes());

            let mut hidden = idle_thread("thr_hidden", project.clone());
            hidden.title = "deploy hidden".into();
            hidden.visibility = Visibility::Hidden;
            store.put_thread(&hidden).unwrap();

            let mut archived = idle_thread("thr_archived", project);
            archived.title = "Old deploy".into();
            archived.archived_at = Some(Timestamp(1));
            store.put_thread(&archived).unwrap();
        })
        .await
}

async fn search(text: &str, archived: bool, cx: &mut TestAppContext) -> Vec<Found> {
    match op(
        Op::Thread(ThreadOp::Search {
            text: text.into(),
            limit: None,
            archived,
        }),
        cx,
    )
    .await
    .unwrap()
    {
        OpResult::Found(found) => found,
        other => panic!("expected found threads, got {other:?}"),
    }
}

fn ids(found: &[Found]) -> Vec<&str> {
    found
        .iter()
        .map(|found| found.thread_id.0.as_str())
        .collect()
}

#[gpui_kit::test]
async fn search_finds_a_title_and_leaves_out_hidden_and_archived_threads(cx: &mut TestAppContext) {
    let fixture = seeded(cx).await;
    let found = search("DEPLOY", false, cx).await;
    assert_eq!(ids(&found), ["thr_titled"]);
    assert_eq!(found[0].project, "Project");
    assert!(!found[0].archived);
    assert_eq!(found[0].snippet, None);
    drop(fixture);
}

#[gpui_kit::test]
async fn archived_threads_come_after_the_others_and_say_so(cx: &mut TestAppContext) {
    let fixture = seeded(cx).await;
    let found = search("deploy", true, cx).await;
    assert_eq!(ids(&found), ["thr_titled", "thr_archived"]);
    assert_eq!(
        found.iter().map(|found| found.archived).collect::<Vec<_>>(),
        [false, true]
    );
    drop(fixture);
}

#[gpui_kit::test]
async fn search_reads_kept_lines_without_escape_codes(cx: &mut TestAppContext) {
    let fixture = seeded(cx).await;
    let found = search("needle-RED", false, cx).await;
    assert_eq!(ids(&found), ["thr_printed"]);
    let snippet = found[0].snippet.clone().unwrap();
    assert_eq!(snippet.text, "NEEDLE-red tail");
    assert_eq!(snippet.scroll, 0);

    let found = search("early-mark", false, cx).await;
    let snippet = found[0].snippet.clone().unwrap();
    assert_eq!(snippet.text, "first EARLY-MARK line");
    assert!(snippet.scroll > 0, "{snippet:?}");
    drop(fixture);
}

#[gpui_kit::test]
async fn search_refuses_no_text_and_a_limit_out_of_range(cx: &mut TestAppContext) {
    let fixture = seeded(cx).await;
    for (text, limit) in [("  ", None), ("deploy", Some(0)), ("deploy", Some(201))] {
        let result = op(
            Op::Thread(ThreadOp::Search {
                text: text.into(),
                limit,
                archived: false,
            }),
            cx,
        )
        .await;
        assert!(matches!(result, Err(OpError::Invalid(_))), "{result:?}");
    }
    let found = match op(
        Op::Thread(ThreadOp::Search {
            text: "e".into(),
            limit: Some(1),
            archived: true,
        }),
        cx,
    )
    .await
    .unwrap()
    {
        OpResult::Found(found) => found,
        other => panic!("{other:?}"),
    };
    assert_eq!(found.len(), 1);
    drop(fixture);
}

async fn printed(cx: &mut TestAppContext, replay: String) -> Fixture {
    let fixture = Fixture::new(cx).await;
    let project = fixture.project.clone();
    fixture
        .restart(cx, move |store| {
            let id = ThreadId("thr_long".into());
            store.put_thread(&idle_thread("thr_long", project)).unwrap();
            keep_screen(store, &id, replay.as_bytes());
        })
        .await
}

fn filler(replay: &mut String, from: usize) {
    for line in from..from + 100 {
        write!(replay, "filler {line}\r\n").unwrap();
    }
}

#[gpui_kit::test]
async fn a_match_across_a_wrapped_row_is_found_and_scrolled_to_its_first_row(
    cx: &mut TestAppContext,
) {
    let mut replay = String::new();
    filler(&mut replay, 0);
    // 75 characters, then a needle that crosses the 80th column.
    write!(
        replay,
        "START-{}WRAP-SPAN{}\r\n",
        "a".repeat(69),
        "b".repeat(220)
    )
    .unwrap();
    filler(&mut replay, 100);
    let fixture = printed(cx, replay).await;
    let found = search("wrap-span", false, cx).await;
    assert_eq!(ids(&found), ["thr_long"]);
    let snippet = found[0].snippet.clone().unwrap();
    assert!(snippet.text.contains("aWRAP-SPANb"), "{snippet:?}");
    op(
        Op::Thread(ThreadOp::Scroll {
            id: ThreadId("thr_long".into()),
            lines: i32::try_from(snippet.scroll).unwrap(),
            terminal: None,
        }),
        cx,
    )
    .await
    .unwrap();
    let grid = fixture
        .workspace
        .read_with(cx, |workspace, _| {
            workspace.grid(&ThreadId("thr_long".into()), None)
        })
        .unwrap();
    let rows: Vec<String> = grid
        .lines
        .iter()
        .map(|line| line.iter().map(|cell| cell.c).collect())
        .collect();
    assert!(
        rows.iter().any(|row| row.starts_with("START-")),
        "{rows:#?}"
    );
}

#[gpui_kit::test]
async fn a_snippet_cuts_a_long_wide_line_on_characters(cx: &mut TestAppContext) {
    let mut replay = format!("{}NEEDLE{}\r\n", "中".repeat(100), "文".repeat(100));
    filler(&mut replay, 0);
    let fixture = printed(cx, replay).await;
    let found = search("needle", false, cx).await;
    let text = found[0].snippet.clone().unwrap().text;
    assert!(text.contains("中NEEDLE文"), "{text}");
    assert!(text.starts_with('…') && text.ends_with('…'), "{text}");
    assert!(text.chars().count() <= 122, "{text}");
    drop(fixture);
}
