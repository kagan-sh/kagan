use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{AutomationOp, Op, OpResult, ProjectId, SettingsOp, Timestamp};

use crate::tests::kagan::{click, label, open, wait_until};

fn program() -> &'static str {
    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

fn rows(cx: &mut TestAppContext) -> Vec<kg_domain::Automation> {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Automation(AutomationOp::List {
                project_id: Some(ProjectId("prj_kagan".into())),
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Automations(rows) => rows,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
fn the_project_page_counts_the_runs_an_automation_started(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Automation(AutomationOp::Add {
                name: "Morning".into(),
                body: "ping".into(),
                project_id: ProjectId("prj_kagan".into()),
                program: program().into(),
                minutes: None,
                at: Some(Timestamp(1)),
            }),
            cx,
        )
    })
    .detach();
    wait_until(cx, |cx| {
        rows(cx).first().is_some_and(|row| !row.threads.is_empty())
    });
    let row = rows(cx).remove(0);
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
    let threads = format!("automation-threads-{}", row.id);
    wait_until(cx, |cx| label(cx, window, &threads) == "1 run");
    assert_eq!(row.threads.len(), 1);
    click(cx, window, &format!("automation-pause-{}", row.id));
    wait_until(cx, |cx| rows(cx).first().is_some_and(|row| row.paused));
    assert_eq!(
        label(cx, window, &format!("automation-pause-{}", row.id)),
        "Resume"
    );
}
