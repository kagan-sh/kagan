use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_core::{Requester, Workspace};
use kg_domain::{
    ConfigOp, InteractionOption, Op, OpError, OpResult, Plugin, PluginId, PluginOp, PreferenceOp,
    ProjectId, ProjectOp, Slot, SlotChoice, ThreadId, ThreadOp,
};
use serde_json::{Map, Value};

use crate::jobs::Ended;
use crate::manifest::{AgentChoice, Grant, Setting};
use crate::source::{Fetched, Source, checkout, fetch};
use crate::{Discovered, PluginHost, Result, activate};

const NO_THREAD: &str = "reload from a thread";

pub(crate) async fn run(
    host: &Entity<PluginHost>,
    op: PluginOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        PluginOp::Start => Err(OpError::Failed(
            "starting a plugin thread needs a window".into(),
        )),
        PluginOp::New { .. }
        | PluginOp::Check { .. }
        | PluginOp::Share { .. }
        | PluginOp::Types { .. }
        | PluginOp::List
        | PluginOp::Remove { .. }
        | PluginOp::Reload { .. }
        | PluginOp::Dev { .. }
        | PluginOp::CancelInstall
        | PluginOp::Logs { .. } => develop(host, op, requester, cx).await,
        PluginOp::Cli {
            name,
            command,
            argv,
            cwd,
            thread_id,
            project_id,
        } => {
            let call = CliCall {
                name,
                command,
                argv,
                cwd,
                thread_id,
                project_id,
            };
            cli(host, call, cx).await
        }
        PluginOp::Command {
            id,
            command,
            thread_id,
            project_id,
        } => {
            let task = cx.update(|cx| {
                PluginHost::dispatch_command(host, &id, &command, thread_id, project_id, cx)
            });
            task.await
        }
        PluginOp::Install {
            source,
            plugin,
            thread_id,
        } => install(host, source, plugin, thread_id, requester, cx).await,
        PluginOp::Update { id, thread_id } => update(host, &id, thread_id, requester, cx).await,
        PluginOp::Index { op } => crate::index::run(host, op, cx).await,
        PluginOp::Search { text } => crate::index::search(host, text, cx).await,
        PluginOp::Enable { id } => enable(host, id, cx).await,
        PluginOp::Disable { id } => disable(host, id, cx).await,
        PluginOp::Config { id, op, thread_id } => {
            config(host, &id, op, thread_id, requester, cx).await
        }
        PluginOp::Use {
            slot,
            id,
            thread_id,
        } => use_slot(slot, id, thread_id, requester, cx).await,
    }
}

// A plugin fills a slot only when a person picks it: a request from a terminal or an agent asks.
async fn use_slot(
    slot: Slot,
    choice: SlotChoice,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if let SlotChoice::Plugin(id) = &choice {
        let offered = cx.update(|cx| {
            crate::contributions(cx).is_ok_and(|list| {
                list.iter()
                    .any(|(plugin, ui)| plugin == id && ui.replacement(slot).is_some())
            })
        });
        if !offered {
            return Err(OpError::Invalid(format!(
                "{id} does not offer a {}; kg plugin list shows what each plugin offers",
                slot.name()
            )));
        }
        if requester.is_some() {
            let question = format!("Let {id} draw the {}?", slot.name());
            choose(
                thread_id.as_ref(),
                requester,
                question.clone(),
                &question,
                permission_options(),
                cx,
            )
            .await?;
        }
    }
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: slot.preference(),
                value: Value::String(choice.to_string()),
            }),
            cx,
        )
    });
    set.await
}

struct CliCall {
    name: String,
    command: String,
    argv: Vec<String>,
    cwd: PathBuf,
    thread_id: Option<ThreadId>,
    project_id: Option<ProjectId>,
}

