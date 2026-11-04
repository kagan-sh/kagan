use gpui_kit::{AppContext as _, Task, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{AskField, InteractionOption, Op, OpError, OpResult, ThreadOp};

use crate::tests::app::picker::start_in_plain_folder;
use crate::tests::kagan::{
    chord, click, focused_of, open, press_key, repo_thread, run_shell, set_preference, shows,
    value_of, wait_until,
};

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
            .id
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
    cx.run_until_parked();
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
    cx.run_until_parked();
    let (present, gone) = async_channel::bounded::<()>(1);
    let deleting = cx.update(|cx| {
        Workspace::dispatch_for(
            Op::Thread(ThreadOp::Delete {
                id: id.clone(),
                approval_thread_id: Some(id.clone()),
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
    cx.run_until_parked();
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
    cx.run_until_parked();
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
    cx.run_until_parked();
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
    cx.run_until_parked();
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
    cx.run_until_parked();
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

#[gpui_kit::test]
fn an_ask_with_fields_opens_a_popup_that_escape_dismisses(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let ask = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Ship it?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
                fields: Some(vec![AskField::Boolean {
                    id: "notify".into(),
                    label: "Notify".into(),
                    default: Some(true),
                }]),
            }),
            cx,
        )
    });
    let OpResult::Text(interaction) = cx.foreground_executor().block_test(ask).unwrap() else {
        panic!("ask");
    };
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    assert!(shows(cx, window, "ask-popup"));
    assert!(
        shows(cx, window, &format!("ask-field-{interaction}-notify")),
        "the popup states its fields for kg ui"
    );
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "ask-popup"));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx)) == ["Ship it?"]
    });
    assert!(!shows(cx, window, "ask-popup"));
    wait_until(cx, |cx| {
        shows(cx, window, &format!("question-{interaction}"))
    });
    wait_until(cx, |cx| focused_of(cx, window, "terminal") == Some(true));
}

#[gpui_kit::test]
fn answering_an_ask_form_restores_terminal_focus(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let ask = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Ship it?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
                fields: Some(vec![AskField::Number {
                    id: "retries".into(),
                    label: "Retries".into(),
                    default: Some(1),
                }]),
            }),
            cx,
        )
    });
    let OpResult::Text(interaction) = cx.foreground_executor().block_test(ask).unwrap() else {
        panic!("ask");
    };
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    click(cx, window, &format!("answer-{interaction}-yes"));
    wait_until(cx, |cx| !shows(cx, window, "ask-popup"));
    wait_until(cx, |cx| focused_of(cx, window, "terminal") == Some(true));
}

#[gpui_kit::test]
fn a_bad_number_in_the_ask_form_shows_on_the_card(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let ask = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Ship it?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
                fields: Some(vec![AskField::Number {
                    id: "retries".into(),
                    label: "Retries".into(),
                    default: Some(1),
                }]),
            }),
            cx,
        )
    });
    let OpResult::Text(interaction) = cx.foreground_executor().block_test(ask).unwrap() else {
        panic!("ask");
    };
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let field = format!("ask-field-{interaction}-retries");
    click(cx, window, &field);
    cx.simulate_keystrokes(window, "backspace");
    cx.simulate_input(window, "nope");
    cx.run_until_parked();
    click(cx, window, &format!("answer-{interaction}-yes"));
    wait_until(cx, |cx| {
        shows(
            cx,
            window,
            &format!("ask-field-error-{interaction}-retries"),
        )
    });
    assert!(
        shows(cx, window, "ask-popup"),
        "the popup stays for the error"
    );
}

