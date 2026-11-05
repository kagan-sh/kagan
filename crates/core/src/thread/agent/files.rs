use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp};
use serde_json::json;

use crate::host::database::Result;

use super::{Known, introduction, note_with, toml};

pub(super) struct AgentFiles<'a> {
    pub(super) agent: &'a Path,
    pub(super) id: &'a str,
    pub(super) cli: &'a Path,
    pub(super) socket: Option<&'a OsStr>,
    pub(super) extra: &'a str,
    pub(super) skills_root: &'a Path,
}

pub(super) async fn write_agent_files(
    known: &Known,
    files: AgentFiles<'_>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let AgentFiles {
        agent,
        id,
        cli,
        socket,
        extra,
        skills_root,
    } = files;
    match known {
        Known::Claude => write_files(claude_files(agent, id, cli, socket, extra), cx).await,
        Known::Cursor => {
            let dir = cursor_dir(agent, id);
            write_files(cursor_files(&dir, extra), cx).await?;
            let skills = skills_root.to_owned();
            cx.background_spawn(async move { link_cursor_skills(&dir, &skills) })
                .await?;
            Ok(())
        }
        Known::Codex | Known::Kimi | Known::OpenCode | Known::Pi => Ok(()),
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
    skills_root: &Path,
    folder: &Path,
) -> Vec<OsString> {
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
                thread.join("note.txt").into_os_string(),
            ]
        }
        Known::Cursor => vec![
            OsString::from("--plugin-dir"),
            cursor_dir(agent, id).into_os_string(),
        ],
        Known::Codex => codex_overrides(id, cli, socket),
        Known::Kimi => kimi_skills(skills_root, folder),
        Known::Pi => vec![
            OsString::from("--skill"),
            skills_root.to_owned().into_os_string(),
        ],
        Known::OpenCode => Vec::new(),
    }
}

