use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{AppContext, AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, OpError, OpResult, SelectMode, ThreadId, ThreadOp, ThreadStatus};
use kg_terminal::{Finished, Launch, Terminal, View};

use crate::host::database::Result;
use crate::workspace::{closed, missing};
use crate::{Thread, Workspace, WorkspaceEvent};

use super::agent::{self, Start};
use super::extra::Extras;
use super::halt::status;
use super::resolve_program;
use crate::end::{End, Loss};

mod exit;
mod screen;
mod turn;

pub(crate) use exit::StopReason;
pub(crate) use exit::{exited, request_stop, start_again};
pub(crate) use screen::follow_terminals;
#[cfg(test)]
pub(crate) use screen::remember;
pub(crate) use turn::hook as turn_hook;

#[cfg(test)]
pub(super) use turn::{Spin, busy, follow};

pub(super) const COLS: u16 = 80;
pub(super) const ROWS: u16 = 24;
const TICK: Duration = Duration::from_millis(33);

pub(crate) async fn start(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<()> {
    let record = thread.read_with(cx, |thread, _| thread.record.clone());
    let folder = this
        .update(cx, |workspace, cx| {
            record
                .environment_id
                .as_ref()
                .and_then(|id| workspace.environments.get(id))
                .map(|environment| environment.read(cx).record.path.clone())
        })
        .map_err(closed)?;
    let size = this
        .update(cx, |workspace, _| {
            workspace
                .runs
                .get(&record.id)
                .and_then(|run| run.size)
                .unwrap_or((COLS, ROWS))
        })
        .map_err(closed)?;
    crate::workspace::hold(this, crate::workspace::Hold::Start, cx).await;
    // Taken before the program starts, so what it begins in that moment counts as after the start.
    let at = crate::workspace::time(this, cx)?;
    let spawned = match folder {
        Some(folder) => spawn(this, &record, folder, size, start, cx).await,
        None => Err(OpError::Invalid("the thread has no folder".into())),
    };
    match spawned {
        Ok(terminal) => {
            this.update(cx, |workspace, cx| {
                workspace.run_mut(&record.id).shown = Some(Shown::Running(terminal));
                follow_terminals(workspace, cx);
            })
            .map_err(closed)?;
            Workspace::events_at(this, record.id, at, vec![EventKind::Started], cx).await?;
            status(this, thread, ThreadStatus::Active, cx).await
        }
        Err(error) => {
            let failed = vec![EventKind::Failed {
                error: Some(error.to_string()),
            }];
            Workspace::events(this, record.id, failed, cx).await?;
            status(this, thread, ThreadStatus::Error, cx).await?;
            Err(error)
        }
    }
}

async fn spawn(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    folder: PathBuf,
    (cols, rows): (u16, u16),
    start: Start,
    cx: &mut AsyncApp,
) -> Result<Terminal> {
    let environment = crate::host::program::program_environment_of(this, cx);
    let program = resolve_program(&record.provider, environment.as_ref())
        .map_err(|error| OpError::Invalid(error.to_string()))?;
    let integration = agent::integrate(this, record, &program, &folder, start, cx).await?;
    this.update(cx, |workspace, cx| {
        turn::begin_run(workspace, &record.id, integration.hooks, cx);
    })
    .map_err(closed)?;
    let args = integration.args.into_iter().chain(program.args).collect();
    let mut env = integration.env;
    if let Some(locale) = cx.background_spawn(async { utf8_locale() }).await {
        env.push(("LANG".into(), locale.into()));
    }
    let launch = Launch {
        program: program.path.clone().into(),
        args,
        cwd: Some(folder.clone()),
        env,
    };
    Terminal::spawn(launch, cols, rows)
        .map_err(|error| OpError::Failed(start_failure(&program.path, &folder, &error)))
}

fn start_failure(program: &Path, folder: &Path, error: &std::io::Error) -> String {
    format!(
        "could not start {} in {}: {error}",
        program.display(),
        folder.display()
    )
}

#[cfg(not(unix))]
pub(super) fn utf8_locale() -> Option<String> {
    None
}

#[cfg(unix)]
pub(super) fn utf8_locale() -> Option<String> {
    static INSTALLED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .any(|key| std::env::var(key).is_ok_and(|value| is_utf8(&value)))
    {
        return None;
    }
    INSTALLED.get_or_init(installed_utf8_locale).clone()
}

#[cfg(unix)]
fn installed_utf8_locale() -> Option<String> {
    let output = std::process::Command::new("locale")
        .arg("-a")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let locales = String::from_utf8_lossy(&output.stdout);
    let preferred = if cfg!(target_os = "macos") {
        "en_US"
    } else {
        "C"
    };
    locales
        .lines()
        .find(|locale| {
            locale
                .split('.')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(preferred))
                && is_utf8(locale)
        })
        .or_else(|| locales.lines().find(|locale| is_utf8(locale)))
        .map(str::to_owned)
}

