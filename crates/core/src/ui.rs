use std::rc::Rc;

use gpui_kit::{App, AsyncApp, Task, WeakEntity};
use kg_domain::{BrowserOp, OpError, OpResult, SurfaceOp, UI_UNAVAILABLE, UiOp};

use crate::Workspace;
use crate::database::Result;
use crate::workspace::closed;

#[derive(Clone, Debug)]
pub enum UiJob {
    Op(UiOp),
    Surface(SurfaceOp),
    Browser(BrowserOp),
    NewTheme { id: String, from: Option<String> },
}

pub type UiDispatch = Rc<dyn Fn(UiJob, &mut App) -> Task<std::result::Result<OpResult, OpError>>>;

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: UiOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    drive(this, UiJob::Op(op), cx).await
}

pub(crate) async fn drive(
    this: &WeakEntity<Workspace>,
    job: UiJob,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let driver = this
        .update(cx, |workspace, _| workspace.ui.clone())
        .map_err(closed)?;
    match driver {
        Some(driver) => cx.update(|cx| driver(job, cx)).await,
        None => Err(OpError::Failed(UI_UNAVAILABLE.into())),
    }
}
