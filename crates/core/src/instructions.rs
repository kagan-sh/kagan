//! The agent rules: the global file in the data folder, and each project's file in its folder.
//! A thread's program is told both when it starts.

use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{INSTRUCTIONS_MAX, InstructionsOp, OpError, OpResult, ProjectId};

use crate::Workspace;
use crate::workspace::{closed, missing};

const GLOBAL_HEADING: &str = "Instructions from your kagan instructions file:";
const PROJECT_HEADING: &str = "Instructions from this project's kagan instructions file:";

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: InstructionsOp,
    cx: &mut AsyncApp,
) -> Result<OpResult, OpError> {
    let project = match &op {
        InstructionsOp::Get { project }
        | InstructionsOp::Set { project, .. }
        | InstructionsOp::Clear { project } => project.clone(),
    };
    let path = file_for(this, project.as_ref(), cx)?;
    match op {
        InstructionsOp::Get { .. } => {
            let text = cx.background_spawn(async move { read(&path) }).await;
            Ok(OpResult::Text(text))
        }
        InstructionsOp::Set { text, .. } => {
            let Some(text) = text else {
                return Err(OpError::Invalid("give the rules text, or --file".into()));
            };
            if text.len() > INSTRUCTIONS_MAX {
                return Err(OpError::Invalid(format!(
                    "the rules are {} bytes; the limit is {INSTRUCTIONS_MAX} bytes",
                    text.len()
                )));
            }
            cx.background_spawn(async move { write(&path, &text) })
                .await?;
            Ok(OpResult::Done)
        }
        InstructionsOp::Clear { .. } => {
            cx.background_spawn(async move { remove(&path) }).await?;
            Ok(OpResult::Done)
        }
    }
}

/// The rules a thread's program is told when it starts in `project_id`: the global rules, then
/// the project's, each cut at the cap.
pub(crate) async fn for_thread(
    this: &WeakEntity<Workspace>,
    project_id: &ProjectId,
    cx: &mut AsyncApp,
) -> Result<String, OpError> {
    let (global, project) = this
        .update(cx, |workspace, cx| {
            let folder = workspace
                .projects
                .get(project_id)
                .map(|project| project.read(cx).record.path.clone());
            (
                global_file(&workspace.data_dir),
                folder.map(|folder| project_file(&folder)),
            )
        })
        .map_err(closed)?;
    Ok(cx
        .background_spawn(async move {
            let project = project.map(|path| read(&path)).unwrap_or_default();
            compose(&read(&global), &project)
        })
        .await)
}

fn file_for(
    this: &WeakEntity<Workspace>,
    project: Option<&ProjectId>,
    cx: &mut AsyncApp,
) -> Result<PathBuf, OpError> {
    this.update(cx, |workspace, cx| match project {
        None => Ok(global_file(&workspace.data_dir)),
        Some(id) => workspace
            .projects
            .get(id)
            .map(|project| project_file(&project.read(cx).record.path))
            .ok_or_else(|| missing("project", id)),
    })
    .map_err(closed)?
}

pub(crate) fn global_file(data_dir: &Path) -> PathBuf {
    data_dir.join("instructions.md")
}

fn project_file(folder: &Path) -> PathBuf {
    folder.join(".kagan").join("instructions.md")
}

/// A missing or unreadable file is empty: it adds nothing to a start.
fn read(path: &Path) -> String {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

fn write(path: &Path, text: &str) -> Result<(), OpError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

fn remove(path: &Path) -> Result<(), OpError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// The text a start adds to its note. Each file is trimmed and cut at the cap; an empty one
/// adds nothing.
fn compose(global: &str, project: &str) -> String {
    [(GLOBAL_HEADING, global), (PROJECT_HEADING, project)]
        .into_iter()
        .filter_map(|(heading, text)| {
            let text = cut(text).trim();
            (!text.is_empty()).then(|| format!("{heading}\n{text}"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn cut(text: &str) -> &str {
    if text.len() <= INSTRUCTIONS_MAX {
        return text;
    }
    let mut end = INSTRUCTIONS_MAX;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_keeps_global_then_project_under_their_own_headings() {
        let text = compose("  be brief  \n", "use cargo t\n");
        assert_eq!(
            text,
            format!("{GLOBAL_HEADING}\nbe brief\n\n{PROJECT_HEADING}\nuse cargo t")
        );
    }

    #[test]
    fn compose_adds_nothing_for_blank_files() {
        assert_eq!(compose("", " \n"), "");
    }

    #[test]
    fn compose_cuts_each_file_at_the_cap_on_a_character_boundary() {
        let global = format!("{}é", "a".repeat(INSTRUCTIONS_MAX - 1));
        let project = "b".repeat(INSTRUCTIONS_MAX + 10);
        let text = compose(&global, &project);
        let global_part = format!("{GLOBAL_HEADING}\n{}", "a".repeat(INSTRUCTIONS_MAX - 1));
        assert!(text.starts_with(&global_part), "{text}");
        assert!(!text.contains('é'), "a split character was kept");
        assert!(text.contains(&"b".repeat(INSTRUCTIONS_MAX)));
        assert!(!text.contains(&"b".repeat(INSTRUCTIONS_MAX + 1)));
    }
}