#[gpui_kit::test]
fn a_select_in_the_ask_form_picks_none_from_its_menu(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let ask = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Ship it?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                    },
                ],
                fields: Some(vec![AskField::Select {
                    id: "mode".into(),
                    label: "Mode".into(),
                    choices: vec![InteractionOption {
                        id: "fast".into(),
                        label: "Fast".into(),
                    }],
                    default: Some("fast".into()),
                    required: false,
                }]),
            }),
            cx,
        )
    });
    let OpResult::Text(interaction) = cx.foreground_executor().block_test(ask).unwrap() else {
        panic!("ask");
    };
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let select = format!("ask-select-{interaction}-mode");
    assert_eq!(value_of(cx, window, &select), Some("Fast".into()));
    click(cx, window, &select);
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-none-{interaction}-mode"))
    });
    click(cx, window, &format!("ask-none-{interaction}-mode"));
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, &select), Some("None".into()));
}

#[gpui_kit::test]
fn an_install_approval_does_not_open_the_ask_popup(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let _ = run_shell(cx);
    let asked = ask_install(cx);
    cx.run_until_parked();
    assert!(!shows(cx, window, "ask-popup"));
    decline(cx, asked);
}

fn yes_no() -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: "yes".into(),
            label: "Yes".into(),
        },
        InteractionOption {
            id: "no".into(),
            label: "No".into(),
        },
    ]
}

/// Asks with fields written as JSON, so these tests state the wire shape from decision 133.
fn ask_fields(
    cx: &mut TestAppContext,
    thread: &kg_domain::ThreadId,
    fields: serde_json::Value,
) -> String {
    let fields: Vec<AskField> = serde_json::from_value(fields).expect("ask fields");
    let ask = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: thread.clone(),
                title: "Ship it?".into(),
                options: yes_no(),
                fields: Some(fields),
            }),
            cx,
        )
    });
    let OpResult::Text(interaction) = cx.foreground_executor().block_test(ask).unwrap() else {
        panic!("ask");
    };
    cx.run_until_parked();
    interaction
}

/// The bounds `kg ui` reports for an element: x, y, width and height, in points.
fn bounds(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, name: &str) -> [f32; 4] {
    cx.update_window(window, |_, window, _| {
        let element =
            crate::ui_registry::find(window, &gpui_kit::ElementId::Name(name.to_owned().into()))
                .unwrap_or_else(|| panic!("{name} is not drawn"));
        let bounds = element.bounds();
        [
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ]
    })
    .unwrap()
}

fn thirty_two_choices() -> serde_json::Value {
    let choices: Vec<_> = (0..32)
        .map(|n| serde_json::json!({ "id": format!("c{n:02}"), "label": format!("Choice {n:02}") }))
        .collect();
    serde_json::json!([{ "type": "select", "id": "mode", "label": "Mode", "choices": choices }])
}

#[gpui_kit::test]
fn typing_stays_in_the_ask_popup_after_reselecting_and_switching_threads(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    let number = serde_json::json!([{ "type": "number", "id": "retries", "label": "Retries" }]);
    click(cx, window, &format!("thread-{}", first.0));
    ask_fields(cx, &first, number.clone());
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    click(cx, window, &format!("thread-{}", first.0));
    assert_eq!(focused_of(cx, window, "ask-popup"), Some(true));
    cx.simulate_input(window, "7");
    assert_ne!(focused_of(cx, window, "terminal"), Some(true));
    let second_ask = ask_fields(cx, &second, number);
    click(cx, window, &format!("thread-{}", second.0));
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-field-{second_ask}-retries"))
    });
    assert_eq!(focused_of(cx, window, "ask-popup"), Some(true));
    cx.simulate_input(window, "7");
    assert_ne!(focused_of(cx, window, "terminal"), Some(true));
}

#[gpui_kit::test]
fn an_ask_form_arriving_in_full_screen_questions_shows_its_card(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1280.), px(800.)));
    cx.run_until_parked();
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    set_preference(cx, "main.right-sidebar", serde_json::json!(true));
    cx.run_until_parked();
    let asked = ask_install(cx);
    cx.run_until_parked();
    click(cx, window, "right-full");
    cx.run_until_parked();
    let interaction = ask_fields(
        cx,
        &id,
        serde_json::json!([{ "type": "boolean", "id": "notify", "label": "Notify" }]),
    );
    assert!(!shows(cx, window, "ask-popup"));
    wait_until(cx, |cx| {
        shows(cx, window, &format!("question-{interaction}"))
    });
    decline(cx, asked);
}

