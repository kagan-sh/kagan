use kg_domain::{OpError, OpResult, SavedProgram, saved_name};

use crate::host::database::{Database, Result};
use crate::workspace::missing;

// Each change is one database call, so a name is never taken twice and a rename is all or nothing.

pub(crate) async fn save(database: Database, name: String, line: String) -> Result<OpResult> {
    let name = saved_name(&name).map_err(OpError::Invalid)?;
    let program = SavedProgram {
        name,
        line: line.trim().to_owned(),
        pinned: false,
    };
    database
        .call(move |store| store.upsert_saved_program(&program))
        .await?;
    Ok(OpResult::Done)
}

enum Renamed {
    Missing(String),
    Taken,
    Done,
}

pub(crate) async fn rename(database: Database, name: String, to: String) -> Result<OpResult> {
    let to = saved_name(&to).map_err(OpError::Invalid)?;
    let from = name.trim().to_owned();
    let target = to.clone();
    // The same name in another case is this program, so renaming to it only changes the case.
    let outcome = database
        .call(move |store| {
            if store.saved_program(&from)?.is_none() {
                return Ok(Renamed::Missing(from));
            }
            if !target.eq_ignore_ascii_case(&from) && store.saved_program(&target)?.is_some() {
                return Ok(Renamed::Taken);
            }
            store.rename_saved_program(&from, &target)?;
            Ok(Renamed::Done)
        })
        .await?;
    match outcome {
        Renamed::Missing(from) => Err(missing("saved program", &from)),
        Renamed::Taken => Err(OpError::Invalid(format!(
            "a saved program named {to} already exists"
        ))),
        Renamed::Done => Ok(OpResult::Done),
    }
}

pub(crate) async fn remove(database: Database, name: String) -> Result<OpResult> {
    let name = name.trim().to_owned();
    database
        .call(move |store| store.delete_saved_program(&name))
        .await?;
    Ok(OpResult::Done)
}

pub(crate) async fn pin(database: Database, name: String, pinned: bool) -> Result<OpResult> {
    let name = name.trim().to_owned();
    database
        .call(move |store| store.set_saved_program_pinned(&name, pinned))
        .await?;
    Ok(OpResult::Done)
}
