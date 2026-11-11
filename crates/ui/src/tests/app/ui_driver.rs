use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, UiOp};

use crate::tests::kagan::open;

fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    cx.foreground_executor().block_test(task)
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
