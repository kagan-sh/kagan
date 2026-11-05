use std::ffi::{OsStr, OsString};
use std::path::Path;

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{EventKind, NoticeSeverity, OpResult, ThreadId};
use serde_json::Value;

use crate::host::database::Result;
use crate::workspace::closed;
use crate::{PluginRequest, SocketLocation, Workspace, skill};

use super::conversation;
use super::run::ResolvedProgram;

const PREFERENCE: &str = "agent-integration";
const SKILL_LINK: &str = ".agents/skills/kg-threads";

// https://code.claude.com/docs/en/cli-reference: the commands in its table, and the aliases plugins and kill.
const CLAUDE_SUBCOMMANDS: &[&str] = &[
    "agents",
    "attach",
    "auth",
    "auto-mode",
    "daemon",
    "doctor",
    "gateway",
    "import",
    "install",
    "kill",
    "logs",
    "mcp",
    "plugin",
    "plugins",
    "purge",
    "remote-control",
    "respawn",
    "rm",
    "self-hosted-runner",
    "setup-token",
    "stop",
    "ultrareview",
    "update",
];

const CLAUDE_PERMISSION_FLAGS: &[&str] = &[
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
];

// Cursor Agent 2026.10.22-27dcf5f, from `agent --help`. `agent` is that program too.
const CURSOR_SUBCOMMANDS: &[&str] = &[
    "about",
    "agent",
    "bedrock",
    "create-chat",
    "generate-rule",
    "help",
    "install-shell-integration",
    "login",
    "logout",
    "ls",
    "mcp",
    "models",
    "persist",
    "plugin",
    "resume",
    "rule",
    "status",
    "team",
    "uninstall-shell-integration",
    "update",
    "whoami",
    "worker",
];

