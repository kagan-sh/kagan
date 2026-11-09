use std::path::PathBuf;

use gpui_kit::{AsyncApp, Entity};
use kg_core::{Requester, Workspace};
use kg_domain::{
    Op, OpError, OpResult, Plugin, PluginId, PluginOp, PreferenceOp, ProjectId, ProjectOp, Slot,
    SlotChoice, ThreadId, ThreadOp,
};
use serde_json::Value;

use crate::ask::{Job, ask};
use crate::host::activate;
use crate::manifest::Grant;
use crate::{Discovered, PluginHost, Result};

use approve::{choose, options};
use config::config;
use develop::{check, dev, disable, enable, logs, new, reload_enabled, remove, share, types};
use install::{install, update};

mod approve;
mod config;
mod develop;
mod install;

pub(crate) fn approved(record: &Plugin) -> Result<Grant> {
    serde_json::from_value(record.approved.clone()).map_err(|error| {
        OpError::Invalid(format!(
            "plugin {}: the stored approval cannot be read; approve it again ({error})",
            record.id
        ))
    })
}

pub(crate) async fn run(
    host: &Entity<PluginHost>,
    op: PluginOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        PluginOp::Start => Err(OpError::Failed(
            "starting a plugin thread needs a window".into(),
        )),
        PluginOp::New { name, dir } => new(host, name, dir, cx).await,
        PluginOp::Check { path } => check(host, path, cx).await,
        PluginOp::Share { path } => share(host, path, cx).await,
        PluginOp::Types { path } => types(host, path, cx).await,
        PluginOp::List => Ok(OpResult::Plugins(host.read_with(cx, |host, _| {
            host.records
                .values()
                .map(|record| Plugin {
                    error: host.failure(&record.id),
                    source: crate::index::shown(&record.source),
                    ..record.clone()
                })
                .collect()
        }))),
        PluginOp::Remove { id, thread_id } => remove(host, id, thread_id, requester, cx).await,
        PluginOp::Reload { id, thread_id } => {
            reload_enabled(host, id, thread_id, requester, cx).await
        }
        PluginOp::Dev { path, thread_id } => dev(host, path, thread_id, requester, cx).await,
        PluginOp::CancelInstall => {
            if host.read_with(cx, |host, _| host.stop_install()) {
                Ok(OpResult::Done)
            } else {
                Err(OpError::Invalid("no git install is running".into()))
            }
        }
        PluginOp::Logs { id, after, .. } => logs(host, id, after, cx).await,
        PluginOp::Cli {
            name,
            command,
            argv,
            cwd,
            thread_id,
            project_id,
        } => {
            let call = CliCall {
                name,
                command,
                argv,
                cwd,
                thread_id,
                project_id,
            };
            cli(host, call, cx).await
        }
        PluginOp::Command {
            id,
            command,
            thread_id,
            project_id,
        } => {
            let task = cx.update(|cx| {
                PluginHost::dispatch_command(host, &id, &command, thread_id, project_id, cx)
            });
            task.await
        }
        PluginOp::Install {
            source,
            plugin,
            thread_id,
        } => install(host, source, plugin, thread_id, requester, cx).await,
        PluginOp::Update { id, thread_id } => update(host, &id, thread_id, requester, cx).await,
        PluginOp::Index { op } => crate::index::run(host, op, cx).await,
        PluginOp::Search { text } => crate::index::search(host, text, cx).await,
        PluginOp::Enable { id } => enable(host, id, cx).await,
        PluginOp::Disable { id } => disable(host, id, cx).await,
        PluginOp::Config { id, op, thread_id } => {
            config(host, &id, op, thread_id, requester, cx).await
        }
        PluginOp::Use {
            slot,
            id,
            thread_id,
        } => use_slot(slot, id, thread_id, requester, cx).await,
    }
}

