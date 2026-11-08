use kg_domain::{
    Conversation, EnvironmentId, EnvironmentStatus, Event, EventKind, Metadata, PluginId,
    ProjectId, Thread, ThreadId, ThreadMark, ThreadStatus, Timestamp,
};
use rusqlite::{Connection, OptionalExtension as _, Row, named_params, params};
use serde_json::Value;

use super::environment::personal_folders;
use super::event::append_events;
use super::{found, not_found};
use crate::column::{Json, Text};
use crate::{Error, Result};

const THREAD_COLUMNS: &str = "id, project_id, environment_id, title, provider, status, \
    visibility, parent_thread_id, lifecycle_owner_thread_id, section_id, pinned, last_read_at, \
    latest_attention_at, archived_at, created_at, updated_at, title_locked, done";

const OWNED_THREADS: &str = "WITH RECURSIVE owned(id) AS ( \
    SELECT id FROM threads WHERE id = ?1 \
    UNION SELECT threads.id FROM threads JOIN owned ON threads.lifecycle_owner_thread_id = owned.id)";

impl super::Store {
    pub fn put_thread(&self, thread: &Thread) -> Result<()> {
        put_thread(&self.conn, thread)
    }

    /// Recovers one thread in one commit: its record, its own events, the questions it held, and each
    /// parent notice. If any write fails, none is kept, so the next start retries the whole thread.
    pub fn recover_thread(
        &mut self,
        thread: &Thread,
        at: Timestamp,
        own: impl IntoIterator<Item = EventKind>,
        notices: impl IntoIterator<Item = (ThreadId, EventKind)>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        put_thread(&tx, thread)?;
        tx.execute(
            "DELETE FROM pending_interactions WHERE thread_id = ?1",
            [Text(&thread.id)],
        )?;
        append_events(&tx, &thread.id, at, own)?;
        for (parent, kind) in notices {
            append_events(&tx, &parent, at, [kind])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Attaches a new thread to a ready environment with its metadata and first events in one commit.
    pub fn create_thread(
        &mut self,
        thread: &Thread,
        metadata: Option<&Metadata>,
        kinds: impl IntoIterator<Item = EventKind>,
    ) -> Result<Vec<Event>> {
        let tx = self.conn.transaction()?;
        let environment = thread.environment_id.as_ref().ok_or_else(|| {
            Error::Conflict("environment must be ready and belong to this project".into())
        })?;
        let folder: std::path::PathBuf = tx
            .query_row(
                "SELECT path FROM environments WHERE id = ?1 AND project_id = ?2 AND status = ?3",
                params![
                    Text(environment),
                    Text(&thread.project_id),
                    Text(EnvironmentStatus::Ready)
                ],
                |row| Ok(row.get::<_, Text<_>>(0)?.0),
            )
            .optional()?
            .ok_or_else(|| {
                Error::Conflict("environment must be ready and belong to this project".into())
            })?;
        if super::environment::removal_pending(&tx, &folder)? {
            return Err(Error::Conflict("that folder is being removed".into()));
        }
        tx.execute(
            "UPDATE environments SET retire_at = NULL WHERE id = ?1",
            [Text(environment)],
        )?;
        put_thread(&tx, thread)?;
        if let Some(metadata) = metadata {
            set_thread_metadata(&tx, &thread.id, &metadata.plugin_id, &metadata.value)?;
        }
        let events = append_events(&tx, &thread.id, thread.created_at, kinds)?;
        tx.commit()?;
        Ok(events)
    }

    pub fn thread(&self, id: &ThreadId) -> Result<Option<Thread>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {THREAD_COLUMNS} FROM threads WHERE id = ?1"),
                [Text(id)],
                thread_row,
            )
            .optional()?)
    }

    /// Newest first by `created_at`; `archived` and `before` filter only when given.
    /// Every thread, visible or hidden, archived or live.
    pub fn all_threads(&self) -> Result<Vec<Thread>> {
        self.threads(None, None, None, None)
    }

    pub fn threads(
        &self,
        project_id: Option<&kg_domain::ProjectId>,
        archived: Option<bool>,
        limit: Option<u32>,
        before: Option<Timestamp>,
    ) -> Result<Vec<Thread>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {THREAD_COLUMNS} FROM threads
             WHERE (?1 IS NULL OR project_id = ?1)
               AND (?2 IS NULL OR (archived_at IS NOT NULL) = ?2)
               AND (?3 IS NULL OR created_at < ?3)
             ORDER BY created_at DESC, id DESC
             LIMIT ?4"
        ))?;
        let threads = statement
            .query_map(
                params![
                    Text(project_id),
                    archived,
                    before.map(|at| at.0),
                    limit.map_or(-1, i64::from),
                ],
                thread_row,
            )?
            .collect::<Result<_, _>>()?;
        Ok(threads)
    }

    pub fn set_thread_status(
        &self,
        id: &ThreadId,
        status: ThreadStatus,
        at: Timestamp,
    ) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET status = ?2, updated_at = ?3,
                latest_attention_at = CASE
                    WHEN ?2 = ?4 AND status IN (?5, ?6) THEN ?3
                    ELSE latest_attention_at END
             WHERE id = ?1",
            params![
                Text(id),
                Text(status),
                at.0,
                Text(ThreadStatus::Error),
                Text(ThreadStatus::Active),
                Text(ThreadStatus::Starting)
            ],
        )?;
        found(changed, "thread", id)
    }

    pub fn conversation(&self, id: &ThreadId) -> Result<Option<Conversation>> {
        let conversation: Option<Text<Option<Conversation>>> = self
            .conn
            .query_row(
                "SELECT conversation FROM threads WHERE id = ?1",
                [Text(id)],
                |row| row.get(0),
            )
            .optional()?;
        conversation
            .map(|Text(conversation)| conversation)
            .ok_or_else(|| not_found("thread", id))
    }

    pub fn set_conversation(&self, id: &ThreadId, conversation: Conversation) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET conversation = ?2 WHERE id = ?1",
            params![Text(id), Text(conversation)],
        )?;
        found(changed, "thread", id)
    }

    pub fn conversation_ids(&self) -> Result<Vec<Conversation>> {
        let mut statement = self
            .conn
            .prepare("SELECT conversation FROM threads WHERE conversation IS NOT NULL")?;
        let ids = statement
            .query_map([], |row| Ok(row.get::<_, Text<_>>(0)?.0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Sets the conversation when this start is still current, the row is still running, and the
    /// column is still `expect`. An id another thread holds is left where it is.
    pub fn claim_conversation(
        &self,
        id: &ThreadId,
        conversation: Conversation,
        expect: Option<Conversation>,
        since: Timestamp,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE threads SET conversation = ?1
             WHERE id = ?2
               AND status IN (?5, ?6)
               AND ((?3 IS NULL AND conversation IS NULL) OR conversation = ?3)
               AND NOT EXISTS (
                 SELECT 1 FROM threads AS other
                 WHERE other.conversation = ?1 AND other.id != ?2
               )
               AND (
                 SELECT at FROM events
                 WHERE thread_id = ?2
                   AND json_extract(kind, '$.type') = 'started'
                 ORDER BY seq DESC LIMIT 1
               ) = ?4",
            params![
                Text(conversation),
                Text(id),
                Text(expect),
                since.0,
                Text(ThreadStatus::Active),
                Text(ThreadStatus::Starting)
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn set_pinned(&self, id: &ThreadId, pinned: bool) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET pinned = ?2 WHERE id = ?1",
            params![Text(id), pinned],
        )?;
        found(changed, "thread", id)
    }

    pub fn set_title(&self, id: &ThreadId, title: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET title = ?2, title_locked = 1 WHERE id = ?1",
            params![Text(id), title],
        )?;
        found(changed, "thread", id)
    }

    pub fn set_title_once(&self, id: &ThreadId, title: &str) -> Result<bool> {
        let locked = self
            .conn
            .query_row(
                "SELECT title_locked FROM threads WHERE id = ?1",
                [Text(id)],
                |row| row.get(0),
            )
            .optional()?;
        let Some(locked) = locked else {
            return Err(not_found("thread", id));
        };
        if locked {
            return Ok(false);
        }
        self.set_title(id, title)?;
        Ok(true)
    }

    pub fn mark_read(&self, id: &ThreadId, at: Timestamp) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET last_read_at = ?2, done = 0 WHERE id = ?1",
            params![Text(id), at.0],
        )?;
        found(changed, "thread", id)
    }

    pub fn set_attention(&self, id: &ThreadId, at: Timestamp) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET latest_attention_at = ?2 WHERE id = ?1",
            params![Text(id), at.0],
        )?;
        found(changed, "thread", id)
    }

    pub fn mark_done(&self, id: &ThreadId) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET done = 1 WHERE id = ?1",
            params![Text(id)],
        )?;
        found(changed, "thread", id)
    }

    /// Archives the thread and its lifecycle dependents; returns an `Archived` event per thread it archived.
    pub fn archive_thread(&mut self, id: &ThreadId, at: Timestamp) -> Result<Vec<Event>> {
        let tx = self.conn.transaction()?;
        if owned_threads(&tx, id)?.is_empty() {
            return Err(not_found("thread", id));
        }
        let mut statement = tx.prepare(&format!(
            "{OWNED_THREADS} UPDATE threads SET archived_at = ?2
             WHERE id IN owned AND archived_at IS NULL RETURNING id"
        ))?;
        let archived: Vec<ThreadId> = statement
            .query_map(params![Text(id), at.0], |row| {
                Ok(row.get::<_, Text<_>>(0)?.0)
            })?
            .collect::<Result<_, _>>()?;
        drop(statement);
        let mut events = Vec::new();
        for id in &archived {
            events.extend(append_events(&tx, id, at, [EventKind::Archived])?);
        }
        tx.commit()?;
        Ok(events)
    }

    pub fn unarchive_thread(&mut self, id: &ThreadId, at: Timestamp) -> Result<Vec<Event>> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE threads SET archived_at = NULL WHERE id = ?1 AND archived_at IS NOT NULL",
            [Text(id)],
        )?;
        if changed == 0 {
            found(
                tx.query_row(
                    "SELECT COUNT(*) FROM threads WHERE id = ?1",
                    [Text(id)],
                    |row| row.get(0),
                )?,
                "thread",
                id,
            )?;
            return Ok(Vec::new());
        }
        tx.execute("UPDATE environments SET retire_at = NULL WHERE id = (SELECT environment_id FROM threads WHERE id = ?1)", [Text(id)])?;
        let events = append_events(&tx, id, at, [EventKind::Unarchived])?;
        tx.commit()?;
        Ok(events)
    }

    pub fn owned_threads(&self, id: &ThreadId) -> Result<Vec<ThreadId>> {
        owned_threads(&self.conn, id)
    }

    /// Deletes the thread, its lifecycle dependents and all their history, and marks `folders` destroyed in the same
    /// commit. `folders` must be what `personal_folders` returned; if it changed, nothing is deleted. Returns a `Deleted` event per thread and that thread's project, read before the row goes.
    pub fn delete_thread(
        &self,
        id: &ThreadId,
        at: Timestamp,
        folders: &[EnvironmentId],
    ) -> Result<Vec<(Event, ProjectId)>> {
        let tx = self.conn.unchecked_transaction()?;
        let owned = owned_threads(&tx, id)?;
        if owned.is_empty() {
            return Err(not_found("thread", id));
        }
        let mut found: Vec<EnvironmentId> = personal_folders(&tx, id)?
            .into_iter()
            .map(|folder| folder.id)
            .collect();
        let mut expected = folders.to_vec();
        found.sort();
        expected.sort();
        if found != expected {
            return Err(Error::Conflict(
                "a folder changed while you decided; delete again".into(),
            ));
        }
        let events = super::event::append_deleted(&tx, &owned, at)?;
        tx.execute("DELETE FROM threads WHERE id = ?1", [Text(id)])?;
        for folder in &found {
            tx.execute(
                "UPDATE environments SET status = ?2 WHERE id = ?1",
                params![Text(folder), Text(EnvironmentStatus::Destroyed)],
            )?;
        }
        tx.commit()?;
        Ok(events)
    }

    pub fn thread_metadata(&self, id: &ThreadId, plugin_id: &PluginId) -> Result<Option<Value>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM thread_metadata WHERE thread_id = ?1 AND plugin_id = ?2",
                params![Text(id), Text(plugin_id)],
                |row| Ok(row.get::<_, Json<_>>(0)?.0),
            )
            .optional()?)
    }

    pub fn set_thread_metadata(
        &self,
        id: &ThreadId,
        plugin_id: &PluginId,
        value: &Value,
    ) -> Result<()> {
        set_thread_metadata(&self.conn, id, plugin_id, value)
    }
}