#[cfg(unix)]
fn is_utf8(locale: &str) -> bool {
    locale
        .to_ascii_lowercase()
        .replace('-', "")
        .contains("utf8")
}

pub(crate) async fn submit(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    text: &str,
    cx: &mut AsyncApp,
) -> Result<String> {
    if text.is_empty() {
        return Err(OpError::Invalid("send needs text".into()));
    }
    if text.len() > kg_domain::TEXT_MAX {
        return Err(OpError::Invalid(format!(
            "send is {} bytes; the cap is {} MiB",
            text.len(),
            kg_domain::TEXT_MAX >> 20
        )));
    }
    let (typed, written) = this
        .update(cx, |workspace, _| match workspace.shown(id) {
            Some(Shown::Running(terminal)) => terminal
                .submit(text)
                .map_err(|error| OpError::Invalid(error.to_string())),
            Some(Shown::Ended(_)) => Err(OpError::Invalid("the program has ended".into())),
            None => Err(missing("terminal", id)),
        })
        .map_err(closed)??;
    // A blocking recv would pin a background worker and starve Stop and the DB.
    // Park on the executor timer; the writer must not wake GPUI tasks (the test scheduler forbids it).
    let executor = cx.background_executor().clone();
    cx.background_spawn(async move {
        loop {
            match written.try_recv() {
                Ok(result) => break result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    executor.timer(Duration::from_millis(2)).await;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    break Err(std::io::Error::other("terminal is closed"));
                }
            }
        }
    })
    .await
    .map_err(|error| OpError::Invalid(error.to_string()))?;
    Ok(typed)
}

pub(crate) async fn send(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    text: &str,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let typed = submit(this, &id, text, cx).await?;
    this.update(cx, |workspace, cx| {
        let now = cx.background_executor().now();
        turn::note_submit(workspace, &id, now, cx);
    })
    .map_err(closed)?;
    Workspace::events(
        this,
        id,
        vec![EventKind::Message {
            role: kg_domain::Role::User,
            text: typed,
            message_id: None,
            attachments: Vec::new(),
        }],
        cx,
    )
    .await?;
    Ok(OpResult::Done)
}

