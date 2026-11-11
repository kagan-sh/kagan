use crate::ThreadId;

use super::automation::valid;
use super::browser::browser_check;
use super::env::{shell_name, variable_name, variable_value};
use super::saved_prompt::TITLE_MAX;
use super::{
    DiffOp, EnvOp, EnvironmentOp, Op, PluginOp, PreviewOp, ProgramOp, ProjectOp, SEARCH_LIMIT,
    SEARCH_TEXT_MAX, SavedPromptOp, SectionOp, SurfaceOp, TERMINAL_COLS, TERMINAL_ROWS, TEXT_MAX,
    TerminalOp, ThreadOp, TurnOp, WindowOp,
};
use crate::{saved_name, split_program};

/// Rejects an op a client must not send.
///
/// # Errors
///
/// Returns an error when a thread id is not a plain identifier (at most 128 ASCII letters,
/// digits, `_`, or `-`), when a surface id is empty or holds a control character, when a search
/// is empty, longer than [`SEARCH_TEXT_MAX`], or names a limit outside [`SEARCH_LIMIT`], when a
/// resize is outside [`TERMINAL_COLS`] and [`TERMINAL_ROWS`], or when a window index is outside
/// 1 to 9.
pub fn check(op: &Op) -> Result<(), String> {
    match op {
        Op::Window(WindowOp::Thread { index }) if !(1..=9).contains(index) => {
            Err("thread index must be between 1 and 9".into())
        }
        Op::Status { thread_id, .. } => plain_opt(thread_id.as_ref()),
        Op::Thread(op) => thread_ids(op)
            .into_iter()
            .try_for_each(plain)
            .and_then(|()| terminal_size(op))
            .and_then(|()| search(op))
            .and_then(|()| match op {
                ThreadOp::Paste { text, file, .. } => carried_text(text.as_ref(), file.as_ref()),
                _ => Ok(()),
            }),
        Op::Environment(op) => environment_thread(op),
        Op::Env(op) => env_check(op),
        Op::Project(ProjectOp::SetProgram {
            program: Some(program),
            ..
        }) => split_program(program)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Op::Program(ProgramOp::Save { name, line }) => {
            saved_name(name)?;
            split_program(line)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        Op::Program(ProgramOp::Rename { name, to }) => {
            saved_name(name)?;
            saved_name(to).map(|_| ())
        }
        Op::Program(ProgramOp::Remove { name } | ProgramOp::Pin { name, .. }) => {
            saved_name(name).map(|_| ())
        }
        Op::Plugin(op) => plain_opt(plugin_thread(op)),
        Op::Browser(op) => plain(op.thread()).and_then(|()| browser_check(op)),
        Op::Guide { .. }
        | Op::Skill(_)
        | Op::Open
        | Op::Project(_)
        | Op::Program(ProgramOp::List { .. } | ProgramOp::Discover { .. })
        | Op::Theme(_)
        | Op::Keymap(_)
        | Op::Preference(_)
        | Op::Instructions(_)
        | Op::Settings(_)
        | Op::Window(_)
        | Op::Sidebar
        | Op::Ui(_)
        | Op::Notify(_)
        | Op::Diagnostics(_)
        | Op::Update(_) => Ok(()),
        Op::Surface(op) => match op {
            SurfaceOp::List | SurfaceOp::Threads => Ok(()),
            SurfaceOp::Open { id, .. } | SurfaceOp::Close { id, .. } => {
                printable(id).ok_or_else(|| "surface id must be a plain name".into())
            }
        },
        Op::Automation(op) => valid(op),
        Op::Section(op) => match op {
            SectionOp::Create { thread_id, .. } => plain_opt(thread_id.as_ref()),
            SectionOp::File { thread_id, .. } => plain(thread_id),
            SectionOp::Rename { .. } | SectionOp::Delete { .. } => Ok(()),
        },
        Op::SavedPrompt(op) => saved_prompt(op),
        Op::Diff(op) => match op {
            DiffOp::List { query } | DiffOp::Show { query, .. } => plain(&query.thread_id),
            DiffOp::Commits { thread_id } => plain(thread_id),
        },
        Op::Preview(op) => match op {
            PreviewOp::Show { thread_id, path } => plain_opt(thread_id.as_ref()).and_then(|()| {
                printable(path).ok_or_else(|| "preview path must be a plain path".into())
            }),
        },
    }
}

fn printable(text: &str) -> Option<()> {
    (!text.is_empty() && !text.chars().any(char::is_control)).then_some(())
}

fn terminal_size(op: &ThreadOp) -> Result<(), String> {
    match op {
        ThreadOp::Resize { cols, rows, .. }
            if !TERMINAL_COLS.contains(cols) || !TERMINAL_ROWS.contains(rows) =>
        {
            Err(format!(
                "terminal size must be {} to {} columns and {} to {} rows",
                TERMINAL_COLS.start(),
                TERMINAL_COLS.end(),
                TERMINAL_ROWS.start(),
                TERMINAL_ROWS.end()
            ))
        }
        _ => Ok(()),
    }
}

/// Text a paste or a fill carries. A client reads `--file` and sends the text, never the path.
pub(super) fn carried_text<P>(text: Option<&String>, file: Option<&P>) -> Result<(), String> {
    match (text, file) {
        (Some(_), Some(_)) => Err("pass the text or --file, not both".into()),
        (None, None) => Err("pass the text or --file".into()),
        (None, Some(_)) => Err("send the text, not a path".into()),
        (Some(text), None) if text.len() > TEXT_MAX => {
            Err(format!("text is over {} MiB", TEXT_MAX >> 20))
        }
        (Some(_), None) => Ok(()),
    }
}

fn search(op: &ThreadOp) -> Result<(), String> {
    match op {
        ThreadOp::Search { text, .. } if text.trim().is_empty() => Err("search needs text".into()),
        ThreadOp::Search { text, .. } if text.chars().count() > SEARCH_TEXT_MAX => Err(format!(
            "search text is longer than {SEARCH_TEXT_MAX} characters"
        )),
        ThreadOp::Search {
            limit: Some(limit), ..
        } if !(1..=SEARCH_LIMIT).contains(limit) => {
            Err(format!("--limit must be from 1 to {SEARCH_LIMIT}"))
        }
        _ => Ok(()),
    }
}

fn plain(id: &ThreadId) -> Result<(), String> {
    if id.plain() {
        Ok(())
    } else {
        Err("thread id must be a plain identifier".into())
    }
}

fn plain_opt(id: Option<&ThreadId>) -> Result<(), String> {
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
        | ThreadOp::Turn(TurnOp::Start { id } | TurnOp::End { id })
        | ThreadOp::Terminal(
            TerminalOp::New { id } | TerminalOp::List { id } | TerminalOp::Close { id, .. },
        )
        | ThreadOp::Details { id }
        | ThreadOp::Links { id }
        | ThreadOp::Archive { id, .. }
        | ThreadOp::Unarchive { id, .. }
        | ThreadOp::SetPinned { id, .. }
        | ThreadOp::Notify { id, .. }
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

// A thread's own terminal sends its thread id, so a program kagan started cannot change what
// later programs get. The check keys off that id; a program that drops it is not stopped here.
fn env_check(op: &EnvOp) -> Result<(), String> {
    match op {
        EnvOp::List { .. } => Ok(()),
        EnvOp::Set {
            thread_id: Some(_), ..
        }
        | EnvOp::Remove {
            thread_id: Some(_), ..
        } => Err(
            "a thread cannot change environment variables; set them in Settings → General or on the project's page"
                .into(),
        ),
        EnvOp::Set { name, value, .. } => {
            variable_name(name)?;
            value.as_deref().map_or(Ok(()), variable_value)
        }
        EnvOp::Remove { name, .. } => shell_name(name),
    }
}

fn saved_prompt(op: &SavedPromptOp) -> Result<(), String> {
    match op {
        SavedPromptOp::Add { title, body } => prompt_title(title).and_then(|()| prompt_body(body)),
        SavedPromptOp::Search { text } if text.chars().count() > SEARCH_TEXT_MAX => Err(format!(
            "search text is longer than {SEARCH_TEXT_MAX} characters"
        )),
        SavedPromptOp::Search { .. } => Ok(()),
        SavedPromptOp::Star { id, .. } | SavedPromptOp::Delete { id } => prompt_id(id),
        SavedPromptOp::Send { id, thread_id } => prompt_id(id).and_then(|()| plain(thread_id)),
    }
}

fn prompt_title(title: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        Err("a saved prompt needs a title".into())
    } else if title.chars().count() > TITLE_MAX {
        Err(format!(
            "a saved prompt title is longer than {TITLE_MAX} characters"
        ))
    } else if title.chars().any(char::is_control) {
        Err("a saved prompt title cannot hold a control character".into())
    } else {
        Ok(())
    }
}

fn prompt_body(body: &str) -> Result<(), String> {
    if body.trim().is_empty() || !body.chars().any(|c| !c.is_control()) {
        Err("a saved prompt needs text".into())
    } else if body.len() > TEXT_MAX {
        Err(format!("text is over {} MiB", TEXT_MAX >> 20))
    } else {
        Ok(())
    }
}

fn prompt_id(id: &crate::SavedPromptId) -> Result<(), String> {
    if id.plain() {
        Ok(())
    } else {
        Err("saved prompt id must be a plain identifier".into())
    }
}

fn environment_thread(op: &EnvironmentOp) -> Result<(), String> {
    match op {
        EnvironmentOp::Restore { thread_id } => plain(thread_id),
        EnvironmentOp::Setup { rev, .. } if rev.starts_with('-') => {
            Err("a revision cannot start with -".into())
        }
        EnvironmentOp::Get { .. }
        | EnvironmentOp::Branches { .. }
        | EnvironmentOp::Setup { .. }
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
        | PluginOp::Outdated
        | PluginOp::Share { .. }
        | PluginOp::Logs { .. }
        | PluginOp::Types { .. }
        | PluginOp::List
        | PluginOp::Index { .. }
        | PluginOp::Search { .. }
        | PluginOp::Enable { .. }
        | PluginOp::Disable { .. }
        | PluginOp::Rail { .. } => None,
    }
}
