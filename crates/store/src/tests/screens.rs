use kg_domain::{EventKind, ThreadId, Timestamp};

use super::{store, thread};
use crate::{Error, ScreenRow};

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