fn set_thread_metadata(
    conn: &Connection,
    id: &ThreadId,
    plugin_id: &PluginId,
    value: &Value,
) -> Result<()> {
    conn.execute(
        "INSERT INTO thread_metadata (thread_id, plugin_id, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(thread_id, plugin_id) DO UPDATE SET value = excluded.value",
        params![Text(id), Text(plugin_id), Json(value)],
    )?;
    Ok(())
}

pub(super) fn owned_threads(conn: &Connection, id: &ThreadId) -> Result<Vec<ThreadId>> {
    let mut statement = conn.prepare(&format!("{OWNED_THREADS} SELECT id FROM owned"))?;
    let owned = statement
        .query_map([Text(id)], |row| Ok(row.get::<_, Text<_>>(0)?.0))?
        .collect::<Result<_, _>>()?;
    Ok(owned)
}

fn thread_row(row: &Row) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: row.get::<_, Text<_>>("id")?.0,
        project_id: row.get::<_, Text<_>>("project_id")?.0,
        environment_id: row.get::<_, Text<_>>("environment_id")?.0,
        title: row.get("title")?,
        provider: row.get("provider")?,
        status: row.get::<_, Text<_>>("status")?.0,
        visibility: row.get::<_, Text<_>>("visibility")?.0,
        parent_thread_id: row.get::<_, Text<_>>("parent_thread_id")?.0,
        lifecycle_owner_thread_id: row.get::<_, Text<_>>("lifecycle_owner_thread_id")?.0,
        section_id: row.get::<_, Text<_>>("section_id")?.0,
        pinned: row.get("pinned")?,
        last_read_at: row.get::<_, Option<i64>>("last_read_at")?.map(Timestamp),
        latest_attention_at: Timestamp(row.get("latest_attention_at")?),
        mark: if row.get("done")? {
            ThreadMark::Done
        } else {
            ThreadMark::None
        },
        archived_at: row.get::<_, Option<i64>>("archived_at")?.map(Timestamp),
        created_at: Timestamp(row.get("created_at")?),
        updated_at: Timestamp(row.get("updated_at")?),
        title_locked: row.get("title_locked")?,
    })
}

fn put_thread(conn: &Connection, thread: &Thread) -> Result<()> {
    conn.execute(
        &super::upsert("threads", THREAD_COLUMNS),
        named_params! {
            ":id": Text(&thread.id),
            ":project_id": Text(&thread.project_id),
            ":environment_id": Text(&thread.environment_id),
            ":title": thread.title,
            ":provider": thread.provider,
            ":status": Text(thread.status),
            ":visibility": Text(thread.visibility),
            ":parent_thread_id": Text(&thread.parent_thread_id),
            ":lifecycle_owner_thread_id": Text(&thread.lifecycle_owner_thread_id),
            ":section_id": Text(&thread.section_id),
            ":pinned": thread.pinned,
            ":last_read_at": thread.last_read_at.map(|at| at.0),
            ":latest_attention_at": thread.latest_attention_at.0,
            ":archived_at": thread.archived_at.map(|at| at.0),
            ":created_at": thread.created_at.0,
            ":updated_at": thread.updated_at.0,
            ":title_locked": thread.title_locked,
            ":done": thread.mark == ThreadMark::Done,
        },
    )?;
    Ok(())
}
