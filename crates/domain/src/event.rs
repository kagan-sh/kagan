use serde::{Deserialize, Serialize};

use crate::{PendingInteractionId, ThreadId, Timestamp};

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
    /// One thought chunk; chunks with the same `messageId` belong to one thought.
    Thought {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    /// The agent's whole plan; a later plan replaces it.
    Plan {
        entries: Vec<PlanEntry>,
    },
    /// Tokens in context, for the header line. Not a row.
    Usage {
        used: u64,
        size: u64,
    },
    /// Every mode and option the agent offers, whole; a later event replaces it. `modes` in the
    /// session answer is the option `mode`, so one event holds what the chip and its menu draw.
    Options {
        #[serde(default)]
        options: Vec<ThreadOption>,
    },
    /// One tool call; later events with the same `id` update it. Each event carries only what the
    /// agent said this time, so a reader folds the events for one id into one row.
    Tool(ToolCall),
    /// kagan handed this tool call to a plugin, or served `skill_load` itself.
    #[serde(rename = "tool.started")]
    ToolStarted {
        id: String,
        name: String,
    },
    /// The tool call ended, whether or not a reply came back. `status` is `error` for a
    /// refusal, a schema miss, a plugin error, or a timeout.
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    #[serde(default)]
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolStatus>,
    /// ACP raw input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// ACP content text and raw output, flattened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<ToolDiff>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<ToolLocation>,
    /// Set only by a permission request, which links the row to its question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<PendingInteractionId>,
}

impl ToolCall {
    pub fn merge(&mut self, delta: Self) {
        if delta.name.is_some() {
            self.name = delta.name;
        }
        if delta.kind.is_some() {
            self.kind = delta.kind;
        }
        if delta.title.is_some() {
            self.title = delta.title;
        }
        if delta.status.is_some() {
            self.status = delta.status;
        }
        if delta.input.is_some() {
            self.input = delta.input;
        }
        if delta.output.is_some() {
            self.output = delta.output;
        }
        if !delta.diff.is_empty() {
            self.diff = delta.diff;
        }
        if !delta.locations.is_empty() {
            self.locations = delta.locations;
        }
        if delta.approval.is_some() {
            self.approval = delta.approval;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeSeverity {
    Info,
    Warning,
    Error,
}

impl EventKind {
    /// The kinds kagan writes now, and the only names a wait or a subscription accepts. Other
    /// kinds still load from an old log.
    pub const LIVE_TYPES: [&'static str; 22] = [
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
        "thought",
        "plan",
        "usage",
        "options",
        "notice",
        "answered",
        "tool",
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
            Self::Thought { .. } => "thought",
            Self::Plan { .. } => "plan",
            Self::Usage { .. } => "usage",
            Self::Options { .. } => "options",
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

/// What a tool call does, as the agent's ACP kind names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    Other,
}

/// What a tool call is about, once kagan reads the agent's name, title and input. Presentation
/// only: nothing here decides whether a row draws. The window's rows and a plugin's page read the
/// same rule, so the two never disagree about one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkKind {
    Read,
    Search,
    Execute,
    Edit,
    Delete,
    Move,
    Fetch,
    Think,
    Mcp { server: String, tool: String },
    Skill { name: String },
    Other,
}

impl WorkKind {
    /// Reads one call's names and input. The agent's own kind decides last, so a call the agent
    /// calls a read still reads as the MCP call or the skill load it names.
    #[must_use]
    pub fn of(
        name: Option<&str>,
        title: Option<&str>,
        kind: Option<ToolKind>,
        input: Option<&serde_json::Value>,
    ) -> Self {
        if let Some(rest) = name.and_then(|name| name.strip_prefix("mcp__"))
            && let Some((server, called)) = rest.split_once("__")
        {
            return Self::Mcp {
                server: server.to_owned(),
                tool: called.to_owned(),
            };
        }
        // kagan writes its own MCP server as `kagan_<tool>`; any other agent's server is only
        // recognisable when it spells the name `mcp__<server>__<tool>`.
        if let Some((server, called)) = name.and_then(|name| name.split_once('_'))
            && server == "kagan"
        {
            return Self::Mcp {
                server: server.to_owned(),
                tool: called.to_owned(),
            };
        }
        let named = input
            .and_then(|input| input.get("name"))
            .and_then(serde_json::Value::as_str);
        if name == Some(SKILL_LOAD)
            || (title.is_some_and(|title| title.starts_with(LOAD_SKILL)) && named.is_some())
        {
            let name = named
                .or_else(|| title.and_then(|title| title.strip_prefix(LOAD_SKILL)))
                .map_or_else(|| A_SKILL, str::trim_start);
            return Self::Skill {
                name: name.to_owned(),
            };
        }
        match kind {
            Some(ToolKind::Read) => Self::Read,
            Some(ToolKind::Edit) => Self::Edit,
            Some(ToolKind::Delete) => Self::Delete,
            Some(ToolKind::Move) => Self::Move,
            Some(ToolKind::Search) => Self::Search,
            Some(ToolKind::Execute) => Self::Execute,
            Some(ToolKind::Think) => Self::Think,
            Some(ToolKind::Fetch) => Self::Fetch,
            _ => Self::Other,
        }
    }

    /// The one word a plugin's page reads for the call: the `tool` field of a `work` item.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Search => "search",
            Self::Execute => "execute",
            Self::Edit => "edit",
            Self::Delete => "delete",
            Self::Move => "move",
            Self::Fetch => "fetch",
            Self::Think => "think",
            Self::Mcp { .. } => "mcp",
            Self::Skill { .. } => "skill",
            Self::Other => "other",
        }
    }

    /// The name kagan calls the call by, when the name says it: `<server>.<tool>` for an MCP call,
    /// the skill's name for a skill load.
    #[must_use]
    pub fn named(&self) -> Option<String> {
        match self {
            Self::Mcp { server, tool } => Some(format!("{server}.{tool}")),
            Self::Skill { name } => Some(name.clone()),
            _ => None,
        }
    }
}

const SKILL_LOAD: &str = "skill_load";
const LOAD_SKILL: &str = "load skill";
const A_SKILL: &str = "a skill";

/// One file a tool call changed. `old` is absent for a file it created.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiff {
    pub path: String,
    pub old: Option<String>,
    pub new: String,
}

