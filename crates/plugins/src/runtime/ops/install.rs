use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_core::{Requester, Workspace};
use kg_domain::{Op, OpError, OpResult, Plugin, PluginId, ThreadId, ThreadOp};

use crate::manifest::Grant;
use crate::source::{Fetched, Source, checkout, fetch};
use crate::{PluginHost, Result};

use super::approve::approve;
use super::{approved, record, start};
use crate::host::{Hold, hold};

/// Holds the install slot. Dropping it frees the slot, even when the install is cancelled, and
/// tells the windows, which read the slot from the host.
struct Claim {
    slot: Rc<RefCell<Option<String>>>,
    stops: Rc<RefCell<Option<async_channel::Sender<()>>>>,
    host: Entity<PluginHost>,
    cx: AsyncApp,
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.slot.borrow_mut().take();
        self.stops.borrow_mut().take();
        let host = self.host.clone();
        self.cx
            .spawn(async move |cx| host.update(cx, |_, cx| cx.notify()))
            .detach();
    }
}

pub(in crate::runtime::ops) async fn install(
    host: &Entity<PluginHost>,
    source: String,
    plugin: Option<String>,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let slot = host.read_with(cx, |host, _| host.installing.clone());
    if let Some(running) = slot.borrow().as_ref() {
        return Err(OpError::Invalid(format!(
            "{running} is already installing; answer it in Questions first"
        )));
    }
    *slot.borrow_mut() = Some(crate::index::shown(&source));
    host.update(cx, |_, cx| cx.notify());
    let stops = host.read_with(cx, |host, _| host.install_stop.clone());
    let _claim = Claim {
        slot,
        stops: stops.clone(),
        host: host.clone(),
        cx: cx.clone(),
    };
    if let Some(thread_id) = &thread_id {
        cx.update(|cx| {
            Workspace::dispatch(
                Op::Thread(ThreadOp::Get {
                    id: thread_id.clone(),
                }),
                cx,
            )
        })
        .await?;
    }
    let (data_dir, indexes) =
        host.read_with(cx, |host, _| (host.data_dir.clone(), host.indexes.clone()));
    let plugins = kg_core::plugins_dir(&data_dir);
    let (stop, stopped) = async_channel::bounded::<()>(1);
    let by_name = crate::index::names_an_index(&source);
    if by_name || source.starts_with("git:") {
        *stops.borrow_mut() = Some(stop);
        host.update(cx, |_, cx| cx.notify());
    }
    let (source, plugin, listed) = {
        let (indexes, stopped) = (indexes.clone(), stopped.clone());
        cx.background_spawn(
            async move { crate::index::resolve(&indexes, source, plugin, &stopped) },
        )
        .await?
    };
    let parsed = Source::parse(&source)?;
    let fetched = {
        let plugins = plugins.clone();
        let data_dir = data_dir.clone();
        cx.background_spawn(async move {
            fetch(
                &parsed,
                plugin.as_deref().map(Path::new),
                &plugins,
                &data_dir,
                &stopped,
            )
            .map(|fetched| (parsed, fetched))
        })
        .await
    };
    stops.borrow_mut().take();
    host.update(cx, |_, cx| cx.notify());
    let (parsed, fetched) = fetched?;
    let manifest = &fetched.discovered.manifest;
    let id = manifest.id.clone();
    if host.read_with(cx, |host, _| host.records.contains_key(&id)) {
        return Err(OpError::Invalid(format!(
            "plugin {id} is already installed; use kg plugin update {id}"
        )));
    }
    let grant = Grant::of(manifest, &fetched.root);
    approve(
        thread_id.as_ref(),
        requester,
        format!("Install {} {} ({id})?", manifest.name, manifest.version),
        crate::index::origin(
            &source,
            &fetched.root,
            fetched.commit.as_deref(),
            listed.as_ref(),
            indexes.as_deref().unwrap_or_default(),
        ),
        grant.lines(),
        "Install",
        cx,
    )
    .await?;
    if host.read_with(cx, |host, _| host.records.contains_key(&id)) {
        return Err(OpError::Invalid(format!(
            "plugin {id} was installed while this waited for approval"
        )));
    }
    let source = match parsed {
        Source::Git { .. } => source,
        Source::Local(_) => fetched.root.display().to_string(),
    };
    keep(host, fetched, source, grant, &plugins, true, cx).await
}

