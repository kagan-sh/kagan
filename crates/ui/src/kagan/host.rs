use super::{
    Kagan, SurfaceCommit,
    toast::{Toast, ToastCommand},
};

use super::{surface, web_panel};
use gpui_kit::{App, Context, Window};
use kg_core::Workspace;
use kg_domain::{PluginId, SurfaceOp};
use kg_plugins::{Follows, HostUi, HostUiEvent, PluginHost, RunCommand};

impl Kagan {
    pub(super) fn watch_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The plugin host starts after the UI's own setup, so the key bindings for its commands are
        // installed here, once the host exists. Installing them again does nothing.
        crate::plugins::init(cx);
        if self.plugin_watch.is_none()
            && let Some(strip) = kg_plugins::PluginStrip::global(cx)
        {
            let ids = plugin_ids(strip.read(cx));
            self.sync_plugin_focus(&ids, cx);
            self.host_events.push(cx.observe(&strip, |this, strip, cx| {
                let ids = plugin_ids(strip.read(cx));
                this.sync_plugin_focus(&ids, cx);
                cx.notify();
            }));
        }
        if self.plugin_watch.is_some() {
            return;
        }
        let Some(host) = kg_plugins::PluginHost::try_global(cx) else {
            return;
        };
        self.plugin_watch = Some(cx.observe_in(&host, window, |this, host, window, cx| {
            this.activity
                .installing(host.read(cx).installing().as_deref());
            this.sync_slots(window, cx);
            this.sync_replaced(window, cx);
            this.reconcile_plugin_views(window, cx);
            this.sync_overlays(window, cx);
            cx.notify();
        }));
        self.activity
            .installing(host.read(cx).installing().as_deref());
        self.sync_overlays(window, cx);
        if let Some(ui) = HostUi::global(cx) {
            self.host_events.push(cx.subscribe_in(
                &ui,
                window,
                |this, _, event: &HostUiEvent, window, cx| this.host_ui(event, window, cx),
            ));
        }
    }

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
    pub(super) fn host_ui(
        &mut self,
        event: &HostUiEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            HostUiEvent::OpenThread { id, reply } => {
                self.select(id.clone(), window, cx);
                let _ = reply.try_send(Ok(())); // A caller that gave up reads no reply.
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
                        let _ = reply.try_send(Ok(())); // A caller that gave up reads no reply.
                    }
                    Ok(SurfaceCommit::Paint) => {
                        let kagan = cx.entity();
                        surface::schedule_settle(kagan, window, cx, move |settled| {
                            let _ = reply
                                .try_send(settled.map(|_| ()).map_err(|error| error.to_string())); // A caller that gave up reads no reply.
                        });
                    }
                    Err(error) => {
                        let _ = reply.try_send(Err(error.to_string())); // A caller that gave up reads no reply.
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
                let _ = reply.try_send(Ok(())); // A caller that gave up reads no reply.
            }
            HostUiEvent::Comment {
                plugin,
                comment,
                reply,
            } => {
                let window = window.window_handle().window_id();
                if let Some(saved) = self.plugin_comment(plugin, comment, window, cx) {
                    let _ = reply.try_send(saved); // A caller that gave up reads no reply.
                }
            }
            HostUiEvent::PostPage {
                plugin,
                panel,
                message,
                reply,
            } => {
                let _ = reply.try_send(web_panel::deliver(plugin, panel, message, cx)); // A caller that gave up reads no reply.
            }
        }
    }

    pub(super) fn open_thread(
        &mut self,
        id: &kg_domain::ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let listed = Workspace::global(cx)
            .read(cx)
            .thread(id)
            .is_some_and(|thread| {
                thread.read(cx).record().visibility == kg_domain::Visibility::Visible
            });
        if listed {
            self.select(id.clone(), window, cx);
        }
    }

    pub(super) fn run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = PluginHost::try_global(cx) else {
            return;
        };
        self.activity.plugin_command(&action.plugin().0);
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

fn plugin_ids(strip: &kg_plugins::PluginStrip) -> Vec<PluginId> {
    strip.items().map(|item| item.id.clone()).collect()
}
