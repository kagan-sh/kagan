use gpui_kit::{App, AppContext as _, Context, Window, actions};
use kg_core::Workspace;
use kg_domain::{Op, WindowOp};

use super::{Kagan, Shown};

actions!(
    kagan,
    [
        NextThread,
        PreviousThread,
        NextThreadNeedingYou,
        Thread1,
        Thread2,
        Thread3,
        Thread4,
        Thread5,
        Thread6,
        Thread7,
        Thread8,
        Thread9,
    ]
);

pub(super) fn bind(cx: &mut App) {
    cx.on_action(|_: &NextThread, cx| {
        Workspace::dispatch(Op::Window(WindowOp::NextThread), cx).detach();
    });
    cx.on_action(|_: &PreviousThread, cx| {
        Workspace::dispatch(Op::Window(WindowOp::PreviousThread), cx).detach();
    });
    cx.on_action(|_: &NextThreadNeedingYou, cx| {
        Workspace::dispatch(Op::Window(WindowOp::NextThreadNeedingYou), cx).detach();
    });
    cx.on_action(|_: &Thread1, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 1 }), cx).detach();
    });
    cx.on_action(|_: &Thread2, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 2 }), cx).detach();
    });
    cx.on_action(|_: &Thread3, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 3 }), cx).detach();
    });
    cx.on_action(|_: &Thread4, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 4 }), cx).detach();
    });
    cx.on_action(|_: &Thread5, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 5 }), cx).detach();
    });
    cx.on_action(|_: &Thread6, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 6 }), cx).detach();
    });
    cx.on_action(|_: &Thread7, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 7 }), cx).detach();
    });
    cx.on_action(|_: &Thread8, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 8 }), cx).detach();
    });
    cx.on_action(|_: &Thread9, cx| {
        Workspace::dispatch(Op::Window(WindowOp::Thread { index: 9 }), cx).detach();
    });
}

pub(super) fn perform(op: WindowOp, cx: &mut App) {
    let Some((handle, kagan)) = super::front(cx) else {
        return;
    };
    let _ = cx.update_window(handle, |_, window, cx| {
        kagan.update(cx, |this, cx| this.navigate(op, window, cx));
    });
}

impl Kagan {
    fn navigate(&mut self, op: WindowOp, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.sidebar.read(cx).thread_ids(&self.shows(cx), cx);
        let covered = match self.screen.shown() {
            Shown::Thread(view) => Some(view.read(cx).thread().clone()),
            Shown::Nothing | Shown::Start(_) => None,
        };
        let shown = self.shown_thread(cx);
        let position = rows.iter().position(|id| Some(id) == covered.as_ref());
        let id = match op {
            WindowOp::NextThread => rows
                .get(position.map_or(0, |index| (index + 1) % rows.len()))
                .cloned(),
            WindowOp::PreviousThread => rows
                .get(position.map_or(rows.len().saturating_sub(1), |index| {
                    if index == 0 {
                        rows.len() - 1
                    } else {
                        index - 1
                    }
                }))
                .cloned(),
            WindowOp::Thread { index } => rows.get(usize::from(index).saturating_sub(1)).cloned(),
            WindowOp::NextThreadNeedingYou => self
                .listed(cx)
                .filter(|thread| Some(&thread.id) != shown.as_ref())
                .filter(|thread| {
                    thread.unread()
                        || self
                            .questions
                            .read(cx)
                            .questions
                            .iter()
                            .any(|question| question.thread_id == thread.id)
                })
                .min_by_key(|thread| (thread.latest_attention_at, &thread.id))
                .map(|thread| thread.id.clone()),
            _ => return,
        };
        if let Some(id) = id {
            self.select(id, window, cx);
            cx.notify();
        } else {
            window.play_system_bell();
        }
    }
}
