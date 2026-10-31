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
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
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
    /// An agent message arrives in chunks that share its `message_id`, when the agent names one.
    Message {
        role: Role,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
    Answered {
        title: String,
        option: String,
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
        #[serde(rename = "beforeSeq")]
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
    /// Every `type` tag, the names `thread.<type>` subscriptions and waits use.
    pub const TYPES: [&'static str; 24] = [
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
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Attachment {
    Image {
        name: String,
        mime_type: String,
        /// Base64, without a `data:` prefix.
        data: String,
    },
    File {
        name: String,
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
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
