use clap::{
    Subcommand,
    builder::{StringValueParser, TypedValueParser},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::json;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum PreferenceOp {
    /// Read a preference
    Get { key: String },
    /// Save a preference
    Set {
        key: String,
        #[arg(value_parser = StringValueParser::new().map(json))]
        value: Value,
    },
}