async fn cli(host: &Entity<PluginHost>, call: CliCall, cx: &mut AsyncApp) -> Result<OpResult> {
    let CliCall {
        name,
        command,
        argv,
        cwd,
        thread_id,
        project_id,
    } = call;
    let mut providers = host.read_with(cx, |host, _| {
        host.active
            .iter()
            .filter(|(_, active)| {
                active.manifest.cli.iter().any(|group| {
                    group.name == name && group.commands.iter().any(|item| item.name == command)
                })
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    });
    if providers.len() > 1 {
        let names: Vec<_> = providers.iter().map(|id| id.0.as_str()).collect();
        return Err(OpError::Invalid(format!(
            "kg {name} {command} is provided by more than one plugin: {}",
            names.join(", ")
        )));
    }
    let plugin = providers.pop().ok_or_else(|| {
        OpError::NotFound(format!("no enabled plugin provides kg {name} {command}"))
    })?;
    let project_id = match (project_id, &thread_id) {
        (Some(project), _) => Some(project),
        (None, Some(id)) => {
            let thread = cx
                .update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Get { id: id.clone() }), cx))
                .await?;
            let OpResult::Thread(thread) = thread else {
                return Err(OpError::Failed("thread get returned no thread".into()));
            };
            Some(thread.project_id)
        }
        (None, None) => {
            let projects = cx
                .update(|cx| Workspace::dispatch(Op::Project(ProjectOp::List), cx))
                .await?;
            let OpResult::Projects(projects) = projects else {
                return Err(OpError::Failed("project list returned no projects".into()));
            };
            projects
                .into_iter()
                .find(|project| cwd.starts_with(&project.path))
                .map(|project| project.id)
        }
    };
    let jobs = host.update(cx, |host, cx| host.load(&plugin, cx))?;
    let job = Map::from_iter([
        ("kind".to_owned(), "cli".into()),
        ("command".to_owned(), command.clone().into()),
        ("argv".to_owned(), argv.into()),
        ("cwd".to_owned(), cwd.display().to_string().into()),
        ("threadId".to_owned(), thread_id.map(|id| id.0).into()),
        ("projectId".to_owned(), project_id.map(|id| id.0).into()),
    ]);
    let unloaded = || {
        OpError::Failed(format!(
            "plugin {plugin} was unloaded before it answered {command}"
        ))
    };
    let (id, replied) = jobs.send(job).ok_or_else(unloaded)?;
    let timer = cx.background_executor().timer(crate::TOOL_TIMEOUT);
    let reply = match jobs.settled(&id, &replied, timer).await {
        Ok(reply) => reply,
        Err(Ended::Unloaded) => return Err(unloaded()),
        Err(Ended::TimedOut) => {
            return Err(OpError::Failed(format!(
                "plugin {plugin} did not answer {command} within {} s",
                crate::TOOL_TIMEOUT.as_secs()
            )));
        }
    };
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    let value = reply.map_err(|error| OpError::Failed(secrets.redact(&plugin, &error)))?;
    let exit_code = value["exitCode"]
        .as_i64()
        .and_then(|code| i32::try_from(code).ok())
        .ok_or_else(|| {
            OpError::Failed(format!(
                "plugin {plugin} answered {command} without exitCode"
            ))
        })?;
    let text = |key| secrets.redact(&plugin, value[key].as_str().unwrap_or_default());
    Ok(OpResult::Cli {
        exit_code,
        stdout: text("stdout"),
        stderr: text("stderr"),
    })
}

async fn develop(
    host: &Entity<PluginHost>,
    op: PluginOp,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        PluginOp::New { name, dir } => {
            let components = host.read_with(cx, |host, _| host.components());
            cx.background_spawn(async move { crate::development::new(&name, dir, &components) })
                .await
        }
        PluginOp::Check { path } => {
            let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
            cx.background_spawn(async move {
                let warnings = crate::development::check(&path, &data_dir)?;
                Ok(if warnings.is_empty() {
                    OpResult::Done
                } else {
                    OpResult::Warnings(warnings)
                })
            })
            .await
        }
        PluginOp::Share { path } => {
            let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
            cx.background_spawn(async move { crate::share::share(&path, &data_dir) })
                .await
        }
        PluginOp::Types { path } => {
            let components = host.read_with(cx, |host, _| host.components());
            cx.background_spawn(async move {
                crate::development::types(&path, &components).map(OpResult::Files)
            })
            .await
        }
        PluginOp::List => Ok(OpResult::Plugins(host.read_with(cx, |host, _| {
            host.records
                .values()
                .map(|record| Plugin {
                    error: host.failure(&record.id),
                    ..record.clone()
                })
                .collect()
        }))),
        PluginOp::Remove { id, thread_id } => remove(host, id, thread_id, requester, cx).await,
        PluginOp::CancelInstall => {
            if host.read_with(cx, |host, _| host.stop_install()) {
                Ok(OpResult::Done)
            } else {
                Err(OpError::Invalid("no git install is running".into()))
            }
        }
        PluginOp::Reload { id, thread_id } => {
            let record = record(host, &id, cx)?;
            if !record.enabled {
                return Err(OpError::Invalid(format!("plugin {id} is disabled")));
            }
            reload(host, record, thread_id, requester, cx).await
        }
        PluginOp::Dev { path, thread_id } => dev(host, path, thread_id, requester, cx).await,
        PluginOp::Logs { id, after, .. } => {
            record(host, &id, cx)?;
            let path = host.read_with(cx, |host, _| {
                host.data_dir.join("plugins").join(&id.0).join("logs.jsonl")
            });
            cx.background_spawn(async move { crate::development::logs(&path, after.unwrap_or(0)) })
                .await
        }
        PluginOp::Start
        | PluginOp::Cli { .. }
        | PluginOp::Command { .. }
        | PluginOp::Install { .. }
        | PluginOp::Update { .. }
        | PluginOp::Index { .. }
        | PluginOp::Search { .. }
        | PluginOp::Enable { .. }
        | PluginOp::Disable { .. }
        | PluginOp::Config { .. }
        | PluginOp::Use { .. } => Err(OpError::Failed("not a development operation".into())),
    }
}

