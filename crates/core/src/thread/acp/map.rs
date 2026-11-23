use agent_client_protocol::v1::schema::{
    Content, ContentBlock, ContentChunk, PermissionOption, PermissionOptionKind,
    PlanEntry as AcpPlanEntry, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent,
    ToolCallLocation, ToolCallStatus, ToolCallUpdateFields, ToolKind as AcpToolKind,
};
use kg_domain::{
    InteractionOption, OptionRole, PlanEntry, PlanStatus, ToolDiff, ToolKind, ToolLocation,
    ToolStatus, TurnOutcome,
};

/// The most of one side of a diff or one tool output kagan keeps, so one event stays small enough
/// to read and to keep in the log.
const TOOL_TEXT_MAX: usize = 64 * 1024;
const CUT: &str = "\n… (truncated)";

/// One chunk of the agent's reply, and the message the agent says it belongs to.
#[derive(Debug)]
pub(super) struct Chunk {
    pub(super) text: String,
    pub(super) message_id: Option<String>,
}

/// The fields of one tool call that this update carries. A later update with the same `id`
/// replaces only the fields it names.
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

/// What one ACP update means, before it becomes a log event.
#[derive(Debug)]
pub(super) enum Update {
    Message(Chunk),
    Thought(Chunk),
    Tool(ToolUpdate),
    Plan(Vec<PlanEntry>),
    Usage { used: u64, size: u64 },
}

/// Reads the updates a transcript draws. Everything else is not requested or not drawn yet.
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
        input: call.raw_input.clone(),
        output: output(&call.content, call.raw_output.as_ref()),
        diff: diffs(&call.content),
        locations: call.locations.iter().map(location).collect(),
    }
}

/// The same, only for the fields a `tool_call_update` names, and none for the ones it leaves out.
pub(super) fn tool_delta(id: String, fields: &ToolCallUpdateFields) -> ToolUpdate {
    ToolUpdate {
        id,
        name: fields.name.clone(),
        kind: fields.kind.map(kind),
        title: fields.title.clone(),
        status: fields.status.map(status),
        input: fields.raw_input.clone(),
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

/// The text of every text content block, then the raw output, joined by a blank line.
fn output(content: &[ToolCallContent], raw: Option<&serde_json::Value>) -> Option<String> {
    let mut parts: Vec<String> = content
        .iter()
        .filter_map(|content| match content {
            ToolCallContent::Content(Content {
                content: ContentBlock::Text(TextContent { text, .. }),
                ..
            }) => Some(cap(text.clone())),
            _ => None,
        })
        .collect();
    if let Some(raw) = raw {
        let text = match raw {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        parts.push(cap(text));
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
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
    let mut cut = TOOL_TEXT_MAX;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].to_owned() + CUT
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
        ToolCallStatus::Pending => ToolStatus::Pending,
        ToolCallStatus::InProgress => ToolStatus::InProgress,
        ToolCallStatus::Completed => ToolStatus::Completed,
        _ => ToolStatus::Failed,
    }
}

/// The agent's options, with kagan's role for each kind. Labels and order stay the agent's.
pub(super) fn options(options: &[PermissionOption]) -> Vec<InteractionOption> {
    options
        .iter()
        .map(|option| InteractionOption {
            id: option.option_id.to_string(),
            label: option.name.clone(),
            role: Some(match option.kind {
                PermissionOptionKind::AllowOnce => OptionRole::Primary,
                PermissionOptionKind::AllowAlways => OptionRole::Safe,
                _ => OptionRole::Destructive,
            }),
        })
        .collect()
}

/// What the question says the agent wants to do, from the tool's kind.
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

/// What an update calls itself on the wire, for the one log line an unread kind gets.
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

/// What the turn's stop reason means, and the words shown when the agent stopped short.
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
mod tests {
    use super::*;

    #[test]
    fn every_stop_reason_has_its_outcome() {
        for (reason, want, error) in [
            (StopReason::EndTurn, TurnOutcome::Completed, None),
            (StopReason::Cancelled, TurnOutcome::Interrupted, None),
            (
                StopReason::MaxTokens,
                TurnOutcome::Failed,
                Some("the agent reached its token limit"),
            ),
            (
                StopReason::MaxTurnRequests,
                TurnOutcome::Failed,
                Some("the agent reached its turn limit"),
            ),
            (
                StopReason::Refusal,
                TurnOutcome::Failed,
                Some("the agent refused to continue"),
            ),
        ] {
            assert_eq!(outcome(reason), (want, error.map(str::to_owned)));
        }
    }

    #[test]
    fn a_long_output_is_cut_at_a_character() {
        let long = "é".repeat(TOOL_TEXT_MAX);
        let cut = cap(long);
        assert!(cut.len() < TOOL_TEXT_MAX + CUT.len() + 4, "{}", cut.len());
        assert!(cut.ends_with(CUT), "{}", &cut[cut.len() - 40..]);
        assert_eq!(cap("short".into()), "short");
    }
}