/// One file a tool call looked at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLocation {
    pub path: String,
    pub line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub text: String,
    pub status: PlanStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
}

/// One thing the agent lets the person choose for this thread, such as its mode or its model, with
/// every value it offered and the one in force. The agent's own words; kagan keeps no table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadOption {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub value: String,
    pub values: Vec<ThreadOptionValue>,
}

impl ThreadOption {
    /// Whether this is the mode the chip names: the option ACP calls a mode, or one kagan reads
    /// from the session answer's `modes`.
    #[must_use]
    pub fn is_mode(&self) -> bool {
        self.category.as_deref() == Some(MODE_CATEGORY) || self.id == MODE_CATEGORY
    }

    /// The name of the value in force, as the agent wrote it.
    #[must_use]
    pub fn current(&self) -> Option<&ThreadOptionValue> {
        self.values.iter().find(|value| value.value == self.value)
    }

    /// Whether the agent offered `value` for this option.
    #[must_use]
    pub fn offers(&self, value: &str) -> bool {
        self.values.iter().any(|offered| offered.value == value)
    }
}

/// One value an agent offers for a [`ThreadOption`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadOptionValue {
    pub value: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The category, and the option id, ACP gives a session's modes.
pub const MODE_CATEGORY: &str = "mode";

/// The one rule that decides whether an option lets the agent act without asking. ACP carries no
/// risk flag, so a word in the option's id or name decides, for every agent alike.
pub struct ModeRisk;

impl ModeRisk {
    /// Whether this mode is risky. The id and the name are read, with `-` and `_` read as spaces,
    /// so `full-access` and `full access` agree.
    #[must_use]
    pub fn of(id: &str, name: &str) -> bool {
        let read = |text: &str| {
            text.to_lowercase()
                .replace(['-', '_'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        let haystack = format!("{} {}", read(id), read(name));
        RISKY_WORDS.iter().any(|word| haystack.contains(word))
    }
}

const RISKY_WORDS: &[&str] = &[
    "bypass",
    "full access",
    "yolo",
    "dangerous",
    "skip permissions",
    "no approval",
];

/// How a tool call kagan served ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolOutcome {
    Ok,
    Error,
}
