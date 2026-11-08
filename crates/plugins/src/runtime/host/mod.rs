//! The plugin host: what is active, what each plugin may do, and the operations over it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Global, Subscription, Task,
    WeakEntity,
};
use gpui_shell::{LoadedApplication, ScriptView, ShellRuntime};
use kg_core::{PluginRequest, Workspace};
use kg_domain::{Event, OpResult, PluginId, ThemeFile};
use serde_json::{Map, Value};

use crate::jobs::Jobs;
use crate::manifest::{self, AgentChoice, Grant, Manifest};
use crate::secret;
use crate::source;
use crate::{
    DropPages, HostUi, Result, commands, event_data, index, ops, process, watchdog, web_store,
};

mod activation;
mod status;
mod tools;
mod views;

pub(crate) use activation::{activate, discover};
pub(crate) use tools::{call, instruct};

use tools::{call_tool, tools};

pub(crate) struct PluginHostGlobal(pub(crate) Entity<PluginHost>);
impl Global for PluginHostGlobal {}

/// Starts the plugin host: reads installed plugins without running them and serves
/// plugin operations and agent tools for the workspace. Call after `Workspace::open`.
///
/// # Errors
/// When the installed plugin records cannot be read.
pub fn init(cx: &mut App, components: gpui_shell::FrozenComponentRegistry) -> Task<Result<()>> {
    HostUi::init(cx);
    crate::PluginStrip::init(cx);
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
    web_store::retry_orphan_stores(&data_dir, &known, cx).await;
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
    let with_status: Vec<PluginId> = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .filter(|(_, active)| active.manifest.ui.status.is_some())
            .map(|(id, _)| id.clone())
            .collect()
    });
    for id in with_status {
        load_status(&host, &id, cx);
    }
    Ok(())
}

/// Loads a plugin whose manifest declares `ui.status`, so its line can appear without opening a
/// panel. Failures go to the plugin log and do not stop the others.
pub(crate) fn load_status(host: &Entity<PluginHost>, id: &PluginId, cx: &mut AsyncApp) {
    let declared = host.read_with(cx, |host, _| {
        host.active
            .get(id)
            .is_some_and(|active| active.manifest.ui.status.is_some())
    });
    if !declared {
        return;
    }
    if let Err(error) = host.update(cx, |host, cx| host.load(id, cx)) {
        let message = error.to_string();
        host.update(cx, |host, _| host.report(id, &message));
    }
}

pub struct PluginHost {
    pub(crate) data_dir: PathBuf,
    pub(crate) secrets: secret::Store,
    /// The source of the install that runs now, if one does. One install runs at a time.
    pub(crate) installing: Rc<RefCell<Option<String>>>,
    /// Index locations, searched in this order, or why the saved list cannot be read.
    pub(crate) indexes: std::result::Result<Vec<String>, index::Error>,
    /// Dropping this sender stops the install that runs now.
    pub(crate) install_stop: Rc<RefCell<Option<async_channel::Sender<()>>>>,
    /// Each installed plugin's name and version from its manifest, kept whether or not it runs.
    pub(crate) identities: BTreeMap<PluginId, (String, String)>,
    pub(crate) warnings: BTreeMap<PluginId, String>,
    pub(crate) skipped: BTreeMap<PluginId, String>,
    pub(crate) records: BTreeMap<PluginId, kg_domain::Plugin>,
    // Set once every enabled plugin found at launch has had its start.
    pub(crate) started: bool,
    pub(crate) active: BTreeMap<PluginId, Active>,
    pub(crate) next_activation: u64,
    pub(crate) permits: BTreeMap<PluginId, Arc<Mutex<Option<u64>>>>,
    pub(crate) components: gpui_shell::FrozenComponentRegistry,
    pub(crate) drop_pages: Option<DropPages>,
    pub(crate) _events: Subscription,
    pub(crate) _watchdog: Task<()>,
    pub(crate) _quit: Subscription,
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

pub(crate) struct Active {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) icon: Vec<u8>,
    pub(crate) themes: Vec<(String, ThemeFile)>,
    pub(crate) grant: Grant,
    pub(crate) schemas: Vec<Schema>,
    pub(crate) settings: Rc<RefCell<Map<String, Value>>>,
    pub(crate) agents: Agents,
    pub(crate) bindings: BTreeMap<String, Option<String>>,
    pub(crate) fault: Option<String>,
    pub(crate) loaded: Option<Loaded>,
    /// Counts each load of this plugin, so a view from an older load can be told apart.
    pub(crate) generation: u64,
    pub(crate) activation: u64,
    /// Dropping these stops the plugin's schedules.
    pub(crate) _schedules: Vec<Task<()>>,
}

