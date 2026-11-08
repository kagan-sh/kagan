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

// OpenCode 1.4.x, from `opencode --help`.
const OPENCODE_SUBCOMMANDS: &[&str] = &[
    "upgrade",
    "update",
    "uninstall",
    "acp",
    "api",
    "debug",
    "auth",
    "mcp",
    "plugin",
    "models",
    "stats",
    "mini",
    "run",
    "session",
    "service",
    "reload",
    "pair",
    "serve",
    "help",
];

// Pi, from `pi --help`.
const PI_SUBCOMMANDS: &[&str] = &[
    "install",
    "remove",
    "uninstall",
    "update",
    "list",
    "config",
    "auth",
    "mcp",
    "help",
];

// Kimi Code 2.4, from `kimi --help`.
const KIMI_SUBCOMMANDS: &[&str] = &[
    "export",
    "fork",
    "provider",
    "session",
    "acp",
    "web",
    "server",
    "rc",
    "remote",
    "login",
    "doctor",
    "vis",
    "install-desktop",
    "app",
    "migrate",
    "import-sessions",
    "upgrade",
    "update",
    "help",
];

// Options that take the next argument, so a value that looks like a subcommand is not one.
const OPENCODE_VALUE_FLAGS: &[&str] = &[
    "--server",
    "--session",
    "-s",
    "--prompt",
    "--log-level",
    "--completions",
];

const PI_VALUE_FLAGS: &[&str] = &[
    "--provider",
    "--model",
    "--api-key",
    "--system-prompt",
    "--append-system-prompt",
    "--mode",
    "--session",
    "--session-id",
    "--fork",
    "--session-dir",
    "--name",
    "-n",
    "--models",
    "--tools",
    "-t",
    "--exclude-tools",
    "-xt",
    "--thinking",
    "--extension",
    "-e",
    "--skill",
    "--prompt-template",
    "--theme",
    "--use-theme",
    "--export",
    "--tui-mode",
];

const KIMI_VALUE_FLAGS: &[&str] = &[
    "-S",
    "--session",
    "-m",
    "--model",
    "-p",
    "--prompt",
    "--output-format",
    "--skills-dir",
    "--agent",
    "--agent-file",
    "--add-dir",
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
    Kimi,
    OpenCode,
    Pi,
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
    let setup = configure(
        Setup {
            this,
            record,
            program,
            folder,
            start,
            data_dir: &data_dir,
            cli: &cli,
            socket: socket.as_deref(),
            known: &known,
        },
        cx,
    );
    let (args, notice, warning) = match setup.await {
        Ok((args, notice, warning)) => (
            args,
            notice.map(|text| (NoticeSeverity::Info, text.to_owned())),
            warning,
        ),
        Err(error) => (
            Vec::new(),
            None,
            Some(format!(
                "kagan could not set up the agent integration, so the program starts without it: {error}"
            )),
        ),
    };
    let mut notices = Vec::new();
    if let Some((severity, text)) = notice {
        notices.push(EventKind::Notice { severity, text });
    }
    if let Some(text) = warning {
        notices.push(EventKind::Notice {
            severity: NoticeSeverity::Warning,
            text,
        });
    }
    if !notices.is_empty() {
        Workspace::events(this, record.id.clone(), notices, cx).await?;
    }
    Ok(Integration { env, args })
}

struct Setup<'a> {
    this: &'a WeakEntity<Workspace>,
    record: &'a kg_domain::Thread,
    program: &'a ResolvedProgram,
    folder: &'a Path,
    start: Start,
    data_dir: &'a Path,
    cli: &'a Path,
    socket: Option<&'a OsStr>,
    known: &'a Known,
}

