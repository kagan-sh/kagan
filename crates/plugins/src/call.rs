use gpui_kit::{AsyncApp, Entity};
use gpui_shell::{HostArguments, HostError, HostValue};
use kg_domain::{OpError, PluginId};
use serde_json::Value;
use std::future::Future;

use crate::area::{awaited, from_host, to_host, with_app};
use crate::ask::{Job, ask};
use crate::{PluginHost, PluginHostGlobal};

pub(crate) const CALL_CAP: usize = 64 * 1024;

pub(crate) fn call(
    arguments: &HostArguments,
    caller: &PluginId,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let target = PluginId(arguments.string(0)?.to_owned());
    let name = arguments.string(1)?.to_owned();
    let input = from_host(arguments.value(2).unwrap_or(&HostValue::Null));
    let caller = caller.clone();
    let task = with_app(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        let task = cx.spawn(async move |cx| run(&host, &caller, &target, &name, input, cx).await);
        awaited(task, cx)
    })?;
    Ok(async move { task.await?.map(to_host).map_err(HostError::new) })
}

async fn run(
    host: &Entity<PluginHost>,
    caller: &PluginId,
    target: &PluginId,
    name: &str,
    input: Value,
    cx: &mut AsyncApp,
) -> Result<Value, String> {
    if serde_json::to_vec(&input).map_or(true, |bytes| bytes.len() > CALL_CAP) {
        return Err(format!("the input to {target} is larger than 64 KiB"));
    }
    host.read_with(cx, |host, _| host.check_call(caller, target, name))?;
    let job = Job::Call {
        name,
        input: &input,
        caller,
    };
    let value = ask(host, target, &job, cx)
        .await
        .map_err(|error| OpError::from(error).to_string())??;
    if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > CALL_CAP) {
        return Err(format!("the reply from {target} is larger than 64 KiB"));
    }
    Ok(value)
}

impl PluginHost {
    fn check_call(&self, caller: &PluginId, target: &PluginId, name: &str) -> Result<(), String> {
        let approved = self
            .active
            .get(caller)
            .is_some_and(|active| active.grant.powers.calls.contains(target));
        if !approved {
            return Err(format!("plugin {caller} is not approved to call {target}"));
        }
        let Some(active) = self.active.get(target) else {
            return Err(format!("plugin {target} is not enabled"));
        };
        if !active.manifest.exports.iter().any(|export| export == name) {
            return Err(format!("plugin {target} does not export {name}"));
        }
        Ok(())
    }
}