fn kimi_skills(skills_root: &Path, folder: &Path) -> Vec<OsString> {
    let mut native = Vec::new();
    if let Some(home) = std::env::var_os("KIMI_CODE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        native.push(home.join("skills"));
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
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
        let _turn = LINKS
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for root in &roots {
            let (owned, removed) = link_root(&folder, &skills_root, &owned_root, &names, root)?;
            hide_from_git(&folder, root, &owned, &removed)?;
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
        let _turn = LINKS
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = folder.join(".agents").join("skills");
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut removed = Vec::new();
        for entry in entries {
            let entry = entry?;
            if own_link(&entry.path(), &legacy) {
                remove_link(&entry.path())?;
                removed.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        if !removed.is_empty() {
            hide_from_git(&folder, ".agents/skills", &[], &removed)?;
        }
        Ok(())
    })
    .await
}

static LINKS: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

fn link_root(
    folder: &Path,
    skills_root: &Path,
    owned_root: &Path,
    names: &[String],
    relative: &str,
) -> std::io::Result<(Vec<String>, Vec<String>)> {
    let root = folder.join(relative);
    std::fs::create_dir_all(&root)?;
    let mut removed = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !names.contains(&name) && own_link(&entry.path(), owned_root) {
            remove_link(&entry.path())?;
            removed.push(name);
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
                    remove_link(&link)?;
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
    Ok((owned, removed))
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

fn remove_link(link: &Path) -> std::io::Result<()> {
    std::fs::remove_file(link)
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

fn hide_from_git(
    folder: &Path,
    root: &str,
    owned: &[String],
    removed: &[String],
) -> std::io::Result<()> {
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
    let prefix = format!("{root}/");
    let mut lines = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let keep = match line.trim().strip_prefix(&prefix) {
            Some(name) if !name.is_empty() && !name.contains('/') => {
                !removed.iter().any(|removed| removed == name)
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
    if changed {
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        std::fs::write(exclude, text)?;
    }
    Ok(())
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
    extra: &str,
) -> Vec<(PathBuf, String)> {
    let thread = agent.join("threads").join(id);
    vec![
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{git_exclude, hide_from_git, link_root};

    fn skill(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(&path).unwrap();
        dunce::canonicalize(path).unwrap()
    }

    fn folder(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.join(".git/info")).unwrap();
        path
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[cfg(unix)]
    #[test]
    fn link_root_links_the_skill_and_hides_only_that_link_from_git() {
        let dir = tempfile::tempdir().unwrap();
        let _source = skill(dir.path(), "source");
        let work = folder(dir.path(), "work");
        let root = ".agents/skills";

        let (owned, removed) =
            link_root(&work, dir.path(), dir.path(), &names(&["kg-threads"]), root).unwrap();
        hide_from_git(&work, root, &owned, &removed).unwrap();
        assert_eq!(
            std::fs::read_link(work.join(".agents/skills/kg-threads")).unwrap(),
            dir.path().join("kg-threads")
        );
        let exclude = work.join(".git/info/exclude");
        assert_eq!(
            std::fs::read_to_string(&exclude).unwrap(),
            ".agents/skills/kg-threads\n"
        );

        let (owned, removed) =
            link_root(&work, dir.path(), dir.path(), &names(&["kg-threads"]), root).unwrap();
        hide_from_git(&work, root, &owned, &removed).unwrap();
        assert_eq!(
            std::fs::read_to_string(&exclude)
                .unwrap()
                .matches(".agents/skills/kg-threads")
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn link_root_leaves_a_person_s_folder_or_unrelated_link_alone() {
        let dir = tempfile::tempdir().unwrap();
        let _source = skill(dir.path(), "source");
        let root = ".agents/skills";

        let folder_path = dir.path().join("folder");
        std::fs::create_dir_all(folder_path.join(".agents/skills/kg-threads")).unwrap();
        let (owned, _) = link_root(
            &folder_path,
            dir.path(),
            dir.path(),
            &names(&["kg-threads"]),
            root,
        )
        .unwrap();
        assert!(owned.is_empty());
        assert!(!folder_path.join(".agents/skills/kg-threads").is_symlink());

        let other = skill(dir.path(), "other");
        let linked = dir.path().join("linked");
        std::fs::create_dir_all(linked.join(".agents/skills")).unwrap();
        std::os::unix::fs::symlink(&other, linked.join(".agents/skills/kg-threads")).unwrap();
        let (owned, _) = link_root(
            &linked,
            dir.path(),
            dir.path(),
            &names(&["kg-threads"]),
            root,
        )
        .unwrap();
        assert!(owned.is_empty());
        assert_eq!(
            std::fs::read_link(linked.join(".agents/skills/kg-threads")).unwrap(),
            other
        );
    }

    #[cfg(unix)]
    #[test]
    fn link_root_removes_a_kagan_link_the_catalog_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let _source = skill(dir.path(), "source");
        let work = folder(dir.path(), "work");
        std::fs::create_dir_all(work.join(".agents/skills")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("gone"), work.join(".agents/skills/gone"))
            .unwrap();

        let (owned, removed) = link_root(
            &work,
            dir.path(),
            dir.path(),
            &names(&["kept"]),
            ".agents/skills",
        )
        .unwrap();
        assert!(removed.contains(&"gone".to_owned()));
        hide_from_git(&work, ".agents/skills", &owned, &removed).unwrap();
        assert!(!work.join(".agents/skills/gone").exists());
        assert!(work.join(".agents/skills/kept").is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn hide_from_git_removes_a_stale_owned_line_and_keeps_a_person_line() {
        let dir = tempfile::tempdir().unwrap();
        let work = folder(dir.path(), "work");
        let exclude = work.join(".git/info/exclude");
        std::fs::create_dir_all(work.join(".agents/skills/mine")).unwrap();
        std::fs::write(
            &exclude,
            "target\n.agents/skills/gone\n.agents/skills/mine\n.agents/skills/future\n",
        )
        .unwrap();

        hide_from_git(
            &work,
            ".agents/skills",
            &names(&["kept"]),
            &names(&["gone"]),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&exclude).unwrap(),
            "target\n.agents/skills/mine\n.agents/skills/future\n.agents/skills/kept\n"
        );
    }

    #[test]
    fn hide_from_git_does_nothing_without_a_git_folder() {
        let dir = tempfile::tempdir().unwrap();
        hide_from_git(dir.path(), ".agents/skills", &names(&["one"]), &[]).unwrap();
        assert!(!dir.path().join(".git/info/exclude").exists());
    }

    #[test]
    fn hide_from_git_refuses_an_exclude_it_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let work = folder(dir.path(), "work");
        let exclude = work.join(".git/info/exclude");
        std::fs::remove_file(&exclude).ok();
        std::fs::create_dir(&exclude).unwrap();
        assert!(hide_from_git(&work, ".agents/skills", &names(&["one"]), &[]).is_err());
    }

    #[test]
    fn kimi_re_adds_its_user_roots() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".agents/skills/probe")).unwrap();
        std::fs::create_dir_all(dir.path().join(".kimi-code/skills/probe")).unwrap();
        let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let previous_home = std::env::var_os(home_var);
        let previous_kimi = std::env::var_os("KIMI_CODE_HOME");
        // SAFETY: nextest runs each test in its own process, so this test owns these
        // process-global variables while the guard restores them.
        unsafe {
            std::env::set_var(home_var, dir.path());
            std::env::remove_var("KIMI_CODE_HOME");
        }
        let args = super::kimi_skills(&dir.path().join("catalog"), dir.path());
        // SAFETY: see above.
        unsafe {
            match previous_home {
                Some(value) => std::env::set_var(home_var, value),
                None => std::env::remove_var(home_var),
            }
            match previous_kimi {
                Some(value) => std::env::set_var("KIMI_CODE_HOME", value),
                None => std::env::remove_var("KIMI_CODE_HOME"),
            }
        }
        let dirs = args
            .chunks(2)
            .map(|pair| pair[1].clone())
            .collect::<Vec<_>>();
        assert!(
            dirs.iter()
                .any(|arg| arg == &dir.path().join(".agents/skills").into_os_string())
        );
        assert!(
            dirs.iter()
                .any(|arg| arg == &dir.path().join(".kimi-code/skills").into_os_string())
        );
    }

    #[test]
    fn a_linked_worktree_uses_the_common_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |cwd: &Path, args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(cwd)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        git(&repo, &["init"]);
        std::fs::write(repo.join("README.md"), "test\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.test",
                "commit",
                "-m",
                "initial",
            ],
        );
        let worktree = dir.path().join("worktree");
        git(
            &repo,
            &["worktree", "add", worktree.to_str().unwrap(), "-b", "test"],
        );

        let exclude = dunce::canonicalize(git_exclude(&worktree).unwrap()).unwrap();
        assert_eq!(
            exclude,
            dunce::canonicalize(repo.join(".git/info/exclude")).unwrap()
        );
        std::fs::create_dir_all(worktree.join(".agents/skills")).unwrap();
        std::fs::write(worktree.join(".agents/skills/one"), "one\n").unwrap();
        std::fs::write(&exclude, ".agents/skills/one\n").unwrap();
        let status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&worktree)
            .output()
            .unwrap();
        assert!(
            status.stdout.is_empty(),
            "{}",
            String::from_utf8_lossy(&status.stdout)
        );
    }

    #[test]
    fn git_exclude_follows_a_gitdir_file_and_skips_a_folder_without_git() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("info")).unwrap();
        let tree = dir.path().join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        assert_eq!(git_exclude(&tree), Some(real.join("info/exclude")));

        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(git_exclude(&plain), None);
    }
}
