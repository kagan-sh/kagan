mod area;
mod catalog;
mod clipboard;
mod commands;
mod contributions;
mod database;
mod development;
mod files;
mod host;
mod index;
#[cfg(test)]
mod index_test;
mod jobs;
pub mod keys;
mod log;

pub(crate) use log::{Level, log};
mod manifest;
mod ops;
mod page;
#[cfg(test)]
mod page_test;
mod process;
mod program;
mod secret;
mod share;
mod source;
#[cfg(test)]
mod tests;
mod threads;
mod ui_host;
mod watchdog;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, Global,
    Subscription, Task, WeakEntity,
};
use gpui_shell::policy::Policy;
use gpui_shell::{Capabilities, HttpRequestGrant, LoadedApplication, ScriptView, ShellRuntime};
use kg_core::{PluginRequest, Workspace};
use kg_domain::{
    AgentTool, Op, OpError, OpResult, PluginId, ProjectId, ThemeFile, ThreadId, ThreadOp,
};
use serde_json::{Map, Value};

use crate::jobs::{Ended, Jobs};
use crate::manifest::{Fetch, Grant, Manifest, Scope, Tool};
pub use catalog::{Example, capabilities, examples, kinds_in, matched};
pub use contributions::{Capability, Contributions, Follows, PanelContribution, Placement};
pub use manifest::{AgentChoice, Setting};
pub use page::{
    MESSAGE_CAP, PageFileError, SCHEME, StoreRemoval, clear_removed_stores, clear_stores,
    keep_store, parse_message, read_plugin_file, read_store_ids, store_dir,
};
pub use ui_host::{HostContext, HostUi, HostUiEvent, ReviewComment, ToastAction};
pub use watchdog::PluginStatus;

type Result<T> = std::result::Result<T, OpError>;
type DropPages = Rc<dyn Fn(&PluginId, &[[u8; 16]], &mut App)>;

fn event_data(kind: &kg_domain::EventKind) -> Value {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.get("data").cloned())
        .unwrap_or(Value::Null)
}

fn http_grants(fetch: &[Fetch]) -> std::result::Result<Vec<HttpRequestGrant>, String> {
    fetch.iter().map(Fetch::http_grant).collect()
}

const TOOL_TIMEOUT: Duration = Duration::from_mins(2);
const INSTRUCT_TIMEOUT: Duration = Duration::from_secs(2);
const INSTRUCT_CAP: usize = 4096;
const INSTRUCT_TOTAL: usize = 16 * 1024;
/// What every reader but the plugin sees for a secret setting that holds a value.
const SET: &str = "set";

/// Starts the plugin host: reads installed plugins without running them and serves
/// plugin operations and agent tools for the workspace. Call after `Workspace::open`.
///
/// # Errors
/// When the installed plugin records cannot be read.
pub fn init(cx: &mut App, components: gpui_shell::FrozenComponentRegistry) -> Task<Result<()>> {
    HostUi::init(cx);
    gpui_shell::init(cx);
    let workspace = Workspace::global(cx);
    let records = workspace.read(cx).plugins();
    let data_dir = workspace.read(cx).data_dir().to_owned();
    let identity_dir = data_dir.clone();
    let host = cx.new(|cx: &mut Context<PluginHost>| PluginHost {
        secrets: secret::Store::new(keyring_core::get_default_store(), data_dir.clone()),
        data_dir,
        installing: Rc::default(),
        indexes: index::load(&identity_dir),
        install_stop: Rc::default(),
        identities: BTreeMap::new(),
        warnings: BTreeMap::new(),
        skipped: BTreeMap::new(),
        records: BTreeMap::new(),
        started: false,
        active: BTreeMap::new(),
        next_activation: 0,
        permits: BTreeMap::new(),
        components,
        drop_pages: None,
        _events: cx.subscribe(&workspace, |_, _, event, cx| {
            PluginHost::deliver_workspace_event(event, cx);
        }),
        _watchdog: cx.spawn(async move |host, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let Ok(marks) = host.update(cx, PluginHost::sample) else {
                    break;
                };
                for (secrets, id, message) in marks {
                    if let Err(error) = cx
                        .background_spawn(async move {
                            secrets.log(&id, crate::log::Level::Warn, &message)
                        })
                        .await
                    {
                        eprintln!("kg: plugin log: {error}");
                    }
                }
            }
        }),
        _quit: cx.on_app_quit(PluginHost::stop_programs),
        #[cfg(test)]
        keep_hold: None,
        #[cfg(test)]
        commit_hold: None,
        #[cfg(test)]
        write_hold: None,
        #[cfg(test)]
        activation_hold: None,
        #[cfg(test)]
        lifecycle_hold: None,
    });
    cx.set_global(PluginHostGlobal(host.clone()));
    workspace.update(cx, |workspace, _| {
        workspace.set_plugin_host(Rc::new(dispatch));
    });
    let plugins = workspace.read(cx).data_dir().join("plugins");
    let data_dir = workspace.read(cx).data_dir().to_owned();
    cx.spawn(async move |cx| boot(host, records, identity_dir, data_dir, plugins, cx).await)
}

