use std::path::{Path, PathBuf};

use gpui_kit::AsyncApp;
use kg_domain::ThreadId;

use super::Retirement;
use crate::database::{Database, Result};

/// The Personal folders that deleting `id` would remove. Only Delete removes them.
pub(crate) async fn personal_folders(database: &Database, id: ThreadId) -> Result<Vec<Retirement>> {
    database
        .call(move |store| {
            let mut folders = Vec::new();
            for environment in store.personal_folders(&id)? {
                let Some(project) = store.project(&environment.project_id)? else {
                    continue;
                };
                folders.push(Retirement {
                    environment,
                    project: project.path,
                });
            }
            Ok(folders)
        })
        .await
}

/// Names each folder and what is in it, for the Delete question.
pub(crate) async fn describe(folders: &[Retirement], cx: &mut AsyncApp) -> String {
    let paths: Vec<PathBuf> = folders
        .iter()
        .map(|folder| folder.environment.path.clone())
        .collect();
    cx.background_executor()
        .spawn(async move {
            paths
                .iter()
                .map(|path| format!("{} ({})", path.display(), contents(path)))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .await
}

fn contents(path: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(path) else {
        return if path.exists() {
            "could not be read".into()
        } else {
            "already gone".into()
        };
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    match names.len() {
        0 => "empty".into(),
        1..=5 => names.join(", "),
        count => format!("{} and {} more", names[..5].join(", "), count - 5),
    }
}
