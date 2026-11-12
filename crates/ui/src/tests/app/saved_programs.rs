#[cfg(unix)]
use gpui_kit::AppContext as _;
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
use crate::tests::kagan::{chord, click, label, open, shows, value_of, wait_until};

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

// Deleting a saved program asks the person first, so the test answers the question.
fn approve_delete(cx: &mut TestAppContext) {
    let id = cx
        .read(|cx| {
            Workspace::global(cx)
                .read(cx)
                .approvals()
                .next()
                .map(|a| a.id.clone())
        })
        .expect("the delete is waiting for the person");
    let answer = cx.update(|cx| Workspace::answer_standalone(id, "approve".into(), cx));
    cx.foreground_executor().block_test(answer).unwrap();
    cx.run_until_parked();
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
    approve_delete(cx);
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
    approve_delete(cx);
    wait_until(cx, |cx| saved(cx).is_empty());
}

#[gpui_kit::test]
fn enter_in_the_rename_box_renames_once_and_leaves_no_error(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Second", "codex --second");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "second");
    click(cx, window, "program-rename-saved-Second");
    wait_until(cx, |cx| shows(cx, window, "custom-name"));
    for _ in 0.."Second".len() {
        cx.simulate_keystrokes(window, "backspace");
    }
    cx.simulate_input(window, "Third");
    cx.simulate_keystrokes(window, "enter enter");
    wait_until(cx, |cx| {
        saved(cx).iter().any(|program| program.label == "Third")
    });
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "saved-program-notice"),
        "a rename that worked leaves no error"
    );
    assert!(!shows(cx, window, "custom-name"), "the rename box closes");
}

#[gpui_kit::test]
fn pinning_keeps_the_selected_row_in_place(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Alpha", "codex --alpha");
    save(cx, "Mid", "codex --mid");
    save(cx, "Zulu", "codex --zulu");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "codex");
    for _ in 0..10 {
        if label(cx, window, &row_id("Zulu")).contains("selected") {
            break;
        }
        cx.simulate_keystrokes(window, "down");
    }
    assert!(label(cx, window, &row_id("Zulu")).contains("selected"));

    click(cx, window, "program-pin-saved-Alpha");
    wait_until(cx, |cx| pinned(cx, "Alpha"));
    cx.run_until_parked();
    assert!(
        label(cx, window, &row_id("Zulu")).contains("selected"),
        "the row the person picked stays selected after a pin"
    );
}

#[gpui_kit::test]
fn a_reopened_picker_starts_with_an_empty_search(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Zulu", "codex --zulu");
    let project = start_in_plain_folder(cx, window, data.path());
    search(cx, window, "zulu");
    wait_until(cx, |cx| {
        value_of(cx, window, "program-search").as_deref() == Some("zulu")
    });
    click(cx, window, &format!("project-new-{}", project.0));
    wait_until(cx, |cx| shows(cx, window, "program-list"));
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "program-search").unwrap_or_default(),
        "",
        "a search from an earlier visit is cleared"
    );
}

// The arrow keys move the grid once it has the focus; the search box keeps Left and Right.
fn focus_grid(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) {
    let picker = cx.update(|cx| kagan.read(cx).picker_entity());
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus_list(window, cx));
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_saved_row_shows_its_buttons_only_while_it_is_selected(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    save(cx, "Alpha", "codex --alpha");
    save(cx, "Zulu", "codex --zulu");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "codex");
    focus_grid(cx, window, &kagan);
    for _ in 0..10 {
        if label(cx, window, &row_id("Zulu")).contains("selected") {
            break;
        }
        cx.simulate_keystrokes(window, "right");
    }
    assert!(label(cx, window, &row_id("Zulu")).contains("selected"));
    assert!(
        shows(cx, window, "program-pin-saved-Zulu"),
        "the selected row shows its buttons"
    );
    assert!(
        !shows(cx, window, "program-pin-saved-Alpha"),
        "a row that is not selected or under the pointer shows none"
    );
}

#[gpui_kit::test]
fn a_saved_program_has_a_letter_mark_like_a_built_in_one(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    save(cx, "Alpha", "codex --alpha");
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "Alpha");
    assert!(
        shows(cx, window, "program-mark-saved-Alpha"),
        "the saved row has its letter mark"
    );
}

// Whether an element lies inside the picker list, top and bottom.
#[cfg(unix)]
fn inside_list(cx: &mut TestAppContext, window: AnyWindowHandle, row: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        let within = crate::ui_registry::find(
            window,
            &gpui_kit::ElementId::Name("program-list".to_owned().into()),
        )
        .expect("the program list");
        let element =
            crate::ui_registry::find(window, &gpui_kit::ElementId::Name(row.to_owned().into()))
                .expect("the row");
        element.bounds.top() >= within.bounds.top()
            && element.bounds.bottom() <= within.bounds.bottom()
    })
    .unwrap()
}

// A long list scrolls. Pinning the picked row with the keyboard moves it to the top and reloads the
// list, and the row stays selected and on screen.
#[cfg(unix)]
#[gpui_kit::test]
fn pinning_the_picked_row_keeps_it_selected_and_on_screen(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    // An installed agent is the default, so a reload that picks the default would show it.
    fixture_programs(cx, data.path(), &["codex"]);
    for n in 0..12 {
        save(cx, &format!("Prog{n:02}"), &format!("codex --p{n:02}"));
    }
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, "codex");
    focus_grid(cx, window, &kagan);
    for _ in 0..40 {
        if label(cx, window, &row_id("Prog10")).contains("selected") {
            break;
        }
        cx.simulate_keystrokes(window, "right");
    }
    cx.run_until_parked();
    assert!(label(cx, window, &row_id("Prog10")).contains("selected"));
    cx.simulate_keystrokes(window, &chord("PinSavedProgram"));
    wait_until(cx, |cx| pinned(cx, "Prog10"));
    cx.run_until_parked();
    assert!(label(cx, window, &row_id("Prog10")).contains("selected"));
    assert!(
        inside_list(cx, window, &row_id("Prog10")),
        "the picked row is still in the visible list"
    );
}