async fn enable(host: &Entity<PluginHost>, id: PluginId, cx: &mut AsyncApp) -> Result<OpResult> {
    let record = record(host, &id, cx)?;
    if !record.enabled {
        let mut starting = record;
        starting.enabled = true;
        start(host, &starting, None, cx).await?;
        set_enabled(&id, true, cx).await?;
        host.update(cx, |host, _| {
            if let Some(record) = host.records.get_mut(&id) {
                record.enabled = true;
            }
        });
    }
    Ok(OpResult::Done)
}

async fn disable(host: &Entity<PluginHost>, id: PluginId, cx: &mut AsyncApp) -> Result<OpResult> {
    record(host, &id, cx)?;
    host.update(cx, |host, cx| {
        host.disable_loaded(&id, cx);
        if let Some(record) = host.records.get_mut(&id) {
            record.enabled = false;
        }
    });
    #[cfg(test)]
    if let Some(hold) = host.update(cx, |host, _| host.lifecycle_hold.take()) {
        let _ = hold.recv().await;
        host.update(cx, |host, cx| {
            host.disable_loaded(&id, cx);
            if let Some(record) = host.records.get_mut(&id) {
                record.enabled = false;
            }
        });
    }
    set_enabled(&id, false, cx).await?;
    Ok(OpResult::Done)
}

/// Starts a plugin. A start that fails leaves its reason on the plugin, in place of any older one.
async fn start(
    host: &Entity<PluginHost>,
    record: &Plugin,
    discovered: Option<Discovered>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let started = activate(host, record, discovered, cx).await;
    if let Err(error) = &started {
        let (id, reason) = (record.id.clone(), error.to_string());
        host.update(cx, |host, _| {
            host.warnings.insert(id, reason);
        });
    }
    started
}

async fn remove(
    host: &Entity<PluginHost>,
    id: PluginId,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    record(host, &id, cx)?;
    let question = format!("Remove “{id}”?");
    choose(
        thread_id.as_ref(),
        requester,
        question.clone(),
        &question,
        removal_options(),
        cx,
    )
    .await?;
    record(host, &id, cx)?;
    let declared = host.read_with(cx, |host, _| {
        host.active
            .get(&id)
            .map(|active| active.manifest.settings.secrets())
            .unwrap_or_default()
    });
    host.update(cx, |host, _| host.end_activation(&id));
    let workspace = cx.update(|cx| Workspace::global(cx));
    if let Err(error) = workspace
        .read_with(cx, |workspace, _| workspace.remove_plugin(id.clone()))
        .await
    {
        host.update(cx, |host, _| host.resume_activation(&id));
        return Err(error);
    }
    let data = host.read_with(cx, |host, _| host.plugin_data(&id));
    let store_ids = crate::page::read_store_ids(&data);
    host.update(cx, |host, cx| {
        host.unload(&id, cx);
        // Drop native pages now: they stay in a UI global until the next frame otherwise, and
        // WebKit then refuses remove_data_store with DataStoreInUse.
        host.drop_pages(&id, &store_ids, cx);
        host.active.remove(&id);
        host.records.remove(&id);
        host.warnings.remove(&id);
        host.skipped.remove(&id);
        host.identities.remove(&id);
        cx.notify();
    });
    let cleared = crate::page::remove_web_stores(store_ids, cx).await;
    crate::page::clear_removed_stores(&data, &cleared.removed);
    let mut warnings = cleared.warnings;
    let store = host.read_with(cx, |host, _| host.secrets.clone());
    let forgotten = cx
        .background_spawn(async move { store.forget(&id, &declared) })
        .await;
    match forgotten {
        Ok(kept) if !kept.is_empty() => warnings.push(format!(
            "the plugin is removed, but the system keychain kept its secrets {}; \
             delete them in the keychain, service kagan",
            kept.join(", ")
        )),
        Err(error) => warnings.push(format!(
            "the plugin is removed, but its secrets were not forgotten: {error}"
        )),
        Ok(_) => {}
    }
    Ok(if warnings.is_empty() {
        OpResult::Done
    } else {
        OpResult::Warnings(warnings)
    })
}

