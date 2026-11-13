use gpui_kit::{ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, UiOp};

use super::activity::{TIMER_ITEM, install_timer_status};
use crate::tests::kagan::{host_then, open, shows, wait_until};

fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    cx.foreground_executor().block_test(task)
}

#[gpui_kit::test]
fn a_plugin_window_opened_before_the_main_window_is_not_window_zero(cx: &mut TestAppContext) {
    let _data = host_then(cx, install_timer_status);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        1,
        "the plugin's hidden window opens before kagan's"
    );
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let main = cx.update(|cx| kagan.read(cx).window_handle());
    wait_until(cx, |cx| shows(cx, main, TIMER_ITEM));

    let Ok(OpResult::Ui(snapshot)) = ui(cx, UiOp::Snapshot { window: 0 }) else {
        panic!("a snapshot of window 0");
    };
    let timer = format!("{:?}", ElementId::from(TIMER_ITEM));
    assert!(
        snapshot.elements.iter().any(|element| element.id == timer),
        "window 0 is the main window: {:?}",
        snapshot.elements.iter().map(|e| &e.id).collect::<Vec<_>>()
    );
    let missing = ui(cx, UiOp::Snapshot { window: 1 });
    assert_eq!(
        missing,
        Err(OpError::NotFound(
            "no window 1; the open windows are numbered 0 to 0".into()
        ))
    );
}

#[gpui_kit::test]
fn a_key_name_the_driver_does_not_know_is_refused(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    let refused = ui(
        cx,
        UiOp::Key {
            keystroke: "notakey".into(),
            window: 0,
        },
    );
    assert!(
        matches!(&refused, Err(OpError::Invalid(message)) if message.contains("notakey")),
        "{refused:?}"
    );
}
