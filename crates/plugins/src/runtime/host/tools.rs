use std::collections::BTreeSet;

use gpui_kit::{AsyncApp, Entity};
use kg_core::Workspace;
use kg_domain::{AgentTool, Op, OpError, OpResult, PluginId, ProjectId, ThreadId, ThreadOp};
use serde_json::Value;

use crate::ask::{Job, ask};
use crate::manifest::Scope;
use crate::{INSTRUCT_CAP, INSTRUCT_TOTAL, Result};

use super::PluginHost;

async fn offered(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<Vec<(PluginId, String)>> {
    let plugins: Vec<(PluginId, Vec<(String, Scope)>)> = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .map(|(id, active)| {
                let tools = active
                    .manifest
                    .tools
                    .iter()
                    .map(|tool| (tool.name.clone(), tool.scope))
                    .collect();
                (id.clone(), tools)
            })
            .collect()
    });
    let mut names = BTreeSet::new();
    let mut offers = Vec::new();
    for (plugin, tools) in plugins {
        let tagged = if tools.iter().any(|(_, scope)| *scope == Scope::Tagged) {
            let metadata = cx
                .update(|cx| {
                    Workspace::dispatch(
                        Op::Thread(ThreadOp::Metadata {
                            id: thread_id.clone(),
                            plugin_id: plugin.clone(),
                        }),
                        cx,
                    )
                })
                .await?;
            !matches!(metadata, OpResult::Metadata(Value::Null))
        } else {
            false
        };
        for (name, scope) in tools {
            let visible = match scope {
                Scope::All => true,
                Scope::Tagged => tagged,
            };
            if visible && names.insert(name.clone()) {
                offers.push((plugin.clone(), name));
            }
        }
    }
    Ok(offers)
}

pub(super) async fn tools(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let offers = offered(host, thread_id, cx).await?;
    Ok(host.read_with(cx, |host, _| {
        let mut instructions: Vec<String> = Vec::new();
        let mut tools = Vec::new();
        for (plugin, name) in &offers {
            let Some(active) = host.active.get(plugin) else {
                continue;
            };
            let Some((tool, schema)) = active.tool(name) else {
                continue;
            };
            if let Some(text) = &active.manifest.instructions {
                let block = format!("Plugin {plugin}:\n{text}");
                if !instructions.contains(&block) {
                    instructions.push(block);
                }
            }
            tools.push(AgentTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: schema.value.clone(),
            });
        }
        OpResult::Tools {
            instructions: (!instructions.is_empty()).then(|| instructions.join("\n\n")),
            tools,
        }
    }))
}

pub(super) async fn call_tool(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    name: &str,
    input: Value,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let not_offered = || {
        OpError::NotFound(format!(
            "agent tool {name} is not offered to thread {thread_id}"
        ))
    };
    let (plugin, _) = offered(host, thread_id, cx)
        .await?
        .into_iter()
        .find(|(_, offered)| offered == name)
        .ok_or_else(not_offered)?;
    let errors = host.read_with(cx, |host, _| {
        let (_, schema) = host.active.get(&plugin)?.tool(name)?;
        Some(
            schema
                .validator
                .iter_errors(&input)
                .map(|error| format!("{}: {error}", error.instance_path()))
                .collect::<Vec<_>>(),
        )
    });
    let errors = errors.ok_or_else(not_offered)?;
    if !errors.is_empty() {
        return Err(OpError::Invalid(format!(
            "input for {name} does not match its schema: {}",
            errors.join("; ")
        )));
    }
    let job = Job::Tool {
        name,
        input: &input,
        thread_id,
    };
    match ask(host, &plugin, &job, cx).await.map_err(OpError::from)? {
        Err(message) => Ok(OpResult::ToolReply {
            text: message,
            is_error: true,
        }),
        Ok(value) => {
            let Some(text) = value["text"].as_str() else {
                return Err(OpError::Failed(format!(
                    "plugin {plugin} answered {name} without text"
                )));
            };
            Ok(OpResult::ToolReply {
                text: text.to_owned(),
                is_error: value["isError"].as_bool().unwrap_or(false),
            })
        }
    }
}

pub(crate) async fn instruct(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    project_id: &ProjectId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let plugins: Vec<PluginId> = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .filter(|(_, active)| active.manifest.instruct)
            .map(|(id, _)| id.clone())
            .collect()
    });
    if plugins.is_empty() {
        return Ok(OpResult::Output(Some(String::new())));
    }
    let mut tasks = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let (host, mut cx) = (host.clone(), cx.clone());
        tasks.push(async move {
            let job = Job::Instruct {
                thread_id,
                project_id,
            };
            let Ok(Ok(reply)) = ask(&host, &plugin, &job, &mut cx).await else {
                return String::new();
            };
            let text = instruct_text(&reply);
            if text.is_empty() {
                return String::new();
            }
            let text = truncate(&text, INSTRUCT_CAP);
            format!("Plugin {plugin}:\n{text}")
        });
    }
    let parts = futures::future::join_all(tasks).await;
    let mut joined = String::new();
    for part in parts.into_iter().filter(|text| !text.is_empty()) {
        let next = if joined.is_empty() {
            part.len()
        } else {
            joined.len() + 2 + part.len()
        };
        if next > INSTRUCT_TOTAL {
            break;
        }
        if !joined.is_empty() {
            joined.push_str("\n\n");
        }
        joined.push_str(&part);
    }
    Ok(OpResult::Output(Some(joined)))
}

fn instruct_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => String::new(),
    }
}

fn truncate(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return text.to_owned();
    }
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
