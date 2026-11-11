use kg_domain::SavedProgram;
use rusqlite::{OptionalExtension as _, named_params};

use super::found;
use crate::Result;

impl super::Store {
    // A name already saved keeps its pin and takes the new line.
    pub fn upsert_saved_program(&self, program: &SavedProgram) -> Result<()> {
        self.conn.execute(
            "INSERT INTO saved_programs (name, line, pinned)
             VALUES (:name, :line, :pinned)
             ON CONFLICT(name) DO UPDATE SET line = excluded.line",
            named_params! {
                ":name": &program.name,
                ":line": &program.line,
                ":pinned": i64::from(program.pinned),
            },
        )?;
        Ok(())
    }

    pub fn saved_program(&self, name: &str) -> Result<Option<SavedProgram>> {
        self.conn
            .query_row(
                "SELECT name, line, pinned FROM saved_programs WHERE name = :name",
                named_params! { ":name": name },
                read_saved_program,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn saved_programs(&self) -> Result<Vec<SavedProgram>> {
        let mut statement = self.conn.prepare(
            "SELECT name, line, pinned FROM saved_programs
             ORDER BY pinned DESC, name COLLATE NOCASE",
        )?;
        let programs = statement
            .query_map([], read_saved_program)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(programs)
    }

    pub fn rename_saved_program(&self, from: &str, to: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE saved_programs SET name = :to WHERE name = :from",
            named_params! { ":from": from, ":to": to },
        )?;
        found(changed, "saved program", &from)
    }

    pub fn set_saved_program_pinned(&self, name: &str, pinned: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE saved_programs SET pinned = :pinned WHERE name = :name",
            named_params! {
                ":name": name,
                ":pinned": i64::from(pinned),
            },
        )?;
        found(changed, "saved program", &name)
    }

    pub fn delete_saved_program(&self, name: &str) -> Result<()> {
        let changed = self.conn.execute(
            "DELETE FROM saved_programs WHERE name = :name",
            named_params! { ":name": name },
        )?;
        found(changed, "saved program", &name)
    }
}

fn read_saved_program(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedProgram> {
    Ok(SavedProgram {
        name: row.get("name")?,
        line: row.get("line")?,
        pinned: row.get::<_, i64>("pinned")? != 0,
    })
}
