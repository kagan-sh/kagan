use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp};
use serde_json::json;

use crate::host::database::Result;

use super::{Known, SKILL_LINK, introduction, note_with, toml};

pub(super) async fn write_agent_files(
    known: &Known,
    agent: &Path,
    id: &str,
    cli: &Path,
    socket: Option<&OsStr>,
    extra: &str,
    cx: &mut AsyncApp,
) -> Result<()> {
    match known {
        Known::Claude => write_files(claude_files(agent, id, cli, socket, extra), cx).await,
        Known::Cursor => {
            let dir = cursor_dir(agent, id);
            write_files(cursor_files(&dir, extra), cx).await?;
            let skills = agent.join("skills");
            cx.background_spawn(async move { link_cursor_skills(&dir, &skills) })
                .await?;
            Ok(())
        }
        Known::Codex => Ok(()),
    }
}

fn cursor_dir(agent: &Path, id: &str) -> PathBuf {
    agent.join("threads").join(id).join("cursor")
}

pub(super) fn flags(
    known: &Known,
    id: &str,
    cli: &Path,
    socket: Option<&OsStr>,
    agent: &Path,
) -> Vec<OsString> {
    match known {
        Known::Claude => {
            let thread = agent.join("threads").join(id);
            vec![
                OsString::from("--mcp-config"),
                thread.join("mcp.json").into_os_string(),
                OsString::from("--plugin-dir"),
                agent.to_path_buf().into_os_string(),
                OsString::from("--append-system-prompt-file"),
                thread.join("note.txt").into_os_string(),
            ]
        }
        Known::Cursor => vec![
            OsString::from("--plugin-dir"),
            cursor_dir(agent, id).into_os_string(),
        ],
        Known::Codex => codex_overrides(id, cli, socket),
    }
}

pub(super) fn environment(
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
fn cursor_files(dir: &Path, extra: &str) -> Vec<(PathBuf, String)> {
    let mut body = format!(
        "---\ndescription: This thread runs inside kagan\nalwaysApply: true\n---\n\n{}\n\nThis thread's id is the environment variable KG_THREAD_ID.\n",
        introduction()
    );
    if !extra.is_empty() {
        body.push('\n');
        body.push_str(extra);
        body.push('\n');
    }
    vec![
        (
            dir.join(".cursor-plugin").join("plugin.json"),
            "{\"name\": \"kagan\"}\n".into(),
        ),
        (dir.join("rules").join("kagan.mdc"), body),
    ]
}

fn link_cursor_skills(dir: &Path, skills: &Path) -> std::io::Result<()> {
    let skills = match dunce::canonicalize(skills) {
        Ok(skills) => skills,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let link = dir.join("skills");
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(&link) {
            Ok(current) if current == skills => return Ok(()),
            Ok(_) | Err(_) => std::fs::remove_file(&link)?,
        },
        Ok(_) => std::fs::remove_dir_all(&link)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    symlink_dir(&skills, &link)
}

// Codex discovers skills in the working folder and has no per-run skills directory. The link is
// the skill. A directory the person already put at this path is left alone. Git exclude keeps
// the link out of the change list.
pub(super) fn link_skill(folder: &Path, skill: &Path) -> std::io::Result<()> {
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
    extra: &str,
) -> Vec<(PathBuf, String)> {
    let thread = agent.join("threads").join(id);
    vec![
        (
            agent.join(".claude-plugin").join("plugin.json"),
            "{\"name\": \"kagan\"}\n".into(),
        ),
        (thread.join("mcp.json"), mcp_config(id, cli, socket)),
        (thread.join("note.txt"), note_with(id, extra)),
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
