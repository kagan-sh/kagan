//! While kagan is open, a due automation starts a thread and sends its text.

use std::time::Duration;

use gpui_kit::{AsyncApp, Context, Task, WeakEntity};
use kg_domain::{
    Automation, AutomationId, AutomationOp, EnvironmentSpec, OpError, OpResult, Run, ThreadId,
    ThreadOp, Timestamp,
};

use crate::Requester;
use crate::host::database::{Database, Result};
use crate::thread::dispatch_thread;
use crate::workspace::{self, Workspace, id, missing, time};

const MINUTE_MS: i64 = 60_000;
const RETRY_MINUTES: u16 = 1;

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
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let changes = !matches!(op, AutomationOp::List { .. });
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
            let lookup = id.clone();
            let automation = database
                .call(move |store| store.automation(&lookup))
                .await?
                .ok_or_else(|| missing("automation", &id))?;
            crate::workspace::approve(
                None,
                requester,
                format!("Delete the automation “{}”?", automation.name),
                "Delete",
                cx,
            )
            .await?;
            let deleted = automation.id;
            database
                .call(move |store| store.delete_automation(&deleted))
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
    if changes {
        Workspace::refresh(this, cx).await?;
    }
    Ok(result)
}

fn due(minutes: Option<u16>, at: Option<Timestamp>, now: Timestamp) -> Result<Timestamp> {
    match (minutes, at) {
        (Some(minutes), None) => Ok(later(now, minutes)),
        (None, Some(at)) => Ok(at),
        _ => Err(OpError::Invalid(
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

// `None` once the workspace is gone, which ends the watch loop. A store error is reported and
// the loop goes on, so one failed read does not stop every automation until kagan restarts.
async fn tick(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Option<()> {
    let database = workspace::database(this, cx).ok()?;
    let now = time(this, cx).ok()?;
    let due = match database.call(move |store| store.due_automations(now)).await {
        Ok(due) => due,
        Err(error) => {
            eprintln!("kg: could not read the automations: {error}");
            return Some(());
        }
    };
    for automation in due {
        if running(this, &automation, cx)? {
            continue;
        }
        fire(this, &database, &automation, cx).await?;
    }
    Some(())
}

fn running(
    this: &WeakEntity<Workspace>,
    automation: &Automation,
    cx: &mut AsyncApp,
) -> Option<bool> {
    this.update(cx, |workspace, cx| {
        automation
            .threads
            .last()
            .and_then(|id| workspace.threads.get(id))
            .is_some_and(|thread| thread.read(cx).running())
    })
    .ok()
}

async fn fire(
    this: &WeakEntity<Workspace>,
    database: &Database,
    automation: &Automation,
    cx: &mut AsyncApp,
) -> Option<()> {
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
    let outcome = match this
        .update(cx, |workspace, cx| workspace.start_run(run, cx))
        .ok()?
    {
        Ok(started) => started.await,
        Err(error) => (None, Err(error)),
    };
    let (named, result) = outcome;
    let saved = match &named {
        Some(id) => thread_saved(database, id).await.unwrap_or_else(|error| {
            report::<()>(Err(error));
            false
        }),
        None => false,
    };
    let Some(thread) = named.filter(|_| saved) else {
        let minutes = automation.minutes.unwrap_or(RETRY_MINUTES);
        report(postpone(this, database, &automation.id, minutes, cx).await);
        return Some(());
    };
    if matches!(&result, Ok(OpResult::Thread(record)) if record.status.live()) {
        let op = ThreadOp::Send {
            id: thread.clone(),
            text: automation.body.clone(),
        };
        report(dispatch_thread(this, database.clone(), op, None, cx).await);
    }
    report(remember(this, database, automation, &thread, cx).await);
    Some(())
}

fn report<T>(result: Result<T>) {
    if let Err(error) = result {
        eprintln!("kg: an automation could not finish: {error}");
    }
}

async fn remember(
    this: &WeakEntity<Workspace>,
    database: &Database,
    automation: &Automation,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<()> {
    let now = time(this, cx)?;
    let next = automation.minutes.map(|minutes| later(now, minutes));
    let id = automation.id.clone();
    let thread_id = thread_id.clone();
    database
        .call(move |store| store.record_automation_run(&id, &thread_id, now, next))
        .await?;
    Workspace::refresh(this, cx).await
}

async fn thread_saved(database: &Database, id: &ThreadId) -> Result<bool> {
    let lookup = id.clone();
    Ok(database
        .call(move |store| store.thread(&lookup))
        .await?
        .is_some())
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
