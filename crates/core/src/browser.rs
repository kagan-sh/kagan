//! The Browser tab's page, reached through the window that owns it.

use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{BrowserOp, OpResult};

use crate::database::Result;
use crate::{UiJob, Workspace};

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: BrowserOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    crate::ui::drive(this, UiJob::Browser(op), cx).await
}
