use std::path::PathBuf;

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_core::{Requester, Workspace};
use kg_domain::{ConfigOp, OpError, OpResult, PluginId, ProjectId, ThreadId};
use serde_json::Value;

use crate::manifest::{AgentChoice, Setting};
use crate::{PluginHost, Result};

use super::approve::approve_folder;
use super::{approved, record};

pub(in crate::ops) async fn config(
    host: &Entity<PluginHost>,
    id: &PluginId,
    op: ConfigOp,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let record = record(host, id, cx)?;
    let key = match &op {
        ConfigOp::Get { key } | ConfigOp::Set { key, .. } | ConfigOp::Clear { key } => key.clone(),
    };
    let found = host.read_with(cx, |host, _| {
        host.activation(id).map(|activation| {
            let active = host.active.get(id)?;
            Some((
                active.manifest.name.clone(),
                active.manifest.settings.get(&key).cloned(),
                host.value(id, &key),
                activation,
                host.permit(id)?,
            ))
        })
    });
    let Some((name, setting, current, activation, permit)) = found.flatten() else {
        return Err(OpError::Invalid(format!(
            "plugin {} is disabled; enable it to change its settings",
            record.id
        )));
    };
    let setting =
        setting.ok_or_else(|| OpError::Invalid(format!("plugin {id} has no setting {key}")))?;
    match op {
        ConfigOp::Get { .. } => Ok(OpResult::Setting(current.unwrap_or(Value::Null))),
        ConfigOp::Set { value: None, .. } => Err(OpError::Invalid(format!(
            "setting {key} needs a value; kg reads it from a prompt or stdin when you leave it out"
        ))),
        ConfigOp::Set {
            value: Some(value), ..
        } if matches!(setting, Setting::Secret { .. }) => {
            let secret = checked(&key, &setting, value)?;
            keep_secret(host, id, &key, secret.as_str(), cx).await
        }
        ConfigOp::Clear { .. } if matches!(setting, Setting::Secret { .. }) => {
            keep_secret(host, id, &key, None, cx).await
        }
        ConfigOp::Clear { .. } if matches!(setting, Setting::Project { .. }) => {
            write_setting(host, id, &key, Value::Null, None, (activation, permit), cx).await
        }
        ConfigOp::Clear { .. } => Err(OpError::Invalid(format!(
            "setting {key} is not a secret or project; set it instead"
        ))),
        ConfigOp::Set {
            value: Some(value), ..
        } => {
            let value = checked(&key, &setting, value)?;
            if matches!(setting, Setting::Project { .. }) {
                let Some(id_text) = value.as_str() else {
                    return Err(OpError::Invalid(format!(
                        "setting {key} ({}) takes a project id",
                        setting.title()
                    )));
                };
                let project = ProjectId(id_text.into());
                let workspace = cx.update(|cx| Workspace::global(cx));
                let found =
                    workspace.read_with(cx, |workspace, _| workspace.project(&project).is_some());
                if !found {
                    return Err(OpError::Invalid(format!("no project {project}")));
                }
            }
            let asked = format!("Let {name} ({id}) read a folder?");
            let folder = approve_folder(&setting, &value, asked, thread_id, requester, cx).await?;
            host.read_with(cx, |host, _| stage(host, id, &key, folder.as_ref()))?;
            write_setting(
                host,
                id,
                &key,
                value,
                folder.as_ref(),
                (activation, permit),
                cx,
            )
            .await
        }
    }
}

pub(in crate::ops) async fn write_setting(
    host: &Entity<PluginHost>,
    id: &PluginId,
    key: &str,
    value: Value,
    folder: Option<&PathBuf>,
    (activation, permit): (u64, std::sync::Arc<std::sync::Mutex<Option<u64>>>),
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if host.read_with(cx, |host, _| host.activation(id)) != Some(activation) {
        return Err(OpError::Invalid(format!(
            "plugin {id} changed while this was saving"
        )));
    }
    #[cfg(test)]
    if let Some(hold) = host.update(cx, |host, _| host.commit_hold.take()) {
        let _ = hold.recv().await;
    }
    let workspace = cx.update(|cx| Workspace::global(cx));
    #[cfg(test)]
    if let Some(hold) = host.update(cx, |host, _| host.write_hold.take()) {
        let _ = hold.recv().await;
    }
    workspace
        .read_with(cx, |workspace, _| {
            workspace.commit_plugin_config(
                id.clone(),
                key.to_owned(),
                value.clone(),
                folder.map(|path| path.display().to_string()),
                kg_core::Permit::new(activation, permit),
            )
        })
        .await?;
    host.update(cx, |host, cx| {
        remember_setting(host, id, key, &value, folder, activation, cx)
    })?;
    Ok(OpResult::Done)
}

