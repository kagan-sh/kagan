//! Plugin schedules: kagan's clock wakes a plugin every so many minutes, while kagan is open.

use std::time::Duration;

use gpui_kit::{AsyncApp, Context, Task, WeakEntity};
use kg_domain::PluginId;
use serde_json::Map;

use crate::PluginHost;
use crate::jobs::Ended;
use crate::manifest::Schedule;

const TIMEOUT: Duration = Duration::from_mins(2);

/// One loop per schedule. Each run waits for the last one to finish, so a run never overlaps
/// the next. Dropping the tasks stops the loops.
pub(crate) fn start(
    plugin: &PluginId,
    schedules: &[Schedule],
    cx: &mut Context<PluginHost>,
) -> Vec<Task<()>> {
    schedules
        .iter()
        .map(|schedule| {
            let plugin = plugin.clone();
            let name = schedule.id.clone();
            let every = Duration::from_mins(u64::from(schedule.every));
            cx.spawn(async move |host, cx| {
                loop {
                    cx.background_executor().timer(every).await;
                    if tick(&host, &plugin, &name, cx).await.is_none() {
                        break;
                    }
                }
            })
        })
        .collect()
}

/// Loads the plugin if it is not loaded, then sends one `schedule` job and waits for its answer.
/// `None` when kagan has dropped the host.
async fn tick(
    host: &WeakEntity<PluginHost>,
    plugin: &PluginId,
    name: &str,
    cx: &mut AsyncApp,
) -> Option<()> {
    let jobs = match host.update(cx, |host, cx| host.load(plugin, cx)).ok()? {
        Ok(jobs) => jobs,
        Err(error) => return report(host, plugin, &error.to_string(), cx),
    };
    let job = Map::from_iter([
        ("kind".to_owned(), "schedule".into()),
        ("name".to_owned(), name.into()),
    ]);
    let Some((id, replied)) = jobs.send(job) else {
        return Some(());
    };
    let timer = cx.background_executor().timer(TIMEOUT);
    let outcome = jobs.settled(&id, &replied, timer).await;
    // A timeout may leave a dead job loop behind, so that load must not be reused. Only the
    // reply channel is gone; a slow handler can still finish what it was doing.
    if matches!(outcome, Err(Ended::TimedOut)) {
        host.update(cx, |host, cx| host.unload(plugin, cx)).ok()?;
    }
    match outcome {
        Ok(Ok(_)) | Err(Ended::Unloaded) => Some(()),
        Ok(Err(message)) => report(host, plugin, &message, cx),
        Err(Ended::TimedOut) => report(
            host,
            plugin,
            &format!(
                "schedule {name} did not finish within {} seconds",
                TIMEOUT.as_secs()
            ),
            cx,
        ),
    }
}

fn report(
    host: &WeakEntity<PluginHost>,
    plugin: &PluginId,
    message: &str,
    cx: &mut AsyncApp,
) -> Option<()> {
    host.update(cx, |host, _| host.report(plugin, message)).ok()
}