async fn dev(
    host: &Entity<PluginHost>,
    path: PathBuf,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let root = cx
        .background_spawn(async move { dunce::canonicalize(path) })
        .await?;
    let record = host
        .read_with(cx, |host, _| {
            host.records
                .values()
                .find(|record| record.root == root && record.commit.is_none())
                .cloned()
        })
        .ok_or_else(|| {
            OpError::NotFound(format!(
                "{} is not an installed local plugin; install it first",
                root.display()
            ))
        })?;
    if !record.enabled {
        return Err(OpError::Invalid(format!(
            "plugin {} is disabled",
            record.id
        )));
    }
    reload(host, record.clone(), thread_id, requester, cx).await?;
    Ok(OpResult::Plugins(vec![record]))
}

async fn reload(
    host: &Entity<PluginHost>,
    record: Plugin,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let plugins = data_dir.join("plugins");
    let root = record.root.clone();
    let mut fetched = cx
        .background_spawn(async move {
            let (_keep, never) = async_channel::bounded::<()>(1);
            fetch(
                &Source::Local(root),
                None,
                &PathBuf::new(),
                &data_dir,
                &never,
            )
        })
        .await?;
    fetched.commit.clone_from(&record.commit);
    if fetched.discovered.manifest.id != record.id {
        return Err(OpError::Invalid(
            "reload cannot change the plugin id".into(),
        ));
    }
    let grant = Grant::of(&fetched.discovered.manifest, &fetched.root);
    let approved: Grant = serde_json::from_value(record.approved.clone())?;
    if !grant.within(&approved) {
        let thread_id = thread_id.ok_or_else(|| OpError::Invalid(NO_THREAD.into()))?;
        approve(
            Some(&thread_id),
            requester,
            format!(
                "Reload {} ({})?",
                fetched.discovered.manifest.name, record.id
            ),
            Vec::new(),
            grant.added(&approved),
            false,
            cx,
        )
        .await?;
    }
    let current = self::record(host, &record.id, cx)?;
    if current != record {
        return Err(OpError::Invalid(
            "plugin changed while reload waited; retry it".into(),
        ));
    }
    let result = keep(host, fetched, record.source, grant, &plugins, false, cx).await?;
    let secrets = host.read_with(cx, |host, _| host.secrets.clone());
    let id = record.id;
    cx.background_spawn(async move { secrets.log(&id, crate::Level::Info, "reloaded") })
        .await?;
    Ok(result)
}

/// Frees the plugin host's install slot when the install ends, even when it is cancelled.
/// Holds the install slot. Dropping it frees the slot, even when the install is cancelled, and
/// tells the windows, which read the slot from the host.
struct Claim {
    slot: Rc<RefCell<Option<String>>>,
    stops: Rc<RefCell<Option<async_channel::Sender<()>>>>,
    host: Entity<PluginHost>,
    cx: AsyncApp,
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.slot.borrow_mut().take();
        self.stops.borrow_mut().take();
        let host = self.host.clone();
        self.cx
            .spawn(async move |cx| host.update(cx, |_, cx| cx.notify()))
            .detach();
    }
}

