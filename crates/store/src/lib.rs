//! SQLite: migrations, typed reads and writes, and the append-only event log.

// Every fallible call returns `Error`: a SQLite failure, or `NotFound` from a write that names a missing row.
#![expect(
    clippy::missing_errors_doc,
    reason = "the store documents its error contract at the module level"
)]

use std::path::PathBuf;

mod column;
mod store;
#[cfg(test)]
mod tests;

pub use store::{ScreenRow, Store};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("{what} {id} not found")]
    NotFound { what: &'static str, id: String },
    #[error("{0}")]
    Conflict(String),
    #[error(
        "this data directory was written by a newer kagan (schema version {found}; this build understands {understood}). Nothing was changed. Open it with that newer kagan, or set KG_DATA_DIR to a different folder"
    )]
    Future { found: i64, understood: i64 },
    #[error(
        "{} was made by a pre-release build of kagan and cannot be opened. Nothing was changed. Move it, and any -wal or -shm file beside it, out of that folder, then start kagan again for a new database",
        path.display()
    )]
    PreRelease { path: PathBuf },
    #[error(
        "{} is not a kagan database, so kagan did not open it. Nothing was changed. Move it out of that folder, or set KG_DATA_DIR to a different folder",
        path.display()
    )]
    Foreign { path: PathBuf },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn migrate<'a>(
    conn: &mut rusqlite::Connection,
    migrations: impl IntoIterator<Item = store::Migration<'a>>,
) -> Result<()> {
    let tx = conn.transaction()?;
    let applied: usize = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let mut version = applied;
    for migration in migrations.into_iter().skip(applied) {
        match migration {
            store::Migration::Sql(sql) => tx.execute_batch(sql)?,
            store::Migration::Guarded { sql, table, column } => {
                if !has_column(&tx, table, column)? {
                    tx.execute_batch(sql)?;
                }
            }
            store::Migration::Step(step) => step(&tx)?,
        }
        version += 1;
    }
    if version != applied {
        tx.pragma_update(None, "user_version", version)?;
    }
    tx.commit()?;
    Ok(())
}

fn has_column(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        [table, column],
        |row| row.get(0),
    )?)
}
