use std::collections::BTreeMap;
use std::time::Duration;

use gpui_kit::{App, AsyncApp, Entity, Task};
use kg_core::Workspace;
use kg_domain::{KeymapAction, KeymapOp, OpResult, PluginId, ProjectId, ThreadId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{PluginHost, Result};

#[derive(Clone, PartialEq, gpui_kit::Action)]
#[action(no_json)]
pub struct RunCommand {
    plugin: PluginId,
    command: String,
}

impl RunCommand {
    #[must_use]
    pub fn new(plugin: PluginId, command: String) -> Self {
        Self { plugin, command }
    }

    #[must_use]
    pub fn plugin(&self) -> &PluginId {
        &self.plugin
    }
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }
}

pub(crate) const BINDINGS: &str = "$hotkeys";

/// docs/plugin-api.md gives a command 10 s; tools and CLI calls get `TOOL_TIMEOUT`.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandWhen {
    #[default]
    Always,
    Thread,
    Project,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Command {
    pub(crate) id: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) key: Option<String>,
    #[serde(default)]
    pub(crate) when: CommandWhen,
    #[serde(default)]
    pub(crate) header: bool,
}

/// Where a command's key comes from. A key the person saved is bound before any manifest's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeySource {
    Saved,
    Manifest,
}

#[derive(Clone, Debug)]
pub struct CommandContribution {
    pub plugin: PluginId,
    pub id: String,
    pub title: String,
    pub key: Option<String>,
    pub when: CommandWhen,
    pub header: bool,
    pub source: KeySource,
}

impl CommandContribution {
    /// The name `kg keymap` gives this command.
    #[must_use]
    pub fn action(&self) -> String {
        format!("{}:{}", self.plugin, self.id)
    }
}

impl PluginHost {
    #[must_use]
    pub fn commands(&self) -> Vec<CommandContribution> {
        self.active
            .iter()
            .flat_map(|(plugin, active)| {
                active.manifest.commands.iter().map(|command| {
                    let (key, source) = match active.bindings.get(&command.id) {
                        Some(saved) => (saved.clone(), KeySource::Saved),
                        None => (command.key.clone(), KeySource::Manifest),
                    };
                    CommandContribution {
                        plugin: plugin.clone(),
                        id: command.id.clone(),
                        title: command.title.clone(),
                        key,
                        when: command.when,
                        header: command.header,
                        source,
                    }
                })
            })
            .collect()
    }

    pub fn dispatch_command(
        host: &Entity<Self>,
        plugin: &PluginId,
        command: &str,
        thread_id: Option<ThreadId>,
        project_id: Option<ProjectId>,
        cx: &mut App,
    ) -> Task<Result<OpResult>> {
        let (host, plugin, command) = (host.clone(), plugin.clone(), command.to_owned());
        cx.spawn(async move |cx| {
            host.update(cx, |host, _| {
                let active = host.active.get(&plugin).ok_or_else(|| {
                    kg_domain::OpError::NotFound(format!("plugin {plugin} is not enabled"))
                })?;
                let command = active
                    .manifest
                    .commands
                    .iter()
                    .find(|item| item.id == command)
                    .ok_or_else(|| {
                        kg_domain::OpError::NotFound(format!(
                            "plugin {plugin} has no command {command}"
                        ))
                    })?;
                let allowed = match command.when {
                    CommandWhen::Always => true,
                    CommandWhen::Thread => thread_id.is_some(),
                    CommandWhen::Project => project_id.is_some(),
                };
                if allowed {
                    Ok(())
                } else {
                    Err(kg_domain::OpError::Invalid(format!(
                        "command {} needs a {:?} on screen",
                        command.id, command.when
                    )))
                }
            })?;
            let job = json!({
                "kind": "command",
                "command": command,
                "threadId": thread_id,
                "projectId": project_id,
            })
            .as_object()
            .cloned()
            .unwrap_or_default();
            let reply =
                crate::host::call(&host, &plugin, &command, job, COMMAND_TIMEOUT, cx).await?;
            reply
                .map_err(kg_domain::OpError::Failed)
                .map(|_| OpResult::Done)
        })
    }
}

