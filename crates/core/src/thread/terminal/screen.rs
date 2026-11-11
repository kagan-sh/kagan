use gpui_kit::{AsyncApp, Context, WeakEntity};
use kg_domain::{EventKind, ThreadId};
use kg_terminal::Finished;

use crate::workspace::Request;
use crate::{Workspace, WorkspaceEvent};

use super::turn::{Spin, follow, waiting};
use super::{Shown, TICK};

pub(super) async fn persist(
    this: &WeakEntity<Workspace>,
    keep: Vec<(ThreadId, u64, Finished)>,
    cx: &mut AsyncApp,
) {
    for (id, seq, finished) in keep {
        crate::workspace::hold(this, crate::workspace::Hold::Screen, cx).await;
        let snapshot = cx
            .background_executor()
            .spawn(async move { finished.snapshot() })
            .await;
        let Ok(database) = this.update(cx, |workspace, _| workspace.database.clone()) else {
            break;
        };
        let loss = crate::end::Loss::from_kept(snapshot.screen_kept, snapshot.history_dropped);
        let row = kg_store::ScreenRow {
            thread_id: id.clone(),
            started_seq: seq,
            cols: snapshot.cols,
            rows: snapshot.rows,
            replay: snapshot.replay,
            history_dropped: snapshot.history_dropped,
            screen_kept: snapshot.screen_kept,
        };
        match database.call(move |store| store.put_screen(&row)).await {
            Ok(true) => {
                // A closed workspace has nothing to remember it in.
                let _ = this.update(cx, |workspace, cx| {
                    remember(workspace, &id, seq, loss, cx);
                    mark_saved(workspace, &id, seq);
                });
            }
            Ok(false) => {
                // A closed workspace has nothing to mark.
                let _ = this.update(cx, |workspace, _| mark_saved(workspace, &id, seq));
            }
            Err(error) => eprintln!("kg: could not keep the screen: {error}"),
        }
    }
}

fn mark_saved(workspace: &mut Workspace, id: &ThreadId, seq: u64) {
    if let Some(run) = workspace.runs.get_mut(id)
        && run.started_seq == Some(seq)
    {
        run.saved = true;
    }
}

pub(crate) fn remember(
    workspace: &mut Workspace,
    id: &ThreadId,
    seq: u64,
    loss: Option<crate::end::Loss>,
    cx: &mut Context<Workspace>,
) {
    let Some(run) = workspace.runs.get_mut(id) else {
        return;
    };
    if run.started_seq != Some(seq) {
        return;
    }
    run.loss = loss;
    cx.emit(WorkspaceEvent::Changed);
}
fn watch(cx: &mut Context<Workspace>) {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(TICK).await;
            let Ok((ended, waited, started, keep, idle)) = this.update(cx, |workspace, cx| {
                let mut ended = Vec::new();
                let mut waited = Vec::new();
                let mut started = Vec::new();
                let mut keep = Vec::new();
                let mut following = false;
                let now = cx.background_executor().now();
                for (id, run) in &workspace.runs {
                    let Some(shown) = &run.shown else {
                        continue;
                    };
                    let terminal = match shown {
                        Shown::Running(terminal) => terminal,
                        Shown::Ended(finished) => {
                            let (changed, follows) = finished.poll();
                            if changed {
                                cx.emit(WorkspaceEvent::Screen(id.clone(), None));
                            }
                            following |= follows;
                            // `settled` is set only after the reader is joined and the last hold released.
                            if finished.settled()
                                && let Some(seq) = run.unsaved()
                            {
                                following = true;
                                keep.push((id.clone(), seq, finished.clone()));
                            }
                            continue;
                        }
                    };
                    following = true;
                    let changed = terminal.view().take_changed();
                    if changed {
                        cx.emit(WorkspaceEvent::Screen(id.clone(), None));
                    }
                    if let Some(thread) = workspace.threads.get(id) {
                        match follow(thread, terminal.title().as_deref(), changed, now, cx) {
                            Some(Spin::Opened) => {
                                cx.emit(WorkspaceEvent::Changed);
                                started.push(id.clone());
                            }
                            Some(Spin::Resumed) => cx.emit(WorkspaceEvent::Changed),
                            Some(Spin::Closed) => waited.push(id.clone()),
                            None => {}
                        }
                    }
                    if let Some(exit) = terminal.exit()
                        && !run.exiting
                    {
                        ended.push((id.clone(), exit));
                    }
                }
                for (id, _) in &ended {
                    workspace.run_mut(id).exiting = true;
                }
                following |= crate::thread::extra::poll(workspace, cx);
                workspace.watching = following;
                (ended, waited, started, keep, !workspace.watching)
            }) else {
                break;
            };
            for id in started {
                if let Err(error) =
                    Workspace::events(&this, id, vec![EventKind::TurnStarted], cx).await
                {
                    eprintln!("kg: could not record that a turn started: {error}");
                }
            }
            for id in waited {
                if let Err(error) = waiting(&this, id, cx).await {
                    eprintln!("kg: could not record that a thread is waiting: {error}");
                }
            }
            persist(&this, keep, cx).await;
            for (thread_id, exit) in ended {
                let submitted = this.update(cx, |workspace, _| {
                    workspace.submit(Request::Exited { thread_id, exit })
                });
                if let Ok(submitted) = submitted
                    && let Err(error) = submitted.await
                {
                    eprintln!("kg: could not record how a program ended: {error}");
                }
            }
            if idle {
                break;
            }
        }
    })
    .detach();
}
pub(super) fn keep_screen(workspace: &mut Workspace, id: &ThreadId, cx: &mut Context<Workspace>) {
    let run = workspace.run_mut(id);
    if let Some(Shown::Running(terminal)) = run.shown.take() {
        run.shown = Some(Shown::Ended(terminal.finish()));
    }
    cx.emit(WorkspaceEvent::Screen(id.clone(), None));
    follow_terminals(workspace, cx);
}

pub(crate) fn follow_terminals(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    if !workspace.watching {
        workspace.watching = true;
        watch(cx);
    }
}
