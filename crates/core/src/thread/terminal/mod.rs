use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{AppContext, AsyncApp, Entity, WeakEntity};
use kg_domain::{EventKind, OpError, OpResult, SelectMode, ThreadId, ThreadOp, ThreadStatus};
use kg_terminal::{Finished, Launch, Terminal};

use crate::database::Result;
use crate::workspace::{closed, missing};
use crate::{Thread, Workspace, WorkspaceEvent};

use super::agent::{self, Start};
use super::halt::status;
use super::resolve_program;

mod exit;
mod screen;
mod turn;

pub(crate) use exit::StopReason;
pub(crate) use exit::{exited, request_stop, start_again};
pub(crate) use screen::follow_terminals;
#[cfg(test)]
pub(crate) use screen::remember;

#[cfg(test)]
pub(super) use turn::{Spin, busy, follow};

pub(super) const COLS: u16 = 80;
pub(super) const ROWS: u16 = 24;
const TICK: Duration = Duration::from_millis(33);
// A program can drop its spinner for a frame in the middle of a reply. Waiting counts only after this quiet.
const QUIET: Duration = Duration::from_secs(1);
pub(crate) const ARM: Duration = Duration::from_secs(2);

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
                .sizes
                .get(&record.id)
                .copied()
                .unwrap_or((COLS, ROWS))
        })
        .map_err(closed)?;
    // Taken before the program starts, so what it begins in that moment counts as after the start.
    let at = crate::workspace::time(this, cx)?;
    let spawned = match folder {
        Some(folder) => spawn(this, &record, folder, size, start, cx).await,
        None => Err(OpError::Invalid("the thread has no folder".into())),
    };
    match spawned {
        Ok(terminal) => {
            this.update(cx, |workspace, cx| {
                workspace
                    .terminals
                    .insert(record.id.clone(), Shown::Running(terminal));
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
    let environment = crate::program::program_environment_of(this, cx);
    let program =
        resolve_program(&record.provider, environment.as_ref()).map_err(OpError::Invalid)?;
    let integration = agent::integrate(this, record, &program, &folder, start, cx).await?;
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

const TEXT_CAP: usize = 1024 * 1024;

pub(crate) async fn submit(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    text: &str,
    cx: &mut AsyncApp,
) -> Result<String> {
    if text.is_empty() {
        return Err(OpError::Invalid("send needs text".into()));
    }
    if text.len() > TEXT_CAP {
        return Err(OpError::Invalid(format!(
            "send is {} bytes; the cap is 1 MiB",
            text.len()
        )));
    }
    let (typed, written) = this
        .update(cx, |workspace, _| match workspace.terminals.get(id) {
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

pub(crate) fn run(
    this: &WeakEntity<Workspace>,
    op: ThreadOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if let ThreadOp::Paste { text, file, .. } = &op {
        if file.is_some() {
            return Err(OpError::Invalid("paste carries text, not a path".into()));
        }
        let Some(text) = text else {
            return Err(OpError::Invalid("paste needs text".into()));
        };
        if text.len() > TEXT_CAP {
            return Err(OpError::Invalid(format!(
                "paste is {} bytes; the cap is 1 MiB",
                text.len()
            )));
        }
    }
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
            workspace.sizes.insert(id.clone(), (*cols, *rows));
        }
        let result = if let Some(number) = number {
            super::extra::find(workspace, &id, number)
                .ok_or_else(|| missing("terminal", &number))
                .map(|terminal| act_running(terminal, op))
        } else {
            workspace
                .terminals
                .get(&id)
                .ok_or_else(|| missing("terminal", &id))
                .map(|shown| act(shown, op))
        }?
        .map_err(|error| OpError::Invalid(error.to_string()));
        if moves_view {
            cx.emit(WorkspaceEvent::Screen(id.clone(), number));
        }
        result
    })
    .map_err(closed)?
}

/// A thread's terminal: the program while it runs, or the screen it left when it ended.
pub(crate) enum Shown {
    Running(Terminal),
    Ended(Finished),
}

fn act(shown: &Shown, op: ThreadOp) -> std::io::Result<OpResult> {
    match shown {
        Shown::Running(terminal) => act_running(terminal, op),
        Shown::Ended(finished) => act_ended(finished, &op),
    }
}

// An ended program has no pty: input is refused, and the view and the text still work.
fn act_ended(finished: &Finished, op: &ThreadOp) -> std::io::Result<OpResult> {
    match op {
        ThreadOp::Input { .. } | ThreadOp::Paste { .. } => {
            Err(std::io::Error::other("the program has ended"))
        }
        ThreadOp::Resize { .. } | ThreadOp::Report { .. } => Ok(OpResult::Done),
        ThreadOp::Scroll { lines, .. } => {
            finished.scroll(*lines);
            Ok(OpResult::Done)
        }
        ThreadOp::Select {
            mode,
            column,
            row,
            right,
            ..
        } => {
            finished.select(terminal_mode(*mode), *column, *row, *right);
            Ok(OpResult::Done)
        }
        ThreadOp::Copy { .. } => Ok(finished.selection().map_or(OpResult::Done, OpResult::Text)),
        ThreadOp::Screen {
            scrollback, lines, ..
        } => screen_result(
            *scrollback,
            *lines,
            || finished.transcript(),
            || finished.screen(),
        ),
        _ => Err(std::io::Error::other("not a terminal operation")),
    }
}

fn act_running(terminal: &Terminal, op: ThreadOp) -> std::io::Result<OpResult> {
    match op {
        ThreadOp::Input { text, .. } => terminal.write(text.into_bytes()).map(|()| OpResult::Done),
        ThreadOp::Resize { cols, rows, .. } => terminal.resize(cols, rows).map(|()| OpResult::Done),
        ThreadOp::Paste { text, .. } => terminal
            .paste(text.as_deref().unwrap_or(""))
            .map(|()| OpResult::Done),
        ThreadOp::Report { bytes, .. } => terminal.report(bytes).map(|()| OpResult::Done),
        ThreadOp::Scroll { lines, .. } => {
            terminal.scroll(lines);
            Ok(OpResult::Done)
        }
        ThreadOp::Select {
            mode,
            column,
            row,
            right,
            ..
        } => {
            terminal.select(terminal_mode(mode), column, row, right);
            Ok(OpResult::Done)
        }
        ThreadOp::Copy { .. } => Ok(terminal.selection().map_or(OpResult::Done, OpResult::Text)),
        ThreadOp::Screen {
            scrollback, lines, ..
        } => screen_result(
            scrollback,
            lines,
            || terminal.transcript(),
            || terminal.screen(),
        ),
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
    /// A terminal's screen as cells, for a window in this process: the program's, or an extra one by number.
    #[must_use]
    pub fn grid(&self, id: &ThreadId, terminal: Option<u32>) -> Option<kg_terminal::Grid> {
        if let Some(number) = terminal {
            return super::extra::find(self, id, number).map(Terminal::grid);
        }
        self.terminals.get(id).map(|shown| match shown {
            Shown::Running(terminal) => terminal.grid(),
            Shown::Ended(finished) => finished.grid(),
        })
    }
}

pub(crate) fn kill(workspace: &mut Workspace, id: &ThreadId) {
    super::extra::end(workspace, id);
    workspace.terminals.remove(id);
    workspace.sizes.remove(id);
    workspace.runs.remove(id);
    workspace.ends.remove(id);
    workspace.losses.remove(id);
    workspace.saved.retain(|(thread, _)| thread != id);
}

/// Drops the screen an ended program left, once its thread is past the archive grace.
pub(crate) fn release_ended(workspace: &mut Workspace, id: &ThreadId) {
    if matches!(workspace.terminals.get(id), Some(Shown::Ended(_))) {
        workspace.terminals.remove(id);
        workspace.losses.remove(id);
    }
}

#[cfg(test)]
mod tests;
