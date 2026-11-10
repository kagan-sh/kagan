use kg_domain::ProjectId;

use super::{id, store, thread};
use crate::{Error, Store};

#[test]
fn conflicting_schema_five_keeps_threads_and_accepts_new_ones() {
    let path =
        std::env::temp_dir().join(format!("kg-store-conflict-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    let migrations = &crate::store::MIGRATIONS;
    let initial = migrations[0]
        .replace("model TEXT NOT NULL DEFAULT ''", "model TEXT NOT NULL")
        .replace(
            "reasoning_level TEXT NOT NULL DEFAULT ''",
            "reasoning_level TEXT NOT NULL",
        );
    crate::migrate(
        &mut conn,
        [
            initial.as_str(),
            migrations[1],
            migrations[3],
            migrations[4],
        ],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 5).unwrap();
    conn.execute("INSERT INTO projects (id, kind, name, path) VALUES ('prj_1', 'standard', 'kagan', '/code/kagan')", []).unwrap();
    conn.execute("INSERT INTO threads (id, project_id, title, provider, model, reasoning_level, status, visibility, pinned, latest_attention_at, created_at, updated_at) VALUES ('thr_1', 'prj_1', 'existing', 'codex', '', '', 'idle', 'visible', 0, 1, 1, 1)", []).unwrap();
    drop(conn);

    let store = Store::open(&path).unwrap();
    assert!(store.thread(&id("thr_1")).unwrap().is_some());
    store.put_thread(&thread("thr_2", 2)).unwrap();
    assert!(store.thread(&id("thr_2")).unwrap().is_some());
    drop(store);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn conflicting_earlier_schemas_upgrade_without_repeating_columns() {
    let migrations = &crate::store::MIGRATIONS;
    for (name, earlier) in [
        ("sort", vec![migrations[0], migrations[1], migrations[3]]),
        ("fields", vec![migrations[0], migrations[1], migrations[4]]),
        (
            "both",
            vec![migrations[0], migrations[1], migrations[3], migrations[4]],
        ),
    ] {
        let path = std::env::temp_dir().join(format!(
            "kg-store-conflict-{name}-{}.sqlite3",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut conn = rusqlite::Connection::open(&path).unwrap();
        crate::migrate(&mut conn, earlier).unwrap();
        drop(conn);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            i64::try_from(migrations.len()).unwrap()
        );
        store.conn.execute("INSERT INTO projects (id, kind, name, path) VALUES ('prj_1', 'standard', 'kagan', '/code/kagan')", []).unwrap();
        store.put_thread(&thread("thr_1", 1)).unwrap();
        drop(store);
        std::fs::remove_file(&path).unwrap();
    }
}

#[test]
fn a_schema_one_database_gains_the_conversation_and_keeps_its_threads() {
    let path = std::env::temp_dir().join(format!("kg-store-v1-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    crate::migrate(&mut conn, crate::store::MIGRATIONS[..1].iter().copied()).unwrap();
    conn.execute(
        "INSERT INTO projects (id, kind, name, path, git_remote_url) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            "prj_1",
            "standard",
            "kagan",
            "/code/kagan",
            Option::<String>::None
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO threads (id, project_id, title, provider, status, visibility, pinned, latest_attention_at, created_at, updated_at) VALUES ('thr_1', 'prj_1', 't', 'claude-code', 'idle', 'visible', 0, 1, 1, 1)",
        [],
    )
    .unwrap();
    let store = Store { conn };
    assert_eq!(store.schema_version().unwrap(), 1);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.schema_version().unwrap(),
        i64::try_from(crate::store::MIGRATIONS.len()).unwrap()
    );
    assert!(store.thread(&id("thr_1")).unwrap().is_some());
    assert_eq!(store.conversation(&id("thr_1")).unwrap(), None);
    assert_eq!(
        store
            .project(&ProjectId("prj_1".into()))
            .unwrap()
            .unwrap()
            .sort,
        0
    );
    drop(store);
    std::fs::remove_file(&path).unwrap();
}
#[test]
fn a_future_schema_is_refused_before_anything_is_written() {
    let dir = std::env::temp_dir().join(format!("kg-store-future-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("kagan.sqlite3");
    let side = dir.join("notes.txt");
    std::fs::write(&side, "keep").unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE canary (id INTEGER); INSERT INTO canary VALUES (1);")
        .unwrap();
    conn.pragma_update(None, "application_id", crate::store::APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 999i64).unwrap();
    drop(conn);
    let Err(error) = Store::open(&path) else {
        panic!("a newer schema was opened");
    };
    let Error::Future { found, understood } = error else {
        panic!("{error}");
    };
    assert_eq!(found, 999);
    assert!(understood < found);
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 999);
    let journal: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "delete");
    let canary: i64 = conn
        .query_row("SELECT id FROM canary", [], |row| row.get(0))
        .unwrap();
    assert_eq!(canary, 1);
    assert_eq!(std::fs::read_to_string(&side).unwrap(), "keep");
    assert!(!dir.join("kagan.sqlite3-wal").exists());
    drop(conn);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_new_database_is_current_schema_and_marked_as_kagan() {
    let store = store();
    assert_eq!(
        store.schema_version().unwrap(),
        i64::try_from(crate::store::MIGRATIONS.len()).unwrap()
    );
    let application: i32 = store
        .conn
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .unwrap();
    assert_eq!(application, crate::store::APPLICATION_ID);
}

#[test]
fn a_pre_release_database_is_refused_with_its_path_and_left_alone() {
    let dir = std::env::temp_dir().join(format!("kg-store-pre-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for version in [1i64, 14] {
        let path = dir.join(format!("v{version}.sqlite3"));
        let conn = rusqlite::Connection::open(&path).unwrap();
        if version == 14 {
            conn.pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()))
                .unwrap();
        }
        conn.execute_batch("CREATE TABLE threads (id TEXT); INSERT INTO threads VALUES ('thr_1');")
            .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        let Err(error) = Store::open(&path) else {
            panic!("a pre-release schema {version} was opened");
        };
        assert!(matches!(error, Error::PreRelease { .. }), "{error}");
        assert_eq!(
            error.to_string(),
            format!(
                "{} was made by a pre-release build of kagan and cannot be opened. Nothing was changed. Move it, and any -wal or -shm file beside it, out of that folder, then start kagan again for a new database",
                path.display()
            )
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn another_file_is_refused_unchanged_and_an_empty_one_is_built() {
    let dir = std::env::temp_dir().join(format!("kg-store-foreign-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let other = dir.join("other.sqlite3");
    let conn = rusqlite::Connection::open(&other).unwrap();
    conn.execute_batch("CREATE TABLE projects (id TEXT); INSERT INTO projects VALUES ('mine');")
        .unwrap();
    drop(conn);
    let text = dir.join("notes.sqlite3");
    std::fs::write(&text, "not a database").unwrap();
    for path in [&other, &text] {
        let before = std::fs::read(path).unwrap();
        let Err(error) = Store::open(path) else {
            panic!("{} was opened", path.display());
        };
        assert_eq!(
            error.to_string(),
            format!(
                "{} is not a kagan database, so kagan did not open it. Nothing was changed. Move it out of that folder, or set KG_DATA_DIR to a different folder",
                path.display()
            )
        );
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    let empty = dir.join("empty.sqlite3");
    std::fs::write(&empty, "").unwrap();
    assert_eq!(
        Store::open(&empty).unwrap().schema_version().unwrap(),
        i64::try_from(crate::store::MIGRATIONS.len()).unwrap()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_migration_leaves_the_previous_schema() {
    let path =
        std::env::temp_dir().join(format!("kg-store-migrate-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    let error = crate::migrate(&mut conn, ["CREATE TABLE keep_me (id INTEGER);", "NOT SQL"]);
    assert!(error.is_err());
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'keep_me'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    drop(conn);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn an_old_notify_switch_becomes_a_level_and_threads_default() {
    fn migrated(stored: Option<&str>) -> (Option<String>, String) {
        use rusqlite::OptionalExtension as _;
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        let before = crate::store::MIGRATIONS
            .iter()
            .position(|sql| sql.contains("ADD COLUMN notify"))
            .unwrap();
        crate::migrate(
            &mut conn,
            crate::store::MIGRATIONS[..before].iter().copied(),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, kind, name, path, git_remote_url) VALUES ('prj_1', 'standard', 'kagan', '/code/kagan', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, project_id, title, provider, status, visibility, pinned, latest_attention_at, created_at, updated_at) VALUES ('thr_1', 'prj_1', 't', 'claude-code', 'idle', 'visible', 0, 1, 1, 1)",
            [],
        )
        .unwrap();
        if let Some(stored) = stored {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('notify', ?1)",
                [stored],
            )
            .unwrap();
        }
        crate::migrate(&mut conn, crate::store::MIGRATIONS.iter().copied()).unwrap();
        let value = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'notify'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        let level = conn
            .query_row("SELECT notify FROM threads WHERE id = 'thr_1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        (value, level)
    }
    assert_eq!(
        migrated(Some("false")),
        (Some("\"off\"".to_owned()), "default".to_owned())
    );
    assert_eq!(
        migrated(Some("true")),
        (Some("\"all\"".to_owned()), "default".to_owned())
    );
    assert_eq!(migrated(None), (None, "default".to_owned()));
}
