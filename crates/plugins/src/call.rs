use std::future::Future;
use std::time::Duration;

use gpui_kit::{AsyncApp, Entity};
use gpui_shell::{HostArguments, HostError, HostValue};
use kg_domain::PluginId;
use serde_json::{Map, Value};

use crate::area::{from_host, to_host};
use crate::jobs::Ended;
use crate::secret;
use crate::{PluginHost, PluginHostGlobal};

pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const CALL_CAP: usize = 64 * 1024;

pub(crate) fn call(
    arguments: &HostArguments,
    caller: &PluginId,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let target = PluginId(arguments.string(0)?.to_owned());
    let name = arguments.string(1)?.to_owned();
    let input = from_host(arguments.value(2).unwrap_or(&HostValue::Null));
    let caller = caller.clone();
    let receive = gpui_shell::with_current_app(|cx| {
        let (sender, receive) = async_channel::bounded(1);
        let host = cx.global::<PluginHostGlobal>().0.clone();
        cx.spawn(async move |cx| {
            let reply = run(&host, &caller, &target, &name, input, cx).await;
            let _ = sender.send(reply).await;
        })
        .detach();
        receive
    })
    .ok_or_else(|| HostError::new("no kagan app context"))?;
    Ok(async move {
        let value = receive
            .recv()
            .await
            .map_err(|error| HostError::new(error.to_string()))?;
        value.map(to_host).map_err(HostError::new)
    })
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
        return Err(format!("the input to {name} is larger than 64 KiB"));
    }
    host.read_with(cx, |host, _| host.check_call(caller, target, name))?;
    let jobs = host
        .update(cx, |host, cx| host.load(target, cx))
        .map_err(|error| error.to_string())?;
    let job = Map::from_iter([
        ("kind".to_owned(), "call".into()),
        ("name".to_owned(), name.into()),
        ("input".to_owned(), input),
        ("caller".to_owned(), caller.0.clone().into()),
    ]);
    let gone = || format!("plugin {target} was unloaded before it answered {name}");
    let (id, replied) = jobs.send(job).ok_or_else(gone)?;
    let timer = cx.background_executor().timer(CALL_TIMEOUT);
    let reply = match jobs.settled(&id, &replied, timer).await {
        Ok(reply) => reply,
        Err(Ended::Unloaded) => return Err(gone()),
        Err(Ended::TimedOut) => {
            return Err(format!(
                "plugin {target} did not answer {name} within {} s",
                CALL_TIMEOUT.as_secs()
            ));
        }
    };
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    let value = reply.map_err(|message| secrets.redact(target, &message))?;
    let value = redact_value(&secrets, target, value);
    if serde_json::to_vec(&value).map_or(true, |bytes| bytes.len() > CALL_CAP) {
        return Err(format!("the reply from {target} is larger than 64 KiB"));
    }
    Ok(value)
}

fn redact_value(secrets: &secret::Store, plugin: &PluginId, value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(secrets.redact(plugin, &text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_value(secrets, plugin, item))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| {
                    (
                        secrets.redact(plugin, &key),
                        redact_value(secrets, plugin, item),
                    )
                })
                .collect(),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) => value,
    }
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
