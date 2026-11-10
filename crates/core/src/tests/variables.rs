use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, op};
#[cfg(unix)]
use super::{fake_agent, ran, started};

fn set(name: &str, project_id: Option<ProjectId>, value: &str) -> Op {
    Op::Env(EnvOp::Set {
        name: name.into(),
        project_id,
        value: Some(value.into()),
    })
}

fn data_dir(fixture: &Fixture, cx: &TestAppContext) -> std::path::PathBuf {
    cx.read(|cx| fixture.workspace.read(cx).data_dir.clone())
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_gets_the_global_variable_and_the_projects_value_over_it(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    op(set("TEST_TOKEN", None, "global-value"), cx)
        .await
        .unwrap();
    op(
        set("TEST_TOKEN", Some(fixture.project.clone()), "project-value"),
        cx,
    )
    .await
    .unwrap();
    op(set("TEST_ONLY_GLOBAL", None, "from-global"), cx)
        .await
        .unwrap();
    let run = fixture.dir.path().join("first");
    let program = format!("{} --arg", fake_agent(&run, "tool").display());
    let (_id, _reap) = started(&fixture, &program, cx).await;
    let env = ran(&run).1;
    assert_eq!(env["TEST_TOKEN"], "project-value");
    assert_eq!(env["TEST_ONLY_GLOBAL"], "from-global");
    assert_eq!(env["KG_PROJECT_ID"], fixture.project.0);
}

#[gpui_kit::test]
async fn listing_names_variables_and_never_returns_a_value(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(set("TEST_TOKEN", None, "secret-value-123"), cx)
        .await
        .unwrap();
    let OpResult::Variables(names) = op(Op::Env(EnvOp::List { project_id: None }), cx)
        .await
        .unwrap()
    else {
        panic!("variables")
    };
    assert_eq!(names, ["TEST_TOKEN"]);
    let shown = format!("{names:?}");
    assert!(!shown.contains("secret-value-123"));
    let stored = std::fs::read_to_string(data_dir(&fixture, cx).join("environment.json")).unwrap();
    assert!(stored.contains("secret-value-123"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(data_dir(&fixture, cx).join("environment.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[gpui_kit::test]
async fn a_kagan_name_an_unknown_project_or_an_unset_removal_is_refused(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let kagan = op(set("KG_THREAD_ID", None, "thr_x"), cx)
        .await
        .unwrap_err();
    assert!(matches!(kagan, OpError::Invalid(_)), "{kagan}");
    let missing = op(
        set("TEST_TOKEN", Some(ProjectId("prj_missing".into())), "x"),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
    let absent = op(
        Op::Env(EnvOp::Remove {
            name: "TEST_NEVER_SET".into(),
            project_id: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(absent, OpError::NotFound(_)), "{absent}");
}

#[gpui_kit::test]
async fn a_removed_projects_variables_are_forgotten(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(
        set("TEST_TOKEN", Some(fixture.project.clone()), "project-value"),
        cx,
    )
    .await
    .unwrap();
    let dir = data_dir(&fixture, cx);
    crate::host::variables::forget_project(&dir, &fixture.project).unwrap();
    let merged = crate::host::variables::for_project(&dir, &fixture.project).unwrap();
    assert!(merged.is_empty(), "{merged:?}");
}

#[gpui_kit::test]
async fn a_project_default_program_is_saved_and_must_parse(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let saved = op(
        Op::Project(ProjectOp::SetProgram {
            id: fixture.project.clone(),
            program: Some("codex --model fast".into()),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Project(project) = saved else {
        panic!("project")
    };
    assert_eq!(project.program.as_deref(), Some("codex --model fast"));
    let broken = op(
        Op::Project(ProjectOp::SetProgram {
            id: fixture.project.clone(),
            program: Some("'unfinished".into()),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(broken, OpError::Invalid(_)), "{broken}");
}
