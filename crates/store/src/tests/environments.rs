use std::path::{Path, PathBuf};

use kg_domain::{
    Environment, EnvironmentId, EnvironmentProvider, EnvironmentStatus, Project, ProjectId,
    ProjectKind,
};

use super::{id, owned, store, thread};

fn environment(
    id: &str,
    path: &str,
    status: EnvironmentStatus,
    provider: EnvironmentProvider,
) -> Environment {
    Environment {
        id: EnvironmentId(id.into()),
        project_id: ProjectId("prj_1".into()),
        path: PathBuf::from(path),
        status,
        provider,
        branch: None,
        base_branch: None,
        retire_at: None,
        error: None,
    }
}

fn personal(id: &str, path: &str) -> Environment {
    environment(
        id,
        path,
        EnvironmentStatus::Ready,
        EnvironmentProvider::Personal,
    )
}

#[test]
fn deleting_an_environment_removes_it() {
    let store = store();
    let env = personal("env_1", "/code/kagan/personal/one");
    store.put_environment(&env).unwrap();
    store.delete_environment(&env.id).unwrap();
    assert!(store.environment(&env.id).unwrap().is_none());
}

#[test]
fn all_environments_lists_every_one() {
    let store = store();
    store
        .put_environment(&personal("env_1", "/code/kagan/personal/one"))
        .unwrap();
    store
        .put_environment(&personal("env_2", "/code/kagan/personal/two"))
        .unwrap();
    let mut ids: Vec<_> = store
        .all_environments()
        .unwrap()
        .into_iter()
        .map(|env| env.id)
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        [EnvironmentId("env_1".into()), EnvironmentId("env_2".into())]
    );
}

#[test]
fn removal_pending_covers_a_destroyed_personal_folder_and_nothing_else() {
    let store = store();
    let folder = Path::new("/code/kagan/personal/one");
    let mut destroyed = personal("env_1", "/code/kagan/personal/one");
    destroyed.status = EnvironmentStatus::Destroyed;
    store.put_environment(&destroyed).unwrap();

    assert!(store.removal_pending(folder).unwrap());
    assert!(store.removal_pending(&folder.join("sub")).unwrap());
    assert!(
        !store
            .removal_pending(Path::new("/code/kagan/personal/one-other"))
            .unwrap()
    );
    assert!(
        !store
            .removal_pending(Path::new("/code/kagan/personal"))
            .unwrap()
    );

    // A live folder, a destroyed worktree and a destroyed folder elsewhere do not block.
    store
        .put_environment(&personal("env_2", "/code/kagan/personal/live"))
        .unwrap();
    let mut worktree = environment(
        "env_3",
        "/code/kagan/worktrees/one",
        EnvironmentStatus::Ready,
        EnvironmentProvider::Worktree,
    );
    worktree.status = EnvironmentStatus::Destroyed;
    store.put_environment(&worktree).unwrap();
    assert!(
        !store
            .removal_pending(Path::new("/code/kagan/personal/live"))
            .unwrap()
    );
    assert!(
        !store
            .removal_pending(Path::new("/code/kagan/worktrees/one"))
            .unwrap()
    );
    assert!(!store.removal_pending(Path::new("/elsewhere")).unwrap());
}

#[test]
fn personal_folders_returns_an_unshared_folder_used_by_an_owned_thread() {
    let store = store();
    let env = personal("env_1", "/code/kagan/personal/one");
    store.put_environment(&env).unwrap();
    let mut owner = thread("thr_1", 1);
    owner.environment_id = Some(env.id.clone());
    store.put_thread(&owner).unwrap();
    let mut child = owned("thr_2", "thr_1", 2);
    child.environment_id = Some(env.id.clone());
    store.put_thread(&child).unwrap();

    assert_eq!(store.personal_folders(&id("thr_1")).unwrap(), vec![env]);
}

#[test]
fn personal_folders_ignores_a_live_worktree() {
    let store = store();
    let worktree = environment(
        "env_1",
        "/code/kagan/worktrees/one",
        EnvironmentStatus::Ready,
        EnvironmentProvider::Worktree,
    );
    store.put_environment(&worktree).unwrap();
    let mut owner = thread("thr_1", 1);
    owner.environment_id = Some(worktree.id.clone());
    store.put_thread(&owner).unwrap();

    assert!(store.personal_folders(&id("thr_1")).unwrap().is_empty());
}

#[test]
fn personal_folders_skips_a_folder_another_thread_uses() {
    let store = store();
    let env = personal("env_1", "/code/kagan/personal/one");
    store.put_environment(&env).unwrap();
    let mut owner = thread("thr_1", 1);
    owner.environment_id = Some(env.id.clone());
    store.put_thread(&owner).unwrap();
    let mut other = thread("thr_2", 2);
    other.environment_id = Some(env.id.clone());
    store.put_thread(&other).unwrap();

    assert!(store.personal_folders(&id("thr_1")).unwrap().is_empty());
}

#[test]
fn personal_folders_skips_a_folder_another_environment_nests_in() {
    let store = store();
    let env = personal("env_1", "/code/kagan/personal/one");
    store.put_environment(&env).unwrap();
    store
        .put_environment(&personal("env_2", "/code/kagan/personal/one/sub"))
        .unwrap();
    let mut owner = thread("thr_1", 1);
    owner.environment_id = Some(env.id.clone());
    store.put_thread(&owner).unwrap();

    assert!(store.personal_folders(&id("thr_1")).unwrap().is_empty());
}

#[test]
fn personal_folders_skips_a_folder_a_project_sits_in() {
    let store = store();
    let env = personal("env_1", "/code/kagan/personal/one");
    store.put_environment(&env).unwrap();
    store
        .put_project(&Project {
            id: ProjectId("prj_2".into()),
            kind: ProjectKind::Standard,
            name: "inside".into(),
            path: "/code/kagan/personal/one/repo".into(),
            git_remote_url: None,
            sort: 1,
            program: None,
        })
        .unwrap();
    let mut owner = thread("thr_1", 1);
    owner.environment_id = Some(env.id.clone());
    store.put_thread(&owner).unwrap();

    assert!(store.personal_folders(&id("thr_1")).unwrap().is_empty());
}

#[test]
fn personal_folders_keeps_a_folder_when_a_sibling_environment_exists() {
    let store = store();
    let env = personal("env_1", "/code/kagan/personal/one");
    store.put_environment(&env).unwrap();
    store
        .put_environment(&personal("env_2", "/code/kagan/personal/other"))
        .unwrap();
    let mut owner = thread("thr_1", 1);
    owner.environment_id = Some(env.id.clone());
    store.put_thread(&owner).unwrap();

    assert_eq!(store.personal_folders(&id("thr_1")).unwrap(), vec![env]);
}
