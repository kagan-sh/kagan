use kg_domain::{Event, Project, ProjectId, ProjectKind, ThreadId, Timestamp};
use rusqlite::{OptionalExtension as _, Row, params};

use super::{event::append_deleted, not_found};
use crate::column::Text;
use crate::{Error, Result};

impl super::Store {
    pub fn put_project(&self, project: &Project) -> Result<()> {
        self.conn.execute(
            "INSERT INTO projects (id, kind, name, path, git_remote_url, sort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, name = excluded.name,
                path = excluded.path, git_remote_url = excluded.git_remote_url,
                sort = excluded.sort",
            params![
                Text(&project.id),
                Text(project.kind),
                project.name,
                Text(&project.path),
                project.git_remote_url,
                project.sort,
            ],
        )?;
        Ok(())
    }

    pub fn project(&self, id: &ProjectId) -> Result<Option<Project>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, kind, name, path, git_remote_url, sort FROM projects WHERE id = ?1",
                [Text(id)],
                project_row,
            )
            .optional()?)
    }

    pub fn delete_project(&self, id: &ProjectId, at: Timestamp) -> Result<Vec<(Event, ProjectId)>> {
        let tx = self.conn.unchecked_transaction()?;
        let ids: Vec<ThreadId> = {
            let mut statement = tx.prepare("SELECT id FROM threads WHERE project_id = ?1")?;
            statement
                .query_map([Text(id)], |row| Ok(row.get::<_, Text<_>>(0)?.0))?
                .collect::<Result<_, _>>()?
        };
        let events = append_deleted(&tx, &ids, at)?;
        tx.execute("DELETE FROM projects WHERE id = ?1", [Text(id)])?;
        tx.commit()?;
        Ok(events)
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        let mut statement = self.conn.prepare(
            "SELECT id, kind, name, path, git_remote_url, sort FROM projects
             ORDER BY sort, name, id",
        )?;
        let projects = statement
            .query_map([], project_row)?
            .collect::<Result<_, _>>()?;
        Ok(projects)
    }

    pub fn reorder_project(&self, id: &ProjectId, before: Option<&ProjectId>) -> Result<()> {
        let mut projects = self.projects()?;
        let Some(index) = projects.iter().position(|project| &project.id == id) else {
            return Err(not_found("project", id));
        };
        if projects[index].kind == ProjectKind::Personal {
            return Err(Error::Conflict(
                "Personal stays last and cannot be moved".into(),
            ));
        }
        let moved = projects.remove(index);
        let mut standard: Vec<Project> = projects
            .into_iter()
            .filter(|project| project.kind == ProjectKind::Standard)
            .collect();
        let at = match before {
            None => standard.len(),
            Some(before) => {
                let Some(at) = standard.iter().position(|project| &project.id == before) else {
                    return Err(not_found("project", before));
                };
                at
            }
        };
        standard.insert(at, moved);
        let tx = self.conn.unchecked_transaction()?;
        for (sort, project) in standard.iter_mut().enumerate() {
            project.sort = i64::try_from(sort).unwrap_or(i64::MAX);
            tx.execute(
                "UPDATE projects SET sort = ?1 WHERE id = ?2",
                params![project.sort, Text(&project.id)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn project_row(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get::<_, Text<_>>(0)?.0,
        kind: row.get::<_, Text<_>>(1)?.0,
        name: row.get(2)?,
        path: row.get::<_, Text<_>>(3)?.0,
        git_remote_url: row.get(4)?,
        sort: row.get(5)?,
    })
}
