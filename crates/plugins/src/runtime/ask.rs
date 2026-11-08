use std::rc::Rc;
use std::time::Duration;

use async_channel::Receiver;
use gpui_kit::{AsyncApp, Context, Entity};
use kg_domain::{OpError, PluginId, ProjectId, ThreadId};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::PluginHost;
use crate::jobs::{Ended, Jobs, Reply};
use crate::secret;

pub(crate) const TOOL_TIMEOUT: Duration = Duration::from_mins(2);
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const INSTRUCT_TIMEOUT: Duration = Duration::from_secs(2);
const OTHER_TIMEOUT: Duration = Duration::from_secs(10);

/// What a plugin's `next()` receives, tagged by `kind`.
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Job<'a> {
    Call {
        name: &'a str,
        input: &'a Value,
        caller: &'a PluginId,
    },
    Schedule {
        name: &'a str,
    },
    Tool {
        name: &'a str,
        input: &'a Value,
        thread_id: &'a ThreadId,
    },
    Instruct {
        thread_id: &'a ThreadId,
        project_id: &'a ProjectId,
    },
    Event {
        #[serde(rename = "type")]
        type_name: &'a str,
        thread_id: &'a ThreadId,
        project_id: &'a ProjectId,
        data: &'a Value,
    },
    Command {
        command: &'a str,
        thread_id: &'a Option<ThreadId>,
        project_id: &'a Option<ProjectId>,
    },
    Cli {
        command: &'a str,
        argv: &'a [String],
        cwd: String,
        thread_id: &'a Option<ThreadId>,
        project_id: &'a Option<ProjectId>,
    },
    Page {
        panel_id: &'a str,
        message: &'a Value,
    },
}

impl Job<'_> {
    pub(crate) fn fields(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(fields)) => fields,
            _ => Map::new(),
        }
    }

    /// The timeouts in `docs/plugin-api.md`.
    pub(crate) fn timeout(&self) -> Duration {
        match self {
            Self::Tool { .. } | Self::Cli { .. } | Self::Schedule { .. } => TOOL_TIMEOUT,
            Self::Call { .. } => CALL_TIMEOUT,
            Self::Instruct { .. } => INSTRUCT_TIMEOUT,
            Self::Event { .. } | Self::Command { .. } | Self::Page { .. } => OTHER_TIMEOUT,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Call { name, .. } | Self::Tool { name, .. } | Self::Schedule { name } => name,
            Self::Cli { command, .. } | Self::Command { command, .. } => command,
            Self::Instruct { .. } => "instruct",
            Self::Event { type_name, .. } => type_name,
            Self::Page { panel_id, .. } => panel_id,
        }
    }
}

/// Why a plugin gave no reply.
pub(crate) enum Unanswered {
    NotLoaded(OpError),
    Unloaded(OpError),
    TimedOut(OpError),
}

impl From<Unanswered> for OpError {
    fn from(unanswered: Unanswered) -> Self {
        match unanswered {
            Unanswered::NotLoaded(error)
            | Unanswered::Unloaded(error)
            | Unanswered::TimedOut(error) => error,
        }
    }
}

/// Sends `job` to the plugin, loading it first, and waits for its reply, with the plugin's
/// secrets redacted from the reply and from its error text. Only a schedule that times out
/// unloads the plugin (`docs/plugin-api.md`).
pub(crate) async fn ask(
    host: &Entity<PluginHost>,
    plugin: &PluginId,
    job: &Job<'_>,
    cx: &mut AsyncApp,
) -> Result<Reply, Unanswered> {
    let jobs = host
        .update(cx, |host, cx| host.load(plugin, cx))
        .map_err(Unanswered::NotLoaded)?;
    let name = job.name();
    let timeout = job.timeout();
    let unloaded = || {
        Unanswered::Unloaded(OpError::Failed(format!(
            "plugin {plugin} was unloaded before it answered {name}"
        )))
    };
    let (id, replied) = jobs.send(job).ok_or_else(unloaded)?;
    let timer = cx.background_executor().timer(timeout);
    let reply = match jobs.settled(&id, &replied, timer).await {
        Ok(reply) => reply,
        Err(Ended::Unloaded) => return Err(unloaded()),
        Err(Ended::TimedOut) => {
            if matches!(job, Job::Schedule { .. }) {
                // A timeout may leave a dead job loop behind, so that load must not be reused.
                host.update(cx, |host, cx| host.unload(plugin, cx));
            }
            return Err(Unanswered::TimedOut(OpError::Failed(format!(
                "plugin {plugin} did not answer {name} within {} s",
                timeout.as_secs()
            ))));
        }
    };
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    Ok(match reply {
        Ok(value) => Ok(redacted(&secrets, plugin, value)),
        Err(message) => Err(secrets.redact(plugin, &message)),
    })
}

/// Sends `job` and ignores the reply. The job's slot stays open so the plugin can still settle
/// it, and goes away after the job's timeout.
pub(crate) fn post(
    host: &mut PluginHost,
    plugin: &PluginId,
    job: &Job<'_>,
    cx: &mut Context<PluginHost>,
) {
    let Ok(jobs) = host.load(plugin, cx) else {
        return;
    };
    if let Some(queued) = jobs.send(job) {
        forget_later(jobs, queued, job.timeout(), cx);
    }
}

pub(crate) fn forget_later(
    jobs: Rc<Jobs>,
    (id, replied): (String, Receiver<Reply>),
    timeout: Duration,
    cx: &mut Context<PluginHost>,
) {
    cx.spawn(async move |_, cx| {
        let timer = cx.background_executor().timer(timeout);
        let _ = jobs.settled(&id, &replied, timer).await;
    })
    .detach();
}

fn redacted(secrets: &secret::Store, plugin: &PluginId, value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(secrets.redact(plugin, &text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redacted(secrets, plugin, item))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, item)| {
                    (
                        secrets.redact(plugin, &key),
                        redacted(secrets, plugin, item),
                    )
                })
                .collect(),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) => value,
    }
}
