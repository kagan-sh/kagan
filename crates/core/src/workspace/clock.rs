use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::{App, AsyncApp, WeakEntity};
use kg_domain::OpError;

use crate::host::database::{Database, Result};

use super::Workspace;

pub(crate) fn now() -> kg_domain::Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    kg_domain::Timestamp(i64::try_from(millis).unwrap_or(i64::MAX))
}

pub(crate) fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}
pub(crate) fn closed(error: impl std::fmt::Display) -> OpError {
    OpError::Failed(error.to_string())
}
pub(crate) fn missing(what: &str, id: &impl std::fmt::Display) -> OpError {
    OpError::NotFound(format!("{what} {id} not found"))
}
pub(crate) fn changed(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Result<()> {
    this.update(cx, |_, cx| {
        cx.emit(super::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)
}
pub(crate) fn database(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Result<Database> {
    this.update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)
}

pub(crate) fn time(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<kg_domain::Timestamp> {
    this.update(cx, |workspace, cx| workspace.clock_now(cx))
        .map_err(closed)
}

impl Workspace {
    pub fn clock_now(&self, cx: &App) -> kg_domain::Timestamp {
        let elapsed = cx.background_executor().now().duration_since(self.clock.1);
        let millis = i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX);
        kg_domain::Timestamp(self.clock.0.0.saturating_add(millis))
    }
}
