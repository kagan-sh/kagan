use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    AppContext as _, Context, Entity, KeyBinding, PathPromptOptions, Subscription, Task, Window,
    actions,
};
use kg_core::Workspace;
use kg_domain::{Listing, Op, OpError, OpResult, OutdatedPlugin, PluginId, PluginOp, RailAction};
use kg_plugins::PluginHost;

use super::Kagan;

const LOG_LINES: usize = 8;

actions!(
    plugins,
    [
        InstallPlugin,
        UpdatePlugin,
        ReloadPlugin,
        ShowPluginLogs,
        RemovePlugin,
        TogglePluginEnabled,
        TogglePluginDetails,
        ToggleInstallForm,
        AddIndex,
        SearchIndexes,
        CheckPluginUpdates,
        UpdateAllPlugins,
    ]
);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", TogglePluginDetails, Some("PluginRow")),
        KeyBinding::new("space", TogglePluginEnabled, Some("PluginRow")),
        KeyBinding::new("u", UpdatePlugin, Some("PluginUpdate")),
        KeyBinding::new("r", ReloadPlugin, Some("PluginRow")),
        KeyBinding::new("l", ShowPluginLogs, Some("PluginRow")),
        KeyBinding::new("x", RemovePlugin, Some("PluginRow")),
        KeyBinding::new("enter", InstallPlugin, Some("PluginInstall")),
        KeyBinding::new("enter", AddIndex, Some("PluginIndex")),
        KeyBinding::new("enter", SearchIndexes, Some("PluginSearch")),
    ]
}

// Hiding and showing move a plugin's rail icons only. The plugin keeps running, and its surfaces
// still open from menus, kg and the command palette.
pub(crate) fn set_rail(plugin: PluginId, action: RailAction, cx: &mut gpui_kit::App) {
    crate::kagan::dispatch::logged(Op::Plugin(PluginOp::Rail { id: plugin, action }), cx);
}

pub(super) struct Updates {
    pub(super) plugins: Vec<OutdatedPlugin>,
    pub(super) failed: Vec<String>,
}

pub(super) struct Plugins {
    pub(super) updates: Option<Updates>,
    /// What the last Update all did, shown until the next check.
    pub(super) note: Option<String>,
    /// True while a check or an update runs, so its buttons wait.
    pub(super) busy: bool,
    pub(super) source: Entity<InputState>,
    pub(super) form: bool,
    pub(super) expanded: Option<PluginId>,
    pub(super) logs: Option<(PluginId, String)>,
    pub(super) focus: Option<PluginId>,
    pub(super) index: Entity<InputState>,
    /// Why the last Add an index failed, shown beside its field until the field changes.
    pub(super) index_error: Option<String>,
    pub(super) query: Entity<InputState>,
    pub(super) found: Option<(Vec<Listing>, Vec<String>)>,
    load: Task<()>,
    ops: Vec<Task<()>>,
    _index_watch: Subscription,
}

impl Plugins {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Kagan>) -> Self {
        let index =
            cx.new(|cx| InputState::new(window, cx).placeholder("git:<url>@<ref> or a folder"));
        let index_watch = cx.subscribe(&index, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.plugins.index_error = None;
                cx.notify();
            }
        });
        Self {
            updates: None,
            note: None,
            busy: false,
            source: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Folder path or git:<url>@<ref>")
            }),
            form: false,
            expanded: None,
            logs: None,
            focus: None,
            index,
            index_error: None,
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Plugin name or words")),
            found: None,
            load: Task::ready(()),
            ops: Vec::new(),
            _index_watch: index_watch,
        }
    }
}

