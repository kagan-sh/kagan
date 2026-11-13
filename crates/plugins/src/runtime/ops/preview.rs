use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_core::{Requester, seed_preview};
use kg_domain::{OpError, OpResult, Plugin, PluginPreview, PluginPreviewOp, PreviewId, ThreadId};
use kg_process::Process;

use crate::host::{Hold, hold};
use crate::manifest::Grant;
use crate::runtime::preview::{self, Running};
use crate::{PluginHost, Result};

use super::approve::{approve, refuse_in_preview};
use super::develop::fetch_local;

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
        PluginPreviewOp::List => Ok(list(host, cx)),
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
    let root = cx
        .background_spawn(async move { dunce::canonicalize(path) })
        .await?;
    let fetched = fetch_local(host, root, cx).await?;
    let manifest = &fetched.discovered.manifest;
    let grant = Grant::of(manifest, &fetched.root);
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let id = preview::new_id();
    let folder = preview::folder(&data_dir, &id);
    let socket = preview::socket_for(&folder);
    #[cfg(unix)]
    {
        preview::check_socket_length(&socket)?;
    }
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
        thread_id,
        path: fetched.root.clone(),
        data_dir: folder.join("data"),
        socket,
    };
    host.update(cx, |host, _| host.previews.reserve(id.clone()));
    let built = build(host, &info, &folder, plugin, cx).await;
    host.update(cx, |host, _| host.previews.release(&id));
    let (app, watch) = match built {
        Ok(processes) => processes,
        Err(error) => {
            // The error already says what the log ends with, so the folder can go.
            drop(preview::remove(&folder));
            return Err(error);
        }
    };
    let (exe, old) = host.update(cx, |host, _| {
        let exe = host.previews.exe.clone();
        let old = host
            .previews
            .replace(Running::new(info.clone(), folder, app, watch));
        (exe, old)
    });
    if let Some(old) = old {
        drop(preview::shut_down_later(
            old,
            &exe,
            cx.background_executor(),
        ));
    }
    Ok(OpResult::PluginPreview(info))
}

// Prepares the folder, then starts the preview's app and its watch.
async fn build(
    host: &Entity<PluginHost>,
    info: &PluginPreview,
    folder: &Path,
    plugin: Plugin,
    cx: &mut AsyncApp,
) -> Result<(Process, Process)> {
    prepare(host, info, folder, plugin, cx).await?;
    boot(host, info, folder, cx).await
}

// Sweeps leftover folders, makes this one and seeds it with the plugin. A sweep keeps every folder
// a start has reserved, so it cannot remove this one or another start's.
async fn prepare(
    host: &Entity<PluginHost>,
    info: &PluginPreview,
    folder: &Path,
    plugin: Plugin,
    cx: &mut AsyncApp,
) -> Result<()> {
    let (data_dir, keep, exe) = host.read_with(cx, |host, _| {
        (
            host.data_dir.clone(),
            host.previews.ids(),
            host.previews.exe.clone(),
        )
    });
    let folder = folder.to_path_buf();
    let preview_data = info.data_dir.clone();
    cx.background_spawn(async move {
        preview::sweep(&exe, &data_dir, &keep)?;
        std::fs::create_dir_all(&folder)?;
        seed_preview(&preview_data, &plugin)
    })
    .await
}

// Starts the app, waits for it to answer `kg open`, then starts the reload watch. The watch must
// still be running a moment later, or the start fails with its log.
async fn boot(
    host: &Entity<PluginHost>,
    info: &PluginPreview,
    folder: &Path,
    cx: &mut AsyncApp,
) -> Result<(Process, Process)> {
    hold(host, Hold::PreviewStart, cx).await;
    let exe = host.read_with(cx, |host, _| host.previews.exe.clone());
    let app = preview::spawn_app(&exe, folder, &info.data_dir, &info.socket)?;
    let (app, ready) = cx
        .background_spawn({
            let (exe, data, socket, log) = (
                exe.clone(),
                info.data_dir.clone(),
                info.socket.clone(),
                folder.to_path_buf(),
            );
            async move {
                let mut app = app;
                let ready = preview::wait_ready(&exe, &data, &socket, &log, &mut app);
                (app, ready)
            }
        })
        .await;
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
    let (removed, exe) = host.update(cx, |host, _| {
        let exe = host.previews.exe.clone();
        (host.previews.remove(&id), exe)
    });
    let Some(running) = removed else {
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
    cx.background_spawn(async move { preview::shut_down(running, &exe) })
        .await?;
    Ok(OpResult::Done)
}

// Shows only previews that still run. One whose app or watch has exited is ended and its folder
// removed.
fn list(host: &Entity<PluginHost>, cx: &mut AsyncApp) -> OpResult {
    let (dead, exe) = host.update(cx, |host, _| {
        let exe = host.previews.exe.clone();
        (host.previews.take_where(exited), exe)
    });
    if !dead.is_empty() {
        cx.background_spawn(async move {
            for running in dead {
                drop(preview::shut_down(running, &exe));
            }
        })
        .detach();
    }
    OpResult::PluginPreviews(host.read_with(cx, |host, _| host.previews.infos()))
}

// Whether a preview's app or its watch has exited.
fn exited(running: &mut Running) -> bool {
    running.app_exited() || running.watch_exited()
}
