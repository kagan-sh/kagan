use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::Workspace;
use kg_domain::{
    AskField, DiffOp, DiffQuery, DiffScope, EnvironmentId, EnvironmentOp, EnvironmentSpec,
    InteractionOption, Metadata, Op, OpResult, Order, PluginId, Project, ProjectId, ProjectOp, Run,
    Section, SectionId, ThreadId, ThreadOp, ThreadStatus, Timestamp, Visibility,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::area::{argument, awaited, dispatch, error, from_host, to_host, with_app};

/// Must match the 256 KiB ceiling published for `threads.output`.
pub(crate) const OUTPUT_CAP: usize = 256 * 1024;

type Build = fn(&HostArguments, &PluginId) -> Result<Op, HostError>;

fn thread(op: ThreadOp) -> Op {
    Op::Thread(op)
}

fn id(args: &HostArguments) -> Result<ThreadId, HostError> {
    Ok(ThreadId(args.string(0)?.to_owned()))
}

const CALLS: [(&str, Build); 23] = [
    ("get", |a, _| Ok(thread(ThreadOp::Get { id: id(a)? }))),
    ("list", |a, _| {
        let ListOptions {
            project_id,
            archived,
            limit,
            before,
        } = optional(a, 0)?;
        Ok(thread(ThreadOp::List {
            project_id,
            archived,
            limit,
            before,
        }))
    }),
    ("stop", |a, _| Ok(thread(ThreadOp::Stop { id: id(a)? }))),
    ("send", |a, _| {
        Ok(thread(ThreadOp::Send {
            id: id(a)?,
            text: a.string(1)?.to_owned(),
        }))
    }),
    ("archive", |a, _| {
        Ok(thread(ThreadOp::Archive { id: id(a)? }))
    }),
    ("unarchive", |a, _| {
        Ok(thread(ThreadOp::Unarchive { id: id(a)? }))
    }),
    ("remove", |a, _| {
        Ok(thread(ThreadOp::Delete {
            id: id(a)?,
            approval_thread_id: None,
        }))
    }),
    ("setPinned", |a, _| {
        Ok(thread(ThreadOp::SetPinned {
            id: id(a)?,
            pinned: argument(a, 1)?,
        }))
    }),
    ("rename", |a, _| {
        Ok(thread(ThreadOp::Rename {
            id: id(a)?,
            title: a.string(1)?.to_owned(),
        }))
    }),
    ("markRead", |a, _| {
        Ok(thread(ThreadOp::MarkRead { id: id(a)? }))
    }),
    ("metadata", |a, plugin| {
        Ok(thread(ThreadOp::Metadata {
            id: id(a)?,
            plugin_id: plugin.clone(),
        }))
    }),
    ("setMetadata", |a, plugin| {
        Ok(thread(ThreadOp::SetMetadata {
            id: id(a)?,
            plugin_id: plugin.clone(),
            value: from_host(a.value(1)?),
        }))
    }),
    ("events", |a, _| {
        let EventsOptions {
            after_seq,
            limit,
            order,
        } = argument(a, 1)?;
        Ok(thread(ThreadOp::Events {
            id: id(a)?,
            after_seq,
            limit,
            order,
        }))
    }),
    ("waitEvent", |a, _| {
        let WaitEventOptions {
            kind,
            after_seq,
            timeout_ms,
        } = argument(a, 1)?;
        Ok(thread(ThreadOp::WaitEvent {
            id: id(a)?,
            kind,
            after_seq,
            timeout_ms,
        }))
    }),
    ("wait", |a, _| {
        let WaitOptions { timeout_ms } = argument(a, 1)?;
        Ok(thread(ThreadOp::Wait {
            id: id(a)?,
            timeout_ms,
        }))
    }),
    ("ask", |a, _| ask_person(a)),
    ("projects", |_, _| Ok(Op::Project(ProjectOp::List))),
    ("project", |a, _| {
        Ok(Op::Project(ProjectOp::Get {
            id: argument(a, 0)?,
        }))
    }),
    ("environment", |a, _| {
        Ok(Op::Environment(EnvironmentOp::Get {
            id: argument(a, 0)?,
        }))
    }),
    ("restoreEnvironment", |a, _| {
        Ok(Op::Environment(EnvironmentOp::Restore {
            thread_id: argument(a, 0)?,
        }))
    }),
    ("spawn", spawn_thread),
    ("changes", |a, _| shown_diff(a, None)),
    ("diff", |a, _| shown_diff(a, Some(a.string(2)?.to_owned()))),
];

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListOptions {
    project_id: Option<ProjectId>,
    archived: Option<bool>,
    limit: Option<u32>,
    before: Option<Timestamp>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsOptions {
    after_seq: Option<u64>,
    limit: Option<u32>,
    order: Option<Order>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitEventOptions {
    #[serde(rename = "type")]
    kind: String,
    after_seq: Option<u64>,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitOptions {
    timeout_ms: u64,
}

pub(crate) fn module(plugin: &PluginId) -> HostModule {
    let mut module = HostModule::new("kagan/threads")
        .declarations(include_str!("threads.d.ts"))
        .function("sidebar", |_| {
            with_app(|cx| {
                serde_json::to_value(PluginSidebar::from(
                    &Workspace::global(cx).read(cx).sidebar(cx),
                ))
                .map(to_host)
                .map_err(error)
            })?
        })
        .async_function("output", read_output);
    for (name, build) in CALLS {
        let plugin = plugin.clone();
        module = module.async_function(name, move |args| dispatch(build(args, &plugin)?));
    }
    module
}

fn read_output(
    args: &HostArguments,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let id = ThreadId(args.string(0)?.to_owned());
    let output = with_app(|cx| awaited(Workspace::thread_output(&id, OUTPUT_CAP, cx), cx))?;
    Ok(async move {
        let text = output.await?.map_err(error)?;
        Ok(to_host(json!({ "output": text })))
    })
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

// A plugin reads the changes of a thread kagan lists, never of a hidden or archived one.
fn shown_diff(args: &HostArguments, path: Option<String>) -> Result<Op, HostError> {
    let thread_id = ThreadId(args.string(0)?.to_owned());
    let scope: DiffScope = argument(args, 1)?;
    let listed = with_app(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread(&thread_id)
            .is_some_and(|thread| {
                let record = thread.read(cx).record();
                record.visibility == Visibility::Visible && record.archived_at.is_none()
            })
    })?;
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

fn ask_person(args: &HostArguments) -> Result<Op, HostError> {
    let options = argument::<Vec<InteractionOption>>(args, 2)?;
    let fields = match args.get(3) {
        None => Vec::<AskField>::new(),
        Some(value) if from_host(value) == Value::Null => Vec::new(),
        Some(_) => argument::<Vec<AskField>>(args, 3)?,
    };
    Ok(thread(ThreadOp::Ask {
        id: id(args)?,
        title: args.string(1)?.to_owned(),
        options,
        fields: Some(fields),
    }))
}

fn optional<T: serde::de::DeserializeOwned + Default>(
    args: &HostArguments,
    ix: usize,
) -> Result<T, HostError> {
    args.get(ix).map_or_else(
        || Ok(T::default()),
        |value| serde_json::from_value(from_host(value)).map_err(error),
    )
}

pub(crate) fn shown(result: OpResult) -> serde_json::Result<Value> {
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

    use super::CALLS;

    fn build(name: &str, args: Vec<serde_json::Value>) -> kg_domain::Op {
        let (_, build) = CALLS.iter().find(|(call, _)| *call == name).unwrap();
        let args = HostArguments::new(args.into_iter().map(super::to_host));
        build(&args, &PluginId("dev.test".into())).unwrap()
    }

    #[test]
    fn answer_and_output_are_not_calls() {
        assert!(
            CALLS
                .iter()
                .all(|(name, _)| !["answer", "output"].contains(name))
        );
    }

    #[test]
    fn rename_is_the_thread_title() {
        let op = build("rename", vec!["thr_1".into(), "Notes".into()]);
        assert!(
            matches!(op, kg_domain::Op::Thread(kg_domain::ThreadOp::Rename { id, title }) if id.0 == "thr_1" && title == "Notes")
        );
    }
}
