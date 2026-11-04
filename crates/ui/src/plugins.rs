use gpui_kit::{App, AppContext as _, Context, Entity, Global, KeyBinding, Subscription};
use kg_domain::PluginId;
use kg_plugins::keys::{conflict, sequence, window_keys};
use kg_plugins::{CommandWhen, PluginHost, RunCommand};

use crate::kagan::CONTEXT;

struct Bindings {
    host: Entity<PluginHost>,
    keys: Vec<(PluginId, String, String, CommandWhen)>,
    refused: Vec<Refused>,
    _subscription: Subscription,
}

/// A plugin command whose key is not bound. `action` is a `kg keymap` name.
#[derive(Clone)]
pub(crate) struct Refused {
    pub(crate) action: String,
    pub(crate) key: String,
    pub(crate) why: Why,
}

#[derive(Clone)]
pub(crate) enum Why {
    /// Another command, named as `kg keymap` names it, took the key first.
    Taken(String),
    /// kagan cannot read the key; the text says why.
    Unreadable(String),
}

struct PluginBindings {
    bindings: Entity<Bindings>,
}
impl Global for PluginBindings {}

pub(crate) fn init(cx: &mut App) {
    crate::theme::follow_plugins(cx);
    if cx.try_global::<PluginBindings>().is_some() {
        return;
    }
    let Some(host) = PluginHost::try_global(cx) else {
        return;
    };
    host.update(cx, |host, _| {
        host.set_drop_pages(std::rc::Rc::new(|plugin, stores, cx| {
            crate::components::drop_stores(stores, cx);
            crate::kagan::drop_web_panel(plugin, cx);
        }));
    });
    let bindings = cx.new(|cx| {
        let subscription = cx.observe(&host, |this: &mut Bindings, host, cx| {
            this.refresh(&host, cx);
        });
        let mut this = Bindings {
            host: host.clone(),
            keys: Vec::new(),
            refused: Vec::new(),
            _subscription: subscription,
        };
        this.refresh(&host, cx);
        this
    });
    cx.set_global(PluginBindings { bindings });
}

/// Binds the plugin keys again. The window calls this when its own shortcuts change, so a plugin key
/// that a built-in or saved shortcut now takes is dropped, and one it freed is bound.
pub(crate) fn rebind(cx: &mut App) {
    let Some(bindings) = cx
        .try_global::<PluginBindings>()
        .map(|global| global.bindings.clone())
    else {
        return;
    };
    bindings.update(cx, Bindings::rebind);
}

pub(crate) fn refused(cx: &App) -> Vec<Refused> {
    cx.try_global::<PluginBindings>()
        .map(|global| global.bindings.read(cx).refused.clone())
        .unwrap_or_default()
}

impl Bindings {
    fn refresh(&mut self, host: &Entity<PluginHost>, cx: &mut Context<Self>) {
        let mut commands = host.read(cx).commands();
        // A key the person saved wins over any manifest's, whatever the plugin order.
        commands.sort_by_key(kg_plugins::CommandContribution::source);
        let keys = commands
            .into_iter()
            .filter_map(|command| {
                command.key().map(|key| {
                    (
                        command.plugin().clone(),
                        command.id().to_owned(),
                        key.to_owned(),
                        command.when(),
                    )
                })
            })
            .collect::<Vec<_>>();
        if keys == self.keys {
            return;
        }
        self.keys = keys;
        self.rebind(cx);
    }

    /// Binds the window's own keys, then each plugin key the window does not already take. The
    /// window's keys win: a plugin key that overlaps one is not bound, and the plugin's log says why.
    /// Plugin keys that collide keep only the first: saved keys, then manifest keys, each in plugin
    /// and command order.
    fn rebind(&mut self, cx: &mut Context<Self>) {
        let mut taken = window_keys(cx);
        let mut bindings: Vec<KeyBinding> = cx
            .key_bindings()
            .borrow()
            .bindings()
            .filter(|binding| !binding.action().as_any().is::<RunCommand>())
            .cloned()
            .collect();
        let mut refused = Vec::new();
        for (plugin, command, key, when) in &self.keys {
            let action = format!("{plugin}:{command}");
            let refuse = |why| {
                (
                    plugin.clone(),
                    command.clone(),
                    Refused {
                        action: action.clone(),
                        key: key.clone(),
                        why,
                    },
                )
            };
            let binding = match crate::kagan::bind(
                key,
                RunCommand::new(plugin.clone(), command.clone()),
                binding_context(*when),
                cx,
            ) {
                Ok(binding) => binding,
                Err(error) => {
                    refused.push(refuse(Why::Unreadable(error)));
                    continue;
                }
            };
            let keys = sequence(&binding);
            if let Some(by) = conflict(&taken, &keys) {
                refused.push(refuse(Why::Taken(by)));
            } else {
                taken.push((keys, action));
                bindings.push(binding);
            }
        }
        cx.clear_key_bindings();
        cx.bind_keys(bindings);
        for (plugin, command, refused) in &refused {
            let message = match &refused.why {
                Why::Taken(by) => format!(
                    "command {command} is not bound to {}: {by} takes that key",
                    refused.key
                ),
                Why::Unreadable(error) => format!(
                    "command {command} is not bound to {}: kagan cannot read that key: {error}",
                    refused.key
                ),
            };
            self.host
                .update(cx, |host, _| host.report(plugin, &message));
        }
        self.refused = refused.into_iter().map(|(_, _, refused)| refused).collect();
    }
}

fn binding_context(when: CommandWhen) -> &'static str {
    match when {
        CommandWhen::Always => CONTEXT,
        CommandWhen::Thread => "Kagan && thread == true",
        CommandWhen::Project => "Kagan && project == true",
    }
}