// Kept apart from the other settings so `host.settings()` never shows them. `None` until picked.
pub(crate) type Agents = Rc<RefCell<BTreeMap<String, Option<AgentChoice>>>>;

pub(crate) struct Discovered {
    pub(crate) manifest: Manifest,
    pub(crate) schemas: Vec<Schema>,
    pub(crate) icon: Vec<u8>,
    pub(crate) themes: Vec<(String, ThemeFile)>,
}

pub(crate) struct Schema {
    pub(crate) value: Value,
    pub(crate) validator: jsonschema::Validator,
}

pub(crate) struct Loaded {
    pub(crate) watchdog: watchdog::Watchdog,
    pub(crate) window: AnyWindowHandle,
    // Drops before `application` and `runtime`: script values must release
    // while the VM that owns them is still alive.
    pub(crate) view: Entity<ScriptView>,
    /// The named views mounted from this load: panels and settings views. Watched for a render
    /// failure, and not kept alive here.
    pub(crate) views: Vec<WeakEntity<ScriptView>>,
    pub(crate) application: LoadedApplication,
    pub(crate) jobs: Rc<Jobs>,
    pub(crate) processes: Entity<process::Processes>,
    pub(crate) runtime: Rc<ShellRuntime>,
}

/// Why a named export did not mount.
#[derive(Debug)]
pub enum MountError {
    /// The plugin does not declare this export. The generation stays mounted.
    Missing(String),
    /// The runtime refused the view.
    Failed(String),
}

