use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext, AsyncApp, Context, Entity, WeakEntity};
use kg_domain::{
    EnvironmentStatus, EventKind, OpError, OpResult, SelectMode, ThreadId, ThreadOp, ThreadStatus,
    TurnOutcome,
};
use kg_terminal::{Exit, Finished, Launch, Terminal};

use crate::database::Result;
use crate::workspace::{Request, closed, missing};
use crate::{Thread, Workspace, WorkspaceEvent};

use super::agent::{self, Start};
use super::halt::status;
use super::resolve_program;

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

async fn persist(
    this: &WeakEntity<Workspace>,
    keep: Vec<(ThreadId, u64, Finished)>,
    cx: &mut AsyncApp,
) {
    for (id, seq, finished) in keep {
        #[cfg(any(test, feature = "test-support"))]
        if let Ok(Some(hold)) = this.update(cx, |workspace, _| workspace.screen_hold.clone()) {
            let _ = hold.recv().await;
        }
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
                let _ = this.update(cx, |workspace, cx| {
                    remember(workspace, &id, seq, loss, cx);
                    workspace.saved.insert((id.clone(), seq));
                });
            }
            Ok(false) => {
                let _ = this.update(cx, |workspace, _| {
                    workspace.saved.insert((id, seq));
                });
            }
            Err(error) => eprintln!("kg: could not keep the screen: {error}"),
        }
    }
}

pub(crate) fn remember(
    workspace: &mut Workspace,
    id: &ThreadId,
    seq: u64,
    loss: Option<crate::end::Loss>,
    cx: &mut Context<Workspace>,
) {
    if workspace.runs.get(id) != Some(&seq) {
        return;
    }
    match loss {
        Some(loss) => {
            workspace.losses.insert(id.clone(), loss);
        }
        None => {
            workspace.losses.remove(id);
        }
    }
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
                // Whether anything still needs following: a running program, or a screen still settling.
                let mut following = false;
                let now = cx.background_executor().now();
                for (id, shown) in &workspace.terminals {
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
                                && let Some(&seq) = workspace.runs.get(id)
                                && !workspace.saved.contains(&(id.clone(), seq))
                            {
                                following = true;
                                keep.push((id.clone(), seq, finished.clone()));
                            }
                            continue;
                        }
                    };
                    following = true;
                    if terminal.take_changed() {
                        cx.emit(WorkspaceEvent::Screen(id.clone(), None));
                    }
                    if let Some(thread) = workspace.threads.get(id) {
                        match follow(thread, terminal.title().as_deref(), now, cx) {
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
                        && !workspace.exiting.contains(id)
                    {
                        ended.push((id.clone(), exit));
                    }
                }
                for (id, _) in &ended {
                    workspace.exiting.insert(id.clone());
                }
                following |= super::extra::poll(workspace, cx);
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

/// Why a program was asked to stop. Quit still records `stopped` and `idle`, and skips `turn.ended`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    Stop,
    Quit,
}

/// Asks a running program to end and marks the thread stopping. Its exit records the stop and keeps
/// the screen. Returns whether there was a program to end.
pub(crate) fn request_stop(workspace: &mut Workspace, id: &ThreadId) -> bool {
    let Some(Shown::Running(terminal)) = workspace.terminals.get(id) else {
        return false;
    };
    terminal.stop();
    workspace
        .stopping
        .entry(id.clone())
        .or_insert(StopReason::Stop);
    true
}

/// Turns a running program's terminal into the screen it left, and follows it until its output settles.
fn keep_screen(workspace: &mut Workspace, id: &ThreadId, cx: &mut Context<Workspace>) {
    if let Some(Shown::Running(terminal)) = workspace.terminals.remove(id) {
        workspace
            .terminals
            .insert(id.clone(), Shown::Ended(terminal.finish()));
    }
    cx.emit(WorkspaceEvent::Screen(id.clone(), None));
    follow_terminals(workspace, cx);
}

pub(super) fn follow_terminals(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    if !workspace.watching {
        workspace.watching = true;
        watch(cx);
    }
}

async fn status_idle(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    status(this, thread, ThreadStatus::Idle, cx).await
}

/// Runs a stopped or recovered thread's program again, in the folder it had. Nothing else calls this.
pub(crate) async fn start_again(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let thread = super::thread(this, &id, cx)?;
    let record = thread.read_with(cx, |thread, _| thread.record.clone());
    if record.archived_at.is_some() {
        return Err(OpError::Invalid(
            "restore the thread before starting it again".into(),
        ));
    }
    if thread.read_with(cx, |thread, _| thread.running()) {
        return Err(OpError::Invalid("the program is still running".into()));
    }
    let database = crate::workspace::database(this, cx)?;
    let folder = record.environment_id.clone();
    let ready = match folder {
        Some(folder) => database
            .call(move |store| store.environment(&folder))
            .await?
            .is_some_and(|environment| environment.status == EnvironmentStatus::Ready),
        None => false,
    };
    if !ready {
        return Err(OpError::Invalid(
            "its folder is not ready; restore it with kg environment restore first".into(),
        ));
    }
    status(this, &thread, ThreadStatus::Starting, cx).await?;
    start(this, &thread, Start::Again, cx).await?;
    Ok(OpResult::Thread(
        thread.read_with(cx, |thread, _| thread.record.clone()),
    ))
}

