use clap::Subcommand;
use serde::{Deserialize, Serialize};

use super::TEXT_MAX;
use crate::{AutomationId, ProjectId, ThreadId, Timestamp};

/// A named text, a program line, and a time, that starts a thread in one project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: AutomationId,
    pub project_id: ProjectId,
    pub name: String,
    pub body: String,
    pub program: String,
    /// Whole minutes between runs, from 1 to 1440. Empty when `at` is set.
    pub minutes: Option<u16>,
    /// When to run once, as milliseconds since the Unix epoch. Empty when `minutes` is set.
    pub at: Option<Timestamp>,
    pub paused: bool,
    /// When the next run is due. Empty after a one-time automation has started a thread.
    pub next_at: Option<Timestamp>,
    /// The thread each run started, oldest first.
    pub threads: Vec<ThreadId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum AutomationOp {
    /// Save an automation on a project
    Add {
        name: String,
        /// The text to send once the program is running
        body: String,
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: ProjectId,
        /// The program line, such as `claude` or `codex`
        #[arg(long)]
        program: String,
        /// Whole minutes between runs, from 1 to 1440
        #[arg(long, conflicts_with = "at")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minutes: Option<u16>,
        /// When to run once, as milliseconds since the Unix epoch
        #[arg(long, conflicts_with = "minutes")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Timestamp>,
    },
    /// Pause an automation. A thread it already started keeps running
    Pause { id: AutomationId },
    /// Let a paused automation start threads again
    Resume { id: AutomationId },
    /// Remove an automation. A thread it already started keeps running
    Delete { id: AutomationId },
    /// List automations, and the thread each run started
    List {
        #[arg(long, env = "KG_PROJECT_ID")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<ProjectId>,
    },
}

pub(super) fn valid(op: &AutomationOp) -> Result<(), String> {
    let AutomationOp::Add {
        name,
        body,
        program,
        minutes,
        at,
        ..
    } = op
    else {
        return Ok(());
    };
    let name = name.trim();
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err("an automation needs a name".into());
    }
    if body.is_empty() {
        return Err("an automation needs text".into());
    }
    if body.len() > TEXT_MAX {
        return Err(format!("text is over {} MiB", TEXT_MAX >> 20));
    }
    crate::split_program(program.trim()).map_err(|error| error.to_string())?;
    match (*minutes, *at) {
        (Some(minutes), None) if (1..=1440).contains(&minutes) => Ok(()),
        (Some(_), None) => Err("minutes must be a whole number from 1 to 1440".into()),
        (None, Some(at)) if at.0 >= 0 => Ok(()),
        (None, Some(_)) => Err("a time cannot be negative".into()),
        (None, None) => Err("an automation needs minutes from 1 to 1440, or one time".into()),
        (Some(_), Some(_)) => Err("an automation takes minutes or one time".into()),
    }
}
