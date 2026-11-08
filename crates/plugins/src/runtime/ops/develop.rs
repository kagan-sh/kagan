use std::path::PathBuf;

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_core::{Requester, Workspace};
use kg_domain::{OpError, OpResult, Plugin, PluginId, PluginOp, ThreadId};

use crate::manifest::Grant;
use crate::source::{Source, fetch};
use crate::{PluginHost, Result};

use super::approve::{approve, choose, removal_options};
use super::install::keep;
use super::{approved, record, start};

pub(in crate::runtime::ops) async fn develop(
    host: &Entity<PluginHost>,
    op: PluginOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        PluginOp::New { name, dir } => {
            let components = host.read_with(cx, |host, _| host.components());
            cx.background_spawn(async move { crate::development::new(&name, dir, &components) })
                .await
        }
        PluginOp::Check { path } => {
            let (data_dir, components) =
                host.read_with(cx, |host, _| (host.data_dir.clone(), host.components()));
            cx.background_spawn(async move {
                let warnings = crate::development::check(&path, &data_dir, &components)?;
                Ok(if warnings.is_empty() {
                    OpResult::Done
                } else {
                    OpResult::Warnings(warnings)
                })
            })
            .await
        }
        PluginOp::Share { path } => {
            let (data_dir, components) =
                host.read_with(cx, |host, _| (host.data_dir.clone(), host.components()));
            cx.background_spawn(async move { crate::share::share(&path, &data_dir, &components) })
                .await
        }
        PluginOp::Types { path } => {
            let components = host.read_with(cx, |host, _| host.components());
            cx.background_spawn(async move {
                crate::development::types(&path, &components).map(OpResult::Files)
            })
            .await
        }
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
        PluginOp::CancelInstall => {
            if host.read_with(cx, |host, _| host.stop_install()) {
                Ok(OpResult::Done)
            } else {
                Err(OpError::Invalid("no git install is running".into()))
            }
        }
        PluginOp::Reload { id, thread_id } => {
            let record = record(host, &id, cx)?;
            if !record.enabled {
                return Err(OpError::Invalid(format!("plugin {id} is disabled")));
            }
            reload(host, record, thread_id, requester, cx).await
        }
        PluginOp::Dev { path, thread_id } => dev(host, path, thread_id, requester, cx).await,
        PluginOp::Logs { id, after, .. } => {
            record(host, &id, cx)?;
            let path = host.read_with(cx, |host, _| {
                host.data_dir.join("plugins").join(&id.0).join("logs.jsonl")
            });
            cx.background_spawn(async move { crate::development::logs(&path, after.unwrap_or(0)) })
                .await
        }
        PluginOp::Start
        | PluginOp::Cli { .. }
        | PluginOp::Command { .. }
        | PluginOp::Install { .. }
        | PluginOp::Update { .. }
        | PluginOp::Index { .. }
        | PluginOp::Search { .. }
        | PluginOp::Enable { .. }
        | PluginOp::Disable { .. }
        | PluginOp::Config { .. }
        | PluginOp::Use { .. } => Err(OpError::Failed("not a development operation".into())),
    }
}

pub(in crate::runtime::ops) async fn enable(
    host: &Entity<PluginHost>,
    id: PluginId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let record = record(host, &id, cx)?;
    if !record.enabled {
        let mut starting = record;
        starting.enabled = true;
        start(host, &starting, None, cx).await?;
        set_enabled(&id, true, cx).await?;
        host.update(cx, |host, _| {
            if let Some(record) = host.records.get_mut(&id) {
                record.enabled = true;
            }
        });
    }
    Ok(OpResult::Done)
}

pub(in crate::runtime::ops) async fn disable(
    host: &Entity<PluginHost>,
    id: PluginId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    record(host, &id, cx)?;
    host.update(cx, |host, cx| {
        host.disable_loaded(&id, cx);
        if let Some(record) = host.records.get_mut(&id) {
            record.enabled = false;
        }
    });
    #[cfg(test)]
    if let Some(hold) = host.update(cx, |host, _| host.lifecycle_hold.take()) {
        let _ = hold.recv().await;
        host.update(cx, |host, cx| {
            host.disable_loaded(&id, cx);
            if let Some(record) = host.records.get_mut(&id) {
                record.enabled = false;
            }
        });
    }
    set_enabled(&id, false, cx).await?;
    Ok(OpResult::Done)
}

