use clap::{
    Subcommand,
    builder::{StringValueParser, TypedValueParser},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::json;

#[derive(Clone, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigOp {
    /// Read a setting
    Get { key: String },
    /// Change a setting
    Set {
        key: String,
        /// Omit it to type it at a prompt that does not show it, or to pipe it on stdin. Do that for a secret, so it stays out of your shell history
        #[arg(allow_negative_numbers = true, value_parser = StringValueParser::new().map(json))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
    },
    /// Forget a secret, or clear a project setting
    Clear { key: String },
}

// A value may be a secret, so a debug print never shows it.
impl std::fmt::Debug for ConfigOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get { key } => f.debug_struct("Get").field("key", key).finish(),
            Self::Set { key, value } => f
                .debug_struct("Set")
                .field("key", key)
                .field("value", &value.as_ref().map(|_| "<hidden>"))
                .finish(),
            Self::Clear { key } => f.debug_struct("Clear").field("key", key).finish(),
        }
    }
}
