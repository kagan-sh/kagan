use gpui_kit::{Context, Entity};
use gpui_shell::ScriptView;
use kg_domain::{OpError, PluginId};

use crate::Result;
use crate::ask::{Job, forget_later};

use super::{MountError, PluginHost};

#[derive(Clone, Copy)]
enum Mount {
    Interactive,
    PaintOnly,
}
impl PluginHost {
    /// Mounts the view class of panel `panel` into `window`.
    ///
    /// The default export stays mounted in the plugin's hidden window for as long
    /// as any named view is alive. A construct or init failure releases that
    /// generation; `fail_mount` then unloads the plugin.
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
        let Some(found) = self.panel(id, panel) else {
            return Err(MountError::Missing(format!(
                "plugin {id} has no panel {panel}"
            )));
        };
        let Some(export) = found.view.clone() else {
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
        self.panel(id, panel).and_then(|item| item.web.clone())
    }

    fn panel(&self, id: &PluginId, panel: &str) -> Option<&crate::PanelContribution> {
        self.active
            .get(id)?
            .manifest
            .ui
            .panels
            .iter()
            .find(|item| item.id == panel)
    }

    /// Whether the panel keeps cookies and storage. Needs its `store` flag and an approved grant.
    #[must_use]
    pub fn web_store(&self, id: &PluginId, panel: &str) -> bool {
        self.allows_page_store(id) && self.panel(id, panel).is_some_and(|item| item.store)
    }

    /// Whether the approved grant lets this plugin keep page data.
    #[must_use]
    pub fn allows_page_store(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.page_store)
    }
}
impl PluginHost {
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
        let job = Job::Page {
            panel_id: panel,
            message: &message,
        };
        let Some(queued) = jobs.send_page(&job, alive, raw.len()) else {
            return Err(OpError::Invalid(format!(
                "plugin {id} cannot take another page message"
            )));
        };
        forget_later(jobs, queued, job.timeout(), cx);
        Ok(())
    }

    /// Writes a line to the plugin's log: something kagan did with the plugin's request, such as
    /// refusing a key the window already uses.
    pub fn report(&self, plugin: &PluginId, message: &str) {
        self.secrets.note(plugin, crate::Level::Warn, message);
    }

    /// Bytes of page payloads still held for `next()` on this plugin.
    #[must_use]
    pub fn pending_page_bytes(&self, id: &PluginId) -> usize {
        self.active
            .get(id)
            .and_then(|active| active.loaded.as_ref())
            .map_or(0, |loaded| loaded.jobs.pending_page_bytes())
    }
}
impl PluginHost {
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
        };
        match mounted {
            Ok(view) => {
                if let Some(loaded) = self
                    .active
                    .get_mut(id)
                    .and_then(|active| active.loaded.as_mut())
                {
                    loaded.views.push(view.downgrade());
                }
                Ok(view)
            }
            Err(error) => {
                let released = error
                    .downcast_ref::<gpui_shell::MountViewError>()
                    .is_none_or(gpui_shell::MountViewError::is_generation_released);
                let error = self.secrets.redact(id, &format!("{error:#}"));
                if released {
                    self.fail_mount(id, &error, cx);
                }
                Err(MountError::Failed(error))
            }
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

    fn fail_mount(&mut self, id: &PluginId, error: &str, cx: &mut Context<Self>) {
        if self
            .active
            .get(id)
            .and_then(|active| active.fault.as_deref())
            == Some(error)
        {
            return;
        }
        if let Some(active) = self.active.get_mut(id) {
            active.fault = Some(error.to_owned());
        }
        self.unload(id, cx);
        self.secrets
            .note_later(id, crate::log::Level::Error, error, cx);
        cx.notify();
    }
}
