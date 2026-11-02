use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{EventKind, NoticeSeverity, ThreadId};
use serde_json::{Value, json};

use crate::database::Result;
use crate::workspace::closed;
use crate::{SocketLocation, Workspace, skill};

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
        return Ok(Integration {
            env,
            args: Vec::new(),
        });
    };
    let setup = async {
        let mut args = arguments(this, &known, id, &cli, socket.as_deref(), &data_dir, cx).await?;
        if matches!(known, Known::Codex) {
            let skill = data_dir.join("agent").join("skills").join("kg-threads");
            if let Err(error) = link_skill(folder, &skill) {
                eprintln!(
                    "kg: could not offer kg-threads in {}: {error}",
                    folder.display()
                );
            }
        }
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

async fn arguments(
    this: &WeakEntity<Workspace>,
    known: &Known,
    id: &str,
    cli: &Path,
    socket: Option<&OsStr>,
    data_dir: &Path,
    cx: &mut AsyncApp,
) -> Result<Vec<OsString>> {
    let agent = data_dir.join("agent");
    skill::mirror(this, agent.join("skills"), cx).await?;
    match known {
        Known::Claude => {
            write_files(claude_files(&agent, id, cli, socket), cx).await?;
            let thread = agent.join("threads").join(id);
            Ok(vec![
                OsString::from("--mcp-config"),
                thread.join("mcp.json").into_os_string(),
                OsString::from("--plugin-dir"),
                agent.into_os_string(),
                OsString::from("--append-system-prompt-file"),
                thread.join("note.txt").into_os_string(),
            ])
        }
        Known::Cursor => {
            write_files(cursor_files(&agent), cx).await?;
            Ok(vec![OsString::from("--plugin-dir"), agent.into_os_string()])
        }
        Known::Codex => Ok(codex_overrides(id, cli, socket)),
    }
}

fn environment(
    thread: &str,
    project: &str,
    environment: Option<&str>,
    cli: &Path,
    socket: Option<&OsStr>,
    path: Option<&OsString>,
) -> Vec<(OsString, OsString)> {
    let mut env = vec![
        (OsString::from("KG_THREAD_ID"), OsString::from(thread)),
        (OsString::from("KG_PROJECT_ID"), OsString::from(project)),
        (OsString::from("KG_CLI"), cli.as_os_str().to_owned()),
    ];
    if let Some(environment) = environment {
        env.push((
            OsString::from("KG_ENVIRONMENT_ID"),
            OsString::from(environment),
        ));
    }
    if let Some(socket) = socket {
        env.push((OsString::from("KG_SOCKET"), socket.to_owned()));
    }
    let folders = cli
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .chain(path.into_iter().flat_map(std::env::split_paths));
    // If the folders cannot be joined, PATH is left as the app has it.
    if let Ok(joined) = std::env::join_paths(folders) {
        env.push((OsString::from("PATH"), joined));
    }
    env
}

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

fn cursor_files(agent: &Path) -> Vec<(PathBuf, String)> {
    vec![
        (
            agent.join(".cursor-plugin").join("plugin.json"),
            "{\"name\": \"kagan\"}\n".into(),
        ),
        (
            agent.join("rules").join("kagan.mdc"),
            format!(
                "---\ndescription: This thread runs inside kagan\nalwaysApply: true\n---\n\n{}\n\nThis thread's id is the environment variable KG_THREAD_ID.\n",
                introduction()
            ),
        ),
    ]
}

// Codex discovers skills in the working folder and has no per-run skills directory. The link is
// the skill. A directory the person already put at this path is left alone. Git exclude keeps
// the link out of the change list.
fn link_skill(folder: &Path, skill: &Path) -> std::io::Result<()> {
    let skill = dunce::canonicalize(skill)?;
    let link = folder.join(".agents").join("skills").join("kg-threads");
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::symlink_metadata(&link) {
        Ok(_) => match std::fs::read_link(&link) {
            Ok(current) if current == skill => {}
            Ok(current) if kagan_skill_link(&current) => std::fs::remove_file(&link)?,
            Ok(_) | Err(_) => return Ok(()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if !link.exists() {
        symlink_dir(&skill, &link)?;
    }
    hide_from_git(folder)
}

fn kagan_skill_link(path: &Path) -> bool {
    let mut parts = path.components().rev();
    parts
        .next()
        .is_some_and(|part| part.as_os_str() == "kg-threads")
        && parts
            .next()
            .is_some_and(|part| part.as_os_str() == "skills")
        && parts.next().is_some_and(|part| part.as_os_str() == "agent")
}

fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
}

fn hide_from_git(folder: &Path) -> std::io::Result<()> {
    let Some(exclude) = git_exclude(folder) else {
        return Ok(());
    };
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = match std::fs::read_to_string(&exclude) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    if text.lines().any(|line| line.trim() == SKILL_LINK) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(SKILL_LINK);
    text.push('\n');
    std::fs::write(exclude, text)
}

fn git_exclude(folder: &Path) -> Option<PathBuf> {
    let git = folder.join(".git");
    if git.is_dir() {
        return Some(git.join("info").join("exclude"));
    }
    if !git.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&git).ok()?;
    let dir = text
        .lines()
        .find_map(|line| line.strip_prefix("gitdir: "))?;
    let dir = Path::new(dir.trim());
    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        folder.join(dir)
    };
    Some(dir.join("info").join("exclude"))
}

fn claude_files(
    agent: &Path,
    id: &str,
    cli: &Path,
    socket: Option<&OsStr>,
) -> Vec<(PathBuf, String)> {
    let thread = agent.join("threads").join(id);
    vec![
        (
            agent.join(".claude-plugin").join("plugin.json"),
            "{\"name\": \"kagan\"}\n".into(),
        ),
        (thread.join("mcp.json"), mcp_config(id, cli, socket)),
        (thread.join("note.txt"), note(id)),
    ]
}

async fn write_files(files: Vec<(PathBuf, String)>, cx: &mut AsyncApp) -> Result<()> {
    cx.background_spawn(async move {
        for (path, text) in files {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, text)?;
        }
        Ok(())
    })
    .await
}

