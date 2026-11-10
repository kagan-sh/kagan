//! The agent rules: the global file in the data folder, and each project's file in its folder.
//! A thread's program is told both when it starts.

use std::io::Read as _;
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
            let text = cx.background_spawn(async move { read_text(&path) }).await?;
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
            let project = project
                .map(|path| read_for_start(&path))
                .unwrap_or_default();
            compose(&read_for_start(&global), &project)
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

/// Most bytes a Get reads. A file past this is a mistake; the editor shows the start of it.
const READ_MAX: u64 = 1024 * 1024;

/// What a start reads. A missing or unreadable file adds nothing. Only the first bytes past the
/// cap are read, because `compose` cuts the text at the cap anyway.
fn read_for_start(path: &Path) -> String {
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    _ = file
        .take(INSTRUCTIONS_MAX as u64 + 3)
        .read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// What Get shows. Only a missing file is empty; any other read error is refused. A file that
/// is not UTF-8 is refused too, so Save cannot write replacement characters back.
fn read_text(path: &Path) -> Result<String, OpError> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(READ_MAX).read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 == READ_MAX;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(error) => {
            let cut_inside_a_character = truncated && error.utf8_error().error_len().is_none();
            if cut_inside_a_character {
                let valid = error.utf8_error().valid_up_to();
                let bytes = error.into_bytes();
                return Ok(String::from_utf8_lossy(&bytes[..valid]).into_owned());
            }
            Err(OpError::Invalid("the rules file is not UTF-8".into()))
        }
    }
}

/// Creates only the file's own folder, so a project whose folder is gone is not built again.
/// The text goes to a temp file first and is renamed into place, so a start never reads half a
/// file.
fn write(path: &Path, text: &str) -> Result<(), OpError> {
    let Some(dir) = path.parent() else {
        return Err(OpError::Failed(format!("{} has no folder", path.display())));
    };
    match std::fs::create_dir(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(OpError::Invalid(
                "the folder for these rules is missing".into(),
            ));
        }
        Err(error) => return Err(error.into()),
    }
    let temp = path.with_extension("md.tmp");
    let written = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        _ = std::fs::remove_file(&temp);
    }
    Ok(written?)
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
