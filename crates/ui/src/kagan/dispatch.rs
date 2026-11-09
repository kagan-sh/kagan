use gpui_kit::{App, AppContext as _};
use kg_core::Workspace;
use kg_domain::Op;

/// Runs `op` for a control that cannot show the result. A failure goes to stderr, since nothing
/// else would tell the person why the change did not happen.
pub(crate) fn logged(op: Op, cx: &mut App) {
    let task = Workspace::dispatch(op, cx);
    cx.background_spawn(async move {
        if let Err(error) = task.await {
            eprintln!("kg: {error}");
        }
    })
    .detach();
}