pub(in crate::runtime::ops) async fn remove(
    host: &Entity<PluginHost>,
    id: PluginId,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    record(host, &id, cx)?;
    let question = format!("Remove “{id}”?");
    choose(
        thread_id.as_ref(),
        requester,
        question.clone(),
        &question,
        removal_options(),
        cx,
    )
    .await?;
    record(host, &id, cx)?;
    let declared = host.read_with(cx, |host, _| {
        host.active
            .get(&id)
            .map(|active| active.manifest.settings.secrets())
            .unwrap_or_default()
    });
    host.update(cx, |host, _| host.end_activation(&id));
    let workspace = cx.update(|cx| Workspace::global(cx));
    if let Err(error) = workspace
        .read_with(cx, |workspace, _| workspace.remove_plugin(id.clone()))
        .await
    {
        host.update(cx, |host, _| host.resume_activation(&id));
        return Err(error);
    }
    let data = host.read_with(cx, |host, _| host.plugin_data(&id));
    let store_ids = crate::web_store::read_store_ids(&data);
    host.update(cx, |host, cx| {
        host.unload(&id, cx);
        // Drop native pages first. They stay in a UI global until the next frame, and until then
        // `WebKit` refuses `remove_data_store` with `DataStoreInUse`.
        host.drop_pages(&id, &store_ids, cx);
        host.active.remove(&id);
        host.records.remove(&id);
        host.warnings.remove(&id);
        host.identities.remove(&id);
        cx.notify();
    });
    // The native view and its frame mouse listener hold the store until the next draw. Refresh
    // so they let go. A store that is still held keeps its `store.id` file.
    cx.update(gpui_kit::App::refresh_windows);
    let cleared = cx
        .spawn(async move |cx| {
            crate::web_store::remove_web_stores_after_release(store_ids, cx).await
        })
        .await;
    crate::web_store::clear_removed_stores(&data, &cleared.removed);
    let mut warnings = cleared.warnings;
    let store = host.read_with(cx, |host, _| host.secrets.clone());
    let forgotten = cx
        .background_spawn(async move { store.forget(&id, &declared) })
        .await;
    match forgotten {
        Ok(kept) if !kept.is_empty() => warnings.push(format!(
            "the plugin is removed, but the system keychain kept its secrets {}; \
             delete them in the keychain, service kagan",
            kept.join(", ")
        )),
        Err(error) => warnings.push(format!(
            "the plugin is removed, but its secrets were not forgotten: {error}"
        )),
        Ok(_) => {}
    }
    Ok(if warnings.is_empty() {
        OpResult::Done
    } else {
        OpResult::Warnings(warnings)
    })
}

pub(in crate::runtime::ops) async fn dev(
    host: &Entity<PluginHost>,
    path: PathBuf,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let root = cx
        .background_spawn(async move { dunce::canonicalize(path) })
        .await?;
    let mut record = host
        .read_with(cx, |host, _| {
            host.records
                .values()
                .find(|record| record.root == root && record.commit.is_none())
                .cloned()
        })
        .ok_or_else(|| {
            OpError::NotFound(format!(
                "{} is not an installed local plugin; install it first",
                root.display()
            ))
        })?;
    if !record.enabled {
        return Err(OpError::Invalid(format!(
            "plugin {} is disabled",
            record.id
        )));
    }
    reload(host, record.clone(), thread_id, requester, cx).await?;
    record.source = crate::index::shown(&record.source);
    Ok(OpResult::Plugins(vec![record]))
}

pub(in crate::runtime::ops) async fn reload(
    host: &Entity<PluginHost>,
    record: Plugin,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let plugins = data_dir.join("plugins");
    let root = record.root.clone();
    let mut fetched = cx
        .background_spawn(async move {
            let (_keep, never) = async_channel::bounded::<()>(1);
            fetch(
                &Source::Local(root),
                None,
                &PathBuf::new(),
                &data_dir,
                &never,
            )
        })
        .await?;
    fetched.commit.clone_from(&record.commit);
    if fetched.discovered.manifest.id != record.id {
        return Err(OpError::Invalid(
            "reload cannot change the plugin id".into(),
        ));
    }
    let grant = Grant::of(&fetched.discovered.manifest, &fetched.root);
    let approved = approved(&record)?;
    if !grant.within(&approved) {
        approve(
            thread_id.as_ref(),
            requester,
            format!(
                "Reload {} ({})?",
                fetched.discovered.manifest.name, record.id
            ),
            Vec::new(),
            grant.added(&approved),
            false,
            cx,
        )
        .await?;
    }
    let current = self::record(host, &record.id, cx)?;
    if current != record {
        return Err(OpError::Invalid(
            "plugin changed while reload waited; retry it".into(),
        ));
    }
    let result = keep(host, fetched, record.source, grant, &plugins, false, cx).await?;
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    let id = record.id;
    cx.background_spawn(async move { secrets.log(&id, crate::Level::Info, "reloaded") })
        .await?;
    Ok(result)
}

pub(in crate::runtime::ops) async fn set_enabled(
    id: &PluginId,
    enabled: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    let id = id.clone();
    let workspace = cx.update(|cx| Workspace::global(cx));
    workspace
        .read_with(cx, |workspace, _| workspace.set_plugin_enabled(id, enabled))
        .await
}