fn dispatch(request: PluginRequest, cx: &mut App) -> Task<Result<OpResult>> {
    let host = cx.global::<PluginHostGlobal>().0.clone();
    match request {
        PluginRequest::Warnings => Task::ready(Ok(OpResult::Warnings({
            let host = host.read(cx);
            let tool_notes = host.tool_notes();
            host.records
                .keys()
                .filter_map(|id| {
                    host.failure(id)
                        .or_else(|| host.skipped.get(id).cloned())
                        .or_else(|| tool_notes.get(id).cloned())
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
impl PluginHost {
    /// The app's whole component catalog, which plugin declarations are generated from.
    #[must_use]
    pub fn components(&self) -> gpui_shell::FrozenComponentRegistry {
        self.components.clone()
    }

    /// Registers a UI hook that drops native pages for a plugin before `WebKit` store removal.
    pub fn set_drop_pages(&mut self, drop_pages: DropPages) {
        self.drop_pages = Some(drop_pages);
    }

    pub(crate) fn drop_pages(&self, id: &PluginId, stores: &[[u8; 16]], cx: &mut App) {
        if let Some(drop_pages) = &self.drop_pages {
            drop_pages(id, stores, cx);
        }
    }

    #[must_use]
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<PluginHostGlobal>().0.clone()
    }

    #[must_use]
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<PluginHostGlobal>()
            .map(|global| global.0.clone())
    }
}
impl PluginHost {
    #[must_use]
    pub fn activation(&self, id: &PluginId) -> Option<u64> {
        self.permits.get(id).and_then(|permit| {
            *permit
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
    }

    pub(crate) fn permit(&self, id: &PluginId) -> Option<Arc<Mutex<Option<u64>>>> {
        self.permits.get(id).cloned()
    }

    pub(crate) fn end_activation(&mut self, id: &PluginId) {
        self.retarget(id, None);
    }

    pub(crate) fn begin_activation(&mut self, id: &PluginId) -> u64 {
        self.next_activation = self.next_activation.saturating_add(1);
        let activation = self.next_activation;
        self.retarget(id, Some(activation));
        activation
    }

    pub(crate) fn resume_activation(&mut self, id: &PluginId) {
        let activation = self.active.get(id).map(|active| active.activation);
        self.retarget(id, activation);
    }

    fn retarget(&mut self, id: &PluginId, activation: Option<u64>) {
        let permit = self
            .permits
            .entry(id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(None)));
        *permit
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = activation;
    }
}
impl PluginHost {
    pub(crate) fn sample(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<(secret::Store, PluginId, String)> {
        let mut marks = Vec::new();
        let mut crashed = Vec::new();
        for (id, active) in &mut self.active {
            let Some(loaded) = &mut active.loaded else {
                continue;
            };
            loaded.views.retain(|view| view.upgrade().is_some());
            let failed = loaded
                .views
                .iter()
                .filter_map(WeakEntity::upgrade)
                .chain(std::iter::once(loaded.view.clone()))
                .find_map(|view| view.read(cx).build_error().map(ToString::to_string));
            if let Some(message) = loaded
                .watchdog
                .sample(loaded.runtime.read_metrics(), failed.as_deref())
            {
                marks.push((self.secrets.clone(), id.clone(), message));
            }
            if loaded.watchdog.error.is_some() {
                crashed.push(id.clone());
            }
        }
        if !marks.is_empty() {
            cx.notify();
        }
        if !crashed.is_empty()
            && let Some(strip) = crate::PluginStrip::global(cx)
        {
            strip.update(cx, |strip, cx| {
                for id in &crashed {
                    strip.clear(id, cx);
                }
            });
        }
        marks
    }

    fn deliver_event(
        &mut self,
        event: &Event,
        project_id: &kg_domain::ProjectId,
        cx: &mut Context<Self>,
    ) {
        let subscription = format!("thread.{}", event.kind.type_name());
        let plugins: Vec<PluginId> = self
            .active
            .iter()
            .filter(|(_, active)| active.manifest.events.contains(&subscription))
            .map(|(id, _)| id.clone())
            .collect();
        if plugins.is_empty() {
            return;
        }
        let data = event_data(&event.kind);
        for plugin in plugins {
            let Ok(jobs) = self.load(&plugin, cx) else {
                continue;
            };
            let mut job = Map::new();
            job.insert("kind".into(), "event".into());
            job.insert("type".into(), event.kind.type_name().into());
            job.insert("threadId".into(), event.thread_id.0.clone().into());
            job.insert("projectId".into(), project_id.0.clone().into());
            job.insert("data".into(), data.clone());
            // The caller gets nothing back. The slot stays so the plugin can still settle the job.
            let Some((id, replied)) = jobs.send(job) else {
                continue;
            };
            cx.spawn(async move |_, cx| {
                let timer = cx.background_executor().timer(Duration::from_secs(10));
                let _ = jobs.settled(&id, &replied, timer).await;
            })
            .detach();
        }
    }
}
impl PluginHost {
    pub(crate) fn deliver_workspace_event(event: &kg_core::WorkspaceEvent, cx: &mut Context<Self>) {
        let kg_core::WorkspaceEvent::Event { event, project_id } = event else {
            return;
        };
        let event = event.clone();
        let project_id = project_id.clone();
        cx.spawn(async move |host, cx| {
            let _ = host.update(cx, |host, cx| host.deliver_event(&event, &project_id, cx));
        })
        .detach();
    }

    pub(crate) fn stop_programs(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl std::future::Future<Output = ()> + use<> {
        let finished: Vec<_> = self
            .active
            .values()
            .filter_map(|active| active.loaded.as_ref())
            .flat_map(|loaded| {
                loaded
                    .processes
                    .update(cx, |processes, _| processes.close())
            })
            .collect();
        async move {
            futures::future::join_all(finished.into_iter().map(|finished| async move {
                let _ = finished.recv().await;
            }))
            .await;
        }
    }

    pub(crate) fn unload(&mut self, id: &PluginId, cx: &mut App) {
        if let Some(strip) = crate::PluginStrip::global(cx) {
            strip.update(cx, |strip, cx| strip.clear(id, cx));
        }
        if let Some(ui) = HostUi::global(cx) {
            ui.update(cx, |ui, _| ui.unload(id));
        }
        if let Some(loaded) = self
            .active
            .get_mut(id)
            .and_then(|active| active.loaded.take())
        {
            loaded.jobs.close();
            loaded
                .processes
                .update(cx, |processes, _| processes.close());
            let _ = loaded
                .window
                .update(cx, |_, window, _| window.remove_window());
        }
    }
}
