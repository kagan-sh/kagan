use std::{fmt::Display, io::Read as _, path::Path};

use kg_domain::{
    Conversation, Environment, EnvironmentId, Event, EventKind, Order, PendingInteraction,
    PendingInteractionId, Plugin, PluginId, Project, ProjectId, ProjectKind, Section, SectionId,
    Thread, ThreadId, ThreadMark, Timestamp,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, Transaction, params};
use serde_json::Value;

use crate::{
    Error, Result,
    column::{Json, Text},
    migrate,
};

// Marks a file this schema made, so a pre-release file is never mistaken for one (decision 106).
pub(crate) const APPLICATION_ID: i32 = 0x6b67_6e31;

pub(crate) const MIGRATIONS: &[&str] = &[
    r"
PRAGMA application_id = 1801940529;
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    git_remote_url TEXT
);
CREATE TABLE environments (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    status TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'project',
    branch TEXT,
    base_branch TEXT,
    retire_at INTEGER,
    error TEXT
);
CREATE INDEX environments_project ON environments(project_id);
CREATE TABLE sections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    environment_id TEXT REFERENCES environments(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    reasoning_level TEXT NOT NULL,
    status TEXT NOT NULL,
    visibility TEXT NOT NULL,
    parent_thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
    lifecycle_owner_thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
    section_id TEXT REFERENCES sections(id) ON DELETE SET NULL,
    pinned INTEGER NOT NULL,
    last_read_at INTEGER,
    latest_attention_at INTEGER NOT NULL,
    archived_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    title_locked INTEGER NOT NULL DEFAULT 0,
    done INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX threads_project_created ON threads(project_id, created_at);
CREATE INDEX threads_created ON threads(created_at);
CREATE INDEX threads_parent ON threads(parent_thread_id);
CREATE INDEX threads_lifecycle_owner ON threads(lifecycle_owner_thread_id);
CREATE INDEX threads_section ON threads(section_id);
CREATE TABLE events (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
) WITHOUT ROWID;
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
WHEN EXISTS (SELECT 1 FROM threads WHERE id = OLD.thread_id)
BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
CREATE INDEX events_rewinds ON events(thread_id, seq)
WHERE json_extract(kind, '$.type') = 'rewound';
CREATE VIEW current_events AS
SELECT event.* FROM events event WHERE NOT EXISTS (
    SELECT 1 FROM events rewind WHERE rewind.thread_id = event.thread_id
    AND json_extract(rewind.kind, '$.type') = 'rewound'
    AND event.seq >= json_extract(rewind.kind, '$.data.beforeSeq')
    AND event.seq < rewind.seq
);
CREATE TABLE pending_interactions (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    options TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX pending_interactions_thread ON pending_interactions(thread_id);
CREATE TABLE thread_metadata (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    plugin_id TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (thread_id, plugin_id)
) WITHOUT ROWID;
CREATE TABLE terminal_screens (
    thread_id TEXT PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    started_seq INTEGER NOT NULL,
    cols INTEGER NOT NULL,
    rows INTEGER NOT NULL,
    replay BLOB NOT NULL,
    history_dropped INTEGER NOT NULL,
    screen_kept INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE plugins (
    id TEXT PRIMARY KEY,
    root TEXT NOT NULL,
    source TEXT NOT NULL,
    commit_id TEXT,
    enabled INTEGER NOT NULL,
    approved TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE plugin_values (
    plugin_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
) WITHOUT ROWID;
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
",
    "ALTER TABLE threads ADD COLUMN conversation TEXT;",
    r"
ALTER TABLE projects ADD COLUMN sort INTEGER NOT NULL DEFAULT 0;
UPDATE projects SET sort = (
    SELECT COUNT(*) FROM projects AS other
    WHERE other.kind = projects.kind
      AND (other.name < projects.name
        OR (other.name = projects.name AND other.id < projects.id))
);
",
];

// Matches `kg_terminal::SNAPSHOT_MAX`. History and the visible redraw share this bound.
const SCREEN_MAX_BYTES: usize = 4 * 1024 * 1024;
const SCREEN_MIN_COLS: i64 = 2;
const SCREEN_MAX_COLS: i64 = 1000;
const SCREEN_MIN_ROWS: i64 = 1;
const SCREEN_MAX_ROWS: i64 = 500;

/// `started_seq` is the `Started` event of the run that produced the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenRow {
    pub thread_id: ThreadId,
    pub started_seq: u64,
    pub cols: u16,
    pub rows: u16,
    pub replay: Vec<u8>,
    pub history_dropped: u32,
    pub screen_kept: bool,
}

const THREAD_COLUMNS: &str = "id, project_id, environment_id, title, provider, model, \
    reasoning_level, status, visibility, parent_thread_id, lifecycle_owner_thread_id, \
    section_id, pinned, last_read_at, latest_attention_at, archived_at, created_at, \
    updated_at, title_locked, done";

const OWNED_THREADS: &str = "WITH RECURSIVE owned(id) AS ( \
    SELECT id FROM threads WHERE id = ?1 \
    UNION SELECT threads.id FROM threads JOIN owned ON threads.lifecycle_owner_thread_id = owned.id)";

pub struct Store {
    pub(crate) conn: Connection,
}

// Reads the SQLite file header, so a file that is not kagan's is refused before SQLite opens it.
fn refuse_unknown(path: &Path) -> Result<()> {
    let mut header = Vec::with_capacity(100);
    std::fs::File::open(path)?
        .take(100)
        .read_to_end(&mut header)?;
    if header.is_empty() {
        return Ok(());
    }
    let foreign = || Error::Foreign {
        path: path.to_owned(),
    };
    if header.len() < 100 || !header.starts_with(b"SQLite format 3\0") {
        return Err(foreign());
    }
    let field = |at: usize| {
        i32::from_be_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    let (schema_cookie, version, application) = (field(40), field(60), field(68));
    if application != APPLICATION_ID {
        return match (version, schema_cookie) {
            (0, 0) => Ok(()),
            (0, _) => Err(foreign()),
            _ => Err(Error::PreRelease {
                path: path.to_owned(),
            }),
        };
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let found: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let understood = i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX);
    if found > understood {
        return Err(Error::Future { found, understood });
    }
    Ok(())
}

fn screen_in_bounds(cols: i64, rows: i64, replay_len: usize, screen_kept: bool) -> bool {
    (SCREEN_MIN_COLS..=SCREEN_MAX_COLS).contains(&cols)
        && (SCREEN_MIN_ROWS..=SCREEN_MAX_ROWS).contains(&rows)
        && replay_len <= SCREEN_MAX_BYTES
        && (screen_kept || replay_len == 0)
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.exists() {
            refuse_unknown(path)?;
        }
        let mut conn = Connection::open(path)?;
        conn.pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn, MIGRATIONS.iter().copied())?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn put_project(&self, project: &Project) -> Result<()> {
        self.conn.execute(
            "INSERT INTO projects (id, kind, name, path, git_remote_url, sort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, name = excluded.name,
                path = excluded.path, git_remote_url = excluded.git_remote_url,
                sort = excluded.sort",
            params![
                Text(&project.id),
                Text(project.kind),
                project.name,
                Text(&project.path),
                project.git_remote_url,
                project.sort,
            ],
        )?;
        Ok(())
    }

    pub fn project(&self, id: &ProjectId) -> Result<Option<Project>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, kind, name, path, git_remote_url, sort FROM projects WHERE id = ?1",
                [Text(id)],
                project,
            )
            .optional()?)
    }

    pub fn remove_project(
        &mut self,
        id: &ProjectId,
        at: Timestamp,
    ) -> Result<Vec<(Event, ProjectId)>> {
        let tx = self.conn.transaction()?;
        let ids: Vec<ThreadId> = {
            let mut statement = tx.prepare("SELECT id FROM threads WHERE project_id = ?1")?;
            statement
                .query_map([Text(id)], |row| Ok(row.get::<_, Text<_>>(0)?.0))?
                .collect::<Result<_, _>>()?
        };
        let events = append_deleted(&tx, &ids, at)?;
        tx.execute("DELETE FROM projects WHERE id = ?1", [Text(id)])?;
        tx.commit()?;
        Ok(events)
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut statement = self.conn.prepare(
            "SELECT id, kind, name, path, git_remote_url, sort FROM projects
             ORDER BY sort, name, id",
        )?;
        let projects = statement
            .query_map([], project)?
            .collect::<Result<_, _>>()?;
        Ok(projects)
    }

    /// Places `id` before `before`, or at the end of the standard projects when `before` is none.
    /// Personal is refused as the moved project and as `before`.
    pub fn reorder_project(&self, id: &ProjectId, before: Option<&ProjectId>) -> Result<()> {
        let mut projects = self.projects()?;
        let Some(index) = projects.iter().position(|project| &project.id == id) else {
            return Err(not_found("project", id));
        };
        if projects[index].kind == ProjectKind::Personal {
            return Err(Error::Conflict(
                "Personal stays last and cannot be moved".into(),
            ));
        }
        let moved = projects.remove(index);
        let mut standard: Vec<Project> = projects
            .into_iter()
            .filter(|project| project.kind == ProjectKind::Standard)
            .collect();
        let at = match before {
            None => standard.len(),
            Some(before) => {
                let Some(at) = standard.iter().position(|project| &project.id == before) else {
                    return Err(not_found("project", before));
                };
                at
            }
        };
        standard.insert(at, moved);
        for (sort, project) in standard.iter_mut().enumerate() {
            project.sort = i64::try_from(sort).unwrap_or(i64::MAX);
            self.put_project(project)?;
        }
        Ok(())
    }

    pub fn delete_environment(&self, id: &EnvironmentId) -> Result<()> {
        self.conn
            .execute("DELETE FROM environments WHERE id = ?1", [Text(id)])?;
        Ok(())
    }

    pub fn put_environment(&self, environment: &Environment) -> Result<()> {
        self.conn.execute(
            "INSERT INTO environments
                (id, project_id, path, status, provider, branch, base_branch, retire_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id,
                path = excluded.path, status = excluded.status,
                provider = excluded.provider, branch = excluded.branch,
                base_branch = excluded.base_branch, retire_at = excluded.retire_at,
                error = excluded.error",
            params![
                Text(&environment.id),
                Text(&environment.project_id),
                Text(&environment.path),
                Text(environment.status),
                Text(environment.provider),
                environment.branch,
                environment.base_branch,
                environment.retire_at.map(|at| at.0),
                environment.error,
            ],
        )?;
        Ok(())
    }

    pub fn environment(&self, id: &EnvironmentId) -> Result<Option<Environment>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, project_id, path, status, provider, branch, base_branch, retire_at, error
                 FROM environments WHERE id = ?1",
                [Text(id)],
                environment,
            )
            .optional()?)
    }

    pub fn environments(&self, project_id: &ProjectId) -> Result<Vec<Environment>> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, path, status, provider, branch, base_branch, retire_at, error
             FROM environments WHERE project_id = ?1 ORDER BY id",
        )?;
        let environments = statement
            .query_map([Text(project_id)], environment)?
            .collect::<Result<_, _>>()?;
        Ok(environments)
    }

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

    /// Inserts a new thread together with its first events.
    pub fn create_thread(
        &mut self,
        thread: &Thread,
        kinds: impl IntoIterator<Item = EventKind>,
    ) -> Result<Vec<Event>> {
        let tx = self.conn.transaction()?;
        put_thread(&tx, thread)?;
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
                thread,
            )
            .optional()?)
    }

    /// Newest first by `created_at`; `archived` and `before` filter only when given.
    pub fn threads(
        &self,
        project_id: Option<&ProjectId>,
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
                thread,
            )?
            .collect::<Result<_, _>>()?;
        Ok(threads)
    }

    pub fn set_thread_status(
        &self,
        id: &ThreadId,
        status: kg_domain::ThreadStatus,
        at: Timestamp,
    ) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET status = ?2, updated_at = ?3,
                latest_attention_at = CASE
                    WHEN ?2 = 'error' AND status IN ('active', 'starting') THEN ?3
                    ELSE latest_attention_at END
             WHERE id = ?1",
            params![Text(id), Text(status), at.0],
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

    pub fn last_started(&self, id: &ThreadId) -> Result<Option<Timestamp>> {
        Ok(self
            .conn
            .query_row(
                "SELECT at FROM events WHERE thread_id = ?1
                 AND json_extract(kind, '$.type') = 'started' ORDER BY seq DESC LIMIT 1",
                [Text(id)],
                |row| row.get(0).map(Timestamp),
            )
            .optional()?)
    }

    pub fn set_conversation(&self, id: &ThreadId, conversation: Conversation) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE threads SET conversation = ?2 WHERE id = ?1",
            params![Text(id), Text(conversation)],
        )?;
        found(changed, "thread", id)
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

    /// Archives the thread and its lifecycle dependents; returns those that were not archived yet.
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

    /// The Personal folders that deleting `id` would remove: only `id` and its dependents use each,
    /// and no other thread, project or environment sits in or on it.
    pub fn personal_folders(&self, id: &ThreadId) -> Result<Vec<Environment>> {
        personal_folders(&self.conn, id)
    }

    /// Whether `path` lies in a folder that a delete is removing, so nothing new may attach to it.
    pub fn removal_pending(&self, path: &Path) -> Result<bool> {
        Ok(all_environments(&self.conn)?.iter().any(|environment| {
            environment.status == kg_domain::EnvironmentStatus::Destroyed
                && environment.provider == kg_domain::EnvironmentProvider::Personal
                && path.starts_with(&environment.path)
        }))
    }

    /// Deletes the thread, its lifecycle dependents and all their history, and marks `folders` destroyed in the same
    /// commit. `folders` must be what `personal_folders` returned; if it changed, nothing is deleted. Returns a `Deleted` event per thread and that thread's project, read before the row goes.
    pub fn delete_thread(
        &mut self,
        id: &ThreadId,
        at: Timestamp,
        folders: &[EnvironmentId],
    ) -> Result<Vec<(Event, ProjectId)>> {
        let tx = self.conn.transaction()?;
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
        let events = append_deleted(&tx, &owned, at)?;
        tx.execute("DELETE FROM threads WHERE id = ?1", [Text(id)])?;
        for folder in &found {
            tx.execute(
                "UPDATE environments SET status = ?2 WHERE id = ?1",
                params![Text(folder), Text(kg_domain::EnvironmentStatus::Destroyed)],
            )?;
        }
        tx.commit()?;
        Ok(events)
    }

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

    /// Appends in order after the thread's last event, numbering from 1.
    pub fn append_events(
        &mut self,
        thread_id: &ThreadId,
        at: Timestamp,
        kinds: impl IntoIterator<Item = EventKind>,
    ) -> Result<Vec<Event>> {
        let tx = self.conn.transaction()?;
        let events = append_events(&tx, thread_id, at, kinds)?;
        tx.commit()?;
        Ok(events)
    }

    pub fn events_after(
        &self,
        thread_id: &ThreadId,
        after_seq: Option<u64>,
        limit: Option<u32>,
        order: Order,
    ) -> Result<Vec<Event>> {
        let direction = match order {
            Order::Asc => "ASC",
            Order::Desc => "DESC",
        };
        let mut statement = self.conn.prepare(&format!(
            "SELECT thread_id, seq, at, kind FROM current_events
             WHERE thread_id = ?1 AND seq > ?2 ORDER BY seq {direction} LIMIT ?3"
        ))?;
        let events = statement
            .query_map(
                params![
                    Text(thread_id),
                    after_seq.unwrap_or(0),
                    limit.map_or(-1, i64::from)
                ],
                event_from,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

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

    pub fn remove_pending_interaction(&self, id: &PendingInteractionId) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM pending_interactions WHERE id = ?1", [Text(id)])?;
        found(changed, "pending interaction", id)
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
        self.conn.execute(
            "INSERT INTO thread_metadata (thread_id, plugin_id, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(thread_id, plugin_id) DO UPDATE SET value = excluded.value",
            params![Text(id), Text(plugin_id), Json(value)],
        )?;
        Ok(())
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

    pub fn remove_plugin(&self, id: &PluginId) -> Result<()> {
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

    /// Writes the screen only when `started_seq` is still that thread's latest `Started` event.
    /// A delayed snapshot from an older run returns `Ok(false)` and changes nothing.
    pub fn put_screen(&self, row: &ScreenRow) -> Result<bool> {
        let cols = i64::from(row.cols);
        let rows = i64::from(row.rows);
        if !screen_in_bounds(cols, rows, row.replay.len(), row.screen_kept) {
            return Err(Error::Conflict(
                "the screen snapshot is outside the geometry or the 4 MiB bound".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let latest: Option<u64> = transaction
            .query_row(
                "SELECT MAX(seq) FROM events \
                 WHERE thread_id = ?1 AND json_extract(kind, '$.type') = 'started'",
                [Text(&row.thread_id)],
                |row| row.get(0),
            )
            .optional()?;
        if latest != Some(row.started_seq) {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "INSERT INTO terminal_screens \
                (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(thread_id) DO UPDATE SET \
                started_seq = excluded.started_seq, cols = excluded.cols, rows = excluded.rows, \
                replay = excluded.replay, history_dropped = excluded.history_dropped, \
                screen_kept = excluded.screen_kept",
            params![
                Text(&row.thread_id),
                row.started_seq,
                cols,
                rows,
                row.replay,
                row.history_dropped,
                i64::from(u8::from(row.screen_kept)),
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn delete_screen(&self, id: &ThreadId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM terminal_screens WHERE thread_id = ?1",
            [Text(id)],
        )?;
        Ok(())
    }

    /// Screens whose run is still the latest `Started`. A row outside the bound comes back with an
    /// empty replay and `screen_kept` clear, so a reader never builds a terminal from it.
    pub fn screens(&self) -> Result<Vec<ScreenRow>> {
        let mut statement = self.conn.prepare(
            "SELECT thread_id, started_seq, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 \
                          AND ((screen_kept = 0 AND length(replay) = 0) OR screen_kept != 0) \
                     THEN cols ELSE 80 END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 \
                          AND ((screen_kept = 0 AND length(replay) = 0) OR screen_kept != 0) \
                     THEN rows ELSE 24 END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 \
                          AND ((screen_kept = 0 AND length(replay) = 0) OR screen_kept != 0) \
                     THEN replay ELSE X'' END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 AND screen_kept != 0 \
                     THEN history_dropped ELSE 0 END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 AND screen_kept != 0 \
                     THEN 1 ELSE 0 END \
             FROM terminal_screens \
             WHERE started_seq = ( \
                    SELECT MAX(seq) FROM events \
                    WHERE events.thread_id = terminal_screens.thread_id \
                      AND json_extract(kind, '$.type') = 'started')",
        )?;
        let rows = statement.query_map(
            params![
                SCREEN_MIN_COLS,
                SCREEN_MAX_COLS,
                SCREEN_MIN_ROWS,
                SCREEN_MAX_ROWS,
                i64::try_from(SCREEN_MAX_BYTES).unwrap_or(i64::MAX),
            ],
            screen_from,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Error::from)
    }

    /// Newest ending events per thread, four at most, newest first.
    pub fn endings(&self) -> Result<Vec<(ThreadId, Vec<EventKind>)>> {
        let mut statement = self.conn.prepare(
            "SELECT thread_id, kind FROM ( \
                SELECT thread_id, kind, \
                    ROW_NUMBER() OVER (PARTITION BY thread_id ORDER BY seq DESC) AS n \
                FROM events \
                WHERE json_extract(kind, '$.type') IN ('stopped', 'exited', 'failed') \
             ) WHERE n <= 4 ORDER BY thread_id, n",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, Text<_>>(0)?.0,
                row.get::<_, Json<EventKind>>(1)?.0,
            ))
        })?;
        let mut grouped: Vec<(ThreadId, Vec<EventKind>)> = Vec::new();
        for row in rows {
            let (id, kind) = row?;
            if let Some((last, kinds)) = grouped.last_mut()
                && *last == id
            {
                kinds.push(kind);
            } else {
                grouped.push((id, vec![kind]));
            }
        }
        Ok(grouped)
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

fn all_environments(conn: &Connection) -> Result<Vec<Environment>> {
    let mut statement = conn.prepare(
        "SELECT id, project_id, path, status, provider, branch, base_branch, retire_at, error
         FROM environments",
    )?;
    let environments = statement
        .query_map([], environment)?
        .collect::<Result<_, _>>()?;
    Ok(environments)
}

fn personal_folders(conn: &Connection, id: &ThreadId) -> Result<Vec<Environment>> {
    let owned = owned_threads(conn, id)?;
    let environments = all_environments(conn)?;
    let mut statement = conn.prepare("SELECT id, environment_id FROM threads")?;
    let threads: Vec<(ThreadId, Option<EnvironmentId>)> = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Text<_>>(0)?.0,
                row.get::<_, Option<Text<_>>>(1)?.map(|text| text.0),
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut statement = conn.prepare("SELECT path FROM projects")?;
    let projects: Vec<std::path::PathBuf> = statement
        .query_map([], |row| Ok(row.get::<_, String>(0)?.into()))?
        .collect::<Result<_, _>>()?;
    let path_of = |environment: &EnvironmentId| {
        environments
            .iter()
            .find(|candidate| &candidate.id == environment)
            .map(|candidate| candidate.path.clone())
    };
    let mut folders = Vec::new();
    for environment in &environments {
        let doomed = environment.provider == kg_domain::EnvironmentProvider::Personal
            && threads.iter().any(|(thread, used)| {
                owned.contains(thread) && used.as_ref() == Some(&environment.id)
            });
        if !doomed {
            continue;
        }
        let folder = &environment.path;
        let shared = threads.iter().any(|(thread, used)| {
            !owned.contains(thread)
                && used
                    .as_ref()
                    .and_then(path_of)
                    .is_some_and(|path| path.starts_with(folder))
        }) || environments
            .iter()
            .any(|other| other.id != environment.id && other.path.starts_with(folder))
            || projects.iter().any(|project| project.starts_with(folder));
        if !shared {
            folders.push(environment.clone());
        }
    }
    Ok(folders)
}

fn append_deleted(
    tx: &Transaction,
    ids: &[ThreadId],
    at: Timestamp,
) -> Result<Vec<(Event, ProjectId)>> {
    let mut events = Vec::new();
    for id in ids {
        let project_id: ProjectId = tx.query_row(
            "SELECT project_id FROM threads WHERE id = ?1",
            [Text(id)],
            |row| Ok(row.get::<_, Text<_>>(0)?.0),
        )?;
        events.extend(
            append_events(tx, id, at, [EventKind::Deleted])?
                .into_iter()
                .map(|event| (event, project_id.clone())),
        );
    }
    Ok(events)
}

fn owned_threads(conn: &Connection, id: &ThreadId) -> Result<Vec<ThreadId>> {
    let mut statement = conn.prepare(&format!("{OWNED_THREADS} SELECT id FROM owned"))?;
    let owned = statement
        .query_map([Text(id)], |row| Ok(row.get::<_, Text<_>>(0)?.0))?
        .collect::<Result<_, _>>()?;
    Ok(owned)
}

fn found(changed: usize, what: &'static str, id: &impl Display) -> Result<()> {
    if changed == 0 {
        Err(not_found(what, id))
    } else {
        Ok(())
    }
}

fn not_found(what: &'static str, id: &impl Display) -> Error {
    Error::NotFound {
        what,
        id: id.to_string(),
    }
}

fn project(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get::<_, Text<_>>(0)?.0,
        kind: row.get::<_, Text<_>>(1)?.0,
        name: row.get(2)?,
        path: row.get::<_, Text<_>>(3)?.0,
        git_remote_url: row.get(4)?,
        sort: row.get(5)?,
    })
}

fn environment(row: &Row) -> rusqlite::Result<Environment> {
    Ok(Environment {
        id: row.get::<_, Text<_>>(0)?.0,
        project_id: row.get::<_, Text<_>>(1)?.0,
        path: row.get::<_, Text<_>>(2)?.0,
        status: row.get::<_, Text<_>>(3)?.0,
        provider: row.get::<_, Text<_>>(4)?.0,
        branch: row.get(5)?,
        base_branch: row.get(6)?,
        retire_at: row.get::<_, Option<i64>>(7)?.map(Timestamp),
        error: row.get(8)?,
    })
}

fn thread(row: &Row) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: row.get::<_, Text<_>>(0)?.0,
        project_id: row.get::<_, Text<_>>(1)?.0,
        environment_id: row.get::<_, Text<_>>(2)?.0,
        title: row.get(3)?,
        provider: row.get(4)?,
        model: row.get(5)?,
        reasoning_level: row.get(6)?,
        status: row.get::<_, Text<_>>(7)?.0,
        visibility: row.get::<_, Text<_>>(8)?.0,
        parent_thread_id: row.get::<_, Text<_>>(9)?.0,
        lifecycle_owner_thread_id: row.get::<_, Text<_>>(10)?.0,
        section_id: row.get::<_, Text<_>>(11)?.0,
        pinned: row.get(12)?,
        last_read_at: row.get::<_, Option<i64>>(13)?.map(Timestamp),
        latest_attention_at: Timestamp(row.get(14)?),
        mark: if row.get(19)? {
            ThreadMark::Done
        } else {
            ThreadMark::None
        },
        archived_at: row.get::<_, Option<i64>>(15)?.map(Timestamp),
        created_at: Timestamp(row.get(16)?),
        updated_at: Timestamp(row.get(17)?),
        title_locked: row.get(18)?,
    })
}

fn put_thread(conn: &Connection, thread: &Thread) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO threads ({THREAD_COLUMNS})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id,
                environment_id = excluded.environment_id, title = excluded.title,
                title_locked = excluded.title_locked,
                provider = excluded.provider, model = excluded.model,
                reasoning_level = excluded.reasoning_level, status = excluded.status,
                visibility = excluded.visibility, parent_thread_id = excluded.parent_thread_id,
                lifecycle_owner_thread_id = excluded.lifecycle_owner_thread_id,
                section_id = excluded.section_id, pinned = excluded.pinned,
                last_read_at = excluded.last_read_at,
                latest_attention_at = excluded.latest_attention_at,
                archived_at = excluded.archived_at, created_at = excluded.created_at,
                updated_at = excluded.updated_at, done = excluded.done"
        ),
        params![
            Text(&thread.id),
            Text(&thread.project_id),
            Text(&thread.environment_id),
            thread.title,
            thread.provider,
            thread.model,
            thread.reasoning_level,
            Text(thread.status),
            Text(thread.visibility),
            Text(&thread.parent_thread_id),
            Text(&thread.lifecycle_owner_thread_id),
            Text(&thread.section_id),
            thread.pinned,
            thread.last_read_at.map(|at| at.0),
            thread.latest_attention_at.0,
            thread.archived_at.map(|at| at.0),
            thread.created_at.0,
            thread.updated_at.0,
            thread.title_locked,
            thread.mark == ThreadMark::Done,
        ],
    )?;
    Ok(())
}