async fn boot(
    host: Entity<PluginHost>,
    records: impl std::future::Future<Output = Result<Vec<kg_domain::Plugin>>>,
    identity_dir: PathBuf,
    data_dir: PathBuf,
    plugins: PathBuf,
    cx: &mut AsyncApp,
) -> Result<()> {
    cx.background_spawn(async move { source::clear_staging(&plugins) })
        .await;
    let records = records.await?;
    let known: BTreeSet<_> = records.iter().map(|record| record.id.clone()).collect();
    page::retry_orphan_stores(&data_dir, &known, cx).await;
    for record in records {
        let root = record.root.clone();
        let data = identity_dir.clone();
        if let Ok(manifest) = cx
            .background_spawn(async move { manifest::read(&root, &data) })
            .await
        {
            host.update(cx, |host, _| {
                host.identities
                    .insert(manifest.id.clone(), (manifest.name, manifest.version));
            });
        }
        host.update(cx, |host, _| {
            host.records.insert(record.id.clone(), record.clone());
        });
        if record.enabled
            && let Err(error) = activate(&host, &record, None, cx).await
        {
            host.update(cx, |host, _| {
                host.warnings.insert(record.id.clone(), error.to_string());
            });
        }
    }
    host.update(cx, |host, cx| {
        host.started = true;
        cx.notify();
    });
    Ok(())
}

/// Enabled contributions, discovered without executing plugin code.
///
/// # Errors
/// When the plugin host has not been initialized.
pub fn contributions(cx: &App) -> Result<Vec<(PluginId, Contributions)>> {
    let host = cx
        .try_global::<PluginHostGlobal>()
        .ok_or_else(|| OpError::Failed("the plugin host is unavailable".into()))?;
    Ok(host
        .0
        .read(cx)
        .active
        .iter()
        .map(|(id, active)| (id.clone(), active.manifest.ui.clone()))
        .collect())
}

/// One plain line per power in an approved grant, as the install screen words them.
///
/// # Errors
/// When `approved` is not a grant.
pub fn power_lines(approved: &Value) -> Result<Vec<String>> {
    Ok(serde_json::from_value::<Grant>(approved.clone())?.lines())
}

pub use commands::{CommandContribution, CommandWhen, KeySource, RunCommand};

/// A theme an enabled plugin ships.
#[derive(Clone, Copy, Debug)]
pub struct PluginTheme<'a> {
    /// `<plugin id>/<theme id>`
    pub id: &'a str,
    /// The plugin's name
    pub plugin: &'a str,
    pub file: &'a ThemeFile,
}

#[derive(Clone, Debug)]
pub enum PluginHostEvent {
    OpenThread(ThreadId),
}

impl EventEmitter<PluginHostEvent> for PluginHost {}

struct PluginHostGlobal(Entity<PluginHost>);
impl Global for PluginHostGlobal {}