pub(crate) fn run(
    this: &WeakEntity<Workspace>,
    op: ThreadOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let (ThreadOp::Input { id, terminal, .. }
    | ThreadOp::Resize { id, terminal, .. }
    | ThreadOp::Screen { id, terminal, .. }
    | ThreadOp::Scroll { id, terminal, .. }
    | ThreadOp::Select { id, terminal, .. }
    | ThreadOp::Copy { id, terminal }
    | ThreadOp::Paste { id, terminal, .. }
    | ThreadOp::Report { id, terminal, .. }) = &op
    else {
        return Err(OpError::Invalid("not a terminal operation".into()));
    };
    let (id, number) = (id.clone(), *terminal);
    // Scrolling and selecting move the view, so the window redraws even when nothing else changed.
    let moves_view = matches!(op, ThreadOp::Scroll { .. } | ThreadOp::Select { .. });
    this.update(cx, |workspace, cx| {
        if number.is_none()
            && let ThreadOp::Resize { cols, rows, .. } = &op
        {
            workspace.run_mut(&id).size = Some((*cols, *rows));
        }
        if number.is_none() {
            let now = cx.background_executor().now();
            match &op {
                ThreadOp::Input { text, .. } => {
                    super::notify::typed(workspace, &id, text, now);
                    if text.ends_with('\r') {
                        turn::note_submit(workspace, &id, now, cx);
                    }
                }
                ThreadOp::Paste { .. } => super::notify::typed(workspace, &id, "", now),
                _ => {}
            }
        }
        let result = if let Some(number) = number {
            super::extra::find(workspace, &id, number)
                .ok_or_else(|| missing("terminal", &number))
                .map(|terminal| act(Some(terminal), terminal.view(), op))
        } else {
            workspace
                .shown(&id)
                .ok_or_else(|| missing("terminal", &id))
                .map(|shown| act(shown.running(), shown.view(), op))
        }?
        .map_err(|error| OpError::Invalid(error.to_string()));
        if moves_view {
            cx.emit(WorkspaceEvent::Screen(id.clone(), number));
        }
        result
    })
    .map_err(closed)?
}

/// Everything the workspace keeps for one thread's program. Forgetting the thread is one `remove`.
#[derive(Default)]
pub(crate) struct Run {
    pub(crate) shown: Option<Shown>,
    pub(crate) extras: Extras,
    pub(crate) size: Option<(u16, u16)>,
    pub(crate) started_seq: Option<u64>,
    /// Whether the run's settled screen was already offered to the store.
    pub(crate) saved: bool,
    pub(crate) end: Option<End>,
    pub(crate) loss: Option<Loss>,
    /// The program was asked to stop, and why. Quit skips `turn.ended` only.
    pub(crate) stopping: Option<StopReason>,
    /// Its exit is queued for the request loop, so the watcher reports each once.
    pub(crate) exiting: bool,
    /// Lines for this thread's parent wait until this instant, so they do not land mid-keystroke.
    pub(crate) held_until: Option<std::time::Instant>,
    /// Lines queued for this thread, by child, to type into it once it is idle.
    pub(crate) notes: std::collections::BTreeMap<ThreadId, String>,
}

impl Run {
    pub(crate) fn begin(&mut self, seq: u64) {
        self.started_seq = Some(seq);
        self.saved = false;
        self.loss = None;
        self.end = None;
    }

    pub(crate) fn unsaved(&self) -> Option<u64> {
        if matches!(self.shown, Some(Shown::Ended(_))) && !self.saved {
            self.started_seq
        } else {
            None
        }
    }

    /// Past the archive grace: drops the screen an ended program left and ends the extra terminals,
    /// keeping their numbers so none is reused. Returns whether any extra terminal ended.
    pub(crate) fn release(&mut self) -> bool {
        if matches!(self.shown, Some(Shown::Ended(_))) {
            self.shown = None;
            self.loss = None;
        }
        !std::mem::take(&mut self.extras.open).is_empty()
    }
}

/// A thread's terminal: the program while it runs, or the screen it left when it ended.
pub(crate) enum Shown {
    Running(Terminal),
    Ended(Finished),
}

impl Shown {
    pub(crate) fn view(&self) -> &View {
        match self {
            Self::Running(terminal) => terminal.view(),
            Self::Ended(finished) => finished.view(),
        }
    }

    fn running(&self) -> Option<&Terminal> {
        match self {
            Self::Running(terminal) => Some(terminal),
            Self::Ended(_) => None,
        }
    }
}

