use super::screen;
use super::{
    ArchiveThread, Kagan, NewThread, RestoreThread, SaveDiagnostics, Shown, StartAgain, StopThread,
    TogglePinned, toast::Toast,
};
use std::collections::BTreeSet;

use crate::terminal::{TerminalView, Typed};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, AppContext as _, Context, PathPromptOptions, Window};
use kg_core::Workspace;
use kg_domain::{
    Op, OpError, OpResult, PreferenceOp, ProjectId, Thread, ThreadId, ThreadOp, ThreadStatus,
    archive_span,
};

impl Kagan {
    pub(super) fn listed<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a Thread> {
        self.sidebar
            .read(cx)
            .listing
            .threads
            .iter()
            .filter(|thread| thread.archived_at.is_none())
    }

    pub(crate) fn shown_thread(&self, cx: &App) -> Option<ThreadId> {
        match self.screen.shown() {
            Shown::Thread(view) if !self.in_settings() => Some(view.read(cx).thread().clone()),
            Shown::Thread(_) | Shown::Nothing | Shown::Start(_) => None,
        }
    }

    pub(crate) fn shown_project(&self, cx: &App) -> Option<ProjectId> {
        if self.in_settings() {
            return None;
        }
        match (self.shown_thread(cx), self.screen.shown()) {
            (Some(id), _) => self.record(&id, cx).map(|thread| thread.project_id.clone()),
            (None, Shown::Start(project)) => Some(project.clone()),
            (None, Shown::Nothing | Shown::Thread(_)) => None,
        }
    }
    pub(super) fn record<'a>(&'a self, id: &ThreadId, cx: &'a App) -> Option<&'a Thread> {
        self.sidebar
            .read(cx)
            .listing
            .threads
            .iter()
            .find(|thread| thread.id == *id)
            .or(self.open_thread.as_ref().filter(|thread| thread.id == *id))
    }

    pub(super) fn note_shown(&mut self, cx: &App) {
        let Some(id) = self.screen.covered_thread(cx) else {
            self.open_thread = None;
            return;
        };
        if self
            .sidebar
            .read(cx)
            .listing
            .threads
            .iter()
            .any(|thread| thread.id == id)
        {
            self.open_thread = None;
            return;
        }
        self.open_thread = Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .map(|thread| thread.read(cx).record().clone())
            .filter(|thread| thread.visibility == kg_domain::Visibility::Visible);
    }

