use std::fmt;

use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::{ProjectId, ThreadId};

/// The longest value kept, in bytes. A value is one line, as a plugin secret is.
pub(crate) const ENV_VALUE_MAX_BYTES: usize = 1024;

#[derive(Clone, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum EnvOp {
    /// Names of the variables every program gets, or of a project's with --project. Values are never shown
    List {
        #[arg(long = "project")]
        project_id: Option<ProjectId>,
    },
    /// Set a variable. The value comes from stdin, or from a prompt that does not show it
    Set {
        name: String,
        #[arg(long = "project")]
        project_id: Option<ProjectId>,
        #[arg(skip)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        /// Set by a thread's own environment; a thread may not change variables
        #[arg(long, env = "KG_THREAD_ID", hide = true)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<ThreadId>,
    },
    /// Remove a variable
    Remove {
        name: String,
        #[arg(long = "project")]
        project_id: Option<ProjectId>,
        #[arg(long, env = "KG_THREAD_ID", hide = true)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<ThreadId>,
    },
}

// A value may be a secret, so a debug print never shows it.
impl fmt::Debug for EnvOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::List { project_id } => f
                .debug_struct("List")
                .field("project_id", project_id)
                .finish(),
            Self::Set {
                name,
                project_id,
                value,
                ..
            } => f
                .debug_struct("Set")
                .field("name", name)
                .field("project_id", project_id)
                .field("value", &value.as_ref().map(|_| "<hidden>"))
                .finish(),
            Self::Remove {
                name, project_id, ..
            } => f
                .debug_struct("Remove")
                .field("name", name)
                .field("project_id", project_id)
                .finish(),
        }
    }
}

/// A name the shell accepts: letters, digits and `_`, not starting with a digit.
///
/// # Errors
///
/// Returns the reason when the name is not one.
pub(crate) fn shell_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let starts = matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_');
    if starts && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Ok(())
    } else {
        Err("a variable name is letters, digits and _, and does not start with a digit".into())
    }
}

/// A name a person may set: a shell name that kagan does not set itself. kagan sets `KG_*` and
/// `PATH`, on any system, so a case that differs does not get past the check.
///
/// # Errors
///
/// Returns the reason when the name is not a shell name, or kagan sets it.
pub(crate) fn variable_name(name: &str) -> Result<(), String> {
    shell_name(name)?;
    let upper = name.to_ascii_uppercase();
    if upper.starts_with("KG_") || upper == "PATH" {
        return Err("kagan sets KG_* names and PATH, so those cannot be set".into());
    }
    Ok(())
}

/// A value kept for a variable: not empty, one line, and at most [`ENV_VALUE_MAX_BYTES`].
///
/// # Errors
///
/// Returns the reason when the value is not one of those. The reason never repeats the value.
pub(crate) fn variable_value(value: &str) -> Result<(), String> {
    if value.is_empty() {
        Err("the value is empty".into())
    } else if value.len() > ENV_VALUE_MAX_BYTES {
        Err(format!("the value is over {ENV_VALUE_MAX_BYTES} bytes"))
    } else if value.contains(['\n', '\r', '\0']) {
        Err("the value must be one line".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "env_test.rs"]
mod tests;
