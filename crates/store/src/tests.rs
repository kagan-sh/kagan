use kg_domain::{
    Project, ProjectId, ProjectKind, Thread, ThreadId, ThreadStatus, Timestamp, Visibility,
};

use crate::Store;

mod automations;
mod environments;
mod events;
mod interactions;
mod plugins;
mod projects;
mod saved_programs;
mod saved_prompts;
mod schema;
mod screens;
mod sections;
mod threads;

fn store() -> Store {
    let store = Store::open(":memory:").unwrap();
    store
        .put_project(&Project {
            id: ProjectId("prj_1".into()),
            kind: ProjectKind::Standard,
            name: "kagan".into(),
            path: "/code/kagan".into(),
            git_remote_url: Some("https://github.com/example/repo".into()),
            sort: 0,
            program: None,
        })
        .unwrap();
    store
}

fn thread(id: &str, created_at: i64) -> Thread {
    Thread {
        id: ThreadId(id.into()),
        project_id: ProjectId("prj_1".into()),
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: "claude-code".into(),
        status: ThreadStatus::Idle,
        visibility: Visibility::Visible,
        parent_thread_id: None,
        quiet: false,
        notify: kg_domain::ThreadNotify::Default,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned: false,
        last_read_at: None,
        latest_attention_at: Timestamp(created_at),
        mark: kg_domain::ThreadMark::None,
        archived_at: None,
        created_at: Timestamp(created_at),
        updated_at: Timestamp(created_at),
    }
}

fn owned(id: &str, owner: &str, created_at: i64) -> Thread {
    Thread {
        lifecycle_owner_thread_id: Some(ThreadId(owner.into())),
        parent_thread_id: Some(ThreadId(owner.into())),
        quiet: false,
        ..thread(id, created_at)
    }
}

fn id(id: &str) -> ThreadId {
    ThreadId(id.into())
}

fn sorted(mut ids: Vec<ThreadId>) -> Vec<ThreadId> {
    ids.sort();
    ids
}
