use gpui_kit::{AnyWindowHandle, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, Program, ProgramOp, ProgramSource};

#[cfg(unix)]
use super::picker::fixture_programs;
use super::picker::start_in_plain_folder;
#[cfg(unix)]
use crate::tests::kagan::shown;
#[cfg(unix)]
use crate::tests::kagan::thread_record;
use crate::tests::kagan::{chord, click, label, open, shows, wait_until};

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

fn pinned(cx: &mut TestAppContext, name: &str) -> bool {
    saved(cx)
        .iter()
        .any(|program| program.label == name && program.pinned)
}

fn row_id(name: &str) -> String {
    format!("program-saved-{name}")
}

// Puts the picker's search on `text`, so the first match is the selected row.
fn search(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    click(cx, window, "program-search");
    cx.simulate_input(window, text);
    cx.run_until_parked();
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_saved_program_is_listed_by_name_and_picking_it_starts_its_line(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    save(cx, "Fast codex", "codex --fast");
    start_in_plain_folder(cx, window, data.path());
    // The label also says ", selected" when the row is the selected one, so only the name is checked.
    assert!(label(cx, window, &row_id("Fast codex")).starts_with("Fast codex"));
    click(cx, window, &row_id("Fast codex"));
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(thread_record(cx, &started).provider, "codex --fast");
}

#[gpui_kit::test]
fn a_custom_command_saved_under_a_name_is_listed_and_the_form_closes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_input(window, "gemini");
    cx.simulate_keystrokes(window, "tab");
    cx.simulate_input(window, "--yolo");
    click(cx, window, "custom-name");
    cx.simulate_input(window, "Gemini, yolo");
    click(cx, window, "saved-program-save");
    cx.run_until_parked();
    // The machine's own agents can fill the list, so a saved program is found by its name.
    search(cx, window, "Gemini");
    wait_until(cx, |cx| shows(cx, window, &row_id("Gemini, yolo")));
    assert!(label(cx, window, &row_id("Gemini, yolo")).starts_with("Gemini, yolo"));
    assert!(!shows(cx, window, "custom-program"));
    assert_eq!(saved(cx).len(), 1);
}

// Two names that share one line get their own rows, ids and buttons.
#[gpui_kit::test]
fn two_names_sharing_a_line_each_pin_rename_and_delete_their_own_row(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Alpha", "codex --shared");
    save(cx, "Zulu", "codex --shared");
    start_in_plain_folder(cx, window, data.path());
    // Both rows share a line, so the search keeps both on screen, below the agents.
    search(cx, window, "shared");
    // The selected row's label also says ", selected", so only its start is the name.
    assert!(label(cx, window, &row_id("Alpha")).starts_with("Alpha"));
    assert!(label(cx, window, &row_id("Zulu")).starts_with("Zulu"));

    click(cx, window, "program-pin-saved-Zulu");
    wait_until(cx, |cx| pinned(cx, "Zulu"));
    assert!(!pinned(cx, "Alpha"), "pinning one name leaves the other");

    click(cx, window, "program-rename-saved-Alpha");
    wait_until(cx, |cx| shows(cx, window, "custom-name"));
    cx.simulate_input(window, " two");
    click(cx, window, "saved-program-save");
    wait_until(cx, |cx| {
        saved(cx).iter().any(|program| program.label == "Alpha two")
    });

    click(cx, window, "program-delete-saved-Zulu");
    wait_until(cx, |cx| {
        saved(cx).iter().all(|program| program.label != "Zulu")
    });
    assert_eq!(saved(cx).len(), 1);
}

#[gpui_kit::test]
fn a_save_without_a_program_points_at_the_program_field(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    click(cx, window, "custom-name");
    cx.simulate_input(window, "Nothing");
    click(cx, window, "saved-program-save");
    cx.run_until_parked();
    assert_eq!(
        label(cx, window, "program-feedback"),
        "Enter a program to start."
    );
    assert!(shows(cx, window, "custom-program"), "the form stays open");
    assert!(saved(cx).is_empty());
}

// The server's refusal shows as a notice, and typing in the name clears it.
#[gpui_kit::test]
fn a_refused_save_says_why_and_the_notice_clears_when_the_name_changes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_input(window, "codex");
    click(cx, window, "saved-program-save");
    wait_until(cx, |cx| shows(cx, window, "saved-program-notice"));
    assert_eq!(
        label(cx, window, "saved-program-notice"),
        "a saved program needs a name"
    );
    click(cx, window, "custom-name");
    cx.simulate_input(window, "Named");
    wait_until(cx, |cx| !shows(cx, window, "saved-program-notice"));
    assert!(saved(cx).is_empty());
}

#[gpui_kit::test]
fn the_save_chord_saves_the_custom_command_under_its_name(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_input(window, "gemini");
    click(cx, window, "custom-name");
    cx.simulate_input(window, "Keyed");
    cx.simulate_keystrokes(window, &chord("SaveSavedProgram"));
    wait_until(cx, |cx| {
        saved(cx).iter().any(|program| program.label == "Keyed")
    });
}

#[gpui_kit::test]
fn the_pin_chord_pins_the_selected_saved_program_only(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Zulu", "codex --zulu");
    save(cx, "Alpha", "codex --alpha");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "Zulu");
    cx.simulate_keystrokes(window, &chord("PinSavedProgram"));
    wait_until(cx, |cx| pinned(cx, "Zulu"));
    assert!(!pinned(cx, "Alpha"));
}

#[gpui_kit::test]
fn the_pin_chord_does_nothing_on_a_program_that_is_not_saved(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Zulu", "codex --zulu");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "codex");
    cx.simulate_keystrokes(window, &chord("PinSavedProgram"));
    cx.run_until_parked();
    assert!(!pinned(cx, "Zulu"));
}

#[gpui_kit::test]
fn the_rename_chord_renames_the_selected_saved_program(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Zulu", "codex --zulu");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "Zulu");
    cx.simulate_keystrokes(window, &chord("RenameSavedProgram"));
    wait_until(cx, |cx| shows(cx, window, "custom-name"));
    cx.simulate_input(window, " two");
    cx.simulate_keystrokes(window, &chord("SaveSavedProgram"));
    wait_until(cx, |cx| {
        saved(cx).iter().any(|program| program.label == "Zulu two")
    });
}

#[gpui_kit::test]
fn the_delete_chord_deletes_the_selected_saved_program(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Zulu", "codex --zulu");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "Zulu");
    cx.simulate_keystrokes(window, &chord("DeleteSavedProgram"));
    wait_until(cx, |cx| saved(cx).is_empty());
}