/// Writes or clears a secret in the store, never in the database, and returns the store's
/// warning when it fell back to a file.
pub(in crate::ops) async fn keep_secret(
    host: &Entity<PluginHost>,
    id: &PluginId,
    key: &str,
    secret: Option<&str>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let store = host.read_with(cx, |host, _| host.secrets.clone());
    let (plugin, name, kept) = (id.clone(), key.to_owned(), secret.map(str::to_owned));
    let warning = cx
        .background_spawn(async move {
            match &kept {
                Some(secret) => store.set(&plugin, &name, secret),
                None => store.clear(&plugin, &name).map(|()| None),
            }
        })
        .await?;
    host.update(cx, |_, cx| cx.notify());
    Ok(warning.map_or(OpResult::Done, |warning| OpResult::Warnings(vec![warning])))
}

pub(in crate::ops) fn remember_setting(
    host: &mut PluginHost,
    id: &PluginId,
    key: &str,
    value: &Value,
    folder: Option<&PathBuf>,
    activation: u64,
    cx: &mut gpui_kit::Context<PluginHost>,
) -> Result<()> {
    if host.activation(id) != Some(activation) {
        return Ok(());
    }
    if let Some(path) = folder {
        if let Some(active) = host.active.get_mut(id) {
            active.grant.paths.insert(key.to_owned(), path.clone());
        }
        if let Some(record) = host.records.get_mut(id) {
            let mut approved = approved(record)?;
            approved.paths.insert(key.to_owned(), path.clone());
            record.approved = serde_json::to_value(&approved)?;
        }
    }
    if let Some(active) = host.active.get(id)
        && let Some(setting) = active.manifest.settings.get(key).cloned()
    {
        active.set(key, &setting, value.clone())?;
    }
    // A folder is granted at load, so the next use loads with the new one.
    if folder.is_some() {
        host.unload(id, cx);
    }
    cx.notify();
    Ok(())
}

pub(in crate::ops) fn stage(
    host: &PluginHost,
    id: &PluginId,
    key: &str,
    folder: Option<&PathBuf>,
) -> Result<()> {
    if host.activation(id).is_none() {
        return Err(OpError::Invalid(format!(
            "plugin {id} changed while this was saving"
        )));
    }
    if folder.is_none() {
        return Ok(());
    }
    let record = host
        .records
        .get(id)
        .ok_or_else(|| OpError::Invalid(format!("plugin {id} was removed while this waited")))?;
    let approved = approved(record)?;
    let still_folder = approved.folders.contains_key(key)
        && host.active.get(id).is_some_and(|active| {
            matches!(
                active.manifest.settings.get(key),
                Some(Setting::Folder { .. })
            )
        });
    if !still_folder {
        return Err(OpError::Invalid(format!(
            "setting {key} of plugin {id} is no longer a folder"
        )));
    }
    Ok(())
}

pub(in crate::ops) fn checked(key: &str, setting: &Setting, value: Value) -> Result<Value> {
    let takes =
        |what: &str| OpError::Invalid(format!("setting {key} ({}) takes {what}", setting.title()));
    match setting {
        Setting::String { .. } if value.is_string() => Ok(value),
        Setting::String { .. } => Err(takes("text")),
        Setting::Boolean { .. } if value.is_boolean() => Ok(value),
        Setting::Boolean { .. } => Err(takes("true or false")),
        Setting::Number { .. } if value.as_f64().is_some_and(f64::is_finite) => Ok(value),
        Setting::Number { .. } => Err(takes("a number, like 3 or 2.5")),
        Setting::Select { options, .. } => match value.as_str() {
            Some(choice) if options.iter().any(|option| option == choice) => Ok(value),
            _ => Err(takes(&format!("one of {}", options.join(", ")))),
        },
        Setting::Folder { .. } => value
            .as_str()
            .filter(|path| !path.chars().any(char::is_control))
            .and_then(|path| dunce::canonicalize(path).ok())
            .filter(|path: &PathBuf| path.is_dir())
            .map(|path| Value::String(path.display().to_string()))
            .ok_or_else(|| takes("an existing folder")),
        Setting::Project { .. } => match value.as_str() {
            Some(id) if !id.is_empty() && !id.chars().any(char::is_control) => Ok(value),
            _ => Err(takes("a project id")),
        },
        Setting::Secret { .. } => {
            let fits = value.as_str().is_some_and(|text| {
                !text.is_empty()
                    && text.len() <= crate::secret::MAX_BYTES
                    && !text.chars().any(char::is_control)
            });
            if fits {
                Ok(value)
            } else {
                Err(takes(&format!(
                    "one line of text, at most {} bytes",
                    crate::secret::MAX_BYTES
                )))
            }
        }
        Setting::Provider { .. } => serde_json::from_value::<AgentChoice>(value)
            .ok()
            .filter(|choice| !choice.provider.is_empty())
            .and_then(|choice| serde_json::to_value(choice).ok())
            .ok_or_else(|| {
                takes(r#"{"provider": <id>, "model"?, "reasoningLevel"?, "permissionMode"?}"#)
            }),
    }
}
