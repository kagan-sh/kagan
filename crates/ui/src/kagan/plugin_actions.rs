use gpui_kit::component::input::InputState;
use gpui_kit::{
    AppContext as _, Context, Entity, KeyBinding, PathPromptOptions, Task, Window, actions,
};
use kg_core::Workspace;
use kg_domain::{Listing, Op, OpError, OpResult, PluginId, PluginOp};
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

pub(super) struct Plugins {
    pub(super) source: Entity<InputState>,
    pub(super) form: bool,
    pub(super) expanded: Option<PluginId>,
    pub(super) logs: Option<(PluginId, String)>,
    pub(super) focus: Option<PluginId>,
    pub(super) index: Entity<InputState>,
    pub(super) query: Entity<InputState>,
    pub(super) found: Option<(Vec<Listing>, Vec<String>)>,
    load: Task<()>,
    ops: Vec<Task<()>>,
}

impl Plugins {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Kagan>) -> Self {
        Self {
            source: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Folder path or git:<url>@<ref>")
            }),
            form: false,
            expanded: None,
            logs: None,
            focus: None,
            index: cx
                .new(|cx| InputState::new(window, cx).placeholder("git:<url>@<ref> or a folder")),
            query: cx.new(|cx| InputState::new(window, cx).placeholder("Plugin name or words")),
            found: None,
            load: Task::ready(()),
            ops: Vec::new(),
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
        let task = Workspace::dispatch(Op::Plugin(op), cx);
        let running = cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                done(this, &result, cx);
                if let Err(error) = result {
                    this.note = Some(format!("Could not {attempt}: {error}").into());
                }
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
