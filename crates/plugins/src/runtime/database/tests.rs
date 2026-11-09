use rusqlite::{Error, ErrorCode, Statement};
use serde_json::Value;

use super::{in_transaction, open};

const CONTROL: [&str; 8] = [
    "BEGIN",
    "BEGIN IMMEDIATE",
    "COMMIT",
    "END",
    "ROLLBACK",
    "SAVEPOINT s",
    "RELEASE s",
    "ROLLBACK TO s",
];

fn denied(prepared: &rusqlite::Result<Statement<'_>>) -> bool {
    matches!(prepared, Err(Error::SqliteFailure(error, _))
        if error.code == ErrorCode::AuthorizationForStatementDenied)
}

#[test]
fn plugin_statements_cannot_begin_or_end_a_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut connection = open(&dir.path().join("database.sqlite3")).unwrap();
    for sql in CONTROL {
        assert!(
            denied(&connection.prepare(sql)),
            "outside a transaction: {sql}"
        );
    }
    for sql in CONTROL {
        let mut refused = false;
        in_transaction(&mut connection, |inside| {
            refused = denied(&inside.prepare(sql));
            Ok(Value::Null)
        })
        .unwrap();
        assert!(refused, "inside a transaction: {sql}");
    }
}