async fn install(
    host: &Entity<PluginHost>,
    source: String,
    plugin: Option<String>,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let slot = host.read_with(cx, |host, _| host.installing.clone());
    if let Some(running) = slot.borrow().as_ref() {
        return Err(OpError::Invalid(format!(
            "{running} is already installing; answer it in Questions first"
        )));
    }
    *slot.borrow_mut() = Some(source.clone());
    host.update(cx, |_, cx| cx.notify());
    let stops = host.read_with(cx, |host, _| host.install_stop.clone());
    let _claim = Claim {
        slot,
        stops: stops.clone(),
        host: host.clone(),
        cx: cx.clone(),
    };
    if let Some(thread_id) = &thread_id {
        cx.update(|cx| {
            Workspace::dispatch(
                Op::Thread(ThreadOp::Get {
                    id: thread_id.clone(),
                }),
                cx,
            )
        })
        .await?;
    }
    let (data_dir, indexes) =
        host.read_with(cx, |host, _| (host.data_dir.clone(), host.indexes.clone()));
    let plugins = data_dir.join("plugins");
    let (stop, stopped) = async_channel::bounded::<()>(1);
    let named = crate::index::named(&source);
    if named || source.starts_with("git:") {
        *stops.borrow_mut() = Some(stop);
        host.update(cx, |_, cx| cx.notify());
    }
    let (source, plugin, listed) = {
        let (indexes, stopped) = (indexes.clone(), stopped.clone());
        cx.background_spawn(
            async move { crate::index::resolve(&indexes, source, plugin, &stopped) },
        )
        .await?
    };
    let parsed = Source::parse(&source)?;
    let fetched = {
        let plugins = plugins.clone();
        let data_dir = data_dir.clone();
        cx.background_spawn(async move {
            fetch(
                &parsed,
                plugin.as_deref().map(Path::new),
                &plugins,
                &data_dir,
                &stopped,
            )
            .map(|fetched| (parsed, fetched))
        })
        .await
    };
    stops.borrow_mut().take();
    host.update(cx, |_, cx| cx.notify());
    let (parsed, fetched) = fetched?;
    let manifest = &fetched.discovered.manifest;
    let id = manifest.id.clone();
    if host.read_with(cx, |host, _| host.records.contains_key(&id)) {
        return Err(OpError::Invalid(format!(
            "plugin {id} is already installed; use kg plugin update {id}"
        )));
    }
    let grant = Grant::of(manifest, &fetched.root);
    approve(
        thread_id.as_ref(),
        requester,
        format!("Install {} {} ({id})?", manifest.name, manifest.version),
        crate::index::origin(
            &source,
            &fetched.root,
            fetched.commit.as_deref(),
            listed.as_ref(),
            indexes.as_deref().unwrap_or_default(),
        ),
        grant.lines(),
        true,
        cx,
    )
    .await?;
    if host.read_with(cx, |host, _| host.records.contains_key(&id)) {
        return Err(OpError::Invalid(format!(
            "plugin {id} was installed while this waited for approval"
        )));
    }
    let source = match parsed {
        Source::Git { .. } => source,
        Source::Local(_) => fetched.root.display().to_string(),
    };
    keep(host, fetched, source, grant, &plugins, true, cx).await
}

async fn update(
    host: &Entity<PluginHost>,
    id: &PluginId,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let record = record(host, id, cx)?;
    let data_dir = host.read_with(cx, |host, _| host.data_dir.clone());
    let plugins = data_dir.join("plugins");
    let fetched = {
        let (plugins, record, data_dir) = (plugins.clone(), record.clone(), data_dir.clone());
        cx.background_spawn(async move {
            let subfolder = match &record.commit {
                Some(commit) => record
                    .root
                    .strip_prefix(checkout(&plugins, &record.id, commit))
                    .map(Path::to_path_buf)
                    .ok(),
                None => None,
            };
            let (_keep, never) = async_channel::bounded::<()>(1);
            fetch(
                &Source::parse(&record.source)?,
                subfolder.as_deref(),
                &plugins,
                &data_dir,
                &never,
            )
        })
        .await?
    };
    let manifest = &fetched.discovered.manifest;
    if manifest.id != *id {
        return Err(OpError::Invalid(format!(
            "{} now holds plugin {}, not {id}",
            record.source, manifest.id
        )));
    }
    let grant = Grant::of(manifest, &fetched.root);
    let approved: Grant = serde_json::from_value(record.approved.clone()).unwrap_or_default();
    let within = grant.within(&approved);
    if within && fetched.commit.is_some() && fetched.commit == record.commit {
        return Ok(OpResult::Done);
    }
    if !within {
        let indexes = host.read_with(cx, |host, _| host.indexes.clone());
        approve(
            thread_id.as_ref(),
            requester,
            format!("Update {} to {} ({id})?", manifest.name, manifest.version),
            crate::index::origin(
                &record.source,
                &fetched.root,
                fetched.commit.as_deref(),
                None,
                indexes.as_deref().unwrap_or_default(),
            ),
            grant.added(&approved),
            false,
            cx,
        )
        .await?;
    }
    keep(host, fetched, record.source, grant, &plugins, false, cx).await
}