pub struct PluginHost {
    data_dir: PathBuf,
    secrets: secret::Store,
    /// The source of the install that runs now, if one does. One install runs at a time.
    installing: Rc<RefCell<Option<String>>>,
    /// Index locations, searched in this order, or why the saved list cannot be read.
    indexes: std::result::Result<Vec<String>, String>,
    /// Dropping this sender stops the install that runs now.
    install_stop: Rc<RefCell<Option<async_channel::Sender<()>>>>,
    /// Each installed plugin's name and version from its manifest, kept whether or not it runs.
    identities: BTreeMap<PluginId, (String, String)>,
    warnings: BTreeMap<PluginId, String>,
    skipped: BTreeMap<PluginId, String>,
    records: BTreeMap<PluginId, kg_domain::Plugin>,
    // Set once every enabled plugin found at launch has had its start.
    started: bool,
    active: BTreeMap<PluginId, Active>,
    next_activation: u64,
    permits: BTreeMap<PluginId, Arc<Mutex<Option<u64>>>>,
    components: gpui_shell::FrozenComponentRegistry,
    drop_pages: Option<DropPages>,
    _events: Subscription,
    _watchdog: Task<()>,
    _quit: Subscription,
    #[cfg(test)]
    pub(crate) keep_hold: Option<async_channel::Receiver<()>>,
    #[cfg(test)]
    pub(crate) commit_hold: Option<async_channel::Receiver<()>>,
    #[cfg(test)]
    pub(crate) write_hold: Option<async_channel::Receiver<()>>,
    #[cfg(test)]
    pub(crate) activation_hold: Option<async_channel::Receiver<()>>,
    #[cfg(test)]
    pub(crate) lifecycle_hold: Option<async_channel::Receiver<()>>,
}

struct Active {
    root: PathBuf,
    manifest: Manifest,
    icon: Vec<u8>,
    themes: Vec<(String, ThemeFile)>,
    grant: Grant,
    schemas: Vec<Schema>,
    settings: Rc<RefCell<Map<String, Value>>>,
    agents: Agents,
    bindings: BTreeMap<String, Option<String>>,
    fault: Option<String>,
    loaded: Option<Loaded>,
    /// Counts each load of this plugin, so a view from an older load can be told apart.
    generation: u64,
    activation: u64,
}

// Kept apart from the other settings so `host.settings()` never shows them. `None` until picked.
pub(crate) type Agents = Rc<RefCell<BTreeMap<String, Option<AgentChoice>>>>;

pub(crate) struct Discovered {
    manifest: Manifest,
    schemas: Vec<Schema>,
    icon: Vec<u8>,
    themes: Vec<(String, ThemeFile)>,
}

struct Schema {
    value: Value,
    validator: jsonschema::Validator,
}

struct Loaded {
    watchdog: watchdog::Watchdog,
    window: AnyWindowHandle,
    // Drops before `application` and `runtime`: script values must release
    // while the VM that owns them is still alive.
    view: Entity<ScriptView>,
    /// The named views mounted from this load: panels and settings views. Watched for a render
    /// failure, and not kept alive here.
    views: Vec<WeakEntity<ScriptView>>,
    application: LoadedApplication,
    jobs: Rc<Jobs>,
    processes: Entity<process::Processes>,
    runtime: Rc<ShellRuntime>,
}

/// Why a named export did not mount.
#[derive(Debug)]
pub enum MountError {
    /// The plugin does not declare this export. The generation stays mounted.
    Missing(String),
    /// The runtime refused the view.
    Failed(String),
}

