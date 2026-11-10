use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{EnvOp, Op, OpResult, ProjectId, ProjectOp, SettingsOp};

use crate::tests::kagan::{click, open, present, wait_until};

fn names(cx: &mut TestAppContext, project: Option<&str>) -> Vec<String> {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Env(EnvOp::List {
                project_id: project.map(|id| ProjectId(id.into())),
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Variables(names) => names,
        other => panic!("{other:?}"),
    }
}

fn snapshot(cx: &mut TestAppContext) -> String {
    let task =
        cx.update(|cx| Workspace::dispatch(Op::Ui(kg_domain::UiOp::Snapshot { window: 0 }), cx));
    cx.run_until_parked();
    serde_json::to_string(&cx.foreground_executor().block_test(task).unwrap()).unwrap()
}

#[gpui_kit::test]
fn a_variable_set_in_general_is_listed_by_name_and_its_value_never_shown(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    cx.run_until_parked();
    click(cx, window, "env-name");
    cx.simulate_input(window, "TEST_TOKEN");
    click(cx, window, "env-value");
    cx.simulate_input(window, "sk-ui-5e2b");
    click(cx, window, "env-save");
    wait_until(cx, |cx| names(cx, None) == ["TEST_TOKEN"]);
    assert!(present(cx, window, "env-remove-TEST_TOKEN"));
    assert!(!snapshot(cx).contains("sk-ui-5e2b"));
    let kept = std::fs::read_to_string(data.path().join("environment.json")).unwrap();
    assert!(kept.contains("sk-ui-5e2b"));

    click(cx, window, "env-remove-TEST_TOKEN");
    wait_until(cx, |cx| names(cx, None).is_empty());
}

#[gpui_kit::test]
fn a_project_page_sets_its_own_variables_and_leaves_the_global_ones(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Settings(SettingsOp::Project {
                id: ProjectId("prj_kagan".into()),
            }),
            cx,
        )
    })
    .detach();
    cx.run_until_parked();
    assert!(present(cx, window, "project-settings-close"));
    click(cx, window, "env-name");
    cx.simulate_input(window, "TEST_PROJECT");
    click(cx, window, "env-value");
    cx.simulate_input(window, "project-value");
    click(cx, window, "env-save");
    wait_until(cx, |cx| names(cx, Some("prj_kagan")) == ["TEST_PROJECT"]);
    assert!(names(cx, None).is_empty());

    click(cx, window, "project-settings-close");
    cx.run_until_parked();
    assert!(!present(cx, window, "project-settings-close"));
}

fn open_project_page(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Settings(SettingsOp::Project {
                id: ProjectId("prj_kagan".into()),
            }),
            cx,
        )
    })
    .detach();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn escape_leaves_the_project_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    open_project_page(cx);
    assert!(present(cx, window, "project-settings-close"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!present(cx, window, "project-settings-close"));
}

#[gpui_kit::test]
fn a_stored_program_that_is_not_installed_is_named_on_the_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::SetProgram {
                id: ProjectId("prj_kagan".into()),
                program: Some("gone-tool --fast".into()),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    open_project_page(cx);
    assert!(present(cx, window, "project-program-missing"));
}
