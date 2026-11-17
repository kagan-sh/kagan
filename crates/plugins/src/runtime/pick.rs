use std::future::Future;

use gpui_shell::{HostError, HostModule, HostValue};
use kg_domain::PluginId;

use crate::PluginHost;
use crate::area::{error, to_host};
use crate::ui_host::{self, HostUiEvent};

pub(crate) fn module(plugin: &PluginId) -> HostModule {
    let owner = plugin.clone();
    HostModule::new("kagan/pick")
        .declarations(include_str!("pick.d.ts"))
        .async_function("start", move |_| start(&owner))
}

fn start(
    plugin: &PluginId,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let (reply, replied) = async_channel::bounded(1);
    let plugin = plugin.clone();
    ui_host::app(|ui, cx| {
        if !ui.read(cx).has_window() {
            return Err(HostError::new("kagan has no window open to pick in"));
        }
        let name = PluginHost::try_global(cx)
            .and_then(|host| host.read(cx).plugin_name(&plugin))
            .unwrap_or_else(|| plugin.0.clone());
        ui.update(cx, |_, cx| {
            cx.emit(HostUiEvent::Pick {
                plugin,
                name,
                reply,
            });
        });
        Ok(())
    })?;
    Ok(async move {
        let answer = replied
            .recv()
            .await
            .map_err(|_| HostError::new("the pick ended without an answer"))?;
        match answer.map_err(HostError::new)? {
            Some(pick) => Ok(to_host(serde_json::to_value(pick).map_err(error)?)),
            None => Ok(HostValue::Null),
        }
    })
}