fn screen_from(row: &Row<'_>) -> rusqlite::Result<ScreenRow> {
    let cols: i64 = row.get(2)?;
    let rows: i64 = row.get(3)?;
    let replay: Vec<u8> = row.get(4)?;
    let kept: i64 = row.get(6)?;
    let screen_kept = kept != 0;
    if !screen_in_bounds(cols, rows, replay.len(), screen_kept) {
        return Err(rusqlite::Error::InvalidColumnType(
            4,
            "replay".into(),
            rusqlite::types::Type::Blob,
        ));
    }
    let cols = u16::try_from(cols).map_err(|_| {
        rusqlite::Error::InvalidColumnType(2, "cols".into(), rusqlite::types::Type::Integer)
    })?;
    let rows = u16::try_from(rows).map_err(|_| {
        rusqlite::Error::InvalidColumnType(3, "rows".into(), rusqlite::types::Type::Integer)
    })?;
    let history_dropped = u32::try_from(row.get::<_, i64>(5)?).unwrap_or(u32::MAX);
    Ok(ScreenRow {
        thread_id: row.get::<_, Text<_>>(0)?.0,
        started_seq: row.get(1)?,
        cols,
        rows,
        replay,
        history_dropped,
        screen_kept,
    })
}

fn event_from(row: &Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        thread_id: row.get::<_, Text<_>>(0)?.0,
        seq: row.get(1)?,
        at: Timestamp(row.get(2)?),
        kind: row.get::<_, Json<_>>(3)?.0,
    })
}

fn append_events(
    conn: &Connection,
    thread_id: &ThreadId,
    at: Timestamp,
    kinds: impl IntoIterator<Item = EventKind>,
) -> Result<Vec<Event>> {
    let last: u64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?1",
        [Text(thread_id)],
        |row| row.get(0),
    )?;
    let mut insert =
        conn.prepare("INSERT INTO events (thread_id, seq, at, kind) VALUES (?1, ?2, ?3, ?4)")?;
    let events = (last + 1..)
        .zip(kinds)
        .map(|(seq, kind)| {
            insert.execute(params![Text(thread_id), seq, at.0, Json(&kind)])?;
            Ok(Event {
                thread_id: thread_id.clone(),
                seq,
                at,
                kind,
            })
        })
        .collect::<Result<_>>()?;
    drop(insert);
    Ok(events)
}