pub(crate) fn discover(root: &Path, data_dir: &Path) -> Result<Discovered> {
    let manifest = manifest::read(root, data_dir)?;
    let icon = manifest
        .icon
        .as_ref()
        .and_then(|path| {
            let root = root.canonicalize().ok()?;
            let path = root.join(path).canonicalize().ok()?;
            path.starts_with(&root)
                .then_some(path)
                .and_then(|path| std::fs::read(path).ok())
        })
        .unwrap_or_default();
    let schemas = manifest
        .tools
        .iter()
        .map(|tool| {
            let invalid = |message: String| {
                OpError::Invalid(format!(
                    "plugin {} tool {} input {}: {message}",
                    manifest.id,
                    tool.name,
                    tool.input.display()
                ))
            };
            let text = std::fs::read_to_string(root.join(&tool.input))
                .map_err(|error| invalid(error.to_string()))?;
            let value: Value =
                serde_json::from_str(&text).map_err(|error| invalid(error.to_string()))?;
            if value["type"] != "object" {
                return Err(invalid("the schema must have type object".into()));
            }
            let validator =
                jsonschema::validator_for(&value).map_err(|error| invalid(error.to_string()))?;
            Ok(Schema { value, validator })
        })
        .collect::<Result<_>>()?;
    let themes = manifest.themes(root).map_err(|message| {
        OpError::Invalid(format!(
            "{} in {}: {message}",
            manifest::FILE,
            root.display()
        ))
    })?;
    Ok(Discovered {
        manifest,
        schemas,
        icon,
        themes,
    })
}

async fn activate(
    host: &Entity<PluginHost>,
    record: &kg_domain::Plugin,
    discovered: Option<Discovered>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let discovered = if let Some(discovered) = discovered {
        discovered
    } else {
        let root = record.root.clone();
        let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
        cx.background_spawn(async move { discover(&root, &data_dir) })
            .await?
    };
    if discovered.manifest.id != record.id {
        return Err(OpError::Invalid(format!(
            "{} holds plugin {}, not {}",
            record.root.display(),
            discovered.manifest.id,
            record.id
        )));
    }
    let approved = crate::ops::approved(record)?;
    let asked = Grant::of(&discovered.manifest, &record.root);
    if !asked.within(&approved) {
        host.update(cx, |host, cx| {
            host.end_activation(&record.id);
            host.unload(&record.id, cx);
            host.active.remove(&record.id);
        });
        return Err(OpError::Invalid(format!(
            "plugin {id} needs approval for: {}; run `kg plugin update {id}` from a thread",
            asked.added(&approved).join(", "),
            id = record.id
        )));
    }
    let mut grant = approved.intersect(&asked);
    host.update(cx, |host, _| host.end_activation(&record.id));
    let workspace = cx.update(|cx| Workspace::global(cx));
    grant.keep_paths(&stored_grant(&record.id, &workspace, cx).await?);
    let declared = &discovered.manifest.settings;
    let (settings, agents) = stored_settings(host, &record.id, declared, &workspace, cx).await?;
    load_secrets(host, &record.id, discovered.manifest.settings.secrets(), cx).await;
    let bindings = plugin_setting(host, &record.id, &workspace, commands::BINDINGS, cx).await?;
    let bindings = match bindings.map(serde_json::from_value).transpose() {
        Ok(bindings) => bindings.unwrap_or_default(),
        Err(error) => {
            host.update(cx, |host, _| host.resume_activation(&record.id));
            return Err(OpError::Failed(error.to_string()));
        }
    };
    host.update(cx, |host, cx| {
        host.unload(&record.id, cx);
        let generation = host
            .active
            .get(&record.id)
            .map_or(0, |active| active.generation);
        let activation = host.begin_activation(&record.id);
        host.warnings.remove(&record.id);
        host.skipped.remove(&record.id);
        let manifest = &discovered.manifest;
        let identity = (manifest.name.clone(), manifest.version.clone());
        host.identities.insert(record.id.clone(), identity);
        host.active.insert(
            record.id.clone(),
            Active {
                root: record.root.clone(),
                manifest: discovered.manifest,
                icon: discovered.icon,
                themes: discovered.themes,
                grant,
                schemas: discovered.schemas,
                settings: Rc::new(RefCell::new(settings)),
                agents: Rc::new(RefCell::new(agents)),
                bindings,
                fault: None,
                loaded: None,
                generation,
                activation,
            },
        );
        cx.notify();
    });
    pause_for_activation(host, cx).await;
    Ok(())
}

