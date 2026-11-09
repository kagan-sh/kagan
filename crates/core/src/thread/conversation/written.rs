use std::path::Path;

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{Conversation, ThreadId};

use crate::Workspace;

use super::{codex_kept, codex_matches, kimi_kept, kimi_matches};

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
            let Ok(stored) = database
                .call(move |store| store.conversation(&thread))
                .await
            else {
                continue;
            };
            let thread = id.clone();
            let Ok(Some(since)) = database
                .call(move |store| store.last_started(&thread))
                .await
            else {
                continue;
            };
            let Ok(root) = this.update(cx, |workspace, _| {
                if kimi {
                    workspace.home("KIMI_CODE_HOME", ".kimi-code")
                } else {
                    workspace.home("CODEX_HOME", ".codex")
                }
            }) else {
                return;
            };
            let folder = folder.clone();
            let look = cx
                .background_spawn(async move {
                    if kimi {
                        let Some(root) = root else {
                            return Look::Missing;
                        };
                        if stored.is_some_and(|conversation| kimi_kept(&root, conversation)) {
                            return Look::Kept;
                        }
                        Look::Files(kimi_matches(&root, &folder, since))
                    } else {
                        let Some(root) = root else {
                            return Look::Missing;
                        };
                        let sessions = root.join("sessions");
                        if stored.is_some_and(|conversation| codex_kept(&sessions, conversation)) {
                            return Look::Kept;
                        }
                        Look::Files(codex_matches(&sessions, &folder, since))
                    }
                })
                .await;
            let Look::Files(matches) = look else {
                if matches!(look, Look::Kept) {
                    return;
                }
                continue;
            };
            let Ok(taken) = database.call(|store| store.conversation_ids()).await else {
                continue;
            };
            let mut free = matches
                .into_iter()
                .filter(|conversation| !taken.contains(conversation));
            let Some(conversation) = free.next() else {
                continue;
            };
            if free.next().is_some() {
                continue;
            }
            let thread = id.clone();
            if database
                .call(move |store| store.claim_conversation(&thread, conversation, stored, since))
                .await
                == Ok(true)
            {
                return;
            }
        }
    })
    .detach();
}

enum Look {
    Missing,
    Kept,
    Files(Vec<Conversation>),
}

fn running(this: &WeakEntity<Workspace>, id: &ThreadId, cx: &mut AsyncApp) -> bool {
    this.update(cx, |workspace, cx| {
        workspace
            .threads
            .get(id)
            .is_some_and(|thread| thread.read(cx).record.status.live())
    })
    .unwrap_or(false)
}
