use kg_domain::{EnvironmentId, ProjectId, ThreadId};

use super::{BranchPick, FolderUse, ProjectIntent, Reuse, StartState};

#[test]
fn a_folder_is_busy_only_when_a_thread_runs_in_it() {
    assert!(FolderUse::of(true).is_busy());
    assert!(!FolderUse::of(false).is_busy());
    assert!(matches!(FolderUse::of(true), FolderUse::Busy));
    assert!(matches!(FolderUse::of(false), FolderUse::Free));
}

#[test]
fn reset_forgets_the_reused_thread_and_the_setup_view() {
    let mut state = StartState {
        reuse: Some(Reuse {
            project: ProjectId("prj_1".into()),
            thread: ThreadId("thr_1".into()),
            environment: EnvironmentId("env_1".into()),
            title: "title".into(),
            name: "name".into(),
            shown: "shown".into(),
        }),
        setup_open: true,
        pick: BranchPick::Named("main".into()),
        intent: ProjectIntent::New,
        ..Default::default()
    };
    state.reset();
    assert!(state.take_reuse_thread().is_none());
    assert!(!state.setup_open);
    assert!(matches!(state.pick, BranchPick::Current));
    assert!(matches!(state.intent, ProjectIntent::Current));
}