async fn pause_for_activation(host: &Entity<PluginHost>, cx: &mut AsyncApp) {
    if let Some(hold) = activation_hold(host, cx) {
        let _ = hold.recv().await;
    }
}

#[cfg(test)]
fn activation_hold(
    host: &Entity<PluginHost>,
    cx: &mut AsyncApp,
) -> Option<async_channel::Receiver<()>> {
    host.update(cx, |host, _| host.activation_hold.take())
}

#[cfg(not(test))]
fn activation_hold(
    _host: &Entity<PluginHost>,
    _cx: &mut AsyncApp,
) -> Option<async_channel::Receiver<()>> {
    None
}

async fn stored_grant(
    id: &PluginId,
    workspace: &Entity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<Grant> {
    let id = id.clone();
    let plugins = workspace
        .read_with(cx, |workspace, _| workspace.plugins())
        .await?;
    let Some(plugin) = plugins.into_iter().find(|plugin| plugin.id == id) else {
        return Ok(Grant::default());
    };
    Ok(serde_json::from_value(plugin.approved).unwrap_or_default())
}

/// The stored settings, with the agent picks kept apart from them.
async fn stored_settings(
    host: &Entity<PluginHost>,
    id: &PluginId,
    declared: &manifest::Settings,
    workspace: &Entity<Workspace>,
    cx: &mut AsyncApp,
) -> Result<(Map<String, Value>, BTreeMap<String, Option<AgentChoice>>)> {
    let mut settings = Map::new();
    let mut agents = BTreeMap::new();
    for (key, setting) in declared.iter() {
        let stored = plugin_setting(host, id, workspace, key, cx).await?;
        match setting {
            Setting::Provider { .. } => {
                let choice = stored.and_then(|value| serde_json::from_value(value).ok());
                agents.insert(key.clone(), choice);
            }
            Setting::Secret { .. } => {
                // A key that was a plain setting before keeps no copy of its value in the database.
                if stored.is_some_and(|value| !value.is_null()) {
                    let cleared = workspace
                        .read_with(cx, |workspace, _| {
                            workspace.set_plugin_value(id.clone(), key.clone(), Value::Null)
                        })
                        .await;
                    if let Err(error) = cleared {
                        host.update(cx, |host, _| host.resume_activation(id));
                        return Err(error);
                    }
                }
                settings.insert(key.clone(), Value::Null);
            }
            Setting::Project { .. } => {
                let value = stored
                    .filter(|value| value.is_null() || value.is_string())
                    .unwrap_or_else(|| setting.default_value());
                settings.insert(key.clone(), value);
            }
            Setting::Number { .. } => {
                let value = stored
                    .and_then(number_value)
                    .unwrap_or_else(|| setting.default_value());
                settings.insert(key.clone(), value);
            }
            Setting::String { .. }
            | Setting::Boolean { .. }
            | Setting::Select { .. }
            | Setting::Folder { .. } => {
                let value = stored.unwrap_or_else(|| setting.default_value());
                settings.insert(key.clone(), value);
            }
        }
    }
    Ok((settings, agents))
}

fn number_value(value: Value) -> Option<Value> {
    match value {
        Value::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {
            Some(Value::Number(number))
        }
        Value::String(text) => match serde_json::from_str::<Value>(text.trim()) {
            Ok(Value::Number(number)) if number.as_f64().is_some_and(f64::is_finite) => {
                Some(Value::Number(number))
            }
            _ => None,
        },
        _ => None,
    }
}

/// A secret the keychain will not give is left unset and named in the plugin log.
async fn load_secrets(
    host: &Entity<PluginHost>,
    id: &PluginId,
    declared: Vec<String>,
    cx: &mut AsyncApp,
) {
    let store = host.read_with(cx, |host, _| host.secrets.clone());
    let id = id.clone();
    cx.background_spawn(async move {
        for problem in store.load(&id, &declared) {
            let _ = store.log(&id, Level::Warn, &problem);
        }
    })
    .await;
}

async fn plugin_setting(
    host: &Entity<PluginHost>,
    id: &PluginId,
    workspace: &Entity<Workspace>,
    key: &str,
    cx: &mut AsyncApp,
) -> Result<Option<Value>> {
    match workspace
        .read_with(cx, |workspace, _| {
            workspace.plugin_value(id.clone(), key.to_owned())
        })
        .await
    {
        Ok(stored) => Ok(stored),
        Err(error) => {
            host.update(cx, |host, _| host.resume_activation(id));
            Err(error)
        }
    }
}

fn dispatch(request: PluginRequest, cx: &mut App) -> Task<Result<OpResult>> {
    let host = cx.global::<PluginHostGlobal>().0.clone();
    match request {
        PluginRequest::Warnings => Task::ready(Ok(OpResult::Warnings({
            let host = host.read(cx);
            host.records
                .keys()
                .filter_map(|id| {
                    host.failure(id)
                        .or_else(|| host.skipped.get(id).cloned())
                        .map(|error| format!("plugin {id}: {error}"))
                })
                .collect()
        }))),
        PluginRequest::Tools { thread_id } => {
            cx.spawn(async move |cx| tools(&host, &thread_id, cx).await)
        }
        PluginRequest::CallTool {
            thread_id,
            name,
            input,
        } => cx.spawn(async move |cx| call_tool(&host, &thread_id, &name, input, cx).await),
        PluginRequest::Instruct {
            thread_id,
            project_id,
        } => cx.spawn(async move |cx| instruct(&host, &thread_id, &project_id, cx).await),
        PluginRequest::Plugin(op, requester) => {
            cx.spawn(async move |cx| ops::run(&host, op, requester, cx).await)
        }
        PluginRequest::Keymap(op) => {
            cx.spawn(async move |cx| commands::keymap(&host, op, cx).await)
        }
    }
}

async fn offered(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<Vec<(PluginId, String)>> {
    let plugins: Vec<(PluginId, Vec<(String, Scope)>)> = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .map(|(id, active)| {
                let tools = active
                    .manifest
                    .tools
                    .iter()
                    .map(|tool| (tool.name.clone(), tool.scope))
                    .collect();
                (id.clone(), tools)
            })
            .collect()
    });
    let mut names = BTreeSet::new();
    let mut offers = Vec::new();
    for (plugin, tools) in plugins {
        let tagged = if tools.iter().any(|(_, scope)| *scope == Scope::Tagged) {
            let metadata = cx
                .update(|cx| {
                    Workspace::dispatch(
                        Op::Thread(ThreadOp::Metadata {
                            id: thread_id.clone(),
                            plugin_id: plugin.clone(),
                        }),
                        cx,
                    )
                })
                .await?;
            !matches!(metadata, OpResult::Metadata(Value::Null))
        } else {
            false
        };
        for (name, scope) in tools {
            let visible = match scope {
                Scope::All => true,
                Scope::Tagged => tagged,
            };
            if visible && names.insert(name.clone()) {
                offers.push((plugin.clone(), name));
            }
        }
    }
    Ok(offers)
}

