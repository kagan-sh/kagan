use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{
    DiagnosticsOp, EventKind, LogCounts, OpError, OpResult, Plugin, PluginLog, Thread, ThreadId,
    ThreadStatus, is_git_source,
};
use kg_store::{ScreenRow, Store};
use kg_terminal::SNAPSHOT_MAX;

use crate::PluginRequest;
use crate::end::End;
use crate::host::database::Result;
use crate::workspace::{Workspace, closed};

pub(crate) async fn export(
    this: &WeakEntity<Workspace>,
    op: DiagnosticsOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let DiagnosticsOp::Export { path } = op;
    let (database, data_dir, terminals) = this
        .update(cx, |workspace, _| {
            (
                workspace.database.clone(),
                workspace.data_dir.clone(),
                workspace.live_containments(),
            )
        })
        .map_err(closed)?;
    let facts = database.call(|store| Facts::read(store)).await?;
    let OpResult::PluginLogs(logs) = super::forward(this, PluginRequest::Logs, cx).await? else {
        return Err(OpError::Failed("plugin host returned no logs".into()));
    };
    let text = render(&facts, &data_dir, &terminals, &logs);
    let path = cx
        .background_spawn(async move {
            let path = match path {
                Some(path) => path,
                None => default_path(&data_dir)
                    .map_err(|error| OpError::Failed(format!("{}: {error}", data_dir.display())))?,
            };
            write_new(&path, &text)?;
            Ok::<std::path::PathBuf, OpError>(path)
        })
        .await?;
    Ok(OpResult::Diagnostics { path })
}

struct Facts {
    schema: i64,
    threads: Vec<Thread>,
    endings: Vec<(ThreadId, Vec<EventKind>)>,
    screens: Vec<ScreenRow>,
    plugins: Vec<Plugin>,
}

impl Facts {
    fn read(store: &Store) -> kg_store::Result<Self> {
        Ok(Self {
            schema: store.schema_version()?,
            threads: store.all_threads()?,
            endings: store.endings()?,
            screens: store.screens()?,
            plugins: store.plugins()?,
        })
    }
}

fn render(
    facts: &Facts,
    data_dir: &Path,
    terminals: &[(ThreadId, Option<u32>, String)],
    logs: &[PluginLog],
) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "kagan {version}\nschema {schema}\ndata {data}\n\n\
             This file lists the kagan version, the schema, the data directory, each thread's id, status, and structured cause, whether a screen was kept and how much history was dropped, each live terminal's thread and containment, and each plugin's id, kind, version, and warn and error counts.\n\
             A containment is `tree` (the program's process tree), `job` (a Windows job object), or `unavailable` with the OS error when the job could not be made, so stopping ends only the program.\n\
             Titles, end text, plugin sources, and plugin log messages are omitted.\n\
             It has no environment variables and no terminal contents.\n\
             A program that was still running when kagan quit has no saved screen.\n\
             The snapshot bound is {bound} bytes for the whole screen and its history.\n\n",
            version = env!("CARGO_PKG_VERSION"),
            schema = facts.schema,
            data = data_dir.display(),
            bound = SNAPSHOT_MAX,
        ),
    );
    out.push_str("threads\n");
    if facts.threads.is_empty() {
        out.push_str("none\n");
    }
    for thread in &facts.threads {
        // Writing to a String cannot fail.
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "{id} {status}\n",
                id = thread.id,
                status = status_word(thread.status),
            ),
        );
        match cause_of(&facts.endings, &thread.id) {
            Some(line) => {
                out.push_str("  end: ");
                out.push_str(&line);
                out.push('\n');
            }
            None => out.push_str("  end: none\n"),
        }
        out.push_str("  ");
        out.push_str(&screen_line(
            facts.screens.iter().find(|row| row.thread_id == thread.id),
        ));
        out.push('\n');
    }
    out.push_str("\nterminals\n");
    if terminals.is_empty() {
        out.push_str("none\n");
    }
    for (thread, number, containment) in terminals {
        let which = match number {
            None => "program".to_string(),
            Some(number) => format!("terminal {number}"),
        };
        // Writing to a String cannot fail.
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("{thread} {which} containment={containment}\n"),
        );
    }
    out.push_str("\nplugins\n");
    if facts.plugins.is_empty() {
        out.push_str("none\n");
    }
    for plugin in &facts.plugins {
        let kind = source_kind(&plugin.source);
        let version = commit_version(plugin.commit.as_deref());
        // Writing to a String cannot fail.
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "{id} enabled={enabled} kind={kind} version={version}\n",
                id = plugin.id,
                enabled = plugin.enabled,
                kind = kind,
                version = version,
            ),
        );
        let counts = logs
            .iter()
            .find(|log| log.id == plugin.id)
            .map(|log| &log.counts);
        out.push_str("  log: ");
        out.push_str(&log_line(counts));
        out.push('\n');
    }
    out
}

