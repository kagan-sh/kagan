use crate::ThreadId;

use super::browser::browser_check;
use super::{
    DiffOp, EnvironmentOp, Op, PluginOp, SEARCH_LIMIT, SEARCH_TEXT_MAX, SectionOp, SurfaceOp,
    TERMINAL_COLS, TERMINAL_ROWS, TerminalOp, ThreadOp, WindowOp,
};

/// Rejects an op a client must not send.
///
/// # Errors
///
/// Returns an error when a thread id is not a plain identifier (at most 128 ASCII letters,
/// digits, `_`, or `-`), when a surface id is empty or holds a control character, when a search
/// is empty, longer than [`SEARCH_TEXT_MAX`], or names a limit outside [`SEARCH_LIMIT`], when a
/// resize is outside [`TERMINAL_COLS`] and [`TERMINAL_ROWS`], or when a window index is outside
/// 1 to 9.
pub fn check(op: &Op) -> Result<(), &'static str> {
    match op {
        Op::Window(WindowOp::Thread { index }) if !(1..=9).contains(index) => {
            Err("thread index must be between 1 and 9")
        }
        Op::Status { thread_id, .. } => plain_opt(thread_id.as_ref()),
        Op::Thread(op) => thread_ids(op)
            .into_iter()
            .try_for_each(plain)
            .and_then(|()| terminal_size(op))
            .and_then(|()| search(op)),
        Op::Environment(op) => environment_thread(op),
        Op::Plugin(op) => plain_opt(plugin_thread(op)),
        Op::Browser(op) => plain(op.thread()).and_then(|()| browser_check(op)),
        Op::Guide { .. }
        | Op::Skill(_)
        | Op::Open
        | Op::Project(_)
        | Op::Program(_)
        | Op::Theme(_)
        | Op::Keymap(_)
        | Op::Preference(_)
        | Op::Settings(_)
        | Op::Window(_)
        | Op::Sidebar
        | Op::Ui(_)
        | Op::Diagnostics(_)
        | Op::Update(_) => Ok(()),
        Op::Surface(op) => match op {
            SurfaceOp::List | SurfaceOp::Threads => Ok(()),
            SurfaceOp::Open { id, .. } | SurfaceOp::Close { id, .. } => {
                if id.is_empty() || id.chars().any(char::is_control) {
                    Err("surface id must be a plain name")
                } else {
                    Ok(())
                }
            }
        },
        Op::Section(op) => match op {
            SectionOp::Create { thread_id, .. } => plain_opt(thread_id.as_ref()),
            SectionOp::File { thread_id, .. } => plain(thread_id),
            SectionOp::Rename { .. } | SectionOp::Delete { .. } => Ok(()),
        },
        Op::Diff(op) => match op {
            DiffOp::List { query } | DiffOp::Show { query, .. } => plain(&query.thread_id),
            DiffOp::Commits { thread_id } => plain(thread_id),
        },
    }
}

fn terminal_size(op: &ThreadOp) -> Result<(), &'static str> {
    match op {
        ThreadOp::Resize { cols, rows, .. }
            if !TERMINAL_COLS.contains(cols) || !TERMINAL_ROWS.contains(rows) =>
        {
            Err("terminal size must be 2 to 1000 columns and 1 to 500 rows")
        }
        _ => Ok(()),
    }
}

fn search(op: &ThreadOp) -> Result<(), &'static str> {
    match op {
        ThreadOp::Search { text, .. } if text.trim().is_empty() => Err("search needs text"),
        ThreadOp::Search { text, .. } if text.chars().count() > SEARCH_TEXT_MAX => {
            Err("search text is longer than 256 characters")
        }
        ThreadOp::Search {
            limit: Some(limit), ..
        } if !(1..=SEARCH_LIMIT).contains(limit) => Err("--limit must be from 1 to 200"),
        _ => Ok(()),
    }
}

fn plain(id: &ThreadId) -> Result<(), &'static str> {
    if id.plain() {
        Ok(())
    } else {
        Err("thread id must be a plain identifier")
    }
}

fn plain_opt(id: Option<&ThreadId>) -> Result<(), &'static str> {
    id.map_or(Ok(()), plain)
}

fn thread_ids(op: &ThreadOp) -> Vec<&ThreadId> {
    match op {
        ThreadOp::List { .. } | ThreadOp::Search { .. } => Vec::new(),
        ThreadOp::Run(run) => run
            .parent_thread_id
            .as_ref()
            .into_iter()
            .chain(run.lifecycle_owner_thread_id.as_ref())
            .collect(),
        ThreadOp::Delete {
            id,
            approval_thread_id,
        } => approval_thread_id
            .iter()
            .chain(std::iter::once(id))
            .collect(),
        ThreadOp::Get { id, .. }
        | ThreadOp::Stop { id, .. }
        | ThreadOp::Input { id, .. }
        | ThreadOp::Resize { id, .. }
        | ThreadOp::Screen { id, .. }
        | ThreadOp::Scroll { id, .. }
        | ThreadOp::Select { id, .. }
        | ThreadOp::Copy { id, .. }
        | ThreadOp::Paste { id, .. }
        | ThreadOp::Send { id, .. }
        | ThreadOp::Report { id, .. }
        | ThreadOp::Terminal(
            TerminalOp::New { id } | TerminalOp::List { id } | TerminalOp::Close { id, .. },
        )
        | ThreadOp::Details { id }
        | ThreadOp::Links { id }
        | ThreadOp::Archive { id, .. }
        | ThreadOp::Unarchive { id, .. }
        | ThreadOp::SetPinned { id, .. }
        | ThreadOp::Rename { id, .. }
        | ThreadOp::MarkRead { id, .. }
        | ThreadOp::Metadata { id, .. }
        | ThreadOp::SetMetadata { id, .. }
        | ThreadOp::Events { id, .. }
        | ThreadOp::WaitEvent { id, .. }
        | ThreadOp::Wait { id, .. }
        | ThreadOp::Ask { id, .. }
        | ThreadOp::StartAgain { id, .. }
        | ThreadOp::Tools { id, .. }
        | ThreadOp::CallTool { id, .. }
        | ThreadOp::Interactions { id, .. } => vec![id],
    }
}

fn environment_thread(op: &EnvironmentOp) -> Result<(), &'static str> {
    match op {
        EnvironmentOp::Restore { thread_id } => plain(thread_id),
        EnvironmentOp::Get { .. }
        | EnvironmentOp::Branches { .. }
        | EnvironmentOp::SetBase { .. }
        | EnvironmentOp::Remove { .. } => Ok(()),
    }
}

fn plugin_thread(op: &PluginOp) -> Option<&ThreadId> {
    match op {
        PluginOp::Cli { thread_id, .. }
        | PluginOp::Command { thread_id, .. }
        | PluginOp::Reload { thread_id, .. }
        | PluginOp::Dev { thread_id, .. }
        | PluginOp::Install { thread_id, .. }
        | PluginOp::Update { thread_id, .. }
        | PluginOp::Config { thread_id, .. }
        | PluginOp::Use { thread_id, .. }
        | PluginOp::Remove { thread_id, .. } => thread_id.as_ref(),
        PluginOp::Start
        | PluginOp::CancelInstall
        | PluginOp::New { .. }
        | PluginOp::Check { .. }
        | PluginOp::Share { .. }
        | PluginOp::Logs { .. }
        | PluginOp::Types { .. }
        | PluginOp::List
        | PluginOp::Index { .. }
        | PluginOp::Search { .. }
        | PluginOp::Enable { .. }
        | PluginOp::Disable { .. } => None,
    }
}
