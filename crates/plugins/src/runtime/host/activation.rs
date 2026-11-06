use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use gpui_kit::{App, AppContext as _, AsyncApp, Context, EmptyView, Entity, WindowOptions};
use gpui_shell::policy::{self, Policy};
use gpui_shell::{Capabilities, ShellRuntime};
use kg_core::Workspace;
use kg_domain::{OpError, PluginId};
use serde_json::{Map, Value};

use crate::jobs::Jobs;
use crate::manifest::{self, AgentChoice, Grant, Setting, Tool};
use crate::{Level, Result, area, commands, http_grants, process, schedule, secret, watchdog};

use super::{Active, Discovered, Loaded, PluginHost, Schema};

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

pub(crate) async fn activate(
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
        let schedules = schedule::start(&record.id, &manifest.schedules, cx);
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
                _schedules: schedules,
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
type Prepared = (Policy, Entity<process::Processes>);

impl Active {
    pub(super) fn value(&self, key: &str) -> Option<Value> {
        match self.agents.borrow().get(key) {
            Some(choice) => serde_json::to_value(choice).ok(),
            None => self.settings.borrow().get(key).cloned(),
        }
    }

    pub(crate) fn set(&self, key: &str, setting: &Setting, value: Value) -> Result<()> {
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
                status: self
                    .manifest
                    .ui
                    .status
                    .as_ref()
                    .map(|status| status.title.clone()),
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

    pub(super) fn tool(&self, name: &str) -> Option<(&Tool, &Schema)> {
        self.manifest
            .tools
            .iter()
            .zip(&self.schemas)
            .find(|(tool, _)| tool.name == name)
    }
}
impl PluginHost {
    pub(crate) fn load(&mut self, id: &PluginId, cx: &mut Context<Self>) -> Result<Rc<Jobs>> {
        // A load whose view crashed is dead: retire it before handing out its queue again.
        let crashed = self
            .active
            .get(id)
            .and_then(|active| active.loaded.as_ref())
            .is_some_and(|loaded| loaded.watchdog.error.is_some());
        if crashed {
            self.unload(id, cx);
        }
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
                    if let Some(strip) = crate::PluginStrip::global(cx) {
                        strip.update(cx, |strip, cx| strip.clear(id, cx));
                    }
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
}
