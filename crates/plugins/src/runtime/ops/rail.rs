use gpui_kit::{AsyncApp, Entity};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, PluginId, PreferenceOp, RailAction, prefs};
use serde_json::Value;

use super::record;
use crate::{PluginHost, Result};

// Hiding only moves an icon, and undoing it is one command, so no person is asked.
pub(in crate::runtime::ops) async fn rail(
    host: &Entity<PluginHost>,
    id: PluginId,
    action: RailAction,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    record(host, &id, cx)?;
    edit(&id, action, cx).await
}

// Uninstalling drops the entry. The plugin is already gone by then, so nothing is checked here.
pub(in crate::runtime::ops) async fn forget(id: &PluginId, cx: &mut AsyncApp) -> Result<()> {
    edit(id, RailAction::Show, cx).await.map(drop)
}

async fn edit(id: &PluginId, action: RailAction, cx: &mut AsyncApp) -> Result<OpResult> {
    let read = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: prefs::RAIL_HIDDEN.to_owned(),
            }),
            cx,
        )
    });
    let mut hidden: Vec<PluginId> = match read.await? {
        OpResult::Setting(Value::Null) => Vec::new(),
        OpResult::Setting(value) => serde_json::from_value(value)?,
        _ => return Err(OpError::Failed("rail-hidden could not be read".into())),
    };
    hidden.retain(|hidden| hidden != id);
    if action == RailAction::Hide {
        hidden.push(id.clone());
    }
    let value = serde_json::to_value(&hidden)?;
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: prefs::RAIL_HIDDEN.to_owned(),
                value,
            }),
            cx,
        )
    });
    set.await
}
