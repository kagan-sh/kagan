use agent_client_protocol::v1::schema::{
    Content, ContentBlock, ContentChunk, PermissionOption, PermissionOptionKind,
    PlanEntry as AcpPlanEntry, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent,
    ToolCallLocation, ToolCallStatus, ToolCallUpdateFields, ToolKind as AcpToolKind,
};
use kg_domain::{
    InteractionOption, OptionRole, PlanEntry, PlanStatus, ToolDiff, ToolKind, ToolLocation,
    ToolStatus, TurnOutcome,
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
pub(super) struct ToolUpdate {
    pub(super) id: String,
    pub(super) name: Option<String>,
    pub(super) kind: Option<ToolKind>,
    pub(super) title: Option<String>,
    pub(super) status: Option<ToolStatus>,
    pub(super) input: Option<serde_json::Value>,
    pub(super) output: Option<String>,
    pub(super) diff: Vec<ToolDiff>,
    pub(super) locations: Vec<ToolLocation>,
}

#[derive(Debug)]
pub(super) enum Update {
    Message(Chunk),
    Thought(Chunk),
    Tool(ToolUpdate),
    Plan(Vec<PlanEntry>),
    Usage { used: u64, size: u64 },
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
        _ => None,
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

fn tool_call(call: &ToolCall) -> ToolUpdate {
    ToolUpdate {
        id: call.tool_call_id.to_string(),
        name: call.name.clone(),
        kind: Some(kind(call.kind)),
        title: Some(call.title.clone()),
        status: Some(status(call.status)),
        input: call.raw_input.as_ref().map(input),
        output: output(&call.content, call.raw_output.as_ref()),
        diff: diffs(&call.content),
        locations: call.locations.iter().map(location).collect(),
    }
}

pub(super) fn tool_delta(id: String, fields: &ToolCallUpdateFields) -> ToolUpdate {
    ToolUpdate {
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