// Options that take the next argument, so a subcommand after one is still seen.
const CURSOR_VALUE_FLAGS: &[&str] = &[
    "--add-dir",
    "--api-key",
    "--endpoint",
    "--header",
    "--mode",
    "--model",
    "--new-session-id",
    "--output-format",
    "--plugin-dir",
    "--sandbox",
    "--worktree",
    "--worktree-base",
    "--workspace",
    "-H",
    "-e",
    "-w",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Start {
    First,
    Again,
}

pub(crate) struct Integration {
    pub(crate) env: Vec<(OsString, OsString)>,
    pub(crate) args: Vec<OsString>,
}

pub(super) enum Known {
    Claude,
    Codex,
    Cursor,
}

/// The `KG_*` variables and the PATH with `kg` on it, which every terminal of the thread gets.
pub(super) fn variables(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    cx: &mut AsyncApp,
) -> Result<Vec<(OsString, OsString)>> {
    let (cli, socket) = this
        .update(cx, |workspace, cx| {
            (
                workspace.cli.clone(),
                cx.try_global::<SocketLocation>()
                    .map(|socket| socket.0.clone()),
            )
        })
        .map_err(closed)?;
    Ok(environment(
        record.id.0.as_str(),
        record.project_id.0.as_str(),
        record.environment_id.as_ref().map(|id| id.0.as_str()),
        &cli,
        socket.as_deref(),
        kg_process::user_path().as_ref(),
    ))
}

pub(crate) async fn integrate(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    program: &ResolvedProgram,
    folder: &Path,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<Integration> {
    let env = variables(this, record, cx)?;
    let (cli, data_dir, database, socket) = this
        .update(cx, |workspace, cx| {
            (
                workspace.cli.clone(),
                workspace.data_dir.clone(),
                workspace.database.clone(),
                cx.try_global::<SocketLocation>()
                    .map(|socket| socket.0.clone()),
            )
        })
        .map_err(closed)?;
    let id = record.id.0.as_str();
    let on = database.call(|store| store.setting(PREFERENCE)).await? != Some(Value::Bool(false));
    let Some(known) = known(&program.path).filter(|known| on && !subcommand(known, &program.args))
    else {
        let _ = this.update(cx, |workspace, cx| {
            if let Some(thread) = workspace.threads.get(&record.id) {
                thread.update(cx, |thread, _| thread.instruct = None);
            }
        });
        return Ok(Integration {
            env,
            args: Vec::new(),
        });
    };
    let setup = async {
        let extra = collect_instruct(this, record, cx).await;
        let agent = data_dir.join("agent");
        skill::mirror(this, agent.join("skills"), cx).await?;
        write_agent_files(&known, &agent, id, &cli, socket.as_deref(), &extra, cx).await?;
        if matches!(known, Known::Codex) {
            let skill = data_dir.join("agent").join("skills").join("kg-threads");
            if let Err(error) = link_skill(folder, &skill) {
                eprintln!(
                    "kg: could not offer kg-threads in {}: {error}",
                    folder.display()
                );
            }
        }
        let mut args = flags(&known, id, &cli, socket.as_deref(), &agent);
        let (conversation, notice) =
            conversation::args(this, &known, record, &program.args, folder, start, cx).await?;
        args.extend(conversation);
        Result::Ok((args, notice))
    };
    let (args, notice) = match setup.await {
        Ok((args, notice)) => (
            args,
            notice.map(|text| (NoticeSeverity::Info, text.to_owned())),
        ),
        Err(error) => (
            Vec::new(),
            Some((
                NoticeSeverity::Warning,
                format!(
                    "kagan could not set up the agent integration, so the program starts without it: {error}"
                ),
            )),
        ),
    };
    if let Some((severity, text)) = notice {
        let notice = EventKind::Notice { severity, text };
        Workspace::events(this, record.id.clone(), vec![notice], cx).await?;
    }
    Ok(Integration { env, args })
}

fn known(program: &Path) -> Option<Known> {
    match program.file_stem()?.to_str()? {
        "claude" => Some(Known::Claude),
        "codex" => Some(Known::Codex),
        "cursor-agent" | "agent" => Some(Known::Cursor),
        _ => None,
    }
}

// Claude takes no flag before a subcommand, so kagan adds nothing to a line that names one. Codex
// takes its flags before a subcommand, so its overrides always apply. A Cursor subcommand is not
// its conversation, so kagan adds nothing to that line either.
fn subcommand(known: &Known, args: &[OsString]) -> bool {
    match known {
        Known::Claude => args
            .iter()
            .find(|arg| {
                !CLAUDE_PERMISSION_FLAGS
                    .iter()
                    .any(|flag| arg.as_os_str() == OsStr::new(flag))
            })
            .and_then(|arg| arg.to_str())
            .is_some_and(|arg| CLAUDE_SUBCOMMANDS.contains(&arg)),
        Known::Cursor => cursor_subcommand(args),
        Known::Codex => false,
    }
}

fn cursor_subcommand(args: &[OsString]) -> bool {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let Some(arg) = arg.to_str() else {
            return false;
        };
        if arg == "--" {
            return args.next().is_some_and(|arg| {
                arg.to_str()
                    .is_some_and(|arg| CURSOR_SUBCOMMANDS.contains(&arg))
            });
        }
        if arg.starts_with('-') {
            if !arg.contains('=') && CURSOR_VALUE_FLAGS.contains(&arg) {
                args.next();
            }
            continue;
        }
        return CURSOR_SUBCOMMANDS.contains(&arg);
    }
    false
}

async fn collect_instruct(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    cx: &mut AsyncApp,
) -> String {
    let text = match crate::dispatch::forward(
        this,
        PluginRequest::Instruct {
            thread_id: record.id.clone(),
            project_id: record.project_id.clone(),
        },
        cx,
    )
    .await
    {
        Ok(OpResult::Output(Some(text))) => text,
        _ => String::new(),
    };
    let instruct = (!text.is_empty()).then(|| text.clone());
    let _ = this.update(cx, |workspace, cx| {
        if let Some(thread) = workspace.threads.get(&record.id) {
            thread.update(cx, |thread, _| {
                thread.instruct = instruct;
            });
        }
    });
    text
}

mod files;

use files::{environment, flags, link_skill, write_agent_files};

pub(crate) async fn forget(this: &WeakEntity<Workspace>, ids: Vec<ThreadId>, cx: &mut AsyncApp) {
    let Ok(data_dir) = this.update(cx, |workspace, _| workspace.data_dir.clone()) else {
        return;
    };
    cx.background_spawn(async move {
        for id in ids {
            let folder = data_dir.join("agent").join("threads").join(&id.0);
            if let Err(error) = std::fs::remove_dir_all(&folder)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("kg: could not remove {}: {error}", folder.display());
            }
        }
    })
    .await;
}
fn introduction() -> &'static str {
    "You are working inside kagan, a workspace for programs in threads. `kg` is on PATH. Run `kg status --json` for this thread, its project, and its environment. Run `kg guide` for the map.\n\
     \n\
     Follow the skill kg-threads before you create a thread, or paste into, wait on, stop, or read another one. Stay in this thread unless the person asked for another.\n\
     \n\
     Plugin powers and a delete are approved by the person in the app. `kg ui` cannot approve them. `kg thread ask` raises a question, and the answer arrives as `answered`. The program may set the title once with `kg thread rename`. A later one says the title was kept, and a name set in the window stays."
}

pub(crate) fn note(id: &str) -> String {
    note_with(id, "")
}

pub(crate) fn note_with(id: &str, extra: &str) -> String {
    let mut text = format!(
        "{}\n\nYou are running inside kagan thread {id}.",
        introduction()
    );
    if !extra.is_empty() {
        text.push_str("\n\n");
        text.push_str(extra);
    }
    text
}

// A literal string takes backslashes and spaces as they are, so Windows paths need no escaping.
fn toml(text: &str) -> String {
    if text.contains('\'') {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{text}'")
    }
}

#[cfg(test)]
mod tests;
