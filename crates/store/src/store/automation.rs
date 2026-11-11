use kg_domain::{Automation, AutomationId, ProjectId, ThreadId, Timestamp};
use rusqlite::{OptionalExtension as _, Row, params};

use super::{found, not_found};
use crate::Result;
use crate::column::Text;

impl super::Store {
    pub fn insert_automation(&self, automation: &Automation) -> Result<()> {
        self.conn.execute(
            "INSERT INTO automations
             (id, project_id, name, body, program, minutes, at, paused, next_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                Text(&automation.id),
                Text(&automation.project_id),
                automation.name,
                automation.body,
                automation.program,
                automation.minutes,
                automation.at.map(|time| time.0),
                automation.paused,
                automation.next_at.map(|time| time.0),
            ],
        )?;
        Ok(())
    }

    pub fn automations(&self, project: Option<&ProjectId>) -> Result<Vec<Automation>> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, name, body, program, minutes, at, paused, next_at
             FROM automations
             WHERE project_id = coalesce(?1, project_id)
             ORDER BY name, id",
        )?;
        let mut automations = statement
            .query_map(params![project.map(Text)], automation_row)?
            .collect::<Result<Vec<_>, _>>()?;
        for automation in &mut automations {
            automation.threads = self.runs(&automation.id)?;
        }
        Ok(automations)
    }

    pub fn due_automations(&self, now: Timestamp) -> Result<Vec<Automation>> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, name, body, program, minutes, at, paused, next_at
             FROM automations
             WHERE paused = 0 AND next_at <= ?1
             ORDER BY next_at, id",
        )?;
        let mut automations = statement
            .query_map([now.0], automation_row)?
            .collect::<Result<Vec<_>, _>>()?;
        for automation in &mut automations {
            automation.threads = self.runs(&automation.id)?;
        }
        Ok(automations)
    }

    pub fn automation(&self, id: &AutomationId) -> Result<Option<Automation>> {
        let mut automation = self
            .conn
            .query_row(
                "SELECT id, project_id, name, body, program, minutes, at, paused, next_at
                 FROM automations WHERE id = ?1",
                [Text(id)],
                automation_row,
            )
            .optional()?;
        if let Some(automation) = &mut automation {
            automation.threads = self.runs(&automation.id)?;
        }
        Ok(automation)
    }

    pub fn set_automation_paused(&self, id: &AutomationId, paused: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE automations SET paused = ?2 WHERE id = ?1",
            params![Text(id), paused],
        )?;
        found(changed, "automation", id)
    }

    pub fn set_automation_next(&self, id: &AutomationId, next_at: Option<Timestamp>) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE automations SET next_at = ?2 WHERE id = ?1",
            params![Text(id), next_at.map(|time| time.0)],
        )?;
        found(changed, "automation", id)
    }

    pub fn record_automation_run(
        &self,
        id: &AutomationId,
        thread_id: &ThreadId,
        at: Timestamp,
        next_at: Option<Timestamp>,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE automations SET next_at = ?2 WHERE id = ?1",
            params![Text(id), next_at.map(|time| time.0)],
        )?;
        if changed == 0 {
            return Err(not_found("automation", id));
        }
        tx.execute(
            "INSERT INTO automation_runs (automation_id, thread_id, started_at)
             VALUES (?1, ?2, ?3)",
            params![Text(id), Text(thread_id), at.0],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_automation(&self, id: &AutomationId) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM automations WHERE id = ?1", [Text(id)])?;
        found(changed, "automation", id)
    }

    fn runs(&self, id: &AutomationId) -> Result<Vec<ThreadId>> {
        let mut statement = self.conn.prepare(
            "SELECT thread_id FROM automation_runs
             WHERE automation_id = ?1 ORDER BY started_at, thread_id",
        )?;
        let runs = statement
            .query_map([Text(id)], |row| Ok(row.get::<_, Text<_>>("thread_id")?.0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(runs)
    }
}

fn automation_row(row: &Row<'_>) -> rusqlite::Result<Automation> {
    Ok(Automation {
        id: row.get::<_, Text<_>>("id")?.0,
        project_id: row.get::<_, Text<_>>("project_id")?.0,
        name: row.get("name")?,
        body: row.get("body")?,
        program: row.get("program")?,
        minutes: row.get("minutes")?,
        at: row.get::<_, Option<i64>>("at")?.map(Timestamp),
        paused: row.get("paused")?,
        next_at: row.get::<_, Option<i64>>("next_at")?.map(Timestamp),
        threads: Vec::new(),
    })
}
