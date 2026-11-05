use super::chord;
use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{InteractionOption, Op, OpError, OpResult, UI_PERSON_ONLY, UiOp};

use crate::kagan::AddProject;

use super::{click, focused_of, open, run_shell, thread_record, wait_until};

#[gpui_kit::test]
fn add_project_opens_the_picker_and_lists_the_folder(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("Fresh folder");
    std::fs::create_dir(&folder).unwrap();

    cx.dispatch_action(window, AddProject);
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.directories && !options.files);
        Some(vec![folder])
    });
    cx.run_until_parked();
    let listed = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .sidebar(cx)
            .projects
            .iter()
            .any(|project| project.name == "Fresh folder")
    });
    assert!(listed);
}

#[gpui_kit::test]
fn synthetic_input_cannot_answer_an_approval(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id
            .0
            .clone()
    });
    let name = format!("answer-{approval}-yes");
    {
        let _armed = crate::synthetic::arm();
        click(cx, window, &name);
        assert!(crate::synthetic::take_person_blocked());
    }
    // A click made while a `kg ui` dispatch runs is synthetic too.
    crate::synthetic::with_active(|| click(cx, window, &name));
    assert!(crate::synthetic::take_person_blocked());
    let still_waiting = cx.update(|cx| Workspace::global(cx).read(cx).approvals().count());
    assert_eq!(still_waiting, 1);
    click(cx, window, &name);
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
}

#[gpui_kit::test]
fn unrelated_input_succeeds_once_while_an_approval_waits(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.run_until_parked();
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let _asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let name = approval_answer(cx);

    // Pinning flips the thread. A retry after a false failure would unpin it, so one success must
    // mean exactly one change.
    assert_eq!(
        ui(
            cx,
            UiOp::Click {
                element_id: format!("thread-{}", id.0),
                within: None,
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(
        ui(
            cx,
            UiOp::Key {
                keystroke: chord("TogglePinned"),
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    wait_until(cx, |cx| thread_record(cx, &id).pinned);
    cx.run_until_parked();
    assert!(thread_record(cx, &id).pinned);
    assert_eq!(
        ui(
            cx,
            UiOp::Key {
                keystroke: chord("ToggleSidebar"),
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(approvals_waiting(cx), 1);
    click(cx, window, &name);
}

#[gpui_kit::test]
fn an_answer_attempt_fails_and_leaves_the_approval_waiting(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
    }];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let name = approval_answer(cx);

    let clicked = ui(
        cx,
        UiOp::Click {
            element_id: name.clone(),
            within: None,
            window: 0,
        },
    );
    assert!(matches!(clicked, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY));
    assert_eq!(approvals_waiting(cx), 1);

    // Tab until the answer button reports focus, then press Enter. Enter must be refused.
    let mut refused_by_keyboard = false;
    for _ in 0..40 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of(cx, window, &name) != Some(true) {
            continue;
        }
        let keyed = ui(
            cx,
            UiOp::Key {
                keystroke: "enter".into(),
                window: 0,
            },
        );
        refused_by_keyboard =
            matches!(keyed, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY);
        break;
    }
    assert!(
        refused_by_keyboard,
        "Enter never reached the approval button"
    );
    assert_eq!(approvals_waiting(cx), 1);

    click(cx, window, &name);
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
}

pub(in crate::tests) fn approval_answer(cx: &mut TestAppContext) -> String {
    let id = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id
            .0
            .clone()
    });
    format!("answer-{id}-yes")
}

pub(in crate::tests) fn approvals_waiting(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).approvals().count())
}

pub(in crate::tests) fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    cx.foreground_executor().block_test(task)
}

#[gpui_kit::test]
fn an_approval_can_be_answered(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let options = vec![
        InteractionOption {
            id: "yes".into(),
            label: "Yes".into(),
        },
        InteractionOption {
            id: "no".into(),
            label: "No".into(),
        },
    ];
    let asked = cx.update(|cx| {
        Workspace::ask_for_person(None, None, "Delete the plugin?".into(), options, cx)
    });
    cx.run_until_parked();
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval waiting")
            .id
            .0
            .clone()
    });
    click(cx, window, &format!("answer-{approval}-yes"));
    assert_eq!(cx.foreground_executor().block_test(asked).unwrap(), "yes");
}
