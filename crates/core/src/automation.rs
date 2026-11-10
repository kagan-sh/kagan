//! While kagan is open, a due automation starts a thread and sends its text.

use std::time::Duration;

use gpui_kit::{AsyncApp, Context, Task, WeakEntity};
use kg_domain::{
    Automation, AutomationId, AutomationOp, EnvironmentSpec, OpResult, ProjectId, Run, ThreadOp,
    Timestamp,
};

use crate::host::database::{Database, Result};
use crate::thread::{dispatch_thread, prepare_run};
use crate::workspace::{self, Workspace, id, missing, time};

const MINUTE_MS: i64 = 60_000;

pub(crate) fn watch(cx: &mut Context<Workspace>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if tick(&this, cx).await.is_none() {
                break;
            }
        }
    })
}

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: AutomationOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let result = match op {
        AutomationOp::Add {
            name,
            body,
            project_id,
            program,
            minutes,
            at,
        } => {
            let lookup = project_id.clone();
            database
                .call(move |store| store.project(&lookup))
                .await?
                .ok_or_else(|| missing("project", &project_id))?;
            let next_at = due(minutes, at, time(this, cx)?)?;
            let automation = Automation {
                id: AutomationId(id("auto")),
                project_id,
                name: name.trim().to_owned(),
                body,
                program: program.trim().to_owned(),
                minutes,
                at,
                paused: false,
                next_at: Some(next_at),
                threads: Vec::new(),
            };
            let saved = automation.clone();
            database
                .call(move |store| store.insert_automation(&saved))
                .await?;
            OpResult::Automation(automation)
        }
        AutomationOp::Pause { id } => set_paused(&database, id, true).await?,
        AutomationOp::Resume { id } => set_paused(&database, id, false).await?,
        AutomationOp::Delete { id } => {
            database
                .call(move |store| store.delete_automation(&id))
                .await?;
            OpResult::Done
        }
        AutomationOp::List { project_id } => {
            let rows = database
                .call(move |store| store.automations(project_id.as_ref()))
                .await?;
            OpResult::Automations(rows)
        }
    };
    Workspace::refresh(this, cx).await?;
    Ok(result)
}

fn due(minutes: Option<u16>, at: Option<Timestamp>, now: Timestamp) -> Result<Timestamp> {
    match (minutes, at) {
        (Some(minutes), None) => Ok(later(now, minutes)),
        (None, Some(at)) => Ok(at),
        _ => Err(kg_domain::OpError::Invalid(
            "an automation needs minutes from 1 to 1440, or one time".into(),
        )),
    }
}

async fn set_paused(database: &Database, id: AutomationId, paused: bool) -> Result<OpResult> {
    let lookup = id.clone();
    database
        .call(move |store| store.set_automation_paused(&lookup, paused))
        .await?;
    let loaded = id.clone();
    let automation = database
        .call(move |store| store.automation(&loaded))
        .await?
        .ok_or_else(|| missing("automation", &id))?;
    Ok(OpResult::Automation(automation))
}