    pub(super) fn waited(&mut self, id: &ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.screen.current(), screen::Center::Work(Shown::Thread(view)) if view.read(cx).thread() == id)
        {
            Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx).detach();
        } else {
            self.refresh(window, cx);
        }
    }

    pub(super) fn read_shown(&self, cx: &mut Context<Self>) {
        if let screen::Center::Work(Shown::Thread(view)) = self.screen.current() {
            let id = view.read(cx).thread().clone();
            if Workspace::global(cx)
                .read(cx)
                .thread(&id)
                .is_some_and(|thread| thread.read(cx).record().mark == kg_domain::ThreadMark::Done)
            {
                Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id }), cx).detach();
            }
        }
    }

    pub(crate) fn select(&mut self, id: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal_thread(&id, cx));
        self.start_on_load = None;
        if self.start.plugin_thread.as_ref() != Some(&id) {
            self.cancel_plugin_start();
        }
        let view = cx.new(|cx| TerminalView::new(id.clone(), None, self.terminal_font_size, cx));
        self.show_work(Shown::Thread(view.clone()), window, cx);
        self.focus_work(window, cx);
        self.typed_watch = Some(cx.subscribe(&view, |this, _, _: &Typed, cx| this.typed(cx)));
        self.sidebar.update(cx, |sidebar, _| sidebar.choose(None));
        self.shown_changed(window, cx);
        self.note_shown(cx);
        Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id }), cx).detach();
        cx.notify();
    }

    pub(super) fn shown_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_changes(cx);
        self.load_details(cx);
        self.sync_tabs(window, cx);
        self.tell_host(window, cx);
    }

    pub(super) fn show_start(
        &mut self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_plugin_start();
        self.start_on_load = None;
        self.show_work(Shown::Start(project.clone()), window, cx);
        self.sidebar
            .update(cx, |sidebar, _| sidebar.choose(Some(project.clone())));
        self.start.reset();
        self.load_branches(project, cx);
        self.picker.update(cx, |picker, cx| picker.load(window, cx));
        self.picker
            .update(cx, |picker, cx| picker.focus(window, cx));
        self.tell_host(window, cx);
        cx.notify();
    }

    pub(super) fn run(&self, op: Op, action: &'static str, cx: &mut Context<Self>) {
        self.run_then(op, cx, move |this, result, window, cx| {
            this.finish(result, action, window, cx);
        });
    }

    pub(super) fn run_then(
        &self,
        op: Op,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Result<OpResult, OpError>, &mut Window, &mut Context<Self>)
        + 'static,
    ) {
        let task = Workspace::dispatch(op, cx);
        let window = self.window;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| then(this, result, window, cx));
            });
        })
        .detach();
    }

    pub(super) fn finish(
        &mut self,
        result: Result<OpResult, OpError>,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if result.is_ok() {
            self.note = None;
        }
        match result {
            Ok(OpResult::Thread(thread)) => self.select(thread.id, window, cx),
            Ok(_) => self.refresh(window, cx),
            Err(error) => {
                self.note = Some(format!("Could not {action}: {error}").into());
                cx.notify();
            }
        }
    }

    pub(super) fn new_thread(
        &mut self,
        _: &NewThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self
            .sidebar
            .read(cx)
            .chosen()
            .cloned()
            .or_else(|| {
                self.shown_thread(cx)
                    .and_then(|id| self.record(&id, cx).map(|thread| thread.project_id.clone()))
            })
            .or_else(|| {
                self.sidebar
                    .read(cx)
                    .listing
                    .projects
                    .first()
                    .map(|project| project.id.clone())
            });
        if let Some(project) = project {
            self.show_start(project, window, cx);
        }
    }

    pub(super) fn stop(&mut self, _: &StopThread, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.shown_thread(cx) {
            self.run(Op::Thread(ThreadOp::Stop { id }), "stop the thread", cx);
        }
    }

    pub(super) fn save_diagnostics(
        &mut self,
        _: &SaveDiagnostics,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_then(
            kg_domain::Op::Diagnostics(kg_domain::DiagnosticsOp::Export { path: None }),
            cx,
            |this, result, _, cx| {
                match result {
                    Ok(kg_domain::OpResult::Diagnostics { path }) => {
                        this.note = None;
                        this.show_toast(
                            Toast {
                                message: format!("Saved diagnostics to {}", path.display()).into(),
                                action: None,
                                undo: Vec::new(),
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.note = Some(format!("Could not save diagnostics: {error}").into());
                    }
                    Ok(_) => {}
                }
                cx.notify();
            },
        );
    }

    pub(super) fn start_again(&mut self, _: &StartAgain, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.shown_thread(cx) {
            self.run(
                Op::Thread(ThreadOp::StartAgain { id }),
                "start the thread again",
                cx,
            );
        }
    }

    pub(super) fn toggle_pinned(
        &mut self,
        _: &TogglePinned,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self
            .shown_thread(cx)
            .and_then(|id| self.record(&id, cx).cloned())
        else {
            return;
        };
        let pin = ThreadOp::SetPinned {
            id: thread.id,
            pinned: !thread.pinned,
        };
        self.run(Op::Thread(pin), "change the pin", cx);
    }

    pub(crate) fn restore(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.shown_thread(cx) {
            self.run(
                Op::Thread(ThreadOp::Unarchive { id }),
                "restore the thread",
                cx,
            );
        }
    }

    pub(super) fn restore_thread(
        &mut self,
        _: &RestoreThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore(window, cx);
    }

    pub(super) fn archive(
        &mut self,
        _: &ArchiveThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self
            .shown_thread(cx)
            .and_then(|id| self.record(&id, cx).cloned())
        else {
            return;
        };
        let confirm = Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: kg_domain::prefs::CONFIRM_ARCHIVE.into(),
            }),
            cx,
        );
        let text = description(
            thread.status,
            archive_span(&self.sidebar.read(cx).listing.threads, &thread.id),
        );
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let ask = matches!(
                confirm.await,
                Ok(OpResult::Setting(serde_json::Value::Bool(true)))
            );
            _ = cx.update_window(handle, |_, window, cx| {
                if ask {
                    confirm_archive(this, thread, text, window, cx);
                } else {
                    _ = this.update(cx, |this, cx| this.archive_now(&thread, window, cx));
                }
            });
        })
        .detach();
    }

    pub(super) fn archive_now(
        &mut self,
        thread: &Thread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = thread.id.clone();
        let leaving = archived_with(&Workspace::global(cx).read(cx).thread_records(cx), &id);
        let next = self.next_thread(&id, &leaving, cx);
        self.dismiss_thread_menus(cx);
        self.close_rename();
        if let Some(next) = next {
            self.select(next, window, cx);
        } else {
            self.cancel_plugin_start();
            self.show_work(Shown::Nothing, window, cx);
            window.focus(&self.focus, cx);
        }
        self.show_toast(
            Toast {
                message: format!("Archived “{}”", thread.title).into(),
                action: None,
                undo: leaving,
            },
            cx,
        );
        self.run(
            Op::Thread(ThreadOp::Archive { id }),
            "archive the thread",
            cx,
        );
    }

    pub(super) fn undo_archive(
        &mut self,
        ids: Vec<ThreadId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toast = None;
        if let Some(id) = ids.first() {
            self.select(id.clone(), window, cx);
        }
        for id in ids {
            self.run(
                Op::Thread(ThreadOp::Unarchive { id }),
                "restore the thread",
                cx,
            );
        }
    }

    pub(super) fn next_thread(
        &self,
        id: &ThreadId,
        leaving: &[ThreadId],
        cx: &App,
    ) -> Option<ThreadId> {
        let leaving: BTreeSet<&ThreadId> = leaving.iter().collect();
        let rows = self.sidebar.read(cx).thread_ids(&self.shows(cx), cx);
        let stays = |row: &ThreadId| !leaving.contains(row);
        let neighbor = rows.iter().position(|row| row == id).and_then(|index| {
            rows.iter()
                .skip(index + 1)
                .find(|row| stays(row))
                .or_else(|| rows.iter().take(index).rev().find(|row| stays(row)))
                .cloned()
        });
        neighbor.or_else(|| {
            self.listed(cx)
                .find(|thread| stays(&thread.id))
                .map(|thread| thread.id.clone())
        })
    }

    pub(super) fn pick_project(window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add project".into()),
        });
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |_, cx| Self::add_picked_project(path, window, cx));
            });
        })
        .detach();
    }
}
// Unarchive fails while the owner is still archived, so the owner is listed first.
pub(super) fn archived_with(threads: &[Thread], id: &ThreadId) -> Vec<ThreadId> {
    let mut ids = vec![id.clone()];
    for owned in kg_domain::owned_thread_ids(threads, id).into_iter().skip(1) {
        if threads
            .iter()
            .find(|thread| thread.id == owned)
            .is_some_and(|thread| thread.archived_at.is_none())
        {
            ids.push(owned);
        }
    }
    ids
}

fn confirm_archive(
    kagan: gpui_kit::WeakEntity<Kagan>,
    thread: Thread,
    text: String,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_alert_dialog(cx, move |dialog, _, _| {
        let kagan = kagan.clone();
        let archived = thread.clone();
        dialog
            .title(format!("Archive “{}”?", thread.title))
            .description(text.clone())
            .ok_text("Archive")
            .on_ok(move |_, window, cx| {
                _ = kagan.update(cx, |kagan, cx| {
                    kagan.archive_now(&archived, window, cx);
                });
                true
            })
    });
}

pub(crate) fn description(status: ThreadStatus, extra: usize) -> String {
    let mut sentences = Vec::new();
    if status.running() {
        sentences.push("This will stop current work.".to_owned());
    }
    sentences.extend(match extra {
        0 => None,
        1 => Some("1 thread that lives with this one will be archived with it.".to_owned()),
        _ => Some(format!(
            "{extra} threads that live with this one will be archived with it."
        )),
    });
    sentences.push("It leaves the sidebar. Restore it from Archived.".to_owned());
    sentences.join(" ")
}
