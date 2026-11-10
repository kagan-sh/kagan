use kg_domain::{
    Environment, EnvironmentId, EnvironmentStatus, Project, ProjectId, ProjectKind, Timestamp,
};

use super::store;
use crate::Error;

#[test]
fn project_and_environment_round_trip() {
    let store = store();
    let environment = Environment {
        id: EnvironmentId("env_1".into()),
        project_id: ProjectId("prj_1".into()),
        path: "/code/kagan-w2".into(),
        status: EnvironmentStatus::Ready,
        provider: kg_domain::EnvironmentProvider::Worktree,
        branch: Some("w2-store".into()),
        base_branch: Some("build".into()),
        retire_at: Some(Timestamp(9)),
        error: None,
    };
    store.put_environment(&environment).unwrap();
    assert_eq!(
        store.environment(&environment.id).unwrap(),
        Some(environment.clone())
    );
    assert_eq!(
        store.environments(&environment.project_id).unwrap(),
        [environment]
    );
    assert_eq!(store.projects().unwrap()[0].name, "kagan");
    assert_eq!(store.project(&ProjectId("prj_2".into())).unwrap(), None);
}

#[test]
fn reorder_project_places_before_a_neighbour() {
    let store = store();
    store
        .put_project(&Project {
            id: ProjectId("prj_2".into()),
            kind: ProjectKind::Standard,
            name: "other".into(),
            path: "/code/other".into(),
            git_remote_url: None,
            sort: 1,
            program: None,
        })
        .unwrap();
    store
        .put_project(&Project {
            id: ProjectId("personal".into()),
            kind: ProjectKind::Personal,
            name: "Personal".into(),
            path: "/personal".into(),
            git_remote_url: None,
            sort: 0,
            program: None,
        })
        .unwrap();
    store
        .reorder_project(&ProjectId("prj_2".into()), Some(&ProjectId("prj_1".into())))
        .unwrap();
    let mut projects = store.projects().unwrap();
    kg_domain::order_sidebar_projects(&mut projects);
    let names: Vec<_> = projects.into_iter().map(|project| project.name).collect();
    assert_eq!(names, ["other", "kagan", "Personal"]);
    assert!(matches!(
        store.reorder_project(&ProjectId("personal".into()), None),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn project_program_is_saved_cleared_and_refused_for_a_missing_project() {
    let store = store();
    let id = ProjectId("prj_1".into());
    store
        .set_project_program(&id, Some("claude --model x"))
        .unwrap();
    assert_eq!(
        store.project(&id).unwrap().unwrap().program.as_deref(),
        Some("claude --model x")
    );
    store.set_project_program(&id, None).unwrap();
    assert_eq!(store.project(&id).unwrap().unwrap().program, None);
    assert!(
        store
            .set_project_program(&ProjectId("prj_9".into()), Some("codex"))
            .is_err()
    );
}