pub(in crate::runtime::ops) async fn update(
    host: &Entity<PluginHost>,
    id: &PluginId,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let record = record(host, id, cx)?;
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let plugins = kg_core::plugins_dir(&data_dir);
    let fetched = {
        let (plugins, record, data_dir) = (plugins.clone(), record.clone(), data_dir.clone());
        cx.background_spawn(async move {
            let subfolder = match &record.commit {
                Some(commit) => record
                    .root
                    .strip_prefix(checkout(&plugins, &record.id, commit))
                    .map(Path::to_path_buf)
                    .ok(),
                None => None,
            };
            let (_keep, never) = async_channel::bounded::<()>(1);
            fetch(
                &Source::parse(&record.source)?,
                subfolder.as_deref(),
                &plugins,
                &data_dir,
                &never,
            )
        })
        .await?
    };
    let manifest = &fetched.discovered.manifest;
    if manifest.id != *id {
        return Err(OpError::Invalid(format!(
            "{} now holds plugin {}, not {id}",
            record.source, manifest.id
        )));
    }
    let grant = Grant::of(manifest, &fetched.root);
    let approved = approved(&record)?;
    let within = grant.within(&approved);
    if within && fetched.commit.is_some() && fetched.commit == record.commit {
        return Ok(OpResult::Done);
    }
    if !within {
        let indexes = host.read_with(cx, |host, _| host.indexes.clone());
        approve(
            thread_id.as_ref(),
            requester,
            format!("Update {} to {} ({id})?", manifest.name, manifest.version),
            crate::index::origin(
                &record.source,
                &fetched.root,
                fetched.commit.as_deref(),
                None,
                indexes.as_deref().unwrap_or_default(),
            ),
            grant.added(&approved),
            "Approve",
            cx,
        )
        .await?;
    }
    keep(host, fetched, record.source, grant, &plugins, false, cx).await
}

pub(in crate::runtime::ops) async fn keep(
    host: &Entity<PluginHost>,
    fetched: Fetched,
    source: String,
    grant: Grant,
    plugins: &Path,
    create: bool,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let commit = fetched.commit.clone();
    let plugins = plugins.to_path_buf();
    let (root, discovered) = cx
        .background_spawn(async move { fetched.keep(&plugins) })
        .await?;
    let id = discovered.manifest.id.clone();
    let paused = host.update(cx, |host, _| host.pause_activation(&id));
    hold(host, Hold::Keep, cx).await;
    let approved = serde_json::to_value(&grant)?;
    let skills = discovered.manifest.skills.clone();
    let workspace = cx.update(|cx| Workspace::global(cx));
    let kept = workspace
        .read_with(cx, |workspace, _| {
            workspace.keep_plugin(
                id.clone(),
                root.clone(),
                source.clone(),
                commit.clone(),
                approved.clone(),
                skills.clone(),
            )
        })
        .await?;
    let record = if let Some(record) = kept {
        record
    } else if create {
        let record = Plugin {
            id: id.clone(),
            root,
            source,
            commit,
            enabled: true,
            approved,
            skills,
            error: None,
        };
        workspace
            .read_with(cx, |workspace, _| workspace.insert_plugin(record.clone()))
            .await?;
        record
    } else {
        return Err(OpError::Invalid("plugin was removed".into()));
    };
    host.update(cx, |host, cx| {
        host.records.insert(record.id.clone(), record.clone());
        cx.notify();
    });
    paused.commit();
    if record.enabled {
        start(host, &record, Some(discovered), cx).await?;
    }
    Ok(OpResult::Done)
}
