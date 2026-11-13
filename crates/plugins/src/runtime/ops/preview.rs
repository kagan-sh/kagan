use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_core::{Requester, seed_preview};
use kg_domain::{OpError, OpResult, Plugin, PluginPreview, PluginPreviewOp, PreviewId, ThreadId};
use kg_process::Process;

use crate::host::{Hold, hold};
use crate::manifest::Grant;
use crate::runtime::preview::{self, Running};
use crate::source::{Fetched, Source, fetch};
use crate::{PluginHost, Result};

use super::approve::approve;

pub(in crate::runtime::ops) async fn run(
    host: &Entity<PluginHost>,
    op: PluginPreviewOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        PluginPreviewOp::Start { path, thread_id } => {
            start(host, path, thread_id, requester, cx).await
        }
        PluginPreviewOp::Stop { id } => stop(host, id, cx).await,
        PluginPreviewOp::List => Ok(OpResult::PluginPreviews(host.read_with(cx, |host, _| {
            host.previews
                .values()
                .map(|running| running.info.clone())
                .collect()
        }))),
    }
}

// Approval comes first, so nothing is made for a plugin the person did not approve. Then the
// folder is made, the app and its reload watch start, and only a preview that answers is kept.
async fn start(
    host: &Entity<PluginHost>,
    path: PathBuf,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let (fetched, data_dir) = read_plugin(host, path, cx).await?;
    let manifest = &fetched.discovered.manifest;
    let grant = Grant::of(manifest, &fetched.root);
    let id = preview::new_id();
    let folder = preview::folder(&data_dir, &id);
    let socket = preview::socket_for(&folder, &id)?;
    approve(
        thread_id.as_ref(),
        requester,
        format!(
            "Preview {} {} ({}) in a hidden kagan?",
            manifest.name, manifest.version, manifest.id
        ),
        Vec::new(),
        grant.lines(),
        "Preview",
        cx,
    )
    .await?;
    let plugin = Plugin {
        id: manifest.id.clone(),
        root: fetched.root.clone(),
        source: fetched.root.display().to_string(),
        commit: None,
        enabled: true,
        approved: serde_json::to_value(&grant)?,
        skills: manifest.skills.clone(),
        error: None,
    };
    let info = PluginPreview {
        id: id.clone(),
        plugin: manifest.id.clone(),
        name: manifest.name.clone(),
        thread_id: thread_id.clone(),
        path: fetched.root.clone(),
        data_dir: folder.join("data"),
        socket: socket.clone(),
    };
    let (app, watch) = launch(host, &info, &folder, plugin, thread_id.as_ref(), cx).await?;
    let running = Running::new(info.clone(), folder, app, watch);
    host.update(cx, |host, _| {
        host.previews.insert(id, running);
    });
    Ok(OpResult::PluginPreview(info))
}

// The plugin's folder read the way reload reads it, and the data folder the person's app keeps.
async fn read_plugin(
    host: &Entity<PluginHost>,
    path: PathBuf,
    cx: &mut AsyncApp,
) -> Result<(Fetched, PathBuf)> {
    let root = cx
        .background_spawn(async move { dunce::canonicalize(path) })
        .await?;
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let fetched = cx
        .background_spawn({
            let data_dir = data_dir.clone();
            async move {
                let (_keep, never) = async_channel::bounded::<()>(1);
                fetch(
                    &Source::Local(root),
                    None,
                    &PathBuf::new(),
                    &data_dir,
                    &never,
                )
            }
        })
        .await?;
    Ok((fetched, data_dir))
}

// Replaces the thread's older preview of this plugin, seeds the new folder, then starts the app
// and its watch. The app answers `kg open` before the watch starts.
async fn launch(
    host: &Entity<PluginHost>,
    info: &PluginPreview,
    folder: &Path,
    plugin: Plugin,
    thread_id: Option<&ThreadId>,
    cx: &mut AsyncApp,
) -> Result<(Process, Process)> {
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let replaced = host.update(cx, |host, _| {
        preview::take_for(&mut host.previews, thread_id, &plugin.id)
    });
    let stale = replaced.map(|running| running.folder.clone());
    let running_ids = host.read_with(cx, |host, _| {
        host.previews.keys().cloned().collect::<Vec<_>>()
    });
    {
        let (data_dir, folder, preview_data) = (
            data_dir.clone(),
            folder.to_path_buf(),
            info.data_dir.clone(),
        );
        cx.background_spawn(async move {
            preview::sweep(&data_dir, &running_ids)?;
            if let Some(stale) = stale {
                preview::remove(&stale)?;
            }
            std::fs::create_dir_all(&folder)?;
            seed_preview(&preview_data, &plugin)
        })
        .await?;
    }

    hold(host, Hold::PreviewStart, cx).await;
    let exe = std::env::current_exe()?;
    let app = preview::spawn_app(&exe, folder, &info.data_dir, &info.socket)?;
    {
        let (exe, data, socket, log_folder) = (
            exe.clone(),
            info.data_dir.clone(),
            info.socket.clone(),
            folder.to_path_buf(),
        );
        cx.background_spawn(async move { preview::wait_ready(&exe, &data, &socket, &log_folder) })
            .await?;
    }
    let mut watch = preview::spawn_watch(&exe, &info.path, folder, &info.data_dir, &info.socket)?;
    cx.background_executor()
        .timer(Duration::from_millis(500))
        .await;
    if watch.child().try_status()?.is_some() {
        return Err(OpError::Failed(format!(
            "the preview's reload watch stopped at once; its log is {}",
            folder.join(preview::WATCH_LOG).display()
        )));
    }
    Ok((app, watch))
}

// Stopping drops the processes, which kills their trees, and then removes the folder.
async fn stop(host: &Entity<PluginHost>, id: PreviewId, cx: &mut AsyncApp) -> Result<OpResult> {
    let Some(running) = host.update(cx, |host, _| host.previews.remove(&id)) else {
        let running = host.read_with(cx, |host, _| {
            host.previews
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        });
        let listed = if running.is_empty() {
            "none is running".to_owned()
        } else {
            format!("running: {}", running.join(", "))
        };
        return Err(OpError::NotFound(format!("no preview {id}; {listed}")));
    };
    let folder = running.folder.clone();
    drop(running);
    cx.background_spawn(async move { preview::remove(&folder) })
        .await?;
    Ok(OpResult::Done)
}
