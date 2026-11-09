//! Plugin schedules: kagan's clock wakes a plugin every so many minutes, while kagan is open.

use std::time::Duration;

use gpui_kit::{AsyncApp, Context, Task, WeakEntity};
use kg_domain::PluginId;

use crate::PluginHost;
use crate::ask::{Job, TOOL_TIMEOUT, Unanswered, ask};
use crate::manifest::Schedule;

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

// `None` once kagan has dropped the host.
async fn tick(
    host: &WeakEntity<PluginHost>,
    plugin: &PluginId,
    name: &str,
    cx: &mut AsyncApp,
) -> Option<()> {
    let entity = host.upgrade()?;
    match ask(&entity, plugin, &Job::Schedule { name }, cx).await {
        Ok(Ok(_)) | Err(Unanswered::Unloaded(_)) => Some(()),
        Ok(Err(message)) => report(host, plugin, &message, cx),
        Err(Unanswered::NotLoaded(error)) => report(host, plugin, &error.to_string(), cx),
        Err(Unanswered::TimedOut(_)) => report(
            host,
            plugin,
            &format!(
                "schedule {name} did not finish within {} seconds",
                TOOL_TIMEOUT.as_secs()
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
    host.update(cx, |host, cx| {
        host.secrets
            .note_later(plugin, crate::Level::Warn, message, cx);
    })
    .ok()
}