async fn configure(
    input: Setup<'_>,
    cx: &mut AsyncApp,
) -> Result<(Vec<OsString>, Option<&'static str>, Option<String>)> {
    let Setup {
        this,
        record,
        program,
        folder,
        start,
        data_dir,
        cli,
        socket,
        known,
    } = input;
    let id = record.id.0.as_str();
    let extra = collect_instruct(this, record, cx).await;
    let instructions = note::read_instructions(data_dir);
    let agent = data_dir.join("agent");
    let (skills_root, skills, skill_errors) = skill::mirror(
        this,
        agent.clone(),
        folder.to_owned(),
        record.project_id.clone(),
        cx,
    )
    .await?;
    write_agent_files(
        known,
        files::AgentFiles {
            agent: &agent,
            id,
            folder,
            cli,
            socket,
            instructions: &instructions,
            extra: &extra,
            skills_root: &skills_root,
        },
        cx,
    )
    .await?;
    let roots: &[&str] = match known {
        Known::Codex => &[".agents/skills"],
        Known::OpenCode => &[".opencode/skills"],
        Known::Claude | Known::Cursor | Known::Kimi | Known::Pi => &[],
    };
    let mut warning = None;
    if let Err(error) = files::retire_legacy(folder, &agent, cx).await {
        warning = Some(format!(
            "kagan could not remove its old skill links in {}: {error}",
            folder.display()
        ));
    }
    if !skill_errors.is_empty() {
        let text = format!(
            "kagan could not use some skills: {}",
            skill_errors
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
        warning = Some(match warning {
            Some(previous) => format!("{previous}; {text}"),
            None => text,
        });
    }
    if !roots.is_empty()
        && let Err(error) =
            files::link_skills(folder, &skills_root, &agent, &skills, roots, cx).await
    {
        let text = format!(
            "kagan could not offer the composed skills in {}: {error}",
            folder.display()
        );
        warning = Some(match warning {
            Some(previous) => format!("{previous}; {text}"),
            None => text,
        });
    }
    let mut args = flags(known, id, cli, socket, &agent, &skills_root, folder);
    let (conversation, notice) =
        conversation::args(this, known, record, &program.args, folder, start, cx).await?;
    args.extend(conversation);
    Ok((args, notice, warning))
}

fn known(program: &Path) -> Option<Known> {
    match program.file_stem()?.to_str()? {
        "claude" => Some(Known::Claude),
        "codex" => Some(Known::Codex),
        "cursor-agent" | "agent" => Some(Known::Cursor),
        "kimi" => Some(Known::Kimi),
        "opencode" => Some(Known::OpenCode),
        "pi" => Some(Known::Pi),
        _ => None,
    }
}

// Claude takes no flag before a subcommand, so kagan adds nothing to a line that names one. Codex
// takes its flags before a subcommand, so its overrides always apply. A Cursor subcommand is not
// its conversation, so kagan adds nothing to that line either. The other programs take flags
// before a subcommand, and a subcommand line is not a conversation.
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
        Known::Kimi => {
            first_word(args, KIMI_VALUE_FLAGS).is_some_and(|arg| KIMI_SUBCOMMANDS.contains(&arg))
        }
        Known::OpenCode => first_word(args, OPENCODE_VALUE_FLAGS)
            .is_some_and(|arg| OPENCODE_SUBCOMMANDS.contains(&arg)),
        Known::Pi => {
            first_word(args, PI_VALUE_FLAGS).is_some_and(|arg| PI_SUBCOMMANDS.contains(&arg))
        }
        Known::Codex => false,
    }
}

fn first_word<'a>(args: &'a [OsString], value_flags: &[&str]) -> Option<&'a str> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            continue;
        };
        if text == "--" {
            return args.next().and_then(|arg| arg.to_str());
        }
        if text.starts_with('-') {
            if !text.contains('=') && value_flags.contains(&text) {
                args.next();
            }
            continue;
        }
        return Some(text);
    }
    None
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
mod note;

pub(crate) use note::{cursor_rule, note_with, read_instructions};

use files::{environment, flags, write_agent_files};

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
