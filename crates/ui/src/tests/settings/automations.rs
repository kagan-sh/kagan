use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::{AppContext as _, TestAppContext, px};
use kg_core::Workspace;
use kg_domain::{AutomationOp, Op, OpResult, ProjectId, ProjectOp, ProjectSection, SettingsOp};

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
                section: None,
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
                in_minutes: Some(1),
            }),
            cx,
        )
    })
    .detach();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_mins(1));
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
    type_into(cx, window, "automation-in", "1");
    assert_eq!(
        value_of(cx, window, "automation-in").as_deref(),
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
    // The core reads the test executor's clock, which can run a second behind the wall clock.
    assert!(
        (before + 59_000..=after + 60_000).contains(&due),
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

// The project menu's Automations item opens the page with its automations in view.
#[gpui_kit::test]
fn the_automations_item_opens_the_project_page_at_its_automations(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Settings(SettingsOp::Project {
                id: ProjectId("prj_kagan".into()),
                section: Some(ProjectSection::Automations),
            }),
            cx,
        )
    })
    .detach();
    cx.run_until_parked();
    cx.run_until_parked();
    assert!(
        in_view(cx, window, "automation-add"),
        "the add form is scrolled into view"
    );
}

fn in_view(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, name: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        let viewport = window.viewport_size();
        crate::ui_registry::find(window, &gpui_kit::ElementId::Name(name.to_owned().into()))
            .is_some_and(|element| {
                element.visible
                    && element.bounds.top() >= px(0.)
                    && element.bounds.bottom() <= viewport.height
            })
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_failed_start_reaches_the_open_project_page_without_a_reopen(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.run_until_parked();
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Automation(AutomationOp::Add {
                name: "Missing".into(),
                body: "ping".into(),
                project_id: ProjectId("prj_kagan".into()),
                program: "kg-no-such-program".into(),
                minutes: None,
                in_minutes: Some(1),
            }),
            cx,
        )
    })
    .detach();
    cx.run_until_parked();
    open_project_page(cx);
    let row = rows(cx).remove(0);
    let threads = format!("automation-threads-{}", row.id);
    cx.executor().advance_clock(Duration::from_secs(61));
    cx.run_until_parked();
    assert!(
        label(cx, window, &threads).starts_with("Last start failed"),
        "{}",
        label(cx, window, &threads)
    );
}
