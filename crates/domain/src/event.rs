use serde::{Deserialize, Serialize};

use crate::{ThreadId, Timestamp};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub thread_id: ThreadId,
    pub seq: u64,
    pub at: Timestamp,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EventKind {
    Created,
    Started,
    /// The program ended with exit code `code`, or was ended by `signal`.
    Exited {
        code: Option<u32>,
        signal: Option<i32>,
    },
    /// The person stopped the program.
    Stopped,
    Idle,
    /// The title went plain after a spinner, so a reply ended. Not the row becoming idle.
    Waiting,
    Failed {
        error: Option<String>,
    },
    Archived,
    Unarchived,
    Deleted,
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "turn.ended")]
    TurnEnded {
        outcome: TurnOutcome,
        error: Option<String>,
    },
    /// An agent message arrives in chunks that share its `messageId`, when the agent names one.
    Message {
        role: Role,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none", alias = "message_id")]
        message_id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<Attachment>,
    },
    /// One tool call; later events with the same `id` update it. `name` is absent when an
    /// update does not retitle the call.
    Tool {
        #[serde(default)]
        id: String,
        name: Option<String>,
        status: ToolStatus,
    },
    /// kagan handed this tool call to a plugin, or served `skill_load` itself.
    #[serde(rename = "tool.started")]
    ToolStarted {
        id: String,
        name: String,
    },
    /// The tool call's reply landed. `status` is `error` for a refusal, a schema miss, a
    /// plugin error, or a timeout.
    #[serde(rename = "tool.ended")]
    ToolEnded {
        id: String,
        status: ToolOutcome,
    },
    /// Restoring the thread failed. The error stays on the event so an old log still opens.
    #[serde(rename = "restore.failed")]
    RestoreFailed {
        error: String,
    },
    /// A worktree snapshot could not be stored; the turn went on.
    #[serde(rename = "snapshot.failed")]
    SnapshotFailed {
        error: String,
    },
    /// The agent told the person something about the thread, for example a setting it changed.
    Notice {
        severity: NoticeSeverity,
        text: String,
    },
    /// The agent lost its connection to the model and is trying again, for example
    /// "Reconnecting... 2/5".
    Reconnecting {
        text: String,
    },
    /// The person answered a pending interaction. `option` is the choice; `title` is the request.
    /// `fields` holds values from an ask form when the question declared them (decision 144).
    Answered {
        title: String,
        option: String,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        fields: std::collections::BTreeMap<String, serde_json::Value>,
    },
    /// An edit, delete, or move tool named this path.
    #[serde(rename = "file.changed")]
    FileChanged {
        path: String,
    },
    /// The person opened this file's diff.
    #[serde(rename = "file.read")]
    FileRead {
        path: String,
    },
    /// The person wrote this line before that file's diff was shown.
    #[serde(rename = "file.predicted")]
    FilePredicted {
        path: String,
        line: String,
    },
    /// The person switched to this thread.
    Opened,
    Rewound {
        before_seq: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeSeverity {
    Info,
    Warning,
    Error,
}

impl EventKind {
    /// Every `type` tag an event can carry, including kinds kagan no longer writes. An old log
    /// may hold any of them.
    pub const TYPES: [&'static str; 26] = [
        "created",
        "started",
        "exited",
        "stopped",
        "idle",
        "waiting",
        "failed",
        "archived",
        "unarchived",
        "deleted",
        "turn.started",
        "turn.ended",
        "message",
        "tool",
        "tool.started",
        "tool.ended",
        "restore.failed",
        "snapshot.failed",
        "notice",
        "answered",
        "file.changed",
        "file.read",
        "file.predicted",
        "opened",
        "rewound",
        "reconnecting",
    ];

    /// The kinds kagan writes now, and the only names a wait or a subscription accepts. A kind
    /// that is in [`Self::TYPES`] but not here still loads from an old log.
    pub const LIVE_TYPES: [&'static str; 17] = [
        "created",
        "started",
        "exited",
        "stopped",
        "idle",
        "waiting",
        "failed",
        "archived",
        "unarchived",
        "deleted",
        "turn.started",
        "turn.ended",
        "message",
        "notice",
        "answered",
        "tool.started",
        "tool.ended",
    ];

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Started => "started",
            Self::Exited { .. } => "exited",
            Self::Stopped => "stopped",
            Self::Idle => "idle",
            Self::Waiting => "waiting",
            Self::Failed { .. } => "failed",
            Self::Archived => "archived",
            Self::Unarchived => "unarchived",
            Self::Deleted => "deleted",
            Self::TurnStarted => "turn.started",
            Self::TurnEnded { .. } => "turn.ended",
            Self::Message { .. } => "message",
            Self::Tool { .. } => "tool",
            Self::ToolStarted { .. } => "tool.started",
            Self::ToolEnded { .. } => "tool.ended",
            Self::RestoreFailed { .. } => "restore.failed",
            Self::SnapshotFailed { .. } => "snapshot.failed",
            Self::Notice { .. } => "notice",
            Self::Answered { .. } => "answered",
            Self::FileChanged { .. } => "file.changed",
            Self::FileRead { .. } => "file.read",
            Self::FilePredicted { .. } => "file.predicted",
            Self::Opened => "opened",
            Self::Rewound { .. } => "rewound",
            Self::Reconnecting { .. } => "reconnecting",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Agent,
}

/// A file or image sent with a user message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Attachment {
    Image {
        name: String,
        #[serde(alias = "mime_type")]
        mime_type: String,
        /// Base64, without a `data:` prefix.
        data: String,
    },
    File {
        name: String,
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none", alias = "mime_type")]
        mime_type: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// How a tool call kagan served ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOutcome {
    Ok,
    Error,
}
