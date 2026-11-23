use std::collections::VecDeque;

use agent_client_protocol::Responder;
use agent_client_protocol::v1::schema::{
    PermissionOptionId, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome,
};

use super::AcpEvent;
use super::map;

/// The permission requests the agent is waiting on, oldest first: the protocol asks for one answer
/// at a time, so only the first is read for, and a request with no options is never held.
#[derive(Default)]
pub(super) struct Asking(VecDeque<Held>);

struct Held {
    answer: async_channel::Receiver<String>,
    responder: Responder<RequestPermissionResponse>,
}

impl Asking {
    /// The oldest request's answer, to read beside the agent's updates.
    pub(super) fn opening(&mut self) -> Option<&mut async_channel::Receiver<String>> {
        self.0.front_mut().map(|held| &mut held.answer)
    }

    /// The person's answer, or the channel closing: the oldest request is answered either way.
    pub(super) fn answered(&mut self, answer: Result<String, async_channel::RecvError>) {
        let Some(asking) = self.0.pop_front() else {
            return;
        };
        let response = match answer {
            Ok(option) => selected(option),
            Err(_) => cancelled(),
        };
        let _ = asking.responder.respond(response);
    }

    /// The agent asked for its turn to end, so every request still open is answered `cancelled`, as
    /// the protocol asks.
    pub(super) fn cancel(&mut self) {
        for asking in self.0.drain(..) {
            let _ = asking.responder.respond(cancelled());
        }
    }

    /// Reports a request to the thread and holds its responder until the person answers.
    pub(super) async fn ask(
        &mut self,
        request: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
        events: &async_channel::Sender<AcpEvent>,
    ) {
        let options = map::options(&request.options);
        if options.is_empty() {
            let _ = responder.respond(cancelled());
            return;
        }
        let tool = map::tool_delta(
            request.tool_call.tool_call_id.to_string(),
            &request.tool_call.fields,
        );
        let (answer, answered) = async_channel::bounded(1);
        if events
            .send(AcpEvent::Permission {
                tool,
                options,
                answer,
            })
            .await
            .is_err()
        {
            let _ = responder.respond(cancelled());
            return;
        }
        self.0.push_back(Held {
            answer: answered,
            responder,
        });
    }
}

fn selected(option: String) -> RequestPermissionResponse {
    RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
        SelectedPermissionOutcome::new(PermissionOptionId::new(option)),
    ))
}

fn cancelled() -> RequestPermissionResponse {
    RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
}
