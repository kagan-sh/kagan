use crate::theme::Tone;
use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{Op, OpError, OpResult, PendingInteraction, Thread, ThreadOp};

pub(crate) struct Attention {
    pub(crate) questions: Vec<PendingInteraction>,
    load: Task<()>,
    _workspace: Subscription,
}
struct AttentionGlobal(Entity<Attention>);
impl Global for AttentionGlobal {}
pub(crate) fn global(cx: &App) -> Entity<Attention> {
    cx.global::<AttentionGlobal>().0.clone()
}
pub(super) fn init(cx: &mut App) {
    let workspace = Workspace::global(cx);
    let attention = cx.new(|cx: &mut Context<Attention>| {
        let watch = cx.subscribe(&workspace, |this, _, event, cx| {
            if matches!(
                event,
                WorkspaceEvent::Changed | WorkspaceEvent::Event { .. } | WorkspaceEvent::Waiting(_)
            ) {
                this.load(cx);
            }
        });
        let mut this = Attention {
            questions: Vec::new(),
            load: Task::ready(()),
            _workspace: watch,
        };
        this.load(cx);
        this
    });
    cx.set_global(AttentionGlobal(attention));
}
impl Attention {
    fn load(&mut self, cx: &mut Context<Self>) {
        let listed = Workspace::dispatch(
            Op::Thread(ThreadOp::List {
                project_id: None,
                archived: Some(false),
                limit: None,
                before: None,
            }),
            cx,
        );
        self.load = cx.spawn(async move |this, cx| {
            let Ok(OpResult::Threads(threads)) = listed.await else {
                return;
            };
            let asked = cx.update(|cx| {
                threads
                    .into_iter()
                    .map(|thread| {
                        Workspace::dispatch(
                            Op::Thread(ThreadOp::Interactions { id: thread.id }),
                            cx,
                        )
                    })
                    .collect::<Vec<Task<Result<OpResult, OpError>>>>()
            });
            let mut questions = Vec::new();
            for task in asked {
                if let Ok(OpResult::Interactions(list)) = task.await {
                    questions.extend(list);
                }
            }
            _ = this.update(cx, |this, cx| {
                if this.questions != questions {
                    this.questions = questions;
                    cx.notify();
                }
            });
        });
    }
    pub(super) fn tone(&self, thread: &Thread, cx: &App) -> Tone {
        let workspace = Workspace::global(cx);
        let workspace = workspace.read(cx);
        Tone::of(
            thread,
            workspace
                .thread(&thread.id)
                .is_some_and(|thread| thread.read(cx).busy()),
            workspace.program_ended(&thread.id),
            self.questions
                .iter()
                .any(|question| question.thread_id == thread.id),
        )
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn count(&self, cx: &App) -> usize {
        Workspace::global(cx)
            .read(cx)
            .sidebar(cx)
            .threads
            .iter()
            .filter(|thread| self.tone(thread, cx) == Tone::Needs)
            .count()
    }
}
