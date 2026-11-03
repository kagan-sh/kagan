use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::{App, AsyncApp, Task, WeakEntity};
use kg_domain::{DropSide, OpError, OpResult, SurfaceOp, UI_UNAVAILABLE, UiOp};

use crate::Workspace;
use crate::database::Result;
use crate::workspace::closed;

#[derive(Clone, Debug)]
pub enum UiJob {
    Snapshot {
        window: u32,
    },
    Click {
        window: u32,
        element_id: String,
        within: Option<String>,
    },
    Type {
        window: u32,
        text: String,
    },
    Key {
        window: u32,
        keystroke: String,
    },
    Scroll {
        window: u32,
        element_id: String,
        within: Option<String>,
        dy: f32,
    },
    Drag {
        window: u32,
        from: String,
        to: String,
        at: DropSide,
        within: Option<String>,
        hold: bool,
    },
    Drop {
        window: u32,
    },
    Screenshot {
        window: u32,
        path: PathBuf,
    },
    Surface(SurfaceOp),
    NewTheme {
        id: String,
        from: Option<String>,
    },
}

pub type UiDispatch = Rc<dyn Fn(UiJob, &mut App) -> Task<std::result::Result<OpResult, OpError>>>;

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: UiOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let job = match op {
        UiOp::Snapshot { window } => UiJob::Snapshot { window },
        UiOp::Click {
            element_id,
            within,
            window,
        } => UiJob::Click {
            window,
            element_id,
            within,
        },
        UiOp::Type { text, window } => UiJob::Type { window, text },
        UiOp::Key { keystroke, window } => UiJob::Key { window, keystroke },
        UiOp::Scroll {
            element_id,
            dy,
            within,
            window,
        } => UiJob::Scroll {
            window,
            element_id,
            within,
            dy,
        },
        UiOp::Drag {
            from,
            to,
            at,
            within,
            hold,
            window,
        } => UiJob::Drag {
            window,
            from,
            to,
            at,
            within,
            hold,
        },
        UiOp::Drop { window } => UiJob::Drop { window },
        UiOp::Screenshot { path, window } => UiJob::Screenshot { window, path },
    };
    drive(this, job, cx).await
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
