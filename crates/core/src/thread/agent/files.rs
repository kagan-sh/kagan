use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp};
use serde_json::json;

use crate::host::database::Result;

use super::{Known, cursor_rule, note_with};

const CURSOR_RULE: &str = ".cursor/rules/kagan.mdc";

#[derive(Clone, Copy)]
pub(super) struct AgentFiles<'a> {
    pub(super) agent: &'a Path,
    pub(super) id: &'a str,
    pub(super) folder: &'a Path,
    pub(super) cli: &'a Path,
    pub(super) socket: Option<&'a OsStr>,
    pub(super) instructions: &'a str,
    pub(super) extra: &'a str,
    pub(super) skills_root: &'a Path,
    pub(super) kimi_home: Option<&'a Path>,
}

pub(super) async fn write_agent_files(
    known: &Known,
    files: AgentFiles<'_>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let AgentFiles {
        agent,
        id,
        folder,
        cli,
        socket,
        instructions,
        extra,
        skills_root,
        kimi_home: _,
    } = files;
    match known {
        Known::Claude => {
            write_files(
                claude_files(agent, id, cli, socket, instructions, extra),
                cx,
            )
            .await
        }
        Known::Cursor => {
            let dir = cursor_dir(agent, id);
            let rule = cursor_rule(instructions, extra);
            write_files(cursor_files(&dir, &rule), cx).await?;
            let skills = skills_root.to_owned();
            let folder = folder.to_owned();
            cx.background_spawn(async move {
                write_workspace_rule(&folder, &rule)?;
                exclude_cursor_rule(&folder)?;
                link_cursor_skills(&dir, &skills)
            })
            .await?;
            Ok(())
        }
        Known::Pi => write_files(pi_files(agent, id, instructions, extra), cx).await,
        Known::Codex | Known::Kimi | Known::OpenCode => Ok(()),
    }
}

fn cursor_dir(agent: &Path, id: &str) -> PathBuf {
    agent.join("threads").join(id).join("cursor")
}

pub(super) fn flags(known: &Known, files: &AgentFiles) -> Vec<OsString> {
    let AgentFiles {
        agent,
        id,
        folder,
        cli,
        socket,
        skills_root,
        kimi_home,
        ..
    } = *files;
    match known {
        Known::Claude => {
            let thread = agent.join("threads").join(id);
            let plugin = skills_root.parent().unwrap_or(agent);
            vec![
                OsString::from("--mcp-config"),
                thread.join("mcp.json").into_os_string(),
                OsString::from("--plugin-dir"),
                plugin.to_path_buf().into_os_string(),
                OsString::from("--append-system-prompt-file"),
                note_path(agent, id).into_os_string(),
                OsString::from("--disallowed-tools"),
                OsString::from("Task,Agent"),
            ]
        }
        Known::Cursor => vec![
            OsString::from("--plugin-dir"),
            cursor_dir(agent, id).into_os_string(),
        ],
        Known::Codex => {
            let mut flags = vec![
                OsString::from("--disable"),
                OsString::from("multi_agent"),
                OsString::from("--disable"),
                OsString::from("multi_agent_v2"),
            ];
            flags.extend(codex_overrides(id, cli, socket));
            flags
        }
        Known::Kimi => kimi_skills(skills_root, folder, kimi_home),
        Known::Pi => vec![
            OsString::from("--skill"),
            skills_root.to_owned().into_os_string(),
            OsString::from("--append-system-prompt"),
            note_path(agent, id).into_os_string(),
        ],
        Known::OpenCode => Vec::new(),
    }
}

