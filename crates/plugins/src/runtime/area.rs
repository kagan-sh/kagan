use std::cell::RefCell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::{App, Entity};
use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use kg_core::Workspace;
use kg_domain::{Op, OpError, PluginId, ThreadOp};
use serde::Deserialize;
use serde_json::{Map, Number, Value};

use crate::manifest::{Area, Grant};

pub(crate) struct Bound<'a> {
    pub grant: &'a Grant,
    pub root: &'a Path,
    pub read_roots: &'a [PathBuf],
    pub processes: &'a Entity<crate::process::Processes>,
}

/// What `host.settings()` reads.
pub(crate) struct Values {
    pub settings: Rc<RefCell<Map<String, Value>>>,
    pub secrets: crate::secret::Store,
    /// The declared activity strip item, if the plugin has one.
    pub status: Option<crate::authoring::contributions::StatusContribution>,
}

pub(crate) enum Gate {
    Always,
    Area(Area),
    Read,
}

/// The app of the script call in progress. A host function runs inside that call, which already
/// holds the app, so it must reach the app through here and never through an `AsyncApp`.
pub(crate) fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Result<R, HostError> {
    gpui_shell::with_current_app(f).ok_or_else(|| HostError::new("no kagan app context"))
}

/// Runs `task` on the app's executor and gives back its result, or says the plugin unloaded
/// before it finished.
pub(crate) fn awaited<T: Send + 'static, F: Future<Output = T> + 'static>(
    task: F,
    cx: &mut App,
) -> impl Future<Output = Result<T, HostError>> + Send + use<T, F> {
    let (sender, receive) = async_channel::bounded(1);
    cx.spawn(async move |_| {
        // The plugin may have unloaded, so nobody may be waiting for the result.
        let _ = sender.send(task.await).await;
    })
    .detach();
    async move {
        receive
            .recv()
            .await
            .map_err(|_| HostError::new("the plugin was unloaded"))
    }
}

pub(crate) fn dispatch(
    op: Op,
) -> Result<impl Future<Output = Result<HostValue, HostError>> + Send + use<>, HostError> {
    let result = with_app(|cx| {
        let task = match op {
            Op::Thread(ThreadOp::Rename { id, title }) => Workspace::rename_once(id, title, cx),
            op => Workspace::dispatch(op, cx),
        };
        awaited(task, cx)
    })?;
    Ok(async move {
        let result = result.await?.map_err(error)?;
        crate::threads::shown(result).map(to_host).map_err(error)
    })
}

pub(crate) fn argument<T: for<'de> Deserialize<'de>>(
    args: &HostArguments,
    ix: usize,
) -> Result<T, HostError> {
    serde_json::from_value(from_host(args.value(ix)?)).map_err(error)
}

pub(crate) fn error(error: impl std::fmt::Display) -> HostError {
    HostError::new(error.to_string())
}

pub(crate) fn modules(
    id: &PluginId,
    values: Values,
    jobs: &Rc<crate::jobs::Jobs>,
    bound: Option<&Bound<'_>>,
    data: &Path,
) -> Vec<(Gate, HostModule)> {
    let process = match bound {
        Some(bound) => crate::process::module(bound.processes, bound.grant, bound.root),
        None => HostModule::new("kagan/process").declarations(crate::process::DECLARATIONS),
    };
    let files = crate::files::module(bound.map(|bound| bound.read_roots), id, &values.secrets);
    let status = values.status.clone();
    let secrets = values.secrets.clone();
    vec![
        (Gate::Always, jobs.module()),
        (Gate::Always, crate::page::module(id)),
        (
            Gate::Always,
            crate::ui_host::extend(host(id, values), id, status.as_ref(), &secrets),
        ),
        (Gate::Area(Area::Process), process),
        (Gate::Area(Area::Threads), crate::threads::module(id)),
        (
            Gate::Area(Area::Database),
            crate::database::module(&data.join("database.sqlite3")),
        ),
        (Gate::Area(Area::Browser), crate::browser::module()),
        (Gate::Read, files),
    ]
}

/// `kagan/host`, always granted.
pub(crate) fn host(id: &PluginId, values: Values) -> HostModule {
    let named = id.0.clone();
    let (logged, read, caller) = (id.clone(), id.clone(), id.clone());
    let Values {
        settings, secrets, ..
    } = values;
    let hidden = secrets.clone();
    HostModule::new("kagan/host")
        .declarations(
            "export function pluginId(): string;\n\
             export function log(level: \"debug\" | \"info\" | \"warn\" | \"error\", message: string): void;\n\
             /** Each declared setting by key. A `secret` setting is its text, or null until a person sets it. */\n\
             export function settings(): Record<string, unknown>;\n\
             /** Runs `name`, which another plugin lists in its `exports`, and resolves to its reply. */\n\
             export function call(plugin: string, name: string, input?: unknown): Promise<unknown>;",
        )
        .async_function("call", move |arguments| {
            crate::call::call(arguments, &caller)
        })
        .function("pluginId", move |_| Ok(named.as_str().into()))
        .function("log", move |arguments| {
            write_log(&hidden, &logged, arguments)
        })
        .function("settings", move |_| {
            let mut values = settings.borrow().clone();
            for (key, secret) in secrets.texts(&read) {
                values.insert(key, secret.into());
            }
            Ok(to_host(Value::Object(values)))
        })
}

fn write_log(
    secrets: &crate::secret::Store,
    plugin: &PluginId,
    arguments: &HostArguments,
) -> Result<HostValue, HostError> {
    let level: crate::Level = arguments
        .string(0)?
        .parse()
        .map_err(|error: OpError| HostError::new(error.to_string()))?;
    secrets
        .log(plugin, level, arguments.string(1)?)
        .map_err(|error| HostError::new(error.to_string()))?;
    Ok(HostValue::Null)
}

pub(crate) fn to_host(value: Value) -> HostValue {
    match value {
        Value::Null => HostValue::Null,
        Value::Bool(flag) => HostValue::Bool(flag),
        Value::Number(number) => HostValue::Number(number.as_f64().unwrap_or(f64::NAN)),
        Value::String(text) => HostValue::Str(text),
        Value::Array(items) => HostValue::Array(items.into_iter().map(to_host).collect()),
        Value::Object(fields) => HostValue::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, to_host(value)))
                .collect(),
        ),
    }
}

pub(crate) fn from_host(value: &HostValue) -> Value {
    match value {
        HostValue::Null => Value::Null,
        HostValue::Bool(flag) => Value::Bool(*flag),
        HostValue::Number(number) => number
            .to_string()
            .parse::<Number>()
            .map_or(Value::Null, Value::Number),
        HostValue::Str(text) => Value::String(text.clone()),
        HostValue::Array(items) => Value::Array(items.iter().map(from_host).collect()),
        HostValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), from_host(value)))
                .collect(),
        ),
    }
}
