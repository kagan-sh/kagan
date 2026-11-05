use async_channel::{Receiver, Sender};
use futures::future::{Either, select};
use gpui_kit::{App, Context, Task};
use kg_domain::{InteractionOption, OpError, OpResult, PendingInteractionId};

use crate::workspace::Request;
use crate::{Workspace, WorkspaceEvent};

#[derive(Clone, Debug)]
pub struct Approval {
    pub id: PendingInteractionId,
    pub title: String,
    pub options: Vec<InteractionOption>,
}

/// The socket connection an op came from. `gone` closes when the connection does.
#[derive(Clone, Debug)]
pub struct Requester(Receiver<()>);

impl Requester {
    #[must_use]
    pub fn new(gone: Receiver<()>) -> Self {
        Self(gone)
    }

    pub(crate) fn is_gone(&self) -> bool {
        self.0.is_closed()
    }

    /// `None` when the requester went away first.
    pub(crate) async fn unless_gone<T>(
        requester: Option<&Self>,
        work: impl Future<Output = T>,
    ) -> Option<T> {
        let Some(requester) = requester else {
            return Some(work.await);
        };
        let gone = requester.0.recv();
        futures::pin_mut!(gone, work);
        match select(gone, work).await {
            Either::Left(_) => None,
            Either::Right((done, _)) => Some(done),
        }
    }

    pub(crate) fn stopped() -> OpError {
        OpError::Failed("the command that asked has stopped".into())
    }
}

pub(crate) struct Waiting {
    approval: Approval,
    answer: Sender<String>,
}

impl Workspace {
    pub fn approvals(&self) -> impl Iterator<Item = &Approval> {
        self.approvals.values().map(|waiting| &waiting.approval)
    }

    pub(crate) fn standalone(
        title: String,
        options: Vec<InteractionOption>,
        requester: Option<Requester>,
        cx: &mut App,
    ) -> Task<Result<String, OpError>> {
        if requester.as_ref().is_some_and(Requester::is_gone) {
            return Task::ready(Err(Requester::stopped()));
        }
        let workspace = Self::global(cx);
        let (answer, answered) = async_channel::bounded(1);
        let id = PendingInteractionId(crate::workspace::id("approval"));
        workspace.update(cx, |workspace, cx| {
            workspace.approvals.insert(
                id.clone(),
                Waiting {
                    approval: Approval {
                        id: id.clone(),
                        title,
                        options,
                    },
                    answer,
                },
            );
            cx.emit(WorkspaceEvent::Open);
            cx.emit(WorkspaceEvent::Changed);
            cx.notify();
        });
        let workspace = workspace.downgrade();
        cx.spawn(async move |cx| {
            let Some(answer) = Requester::unless_gone(requester.as_ref(), answered.recv()).await
            else {
                let _ = workspace.update(cx, |workspace, cx| {
                    workspace.approvals.remove(&id);
                    cx.emit(WorkspaceEvent::Changed);
                    cx.notify();
                });
                return Err(Requester::stopped());
            };
            answer.map_err(|_| OpError::Failed("the approval was cancelled".into()))
        })
    }

    /// Answers a standalone approval from native UI only; no operation exposes this method.
    pub fn answer_standalone(
        interaction_id: PendingInteractionId,
        option_id: String,
        cx: &mut App,
    ) -> Task<Result<OpResult, OpError>> {
        let workspace = Self::global(cx);
        let submitted = workspace.read(cx).submit(Request::AnswerStandalone {
            interaction_id,
            option_id,
        });
        cx.spawn(async move |_| submitted.await)
    }

    pub(crate) fn answer_approval(
        &mut self,
        id: &PendingInteractionId,
        option: String,
        cx: &mut Context<Self>,
    ) -> Result<OpResult, OpError> {
        let waiting = self
            .approvals
            .get(id)
            .ok_or_else(|| OpError::NotFound("this request was already answered".into()))?;
        if !waiting
            .approval
            .options
            .iter()
            .any(|choice| choice.id == option)
        {
            return Err(OpError::Invalid(format!(
                "approval {id} has no option {option}"
            )));
        }
        if let Some(waiting) = self.approvals.remove(id) {
            waiting
                .answer
                .try_send(option)
                .map_err(|_| OpError::Failed("the approval was cancelled".into()))?;
        }
        cx.emit(WorkspaceEvent::Changed);
        cx.notify();
        Ok(OpResult::Done)
    }
}