fn kimi_skills(skills_root: &Path, folder: &Path, kimi_home: Option<&Path>) -> Vec<OsString> {
    let mut native = Vec::new();
    if let Some(home) = kimi_home {
        native.push(home.join("skills"));
    }
    if let Some(home) = crate::host::program::user_home() {
        native.push(home.join(".kimi-code/skills"));
        native.push(home.join(".kimi/skills"));
        native.push(home.join(".agents/skills"));
    }
    native.push(folder.join(".kimi-code/skills"));
    native.push(folder.join(".agents/skills"));
    let mut args = Vec::new();
    for dir in native.into_iter().filter(|dir| dir.is_dir()) {
        args.push(OsString::from("--skills-dir"));
        args.push(dir.into_os_string());
    }
    args.push(OsString::from("--skills-dir"));
    args.push(skills_root.to_owned().into_os_string());
    args
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
fn cursor_files(dir: &Path, rule: &str) -> Vec<(PathBuf, String)> {
    vec![
        (
            dir.join(".cursor-plugin").join("plugin.json"),
            "{\"name\": \"kagan\"}\n".into(),
        ),
        (dir.join("rules").join("kagan.mdc"), rule.to_owned()),
    ]
}

fn write_workspace_rule(folder: &Path, rule: &str) -> std::io::Result<()> {
    let path = folder.join(CURSOR_RULE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, rule)
}

fn exclude_cursor_rule(folder: &Path) -> std::io::Result<()> {
    let _turn = links_turn();
    edit_exclude(folder, |text| {
        if text.lines().any(|line| line.trim() == CURSOR_RULE) {
            return None;
        }
        let mut next = text.to_owned();
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(CURSOR_RULE);
        next.push('\n');
        Some(next)
    })
}

fn note_path(agent: &Path, id: &str) -> PathBuf {
    agent.join("threads").join(id).join("note.txt")
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

// Codex and OpenCode read skills from the working folder and have no per-run skills
// directory. Kagan links each composed skill into that folder and leaves any entry the person
// already put at the same name alone. Git exclude names only the links Kagan owns.
pub(super) async fn link_skills(
    folder: &Path,
    skills_root: &Path,
    owned_root: &Path,
    names: &[String],
    roots: &[&str],
    cx: &mut AsyncApp,
) -> std::io::Result<()> {
    let folder = folder.to_owned();
    let skills_root = skills_root.to_owned();
    let owned_root = owned_root.to_owned();
    let names = names.to_vec();
    let roots = roots
        .iter()
        .map(|root| (*root).to_owned())
        .collect::<Vec<_>>();
    cx.background_spawn(async move {
        let _turn = links_turn();
        for root in &roots {
            let owned = link_root(&folder, &skills_root, &owned_root, &names, root)?;
            hide_from_git(&folder, root, &owned)?;
        }
        Ok(())
    })
    .await
}

pub(super) async fn retire_legacy(
    folder: &Path,
    agent: &Path,
    cx: &mut AsyncApp,
) -> std::io::Result<()> {
    let folder = folder.to_owned();
    let legacy = agent.join("skills");
    cx.background_spawn(async move {
        let _turn = links_turn();
        let root = folder.join(".agents").join("skills");
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            if own_link(&entry.path(), &legacy) {
                std::fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    })
    .await
}

fn links_turn() -> std::sync::MutexGuard<'static, ()> {
    static LINKS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LINKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn link_root(
    folder: &Path,
    skills_root: &Path,
    owned_root: &Path,
    names: &[String],
    relative: &str,
) -> std::io::Result<Vec<String>> {
    let root = folder.join(relative);
    std::fs::create_dir_all(&root)?;
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !names.contains(&name) && own_link(&entry.path(), owned_root) {
            std::fs::remove_file(entry.path())?;
        }
    }
    let mut owned = Vec::new();
    for name in names {
        let link = root.join(name);
        let target = skills_root.join(name);
        match std::fs::symlink_metadata(&link) {
            Ok(meta) if meta.file_type().is_symlink() => {
                if std::fs::read_link(&link).is_ok_and(|current| current == target) {
                    owned.push(name.clone());
                    continue;
                }
                if own_link(&link, owned_root) {
                    std::fs::remove_file(&link)?;
                } else {
                    continue;
                }
            }
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if !link.exists() {
            symlink_dir(&target, &link)?;
        }
        if own_link(&link, owned_root) {
            owned.push(name.clone());
        }
    }
    Ok(owned)
}

fn own_link(link: &Path, owned_root: &Path) -> bool {
    std::fs::read_link(link).is_ok_and(|target| {
        let target = if target.is_absolute() {
            target
        } else {
            link.parent()
                .map(|parent| parent.join(&target))
                .unwrap_or(target)
        };
        target.starts_with(owned_root)
    })
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

fn hide_from_git(folder: &Path, root: &str, owned: &[String]) -> std::io::Result<()> {
    edit_exclude(folder, |text| {
        let prefix = format!("{root}/");
        let mut lines = Vec::new();
        let mut changed = false;
        for line in text.lines() {
            let keep = match line.trim().strip_prefix(&prefix) {
                Some(name) if !name.is_empty() && !name.contains('/') => {
                    let link = folder.join(root).join(name);
                    !matches!(
                        std::fs::symlink_metadata(&link),
                        Ok(meta) if !meta.file_type().is_symlink()
                    )
                }
                _ => true,
            };
            if keep {
                lines.push(line.to_owned());
            } else {
                changed = true;
            }
        }
        for name in owned {
            let line = format!("{root}/{name}");
            if !lines.iter().any(|existing| existing.trim() == line) {
                lines.push(line);
                changed = true;
            }
        }
        changed.then(|| {
            let mut text = lines.join("\n");
            if !text.is_empty() {
                text.push('\n');
            }
            text
        })
    })
}

/// Rewrites the folder's git exclude file when `edit` returns new text. A folder with no git does nothing.
fn edit_exclude(folder: &Path, edit: impl FnOnce(&str) -> Option<String>) -> std::io::Result<()> {
    let Some(exclude) = git_exclude(folder) else {
        return Ok(());
    };
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = match std::fs::read_to_string(&exclude) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    match edit(&text) {
        Some(next) => std::fs::write(exclude, next),
        None => Ok(()),
    }
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
    let common = std::fs::read_to_string(dir.join("commondir"))
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .map(|text| {
            let path = Path::new(&text);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                dir.join(path)
            }
        })
        .map_or_else(
            || dir.clone(),
            |path| dunce::canonicalize(&path).unwrap_or(path),
        );
    Some(common.join("info").join("exclude"))
}

fn claude_files(
    agent: &Path,
    id: &str,
    cli: &Path,
    socket: Option<&OsStr>,
    instructions: &str,
    extra: &str,
) -> Vec<(PathBuf, String)> {
    vec![
        (
            agent.join("threads").join(id).join("mcp.json"),
            mcp_config(id, cli, socket),
        ),
        (note_path(agent, id), note_with(id, instructions, extra)),
    ]
}

fn pi_files(agent: &Path, id: &str, instructions: &str, extra: &str) -> Vec<(PathBuf, String)> {
    vec![(note_path(agent, id), note_with(id, instructions, extra))]
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

#[cfg(test)]
#[path = "files_test.rs"]
mod tests;

// A literal string takes backslashes and spaces as they are, so Windows paths need no escaping.
pub(super) fn toml(text: &str) -> String {
    if text.contains('\'') {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{text}'")
    }
}
