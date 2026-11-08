use std::path::Path;

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::ThreadId;

use crate::Workspace;

use super::{codex_started, home, kimi_root, kimi_started};

pub(super) fn note_when_written(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    folder: &Path,
    kimi: bool,
    cx: &mut AsyncApp,
) {
    let this = this.clone();
    let id = record.id.clone();
    let folder = folder.to_owned();
    cx.spawn(async move |cx| {
        let Ok(database) = crate::workspace::database(&this, cx) else {
            return;
        };
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(1))
                .await;
            if !running(&this, &id, cx) {
                return;
            }
            let thread = id.clone();
            if !matches!(
                database
                    .call(move |store| store.conversation(&thread))
                    .await,
                Ok(None)
            ) {
                return;
            }
            let Ok((codex_home, kimi_home)) = this.update(cx, |workspace, _| {
                (workspace.codex_home.clone(), workspace.kimi_home.clone())
            }) else {
                return;
            };
            let thread = id.clone();
            let Ok(Some(since)) = database
                .call(move |store| store.last_started(&thread))
                .await
            else {
                continue;
            };
            let folder = folder.clone();
            let found = cx
                .background_spawn(async move {
                    if kimi {
                        kimi_started(&kimi_root(kimi_home)?, &folder, since)
                    } else {
                        let sessions = codex_home
                            .or_else(|| home("CODEX_HOME", ".codex"))?
                            .join("sessions");
                        codex_started(&sessions, &folder, since)
                    }
                })
                .await;
            if let Some(conversation) = found {
                let thread = id.clone();
                let _ = database
                    .call(move |store| store.set_conversation(&thread, conversation))
                    .await;
                return;
            }
        }
    })
    .detach();
}

fn running(this: &WeakEntity<Workspace>, id: &ThreadId, cx: &mut AsyncApp) -> bool {
    this.update(cx, |workspace, cx| {
        workspace.threads.get(id).is_some_and(|thread| {
            matches!(
                thread.read(cx).record.status,
                kg_domain::ThreadStatus::Active | kg_domain::ThreadStatus::Starting
            )
        })
    })
    .unwrap_or(false)
}