impl Kagan {
    pub(super) fn refresh_plugins(&mut self, cx: &mut Context<Self>) {
        let window = self.window;
        self.plugins.load = cx.spawn(async move |this, cx| {
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.sync_slots(window, cx);
                    cx.notify();
                });
            });
        });
    }

    pub(super) fn plugin_op(
        &mut self,
        op: PluginOp,
        attempt: &'static str,
        done: impl FnOnce(&mut Self, &Result<OpResult, OpError>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.run_plugin_op(
            op,
            move |this, result, cx| {
                done(this, &result, cx);
                if let Err(error) = result {
                    this.note = Some(format!("Could not {attempt}: {error}").into());
                }
            },
            cx,
        );
    }

    /// Runs a plugin op and hands its result to `finish`, which decides where a failure shows.
    pub(super) fn run_plugin_op(
        &mut self,
        op: PluginOp,
        finish: impl FnOnce(&mut Self, Result<OpResult, OpError>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let task = Workspace::dispatch(Op::Plugin(op), cx);
        let running = cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                finish(this, result, cx);
                this.refresh_plugins(cx);
                cx.notify();
            });
        });
        self.plugins.ops.push(running);
    }

    pub(super) fn install_plugin(&mut self, cx: &mut Context<Self>) {
        let source = self.plugins.source.read(cx).value().trim().to_owned();
        if !source.is_empty() {
            self.install_source(source, cx);
        }
    }

    pub(super) fn install_source(&mut self, source: String, cx: &mut Context<Self>) {
        let op = PluginOp::Install {
            source,
            plugin: None,
            thread_id: None,
        };
        let before: Vec<_> = PluginHost::try_global(cx)
            .map(|host| {
                host.read(cx)
                    .status()
                    .into_iter()
                    .map(|status| status.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        self.plugin_op(
            op,
            "install the plugin",
            move |this, result, cx| {
                if result.is_err() {
                    return;
                }
                this.plugins.form = false;
                this.reconcile_setup_guide(cx);
                let id = PluginHost::try_global(cx).and_then(|host| {
                    host.read(cx)
                        .status()
                        .into_iter()
                        .find(|status| !before.contains(&status.id))
                        .map(|status| status.id.clone())
                });
                this.plugins.focus = id;
                let window = this.window;
                let source = this.plugins.source.clone();
                cx.spawn(async move |_, cx| {
                    _ = cx.update_window(window, |_, window, cx| {
                        source.update(cx, |state, cx| state.set_value(String::new(), window, cx));
                    });
                })
                .detach();
            },
            cx,
        );
    }

    pub(super) fn choose_plugin_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        let window = self.window;
        let source = self.plugins.source.clone();
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            _ = cx.update_window(window, |_, window, cx| {
                source.update(cx, |state, cx| {
                    state.set_value(path.to_string_lossy().into_owned(), window, cx);
                });
            });
        })
        .detach();
    }

    pub(super) fn toggle_form(&mut self, cx: &mut Context<Self>) {
        self.plugins.form = !self.plugins.form;
        cx.notify();
    }

    pub(super) fn toggle_form_action(
        &mut self,
        _: &ToggleInstallForm,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_form(cx);
    }

    pub(super) fn check_updates_action(
        &mut self,
        _: &CheckPluginUpdates,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_updates(cx);
    }

    fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.plugins.busy {
            return;
        }
        self.plugins.busy = true;
        self.plugin_op(
            PluginOp::Outdated,
            "check for updates",
            |this, result, _| {
                this.plugins.busy = false;
                if let Ok(OpResult::Outdated { plugins, failed }) = result {
                    this.plugins.note = None;
                    this.plugins.updates = Some(Updates {
                        plugins: plugins.clone(),
                        failed: failed.clone(),
                    });
                }
            },
            cx,
        );
    }

    pub(super) fn update_all_action(
        &mut self,
        _: &UpdateAllPlugins,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.plugins.busy {
            return;
        }
        self.plugins.busy = true;
        self.plugin_op(
            PluginOp::Update {
                target: kg_domain::UpdateTarget::All,
                thread_id: None,
            },
            "update the plugins",
            |this, result, cx| {
                this.plugins.busy = false;
                match result {
                    Ok(OpResult::Updated {
                        updated,
                        failed,
                        needs_approval,
                    }) => {
                        this.plugins.note = Some(super::settings::update_outcome(
                            updated,
                            failed,
                            needs_approval,
                        ));
                    }
                    Err(error) => this.plugins.note = Some(error.to_string()),
                    Ok(_) => {}
                }
                this.plugins.updates = None;
                this.check_updates(cx);
            },
            cx,
        );
    }

    pub(super) fn toggle_details(&mut self, id: &PluginId, cx: &mut Context<Self>) {
        if self.plugins.expanded.as_ref() == Some(id) {
            self.plugins.expanded = None;
        } else {
            self.plugins.expanded = Some(id.clone());
            self.plugins.logs = None;
        }
        cx.notify();
    }

    pub(super) fn open_plugin_logs(
        &mut self,
        id: &PluginId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manage_plugins(window, cx);
        self.show_plugin_logs(id, cx);
    }

    pub(super) fn toggle_plugin_enabled(&mut self, id: &PluginId, cx: &mut Context<Self>) {
        let on = PluginHost::try_global(cx).is_some_and(|host| {
            host.read(cx)
                .status()
                .iter()
                .any(|status| &status.id == id && status.enabled)
        });
        let op = if on {
            PluginOp::Disable { id: id.clone() }
        } else {
            PluginOp::Enable { id: id.clone() }
        };
        self.plugin_op(op, "change the plugin", |_, _, _| {}, cx);
    }

    pub(super) fn show_plugin_logs(&mut self, id: &PluginId, cx: &mut Context<Self>) {
        self.plugins.expanded = Some(id.clone());
        self.plugins.logs = None;
        self.show_logs(id, cx);
    }

    pub(super) fn show_logs(&mut self, id: &PluginId, cx: &mut Context<Self>) {
        let op = PluginOp::Logs {
            id: id.clone(),
            follow: false,
            after: None,
        };
        let id = id.clone();
        self.plugin_op(
            op,
            "read the logs",
            move |this, result, _| {
                if let Ok(OpResult::Logs { text, .. }) = result {
                    let mut lines: Vec<&str> = text.lines().rev().take(LOG_LINES).collect();
                    lines.reverse();
                    this.plugins.logs = Some((id, lines.join("\n")));
                }
            },
            cx,
        );
    }
}
