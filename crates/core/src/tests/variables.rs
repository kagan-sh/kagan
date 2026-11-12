use gpui_kit::TestAppContext;
use kg_domain::*;

use super::{Fixture, op, pending_delete};
#[cfg(unix)]
use super::{fake_agent, ran, started};
use crate::Workspace;

fn set(name: &str, project_id: Option<ProjectId>, value: &str) -> Op {
    Op::Env(EnvOp::Set {
        name: name.into(),
        project_id,
        value: Some(value.into()),
        thread_id: None,
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
            thread_id: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(absent, OpError::NotFound(_)), "{absent}");
}

#[gpui_kit::test]
async fn a_removed_project_takes_its_variables_with_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(
        set("TEST_TOKEN", Some(fixture.project.clone()), "project-value"),
        cx,
    )
    .await
    .unwrap();
    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    remove.await.unwrap();
    let merged =
        crate::host::variables::for_project(&data_dir(&fixture, cx), &fixture.project).unwrap();
    assert!(merged.is_empty(), "{merged:?}");
}

#[gpui_kit::test]
async fn a_project_page_opens_only_for_a_live_project(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let missing = op(
        Op::Settings(SettingsOp::Project {
            id: ProjectId("prj_missing".into()),
            section: None,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
    let shown = op(
        Op::Settings(SettingsOp::Project {
            id: fixture.project.clone(),
            section: None,
        }),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(shown, OpResult::Done);
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

#[cfg(unix)]
#[gpui_kit::test]
async fn the_variables_file_is_owner_only_from_its_first_write(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = Fixture::new(cx).await;
    let file = data_dir(&fixture, cx).join("environment.json");
    assert!(!file.exists());
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    op(set("TEST_TOKEN", None, "first-value"), cx)
        .await
        .unwrap();
    assert_eq!(mode(&file), 0o600);
    op(set("TEST_OTHER", None, "second-value"), cx)
        .await
        .unwrap();
    assert_eq!(mode(&file), 0o600);
    op(
        Op::Env(EnvOp::Remove {
            name: "TEST_TOKEN".into(),
            project_id: None,
            thread_id: None,
        }),
        cx,
    )
    .await
    .unwrap();
    assert_eq!(mode(&file), 0o600);
}

#[gpui_kit::test]
async fn a_value_never_reaches_a_reply_or_an_error(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let secret = "sk-secret-value-9";
    let done = op(set("TEST_TOKEN", None, secret), cx).await.unwrap();
    assert!(!format!("{done:?}").contains(secret));
    let broken = op(
        set("TEST_TOKEN", None, &format!("{secret}\nsecond line")),
        cx,
    )
    .await
    .unwrap_err();
    assert!(!broken.to_string().contains(secret), "{broken}");
    assert!(!format!("{broken:?}").contains(secret), "{broken:?}");
}

#[gpui_kit::test]
async fn a_thread_cannot_change_a_variable_through_the_op(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let thread = Some(ThreadId("thr_program".into()));
    let set = Op::Env(EnvOp::Set {
        name: "TEST_TOKEN".into(),
        project_id: None,
        value: Some("from-a-thread".into()),
        thread_id: thread.clone(),
    });
    let refused = op(set, cx).await.unwrap_err();
    assert!(matches!(refused, OpError::Invalid(_)), "{refused}");
    let removed = op(
        Op::Env(EnvOp::Remove {
            name: "TEST_TOKEN".into(),
            project_id: None,
            thread_id: thread,
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(removed, OpError::Invalid(_)), "{removed}");
    assert_eq!(
        op(Op::Env(EnvOp::List { project_id: None }), cx)
            .await
            .unwrap(),
        OpResult::Variables(Vec::new())
    );
}

#[gpui_kit::test]
async fn a_corrupt_file_does_not_stop_a_project_removal(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    std::fs::write(
        data_dir(&fixture, cx).join("environment.json"),
        "{ not json",
    )
    .unwrap();
    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::Remove {
                id: fixture.project.clone(),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let (approval, _) = pending_delete(cx);
    cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    let result = remove.await.unwrap();
    let OpResult::Warnings(warnings) = result else {
        panic!("{result:?}")
    };
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("environment.json")),
        "{warnings:?}"
    );
    assert!(cx.read(|cx| {
        fixture
            .workspace
            .read(cx)
            .project(&fixture.project)
            .is_none()
    }));
}

#[gpui_kit::test]
async fn a_corrupt_file_names_its_path_and_a_set_moves_it_aside(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let dir = data_dir(&fixture, cx);
    let file = dir.join("environment.json");
    std::fs::write(&file, "{ not json").unwrap();
    let start = crate::host::variables::for_project(&dir, &fixture.project).unwrap_err();
    assert!(start.to_string().contains("environment.json"), "{start}");
    let OpResult::Warnings(warnings) = op(set("TEST_TOKEN", None, "fresh-value"), cx)
        .await
        .unwrap()
    else {
        panic!("warnings")
    };
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("environment.json.unreadable")),
        "{warnings:?}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("environment.json.unreadable")).unwrap(),
        "{ not json"
    );
    assert_eq!(
        op(Op::Env(EnvOp::List { project_id: None }), cx)
            .await
            .unwrap(),
        OpResult::Variables(vec!["TEST_TOKEN".into()])
    );
    assert!(crate::host::variables::for_project(&dir, &fixture.project).is_ok());
}

#[gpui_kit::test]
async fn removing_a_projects_last_variable_drops_its_entry(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    op(
        set("TEST_TOKEN", Some(fixture.project.clone()), "project-value"),
        cx,
    )
    .await
    .unwrap();
    op(
        Op::Env(EnvOp::Remove {
            name: "TEST_TOKEN".into(),
            project_id: Some(fixture.project.clone()),
            thread_id: None,
        }),
        cx,
    )
    .await
    .unwrap();
    let file = data_dir(&fixture, cx).join("environment.json");
    let stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert!(
        stored["projects"].get(fixture.project.0.as_str()).is_none(),
        "{stored}"
    );
}
