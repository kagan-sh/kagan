use gpui_kit::{AppContext as _, TestAppContext};
use kg_core::Workspace;
use kg_domain::{EnvOp, Op, OpResult, ProjectId};

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
    let (kagan, window, _data) = open(cx);
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| {
            kagan.open_project_page(ProjectId("prj_kagan".into()), window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert!(present(cx, window, "project-settings-done"));
    click(cx, window, "env-name");
    cx.simulate_input(window, "TEST_PROJECT");
    click(cx, window, "env-value");
    cx.simulate_input(window, "project-value");
    click(cx, window, "env-save");
    wait_until(cx, |cx| names(cx, Some("prj_kagan")) == ["TEST_PROJECT"]);
    assert!(names(cx, None).is_empty());

    click(cx, window, "project-settings-done");
    cx.run_until_parked();
    assert!(!present(cx, window, "project-settings-done"));
}