/// Records approved code with its grant and makes it active.
async fn keep(
    host: &Entity<PluginHost>,
    fetched: Fetched,
    source: String,
    grant: Grant,
    plugins: &Path,
    create: bool,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let commit = fetched.commit.clone();
    let plugins = plugins.to_path_buf();
    let (root, discovered) = cx
        .background_spawn(async move { fetched.keep(&plugins) })
        .await?;
    let id = discovered.manifest.id.clone();
    host.update(cx, |host, _| host.end_activation(&id));
    #[cfg(test)]
    if let Some(hold) = host.read_with(cx, |host, _| host.keep_hold.clone()) {
        let _ = hold.recv().await;
    }
    let approved = serde_json::to_value(&grant)?;
    let workspace = cx.update(|cx| Workspace::global(cx));
    let kept = workspace
        .read_with(cx, |workspace, _| {
            workspace.keep_plugin(
                id.clone(),
                root.clone(),
                source.clone(),
                commit.clone(),
                approved.clone(),
            )
        })
        .await?;
    let record = if let Some(record) = kept {
        record
    } else if create {
        let record = Plugin {
            id: id.clone(),
            root,
            source,
            commit,
            enabled: true,
            approved,
            error: None,
        };
        workspace
            .read_with(cx, |workspace, _| workspace.insert_plugin(record.clone()))
            .await?;
        record
    } else {
        return Err(OpError::Invalid("plugin was removed".into()));
    };
    host.update(cx, |host, cx| {
        host.records.insert(record.id.clone(), record.clone());
        cx.notify();
    });
    if record.enabled {
        start(host, &record, Some(discovered), cx).await?;
    }
    Ok(OpResult::Done)
}

/// `about` says where the code comes from, between the question and the powers.
async fn approve(
    thread_id: Option<&ThreadId>,
    requester: Option<Requester>,
    question: String,
    about: Vec<String>,
    lines: Vec<String>,
    install: bool,
    cx: &mut AsyncApp,
) -> Result<()> {
    let head = std::iter::once(question.clone())
        .chain(about)
        .collect::<Vec<_>>()
        .join("\n");
    let title = if lines.is_empty() {
        format!(
            "{head}\nIt asks for no extra powers. It cannot run programs or read your other files. It can read its own files and store data."
        )
    } else {
        let lines: Vec<String> = lines.iter().map(|line| format!("- {line}")).collect();
        format!("{head}\nIt:\n{}", lines.join("\n"))
    };
    let mut options = permission_options();
    if install {
        options[0].label = "Install".into();
        options[1].label = "Cancel".into();
    }
    choose(thread_id, requester, title, &question, options, cx).await
}

fn permission_options() -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: "approve".into(),
            label: "Approve".into(),
        },
        InteractionOption {
            id: "decline".into(),
            label: "Decline".into(),
        },
    ]
}

fn removal_options() -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: "decline".into(),
            label: "Cancel".into(),
        },
        InteractionOption {
            id: "approve".into(),
            label: "Remove".into(),
        },
    ]
}

async fn choose(
    thread_id: Option<&ThreadId>,
    requester: Option<Requester>,
    title: String,
    question: &str,
    options: Vec<InteractionOption>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let answer = cx
        .update(|cx| Workspace::ask_for_person(thread_id, requester, title, options, cx))
        .await?;
    if answer == "approve" {
        Ok(())
    } else {
        Err(OpError::Invalid(format!("declined: {question}")))
    }
}

