use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, commit, git_in, op, started};

async fn details(id: &ThreadId, cx: &mut TestAppContext) -> ThreadDetails {
    match op(Op::Thread(ThreadOp::Details { id: id.clone() }), cx)
        .await
        .unwrap()
    {
        OpResult::Details(details) => details,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
async fn a_folder_without_git_shows_only_the_folder_and_program(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;
    let read = details(&id, cx).await;
    assert_eq!(
        read.folder,
        dunce::canonicalize(fixture.dir.path()).unwrap()
    );
    assert_eq!(read.environment, EnvironmentProvider::Project);
    assert_eq!(read.program, "/bin/sh");
    assert_eq!(read.git, None);
}

#[gpui_kit::test]
async fn a_branch_shows_its_base_counts_and_last_commits(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let path = fixture.dir.path();
    git_in(path, &["init", "-b", "main"]);
    commit(path, &["commit", "--allow-empty", "-m", "First"]);
    commit(path, &["commit", "--allow-empty", "-m", "Second"]);
    git_in(path, &["checkout", "-b", "feature"]);
    commit(path, &["commit", "--allow-empty", "-m", "On feature"]);
    git_in(path, &["checkout", "main"]);
    commit(path, &["commit", "--allow-empty", "-m", "On main"]);
    git_in(path, &["checkout", "feature"]);
    let (id, _reap) = started(&fixture, "/bin/sh", cx).await;

    let git = details(&id, cx).await.git.unwrap();
    assert_eq!(git.branch.as_deref(), Some("feature"));
    assert_eq!(
        git.base,
        Some(BaseBranch {
            name: "main".into(),
            ahead: 1,
            behind: 1,
        })
    );
    let summaries: Vec<&str> = git.commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["On feature", "Second", "First"]);
    assert!(git.commits.iter().all(|c| c.rev.len() < 40));
}

#[gpui_kit::test]
async fn a_missing_thread_is_not_found(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let missing = op(
        Op::Thread(ThreadOp::Details {
            id: ThreadId("thr_missing".into()),
        }),
        cx,
    )
    .await;
    assert!(matches!(missing, Err(OpError::NotFound(_))), "{missing:?}");
}
