use super::{
    Kagan, SurfaceCommit,
    toast::{Toast, ToastCommand},
};

use super::{surface, web_panel};
use gpui_kit::{App, Context, Window};
use kg_core::Workspace;
use kg_domain::{PluginId, SurfaceOp};
use kg_plugins::{Follows, HostUi, HostUiEvent, PluginHost, PluginHostEvent, RunCommand};

impl Kagan {
    /// Watches the plugin host, so a plugin turned off or updated changes its panel at once.
    pub(super) fn watch_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The plugin host starts after the UI's own setup, so the key bindings for its commands are
        // installed here, once the host exists. Installing them again does nothing.
        crate::plugins::init(cx);
        if self.plugin_watch.is_some() {
            return;
        }
        let Some(host) = kg_plugins::PluginHost::try_global(cx) else {
            return;
        };
        self.plugin_watch = Some(cx.observe_in(&host, window, |this, _, window, cx| {
            this.sync_slots(window, cx);
            this.sync_replaced(window, cx);
            this.reconcile_plugin_views(window, cx);
            this.sync_overlays(window, cx);
            cx.notify();
        }));
        self.sync_overlays(window, cx);
        if let Some(ui) = HostUi::global(cx) {
            self.host_events.push(cx.subscribe_in(
                &ui,
                window,
                |this, _, event: &HostUiEvent, window, cx| this.host_ui(event, window, cx),
            ));
        }
        self.host_events.push(cx.subscribe_in(
            &host,
            window,
            |this, _, event: &PluginHostEvent, window, cx| {
                this.plugin_host_event(event, window, cx);
            },
        ));
    }

    /// Tells the plugin host which thread and project the window shows. Plugin panels that follow
    /// the thread read it from there.
    pub(super) fn tell_host(&self, window: &Window, cx: &mut App) {
        let Some(host) = HostUi::global(cx) else {
            return;
        };
        let thread = self.shown_thread(cx);
        let project = self.shown_project(cx);
        let follows: Vec<PluginId> = kg_plugins::contributions(cx)
            .map(|list| {
                list.into_iter()
                    .filter(|(_, ui)| {
                        ui.panels
                            .iter()
                            .any(|panel| matches!(panel.follows, Some(Follows::Thread)))
                    })
                    .map(|(plugin, _)| plugin)
                    .collect()
            })
            .unwrap_or_default();
        let handle = window.window_handle();
        let focused = window.is_window_active();
        host.update(cx, |host, cx| {
            host.set_context(handle, thread, project, focused, &follows, cx);
        });
    }
    /// A plugin asked the window for something: a thread, a panel, or a toast. Each request gets
    /// one reply, so the plugin's call returns the result instead of waiting on a dropped sender.
    pub(super) fn host_ui(
        &mut self,
        event: &HostUiEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            HostUiEvent::OpenThread { id, reply } => {
                self.select(id.clone(), window, cx);
                let _ = reply.try_send(Ok(()));
            }
            HostUiEvent::OpenPanel {
                plugin,
                panel,
                props,
                reply,
            } => {
                let reply = reply.clone();
                let step = self.commit(
                    SurfaceOp::Open {
                        plugin: plugin.clone(),
                        id: panel.clone(),
                    },
                    Some(props.clone()),
                    window,
                    cx,
                );
                match step {
                    Ok(SurfaceCommit::Ready(_)) => {
                        let _ = reply.try_send(Ok(()));
                    }
                    Ok(SurfaceCommit::Paint) => {
                        let kagan = cx.entity();
                        surface::schedule_settle(kagan, window, cx, move |settled| {
                            let _ = reply
                                .try_send(settled.map(|_| ()).map_err(|error| error.to_string()));
                        });
                    }
                    Err(error) => {
                        let _ = reply.try_send(Err(error.to_string()));
                    }
                }
            }
            HostUiEvent::Toast {
                plugin,
                message,
                action,
                reply,
            } => {
                self.show_toast(
                    Toast {
                        message: message.clone().into(),
                        action: action.as_ref().map(|action| ToastCommand {
                            plugin: plugin.clone(),
                            command: action.command.clone(),
                            label: action.label.clone().into(),
                        }),
                        undo: Vec::new(),
                    },
                    cx,
                );
                let _ = reply.try_send(Ok(()));
            }
            HostUiEvent::Comment {
                plugin,
                comment,
                reply,
            } => {
                let window = window.window_handle().window_id();
                if let Some(saved) = self.plugin_comment(plugin, comment, window, cx) {
                    let _ = reply.try_send(saved);
                }
            }
            HostUiEvent::PostPage {
                plugin,
                panel,
                message,
                reply,
            } => {
                let _ = reply.try_send(web_panel::post(plugin, panel, message, cx));
            }
        }
    }

    /// A plugin's own notification opens its thread. File views are not shown in this window, so
    /// a request for one gets an answer rather than a wait.
    pub(super) fn plugin_host_event(
        &mut self,
        event: &PluginHostEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PluginHostEvent::OpenThread(id) => {
                let visible = Workspace::global(cx)
                    .read(cx)
                    .thread(id)
                    .is_some_and(|thread| {
                        thread.read(cx).record().visibility == kg_domain::Visibility::Visible
                    });
                if visible {
                    self.select(id.clone(), window, cx);
                }
            }
        }
    }

    /// A plugin command bound to a key or a toast. It runs in the thread and project on screen.
    pub(super) fn run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = PluginHost::try_global(cx) else {
            return;
        };
        let task = PluginHost::dispatch_command(
            &host,
            action.plugin(),
            action.command(),
            self.shown_thread(cx),
            self.shown_project(cx),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = task.await {
                _ = this.update(cx, |this, cx| {
                    this.note = Some(error.to_string().into());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