fn cause_of(endings: &[(ThreadId, Vec<kg_domain::EventKind>)], id: &ThreadId) -> Option<String> {
    endings
        .iter()
        .find(|(thread, _)| thread == id)
        .and_then(|(_, kinds)| End::from_newest(kinds))
        .map(|end| match end {
            End::Failed(_) => "Failed.".into(),
            end => end.line(),
        })
}

fn source_kind(source: &str) -> &'static str {
    if is_git_source(source) { "git" } else { "path" }
}

fn commit_version(commit: Option<&str>) -> &str {
    match commit {
        Some(text) if hex_commit(text) => text,
        Some(_) => "unknown",
        None => "none",
    }
}

fn hex_commit(text: &str) -> bool {
    (7..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn screen_line(row: Option<&ScreenRow>) -> String {
    match row {
        None => "screen: none".into(),
        Some(row) if !row.screen_kept => {
            "screen: not kept, outside the geometry or 4 MiB bound".into()
        }
        Some(row) if row.history_dropped > 0 => format!(
            "screen: {} bytes, older scrollback was not kept ({} lines)",
            row.replay.len(),
            row.history_dropped
        ),
        Some(row) => format!("screen: {} bytes", row.replay.len()),
    }
}

fn status_word(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::Pending => "pending",
        ThreadStatus::Idle => "idle",
        ThreadStatus::Starting => "starting",
        ThreadStatus::Active => "active",
        ThreadStatus::Stopping => "stopping",
        ThreadStatus::Error => "error",
    }
}

fn log_line(counts: Option<&LogCounts>) -> String {
    match counts {
        None => "not read, no plugin host".into(),
        Some(LogCounts::Missing) => "no log".into(),
        Some(LogCounts::Unreadable) => "could not read the log".into(),
        Some(LogCounts::Counted { warn: 0, error: 0 }) => "no warn or error lines".into(),
        Some(LogCounts::Counted { warn, error }) => format!("{warn} warn, {error} error"),
    }
}

fn default_path(data_dir: &Path) -> std::io::Result<PathBuf> {
    let dir = data_dir.join("diagnostics");
    std::fs::create_dir_all(&dir)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis());
    let candidates = std::iter::once(dir.join(format!("kagan-{millis}.txt")))
        .chain((2..50).map(|index| dir.join(format!("kagan-{millis}-{index}.txt"))));
    for path in candidates {
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(std::io::Error::other(
        "could not choose a diagnostics file name",
    ))
}

fn write_new(path: &Path, text: &str) -> std::result::Result<(), OpError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        return Err(OpError::Invalid(format!(
            "the folder does not exist: {}. Create it, or omit --path to write under the data directory",
            parent.display()
        )));
    }
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(OpError::Invalid("that file already exists".into()));
        }
        Err(error) => {
            return Err(OpError::Failed(format!("{}: {error}", path.display())));
        }
    };
    if let Err(error) = file.write_all(text.as_bytes()) {
        drop(file);
        // The write error below is the one to report; the half-written file is only removed.
        let _ = std::fs::remove_file(path);
        return Err(OpError::Failed(format!("{}: {error}", path.display())));
    }
    Ok(())
}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod tests;
