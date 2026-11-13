use gpui_kit::{AsyncApp, Entity};
use kg_core::Requester;
use kg_domain::{OpError, OpResult, PluginPreviewOp};

use crate::{PluginHost, Result};

pub(in crate::runtime::ops) async fn run(
    _host: &Entity<PluginHost>,
    _op: PluginPreviewOp,
    _requester: Option<Requester>,
    _cx: &mut AsyncApp,
) -> Result<OpResult> {
    Err(OpError::Failed("plugin preview is not built yet".into()))
}