// A plugin fills a slot only when a person picks it: a request from a terminal or an agent asks.
async fn use_slot(
    slot: Slot,
    choice: SlotChoice,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if let SlotChoice::Plugin(id) = &choice {
        let offered = cx.update(|cx| {
            crate::contributions(cx).is_ok_and(|list| {
                list.iter()
                    .any(|(plugin, ui)| plugin == id && ui.replacement(slot).is_some())
            })
        });
        if !offered {
            return Err(OpError::Invalid(format!(
                "{id} does not offer a {}; kg plugin list shows what each plugin offers",
                slot.name()
            )));
        }
        if requester.is_some() {
            let question = format!("Let {id} draw the {}?", slot.name());
            choose(
                thread_id.as_ref(),
                requester,
                question.clone(),
                &question,
                options("Approve", false),
                cx,
            )
            .await?;
        }
    }
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: slot.preference().to_owned(),
                value: Value::String(choice.to_string()),
            }),
            cx,
        )
    });
    set.await
}

struct CliCall {
    name: String,
    command: String,
    argv: Vec<String>,
    cwd: PathBuf,
    thread_id: Option<ThreadId>,
    project_id: Option<ProjectId>,
}

async fn cli(host: &Entity<PluginHost>, call: CliCall, cx: &mut AsyncApp) -> Result<OpResult> {
    let CliCall {
        name,
        command,
        argv,
        cwd,
        thread_id,
        project_id,
    } = call;
    let mut providers = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .filter(|(_, active)| {
                active.manifest.cli.iter().any(|group| {
                    group.name == name && group.commands.iter().any(|item| item.name == command)
                })
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    });
    if providers.len() > 1 {
        let names: Vec<_> = providers.iter().map(|id| id.0.as_str()).collect();
        return Err(OpError::Invalid(format!(
            "kg {name} {command} is provided by more than one plugin: {}",
            names.join(", ")
        )));
    }
    let plugin = providers.pop().ok_or_else(|| {
        OpError::NotFound(format!("no enabled plugin provides kg {name} {command}"))
    })?;
    let project_id = match (project_id, &thread_id) {
        (Some(project), _) => Some(project),
        (None, Some(id)) => {
            let thread = cx
                .update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Get { id: id.clone() }), cx))
                .await?;
            let OpResult::Thread(thread) = thread else {
                return Err(OpError::Failed("thread get returned no thread".into()));
            };
            Some(thread.project_id)
        }
        (None, None) => {
            let projects = cx
                .update(|cx| Workspace::dispatch(Op::Project(ProjectOp::List), cx))
                .await?;
            let OpResult::Projects(projects) = projects else {
                return Err(OpError::Failed("project list returned no projects".into()));
            };
            projects
                .into_iter()
                .find(|project| cwd.starts_with(&project.path))
                .map(|project| project.id)
        }
    };
    let job = Job::Cli {
        command: &command,
        argv: &argv,
        cwd: cwd.display().to_string(),
        thread_id: &thread_id,
        project_id: &project_id,
    };
    let value = ask(host, &plugin, &job, cx)
        .await
        .map_err(OpError::from)?
        .map_err(OpError::Failed)?;
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    let exit_code = value["exitCode"]
        .as_i64()
        .and_then(|code| i32::try_from(code).ok())
        .ok_or_else(|| {
            OpError::Failed(format!(
                "plugin {plugin} answered {command} without exitCode"
            ))
        })?;
    let text = |key| secrets.redact(&plugin, value[key].as_str().unwrap_or_default());
    Ok(OpResult::Cli {
        exit_code,
        stdout: text("stdout"),
        stderr: text("stderr"),
    })
}

// Starts a plugin. A start that fails leaves its reason on the plugin, in place of any older one.
async fn start(
    host: &Entity<PluginHost>,
    record: &Plugin,
    discovered: Option<Discovered>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let started = activate(host, record, discovered, cx).await;
    if let Err(error) = &started {
        let (id, reason) = (record.id.clone(), error.to_string());
        host.update(cx, |host, _| {
            host.warnings.insert(id, reason);
        });
    }
    if started.is_ok() {
        crate::host::load_status(host, &record.id, cx);
    }
    started
}

fn record(host: &Entity<PluginHost>, id: &PluginId, cx: &mut AsyncApp) -> Result<Plugin> {
    host.read_with(cx, |host, _| host.records.get(id).cloned())
        .ok_or_else(|| OpError::NotFound(format!("plugin {id} is not installed")))
}