#[gpui_kit::test]
fn a_long_menu_reaches_its_last_choice_by_pointer(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let interaction = ask_fields(cx, &id, thirty_two_choices());
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let select = format!("ask-select-{interaction}-mode");
    click(cx, window, &select);
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-choice-{interaction}-mode-c00"))
    });
    let first = gpui_kit::ElementId::Name(format!("ask-choice-{interaction}-mode-c00").into());
    cx.update_window(window, |_, window, cx| {
        let wheel = gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-2000.)));
        crate::ui_dispatch::scroll_within(window, &first, None, wheel, cx).unwrap();
    })
    .unwrap();
    click(cx, window, &format!("ask-choice-{interaction}-mode-c31"));
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, &select), Some("Choice 31".into()));
}

#[gpui_kit::test]
fn a_long_menu_reaches_its_last_choice_by_keyboard(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let interaction = ask_fields(cx, &id, thirty_two_choices());
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let select = format!("ask-select-{interaction}-mode");
    click(cx, window, &select);
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-choice-{interaction}-mode-c00"))
    });
    for _ in 0..33 {
        press_key(cx, window, "down");
    }
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-choice-{interaction}-mode-c31"))
    });
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, &select), Some("Choice 31".into()));
}

#[gpui_kit::test]
fn a_128_character_choice_stays_inside_the_ask_popup(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let unbroken = "a".repeat(128);
    let spaced = format!("{}ab", "ab ".repeat(42));
    assert_eq!(spaced.chars().count(), 128);
    let interaction = ask_fields(
        cx,
        &id,
        serde_json::json!([{
            "type": "select",
            "id": "mode",
            "label": "Mode",
            "choices": [
                { "id": "unbroken", "label": unbroken },
                { "id": "spaced", "label": spaced },
                { "id": "short", "label": "Short" }
            ]
        }]),
    );
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let card = bounds(cx, window, &format!("question-{interaction}"));
    let select = bounds(cx, window, &format!("ask-select-{interaction}-mode"));
    assert!(
        select[2] <= card[2],
        "the select is {} wide, the popup {} wide",
        select[2],
        card[2]
    );
    click(cx, window, &format!("ask-select-{interaction}-mode"));
    wait_until(cx, |cx| {
        shows(
            cx,
            window,
            &format!("ask-choice-{interaction}-mode-unbroken"),
        )
    });
    let item = bounds(
        cx,
        window,
        &format!("ask-choice-{interaction}-mode-unbroken"),
    );
    assert!(
        item[0] + item[2] <= card[0] + card[2],
        "the 128-character choice runs past the popup: {item:?} in {card:?}"
    );
    let short = bounds(cx, window, &format!("ask-choice-{interaction}-mode-short"));
    for (name, wrapped) in [
        ("unbroken", item),
        (
            "spaced",
            bounds(cx, window, &format!("ask-choice-{interaction}-mode-spaced")),
        ),
    ] {
        assert!(
            wrapped[3] > short[3],
            "the {name} label is one line tall, {wrapped:?}, so it is cut off; the short one is {short:?}"
        );
    }
}

