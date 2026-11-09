use std::collections::BTreeSet;

use gpui_kit::{App, AppContext as _, AsyncApp, Entity, WeakEntity};
use kg_domain::{EnvironmentId, ProjectId, ThreadId};

use crate::host::database::Result;
use crate::workspace::clock::{closed, time};
use crate::workspace::screens::{remember, restore_unshown};
use crate::{Environment, Project, Thread};

use super::{Workspace, WorkspaceEvent};

impl Workspace {
    #[must_use]
    pub fn sections(&self) -> &[kg_domain::Section] {
        &self.sections
    }
    #[must_use]
    pub fn sidebar(&self, cx: &App) -> kg_domain::Sidebar {
        kg_domain::Sidebar::new(
            self.projects
                .values()
                .map(|project| project.read(cx).record().clone())
                .collect(),
            self.sections.clone(),
            self.threads
                .values()
                .map(|thread| thread.read(cx).record().clone())
                .collect(),
        )
    }
    #[must_use]
    pub fn project(&self, id: &ProjectId) -> Option<&Entity<Project>> {
        self.projects.get(id)
    }
    #[must_use]
    pub fn thread(&self, id: &ThreadId) -> Option<&Entity<Thread>> {
        self.threads.get(id)
    }
    #[must_use]
    pub fn thread_records(&self, cx: &App) -> Vec<kg_domain::Thread> {
        self.threads
            .values()
            .map(|thread| thread.read(cx).record().clone())
            .collect()
    }
    #[must_use]
    pub fn environment(&self, id: &EnvironmentId) -> Option<&Entity<Environment>> {
        self.environments.get(id)
    }
    pub(crate) async fn refresh(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        let database = crate::workspace::database(this, cx)?;
        let (projects, environments, threads, sections, endings, screens) = database
            .call(|store| {
                Ok((
                    store.projects()?,
                    store.all_environments()?,
                    store.all_threads()?,
                    store.sections()?,
                    store.endings()?,
                    store.screens()?,
                ))
            })
            .await?;
        let unshown = this
            .update(cx, |workspace, cx| {
                workspace.sections = sections;
                let project_ids: BTreeSet<&ProjectId> =
                    projects.iter().map(|record| &record.id).collect();
                workspace.projects.retain(|id, _| project_ids.contains(id));
                let environment_ids: BTreeSet<&EnvironmentId> =
                    environments.iter().map(|record| &record.id).collect();
                workspace
                    .environments
                    .retain(|id, _| environment_ids.contains(id));
                for record in projects {
                    if let Some(entity) = workspace.projects.get(&record.id) {
                        entity.update(cx, |project, cx| {
                            project.record = record;
                            cx.notify();
                        });
                    } else {
                        workspace
                            .projects
                            .insert(record.id.clone(), cx.new(|_| Project { record }));
                    }
                }
                for record in environments {
                    if let Some(entity) = workspace.environments.get(&record.id) {
                        entity.update(cx, |environment, cx| {
                            environment.record = record;
                            cx.notify();
                        });
                    } else {
                        workspace
                            .environments
                            .insert(record.id.clone(), cx.new(|_| Environment { record }));
                    }
                }
                let thread_ids: BTreeSet<&ThreadId> =
                    threads.iter().map(|record| &record.id).collect();
                workspace.threads.retain(|id, _| thread_ids.contains(id));
                for record in threads {
                    if let Some(entity) = workspace.threads.get(&record.id) {
                        entity.update(cx, |thread, cx| {
                            thread.record = record;
                            cx.notify();
                        });
                    } else {
                        workspace
                            .threads
                            .insert(record.id.clone(), cx.new(|_| Thread::new(record)));
                    }
                }
                for run in workspace.runs.values_mut() {
                    run.end = None;
                    run.loss = None;
                }
                for (id, kinds) in endings {
                    if let Some(end) = crate::end::End::from_newest(&kinds) {
                        workspace.run_mut(&id).end = Some(end);
                    }
                }
                let unshown = remember(workspace, screens);
                cx.emit(WorkspaceEvent::Changed);
                cx.notify();
                unshown
            })
            .map_err(closed)?;
        crate::environment::drop_expired_screens(this, cx).await?;
        restore_unshown(this, unshown, cx).await
    }
    pub(crate) async fn events(
        this: &WeakEntity<Self>,
        id: ThreadId,
        kinds: Vec<kg_domain::EventKind>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let at = time(this, cx)?;
        Self::events_at(this, id, at, kinds, cx).await
    }
    pub(crate) async fn events_at(
        this: &WeakEntity<Self>,
        id: ThreadId,
        at: kg_domain::Timestamp,
        kinds: Vec<kg_domain::EventKind>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let database = crate::workspace::database(this, cx)?;
        let started = kinds
            .iter()
            .any(|kind| matches!(kind, kg_domain::EventKind::Started));
        let thread_id = id.clone();
        let (project_id, events) = database
            .call(move |store| {
                let project_id = store
                    .thread(&id)?
                    .ok_or(kg_store::Error::NotFound {
                        what: "thread",
                        id: id.to_string(),
                    })?
                    .project_id;
                let events = store.append_events(&id, at, kinds)?;
                if started {
                    store.delete_screen(&id)?;
                }
                Ok((project_id, events))
            })
            .await?;
        this.update(cx, |workspace, cx| {
            let mut newest = Vec::new();
            let mut run = None;
            for event in &events {
                if matches!(event.kind, kg_domain::EventKind::Started) {
                    run = Some(event.seq);
                }
                if matches!(
                    event.kind,
                    kg_domain::EventKind::Stopped
                        | kg_domain::EventKind::Exited { .. }
                        | kg_domain::EventKind::Failed { .. }
                ) {
                    newest.push(event.kind.clone());
                }
            }
            newest.reverse();
            if let Some(seq) = run {
                workspace.run_mut(&thread_id).begin(seq);
            }
            if let Some(end) = crate::end::End::from_newest(&newest) {
                workspace.run_mut(&thread_id).end = Some(end);
            }
            crate::thread::notify::after(workspace, &thread_id, &events, cx);
            for event in events {
                cx.emit(WorkspaceEvent::Event {
                    event,
                    project_id: project_id.clone(),
                });
            }
            cx.notify();
        })
        .map_err(closed)
    }
}
