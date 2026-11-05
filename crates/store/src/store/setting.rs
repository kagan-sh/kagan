use rusqlite::{OptionalExtension as _, params};
use serde_json::Value;

use crate::Result;
use crate::column::Json;

impl super::Store {
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