async fn tools(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let offers = offered(host, thread_id, cx).await?;
    Ok(host.read_with(cx, |host, _| {
        let mut instructions: Vec<&str> = Vec::new();
        let mut tools = Vec::new();
        for (plugin, name) in &offers {
            let Some(active) = host.active.get(plugin) else {
                continue;
            };
            let Some((tool, schema)) = active.tool(name) else {
                continue;
            };
            if let Some(text) = &active.manifest.instructions
                && !instructions.contains(&text.as_str())
            {
                instructions.push(text);
            }
            tools.push(AgentTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: schema.value.clone(),
            });
        }
        OpResult::Tools {
            instructions: (!instructions.is_empty()).then(|| instructions.join("\n\n")),
            tools,
        }
    }))
}

async fn call_tool(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    name: &str,
    input: Value,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let not_offered = || {
        OpError::NotFound(format!(
            "agent tool {name} is not offered to thread {thread_id}"
        ))
    };
    let (plugin, _) = offered(host, thread_id, cx)
        .await?
        .into_iter()
        .find(|(_, offered)| offered == name)
        .ok_or_else(not_offered)?;
    let errors = host.read_with(cx, |host, _| {
        let (_, schema) = host.active.get(&plugin)?.tool(name)?;
        Some(
            schema
                .validator
                .iter_errors(&input)
                .map(|error| format!("{}: {error}", error.instance_path()))
                .collect::<Vec<_>>(),
        )
    });
    let errors = errors.ok_or_else(not_offered)?;
    if !errors.is_empty() {
        return Err(OpError::Invalid(format!(
            "input for {name} does not match its schema: {}",
            errors.join("; ")
        )));
    }
    let jobs = host.update(cx, |host, cx| host.load(&plugin, cx))?;
    let job = Map::from_iter([
        ("kind".to_owned(), "tool".into()),
        ("name".to_owned(), name.into()),
        ("input".to_owned(), input),
        ("threadId".to_owned(), thread_id.0.clone().into()),
    ]);
    let unloaded = || {
        OpError::Failed(format!(
            "plugin {plugin} was unloaded before it answered {name}"
        ))
    };
    let (id, replied) = jobs.send(job).ok_or_else(unloaded)?;
    let timer = cx.background_executor().timer(TOOL_TIMEOUT);
    let reply = match jobs.settled(&id, &replied, timer).await {
        Ok(reply) => reply,
        Err(Ended::Unloaded) => return Err(unloaded()),
        Err(Ended::TimedOut) => {
            return Err(OpError::Failed(format!(
                "plugin {plugin} did not answer {name} within {} s",
                TOOL_TIMEOUT.as_secs()
            )));
        }
    };
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    match reply {
        Err(message) => Ok(OpResult::ToolReply {
            text: secrets.redact(&plugin, &message),
            is_error: true,
        }),
        Ok(value) => {
            let Some(text) = value["text"].as_str() else {
                return Err(OpError::Failed(format!(
                    "plugin {plugin} answered {name} without text"
                )));
            };
            Ok(OpResult::ToolReply {
                text: secrets.redact(&plugin, text),
                is_error: value["isError"].as_bool().unwrap_or(false),
            })
        }
    }
}

