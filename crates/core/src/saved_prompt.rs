use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{OpResult, SavedPrompt, SavedPromptId, SavedPromptOp};

use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::workspace::{id, missing};

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: SavedPromptOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        SavedPromptOp::Add { title, body } => {
            let prompt = SavedPrompt {
                id: SavedPromptId(id("svp")),
                title: title.trim().to_owned(),
                body,
                starred: false,
            };
            let stored = prompt.clone();
            database
                .call(move |store| store.insert_saved_prompt(&stored))
                .await?;
            Ok(OpResult::SavedPrompt(prompt))
        }
        SavedPromptOp::Search { text } => {
            let text = text.trim().to_owned();
            let prompts = database
                .call(move |store| store.search_saved_prompts(&text))
                .await?;
            Ok(OpResult::SavedPrompts(prompts))
        }
        SavedPromptOp::Star { id, starred } => {
            let lookup = id.clone();
            let prompt = database
                .call(move |store| {
                    store.set_saved_prompt_starred(&lookup, starred)?;
                    store.saved_prompt(&lookup)
                })
                .await?
                .ok_or_else(|| missing("saved prompt", &id))?;
            Ok(OpResult::SavedPrompt(prompt))
        }
        SavedPromptOp::Delete { id } => {
            database
                .call(move |store| store.delete_saved_prompt(&id))
                .await?;
            Ok(OpResult::Done)
        }
        SavedPromptOp::Send { id, thread_id } => {
            let lookup = id.clone();
            let prompt = database
                .call(move |store| store.saved_prompt(&lookup))
                .await?
                .ok_or_else(|| missing("saved prompt", &id))?;
            crate::thread::send(this, thread_id, &prompt.body, cx).await
        }
    }
}
