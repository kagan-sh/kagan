use std::path::Path;
use std::time::Duration;

use gpui_shell::{HostError, HostModule};
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, TransactionBehavior, params_from_iter};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::area::to_host;
use crate::area::{argument, error};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    sql: String,
    #[serde(default)]
    params: Vec<Value>,
}

enum Request {
    Migrate(Vec<String>),
    Query(Statement),
    Run(Statement),
    Transaction(Vec<Statement>),
}

pub(crate) fn module(path: &Path) -> HostModule {
    let mut module = HostModule::new("kagan/database").declarations(
        "export type Parameter = null | boolean | number | string | number[];\n\
         export interface Statement { sql: string; params?: Parameter[] }\n\
         export function migrate(statements: string[]): Promise<void>;\n\
         export function query(sql: string, params: Parameter[]): Promise<Record<string, unknown>[]>;\n\
         export function run(sql: string, params: Parameter[]): Promise<{ changes: number; lastId: number }>;\n\
         export function transaction(statements: Statement[]): Promise<void>;",
    );
    for name in ["migrate", "query", "run", "transaction"] {
        let path = path.to_owned();
        module = module.async_function(name, move |args| {
            let request = match name {
                "migrate" => Request::Migrate(argument(args, 0)?),
                "transaction" => Request::Transaction(argument(args, 0)?),
                "query" | "run" => {
                    let statement = Statement {
                        sql: args.string(0)?.into(),
                        params: argument(args, 1)?,
                    };
                    if name == "query" {
                        Request::Query(statement)
                    } else {
                        Request::Run(statement)
                    }
                }
                _ => unreachable!("only database functions are registered"),
            };
            let path = path.clone();
            Ok(async move { execute(&path, request).map(to_host) })
        });
    }
    module
}

fn open(path: &Path) -> Result<Connection, HostError> {
    let connection = Connection::open(path).map_err(error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(error)?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(error)?;
    authorize(&connection, false)?;
    Ok(connection)
}

fn authorize(connection: &Connection, transactions: bool) -> Result<(), HostError> {
    connection
        .authorizer(Some(move |context: AuthContext<'_>| match context.action {
            // A parameterized ATTACH is Unknown in rusqlite because its filename is absent.
            AuthAction::Unknown { .. } | AuthAction::Attach { .. } | AuthAction::Detach { .. } => {
                Authorization::Deny
            }
            AuthAction::Transaction { .. } | AuthAction::Savepoint { .. } if !transactions => {
                Authorization::Deny
            }
            AuthAction::Pragma {
                pragma_name,
                pragma_value: Some(_),
            } if !matches!(pragma_name, "user_version") => Authorization::Deny,
            _ => Authorization::Allow,
        }))
        .map_err(error)
}

fn execute(path: &Path, request: Request) -> Result<Value, HostError> {
    let mut connection = open(path)?;
    match request {
        Request::Migrate(statements) => {
            authorize(&connection, true)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(error)?;
            authorize(&transaction, false)?;
            transaction.execute_batch("CREATE TABLE IF NOT EXISTS _kagan_migrations (position INTEGER PRIMARY KEY, sql TEXT NOT NULL)").map_err(error)?;
            let applied = {
                let mut query = transaction
                    .prepare("SELECT sql FROM _kagan_migrations ORDER BY position")
                    .map_err(error)?;
                query
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(error)?
            };
            if !statements.starts_with(&applied) {
                return Err(HostError::new(
                    "applied migrations cannot be changed or removed",
                ));
            }
            for (ix, sql) in statements.iter().enumerate().skip(applied.len()) {
                transaction.execute_batch(sql).map_err(error)?;
                transaction
                    .execute(
                        "INSERT INTO _kagan_migrations (position, sql) VALUES (?1, ?2)",
                        (ix, sql),
                    )
                    .map_err(error)?;
            }
            authorize(&transaction, true)?;
            transaction.commit().map_err(error)?;
            Ok(Value::Null)
        }
        Request::Query(statement) => {
            let params = parameters(statement.params)?;
            let mut query = connection.prepare(&statement.sql).map_err(error)?;
            if !query.readonly() {
                return Err(HostError::new("query requires a read-only statement"));
            }
            let columns = query
                .column_names()
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut rows = query.query(params_from_iter(params)).map_err(error)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().map_err(error)? {
                let mut fields = Map::new();
                for (ix, name) in columns.iter().enumerate() {
                    let value = match row.get_ref(ix).map_err(error)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(number) => json!(number),
                        ValueRef::Real(number) => json!(number),
                        ValueRef::Text(text) => {
                            Value::String(std::str::from_utf8(text).map_err(error)?.into())
                        }
                        ValueRef::Blob(bytes) => json!(bytes),
                    };
                    fields.insert(name.clone(), value);
                }
                result.push(Value::Object(fields));
            }
            Ok(Value::Array(result))
        }
        Request::Run(statement) => {
            let changes = run(&connection, statement)?;
            Ok(json!({ "changes": changes, "lastId": connection.last_insert_rowid() }))
        }
        Request::Transaction(statements) => {
            authorize(&connection, true)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(error)?;
            authorize(&transaction, false)?;
            for statement in statements {
                run(&transaction, statement)?;
            }
            authorize(&transaction, true)?;
            transaction.commit().map_err(error)?;
            Ok(Value::Null)
        }
    }
}

fn run(connection: &Connection, statement: Statement) -> Result<usize, HostError> {
    connection
        .execute(
            &statement.sql,
            params_from_iter(parameters(statement.params)?),
        )
        .map_err(error)
}

fn parameters(values: Vec<Value>) -> Result<Vec<SqlValue>, HostError> {
    values
        .into_iter()
        .map(|value| match value {
            Value::Null => Ok(SqlValue::Null),
            Value::Bool(value) => Ok(SqlValue::Integer(i64::from(value))),
            Value::Number(value) => value
                .as_i64()
                .map(SqlValue::Integer)
                .or_else(|| value.as_f64().map(SqlValue::Real))
                .ok_or_else(|| HostError::new("invalid SQL number")),
            Value::String(value) => Ok(SqlValue::Text(value)),
            Value::Array(bytes) => bytes
                .into_iter()
                .map(|byte| {
                    byte.as_u64()
                        .and_then(|byte| u8::try_from(byte).ok())
                        .ok_or_else(|| HostError::new("SQL blob must contain bytes"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(SqlValue::Blob),
            Value::Object(_) => Err(HostError::new(
                "SQL parameters must be null, boolean, number, text or bytes",
            )),
        })
        .collect()
}
