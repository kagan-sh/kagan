use std::path::Path;

use kg_domain::{Plugin, PluginId};
use rusqlite::{OptionalExtension as _, params};
use serde_json::Value;

use super::found;
use crate::Result;
use crate::column::{Json, Text};

impl super::Store {
    pub fn insert_plugin(&self, plugin: &Plugin) -> Result<()> {
        self.conn.execute(
            "INSERT INTO plugins (id, root, source, commit_id, enabled, approved)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Text(&plugin.id),
                Text(&plugin.root),
                plugin.source,
                plugin.commit,
                plugin.enabled,
                Json(&plugin.approved),
            ],
        )?;
        Ok(())
    }

    pub fn set_plugin_enabled(&self, id: &PluginId, enabled: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE plugins SET enabled = ?1 WHERE id = ?2",
            params![enabled, Text(id)],
        )?;
        found(changed, "plugin", id)
    }

    pub fn set_plugin_approved(&self, id: &PluginId, approved: &Value) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE plugins SET approved = ?1 WHERE id = ?2",
            params![Json(approved), Text(id)],
        )?;
        found(changed, "plugin", id)
    }

    pub fn keep_plugin_checkout(
        &self,
        id: &PluginId,
        root: &Path,
        source: &str,
        commit: &Option<String>,
        approved: &Value,
    ) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE plugins SET root = ?1, source = ?2, commit_id = ?3, approved = ?4 WHERE id = ?5",
            params![Text(root), source, commit, Json(approved), Text(id)],
        )?;
        found(changed, "plugin", id)
    }

    pub fn delete_plugin(&self, id: &PluginId) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM plugin_values WHERE plugin_id = ?1",
            params![Text(id)],
        )?;
        transaction.execute("DELETE FROM plugins WHERE id = ?1", params![Text(id)])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn plugins(&self) -> Result<Vec<Plugin>> {
        let mut statement = self.conn.prepare(
            "SELECT id, root, source, commit_id, enabled, approved FROM plugins ORDER BY id",
        )?;
        let plugins = statement
            .query_map([], |row| {
                Ok(Plugin {
                    id: row.get::<_, Text<_>>(0)?.0,
                    root: row.get::<_, Text<_>>(1)?.0,
                    source: row.get(2)?,
                    commit: row.get(3)?,
                    enabled: row.get(4)?,
                    approved: row.get::<_, Json<_>>(5)?.0,
                    error: None,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(plugins)
    }

    pub fn plugin_value(&self, plugin_id: &PluginId, key: &str) -> Result<Option<Value>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM plugin_values WHERE plugin_id = ?1 AND key = ?2",
                params![Text(plugin_id), key],
                |row| Ok(row.get::<_, Json<_>>(0)?.0),
            )
            .optional()?)
    }

    pub fn set_plugin_value(&self, plugin_id: &PluginId, key: &str, value: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO plugin_values (plugin_id, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value",
            params![Text(plugin_id), key, Json(value)],
        )?;
        Ok(())
    }
}
