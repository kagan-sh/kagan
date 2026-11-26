use agent_client_protocol::v1::schema::{
    Content, ContentBlock, ContentChunk, PermissionOption, PermissionOptionKind,
    PlanEntry as AcpPlanEntry, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionModeState, SessionUpdate, StopReason, TextContent,
    ToolCall as AcpToolCall, ToolCallContent, ToolCallLocation, ToolCallStatus,
    ToolCallUpdateFields, ToolKind as AcpToolKind,
};
use kg_domain::{
    InteractionOption, MODE_CATEGORY, OptionRole, PlanEntry, PlanStatus, ThreadOption,
    ThreadOptionValue, ToolCall, ToolDiff, ToolKind, ToolLocation, ToolStatus, TurnOutcome,
};

const TOOL_TEXT_MAX: usize = 64 * 1024;
// A write tool's input is the whole file, and the log, `kg thread events` and every chat read it.
const TOOL_INPUT_MAX: usize = 8 * 1024;
const CUT: &str = "\n… (truncated)";

#[derive(Debug)]
pub(super) struct Chunk {
    pub(super) text: String,
    pub(super) message_id: Option<String>,
}

#[derive(Debug)]
pub(super) enum Update {
    Message(Chunk),
    Thought(Chunk),
    Tool(ToolCall),
    Plan(Vec<PlanEntry>),
    Usage {
        used: u64,
        size: u64,
    },
    /// The agent changed its mode.
    Mode(String),
    /// The agent's whole set of options and their values.
    Config(Vec<ThreadOption>),
}

pub(super) fn read(update: &SessionUpdate) -> Option<Update> {
    match update {
        SessionUpdate::AgentMessageChunk(content) => Some(Update::Message(chunk(content)?)),
        SessionUpdate::AgentThoughtChunk(content) => Some(Update::Thought(chunk(content)?)),
        SessionUpdate::ToolCall(call) => Some(Update::Tool(tool_call(call))),
        SessionUpdate::ToolCallUpdate(update) => Some(Update::Tool(tool_delta(
            update.tool_call_id.to_string(),
            &update.fields,
        ))),
        SessionUpdate::Plan(plan) => Some(Update::Plan(plan.entries.iter().map(entry).collect())),
        SessionUpdate::UsageUpdate(usage) => Some(Update::Usage {
            used: usage.used,
            size: usage.size,
        }),
        SessionUpdate::CurrentModeUpdate(update) => {
            Some(Update::Mode(update.current_mode_id.to_string()))
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            Some(Update::Config(config_options(&update.config_options)))
        }
        _ => None,
    }
}

// The session answer's `modes` and `configOptions`, as the chip and its menu draw them. `modes`
// becomes the option `mode`, and a configuration option that names itself `mode` replaces it.
pub(super) fn session_options(
    modes: Option<&SessionModeState>,
    config: Option<&[SessionConfigOption]>,
) -> Vec<ThreadOption> {
    let mut options: Vec<ThreadOption> = modes.map(mode_state).into_iter().collect();
    for option in config_options(config.unwrap_or_default()) {
        match options.iter_mut().find(|known| known.id == option.id) {
            Some(slot) => *slot = option,
            None => options.push(option),
        }
    }
    options
}

/// The configuration options of one answer or update. Only a select is kept: its values have names
/// the chip's menu shows, where a boolean has only on and off.
pub(super) fn config_options(config: &[SessionConfigOption]) -> Vec<ThreadOption> {
    config.iter().filter_map(option).collect()
}

fn mode_state(modes: &SessionModeState) -> ThreadOption {
    ThreadOption {
        id: MODE_CATEGORY.to_owned(),
        name: "Mode".to_owned(),
        category: Some(MODE_CATEGORY.to_owned()),
        description: None,
        value: modes.current_mode_id.to_string(),
        values: modes
            .available_modes
            .iter()
            .map(|mode| ThreadOptionValue {
                value: mode.id.to_string(),
                name: mode.name.clone(),
                description: mode.description.clone(),
            })
            .collect(),
    }
}