// An ended program has no pty: input is refused, and the view and the text still work.
fn act(running: Option<&Terminal>, view: &View, op: ThreadOp) -> std::io::Result<OpResult> {
    match (op, running) {
        (ThreadOp::Input { text, .. }, Some(terminal)) => {
            terminal.write(text.into_bytes()).map(|()| OpResult::Done)
        }
        (ThreadOp::Paste { text, .. }, Some(terminal)) => terminal
            .paste(text.as_deref().unwrap_or(""))
            .map(|()| OpResult::Done),
        (ThreadOp::Input { .. } | ThreadOp::Paste { .. }, None) => {
            Err(std::io::Error::other("the program has ended"))
        }
        (ThreadOp::Resize { cols, rows, .. }, Some(terminal)) => {
            terminal.resize(cols, rows).map(|()| OpResult::Done)
        }
        (ThreadOp::Report { bytes, .. }, Some(terminal)) => {
            terminal.report(bytes).map(|()| OpResult::Done)
        }
        (ThreadOp::Resize { .. } | ThreadOp::Report { .. }, None) => Ok(OpResult::Done),
        (ThreadOp::Scroll { lines, .. }, _) => {
            view.scroll(lines);
            Ok(OpResult::Done)
        }
        (
            ThreadOp::Select {
                mode,
                column,
                row,
                right,
                ..
            },
            _,
        ) => {
            view.select(terminal_mode(mode), column, row, right);
            Ok(OpResult::Done)
        }
        (ThreadOp::Copy { .. }, _) => Ok(view.selection().map_or(OpResult::Done, OpResult::Text)),
        (
            ThreadOp::Screen {
                scrollback, lines, ..
            },
            _,
        ) => screen_result(scrollback, lines, || view.transcript(), || view.screen()),
        _ => Err(std::io::Error::other("not a terminal operation")),
    }
}

fn screen_result(
    scrollback: bool,
    lines: Option<u32>,
    transcript: impl FnOnce() -> String,
    view: impl FnOnce() -> String,
) -> std::io::Result<OpResult> {
    if let Some(count) = lines {
        if !scrollback {
            return Err(std::io::Error::other("--lines needs --scrollback"));
        }
        if !(1..=10_000).contains(&count) {
            return Err(std::io::Error::other("--lines must be from 1 to 10000"));
        }
        return Ok(OpResult::Screen(last_rows(&transcript(), count)));
    }
    Ok(OpResult::Screen(if scrollback {
        transcript()
    } else {
        view()
    }))
}

fn last_rows(transcript: &str, count: u32) -> String {
    let rows: Vec<&str> = transcript.lines().collect();
    let count = count as usize;
    if rows.len() <= count {
        return transcript.to_owned();
    }
    rows[rows.len() - count..].join("\n")
}

fn terminal_mode(mode: SelectMode) -> kg_terminal::SelectMode {
    match mode {
        SelectMode::Simple => kg_terminal::SelectMode::Simple,
        SelectMode::Word => kg_terminal::SelectMode::Word,
        SelectMode::Line => kg_terminal::SelectMode::Line,
        SelectMode::Block => kg_terminal::SelectMode::Block,
        SelectMode::Extend => kg_terminal::SelectMode::Extend,
    }
}

impl Workspace {
    pub(crate) fn run_mut(&mut self, id: &ThreadId) -> &mut Run {
        self.runs.entry(id.clone()).or_default()
    }

    pub(crate) fn shown(&self, id: &ThreadId) -> Option<&Shown> {
        self.runs.get(id)?.shown.as_ref()
    }

    /// A terminal's screen as cells, for a window in this process: the program's, or an extra one by number.
    #[must_use]
    pub fn grid(&self, id: &ThreadId, terminal: Option<u32>) -> Option<kg_terminal::Grid> {
        if let Some(number) = terminal {
            return super::extra::find(self, id, number).map(|terminal| terminal.view().grid());
        }
        self.shown(id).map(|shown| shown.view().grid())
    }
}

pub(crate) fn kill(workspace: &mut Workspace, id: &ThreadId) {
    workspace.runs.remove(id);
}

#[cfg(test)]
mod tests;
