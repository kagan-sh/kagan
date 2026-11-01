use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{Conversation, EventKind, NoticeSeverity, ThreadId};
use serde_json::{Value, json};

use crate::database::Result;
use crate::workspace::closed;
use crate::{SocketLocation, Workspace, skill};

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

// Flags with which the person already picks the conversation, so kagan adds none of its own.
const CLAUDE_CONVERSATION_FLAGS: &[&str] = &[
    "-c",
    "--continue",
    "-r",
    "--resume",
    "--session-id",
    "--fork-session",
    "--from-pr",
    "--teleport",
    "-p",
    "--print",
    "--no-session-persistence",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Start {
    First,
    Again,
}

enum Last {
    None,
    Gone,
    Kept(Conversation),
}

pub(crate) struct Integration {
    pub(crate) env: Vec<(OsString, OsString)>,
    pub(crate) args: Vec<OsString>,
}

enum Known {
    Claude,
    Codex,
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
        let (conversation, notice) =
            conversation_args(this, &known, record, &program.args, start, cx).await?;
        args.extend(conversation);
        Result::Ok((args, notice.map(str::to_owned)))
    };
    let (args, notice) = setup.await.unwrap_or_else(|error| {
        (
            Vec::new(),
            Some(format!(
                "kagan could not set up the agent integration, so the program starts without it: {error}"
            )),
        )
    });
    if let Some(text) = notice {
        let notice = EventKind::Notice {
            severity: NoticeSeverity::Warning,
            text,
        };
        Workspace::events(this, record.id.clone(), vec![notice], cx).await?;
    }
    Ok(Integration { env, args })
}

async fn conversation_args(
    this: &WeakEntity<Workspace>,
    known: &Known,
    record: &kg_domain::Thread,
    args: &[OsString],
    start: Start,
    cx: &mut AsyncApp,
) -> Result<(Vec<OsString>, Option<&'static str>)> {
    match known {
        Known::Codex => Ok((
            Vec::new(),
            (start == Start::Again)
                .then_some("kagan cannot resume a Codex conversation, so Codex starts a new one."),
        )),
        Known::Claude if picks_conversation(args) => Ok((Vec::new(), None)),
        Known::Claude => {
            let (database, config) = this
                .update(cx, |workspace, _| {
                    (workspace.database.clone(), workspace.claude_config.clone())
                })
                .map_err(closed)?;
            let id = record.id.clone();
            let stored = database.call(move |store| store.conversation(&id)).await?;
            let last = match stored {
                None => Last::None,
                Some(conversation) => {
                    cx.background_spawn(async move {
                        let config = config.or_else(claude_config);
                        if config.is_some_and(|config| claude_kept(&config, conversation)) {
                            Last::Kept(conversation)
                        } else {
                            Last::Gone
                        }
                    })
                    .await
                }
            };
            let notice = match last {
                Last::Kept(conversation) => {
                    return Ok((
                        vec![OsString::from("--resume"), conversation.to_string().into()],
                        None,
                    ));
                }
                Last::None => {
                    "This thread has no saved conversation to resume, so the agent starts a new one."
                }
                Last::Gone => {
                    "kagan could not find the agent's last conversation, so it starts a new one."
                }
            };
            let conversation = Conversation::fresh();
            let id = record.id.clone();
            database
                .call(move |store| store.set_conversation(&id, conversation))
                .await?;
            Ok((
                vec![
                    OsString::from("--session-id"),
                    conversation.to_string().into(),
                ],
                (start == Start::Again).then_some(notice),
            ))
        }
    }
}

fn picks_conversation(args: &[OsString]) -> bool {
    args.iter().filter_map(|arg| arg.to_str()).any(|arg| {
        CLAUDE_CONVERSATION_FLAGS.iter().any(|flag| {
            arg.strip_prefix(flag)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
        })
    })
}

fn claude_config() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(".claude"))
        })
}

// Claude Code writes a conversation to `projects/<folder>/<id>.jsonl` once something is said, and
// `--resume` with an id it has not written fails. The folder name is Claude's own encoding of the
// working folder, so every folder is searched for the id.
fn claude_kept(config: &Path, conversation: Conversation) -> bool {
    let name = format!("{conversation}.jsonl");
    std::fs::read_dir(config.join("projects"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|folder| folder.path().join(&name).is_file())
}

fn known(program: &Path) -> Option<Known> {
    match program.file_stem()?.to_str()? {
        "claude" => Some(Known::Claude),
        "codex" => Some(Known::Codex),
        _ => None,
    }
}

// Claude takes no flag before a subcommand, so kagan adds nothing to a line that names one. Codex
// takes its flags before a subcommand, so its overrides always apply.
fn subcommand(known: &Known, args: &[OsString]) -> bool {
    let Known::Claude = known else {
        return false;
    };
    args.iter()
        .find(|arg| {
            !CLAUDE_PERMISSION_FLAGS
                .iter()
                .any(|flag| arg.as_os_str() == OsStr::new(flag))
        })
        .and_then(|arg| arg.to_str())
        .is_some_and(|arg| CLAUDE_SUBCOMMANDS.contains(&arg))
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
    match known {
        Known::Claude => {
            let agent = data_dir.join("agent");
            skill::mirror(this, agent.join("skills"), cx).await?;
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

pub(crate) fn note(id: &str) -> String {
    format!(
        "You are running inside kagan thread {id}. Use `kg` (run `kg guide`) for threads, projects, plugins and skills. Plugin powers are approved by the person. The program may set the title once with `kg thread rename`. A later one says the title was kept, and a name set in the window stays. `kg thread ask` raises a question, and the answer arrives as `answered`."
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
