use std::path::Path;

use rusqlite::{
    Connection, params_from_iter,
    types::{Value as Sql, ValueRef},
};
use serde_json::{Map, Number, Value};

use crate::{Result, migrate};

pub struct PluginDb {
    conn: Connection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Changes {
    pub changes: u64,
    pub last_id: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Statement {
    pub sql: String,
    pub params: Vec<Value>,
}

impl PluginDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(Self { conn })
    }

    /// Applies each statement the file has not seen yet, in order, all in one transaction.
    pub fn migrate(&mut self, statements: &[String]) -> Result<()> {
        migrate(&mut self.conn, statements.iter().map(String::as_str))
    }

    /// Rows as objects keyed by column name.
    pub fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Map<String, Value>>> {
        let mut statement = self.conn.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let rows = statement
            .query_map(params_from_iter(params.iter().map(sql_value)), |row| {
                names
                    .iter()
                    .enumerate()
                    .map(|(index, name)| Ok((name.clone(), json_value(row.get_ref(index)?))))
                    .collect()
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn run(&self, sql: &str, params: &[Value]) -> Result<Changes> {
        run(&self.conn, sql, params)
    }

    /// All or nothing.
    pub fn transaction(&mut self, statements: &[Statement]) -> Result<Vec<Changes>> {
        let tx = self.conn.transaction()?;
        let changes = statements
            .iter()
            .map(|statement| run(&tx, &statement.sql, &statement.params))
            .collect::<Result<_>>()?;
        tx.commit()?;
        Ok(changes)
    }
}

fn run(conn: &Connection, sql: &str, params: &[Value]) -> Result<Changes> {
    let changes = conn.execute(sql, params_from_iter(params.iter().map(sql_value)))?;
    Ok(Changes {
        changes: changes as u64,
        last_id: conn.last_insert_rowid(),
    })
}

fn sql_value(value: &Value) -> Sql {
    match value {
        Value::Null => Sql::Null,
        Value::Bool(flag) => Sql::Integer(i64::from(*flag)),
        Value::Number(number) => number
            .as_i64()
            .map(Sql::Integer)
            .or_else(|| number.as_f64().map(Sql::Real))
            .unwrap_or(Sql::Null),
        Value::String(text) => Sql::Text(text.clone()),
        Value::Array(_) | Value::Object(_) => Sql::Text(value.to_string()),
    }
}

fn json_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => Value::from(number),
        ValueRef::Real(number) => Number::from_f64(number).map_or(Value::Null, Value::Number),
        ValueRef::Text(text) => Value::String(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(bytes) => Value::from(bytes),
    }
}
