use std::collections::BTreeSet;

use kg_domain::ThreadId;
use rusqlite::{OptionalExtension as _, params};
use serde_json::Value;

use crate::Result;
use crate::column::Json;

const RESUME_THREADS: &str = "resume-threads";

impl super::Store {
    /// Threads a quit left running, so the next open starts them again. Not a preference.
    pub fn resume_threads(&self) -> Result<BTreeSet<ThreadId>> {
        let Some(value) = self.setting(RESUME_THREADS)? else {
            return Ok(BTreeSet::new());
        };
        let Some(ids) = value.as_array() else {
            return Ok(BTreeSet::new());
        };
        Ok(ids
            .iter()
            .filter_map(|id| id.as_str().map(|id| ThreadId(id.to_owned())))
            .collect())
    }

    pub fn set_resume_threads(&self, ids: &BTreeSet<ThreadId>) -> Result<()> {
        let value = Value::Array(ids.iter().map(|id| Value::String(id.0.clone())).collect());
        self.set_setting(RESUME_THREADS, &value)
    }

    pub fn remember_resume(&mut self, ids: &[ThreadId]) -> Result<()> {
        let mut remembered = self.resume_threads()?;
        remembered.extend(ids.iter().cloned());
        self.set_resume_threads(&remembered)
    }

    pub fn setting(&self, key: &str) -> Result<Option<Value>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                Ok(row.get::<_, Json<_>>(0)?.0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, Json(value)],
        )?;
        Ok(())
    }
}
