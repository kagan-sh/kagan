use kg_domain::{Event, Project, ProjectId, ProjectKind, ThreadId, Timestamp};
use rusqlite::{OptionalExtension as _, Row, named_params, params};

use super::{event::append_deleted, not_found};
use crate::column::Text;
use crate::{Error, Result};

const PROJECT_COLUMNS: &str = "id, kind, name, path, git_remote_url, sort, program";

impl super::Store {
    pub fn put_project(&self, project: &Project) -> Result<()> {
        self.conn.execute(
            &super::upsert("projects", PROJECT_COLUMNS),
            named_params! {
                ":id": Text(&project.id),
                ":kind": Text(project.kind),
                ":name": project.name,
                ":path": Text(&project.path),
                ":git_remote_url": project.git_remote_url,
                ":sort": project.sort,
                ":program": project.program,
            },
        )?;
        Ok(())
    }

    pub fn project(&self, id: &ProjectId) -> Result<Option<Project>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
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
        let mut statement = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY sort, name, id"
        ))?;
        let projects = statement
            .query_map([], project_row)?
            .collect::<Result<_, _>>()?;
        Ok(projects)
    }

    pub fn set_project_program(&self, id: &ProjectId, program: Option<&str>) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE projects SET program = ?1 WHERE id = ?2",
            params![program, Text(id)],
        )?;
        if changed == 0 {
            return Err(not_found("project", id));
        }
        Ok(())
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
        id: row.get::<_, Text<_>>("id")?.0,
        kind: row.get::<_, Text<_>>("kind")?.0,
        name: row.get("name")?,
        path: row.get::<_, Text<_>>("path")?.0,
        git_remote_url: row.get("git_remote_url")?,
        sort: row.get("sort")?,
        program: row.get("program")?,
    })
}
