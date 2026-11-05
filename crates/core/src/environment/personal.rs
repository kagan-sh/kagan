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

/// Removes a Personal folder. Windows refuses while the program that was just killed still has
/// the folder open, so the removal tries again for up to five seconds.
pub(crate) fn remove(path: &Path) -> std::io::Result<()> {
    let mut tries = 0;
    loop {
        match std::fs::remove_dir_all(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) if tries < 50 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            outcome => return outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contents_names_an_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(contents(dir.path()), "empty");
    }

    #[test]
    fn contents_lists_names_in_order_and_caps_a_long_folder() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["c", "a", "b"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        assert_eq!(contents(dir.path()), "a, b, c");

        let many = tempfile::tempdir().unwrap();
        for name in ["f", "e", "d", "c", "b", "a", "g"] {
            std::fs::write(many.path().join(name), "").unwrap();
        }
        assert_eq!(contents(many.path()), "a, b, c, d, e and 2 more");
    }

    #[test]
    fn contents_reports_a_folder_that_is_already_gone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone");
        assert_eq!(contents(&path), "already gone");
    }

    #[test]
    fn remove_removes_a_folder_and_tolerates_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personal");
        std::fs::create_dir_all(path.join("nested")).unwrap();
        std::fs::write(path.join("nested/note"), "").unwrap();
        remove(&path).unwrap();
        assert!(!path.exists());
        remove(&path).unwrap();
    }
}
