use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::Workspace;
use kg_domain::{
    DiffOp, DiffQuery, DiffScope, EnvironmentId, EnvironmentOp, EnvironmentSpec, InteractionOption,
    Metadata, Op, OpError, OpResult, PendingInteractionId, PluginId, Project, ProjectId, ProjectOp,
    Run, Section, SectionId, ThreadId, ThreadOp, ThreadStatus, Timestamp, Visibility,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Agents;
use crate::area::{from_host, to_host};

pub(crate) fn module(plugin: &PluginId, _agents: &Agents) -> HostModule {
    let mut module = HostModule::new("kagan/threads");
    let methods = [
        "get",
        "list",
        "stop",
        "send",
        "archive",
        "unarchive",
        "remove",
        "setPinned",
        "rename",
        "markRead",
        "metadata",
        "setMetadata",
        "events",
        "waitEvent",
        "wait",
        "ask",
        "projects",
        "project",
        "environment",
        "restoreEnvironment",
    ];
    module = module
        .declarations(include_str!("threads.d.ts"))
        .function("sidebar", |_| {
            gpui_shell::with_current_app(|cx| {
                serde_json::to_value(PluginSidebar::from(
                    &Workspace::global(cx).read(cx).sidebar(cx),
                ))
                .map(to_host)
                .map_err(error)
            })
            .ok_or_else(|| HostError::new("no kagan app context"))?
        });
    let owner = plugin.clone();
    module = module.async_function("spawn", move |args| dispatch(spawn_thread(args, &owner)?));
    module = module.async_function("answer", answer_question);
    module = module.async_function("changes", |args| dispatch(shown_diff(args, None)?));
    module = module.async_function("diff", |args| {
        dispatch(shown_diff(args, Some(args.string(2)?.to_owned()))?)
    });
    for name in methods {
        let plugin = plugin.clone();
        module = module.async_function(name, move |args| dispatch(operation(name, args, &plugin)?));
    }
    module
}

fn operation(name: &str, args: &HostArguments, plugin: &PluginId) -> Result<Op, HostError> {
    match name {
        "get" | "stop" | "archive" | "unarchive" => thread_by_id(name, args),
        "send" => send_line(args),
        "markRead" => thread_by_id("mark-read", args),
        "remove" => thread_by_id("delete", args),
        "list" => list_threads(args),
        "events" => with_fields("events", args),
        "waitEvent" => wait_event(args),
        "wait" => wait_for_settle(args),
        "ask" => ask_person(args),
        "setPinned" => set_pinned(args),
        "rename" => rename_thread(args),
        "metadata" => read_metadata(args, plugin),
        "setMetadata" => write_metadata(args, plugin),
        "projects" => Ok(Op::Project(ProjectOp::List)),
        "project" => get_project(args),
        "environment" => get_environment(args),
        "restoreEnvironment" => restore_environment(args),
        _ => Err(HostError::new(format!("unknown threads function {name}"))),
    }
}

fn send_line(args: &HostArguments) -> Result<Op, HostError> {
    as_thread(json!({
        "send": {
            "id": args.string(0)?,
            "text": args.string(1)?,
        }
    }))
}

fn thread_by_id(name: &str, args: &HostArguments) -> Result<Op, HostError> {
    as_thread(json!({ name: { "id": args.string(0)? } }))
}

fn as_thread(value: Value) -> Result<Op, HostError> {
    serde_json::from_value(value).map(Op::Thread).map_err(error)
}

fn list_threads(args: &HostArguments) -> Result<Op, HostError> {
    as_thread(json!({ "list": optional(args, 0) }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginSpawn {
    project_id: ProjectId,
    program: String,
    title: Option<String>,
    environment: EnvironmentSpec,
    visibility: Option<Visibility>,
    parent_thread_id: Option<ThreadId>,
    lifecycle_owner_thread_id: Option<ThreadId>,
    metadata: Option<Value>,
}

fn spawn_thread(args: &HostArguments, plugin: &PluginId) -> Result<Op, HostError> {
    let spawn: PluginSpawn = argument(args, 0)?;
    Ok(Op::Thread(ThreadOp::Run(Run {
        project_id: spawn.project_id,
        program: spawn.program,
        title: spawn.title,
        environment: spawn.environment,
        visibility: spawn.visibility,
        parent_thread_id: spawn.parent_thread_id,
        lifecycle_owner_thread_id: spawn.lifecycle_owner_thread_id,
        metadata: spawn.metadata.map(|value| Metadata {
            plugin_id: plugin.clone(),
            value,
        }),
    })))
}

fn answer_question(
    args: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let thread_id = ThreadId(args.string(0)?.to_owned());
    let interaction_id = PendingInteractionId(args.string(1)?.to_owned());
    let option_id = args.string(2)?.to_owned();
    let receive = gpui_shell::with_current_app(|cx| {
        if Workspace::global(cx)
            .read(cx)
            .needs_a_person(&interaction_id)
        {
            return Err(HostError::new("a person answers this"));
        }
        Ok(watch(
            Workspace::answer(&thread_id, interaction_id, option_id, cx),
            cx,
        ))
    })
    .ok_or_else(|| HostError::new("no kagan app context"))??;
    Ok(finish(receive))
}

// A plugin reads the changes of a thread kagan lists, never of a hidden or archived one.
fn shown_diff(args: &HostArguments, path: Option<String>) -> Result<Op, HostError> {
    let thread_id = ThreadId(args.string(0)?.to_owned());
    let scope: DiffScope = argument(args, 1)?;
    let listed = gpui_shell::with_current_app(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread(&thread_id)
            .is_some_and(|thread| {
                let record = thread.read(cx).record();
                record.visibility == Visibility::Visible && record.archived_at.is_none()
            })
    })
    .ok_or_else(|| HostError::new("no kagan app context"))?;
    if !listed {
        return Err(HostError::new(format!(
            "thread {thread_id} is not in kagan's thread list"
        )));
    }
    let query = DiffQuery {
        thread_id,
        scope,
        base: None,
        rev: (scope == DiffScope::Commit).then(|| "HEAD".to_owned()),
    };
    Ok(Op::Diff(match path {
        Some(path) => DiffOp::Show { query, path },
        None => DiffOp::List { query },
    }))
}

fn with_fields(kind: &str, args: &HostArguments) -> Result<Op, HostError> {
    let mut fields = argument::<serde_json::Map<String, Value>>(args, 1)?;
    fields.insert("id".into(), json!(args.string(0)?));
    as_thread(json!({ kind: fields }))
}

fn wait_event(args: &HostArguments) -> Result<Op, HostError> {
    let mut fields = argument::<serde_json::Map<String, Value>>(args, 1)?;
    fields.insert("id".into(), json!(args.string(0)?));
    if let Some(kind) = fields.remove("type") {
        fields.insert("kind".into(), kind);
    }
    as_thread(json!({ "wait-event": fields }))
}

fn ask_person(args: &HostArguments) -> Result<Op, HostError> {
    let options = argument::<Vec<InteractionOption>>(args, 2)?;
    as_thread(json!({
        "ask": {
            "id": args.string(0)?,
            "title": args.string(1)?,
            "options": options,
        }
    }))
}

fn wait_for_settle(args: &HostArguments) -> Result<Op, HostError> {
    let mut fields = argument::<serde_json::Map<String, Value>>(args, 1)?;
    fields.insert("id".into(), json!(args.string(0)?));
    as_thread(json!({ "wait": fields }))
}

fn set_pinned(args: &HostArguments) -> Result<Op, HostError> {
    as_thread(json!({
        "set-pinned": { "id": args.string(0)?, "pinned": argument::<bool>(args, 1)? }
    }))
}

fn rename_thread(args: &HostArguments) -> Result<Op, HostError> {
    as_thread(json!({
        "rename": { "id": args.string(0)?, "title": args.string(1)? }
    }))
}

fn read_metadata(args: &HostArguments, plugin: &PluginId) -> Result<Op, HostError> {
    as_thread(json!({ "metadata": { "id": args.string(0)?, "pluginId": plugin } }))
}

fn write_metadata(args: &HostArguments, plugin: &PluginId) -> Result<Op, HostError> {
    as_thread(json!({
        "set-metadata": {
            "id": args.string(0)?,
            "metadata": { "pluginId": plugin, "value": from_host(args.value(1)?) }
        }
    }))
}

fn get_project(args: &HostArguments) -> Result<Op, HostError> {
    Ok(Op::Project(ProjectOp::Get {
        id: argument(args, 0)?,
    }))
}

fn get_environment(args: &HostArguments) -> Result<Op, HostError> {
    Ok(Op::Environment(EnvironmentOp::Get {
        id: argument(args, 0)?,
    }))
}

fn restore_environment(args: &HostArguments) -> Result<Op, HostError> {
    Ok(Op::Environment(EnvironmentOp::Restore {
        thread_id: argument(args, 0)?,
    }))
}

pub(crate) fn argument<T: for<'de> Deserialize<'de>>(
    args: &HostArguments,
    ix: usize,
) -> Result<T, HostError> {
    serde_json::from_value(from_host(args.value(ix)?)).map_err(error)
}

fn optional(args: &HostArguments, ix: usize) -> Value {
    args.get(ix).map_or_else(|| json!({}), from_host)
}

pub(crate) fn error(error: impl std::fmt::Display) -> HostError {
    HostError::new(error.to_string())
}

pub(crate) fn dispatch(
    op: Op,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send, HostError> {
    let receive = gpui_shell::with_current_app(|cx| watch(Workspace::dispatch(op, cx), cx))
        .ok_or_else(|| HostError::new("no kagan app context"))?;
    Ok(finish(receive))
}

fn watch(
    task: impl Future<Output = Result<OpResult, OpError>> + 'static,
    cx: &mut gpui_kit::App,
) -> async_channel::Receiver<Result<OpResult, OpError>> {
    let (sender, receive) = async_channel::bounded(1);
    cx.spawn(async move |_| {
        let _ = sender.send(task.await).await;
    })
    .detach();
    receive
}

async fn finish(
    receive: async_channel::Receiver<Result<OpResult, OpError>>,
) -> Result<HostValue, HostError> {
    let result = receive.recv().await.map_err(error)?.map_err(error)?;
    shown(result).map(to_host).map_err(error)
}

fn shown(result: OpResult) -> serde_json::Result<Value> {
    match &result {
        OpResult::Thread(thread) => serde_json::to_value(PluginThread::from(thread)),
        OpResult::Threads(threads) => {
            serde_json::to_value(threads.iter().map(PluginThread::from).collect::<Vec<_>>())
        }
        OpResult::Sidebar(sidebar) => serde_json::to_value(PluginSidebar::from(sidebar)),
        _ => Ok(match serde_json::to_value(result)? {
            Value::Object(fields) => fields.into_values().next().unwrap_or(Value::Null),
            _ => Value::Null,
        }),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginThread<'a> {
    id: &'a ThreadId,
    project_id: &'a ProjectId,
    environment_id: &'a Option<EnvironmentId>,
    title: &'a str,
    status: ThreadStatus,
    visibility: Visibility,
    parent_thread_id: &'a Option<ThreadId>,
    lifecycle_owner_thread_id: &'a Option<ThreadId>,
    section_id: &'a Option<SectionId>,
    pinned: bool,
    last_read_at: Option<Timestamp>,
    latest_attention_at: Timestamp,
    archived_at: Option<Timestamp>,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl<'a> From<&'a kg_domain::Thread> for PluginThread<'a> {
    fn from(thread: &'a kg_domain::Thread) -> Self {
        // No `..`: a field added to the record must be shown or hidden here on purpose.
        let kg_domain::Thread {
            id,
            project_id,
            environment_id,
            title,
            title_locked: _,
            provider: _,
            model: _,
            reasoning_level: _,
            status,
            visibility,
            parent_thread_id,
            lifecycle_owner_thread_id,
            section_id,
            pinned,
            last_read_at,
            latest_attention_at,
            mark: _,
            archived_at,
            created_at,
            updated_at,
        } = thread;
        Self {
            id,
            project_id,
            environment_id,
            title,
            status: *status,
            visibility: *visibility,
            parent_thread_id,
            lifecycle_owner_thread_id,
            section_id,
            pinned: *pinned,
            last_read_at: *last_read_at,
            latest_attention_at: *latest_attention_at,
            archived_at: *archived_at,
            created_at: *created_at,
            updated_at: *updated_at,
        }
    }
}

#[derive(Serialize)]
struct PluginSidebar<'a> {
    projects: &'a [Project],
    sections: &'a [Section],
    threads: Vec<PluginThread<'a>>,
}

impl<'a> From<&'a kg_domain::Sidebar> for PluginSidebar<'a> {
    fn from(sidebar: &'a kg_domain::Sidebar) -> Self {
        let kg_domain::Sidebar {
            projects,
            sections,
            threads,
        } = sidebar;
        Self {
            projects,
            sections,
            threads: threads.iter().map(PluginThread::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_shell::HostArguments;
    use kg_domain::PluginId;

    use super::operation;

    #[test]
    fn answer_is_not_an_operation() {
        let plugin = PluginId("dev.test".into());
        let answer = operation("answer", &HostArguments::new(Vec::new()), &plugin).unwrap_err();
        assert!(
            answer
                .to_string()
                .contains("unknown threads function answer"),
            "{answer}"
        );
    }

    #[test]
    fn rename_is_the_thread_title() {
        let op = operation(
            "rename",
            &HostArguments::new(vec![
                super::to_host(serde_json::json!("thr_1")),
                super::to_host(serde_json::json!("Notes")),
            ]),
            &PluginId("dev.test".into()),
        )
        .unwrap();
        assert!(
            matches!(op, kg_domain::Op::Thread(kg_domain::ThreadOp::Rename { id, title }) if id.0 == "thr_1" && title == "Notes")
        );
    }
}
