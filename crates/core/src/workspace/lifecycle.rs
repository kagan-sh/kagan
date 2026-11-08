use std::path::Path;
use std::time::Duration;

use gpui_kit::{App, Task};
use kg_domain::{Op, OpError, ThreadStatus};

use crate::host::database::Result;

use super::{PluginDispatch, QUIT_DEADLINE, Workspace};

impl Workspace {
    pub fn set_plugin_host(&mut self, dispatch: PluginDispatch) {
        self.plugins = Some(dispatch);
    }
    pub fn set_ui_driver(&mut self, dispatch: crate::dispatch::ui::UiDispatch) {
        self.ui = Some(dispatch);
    }
    pub(crate) fn end_programs(&mut self) {
        for run in self.runs.values_mut() {
            run.extras = crate::thread::Extras::default();
            run.shown = None;
        }
    }
    pub fn stop_for_quit(cx: &mut App) -> Task<Result<()>> {
        let workspace = Self::global(cx);
        let resume: Vec<_> = workspace
            .read(cx)
            .threads
            .iter()
            .filter(|(_, thread)| {
                matches!(
                    thread.read(cx).record.status,
                    ThreadStatus::Active | ThreadStatus::Starting
                )
            })
            .map(|(id, _)| id.clone())
            .collect();
        let ids: Vec<_> = workspace
            .read(cx)
            .threads
            .iter()
            .filter(|(_, thread)| thread.read(cx).running())
            .map(|(id, _)| id.clone())
            .collect();
        let database = workspace.read(cx).database.clone();
        cx.spawn(async move |cx| {
            let deadline = cx.background_executor().timer(QUIT_DEADLINE);
            let work = async {
                if !resume.is_empty() {
                    database
                        .call(move |store| store.remember_resume(&resume))
                        .await?;
                }
                let stops: Vec<_> = cx.update(|cx| {
                    workspace.update(cx, |workspace, _| {
                        for id in &ids {
                            workspace.run_mut(id).stopping = Some(crate::thread::StopReason::Quit);
                        }
                    });
                    ids.iter()
                        .map(|id| {
                            Self::dispatch(
                                Op::Thread(kg_domain::ThreadOp::Stop { id: id.clone() }),
                                cx,
                            )
                        })
                        .collect::<Vec<_>>()
                });
                let mut stopped = Ok(());
                for stop in stops {
                    if let Err(error) = stop.await {
                        stopped = Err(error);
                    }
                }
                loop {
                    let done = workspace.read_with(cx, |workspace, cx| {
                        ids.iter().all(|id| {
                            let running = workspace
                                .threads
                                .get(id)
                                .is_some_and(|thread| thread.read(cx).running());
                            !running
                                && workspace
                                    .runs
                                    .get(id)
                                    .is_none_or(|run| run.unsaved().is_none())
                        })
                    });
                    if done {
                        return stopped;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                }
            };
            futures::pin_mut!(work, deadline);
            match futures::future::select(work, deadline).await {
                futures::future::Either::Left((result, _)) => result,
                futures::future::Either::Right(_) => {
                    Err(OpError::Failed("timed out stopping programs".into()))
                }
            }
        })
    }
    #[cfg(any(test, feature = "test"))]
    pub fn hold_screen_save(&mut self, hold: async_channel::Receiver<()>) {
        self.holds.insert(crate::workspace::Hold::Screen, hold);
    }
    #[must_use]
    pub fn running_programs(&self, cx: &App) -> usize {
        self.threads
            .values()
            .filter(|thread| thread.read(cx).running())
            .count()
    }

    /// Counts threads whose program title says it is busy with a reply.
    #[must_use]
    pub fn busy_threads(&self, cx: &App) -> usize {
        self.threads
            .values()
            .filter(|thread| thread.read(cx).busy())
            .count()
    }
    #[cfg(any(test, feature = "test"))]
    #[must_use]
    pub fn terminal_process_id(&self, id: &kg_domain::ThreadId) -> Option<u32> {
        match self.shown(id)? {
            crate::thread::Shown::Running(terminal) => Some(terminal.process_id()),
            crate::thread::Shown::Ended(_) => None,
        }
    }
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