pub(crate) fn checked_key(key: &str) -> Result<String> {
    if key.trim().is_empty() {
        return Err(kg_domain::OpError::Invalid("key cannot be empty".into()));
    }
    key.split_whitespace()
        .map(|key| {
            kg_core::parse_key(key)
                .map(|key| key.unparse(kg_core::host_os()))
                .map_err(|error| kg_domain::OpError::Invalid(error.to_string()))
        })
        .collect::<Result<Vec<_>>>()
        .map(|keys| keys.join(" "))
}

/// The plugin half of `kg keymap`: a command is `<plugin-id>:<command>`. Core checked the key.
pub(crate) async fn keymap(
    host: &Entity<PluginHost>,
    op: KeymapOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let (action, key) = match op {
        KeymapOp::Get => {
            let keys: Map<String, Value> = host.read_with(cx, |host, _| {
                host.commands()
                    .into_iter()
                    .map(|command| {
                        (
                            command.action(),
                            command.key.map_or(Value::Null, Value::from),
                        )
                    })
                    .collect()
            });
            return Ok(OpResult::Setting(Value::Object(keys)));
        }
        KeymapOp::Set {
            action,
            reset: true,
            ..
        } => (action, Saved::Default),
        KeymapOp::Set { action, key, .. } => (action, Saved::Key(key)),
        KeymapOp::Remove { action } => (action, Saved::None),
    };
    let KeymapAction::Plugin { plugin, command } = KeymapAction::parse(&action)? else {
        return Err(kg_domain::OpError::Invalid(format!(
            "{action} is not <plugin-id>:<command>"
        )));
    };
    let bindings = host.update(cx, |host, cx| {
        let commands = host.commands();
        let active = host.active.get_mut(&plugin).ok_or_else(|| {
            kg_domain::OpError::NotFound(format!("plugin {plugin} is not enabled"))
        })?;
        if !active
            .manifest
            .commands
            .iter()
            .any(|item| item.id == command)
        {
            return Err(kg_domain::OpError::NotFound(format!(
                "plugin {plugin} has no command {command}"
            )));
        }
        match key {
            Saved::Default => {
                active.bindings.remove(&command);
            }
            Saved::None => {
                active.bindings.insert(command, None);
            }
            Saved::Key(key) => {
                // The window refuses a key that overlaps one of its own, so a plugin key may not either.
                let entered = crate::keys::text_sequence(&key);
                let taken =
                    crate::keys::conflict(&crate::keys::window_keys(cx), &entered).or_else(|| {
                        commands
                            .iter()
                            .filter(|item| item.plugin != plugin || item.id != command)
                            .find(|item| {
                                item.key.as_deref().is_some_and(|other| {
                                    crate::keys::overlaps(
                                        &entered,
                                        &crate::keys::text_sequence(other),
                                    )
                                })
                            })
                            .map(CommandContribution::action)
                    });
                if let Some(conflict) = taken {
                    return Ok(Err(conflict));
                }
                active.bindings.insert(command, Some(key));
            }
        }
        cx.notify();
        Ok(Ok(active.bindings.clone()))
    })?;
    match bindings {
        Ok(bindings) => {
            store_bindings(host, plugin, &bindings, cx).await?;
            Ok(OpResult::Setting(json!({ "ok": true })))
        }
        Err(conflict) => Ok(OpResult::Setting(
            json!({ "ok": false, "conflict": conflict }),
        )),
    }
}

enum Saved {
    Default,
    None,
    Key(String),
}

async fn store_bindings(
    host: &Entity<PluginHost>,
    plugin: PluginId,
    bindings: &BTreeMap<String, Option<String>>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let permit = host.read_with(cx, |host, _| {
        Some(kg_core::Permit::new(
            host.activation(&plugin)?,
            host.permit(&plugin)?,
        ))
    });
    let Some(permit) = permit else {
        return Err(kg_domain::OpError::Invalid(format!(
            "plugin {plugin} changed while this was saving"
        )));
    };
    let value = serde_json::to_value(bindings)?;
    let workspace = cx.update(|cx| Workspace::global(cx));
    workspace
        .read_with(cx, |workspace, _| {
            workspace.commit_plugin_config(plugin.clone(), BINDINGS.into(), value, None, permit)
        })
        .await?;
    Ok(())
}
