use kg_domain::{PendingInteraction, PendingInteractionId, ThreadId, Timestamp};
use rusqlite::params;

use super::found;
use crate::Result;
use crate::column::{Json, Text};

impl super::Store {
    pub fn put_pending_interaction(&self, interaction: &PendingInteraction) -> Result<()> {
        self.conn.execute(
            "INSERT INTO pending_interactions (id, thread_id, title, options, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET thread_id = excluded.thread_id,
                title = excluded.title, options = excluded.options,
                created_at = excluded.created_at",
            params![
                Text(&interaction.id),
                Text(&interaction.thread_id),
                interaction.title,
                Json(&interaction.options),
                interaction.created_at.0,
            ],
        )?;
        Ok(())
    }

    /// Oldest first.
    pub fn pending_interactions(&self, thread_id: &ThreadId) -> Result<Vec<PendingInteraction>> {
        let mut statement = self.conn.prepare(
            "SELECT id, thread_id, title, options, created_at FROM pending_interactions
             WHERE thread_id = ?1 ORDER BY created_at, rowid",
        )?;
        let interactions = statement
            .query_map([Text(thread_id)], |row| {
                Ok(PendingInteraction {
                    id: row.get::<_, Text<_>>(0)?.0,
                    thread_id: row.get::<_, Text<_>>(1)?.0,
                    title: row.get(2)?,
                    options: row.get::<_, Json<_>>(3)?.0,
                    created_at: Timestamp(row.get(4)?),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(interactions)
    }

    pub fn delete_pending_interaction(&self, id: &PendingInteractionId) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM pending_interactions WHERE id = ?1", [Text(id)])?;
        found(changed, "pending interaction", id)
    }
}
