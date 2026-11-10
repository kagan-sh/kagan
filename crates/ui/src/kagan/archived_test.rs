use kg_domain::{ProjectId, Thread, ThreadId, ThreadMark, ThreadStatus, Timestamp, Visibility};

use super::in_archived_list;

fn thread(visibility: Visibility, archived: bool) -> Thread {
    Thread {
        id: ThreadId("thr_1".into()),
        project_id: ProjectId("prj_1".into()),
        environment_id: None,
        title: "title".into(),
        title_locked: false,
        provider: "sh".into(),
        status: ThreadStatus::Idle,
        visibility,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned: false,
        last_read_at: None,
        latest_attention_at: Timestamp(1),
        mark: ThreadMark::None,
        archived_at: archived.then_some(Timestamp(2)),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}

#[test]
fn the_archived_list_shows_only_visible_archived_threads() {
    assert!(in_archived_list(&thread(Visibility::Visible, true)));
    assert!(!in_archived_list(&thread(Visibility::Hidden, true)));
    assert!(!in_archived_list(&thread(Visibility::Visible, false)));
    assert!(!in_archived_list(&thread(Visibility::Hidden, false)));
}