#[gpui_kit::test]
fn an_answer_over_the_length_cap_shows_beside_its_field(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let interaction = ask_fields(
        cx,
        &id,
        serde_json::json!([{ "type": "string", "id": "note", "label": "Note" }]),
    );
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let field = format!("ask-field-{interaction}-note");
    click(cx, window, &field);
    cx.simulate_input(window, &"x".repeat(4097));
    cx.run_until_parked();
    click(cx, window, &format!("answer-{interaction}-yes"));
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-field-error-{interaction}-note"))
    });
    assert!(
        shows(cx, window, "ask-popup"),
        "the popup stays open for the fix"
    );
    assert!(
        !shows(cx, window, "note"),
        "the error does not go behind the popup"
    );
    click(cx, window, &field);
    cx.simulate_keystrokes(window, "backspace");
    cx.run_until_parked();
    click(cx, window, &format!("answer-{interaction}-yes"));
    wait_until(cx, |cx| !shows(cx, window, "ask-popup"));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).question_titles(cx))
            .is_empty()
    });
}

#[gpui_kit::test]
fn a_required_select_has_no_none_item(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let interaction = ask_fields(
        cx,
        &id,
        serde_json::json!([{
            "type": "select",
            "id": "mode",
            "label": "Mode",
            "required": true,
            "choices": [{ "id": "fast", "label": "Fast" }]
        }]),
    );
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    let select = format!("ask-select-{interaction}-mode");
    assert_eq!(value_of(cx, window, &select), Some("Fast".into()));
    click(cx, window, &select);
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-choice-{interaction}-mode-fast"))
    });
    assert!(!shows(cx, window, &format!("ask-none-{interaction}-mode")));
}

fn focus_inside_popup(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    interaction: &str,
) -> bool {
    [
        "ask-popup".to_owned(),
        format!("ask-field-{interaction}-retries"),
        format!("answer-{interaction}-yes"),
        format!("answer-{interaction}-no"),
    ]
    .iter()
    .any(|name| focused_of(cx, window, name) == Some(true))
}

#[gpui_kit::test]
fn tab_and_shift_tab_stay_in_the_ask_popup(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let interaction = ask_fields(
        cx,
        &id,
        serde_json::json!([{ "type": "number", "id": "retries", "label": "Retries" }]),
    );
    wait_until(cx, |cx| shows(cx, window, "ask-popup"));
    for _ in 0..6 {
        press_key(cx, window, "tab");
        assert!(
            focus_inside_popup(cx, window, &interaction),
            "Tab left the ask popup"
        );
    }
    for _ in 0..6 {
        press_key(cx, window, "shift-tab");
        assert!(
            focus_inside_popup(cx, window, &interaction),
            "Shift-Tab left the ask popup"
        );
    }
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "ask-popup"));
    wait_until(cx, |cx| focused_of(cx, window, "terminal") == Some(true));
}

#[gpui_kit::test]
fn an_ask_does_not_cover_a_file_review_of_its_thread(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| shows(cx, window, "review"));
    assert!(
        !shows(cx, window, "terminal"),
        "the review covers the terminal"
    );
    ask_fields(
        cx,
        &thread,
        serde_json::json!([{ "type": "boolean", "id": "notify", "label": "Notify" }]),
    );
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "ask-popup"),
        "the popup covers the review"
    );
    assert_eq!(
        cx.update(|cx| kagan.read(cx).question_titles(cx)),
        ["Ship it?"]
    );
}

#[gpui_kit::test]
fn dismissing_one_ask_keeps_another_dismissed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    let second = run_shell(cx);
    let notify = serde_json::json!([{ "type": "boolean", "id": "notify", "label": "Notify" }]);
    click(cx, window, &format!("thread-{}", first.0));
    let first_ask = ask_fields(cx, &first, notify.clone());
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-field-{first_ask}-notify"))
    });
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "ask-popup"));
    click(cx, window, &format!("thread-{}", second.0));
    let second_ask = ask_fields(cx, &second, notify);
    wait_until(cx, |cx| {
        shows(cx, window, &format!("ask-field-{second_ask}-notify"))
    });
    press_key(cx, window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "ask-popup"));
    click(cx, window, &format!("thread-{}", first.0));
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "ask-popup"),
        "the first ask reopened after its dismissal"
    );
}
