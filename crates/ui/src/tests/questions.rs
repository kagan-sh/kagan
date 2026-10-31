use gpui_kit::{AppContext as _, Task, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{InteractionOption, Op, OpError, ThreadOp};

use super::kagan::{
    chord, click, focused_of, open, repo_thread, run_shell, set_preference, shows, wait_until,
};
use super::picker::start_in_plain_folder;
use super::sidebars::settle;

fn install_options() -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: "approve".into(),
            label: "Install".into(),
        },
        InteractionOption {
            id: "decline".into(),
            label: "Cancel".into(),
        },
    ]
}

fn approvals(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).approvals().count())
}

fn first_approval(cx: &mut TestAppContext) -> kg_domain::PendingInteractionId {
    cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("an approval")
            .id()
            .clone()
    })
}

fn ask_install(cx: &mut TestAppContext) -> Task<Result<String, OpError>> {
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Install hello?".into(), install_options(), cx)
    });
    wait_until(cx, |cx| approvals(cx) == 1);
    asked
}

fn decline(cx: &mut TestAppContext, asked: Task<Result<String, OpError>>) {
    let id = first_approval(cx);
    let answered = cx.update(|cx| Workspace::answer_standalone(id, "decline".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    assert_eq!(
        cx.foreground_executor().block_test(asked).unwrap(),
        "decline"
    );
    wait_until(cx, |cx| approvals(cx) == 0);
}

#[gpui_kit::test]
fn a_person_question_opens_the_panels_and_keeps_the_typing_focus(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    settle(cx);
    let picker = cx.update(|cx| kagan.read(cx).picker_entity());
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(focused_of(cx, window, "program-search"), Some(true));
    assert!(!shows(cx, window, "needs-you-header"));
    let asked = ask_install(cx);
    assert!(shows(cx, window, "needs-you-header"));
    assert_eq!(focused_of(cx, window, "program-search"), Some(true));
    decline(cx, asked);
    assert!(!shows(cx, window, "needs-you-header"));
}

#[gpui_kit::test]
fn a_stopped_command_takes_its_thread_question_and_the_overlay_away(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    settle(cx);
    let (present, gone) = async_channel::bounded::<()>(1);
    let deleting = cx.update(|cx| {
        Workspace::dispatch_for(
            Op::Thread(ThreadOp::Delete {
                id: id.clone(),
                thread_id: Some(id.clone()),
            }),
            Some(kg_core::Requester::new(gone)),
            cx,
        )
    });
    wait_until(cx, |cx| shows(cx, window, "right-overlay-panel"));
    assert!(
        !cx.update(|cx| kagan.read(cx).question_titles(cx))
            .is_empty()
    );
    drop(present);
    settle(cx);
    assert!(
        cx.update(|cx| kagan.read(cx).question_titles(cx))
            .is_empty()
    );
    assert!(!shows(cx, window, "needs-you-header"));
    assert!(!shows(cx, window, "right-overlay-panel"));
    assert!(cx.foreground_executor().block_test(deleting).is_err());
}

#[gpui_kit::test]
fn closing_the_last_question_restores_focus_from_the_panels(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    settle(cx);
    let picker = cx.update(|cx| kagan.read(cx).picker_entity());
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    let asked = ask_install(cx);
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("ToggleRightSidebar"));
    cx.run_until_parked();
    let id = first_approval(cx);
    assert_eq!(
        focused_of(cx, window, &format!("answer-{}-decline", id.0)),
        Some(true)
    );
    decline(cx, asked);
    wait_until(cx, |cx| !shows(cx, window, "right-overlay-panel"));
    assert_eq!(focused_of(cx, window, "program-search"), Some(true));
}

#[gpui_kit::test]
fn a_question_replaces_the_left_overlay_and_restores_focus(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    settle(cx);
    let picker = cx.update(|cx| kagan.read(cx).picker_entity());
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    let asked = ask_install(cx);
    wait_until(cx, |cx| shows(cx, window, "right-overlay-panel"));
    assert!(!shows(cx, window, "left-overlay-panel"));
    assert_eq!(focused_of(cx, window, "program-search"), Some(true));
    decline(cx, asked);
}

#[gpui_kit::test]
fn a_question_that_arrives_in_review_shows_when_the_review_closes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    cx.simulate_window_resize(window, size(px(480.), px(720.)));
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "changes-open-review");
    wait_until(cx, |cx| shows(cx, window, "review"));
    let asked = ask_install(cx);
    assert!(!shows(cx, window, "needs-you-header"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "needs-you-header"));
    decline(cx, asked);
}

#[gpui_kit::test]
fn a_question_that_arrives_in_settings_shows_when_settings_closes(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    settle(cx);
    click(cx, window, "settings-gear");
    let asked = ask_install(cx);
    assert!(!shows(cx, window, "needs-you-header"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "needs-you-header"));
    decline(cx, asked);
}

#[gpui_kit::test]
fn an_install_card_focuses_cancel(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let asked = ask_install(cx);
    let id = first_approval(cx);
    set_preference(cx, "main.right-sidebar", serde_json::json!(true));
    cx.simulate_window_resize(window, size(px(800.), px(720.)));
    settle(cx);
    click(cx, window, "right-sidebar-toggle");
    assert_eq!(
        focused_of(cx, window, &format!("answer-{}-decline", id.0)),
        Some(true)
    );
    assert_ne!(
        focused_of(cx, window, &format!("answer-{}-approve", id.0)),
        Some(true)
    );
    decline(cx, asked);
}
