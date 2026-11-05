use std::path::Path;
use std::time::Duration;

use gpui_kit::{App, Task};
use kg_domain::{Op, OpError};

use crate::host::database::Result;

use super::{PluginDispatch, QUIT_DEADLINE, Workspace};

impl Workspace {
    pub fn set_plugin_host(&mut self, dispatch: PluginDispatch) {
        self.plugins = Some(dispatch);
    }
    pub fn set_ui_driver(&mut self, dispatch: crate::dispatch::ui::UiDispatch) {
        self.ui = Some(dispatch);
    }
    pub fn end_programs(&mut self) {
        self.extras.clear();
        self.terminals.clear();
    }
    pub fn stop_for_quit(cx: &mut App) -> Task<Result<()>> {
        let workspace = Self::global(cx);
        let ids: Vec<_> = workspace
            .read(cx)
            .threads
            .iter()
            .filter(|(_, thread)| thread.read(cx).running())
            .map(|(id, _)| id.clone())
            .collect();
        workspace.update(cx, |workspace, _| {
            for id in &ids {
                workspace
                    .stopping
                    .insert(id.clone(), crate::thread::StopReason::Quit);
            }
        });
        let stops: Vec<_> = ids
            .iter()
            .map(|id| Self::dispatch(Op::Thread(kg_domain::ThreadOp::Stop { id: id.clone() }), cx))
            .collect();
        cx.spawn(async move |cx| {
            let deadline = cx.background_executor().timer(QUIT_DEADLINE);
            let work = async {
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
                            let saving = matches!(
                                workspace.terminals.get(id),
                                Some(crate::thread::Shown::Ended(_))
                            ) && workspace
                                .runs
                                .get(id)
                                .is_some_and(|seq| !workspace.saved.contains(&(id.clone(), *seq)));
                            !running && !saving
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
        self.screen_hold = Some(hold);
    }
    #[must_use]
    pub fn running_programs(&self, cx: &App) -> usize {
        self.threads
            .values()
            .filter(|thread| thread.read(cx).running())
            .count()
    }

    /// Threads whose program's title says it is busy with a reply (CONTEXT).
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
        match self.terminals.get(id)? {
            crate::thread::Shown::Running(terminal) => Some(terminal.process_id()),
            crate::thread::Shown::Ended(_) => None,
        }
    }
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
