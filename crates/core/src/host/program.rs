use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};

use crate::Workspace;
use kg_domain::{
    KNOWN_PROGRAMS, LISTED_WHEN_MISSING, OpError, OpResult, PROGRAM_INSTALL_FOLDERS, Program,
    ProgramList, ProgramSource, ProjectId, SavedProgram, Visibility, split_program,
};
use kg_process::{find_program, pathext, user_path};

use crate::host::database::{Database, Result};

const VERSION_LIMIT: Duration = Duration::from_secs(3);
const RECENT_LIMIT: usize = 5;

/// The programs a new thread can run: the known agents, the shell, then the recent programs.
/// Only the known agents get a version, because only they are safe to run for one.
pub(crate) async fn list(
    this: &WeakEntity<Workspace>,
    database: &Database,
    include_versions: bool,
    project_id: Option<ProjectId>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let environment = program_environment_of(this, cx);
    let home = program_home_of(this, cx);
    if let Some(environment) = &environment
        && !Path::new(&environment.shell).is_file()
    {
        return Err(OpError::Failed(format!(
            "the shell {} is missing",
            Path::new(&environment.shell).display()
        )));
    }
    let project_program = match project_id {
        Some(id) => {
            let lookup = id.clone();
            let project = database
                .call(move |store| store.project(&lookup))
                .await?
                .ok_or_else(|| crate::workspace::missing("project", &id))?;
            project.program
        }
        None => None,
    };
    let recent = database
        .call(|store| store.all_threads())
        .await?
        .into_iter()
        .filter(|thread| thread.visibility == Visibility::Visible && !thread.provider.is_empty())
        .map(|thread| thread.provider);
    // The project's program goes first, so it is listed among the recent ones.
    let recent = project_program.iter().cloned().chain(recent);
    let recent: Vec<String> = recent.fold(Vec::new(), |mut distinct, command| {
        if !distinct.contains(&command) && distinct.len() < RECENT_LIMIT {
            distinct.push(command);
        }
        distinct
    });
    let saved = database.call(|store| store.saved_programs()).await?;
    let programs = cx
        .background_executor()
        .spawn(async move {
            let (path, pathext, shell) = read_environment(environment);
            let mut list = discover(
                path.as_deref(),
                pathext.as_deref(),
                shell.as_deref(),
                home.as_deref(),
                &saved,
                &recent,
                project_program.as_deref(),
            );
            if include_versions {
                versions(&mut list);
            }
            list
        })
        .await;
    Ok(OpResult::Programs(programs))
}

/// The programs found with `path`, `pathext` and `home`, the `shell`, the `saved` programs, and the
/// `recent` commands, newest first. The default is the `project` program when it is installed,
/// else the first installed known agent. Versions are filled in separately, so this reads nothing
/// but the filesystem.
pub(crate) fn discover(
    path: Option<&OsStr>,
    pathext: Option<&OsStr>,
    shell: Option<&OsStr>,
    home: Option<&Path>,
    saved: &[SavedProgram],
    recent: &[String],
    project: Option<&str>,
) -> ProgramList {
    let mut programs = Vec::new();
    for (command, label) in KNOWN_PROGRAMS {
        let found = locate(Path::new(command), path, pathext, home);
        if found.is_none() && !LISTED_WHEN_MISSING.contains(command) {
            continue;
        }
        programs.push(Program {
            command: (*command).into(),
            label: (*label).into(),
            source: ProgramSource::Known,
            path: found,
            version: None,
            pinned: false,
        });
    }
    if let Some(shell) = shell.filter(|shell| !shell.is_empty()) {
        let shell = PathBuf::from(shell);
        let text = shell.display().to_string();
        let command = if text.contains(' ') {
            format!("'{text}'")
        } else {
            text
        };
        programs.push(Program {
            command,
            label: "Shell".into(),
            source: ProgramSource::Shell,
            path: shell.is_file().then_some(shell),
            version: None,
            pinned: false,
        });
    }
    for saved in saved {
        let Ok((name, _)) = split_program(&saved.line) else {
            continue;
        };
        programs.push(Program {
            command: saved.line.clone(),
            label: saved.name.clone(),
            source: ProgramSource::Saved,
            path: find_program(Path::new(&name), path, pathext),
            version: None,
            pinned: saved.pinned,
        });
    }
    for command in recent {
        if programs.iter().any(|program| &program.command == command) {
            continue;
        }
        let Ok((name, _)) = split_program(command) else {
            continue;
        };
        programs.push(Program {
            command: command.clone(),
            label: label_of(&name),
            source: ProgramSource::Recent,
            path: locate(Path::new(&name), path, pathext, home),
            version: None,
            pinned: false,
        });
    }
    let default = project
        .and_then(|project| {
            programs
                .iter()
                .find(|program| program.command == project && program.installed())
        })
        .or_else(|| {
            programs
                .iter()
                .find(|program| program.source == ProgramSource::Known && program.installed())
        })
        .map(|program| program.command.clone());
    ProgramList { programs, default }
}

