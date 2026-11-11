use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{Op, OpResult, Program, ProgramOp, ProgramSource};

use super::picker::{fixture_programs, start_in_plain_folder};
use crate::tests::kagan::{click, label, open, shown, shows, thread_record, wait_until};

fn save(cx: &mut TestAppContext, name: &str, line: &str) {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Program(ProgramOp::Save {
                name: name.into(),
                line: line.into(),
            }),
            cx,
        )
    });
    cx.foreground_executor()
        .block_test(task)
        .expect("the program is saved");
}

fn saved(cx: &mut TestAppContext) -> Vec<Program> {
    let listed =
        cx.update(|cx| Workspace::dispatch(Op::Program(ProgramOp::List { project_id: None }), cx));
    let OpResult::Programs(list) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("programs");
    };
    list.programs
        .into_iter()
        .filter(|program| program.source == ProgramSource::Saved)
        .collect()
}

fn row_id(command: &str) -> String {
    format!("program-saved-{command}")
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_saved_program_is_listed_by_name_and_picking_it_starts_its_line(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    save(cx, "Fast codex", "codex --fast");
    start_in_plain_folder(cx, window, data.path());
    assert_eq!(label(cx, window, &row_id("codex --fast")), "Fast codex");
    click(cx, window, &row_id("codex --fast"));
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(thread_record(cx, &started).provider, "codex --fast");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_custom_command_saved_under_a_name_is_listed_and_the_form_closes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["gemini"]);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    let program = bin.join("gemini").display().to_string();
    cx.simulate_input(window, &program);
    cx.simulate_keystrokes(window, "tab");
    cx.simulate_input(window, "--yolo");
    click(cx, window, "custom-name");
    cx.simulate_input(window, "Gemini, yolo");
    click(cx, window, "saved-program-save");
    let line = format!("{program} --yolo");
    wait_until(cx, |cx| shows(cx, window, &row_id(&line)));
    assert_eq!(label(cx, window, &row_id(&line)), "Gemini, yolo");
    assert!(!shows(cx, window, "custom-program"));
    assert_eq!(saved(cx).len(), 1);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_saved_program_is_pinned_renamed_and_deleted_from_its_row(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    save(cx, "Alpha", "codex --alpha");
    save(cx, "Zulu", "codex --zulu");
    start_in_plain_folder(cx, window, data.path());

    click(cx, window, "program-pin-saved-codex --zulu");
    wait_until(cx, |cx| {
        saved(cx)
            .iter()
            .any(|program| program.label == "Zulu" && program.pinned)
    });
    let names: Vec<_> = saved(cx).into_iter().map(|program| program.label).collect();
    assert_eq!(names, ["Zulu", "Alpha"], "a pinned program is listed first");

    click(cx, window, "program-rename-saved-codex --alpha");
    wait_until(cx, |cx| shows(cx, window, "custom-name"));
    assert!(
        !shows(cx, window, "custom-program"),
        "a rename shows only the name"
    );
    cx.simulate_input(window, " two");
    click(cx, window, "saved-program-save");
    wait_until(cx, |cx| {
        saved(cx).iter().any(|program| program.label == "Alpha two")
    });

    click(cx, window, "program-delete-saved-codex --zulu");
    wait_until(cx, |cx| {
        saved(cx).iter().all(|program| program.label != "Zulu")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_save_that_cannot_work_says_why_and_keeps_the_form(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    click(cx, window, "custom-name");
    cx.simulate_input(window, "Nothing");
    click(cx, window, "saved-program-save");
    wait_until(cx, |cx| shows(cx, window, "saved-program-notice"));
    assert_eq!(
        label(cx, window, "saved-program-notice"),
        "Enter a program to start."
    );
    assert!(shows(cx, window, "custom-program"), "the form stays open");
    assert!(saved(cx).is_empty());
}
