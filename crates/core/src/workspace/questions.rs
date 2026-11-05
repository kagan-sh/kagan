use gpui_kit::{App, Task};

use crate::database::Result;
use kg_domain::{InteractionOption, OpError, OpResult, PendingInteractionId, ThreadId};

use crate::Requester;

use super::{Request, Workspace, WorkspaceGlobal};

impl Workspace {
    /// Answers a pending question. Not an `Op`, so the socket and MCP cannot answer.
    /// Only the window calls this, as a person's choice.
    pub fn answer(
        thread_id: &ThreadId,
        interaction_id: PendingInteractionId,
        option_id: String,
        cx: &mut App,
    ) -> Task<Result<OpResult>> {
        let Some(global) = cx.try_global::<WorkspaceGlobal>() else {
            return Task::ready(Err(OpError::Failed("workspace is not open".into())));
        };
        let workspace = global.0.clone();
        let submitted = workspace.read(cx).submit(Request::Answer {
            thread_id: thread_id.clone(),
            interaction_id,
            option_id,
        });
        cx.spawn(async move |_| submitted.await)
    }
    /// Raises a question on the thread and resolves with the chosen option id.
    /// [`Self::answer`] answers it. Fails when the thread
    /// stops or the question is otherwise cancelled.
    #[cfg(any(test, feature = "test-support"))]
    pub fn ask(
        thread_id: &ThreadId,
        title: String,
        options: Vec<InteractionOption>,
        cx: &mut App,
    ) -> Task<Result<String>> {
        Self::raise_question(thread_id, title, options, false, None, cx)
    }
    /// Raises a question a plugin cannot answer, on the thread or standalone. Install, a
    /// provider change, and thread delete use this. [`Self::answer`] still answers it for a
    /// person. The question goes, and the task fails, when the requester goes.
    pub fn ask_for_person(
        thread_id: Option<&ThreadId>,
        requester: Option<Requester>,
        title: String,
        options: Vec<InteractionOption>,
        cx: &mut App,
    ) -> Task<Result<String>> {
        match thread_id {
            Some(thread_id) => Self::raise_question(thread_id, title, options, true, requester, cx),
            None => Self::standalone(title, options, requester, cx),
        }
    }
    fn raise_question(
        thread_id: &ThreadId,
        title: String,
        options: Vec<InteractionOption>,
        for_a_person: bool,
        requester: Option<Requester>,
        cx: &mut App,
    ) -> Task<Result<String>> {
        if requester.as_ref().is_some_and(Requester::is_gone) {
            return Task::ready(Err(Requester::stopped()));
        }
        let Some(global) = cx.try_global::<WorkspaceGlobal>() else {
            return Task::ready(Err(OpError::Failed("workspace is not open".into())));
        };
        let this = global.0.downgrade();
        let thread_id = thread_id.clone();
        cx.spawn(async move |cx| {
            let thread = crate::thread::thread(&this, &thread_id, cx)?;
            if thread.read_with(cx, |thread, _| thread.record.archived_at.is_some()) {
                return Err(OpError::Invalid(format!(
                    "thread {thread_id} is archived; ask from an active thread"
                )));
            }
            let (answer, answered) = async_channel::bounded(1);
            let raised = crate::thread::raise(
                &this,
                &thread,
                title,
                options,
                |workspace, thread_id, id| {
                    if for_a_person {
                        workspace.for_a_person.insert(id.clone());
                    }
                    crate::thread::insert_reply(workspace, thread_id.clone(), id, answer);
                },
                cx,
            )
            .await?;
            let Some(answer) = Requester::unless_gone(requester.as_ref(), answered.recv()).await
            else {
                crate::thread::withdraw(&this, raised, cx).await?;
                return Err(Requester::stopped());
            };
            answer.map_err(|_| {
                OpError::Failed(format!("the question on thread {thread_id} was cancelled"))
            })
        })
    }
    /// An install, a provider change, a thread delete, or a standalone approval.
    #[must_use]
    pub fn needs_a_person(&self, id: &PendingInteractionId) -> bool {
        self.for_a_person.contains(id) || self.approvals.contains_key(id)
    }
}