fn label_of(name: &str) -> String {
    let base = Path::new(name)
        .file_stem()
        .map_or_else(|| name.into(), |file| file.to_string_lossy().into_owned());
    KNOWN_PROGRAMS
        .iter()
        .find(|(command, _)| *command == base)
        .map_or(base, |(_, label)| (*label).into())
}

pub(crate) fn locate(
    name: &Path,
    path: Option<&OsStr>,
    pathext: Option<&OsStr>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    find_program(name, path, pathext).or_else(|| {
        let command = name.to_str().filter(|_| name.components().count() == 1)?;
        let home = home?;
        PROGRAM_INSTALL_FOLDERS
            .iter()
            .filter(|(known, _)| *known == command)
            .find_map(|(_, folder)| find_program(&home.join(folder).join(command), None, pathext))
    })
}

pub(crate) fn versions(list: &mut ProgramList) {
    for program in &mut list.programs {
        if program.source != ProgramSource::Known {
            continue;
        }
        if let Some(path) = &program.path {
            let mut command = std::process::Command::new(path);
            command.arg("--version");
            program.version = kg_process::capture(command, VERSION_LIMIT, || false)
                .ok()
                .filter(|output| output.code == 0)
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
    }
}

fn shell_variable() -> &'static str {
    if cfg!(windows) { "COMSPEC" } else { "SHELL" }
}

/// The person's login shell: the one `environment` names, else `SHELL` (`COMSPEC` on Windows).
pub(crate) fn login_shell(environment: Option<&ProgramEnvironment>) -> OsString {
    environment
        .map(|environment| environment.shell.clone())
        .or_else(|| std::env::var_os(shell_variable()))
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into())
}

/// Where one workspace finds its programs: a PATH and a shell. A test sets them, so the list does
/// not depend on what the machine has. The app leaves them unset and reads the login PATH.
#[derive(Clone, Debug)]
pub(crate) struct ProgramEnvironment {
    pub(crate) path: OsString,
    pub(crate) shell: OsString,
}

/// The PATH `environment` names, or the login PATH when it names none.
pub(crate) fn search_path(environment: Option<&ProgramEnvironment>) -> Option<OsString> {
    environment.map_or_else(user_path, |environment| Some(environment.path.clone()))
}

#[cfg(any(test, feature = "test"))]
pub(crate) fn program_environment_of(
    this: &WeakEntity<Workspace>,
    cx: &mut AsyncApp,
) -> Option<ProgramEnvironment> {
    this.read_with(cx, |workspace, _| workspace.program_environment.clone())
        .ok()
        .flatten()
}

#[cfg(not(any(test, feature = "test")))]
pub(crate) fn program_environment_of(
    _: &WeakEntity<Workspace>,
    _: &mut AsyncApp,
) -> Option<ProgramEnvironment> {
    None
}

fn read_environment(
    environment: Option<ProgramEnvironment>,
) -> (Option<OsString>, Option<OsString>, Option<OsString>) {
    match environment {
        Some(environment) => (Some(environment.path), pathext(), Some(environment.shell)),
        None => (
            search_path(None),
            pathext(),
            std::env::var_os(shell_variable()),
        ),
    }
}

pub(crate) fn program_home_of(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Option<PathBuf> {
    this.read_with(cx, |workspace, _| workspace.home_set(home_variable()))
        .ok()
        .flatten()
}

pub(crate) fn home_variable() -> &'static str {
    if cfg!(windows) { "USERPROFILE" } else { "HOME" }
}

/// The folder `variable` names, when it is set and not empty.
pub(crate) fn home_set(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

pub(crate) fn user_home() -> Option<PathBuf> {
    std::env::var_os(home_variable()).map(PathBuf::from)
}

impl Workspace {
    /// Where a program keeps its files: a test's folder, else the folder `variable` names.
    #[cfg_attr(
        not(any(test, feature = "test")),
        expect(clippy::unused_self, reason = "only tests keep a folder per workspace")
    )]
    pub(crate) fn home_set(&self, variable: &'static str) -> Option<PathBuf> {
        #[cfg(any(test, feature = "test"))]
        if let Some(set) = self.homes.get(variable) {
            return Some(set.clone());
        }
        home_set(variable)
    }

    /// `home_set`, else `folder` under the user's home.
    pub(crate) fn home(&self, variable: &'static str, folder: &str) -> Option<PathBuf> {
        self.home_set(variable)
            .or_else(|| user_home().map(|home| home.join(folder)))
    }
}

#[cfg(all(test, unix))]
#[path = "program_test.rs"]
mod tests;
