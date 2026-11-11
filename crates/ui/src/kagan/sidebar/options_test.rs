use kg_domain::{
    ProjectId, ThreadId, ThreadMark, ThreadNotify, ThreadStatus, Timestamp, Visibility,
};

use super::options::{ListOptions, ThreadOrder, sort_threads};

fn thread(id: &str, created: i64, attention: i64, pinned: bool, read: bool) -> kg_domain::Thread {
    kg_domain::Thread {
        id: ThreadId(id.into()),
        project_id: ProjectId("proj".into()),
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: String::new(),
        status: ThreadStatus::Idle,
        visibility: Visibility::Visible,
        parent_thread_id: None,
        quiet: false,
        notify: ThreadNotify::Default,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned,
        last_read_at: read.then_some(Timestamp(attention)),
        latest_attention_at: Timestamp(attention),
        mark: ThreadMark::None,
        archived_at: None,
        created_at: Timestamp(created),
        updated_at: Timestamp(created),
    }
}

fn ids<'a>(threads: &[&'a kg_domain::Thread]) -> Vec<&'a str> {
    threads.iter().map(|thread| thread.id.0.as_str()).collect()
}

#[test]
fn current_order_is_newest_first_with_pinned_threads_first() {
    let pinned_old = thread("pinned-old", 1, 1, true, true);
    let fresh = thread("fresh", 5, 1, false, true);
    let pinned_new = thread("pinned-new", 4, 1, true, true);
    let mut threads = vec![&fresh, &pinned_old, &pinned_new];
    sort_threads(&mut threads, ListOptions::default());
    assert_eq!(ids(&threads), ["pinned-new", "pinned-old", "fresh"]);
}

#[test]
fn attention_order_is_newest_attention_first_and_pinned_stays_first() {
    let pinned = thread("pinned", 1, 1, true, true);
    let loud = thread("loud", 2, 9, false, true);
    let quiet = thread("quiet", 8, 3, false, true);
    let mut threads = vec![&quiet, &pinned, &loud];
    let options = ListOptions {
        order: ThreadOrder::Attention,
        ..ListOptions::default()
    };
    sort_threads(&mut threads, options);
    assert_eq!(ids(&threads), ["pinned", "loud", "quiet"]);
}

#[test]
fn unread_comes_after_pinned_and_before_the_rest() {
    let pinned_read = thread("pinned-read", 2, 2, true, true);
    let pinned_unread = thread("pinned-unread", 1, 4, true, false);
    let unread = thread("unread", 3, 3, false, false);
    let read = thread("read", 9, 9, false, true);
    let mut threads = vec![&read, &unread, &pinned_read, &pinned_unread];
    let options = ListOptions {
        unread_first: true,
        ..ListOptions::default()
    };
    sort_threads(&mut threads, options);
    assert_eq!(
        ids(&threads),
        ["pinned-read", "pinned-unread", "unread", "read"]
    );
}

#[test]
fn the_same_moment_keeps_one_order_by_id() {
    let low = thread("a", 1, 5, false, true);
    let high = thread("b", 1, 5, false, true);
    let mut threads = vec![&low, &high];
    let options = ListOptions {
        order: ThreadOrder::Attention,
        ..ListOptions::default()
    };
    sort_threads(&mut threads, options);
    assert_eq!(ids(&threads), ["b", "a"]);
}