async fn instruct(
    host: &Entity<PluginHost>,
    thread_id: &ThreadId,
    project_id: &ProjectId,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let plugins: Vec<PluginId> = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .filter(|(_, active)| active.manifest.instruct)
            .map(|(id, _)| id.clone())
            .collect()
    });
    if plugins.is_empty() {
        return Ok(OpResult::Output(Some(String::new())));
    }
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    let mut tasks = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let Ok(jobs) = host.update(cx, |host, cx| host.load(&plugin, cx)) else {
            continue;
        };
        let job = Map::from_iter([
            ("kind".to_owned(), "instruct".into()),
            ("threadId".to_owned(), thread_id.0.clone().into()),
            ("projectId".to_owned(), project_id.0.clone().into()),
        ]);
        let Some((id, replied)) = jobs.send(job) else {
            continue;
        };
        let timer = cx.background_executor().timer(INSTRUCT_TIMEOUT);
        let secrets = secrets.clone();
        tasks.push(async move {
            let Ok(Ok(reply)) = jobs.settled(&id, &replied, timer).await else {
                return String::new();
            };
            let text = instruct_text(&reply);
            if text.is_empty() {
                return String::new();
            }
            let text = secrets.redact(&plugin, &text);
            let text = truncate(&text, INSTRUCT_CAP);
            format!("Plugin {plugin}:\n{text}")
        });
    }
    let parts = futures::future::join_all(tasks).await;
    let mut joined = String::new();
    for part in parts.into_iter().filter(|text| !text.is_empty()) {
        let next = if joined.is_empty() {
            part.len()
        } else {
            joined.len() + 2 + part.len()
        };
        if next > INSTRUCT_TOTAL {
            break;
        }
        if !joined.is_empty() {
            joined.push_str("\n\n");
        }
        joined.push_str(&part);
    }
    Ok(OpResult::Output(Some(joined)))
}

