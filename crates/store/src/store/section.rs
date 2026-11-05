use kg_domain::{Section, SectionId, ThreadId};
use rusqlite::{OptionalExtension as _, params};

use super::{found, not_found};
use crate::Result;
use crate::column::Text;

impl super::Store {
    pub fn put_section(&self, section: &Section) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sections (id, name) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![Text(&section.id), section.name],
        )?;
        Ok(())
    }

    pub fn sections(&self) -> Result<Vec<Section>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name FROM sections ORDER BY name, id")?;
        let sections = statement
            .query_map([], |row| {
                Ok(Section {
                    id: row.get::<_, Text<_>>(0)?.0,
                    name: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(sections)
    }

    pub fn delete_section(&self, id: &SectionId) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM sections WHERE id = ?1", [Text(id)])?;
        found(changed, "section", id)
    }

    pub fn rename_section(&self, id: &SectionId, name: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE sections SET name = ?2 WHERE id = ?1",
            params![Text(id), name],
        )?;
        found(changed, "section", id)
    }

    pub fn set_section(&self, id: &ThreadId, section: Option<&SectionId>) -> Result<()> {
        if let Some(section) = section {
            let exists = self
                .conn
                .query_row(
                    "SELECT 1 FROM sections WHERE id = ?1",
                    [Text(section)],
                    |_| Ok(1_i32),
                )
                .optional()?;
            if exists.is_none() {
                return Err(not_found("section", section));
            }
        }
        let changed = self.conn.execute(
            "UPDATE threads SET section_id = ?2 WHERE id = ?1",
            params![Text(id), section.map(Text)],
        )?;
        found(changed, "thread", id)
    }
}
