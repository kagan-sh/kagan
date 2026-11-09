use std::fmt::Display;
use std::io::Read as _;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

mod environment;
mod event;
mod interaction;
mod plugin;
mod project;
mod screen;
mod section;
mod setting;
mod thread;

pub use screen::ScreenRow;

use crate::{Error, Result};

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
    model TEXT NOT NULL DEFAULT '',
    reasoning_level TEXT NOT NULL DEFAULT '',
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
    "ALTER TABLE threads DROP COLUMN model; ALTER TABLE threads DROP COLUMN reasoning_level;",
    r"
ALTER TABLE projects ADD COLUMN sort INTEGER NOT NULL DEFAULT 0;
UPDATE projects SET sort = (
    SELECT COUNT(*) FROM projects AS other
    WHERE other.kind = projects.kind
      AND (other.name < projects.name
        OR (other.name = projects.name AND other.id < projects.id))
);
",
    "ALTER TABLE pending_interactions ADD COLUMN fields TEXT;",
    REPAIR_THREAD_COLUMNS,
    "ALTER TABLE plugins ADD COLUMN skills TEXT NOT NULL DEFAULT '[]';",
];

pub(crate) const REPAIR_THREAD_COLUMNS: &str = "-- repair conflicting thread migrations";

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

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.exists() {
            refuse_unknown(path)?;
        }
        let mut conn = Connection::open(path)?;
        conn.pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        crate::migrate(&mut conn, MIGRATIONS.iter().copied())?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }
}

// An insert of `columns` into `table` with `:column` placeholders that replaces the row with the
// same `id`.
fn upsert(table: &str, columns: &str) -> String {
    let names: Vec<&str> = columns.split(", ").collect();
    let values = names
        .iter()
        .map(|name| format!(":{name}"))
        .collect::<Vec<_>>();
    let updates = names
        .iter()
        .filter(|name| **name != "id")
        .map(|name| format!("{name} = excluded.{name}"))
        .collect::<Vec<_>>();
    format!(
        "INSERT INTO {table} ({columns}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {}",
        values.join(", "),
        updates.join(", ")
    )
}

// The `seq` of the latest `started` event of the thread named by the SQL expression `thread`.
fn latest_start(thread: &str) -> String {
    format!(
        "(SELECT MAX(seq) FROM events AS start WHERE start.thread_id = {thread} \
         AND json_extract(start.kind, '$.type') = 'started')"
    )
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