fn option(option: &SessionConfigOption) -> Option<ThreadOption> {
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };
    let values = match &select.options {
        SessionConfigSelectOptions::Ungrouped(values) => values.iter().collect::<Vec<_>>(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| &group.options)
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    Some(ThreadOption {
        id: option.id.to_string(),
        name: option.name.clone(),
        category: option.category.as_ref().map(category),
        description: option.description.clone(),
        value: select.current_value.to_string(),
        values: values
            .into_iter()
            .map(|value| ThreadOptionValue {
                value: value.value.to_string(),
                name: value.name.clone(),
                description: value.description.clone(),
            })
            .collect(),
    })
}

fn category(category: &SessionConfigOptionCategory) -> String {
    match category {
        SessionConfigOptionCategory::Mode => MODE_CATEGORY.to_owned(),
        SessionConfigOptionCategory::Model => "model".to_owned(),
        SessionConfigOptionCategory::ModelConfig => "model_config".to_owned(),
        SessionConfigOptionCategory::ThoughtLevel => "thought_level".to_owned(),
        SessionConfigOptionCategory::Other(other) => other.clone(),
        _ => "other".to_owned(),
    }
}

fn chunk(content: &ContentChunk) -> Option<Chunk> {
    let ContentBlock::Text(TextContent { text, .. }) = &content.content else {
        return None;
    };
    Some(Chunk {
        text: text.clone(),
        message_id: content.message_id.as_ref().map(ToString::to_string),
    })
}

fn entry(entry: &AcpPlanEntry) -> PlanEntry {
    PlanEntry {
        text: entry.content.clone(),
        status: match entry.status {
            agent_client_protocol::v1::schema::PlanEntryStatus::Completed => PlanStatus::Completed,
            agent_client_protocol::v1::schema::PlanEntryStatus::InProgress => {
                PlanStatus::InProgress
            }
            _ => PlanStatus::Pending,
        },
    }
}

fn tool_call(call: &AcpToolCall) -> ToolCall {
    ToolCall {
        id: call.tool_call_id.to_string(),
        name: call.name.clone(),
        kind: Some(kind(call.kind)),
        title: Some(call.title.clone()),
        status: Some(status(call.status)),
        input: call.raw_input.as_ref().map(input),
        output: output(&call.content, call.raw_output.as_ref()),
        diff: diffs(&call.content),
        locations: call.locations.iter().map(location).collect(),
        approval: None,
    }
}

pub(super) fn tool_delta(id: String, fields: &ToolCallUpdateFields) -> ToolCall {
    ToolCall {
        id,
        name: fields.name.clone(),
        kind: fields.kind.map(kind),
        title: fields.title.clone(),
        status: fields.status.map(status),
        input: fields.raw_input.as_ref().map(input),
        output: fields
            .content
            .as_deref()
            .and_then(|content| output(content, fields.raw_output.as_ref())),
        diff: fields.content.as_deref().map_or_else(Vec::new, diffs),
        locations: fields
            .locations
            .as_deref()
            .map_or_else(Vec::new, |locations| {
                locations.iter().map(location).collect()
            }),
        approval: None,
    }
}

