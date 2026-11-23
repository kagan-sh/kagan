use agent_client_protocol::v1::schema::{ContentBlock, SessionUpdate, StopReason, TextContent};
use kg_domain::TurnOutcome;

/// One chunk of the agent's reply, and the message the agent says it belongs to.
pub(super) struct Chunk {
    pub(super) text: String,
    pub(super) message_id: Option<String>,
}

/// The text of an agent message chunk. Every other update is slice 2's to show.
pub(super) fn chunk(update: &SessionUpdate) -> Option<Chunk> {
    let SessionUpdate::AgentMessageChunk(content) = update else {
        return None;
    };
    let ContentBlock::Text(TextContent { text, .. }) = &content.content else {
        return None;
    };
    Some(Chunk {
        text: text.clone(),
        message_id: content.message_id.as_ref().map(ToString::to_string),
    })
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
        _ => (TurnOutcome::Failed, None),
    }
}
