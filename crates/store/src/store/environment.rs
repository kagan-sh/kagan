use std::path::Path;

use kg_domain::{
    Environment, EnvironmentId, EnvironmentProvider, EnvironmentStatus, ProjectId, ThreadId,
};
use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::thread::owned_threads;
use crate::Result;
use crate::column::Text;

impl super::Store {
    pub fn delete_environment(&self, id: &EnvironmentId) -> Result<()> {
        self.conn
            .execute("DELETE FROM environments WHERE id = ?1", [Text(id)])?;
        Ok(())
    }

    pub fn put_environment(&self, environment: &Environment) -> Result<()> {
        self.conn.execute(
            "INSERT INTO environments
                (id, project_id, path, status, provider, branch, base_branch, retire_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id,
                path = excluded.path, status = excluded.status,
                provider = excluded.provider, branch = excluded.branch,
                base_branch = excluded.base_branch, retire_at = excluded.retire_at,
                error = excluded.error",
            params![
                Text(&environment.id),
                Text(&environment.project_id),
                Text(&environment.path),
                Text(environment.status),
                Text(environment.provider),
                environment.branch,
                environment.base_branch,
                environment.retire_at.map(|at| at.0),
                environment.error,
            ],
        )?;
        Ok(())
    }

    pub fn environment(&self, id: &EnvironmentId) -> Result<Option<Environment>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, project_id, path, status, provider, branch, base_branch, retire_at, error
                 FROM environments WHERE id = ?1",
                [Text(id)],
                environment_row,
            )
            .optional()?)
    }

    pub fn environments(&self, project_id: &ProjectId) -> Result<Vec<Environment>> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, path, status, provider, branch, base_branch, retire_at, error
             FROM environments WHERE project_id = ?1 ORDER BY id",
        )?;
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
    let mut statement = conn.prepare(
        "SELECT id, project_id, path, status, provider, branch, base_branch, retire_at, error
         FROM environments",
    )?;
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
        id: row.get::<_, Text<_>>(0)?.0,
        project_id: row.get::<_, Text<_>>(1)?.0,
        path: row.get::<_, Text<_>>(2)?.0,
        status: row.get::<_, Text<_>>(3)?.0,
        provider: row.get::<_, Text<_>>(4)?.0,
        branch: row.get(5)?,
        base_branch: row.get(6)?,
        retire_at: row.get::<_, Option<i64>>(7)?.map(kg_domain::Timestamp),
        error: row.get(8)?,
    })
}
