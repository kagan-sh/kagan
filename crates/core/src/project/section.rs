use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{OpError, OpResult, Section, SectionId, SectionOp};

use crate::Workspace;
use crate::host::database::{Database, Result};
use crate::workspace::id;

use crate::thread::thread;

fn named(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        Err(OpError::Invalid("a section needs a name".into()))
    } else {
        Ok(name.to_owned())
    }
}
pub(crate) async fn section(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: SectionOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        SectionOp::Create { name, thread_id } => {
            let name = named(&name)?;
            if let Some(id) = &thread_id {
                thread(this, id, cx)?;
            }
            let section = Section {
                id: SectionId(id("sec")),
                name,
            };
            database
                .call(move |store| {
                    store.put_section(&section)?;
                    if let Some(thread_id) = &thread_id {
                        store.set_section(thread_id, Some(&section.id))?;
                    }
                    Ok(())
                })
                .await?;
        }
        SectionOp::Rename { id, name } => {
            let name = named(&name)?;
            database
                .call(move |store| store.rename_section(&id, &name))
                .await?;
        }
        SectionOp::Delete { id } => {
            database
                .call(move |store| store.delete_section(&id))
                .await?;
        }
        SectionOp::File {
            thread_id,
            section_id,
        } => {
            thread(this, &thread_id, cx)?;
            database
                .call(move |store| store.set_section(&thread_id, section_id.as_ref()))
                .await?;
        }
    }
    Workspace::refresh(this, cx).await?;
    Ok(OpResult::Done)
}