fn mcp_config(id: &str, cli: &Path, socket: Option<&OsStr>) -> String {
    let mut server = json!({ "command": cli.to_string_lossy(), "args": ["mcp", "--thread", id] });
    if let Some(socket) = socket {
        server["env"] = json!({ "KG_SOCKET": socket.to_string_lossy() });
    }
    json!({ "mcpServers": { "kagan": server } }).to_string()
}

fn codex_overrides(id: &str, cli: &Path, socket: Option<&OsStr>) -> Vec<OsString> {
    let mut items = vec![
        format!("mcp_servers.kagan.command={}", toml(&cli.to_string_lossy())),
        format!(
            "mcp_servers.kagan.args=[{}, {}, {}]",
            toml("mcp"),
            toml("--thread"),
            toml(id)
        ),
    ];
    if let Some(socket) = socket {
        items.push(format!(
            "mcp_servers.kagan.env={{KG_SOCKET = {}}}",
            toml(&socket.to_string_lossy())
        ));
    }
    items
        .iter()
        .flat_map(|item| ["-c", item.as_str()])
        .map(OsString::from)
        .collect()
}

fn introduction() -> &'static str {
    "You are working inside kagan, a workspace for programs in threads. `kg` is on PATH. Run `kg status --json` for this thread, its project, and its environment. Run `kg guide` for the map.\n\
     \n\
     Follow the skill kg-threads before you create a thread, or paste into, wait on, stop, or read another one. Stay in this thread unless the person asked for another.\n\
     \n\
     Plugin powers and a delete are approved by the person in the app. `kg ui` cannot approve them. `kg thread ask` raises a question, and the answer arrives as `answered`. The program may set the title once with `kg thread rename`. A later one says the title was kept, and a name set in the window stays."
}

pub(crate) fn note(id: &str) -> String {
    format!(
        "{}\n\nYou are running inside kagan thread {id}.",
        introduction()
    )
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