/// Records how the program ended. It runs in the request loop, after the start that made it, so Active is set first.
pub(crate) async fn exited(
    this: &WeakEntity<Workspace>,
    id: ThreadId,
    exit: Exit,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let reason = this
        .update(cx, |workspace, _| {
            workspace.exiting.remove(&id);
            workspace.stopping.remove(&id)
        })
        .map_err(closed)?;
    let Ok(thread) = super::thread(this, &id, cx) else {
        return Ok(OpResult::Done);
    };
    let (current, turn_open) =
        thread.read_with(cx, |thread, _| (thread.record.status, thread.turn_open));
    if !matches!(current, ThreadStatus::Active | ThreadStatus::Stopping) {
        return Ok(OpResult::Done);
    }
    if turn_open {
        thread.update(cx, |thread, _| thread.turn_open = false);
    }
    super::halt::settle(this, &thread, cx).await?;
    this.update(cx, |workspace, cx| keep_screen(workspace, &id, cx))
        .map_err(closed)?;
    if let Some(reason) = reason {
        Workspace::events(
            this,
            id,
            close_turn(
                turn_open && reason != StopReason::Quit,
                TurnOutcome::Interrupted,
                None,
                vec![EventKind::Stopped, EventKind::Idle],
            ),
            cx,
        )
        .await?;
        status_idle(this, &thread, cx).await?;
        return Ok(OpResult::Done);
    }
    let (events, next) = match exit {
        Exit::Code(0) => (
            close_turn(
                turn_open,
                TurnOutcome::Completed,
                None,
                vec![
                    EventKind::Exited {
                        code: Some(0),
                        signal: None,
                    },
                    EventKind::Idle,
                ],
            ),
            ThreadStatus::Idle,
        ),
        Exit::Code(code) => {
            let error = format!("the program exited with code {code}");
            (
                close_turn(
                    turn_open,
                    TurnOutcome::Failed,
                    Some(error.clone()),
                    vec![
                        EventKind::Exited {
                            code: Some(code),
                            signal: None,
                        },
                        EventKind::Failed { error: Some(error) },
                    ],
                ),
                ThreadStatus::Error,
            )
        }
        Exit::Signal(signal) => {
            let error = format!("the program was ended by signal {signal}");
            (
                close_turn(
                    turn_open,
                    TurnOutcome::Failed,
                    Some(error.clone()),
                    vec![
                        EventKind::Exited {
                            code: None,
                            signal: Some(signal),
                        },
                        EventKind::Failed { error: Some(error) },
                    ],
                ),
                ThreadStatus::Error,
            )
        }
    };
    Workspace::events(this, id, events, cx).await?;
    status(this, &thread, next, cx).await?;
    if next == ThreadStatus::Idle {
        done(this, &thread, cx).await?;
    }
    Ok(OpResult::Done)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spin {
    Opened,
    /// Stop cleared `busy` while the title was still a spinner.
    Resumed,
    Closed,
}

fn follow(
    thread: &Entity<Thread>,
    title: Option<&str>,
    now: Instant,
    cx: &mut App,
) -> Option<Spin> {
    let busy_title = title.is_some_and(busy);
    thread.update(cx, |thread, cx| {
        let next = if busy_title {
            thread.plain_since = None;
            true
        } else if thread.busy {
            let since = *thread.plain_since.get_or_insert(now);
            now.duration_since(since) < QUIET
        } else {
            false
        };
        if next == thread.busy {
            return None;
        }
        let opened = next && !thread.turn_open;
        thread.busy = next;
        thread.turn_open = next;
        cx.notify();
        Some(if !next {
            Spin::Closed
        } else if opened {
            Spin::Opened
        } else {
            Spin::Resumed
        })
    })
}

async fn waiting(this: &WeakEntity<Workspace>, id: ThreadId, cx: &mut AsyncApp) -> Result<()> {
    let reason = this
        .update(cx, |workspace, _| workspace.stopping.get(&id).copied())
        .map_err(closed)?;
    let thread = super::thread(this, &id, cx)?;
    done(this, &thread, cx).await?;
    let mut kinds = vec![EventKind::Waiting];
    if reason != Some(StopReason::Quit) {
        kinds.insert(
            0,
            EventKind::TurnEnded {
                outcome: if reason.is_some() {
                    TurnOutcome::Interrupted
                } else {
                    TurnOutcome::Completed
                },
                error: None,
            },
        );
    }
    Workspace::events(this, id.clone(), kinds, cx).await?;
    this.update(cx, |_, cx| cx.emit(WorkspaceEvent::Waiting(id)))
        .map_err(closed)
}

async fn done(
    this: &WeakEntity<Workspace>,
    thread: &Entity<Thread>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let id = thread.read_with(cx, |thread, _| thread.record.id.clone());
    let saved = id.clone();
    database.call(move |store| store.mark_done(&saved)).await?;
    thread.update(cx, |thread, cx| {
        thread.record.mark = kg_domain::ThreadMark::Done;
        cx.notify();
    });
    this.update(cx, |_, cx| {
        cx.emit(WorkspaceEvent::Finished(id));
        cx.emit(WorkspaceEvent::Changed);
    })
    .map_err(closed)
}

fn close_turn(
    open: bool,
    outcome: TurnOutcome,
    error: Option<String>,
    mut kinds: Vec<EventKind>,
) -> Vec<EventKind> {
    if open {
        kinds.insert(0, EventKind::TurnEnded { outcome, error });
    }
    kinds
}

fn busy(title: &str) -> bool {
    title
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '\u{2800}'..='\u{28FF}' | '\u{25D0}'..='\u{25D3}'))
}

#[cfg(test)]
mod tests;
