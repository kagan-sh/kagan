use std::ffi::OsString;

use gpui_kit::{AppContext as _, AsyncApp, Context, WeakEntity};
use kg_domain::{EnvironmentStatus, ExtraTerminal, OpError, OpResult, TerminalOp, ThreadId};
use kg_terminal::{Launch, Lines, Terminal};

use crate::database::Result;
use crate::workspace::{closed, missing};
use crate::{Workspace, WorkspaceEvent};

/// The shells a person opened beside one thread's program. `last` only grows, so a number is never reused.
#[derive(Default)]
pub(crate) struct Extras {
    open: Vec<(u32, Terminal)>,
    last: u32,
}

pub(super) async fn run(
    this: &WeakEntity<Workspace>,
    op: TerminalOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        TerminalOp::New { id } => open(this, id, cx).await,
        TerminalOp::List { id } => this
            .update(cx, |workspace, _| {
                workspace
                    .threads
                    .contains_key(&id)
                    .then(|| OpResult::Terminals(workspace.extras(&id)))
                    .ok_or_else(|| missing("thread", &id))
            })
            .map_err(closed)?,
        TerminalOp::Close { id, number } => this
            .update(cx, |workspace, cx| {
                let open = &mut workspace
                    .extras
                    .get_mut(&id)
                    .ok_or_else(|| missing("terminal", &number))?
                    .open;
                let index = open
                    .iter()
                    .position(|(n, _)| *n == number)
                    .ok_or_else(|| missing("terminal", &number))?;
                open.remove(index);
                cx.emit(WorkspaceEvent::Changed);
                Ok(OpResult::Done)
            })
            .map_err(closed)?,
    }
}

async fn open(this: &WeakEntity<Workspace>, id: ThreadId, cx: &mut AsyncApp) -> Result<OpResult> {
    let thread = super::thread(this, &id, cx)?;
    let record = thread.read_with(cx, |thread, _| thread.record.clone());
    if record.archived_at.is_some() {
        return Err(OpError::Invalid(
            "restore the thread before opening a terminal".into(),
        ));
    }
    let (folder, size) = this
        .update(cx, |workspace, cx| {
            let folder = record
                .environment_id
                .as_ref()
                .and_then(|id| workspace.environments.get(id))
                .map(|environment| environment.read(cx).record.clone())
                .filter(|environment| environment.status == EnvironmentStatus::Ready)
                .map(|environment| environment.path);
            (folder, workspace.sizes.get(&id).copied())
        })
        .map_err(closed)?;
    let folder = folder.ok_or_else(|| OpError::Invalid("its folder is not ready".into()))?;
    let shell = crate::program::login_shell(crate::program::environment_of(this, cx).as_ref());
    let mut env = super::agent::variables(this, &record, cx)?;
    if let Some(locale) = cx
        .background_spawn(async { super::terminal::utf8_locale() })
        .await
    {
        env.push(("LANG".into(), locale.into()));
    }
    let args: Vec<OsString> = if cfg!(windows) {
        Vec::new()
    } else {
        vec!["-l".into()]
    };
    let (cols, rows) = size.unwrap_or((super::terminal::COLS, super::terminal::ROWS));
    let terminal = Terminal::spawn(
        Launch {
            program: shell,
            args,
            cwd: Some(folder),
            env,
        },
        cols,
        rows,
    )
    .map_err(|error| OpError::Failed(error.to_string()))?;
    this.update(cx, |workspace, cx| {
        let extras = workspace.extras.entry(id).or_default();
        extras.last += 1;
        let number = extras.last;
        extras.open.push((number, terminal));
        super::terminal::follow_terminals(workspace, cx);
        cx.emit(WorkspaceEvent::Changed);
        OpResult::Terminals(vec![ExtraTerminal {
            number,
            title: None,
        }])
    })
    .map_err(closed)
}

impl Workspace {
    /// The thread's extra terminals, oldest first.
    #[must_use]
    pub fn extras(&self, id: &ThreadId) -> Vec<ExtraTerminal> {
        self.extras
            .get(id)
            .into_iter()
            .flat_map(|extras| &extras.open)
            .map(|(number, terminal)| ExtraTerminal {
                number: *number,
                title: terminal.title(),
            })
            .collect()
    }
}

pub(super) fn find<'a>(
    workspace: &'a Workspace,
    id: &ThreadId,
    number: u32,
) -> Option<&'a Terminal> {
    workspace
        .extras
        .get(id)?
        .open
        .iter()
        .find_map(|(n, terminal)| (*n == number).then_some(terminal))
}

/// Every link the thread's terminals printed, the program's first, each once at its last place.
pub(super) async fn links(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let terminals = this
        .update(cx, |workspace, _| {
            if !workspace.threads.contains_key(&id) {
                return Err(missing("thread", &id));
            }
            let program = workspace.terminals.get(&id).map(|shown| match shown {
                super::Shown::Running(terminal) => terminal.lines(),
                super::Shown::Ended(finished) => finished.lines(),
            });
            let extras = workspace
                .extras
                .get(&id)
                .into_iter()
                .flat_map(|extras| extras.open.iter().map(|(_, terminal)| terminal.lines()));
            Ok(program.into_iter().chain(extras).collect::<Vec<Lines>>())
        })
        .map_err(closed)??;
    let found = cx
        .background_spawn(async move {
            let mut all: Vec<String> = Vec::new();
            for kept in terminals.iter().map(Lines::kept) {
                for line in &kept.lines {
                    all.extend(
                        kg_terminal::links(&line.text)
                            .into_iter()
                            .map(|range| line.text[range].to_owned()),
                    );
                }
            }
            // Newest first keeps each link at its last place; the order is turned back after.
            let mut seen = std::collections::HashSet::new();
            let mut found: Vec<String> = all
                .into_iter()
                .rev()
                .filter(|link| seen.insert(link.clone()))
                .collect();
            found.reverse();
            found
        })
        .await;
    Ok(OpResult::Links(found))
}

/// Ends every extra terminal of a thread and forgets its numbers. Dropping a terminal ends its
/// program. Returns whether the thread had any.
pub(crate) fn end(workspace: &mut Workspace, id: &ThreadId) -> bool {
    workspace.extras.remove(id).is_some()
}

/// Sends their screen changes and closes the ones whose shell exited. Returns whether any are open.
pub(super) fn poll(workspace: &mut Workspace, cx: &mut Context<Workspace>) -> bool {
    let mut closed = false;
    for (id, extras) in &mut workspace.extras {
        extras.open.retain(|(number, terminal)| {
            if terminal.take_changed() {
                cx.emit(WorkspaceEvent::Screen(id.clone(), Some(*number)));
            }
            let open = terminal.exit().is_none();
            closed |= !open;
            open
        });
    }
    if closed {
        cx.emit(WorkspaceEvent::Changed);
    }
    workspace
        .extras
        .values()
        .any(|extras| !extras.open.is_empty())
}
