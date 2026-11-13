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

use super::approve::{approve, refuse_in_preview};

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
        PluginPreviewOp::List => list(host, cx),
    }
}

// Approval comes first, so nothing is made for a plugin the person did not approve. The new
// preview is built under a reserved id, and it replaces the thread's old one only once it answers.
async fn start(
    host: &Entity<PluginHost>,
    path: PathBuf,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    refuse_in_preview(cx)?;
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
    host.update(cx, |host, _| host.previews.reserve(id.clone()));
    let launched = launch(host, &info, &folder, plugin, cx).await;
    host.update(cx, |host, _| host.previews.release(&id));
    let (app, watch) = match launched {
        Ok(processes) => processes,
        Err(error) => {
            // The error already says what the log ends with, so the folder can go.
            drop(preview::remove(&folder));
            return Err(error);
        }
    };
    let replaced = host.update(cx, |host, _| {
        host.previews.take_for(thread_id.as_ref(), &manifest.id)
    });
    if let Some(old) = replaced {
        drop(preview::shut_down_later(old, cx.background_executor()));
    }
    host.update(cx, |host, _| {
        host.previews
            .insert(Running::new(info.clone(), folder, app, watch));
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

// Sweeps leftovers, seeds the folder, then starts the app and its watch. The app must answer
// `kg open` before the watch starts. A failure in any step returns with its reason.
async fn launch(
    host: &Entity<PluginHost>,
    info: &PluginPreview,
    folder: &Path,
    plugin: Plugin,
    cx: &mut AsyncApp,
) -> Result<(Process, Process)> {
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let keep = host.read_with(cx, |host, _| host.previews.ids());
    let exe = preview::exe()?;
    {
        let (exe, data_dir, folder, preview_data) = (
            exe.clone(),
            data_dir.clone(),
            folder.to_path_buf(),
            info.data_dir.clone(),
        );
        cx.background_spawn(async move {
            preview::sweep(&exe, &data_dir, &keep)?;
            std::fs::create_dir_all(&folder)?;
            seed_preview(&preview_data, &plugin)
        })
        .await?;
    }

    hold(host, Hold::PreviewStart, cx).await;
    let mut app = preview::spawn_app(&exe, folder, &info.data_dir, &info.socket)?;
    let (app, ready) = {
        let (exe, data, socket, log_folder) = (
            exe.clone(),
            info.data_dir.clone(),
            info.socket.clone(),
            folder.to_path_buf(),
        );
        cx.background_spawn(async move {
            let ready = preview::wait_ready(&exe, &data, &socket, &log_folder, &mut app);
            (app, ready)
        })
        .await
    };
    ready?;
    let mut watch = preview::spawn_watch(&exe, &info.path, folder, &info.data_dir, &info.socket)?;
    cx.background_executor()
        .timer(Duration::from_millis(500))
        .await;
    if watch.child().try_status()?.is_some() {
        return Err(preview::stopped_message(
            "the preview's reload watch stopped at once",
            folder,
        ));
    }
    Ok((app, watch))
}

// Ends the preview under this id, and says which ones run when it is not one of them.
async fn stop(host: &Entity<PluginHost>, id: PreviewId, cx: &mut AsyncApp) -> Result<OpResult> {
    refuse_in_preview(cx)?;
    let Some(running) = host.update(cx, |host, _| host.previews.remove(&id)) else {
        let running: Vec<String> = host.read_with(cx, |host, _| {
            host.previews
                .infos()
                .iter()
                .map(|info| info.id.to_string())
                .collect()
        });
        let listed = if running.is_empty() {
            "none is running".to_owned()
        } else {
            format!("running: {}", running.join(", "))
        };
        return Err(OpError::NotFound(format!("no preview {id}; {listed}")));
    };
    let exe = preview::exe()?;
    cx.background_spawn(async move { preview::shut_down(running, &exe) })
        .await?;
    Ok(OpResult::Done)
}

// Shows only previews that still run. One that has exited is ended and its folder removed.
fn list(host: &Entity<PluginHost>, cx: &mut AsyncApp) -> Result<OpResult> {
    let dead = host.update(cx, |host, _| host.previews.take_dead());
    if !dead.is_empty() {
        let exe = preview::exe().unwrap_or_default();
        cx.background_spawn(async move {
            for running in dead {
                drop(preview::shut_down(running, &exe));
            }
        })
        .detach();
    }
    Ok(OpResult::PluginPreviews(
        host.read_with(cx, |host, _| host.previews.infos()),
    ))
}