fn instruct_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => String::new(),
    }
}

fn truncate(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return text.to_owned();
    }
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

type Prepared = (Policy, Entity<process::Processes>);

impl Active {
    fn value(&self, key: &str) -> Option<Value> {
        match self.agents.borrow().get(key) {
            Some(choice) => serde_json::to_value(choice).ok(),
            None => self.settings.borrow().get(key).cloned(),
        }
    }

    fn set(&self, key: &str, setting: &Setting, value: Value) -> Result<()> {
        if let Setting::Provider { .. } = setting {
            let choice = serde_json::from_value(value)?;
            self.agents
                .borrow_mut()
                .insert(key.to_owned(), Some(choice));
        } else {
            self.settings.borrow_mut().insert(key.to_owned(), value);
        }
        Ok(())
    }

    fn policy(
        &self,
        id: &PluginId,
        data: &Path,
        workspace_dir: &Path,
        jobs: &Rc<Jobs>,
        secrets: &secret::Store,
        cx: &mut App,
    ) -> Result<Prepared> {
        let failed = |error: &dyn std::fmt::Display| {
            OpError::Failed(format!("plugin {id} failed to load: {error}"))
        };
        let mut read = vec![self.root.clone(), data.to_owned()];
        // Only a folder a person approved, and only while the setting still holds it.
        read.extend(
            self.grant
                .paths
                .iter()
                .filter(|(key, path)| {
                    self.settings.borrow().get(*key).and_then(Value::as_str) == path.to_str()
                })
                .map(|(_, path)| path.clone()),
        );
        for entry in &self.grant.powers.read {
            if let Some(folder) =
                crate::manifest::read_root(entry, workspace_dir).map_err(|error| failed(&error))?
            {
                read.push(folder);
            }
        }
        let read_roots = read.clone();
        let powers = &self.grant.powers;
        let write = vec![data.to_owned()];
        // A plugin sees no environment variable it was not granted: the shell's defaults include
        // proxy URLs, which often carry a password.
        let capabilities = Capabilities::new()
            .default_env_variables([])
            .read_roots(read)
            .write_roots(write)
            .storage(true)
            .clipboard_read(powers.clipboard.read)
            .clipboard_write(powers.clipboard.write)
            .http_requests(http_grants(&powers.fetch).map_err(|error| failed(&error))?);

        let processes = cx.new(|_| process::Processes::default());
        let policy = Policy::new()
            .with_application(&id.0)
            .with_data_dir(data)
            .with_capabilities(capabilities)
            .with_storage_path(data.join("store.json"));
        let bound = area::Bound {
            grant: &self.grant,
            root: &self.root,
            read_roots: &read_roots,
            processes: &processes,
        };
        let policy = area::modules(
            id,
            area::Values {
                settings: self.settings.clone(),
                secrets: secrets.clone(),
            },
            &self.agents,
            jobs,
            Some(&bound),
            data,
        )
        .into_iter()
        .filter(|(gate, _)| match gate {
            area::Gate::Always => true,
            area::Gate::Area(area) => powers.areas.contains(area),
            area::Gate::Read => !powers.read.is_empty(),
        })
        .map(|(_, module)| module)
        .try_fold(policy, Policy::with_host_module)
        .map_err(|e| failed(&e))?;
        Ok((policy, processes))
    }

    fn tool(&self, name: &str) -> Option<(&Tool, &Schema)> {
        self.manifest
            .tools
            .iter()
            .zip(&self.schemas)
            .find(|(tool, _)| tool.name == name)
    }
}

#[derive(Clone, Copy)]
enum Mount {
    Interactive,
    PaintOnly,
}
