use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{AutomationOp, Op, OpResult, ProjectId, ProjectOp, SettingsOp, Timestamp};

use crate::tests::kagan::{click, label, open, present, value_of, wait_until};

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

fn type_into(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, field: &str, text: &str) {
    click(cx, window, field);
    cx.simulate_input(window, text);
    cx.run_until_parked();
}

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[gpui_kit::test]
fn the_project_page_counts_the_runs_an_automation_started(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.run_until_parked();
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Automation(AutomationOp::Add {
                name: "Morning".into(),
                body: "ping".into(),
                project_id: ProjectId("prj_kagan".into()),
                program: program().into(),
                minutes: None,
                at: Some(Timestamp(now_ms() + 1_000)),
            }),
            cx,
        )
    })
    .detach();
    wait_until(cx, |cx| {
        rows(cx).first().is_some_and(|row| !row.threads.is_empty())
    });
    let row = rows(cx).remove(0);
    open_project_page(cx);
    let threads = format!("automation-threads-{}", row.id);
    wait_until(cx, |cx| label(cx, window, &threads) == "1 run");
    assert_eq!(row.threads.len(), 1);

    click(cx, window, &format!("automation-pause-{}", row.id));
    wait_until(cx, |cx| rows(cx).first().is_some_and(|row| row.paused));
    assert_eq!(
        label(cx, window, &format!("automation-pause-{}", row.id)),
        "Resume"
    );

    open_project_page(cx);
    let open_last = format!("automation-open-{}", row.id);
    assert!(present(cx, window, &open_last));
    click(cx, window, &open_last);
    let last = row.threads.first().cloned();
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == last
    });
}

#[gpui_kit::test]
fn once_in_saves_a_one_time_automation_that_is_due_in_that_many_minutes(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    open_project_page(cx);
    type_into(cx, window, "automation-name", "Lunch");
    type_into(cx, window, "automation-body", "ping");
    type_into(cx, window, "automation-program", program());
    type_into(cx, window, "automation-at", "1");
    assert_eq!(
        value_of(cx, window, "automation-at").as_deref(),
        Some("1"),
        "the typed minutes reached the field"
    );

    let before = now_ms();
    click(cx, window, "automation-add");
    wait_until(cx, |cx| !rows(cx).is_empty());
    let after = now_ms();
    let row = rows(cx).remove(0);
    assert_eq!(row.minutes, None);
    let due = row.at.expect("a one-time run").0;
    assert!(
        (before + 60_000..=after + 60_000).contains(&due),
        "due at {due}, expected one minute after {before}..{after}"
    );
}

#[gpui_kit::test]
fn an_empty_program_uses_the_project_default(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(ProjectOp::SetProgram {
                id: ProjectId("prj_kagan".into()),
                program: Some(program().into()),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    open_project_page(cx);
    type_into(cx, window, "automation-name", "Morning");
    type_into(cx, window, "automation-body", "ping");
    type_into(cx, window, "automation-minutes", "5");
    click(cx, window, "automation-add");
    wait_until(cx, |cx| !rows(cx).is_empty());
    assert_eq!(rows(cx).remove(0).program, program());
}
