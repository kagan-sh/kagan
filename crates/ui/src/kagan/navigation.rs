use gpui_kit::{App, Context, Window, actions};
use kg_domain::WindowOp;

use super::{Kagan, Shown, window};

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
    window::bind::<NextThread>(cx, WindowOp::NextThread);
    window::bind::<PreviousThread>(cx, WindowOp::PreviousThread);
    window::bind::<NextThreadNeedingYou>(cx, WindowOp::NextThreadNeedingYou);
    window::bind::<Thread1>(cx, WindowOp::Thread { index: 1 });
    window::bind::<Thread2>(cx, WindowOp::Thread { index: 2 });
    window::bind::<Thread3>(cx, WindowOp::Thread { index: 3 });
    window::bind::<Thread4>(cx, WindowOp::Thread { index: 4 });
    window::bind::<Thread5>(cx, WindowOp::Thread { index: 5 });
    window::bind::<Thread6>(cx, WindowOp::Thread { index: 6 });
    window::bind::<Thread7>(cx, WindowOp::Thread { index: 7 });
    window::bind::<Thread8>(cx, WindowOp::Thread { index: 8 });
    window::bind::<Thread9>(cx, WindowOp::Thread { index: 9 });
}

pub(super) fn perform(op: WindowOp, cx: &mut App) {
    window::with_front(cx, |this, window, cx| this.navigate(op, window, cx));
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
                .filter(|thread| self.questions.read(cx).needs_you(thread))
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