fn output(content: &[ToolCallContent], raw: Option<&serde_json::Value>) -> Option<String> {
    let parts: Vec<String> = content
        .iter()
        .filter_map(|content| match content {
            ToolCallContent::Content(Content {
                content: ContentBlock::Text(TextContent { text, .. }),
                ..
            }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    let text = match (parts.is_empty(), raw) {
        (true, Some(raw)) => match raw {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        },
        (true, None) => return None,
        (false, _) => parts.join("\n\n"),
    };
    (!text.trim().is_empty()).then(|| cap(text))
}

fn input(value: &serde_json::Value) -> serde_json::Value {
    let text = value.to_string();
    if text.len() <= TOOL_INPUT_MAX {
        return value.clone();
    }
    serde_json::Value::String(cut(&text, TOOL_INPUT_MAX))
}

fn diffs(content: &[ToolCallContent]) -> Vec<ToolDiff> {
    content
        .iter()
        .filter_map(|content| match content {
            ToolCallContent::Diff(diff) => Some(ToolDiff {
                path: diff.path.display().to_string(),
                old: diff.old_text.as_ref().map(|old| cap(old.clone())),
                new: cap(diff.new_text.clone()),
            }),
            _ => None,
        })
        .collect()
}

fn location(location: &ToolCallLocation) -> ToolLocation {
    ToolLocation {
        path: location.path.display().to_string(),
        line: location.line,
    }
}

fn cap(text: String) -> String {
    if text.len() <= TOOL_TEXT_MAX {
        return text;
    }
    cut(&text, TOOL_TEXT_MAX)
}

fn cut(text: &str, max: usize) -> String {
    text[..text.floor_char_boundary(max)].to_owned() + CUT
}

fn kind(kind: AcpToolKind) -> ToolKind {
    match kind {
        AcpToolKind::Read => ToolKind::Read,
        AcpToolKind::Edit => ToolKind::Edit,
        AcpToolKind::Delete => ToolKind::Delete,
        AcpToolKind::Move => ToolKind::Move,
        AcpToolKind::Search => ToolKind::Search,
        AcpToolKind::Execute => ToolKind::Execute,
        AcpToolKind::Think => ToolKind::Think,
        AcpToolKind::Fetch => ToolKind::Fetch,
        AcpToolKind::SwitchMode => ToolKind::SwitchMode,
        _ => ToolKind::Other,
    }
}

fn status(status: ToolCallStatus) -> ToolStatus {
    match status {
        ToolCallStatus::InProgress => ToolStatus::InProgress,
        ToolCallStatus::Completed => ToolStatus::Completed,
        ToolCallStatus::Failed => ToolStatus::Failed,
        // Pending, and any status kagan has not met, is read as still to come: nothing is called
        // failed on the strength of a word kagan does not know.
        _ => ToolStatus::Pending,
    }
}

pub(super) fn options(options: &[PermissionOption]) -> Vec<InteractionOption> {
    options
        .iter()
        .map(|option| InteractionOption {
            id: option.option_id.to_string(),
            label: option.name.clone(),
            role: match option.kind {
                PermissionOptionKind::AllowOnce => Some(OptionRole::Primary),
                // Always allow is a grant the agent remembers, and the card must not open on it.
                PermissionOptionKind::AllowAlways => None,
                _ => Some(OptionRole::Destructive),
            },
        })
        .collect()
}

pub(super) fn phrase(kind: Option<ToolKind>) -> &'static str {
    match kind {
        Some(ToolKind::Read) => "read a file",
        Some(ToolKind::Edit) => "edit a file",
        Some(ToolKind::Delete) => "delete a file",
        Some(ToolKind::Move) => "move a file",
        Some(ToolKind::Search) => "search the folder",
        Some(ToolKind::Execute) => "run a command",
        Some(ToolKind::Fetch) => "fetch a page",
        _ => "use a tool",
    }
}

pub(super) fn name(update: &SessionUpdate) -> String {
    serde_json::to_value(update)
        .ok()
        .and_then(|value| {
            value
                .get("sessionUpdate")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "an update with no name".into())
}

pub(super) fn outcome(reason: StopReason) -> (TurnOutcome, Option<String>) {
    match reason {
        StopReason::EndTurn => (TurnOutcome::Completed, None),
        StopReason::Cancelled => (TurnOutcome::Interrupted, None),
        StopReason::MaxTokens => (
            TurnOutcome::Failed,
            Some("the agent reached its token limit".into()),
        ),
        StopReason::MaxTurnRequests => (
            TurnOutcome::Failed,
            Some("the agent reached its turn limit".into()),
        ),
        StopReason::Refusal => (
            TurnOutcome::Failed,
            Some("the agent refused to continue".into()),
        ),
        _ => (
            TurnOutcome::Failed,
            Some("the agent stopped for a reason kagan does not know".into()),
        ),
    }
}

#[cfg(test)]
#[path = "map_test.rs"]
mod tests;