/// Asks a person before a folder setting names a folder; returns that folder.
async fn approve_folder(
    setting: &Setting,
    value: &Value,
    question: String,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<Option<PathBuf>> {
    let (Setting::Folder { title }, Some(path)) = (setting, value.as_str()) else {
        return Ok(None);
    };
    let reads = vec![format!("reads {path} ({title})")];
    approve(
        thread_id.as_ref(),
        requester,
        question,
        Vec::new(),
        reads,
        false,
        cx,
    )
    .await?;
    Ok(Some(PathBuf::from(path)))
}

async fn config(
    host: &Entity<PluginHost>,
    id: &PluginId,
    op: ConfigOp,
    thread_id: Option<ThreadId>,
    requester: Option<Requester>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let record = record(host, id, cx)?;
    let key = match &op {
        ConfigOp::Get { key } | ConfigOp::Set { key, .. } | ConfigOp::Clear { key } => key.clone(),
    };
    let found = host.read_with(cx, |host, _| {
        host.activation(id).map(|activation| {
            let active = host.active.get(id)?;
            Some((
                active.manifest.name.clone(),
                active.manifest.settings.get(&key).cloned(),
                host.value(id, &key),
                activation,
                host.permit(id)?,
            ))
        })
    });
    let Some((name, setting, current, activation, permit)) = found.flatten() else {
        return Err(OpError::Invalid(format!(
            "plugin {} is disabled; enable it to change its settings",
            record.id
        )));
    };
    let setting =
        setting.ok_or_else(|| OpError::Invalid(format!("plugin {id} has no setting {key}")))?;
    match op {
        ConfigOp::Get { .. } => Ok(OpResult::Setting(current.unwrap_or(Value::Null))),
        ConfigOp::Set { value: None, .. } => Err(OpError::Invalid(format!(
            "setting {key} needs a value; kg reads it from a prompt or stdin when you leave it out"
        ))),
        ConfigOp::Set {
            value: Some(value), ..
        } if matches!(setting, Setting::Secret { .. }) => {
            let secret = checked(&key, &setting, value)?;
            keep_secret(host, id, &key, secret.as_str(), cx).await
        }
        ConfigOp::Clear { .. } if matches!(setting, Setting::Secret { .. }) => {
            keep_secret(host, id, &key, None, cx).await
        }
        ConfigOp::Clear { .. } => Err(OpError::Invalid(format!(
            "setting {key} is not a secret; set it instead"
        ))),
        ConfigOp::Set {
            value: Some(value), ..
        } => {
            let value = checked(&key, &setting, value)?;
            let asked = format!("Let {name} ({id}) read a folder?");
            let folder = approve_folder(&setting, &value, asked, thread_id, requester, cx).await?;
            host.read_with(cx, |host, _| stage(host, id, &key, folder.as_ref()))?;
            if host.read_with(cx, |host, _| host.activation(id)) != Some(activation) {
                return Err(OpError::Invalid(format!(
                    "plugin {id} changed while this was saving"
                )));
            }
            #[cfg(test)]
            if let Some(hold) = host.update(cx, |host, _| host.commit_hold.take()) {
                let _ = hold.recv().await;
            }
            let workspace = cx.update(|cx| Workspace::global(cx));
            #[cfg(test)]
            if let Some(hold) = host.update(cx, |host, _| host.write_hold.take()) {
                let _ = hold.recv().await;
            }
            let wrote = workspace
                .read_with(cx, |workspace, _| {
                    workspace.commit_plugin_config(
                        id.clone(),
                        key.clone(),
                        value.clone(),
                        folder.as_ref().map(|path| path.display().to_string()),
                        permit,
                        activation,
                    )
                })
                .await?;
            if !wrote {
                return Err(OpError::Invalid(format!(
                    "plugin {id} changed while this was saving"
                )));
            }
            host.update(cx, |host, cx| {
                remember_setting(host, id, &key, &value, folder.as_ref(), activation, cx)
            })?;
            Ok(OpResult::Done)
        }
    }
}

/// Writes or clears a secret in the store, never in the database, and returns the store's
/// warning when it fell back to a file.
async fn keep_secret(
    host: &Entity<PluginHost>,
    id: &PluginId,
    key: &str,
    secret: Option<&str>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let store = host.read_with(cx, |host, _| host.secrets.clone());
    let (plugin, name, kept) = (id.clone(), key.to_owned(), secret.map(str::to_owned));
    let warning = cx
        .background_spawn(async move {
            match &kept {
                Some(secret) => store.set(&plugin, &name, secret),
                None => store.clear(&plugin, &name).map(|()| None),
            }
        })
        .await?;
    host.update(cx, |_, cx| cx.notify());
    Ok(warning.map_or(OpResult::Done, |warning| OpResult::Warnings(vec![warning])))
}

fn remember_setting(
    host: &mut PluginHost,
    id: &PluginId,
    key: &str,
    value: &Value,
    folder: Option<&PathBuf>,
    activation: u64,
    cx: &mut gpui_kit::Context<PluginHost>,
) -> Result<()> {
    if host.activation(id) != Some(activation) {
        return Ok(());
    }
    if let Some(path) = folder {
        if let Some(active) = host.active.get_mut(id) {
            active.grant.paths.insert(key.to_owned(), path.clone());
        }
        if let Some(record) = host.records.get_mut(id) {
            let mut approved: Grant =
                serde_json::from_value(record.approved.clone()).unwrap_or_default();
            approved.paths.insert(key.to_owned(), path.clone());
            record.approved = serde_json::to_value(&approved)?;
        }
    }
    if let Some(active) = host.active.get(id)
        && let Some(setting) = active.manifest.settings.get(key).cloned()
    {
        active.set(key, &setting, value.clone())?;
    }
    // A folder is granted at load, so the next use loads with the new one.
    if folder.is_some() {
        host.unload(id, cx);
    }
    cx.notify();
    Ok(())
}

fn stage(host: &PluginHost, id: &PluginId, key: &str, folder: Option<&PathBuf>) -> Result<()> {
    if host.activation(id).is_none() {
        return Err(OpError::Invalid(format!(
            "plugin {id} changed while this was saving"
        )));
    }
    if folder.is_none() {
        return Ok(());
    }
    let record = host
        .records
        .get(id)
        .ok_or_else(|| OpError::Invalid(format!("plugin {id} was removed while this waited")))?;
    let approved: Grant = serde_json::from_value(record.approved.clone()).unwrap_or_default();
    let still_folder = approved.folders.contains_key(key)
        && host.active.get(id).is_some_and(|active| {
            matches!(
                active.manifest.settings.get(key),
                Some(Setting::Folder { .. })
            )
        });
    if !still_folder {
        return Err(OpError::Invalid(format!(
            "setting {key} of plugin {id} is no longer a folder"
        )));
    }
    Ok(())
}

async fn set_enabled(id: &PluginId, enabled: bool, cx: &mut AsyncApp) -> Result<()> {
    let id = id.clone();
    let workspace = cx.update(|cx| Workspace::global(cx));
    workspace
        .read_with(cx, |workspace, _| workspace.set_plugin_enabled(id, enabled))
        .await
}

fn checked(key: &str, setting: &Setting, value: Value) -> Result<Value> {
    let takes =
        |what: &str| OpError::Invalid(format!("setting {key} ({}) takes {what}", setting.title()));
    match setting {
        Setting::String { .. } if value.is_string() => Ok(value),
        Setting::String { .. } => Err(takes("text")),
        Setting::Boolean { .. } if value.is_boolean() => Ok(value),
        Setting::Boolean { .. } => Err(takes("true or false")),
        Setting::Select { options, .. } => match value.as_str() {
            Some(choice) if options.iter().any(|option| option == choice) => Ok(value),
            _ => Err(takes(&format!("one of {}", options.join(", ")))),
        },
        Setting::Folder { .. } => value
            .as_str()
            .filter(|path| !path.chars().any(char::is_control))
            .and_then(|path| dunce::canonicalize(path).ok())
            .filter(|path: &PathBuf| path.is_dir())
            .map(|path| Value::String(path.display().to_string()))
            .ok_or_else(|| takes("an existing folder")),
        Setting::Secret { .. } => {
            let fits = value.as_str().is_some_and(|text| {
                !text.is_empty()
                    && text.len() <= crate::secret::MAX_BYTES
                    && !text.chars().any(char::is_control)
            });
            if fits {
                Ok(value)
            } else {
                Err(takes(&format!(
                    "one line of text, at most {} bytes",
                    crate::secret::MAX_BYTES
                )))
            }
        }
        Setting::Provider { .. } => serde_json::from_value::<AgentChoice>(value)
            .ok()
            .filter(|choice| !choice.provider.is_empty())
            .and_then(|choice| serde_json::to_value(choice).ok())
            .ok_or_else(|| {
                takes(r#"{"provider": <id>, "model"?, "reasoningLevel"?, "permissionMode"?}"#)
            }),
    }
}

fn record(host: &Entity<PluginHost>, id: &PluginId, cx: &mut AsyncApp) -> Result<Plugin> {
    host.read_with(cx, |host, _| host.records.get(id).cloned())
        .ok_or_else(|| OpError::NotFound(format!("plugin {id} is not installed")))
}
