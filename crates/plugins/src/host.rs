//! The plugin host: what is active, what each plugin may do, and the operations over it.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use std::sync::Mutex;
use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, EmptyView, Entity, WeakEntity, WindowOptions};
use gpui_shell::policy::{self, Policy};
use gpui_shell::{ScriptView, ShellRuntime};
use kg_domain::{Event, OpError, PluginId};
use serde_json::{Map, Value};

use crate::jobs::Jobs;
use crate::manifest::{Grant, Setting};
use crate::{
    DropPages, HostUi, Level, Loaded, Mount, MountError, PluginHost, PluginHostGlobal,
    PluginStatus, PluginTheme, Result, SET, event_data, index, secret, watchdog,
};

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

    #[must_use]
    pub fn settings(&self, id: &PluginId) -> Vec<(String, Setting, Value)> {
        self.active.get(id).map_or_else(Vec::new, |active| {
            active
                .manifest
                .settings
                .iter()
                .map(|(key, schema)| {
                    let value = self.value(id, key).unwrap_or(Value::Null);
                    (key.clone(), schema.clone(), value)
                })
                .collect()
        })
    }

    /// Said beside a secret setting when this system has no keychain, so secrets go to a file.
    #[must_use]
    pub fn secret_warning(&self) -> Option<String> {
        self.secrets.warning()
    }

    /// A setting's value as everyone but the plugin sees it: a secret is `"set"` or `null`.
    pub(crate) fn value(&self, id: &PluginId, key: &str) -> Option<Value> {
        let active = self.active.get(id)?;
        if let Some(Setting::Secret { .. }) = active.manifest.settings.get(key) {
            return Some(self.secrets.is_set(id, key).then_some(SET).into());
        }
        active.value(key)
    }

    /// Why a plugin is not running: a fault, a crash, or a start that failed.
    pub(crate) fn failure(&self, id: &PluginId) -> Option<String> {
        self.active
            .get(id)
            .and_then(|active| {
                active.fault.clone().or_else(|| {
                    active
                        .loaded
                        .as_ref()
                        .and_then(|loaded| loaded.watchdog.error.clone())
                })
            })
            .or_else(|| self.warnings.get(id).cloned())
            .map(|failure| self.secrets.redact(id, &failure))
    }

    /// Index locations without passwords.
    ///
    /// # Errors
    /// When the saved list cannot be read; the text says why.
    pub fn indexes(&self) -> std::result::Result<Vec<String>, OpError> {
        self.indexes
            .as_ref()
            .map(|saved| saved.iter().map(|known| index::shown(known)).collect())
            .map_err(|error| OpError::Failed(error.clone()))
    }

    #[must_use]
    pub fn allows_open_url(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.open_url)
    }

    #[must_use]
    pub fn installing(&self) -> Option<String> {
        self.installing.borrow().clone()
    }

    /// Stops the install that runs now, so its git steps end. Returns false when none runs.
    #[must_use]
    pub(crate) fn stop_install(&self) -> bool {
        self.install_stop.borrow_mut().take().is_some()
    }

    /// Whether a git install is downloading now, so Cancel applies.
    #[must_use]
    pub fn stoppable(&self) -> bool {
        self.install_stop.borrow().is_some()
    }

    #[must_use]
    pub fn status(&self) -> Vec<PluginStatus> {
        self.records
            .iter()
            .map(|(id, record)| {
                let active = self.active.get(id);
                let watchdog = active
                    .and_then(|active| active.loaded.as_ref())
                    .map(|loaded| &loaded.watchdog);
                let approved = crate::ops::approved(record).unwrap_or_else(|error| {
                    eprintln!("kg: {error}");
                    Grant::default()
                });
                PluginStatus {
                    id: id.clone(),
                    name: self
                        .identities
                        .get(id)
                        .map_or_else(|| id.0.clone(), |(name, _)| name.clone()),
                    version: self.identities.get(id).map(|(_, version)| version.clone()),
                    source: crate::index::shown(&record.source),
                    powers: approved.lines(),
                    enabled: record.enabled,
                    slow: watchdog.is_some_and(|watchdog| watchdog.slow),
                    error: self.failure(id),
                    note: self.skipped.get(id).cloned(),
                }
            })
            .collect()
    }

    /// The themes of enabled plugins.
    pub fn themes(&self) -> impl Iterator<Item = PluginTheme<'_>> {
        self.active.values().flat_map(|active| {
            active.themes.iter().map(|(id, file)| PluginTheme {
                id,
                plugin: &active.manifest.name,
                file,
            })
        })
    }

    /// Returns the installed plugin's icon, if it has a readable icon file.
    #[must_use]
    pub fn icon(&self, id: &PluginId) -> Option<&[u8]> {
        self.active
            .get(id)
            .map(|active| active.icon.as_slice())
            .filter(|icon| !icon.is_empty())
    }

    /// The script view `id` and the hosts that plugin's `powers.fetch` names.
    #[must_use]
    pub fn view_fetch(
        &self,
        id: gpui_kit::EntityId,
    ) -> Option<(WeakEntity<ScriptView>, PluginId, Vec<String>)> {
        for (plugin, active) in &self.active {
            let Some(loaded) = &active.loaded else {
                continue;
            };
            let view = if loaded.view.entity_id() == id {
                Some(loaded.view.downgrade())
            } else {
                loaded
                    .views
                    .iter()
                    .find(|view| view.entity_id() == id)
                    .cloned()
            };
            if let Some(view) = view {
                let hosts = active
                    .grant
                    .powers
                    .fetch
                    .iter()
                    .filter_map(|fetch| fetch.view_host().ok())
                    .collect();
                return Some((view, plugin.clone(), hosts));
            }
        }
        None
    }

    pub(crate) fn sample(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<(secret::Store, PluginId, String)> {
        let mut marks = Vec::new();
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
        }
        if !marks.is_empty() {
            cx.notify();
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

    pub(crate) fn load(&mut self, id: &PluginId, cx: &mut Context<Self>) -> Result<Rc<Jobs>> {
        if let Some(loaded) = self
            .active
            .get(id)
            .and_then(|active| active.loaded.as_ref())
        {
            return Ok(loaded.jobs.clone());
        }
        let failed = |error: &dyn std::fmt::Display| {
            OpError::Failed(format!("plugin {id} failed to load: {error}"))
        };
        let runtime = ShellRuntime::new_isolated_with_components(self.components.clone())
            .map_err(|e| failed(&e))?;
        let data = self.data_dir.join("plugins").join(&id.0).join("data");
        std::fs::create_dir_all(&data)?;
        let active = self
            .active
            .get_mut(id)
            .ok_or_else(|| OpError::NotFound(format!("plugin {id} is not enabled")))?;
        let jobs = Jobs::new();
        let (policy, processes) =
            active.policy(id, &data, &self.data_dir, &jobs, &self.secrets, cx)?;
        let options = WindowOptions {
            show: false,
            focus: false,
            ..WindowOptions::default()
        };
        let window = cx
            .open_window(options, |_, cx| cx.new(|_| EmptyView))
            .map_err(|e| failed(&e))?;
        let (root, entry) = (active.root.clone(), active.manifest.entry.clone());
        // gpui-shell hands a mounted view the default policy, so it is swapped in for the load only.
        // The application handle stays: named exports mount beside this default view and share its generation.
        let mounted = window
            .update(cx, |_, window, cx| {
                policy::set_default(policy);
                let mounted = runtime
                    .load_application(&root, &entry)
                    .and_then(|application| {
                        runtime
                            .mount_application(&application, window, cx)
                            .map(|view| (application, view))
                    });
                policy::set_default(Policy::new());
                mounted
            })
            .map_err(|e| failed(&e));
        let (application, view) =
            match mounted.and_then(|mounted| mounted.map_err(|e| failed(&format!("{e:#}")))) {
                Ok(mounted) => mounted,
                Err(error) => {
                    processes.update(cx, |processes, _| processes.close());
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                    let _ = self.secrets.log(id, Level::Error, &error.to_string());
                    self.warnings.insert(id.clone(), error.to_string());
                    return Err(error);
                }
            };
        active.generation += 1;
        active.loaded = Some(Loaded {
            watchdog: watchdog::Watchdog::default(),
            runtime,
            window: window.into(),
            view,
            views: Vec::new(),
            application,
            jobs: jobs.clone(),
            processes,
        });
        self.warnings.remove(id);
        Ok(jobs)
    }

    /// The activation that may commit this plugin's settings. A folder unload keeps it.
    /// A new start assigns a new number. `None` when no start is current.
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

    /// How many times this plugin has loaded, while it is loaded. `None` when it is not loaded: a
    /// reload leaves it unloaded until its next use, so its views from before are gone.
    #[must_use]
    pub fn generation(&self, id: &PluginId) -> Option<u64> {
        self.active
            .get(id)
            .filter(|active| active.loaded.is_some())
            .map(|active| active.generation)
    }

    /// Whether every enabled plugin found at launch has had its start.
    #[must_use]
    pub fn started(&self) -> bool {
        self.started
    }

    #[must_use]
    pub fn is_loaded(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.loaded.is_some())
    }

    /// Mounts the view class of panel `panel` into `window`.
    ///
    /// The default export stays mounted in the plugin's hidden window for as long
    /// as any named view is alive. A construct or init failure releases that
    /// generation; [`Self::reject_mount`] then unloads the plugin.
    ///
    /// # Errors
    ///
    /// [`MountError::Missing`] when the manifest has no such panel.
    /// [`MountError::Failed`] when the runtime refuses the view.
    pub fn mount_panel(
        &mut self,
        id: &PluginId,
        panel: &str,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> std::result::Result<Entity<ScriptView>, MountError> {
        let found = self.active.get(id).and_then(|active| {
            active
                .manifest
                .ui
                .panels
                .iter()
                .find(|item| item.id == panel)
                .map(|item| item.view.clone())
        });
        let Some(export) = found else {
            return Err(MountError::Missing(format!(
                "plugin {id} has no panel {panel}"
            )));
        };
        let Some(export) = export else {
            return Err(MountError::Missing(format!("panel {panel} is a web panel")));
        };
        self.mount_named_view(id, &export, window, cx)
    }

    /// Loads the plugin so its script can serve a web panel. The page itself is not a view.
    ///
    /// # Errors
    ///
    /// [`MountError::Failed`] when the runtime refuses the entry.
    pub fn load_plugin(
        &mut self,
        id: &PluginId,
        cx: &mut Context<Self>,
    ) -> std::result::Result<(), MountError> {
        self.load(id, cx)
            .map(|_| ())
            .map_err(|error| MountError::Failed(error.to_string()))
    }

    /// The HTML file panel `panel` shows, when that panel is a web panel.
    #[must_use]
    pub fn web_file(&self, id: &PluginId, panel: &str) -> Option<String> {
        self.active.get(id).and_then(|active| {
            active
                .manifest
                .ui
                .panels
                .iter()
                .find(|item| item.id == panel)
                .and_then(|item| item.web.clone())
        })
    }

    /// Whether the panel keeps cookies and storage. Needs its `store` flag and an approved grant.
    #[must_use]
    pub fn web_store(&self, id: &PluginId, panel: &str) -> bool {
        self.active
            .get(id)
            .filter(|active| active.grant.page_store)
            .and_then(|active| {
                active
                    .manifest
                    .ui
                    .panels
                    .iter()
                    .find(|item| item.id == panel)
                    .map(|item| item.store)
            })
            .unwrap_or(false)
    }

    /// Whether the approved grant lets this plugin keep page data.
    #[must_use]
    pub fn allows_page_store(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.page_store)
    }

    #[must_use]
    pub fn plugin_data(&self, id: &PluginId) -> PathBuf {
        self.data_dir.join("plugins").join(&id.0).join("data")
    }

    /// The plugin folder, for serving that plugin's own page.
    #[must_use]
    pub fn plugin_root(&self, id: &PluginId) -> Option<PathBuf> {
        self.active.get(id).map(|active| active.root.clone())
    }

    /// Hosts this plugin's `powers.fetch` names.
    #[must_use]
    pub fn fetch_hosts(&self, id: &PluginId) -> Vec<String> {
        self.active
            .get(id)
            .map(|active| {
                active
                    .grant
                    .powers
                    .fetch
                    .iter()
                    .map(|fetch| fetch.host.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Delivers one JSON page message to the plugin script as a `page` job.
    ///
    /// `alive` is the open page; when it drops, a still-queued job is discarded before `next()`.
    ///
    /// # Errors
    ///
    /// The text is over the size cap, is not JSON, the plugin has no such web panel loaded,
    /// the page is gone, or too many page jobs are already waiting.
    pub fn deliver_page(
        &mut self,
        id: &PluginId,
        panel: &str,
        raw: &str,
        alive: std::sync::Weak<()>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let message = crate::page::parse_message(raw)?;
        if self.web_file(id, panel).is_none() {
            return Err(OpError::Invalid(format!(
                "plugin {id} has no web panel {panel}"
            )));
        }
        if alive.upgrade().is_none() {
            return Err(OpError::Invalid("the web panel is not on screen".into()));
        }
        let jobs = self
            .active
            .get(id)
            .and_then(|active| active.loaded.as_ref())
            .map(|loaded| loaded.jobs.clone())
            .ok_or_else(|| OpError::Invalid(format!("plugin {id} is not loaded")))?;
        let Some((job, replied)) = crate::page::enqueue(&jobs, panel, message, alive, raw.len())
        else {
            return Err(OpError::Invalid(format!(
                "plugin {id} cannot take another page message"
            )));
        };
        cx.spawn(async move |_, cx| {
            let timer = cx.background_executor().timer(Duration::from_secs(10));
            let _ = jobs.settled(&job, &replied, timer).await;
        })
        .detach();
        Ok(())
    }

    /// Bytes of page payloads still held for `next()` on this plugin.
    #[must_use]
    pub fn pending_page_bytes(&self, id: &PluginId) -> usize {
        self.active
            .get(id)
            .and_then(|active| active.loaded.as_ref())
            .map_or(0, |loaded| loaded.jobs.pending_page_bytes())
    }

    /// Writes a line to the plugin's log: something kagan did with the plugin's request, such as
    /// refusing a key the window already uses.
    pub fn report(&self, plugin: &PluginId, message: &str) {
        let _ = self.secrets.log(plugin, crate::Level::Warn, message);
    }

    /// Mounts a declared export beside the default view.
    ///
    /// The returned view does not own the generation: dropping it leaves the
    /// hidden default view mounted. A missing export leaves that generation up.
    /// A construct or init failure releases it, and this unloads the plugin.
    ///
    /// # Errors
    ///
    /// [`MountError::Missing`] when the plugin does not declare `export`.
    /// [`MountError::Failed`] when the runtime refuses the view.
    pub fn mount_named_view(
        &mut self,
        id: &PluginId,
        export: &str,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> std::result::Result<Entity<ScriptView>, MountError> {
        if !self.declares_view(id, export) {
            return Err(MountError::Missing(format!(
                "plugin {id} does not export {export}"
            )));
        }
        self.mount(id, export, Mount::Interactive, window, cx)
    }

    /// Mounts the plugin's `ui.overlay` view so that it may only paint (decision 100). A view that
    /// asks for input, focus, a popup or the window fails for good, with the reason as its
    /// build error.
    ///
    /// # Errors
    ///
    /// As [`Self::mount_named_view`]; [`MountError::Missing`] when the plugin declares no overlay.
    pub fn mount_overlay(
        &mut self,
        id: &PluginId,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> std::result::Result<Entity<ScriptView>, MountError> {
        let export = self
            .active
            .get(id)
            .and_then(|active| active.manifest.ui.overlay.clone())
            .ok_or_else(|| MountError::Missing(format!("plugin {id} has no overlay")))?;
        self.mount(id, &export, Mount::PaintOnly, window, cx)
    }

    fn mount(
        &mut self,
        id: &PluginId,
        export: &str,
        mount: Mount,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> std::result::Result<Entity<ScriptView>, MountError> {
        self.load(id, cx)
            .map_err(|error| MountError::Failed(error.to_string()))?;
        let loaded = self
            .active
            .get(id)
            .and_then(|active| active.loaded.as_ref())
            .ok_or_else(|| MountError::Failed(format!("plugin {id} is not loaded")))?;
        let mounted = match mount {
            Mount::Interactive => {
                loaded
                    .runtime
                    .mount_exported_view(&loaded.application, export, window, cx)
            }
            Mount::PaintOnly => {
                loaded
                    .runtime
                    .mount_paint_only_view(&loaded.application, export, window, cx)
            }
        }
        .map_err(|error| MountError::Failed(format!("{error:#}")));
        if let Ok(view) = &mounted
            && let Some(loaded) = self
                .active
                .get_mut(id)
                .and_then(|active| active.loaded.as_mut())
        {
            loaded.views.push(view.downgrade());
        }
        match mounted {
            Err(MountError::Failed(error)) => {
                let error = self.secrets.redact(id, &error);
                self.reject_mount(id, &error, cx);
                Err(MountError::Failed(error))
            }
            mounted => mounted,
        }
    }

    fn declares_view(&self, id: &PluginId, export: &str) -> bool {
        let Some(ui) = self.active.get(id).map(|active| &active.manifest.ui) else {
            return false;
        };
        let named = |view: &str| view == export;
        ui.panels
            .iter()
            .any(|panel| panel.view.as_deref().is_some_and(named))
            || ui.settings.as_deref().is_some_and(named)
    }

    pub fn disable_loaded(&mut self, id: &PluginId, cx: &mut Context<Self>) {
        self.end_activation(id);
        self.unload(id, cx);
        self.active.remove(id);
        self.warnings.remove(id);
        cx.notify();
    }

    /// Unloads `id` after a mount releases its generation, records the error on
    /// the plugin row, and appends it to the plugin log.
    ///
    /// A missing export leaves the generation mounted. Those return false.
    pub fn reject_mount(&mut self, id: &PluginId, error: &str, cx: &mut Context<Self>) -> bool {
        if error.contains("pinned gpui-shell")
            || error.contains("export `")
            || error.contains("has no panel")
            || error.contains("does not export")
        {
            return false;
        }
        if self
            .active
            .get(id)
            .and_then(|active| active.fault.as_deref())
            == Some(error)
        {
            return true;
        }
        if let Some(active) = self.active.get_mut(id) {
            active.fault = Some(error.to_owned());
        }
        let secrets = self.secrets.clone();
        let plugin = id.clone();
        let message = error.to_owned();
        self.unload(id, cx);
        cx.spawn(async move |_, cx| {
            if let Err(error) = cx
                .background_spawn(async move {
                    secrets.log(&plugin, crate::log::Level::Error, &message)
                })
                .await
            {
                eprintln!("kg: plugin log: {error}");
            }
        })
        .detach();
        cx.notify();
        true
    }

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
