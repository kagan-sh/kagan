use std::path::Path;

use kg_domain::{
    Environment, EnvironmentId, EnvironmentProvider, EnvironmentStatus, ProjectId, ThreadId,
};
use rusqlite::{Connection, OptionalExtension as _, Row, named_params};

use super::thread::owned_threads;
use crate::Result;
use crate::column::Text;

const ENVIRONMENT_COLUMNS: &str =
    "id, project_id, path, status, provider, branch, base_branch, retire_at, error";

impl super::Store {
    pub fn delete_environment(&self, id: &EnvironmentId) -> Result<()> {
        self.conn
            .execute("DELETE FROM environments WHERE id = ?1", [Text(id)])?;
        Ok(())
    }

    pub fn put_environment(&self, environment: &Environment) -> Result<()> {
        self.conn.execute(
            &super::upsert("environments", ENVIRONMENT_COLUMNS),
            named_params! {
                ":id": Text(&environment.id),
                ":project_id": Text(&environment.project_id),
                ":path": Text(&environment.path),
                ":status": Text(environment.status),
                ":provider": Text(environment.provider),
                ":branch": environment.branch,
                ":base_branch": environment.base_branch,
                ":retire_at": environment.retire_at.map(|at| at.0),
                ":error": environment.error,
            },
        )?;
        Ok(())
    }

    pub fn environment(&self, id: &EnvironmentId) -> Result<Option<Environment>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {ENVIRONMENT_COLUMNS} FROM environments WHERE id = ?1"),
                [Text(id)],
                environment_row,
            )
            .optional()?)
    }

    pub fn all_environments(&self) -> Result<Vec<Environment>> {
        all_environments(&self.conn)
    }

    pub fn environments(&self, project_id: &ProjectId) -> Result<Vec<Environment>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {ENVIRONMENT_COLUMNS} FROM environments WHERE project_id = ?1 ORDER BY id"
        ))?;
        let environments = statement
            .query_map([Text(project_id)], environment_row)?
            .collect::<Result<_, _>>()?;
        Ok(environments)
    }

    /// Whether `path` lies in a folder that a delete is removing, so nothing new may attach to it.
    pub fn removal_pending(&self, path: &Path) -> Result<bool> {
        removal_pending(&self.conn, path)
    }

    /// The Personal folders that deleting `id` would remove: only `id` and its dependents use each,
    /// and no other thread, project or environment sits in or on it.
    pub fn personal_folders(&self, id: &ThreadId) -> Result<Vec<Environment>> {
        personal_folders(&self.conn, id)
    }
}

pub(super) fn removal_pending(conn: &Connection, path: &Path) -> Result<bool> {
    Ok(all_environments(conn)?.iter().any(|environment| {
        environment.status == EnvironmentStatus::Destroyed
            && environment.provider == EnvironmentProvider::Personal
            && path.starts_with(&environment.path)
    }))
}

fn all_environments(conn: &Connection) -> Result<Vec<Environment>> {
    let mut statement = conn.prepare(&format!("SELECT {ENVIRONMENT_COLUMNS} FROM environments"))?;
    let environments = statement
        .query_map([], environment_row)?
        .collect::<Result<_, _>>()?;
    Ok(environments)
}

pub(super) fn personal_folders(conn: &Connection, id: &ThreadId) -> Result<Vec<Environment>> {
    let owned = owned_threads(conn, id)?;
    let environments = all_environments(conn)?;
    let mut statement = conn.prepare("SELECT id, environment_id FROM threads")?;
    let threads: Vec<(ThreadId, Option<EnvironmentId>)> = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Text<_>>(0)?.0,
                row.get::<_, Option<Text<_>>>(1)?.map(|text| text.0),
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut statement = conn.prepare("SELECT path FROM projects")?;
    let projects: Vec<std::path::PathBuf> = statement
        .query_map([], |row| Ok(row.get::<_, String>(0)?.into()))?
        .collect::<Result<_, _>>()?;
    let path_of = |environment: &EnvironmentId| {
        environments
            .iter()
            .find(|candidate| &candidate.id == environment)
            .map(|candidate| candidate.path.clone())
    };
    let mut folders = Vec::new();
    for environment in &environments {
        let doomed = environment.provider == EnvironmentProvider::Personal
            && threads.iter().any(|(thread, used)| {
                owned.contains(thread) && used.as_ref() == Some(&environment.id)
            });
        if !doomed {
            continue;
        }
        let folder = &environment.path;
        let shared = threads.iter().any(|(thread, used)| {
            !owned.contains(thread)
                && used
                    .as_ref()
                    .and_then(path_of)
                    .is_some_and(|path| path.starts_with(folder))
        }) || environments
            .iter()
            .any(|other| other.id != environment.id && other.path.starts_with(folder))
            || projects.iter().any(|project| project.starts_with(folder));
        if !shared {
            folders.push(environment.clone());
        }
    }
    Ok(folders)
}

fn environment_row(row: &Row) -> rusqlite::Result<Environment> {
    Ok(Environment {
        id: row.get::<_, Text<_>>("id")?.0,
        project_id: row.get::<_, Text<_>>("project_id")?.0,
        path: row.get::<_, Text<_>>("path")?.0,
        status: row.get::<_, Text<_>>("status")?.0,
        provider: row.get::<_, Text<_>>("provider")?.0,
        branch: row.get("branch")?,
        base_branch: row.get("base_branch")?,
        retire_at: row
            .get::<_, Option<i64>>("retire_at")?
            .map(kg_domain::Timestamp),
        error: row.get("error")?,
    })
}