async fn tick(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Option<()> {
    let database = workspace::database(this, cx).ok()?;
    let now = time(this, cx).ok()?;
    let automations = database.call(|store| store.automations(None)).await.ok()?;
    for automation in automations {
        if !is_due(&automation, now) {
            continue;
        }
        let busy = running(this, &automation, cx)?;
        if busy {
            continue;
        }
        fire(this, &database, &automation, cx).await?;
    }
    Some(())
}

fn is_due(automation: &Automation, now: Timestamp) -> bool {
    !automation.paused && automation.next_at.is_some_and(|next| next <= now)
}

fn running(
    this: &WeakEntity<Workspace>,
    automation: &Automation,
    cx: &mut AsyncApp,
) -> Option<bool> {
    this.update(cx, |workspace, cx| {
        automation.threads.iter().any(|id| {
            workspace
                .threads
                .get(id)
                .is_some_and(|thread| thread.read(cx).running())
        })
    })
    .ok()
}

async fn fire(
    this: &WeakEntity<Workspace>,
    database: &Database,
    automation: &Automation,
    cx: &mut AsyncApp,
) -> Option<()> {
    let project = automation.project_id.clone();
    if !this
        .update(cx, |workspace, _| begin(workspace, &project))
        .ok()?
    {
        return Some(());
    }
    let started = start(this, automation, cx).await;
    this.update(cx, |workspace, _| finish(workspace, &project))
        .ok()?;
    match started? {
        Started::None => {
            if let Some(minutes) = automation.minutes {
                let _ = postpone(this, database, &automation.id, minutes, cx).await;
            }
        }
        Started::Thread(thread) => {
            if thread.status.live() {
                let id = thread.id.clone();
                let text = automation.body.clone();
                let _ = dispatch_thread(
                    this,
                    database.clone(),
                    ThreadOp::Send { id, text },
                    None,
                    cx,
                )
                .await;
            }
            let _ = remember(this, database, automation, &thread.id, cx).await;
        }
    }
    Some(())
}

fn begin(workspace: &mut Workspace, project: &ProjectId) -> bool {
    if workspace.removing_projects.contains(project) {
        return false;
    }
    *workspace
        .preparing_projects
        .entry(project.clone())
        .or_default() += 1;
    true
}

fn finish(workspace: &mut Workspace, project: &ProjectId) {
    let Some(count) = workspace.preparing_projects.get_mut(project) else {
        return;
    };
    *count = count.saturating_sub(1);
    if *count == 0 {
        workspace.preparing_projects.remove(project);
    }
}

enum Started {
    None,
    Thread(Box<kg_domain::Thread>),
}

async fn start(
    this: &WeakEntity<Workspace>,
    automation: &Automation,
    cx: &mut AsyncApp,
) -> Option<Started> {
    let run = Run {
        project_id: automation.project_id.clone(),
        program: automation.program.clone(),
        title: Some(automation.name.clone()),
        environment: EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let Ok(prepared) = prepare_run(this, run, cx).await else {
        return Some(Started::None);
    };
    let thread_id = prepared.thread_id().clone();
    let submitted = this
        .update(cx, |workspace, _| {
            workspace.submit(workspace::Request::Run(Box::new(prepared)))
        })
        .ok()?;
    let found = match submitted.await {
        Ok(OpResult::Thread(thread)) => Some(thread),
        _ => this
            .update(cx, |workspace, cx| {
                workspace
                    .threads
                    .get(&thread_id)
                    .map(|thread| thread.read(cx).record().clone())
            })
            .ok()?,
    };
    Some(match found {
        Some(thread) => Started::Thread(Box::new(thread)),
        None => Started::None,
    })
}

async fn remember(
    this: &WeakEntity<Workspace>,
    database: &Database,
    automation: &Automation,
    thread_id: &kg_domain::ThreadId,
    cx: &mut AsyncApp,
) -> Result<()> {
    let now = time(this, cx)?;
    // One start moves the next due time forward by one interval. Time spent quit, and a run
    // that is still going, do not start a thread for every missed interval.
    let next = automation.minutes.map(|minutes| later(now, minutes));
    let id = automation.id.clone();
    let thread_id = thread_id.clone();
    database
        .call(move |store| store.record_automation_run(&id, &thread_id, now, next))
        .await
}

async fn postpone(
    this: &WeakEntity<Workspace>,
    database: &Database,
    id: &AutomationId,
    minutes: u16,
    cx: &mut AsyncApp,
) -> Result<()> {
    let next = later(time(this, cx)?, minutes);
    let id = id.clone();
    database
        .call(move |store| store.set_automation_next(&id, Some(next)))
        .await
}

fn later(now: Timestamp, minutes: u16) -> Timestamp {
    Timestamp(
        now.0
            .saturating_add(i64::from(minutes).saturating_mul(MINUTE_MS)),
    )
}
