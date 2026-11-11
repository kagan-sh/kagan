use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{
    Op, OpError, OpResult, SavedPrompt, SavedPromptId, SavedPromptOp, SettingsOp, ThreadOp,
    WindowOp,
};

use crate::kagan::Kagan;
use crate::kagan::commands::saved::chords;
use crate::kagan::commands::state as commands_state;
use crate::tests::kagan::{click, open, run_shell, shows, value_of, wait_until};

fn dispatch(cx: &mut TestAppContext, op: Op) -> OpResult {
    outcome(cx, op).unwrap()
}

fn outcome(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let result = cx.foreground_executor().block_test(task);
    cx.run_until_parked();
    result
}

fn query(cx: &mut TestAppContext, window: AnyWindowHandle, kagan: &Entity<Kagan>, text: &str) {
    let state = cx
        .update(|cx| commands_state(kagan.read(cx)))
        .expect("the command list is open");
    cx.update_window(window, |_, window, cx| {
        state.update(cx, |state, cx| state.set_query(text, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
}

fn add(cx: &mut TestAppContext, title: &str, body: &str) -> SavedPrompt {
    match dispatch(
        cx,
        Op::SavedPrompt(SavedPromptOp::Add {
            title: title.into(),
            body: Some(body.into()),
            file: None,
        }),
    ) {
        OpResult::SavedPrompt(prompt) => prompt,
        other => panic!("{other:?}"),
    }
}

fn star_key() -> &'static str {
    chords()[0]
}

fn new_key() -> &'static str {
    chords()[1]
}

fn delete_key() -> &'static str {
    chords()[2]
}

fn save_key() -> &'static str {
    chords()[3]
}

fn row(id: &SavedPromptId) -> String {
    format!("saved-prompt-{}", id.0)
}

#[gpui_kit::test]
fn the_command_list_opens_saved_prompts_and_can_search_star_delete_and_add(
    cx: &mut TestAppContext,
) {
    let (kagan, window, _data) = open(cx);
    let notes = add(cx, "Notes", "remember this");
    let other = add(cx, "Other", "Hello body");
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "saved prompts");
    assert!(shows(cx, window, "command-OpenSavedPrompts"));
    click(cx, window, "command-OpenSavedPrompts");
    assert!(shows(cx, window, &row(&notes.id)));
    assert!(shows(cx, window, &row(&other.id)));
    assert_eq!(
        value_of(cx, window, &format!("saved-prompt-star-{}", notes.id.0)).as_deref(),
        Some("unstarred")
    );
    query(cx, window, &kagan, "hello");
    assert!(!shows(cx, window, &row(&notes.id)));
    assert!(shows(cx, window, &row(&other.id)));
    click(cx, window, &format!("saved-prompt-star-{}", other.id.0));
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, &format!("saved-prompt-star-{}", other.id.0)).as_deref(),
        Some("starred")
    );
    click(cx, window, &format!("saved-prompt-delete-{}", other.id.0));
    cx.run_until_parked();
    assert!(!shows(cx, window, &row(&other.id)));
    query(cx, window, &kagan, "");
    click(cx, window, "saved-prompt-new");
    assert!(shows(cx, window, "saved-prompt-title"));
    click(cx, window, "saved-prompt-title");
    cx.simulate_input(window, "Fresh");
    click(cx, window, "saved-prompt-body");
    cx.simulate_input(window, "fresh body");
    click(cx, window, "saved-prompt-save");
    cx.run_until_parked();
    assert!(has_value(cx, window, "fresh body"));
}

fn has_value(cx: &mut TestAppContext, window: AnyWindowHandle, value: &str) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::snapshots(window)
            .iter()
            .any(|element| element.visible && element.value.as_deref() == Some(value))
    })
    .unwrap()
}

#[gpui_kit::test]
fn star_and_delete_act_on_the_selected_row_from_the_keyboard(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let notes = add(cx, "Notes", "remember this");
    dispatch(cx, Op::Window(WindowOp::SavedPrompts));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, star_key());
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, &format!("saved-prompt-star-{}", notes.id.0)).as_deref(),
        Some("starred")
    );
    cx.simulate_keystrokes(window, delete_key());
    cx.run_until_parked();
    assert!(!shows(cx, window, &row(&notes.id)));
    assert!(cx.update(|cx| commands_state(kagan.read(cx))).is_some());
}

#[gpui_kit::test]
fn new_then_escape_leaves_the_list_open_and_drops_the_draft(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    dispatch(cx, Op::Window(WindowOp::SavedPrompts));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, new_key());
    assert!(shows(cx, window, "saved-prompt-title"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "saved-prompt-title"));
    assert!(cx.update(|cx| commands_state(kagan.read(cx))).is_some());
}

#[gpui_kit::test]
fn the_new_key_mid_draft_keeps_the_typed_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    dispatch(cx, Op::Window(WindowOp::SavedPrompts));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, new_key());
    click(cx, window, "saved-prompt-title");
    cx.simulate_input(window, "Kept");
    cx.simulate_keystrokes(window, new_key());
    cx.run_until_parked();
    assert!(has_value(cx, window, "Kept"));
}

#[gpui_kit::test]
fn the_save_key_adds_the_draft_from_its_body(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    dispatch(cx, Op::Window(WindowOp::SavedPrompts));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, new_key());
    click(cx, window, "saved-prompt-title");
    cx.simulate_input(window, "Keys");
    click(cx, window, "saved-prompt-body");
    cx.simulate_input(window, "typed by key");
    cx.simulate_keystrokes(window, save_key());
    cx.run_until_parked();
    assert!(!shows(cx, window, "saved-prompt-title"));
    assert!(has_value(cx, window, "typed by key"));
    assert!(cx.update(|cx| commands_state(kagan.read(cx))).is_some());
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_sends_the_selected_prompt_into_the_shown_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    add(cx, "Mark", "printf saved-mark");
    dispatch(cx, Op::Window(WindowOp::SavedPrompts));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| screen(cx, &id).contains("saved-mark"));
    assert!(cx.update(|cx| commands_state(kagan.read(cx))).is_none());
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_says_no_thread_is_shown_while_settings_covers_the_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    add(cx, "Away", "not here");
    dispatch(cx, Op::Settings(SettingsOp::Open));
    dispatch(cx, Op::Window(WindowOp::SavedPrompts));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert!(has_value(cx, window, "No thread is shown."));
    assert!(cx.update(|cx| commands_state(kagan.read(cx))).is_some());
}

#[cfg(unix)]
fn screen(cx: &mut TestAppContext, id: &kg_domain::ThreadId) -> String {
    match dispatch(
        cx,
        Op::Thread(ThreadOp::Screen {
            id: id.clone(),
            terminal: None,
            scrollback: true,
            lines: None,
        }),
    ) {
        OpResult::Screen(text) => text,
        other => panic!("{other:?}"),
    }
}
