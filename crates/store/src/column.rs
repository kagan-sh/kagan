use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

pub(crate) struct Text<T>(pub T);

impl<T: Serialize> ToSql for Text<T> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match serde_json::to_value(&self.0) {
            Ok(Value::String(text)) => Ok(ToSqlOutput::from(text)),
            Ok(Value::Null) => Ok(ToSqlOutput::Borrowed(ValueRef::Null)),
            Ok(other) => Err(rusqlite::Error::ToSqlConversionFailure(
                format!("not a text column value: {other}").into(),
            )),
            Err(error) => Err(rusqlite::Error::ToSqlConversionFailure(error.into())),
        }
    }
}

impl<T: DeserializeOwned> FromSql for Text<T> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let value = match value {
            ValueRef::Null => Value::Null,
            other => Value::String(other.as_str()?.to_owned()),
        };
        serde_json::from_value(value)
            .map(Text)
            .map_err(FromSqlError::other)
    }
}

pub(crate) struct Json<T>(pub T);

impl<T: Serialize> ToSql for Json<T> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        serde_json::to_string(&self.0)
            .map(ToSqlOutput::from)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))
    }
}

impl<T: DeserializeOwned> FromSql for Json<T> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        serde_json::from_str(value.as_str()?)
            .map(Json)
            .map_err(FromSqlError::other)
    }
}
