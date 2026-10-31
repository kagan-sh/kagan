//! The Keyboard section: the built-in shortcuts by where they apply, then each enabled plugin's
//! commands under its name. A row shows its key, or Not set, and binds through `kg keymap`.

use crate::theme::TypeScale as _;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::{
    AnyElement, App, Context, ElementId, Hsla, InteractiveElement as _, IntoElement, Keystroke,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div,
};
use kg_core::Workspace;
use kg_domain::{KeymapAction, KeymapOp, Op, OpResult, PluginId, SHORTCUTS};
use kg_plugins::{CommandContribution, PluginHost, RunCommand};

use super::Kagan;
use super::pages::{Group, Row, group, row};
use crate::plugins::{Refused, Why, refused};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[derive(Default)]
pub(in crate::kagan) struct Recorder {
    recording: Option<(String, Subscription)>,
    fault: Option<(String, String)>,
}

struct Shortcut {
    target: KeymapAction,
    id: String,
    title: String,
    keys: Vec<Keystroke>,
}

type Labels = BTreeMap<String, String>;

pub(super) fn groups(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> Vec<Group> {
    let refused = refused(cx);
    let labels = labels(cx);
    let mut built_in: [Vec<Shortcut>; 4] = std::array::from_fn(|_| Vec::new());
    let mut seen = BTreeSet::new();
    for binding in cx.key_bindings().borrow().bindings() {
        let action = binding.action();
        if action.as_any().is::<RunCommand>() || binding.keystrokes().is_empty() {
            continue;
        }
        let name = action.name();
        let Some(shortcut) = SHORTCUTS.iter().find(|shortcut| shortcut.action == name) else {
            continue;
        };
        let short = name.rsplit("::").next().unwrap_or(name).to_owned();
        if !seen.insert(short.clone()) {
            continue;
        }
        let section = if name.starts_with("review::") {
            3
        } else {
            match short.as_str() {
                "UseProjectFolder"
                | "UseWorktreeFolder"
                | "RetryBranches"
                | "RetryPrograms"
                | "ToggleCustomCommand" => 2,
                "StopThread" | "ArchiveThread" | "TogglePinned" | "StartAgain"
                | "RestoreThread" | "ThreadActions" => 1,
                _ => 0,
            }
        };
        built_in[section].push(Shortcut {
            target: KeymapAction::BuiltIn(shortcut),
            id: short,
            title: shortcut.label.to_owned(),
            keys: binding
                .keystrokes()
                .iter()
                .map(|key| key.inner().clone())
                .collect(),
        });
    }
    let mut out: Vec<Group> = ["Window", "Thread", "New thread", "Review"]
        .into_iter()
        .zip(built_in)
        .map(|(title, shortcuts)| {
            let key = format!("shortcuts-{}", title.to_lowercase().replace(' ', "-"));
            let rows = shortcuts
                .iter()
                .map(|shortcut| shortcut_row(kagan, shortcut, &refused, &labels, window, cx))
                .collect();
            headed(group(key, title, rows))
        })
        .collect();
    for (plugin, name, commands) in plugin_commands(cx) {
        let rows = commands
            .into_iter()
            .map(|command| {
                let action = command.action();
                let keys = command
                    .key()
                    .filter(|_| !refused.iter().any(|refused| refused.action == action))
                    .map(parse)
                    .unwrap_or_default();
                let shortcut = Shortcut {
                    target: KeymapAction::Plugin {
                        plugin: plugin.clone(),
                        command: command.id().to_owned(),
                    },
                    id: format!("plugin-{}-{}", plugin.0, command.id()),
                    title: command.title().to_owned(),
                    keys,
                };
                shortcut_row(kagan, &shortcut, &refused, &labels, window, cx)
            })
            .collect();
        out.push(headed(group(
            format!("shortcuts-plugin-{}", plugin.0),
            name,
            rows,
        )));
    }
    out
}

/// How many groups the Keyboard section has, so the page can scroll past it.
pub(super) fn count(cx: &App) -> usize {
    4 + plugin_commands(cx).len()
}

fn headed(mut group: Group) -> Group {
    group.headed = true;
    group
}

pub(in crate::kagan) fn plugin_commands(
    cx: &App,
) -> Vec<(PluginId, String, Vec<CommandContribution>)> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    let host = host.read(cx);
    let commands = host.commands();
    host.status()
        .iter()
        .filter(|status| status.is_enabled())
        .filter_map(|status| {
            let own: Vec<_> = commands
                .iter()
                .filter(|command| command.plugin() == status.id())
                .cloned()
                .collect();
            (!own.is_empty()).then(|| (status.id().clone(), status.name().to_owned(), own))
        })
        .collect()
}

fn labels(cx: &App) -> Labels {
    let built_in = SHORTCUTS
        .iter()
        .map(|shortcut| (shortcut.name.to_owned(), shortcut.label.to_owned()));
    let plugins = plugin_commands(cx)
        .into_iter()
        .flat_map(|(_, name, commands)| {
            commands
                .into_iter()
                .map(move |command| (command.action(), format!("{} in {name}", command.title())))
        });
    built_in.chain(plugins).collect()
}

fn label(labels: &Labels, action: &str) -> String {
    labels
        .get(action)
        .cloned()
        .unwrap_or_else(|| action.to_owned())
}

fn shortcut_row(
    kagan: &Kagan,
    shortcut: &Shortcut,
    refused: &[Refused],
    labels: &Labels,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Row {
    let action = shortcut.target.to_string();
    let mut notes: Vec<String> = refused
        .iter()
        .filter(|refused| refused.action == action)
        .map(|refused| match &refused.why {
            Why::Taken(by) => taken(&refused.key, &label(labels, by)),
            Why::Unreadable(error) => {
                format!("{} is not a key kagan can read: {error}.", refused.key)
            }
        })
        .collect();
    notes.extend(
        refused
            .iter()
            .filter(|refused| matches!(&refused.why, Why::Taken(by) if *by == action))
            .map(|refused| format!("{} also wants this key.", label(labels, &refused.action))),
    );
    if let Some((_, fault)) = kagan
        .keyboard
        .fault
        .as_ref()
        .filter(|(fault, _)| *fault == action)
    {
        notes.push(fault.clone());
    }
    let recording = kagan
        .keyboard
        .recording
        .as_ref()
        .is_some_and(|(recording, _)| *recording == action);
    let control = controls(shortcut, &action, recording, window, cx);
    row(&shortcut.id, &shortcut.title, &notes.join(" "), control)
}

// One button, Bind, Change or Cancel, keeps its id, so keyboard focus stays on it while it waits.
fn controls(
    shortcut: &Shortcut,
    action: &str,
    recording: bool,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let id = &shortcut.id;
    let muted = cx.theme().muted_foreground;
    let line = h_flex().gap_1().items_center();
    let line = if recording {
        line.child(status(id, "Press a key", "Press a key…", muted))
    } else if shortcut.keys.is_empty() {
        line.child(status(id, "Not set", "Not set", muted))
    } else {
        line.child(keycaps(id, &shortcut.keys))
    };
    let label = if recording {
        "Cancel"
    } else if shortcut.keys.is_empty() {
        "Bind"
    } else {
        "Change"
    };
    let record = action.to_owned();
    let line = line.child(button(
        id,
        "key",
        label,
        window,
        cx,
        move |this, window, cx| {
            if recording {
                this.keyboard.recording = None;
                cx.notify();
            } else {
                this.record_shortcut(record.clone(), window, cx);
            }
        },
    ));
    let removable = matches!(shortcut.target, KeymapAction::Plugin { .. });
    let line = if removable && !recording && !shortcut.keys.is_empty() {
        let action = action.to_owned();
        line.child(button(
            id,
            "remove",
            "Remove",
            window,
            cx,
            move |this, _, cx| {
                this.save_shortcut(action.clone(), None, cx);
            },
        ))
    } else {
        line
    };
    line.into_any_element()
}

fn status(id: &str, label: &'static str, text: &'static str, muted: Hsla) -> impl IntoElement {
    div()
        .id(ElementId::Name(format!("shortcut-{id}").into()))
        .ui_container()
        .aria_label(label)
        .type_small()
        .text_color(muted)
        .child(text)
}

fn button(
    id: &str,
    name: &str,
    label: &str,
    window: &mut Window,
    cx: &mut Context<Kagan>,
    act: impl Fn(&mut Kagan, &mut Window, &mut Context<Kagan>) + 'static,
) -> AnyElement {
    let control_id = format!("shortcut-{id}-{name}");
    // Holds the button's own focus, so the page reports where the keyboard is.
    let scope = window
        .use_keyed_state(
            ElementId::Name(format!("{control_id}-focus").into()),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone();
    div()
        .id(ElementId::Name(control_id.into()))
        .track_focus(&scope)
        .ui_observe_as(Facts {
            focus: Some(scope),
            ..Facts::button(label)
        })
        .child(
            Button::new(ElementId::Name(
                format!("button-shortcut-{id}-{name}").into(),
            ))
            .xsmall()
            .ghost()
            .label(label.to_owned())
            .on_click(cx.listener(move |this, _, window, cx| act(this, window, cx))),
        )
        .into_any_element()
}

fn keycaps(id: &str, strokes: &[Keystroke]) -> AnyElement {
    div()
        .id(ElementId::Name(format!("shortcut-{id}").into()))
        .ui_container()
        .aria_label(SharedString::from(keys_text(strokes)))
        .child(
            h_flex()
                .gap_1()
                .items_center()
                .children(strokes.iter().cloned().map(Kbd::new)),
        )
        .into_any_element()
}

fn parse(key: &str) -> Vec<Keystroke> {
    key.split_whitespace()
        .filter_map(|key| kg_core::parse_key(key).ok())
        .collect()
}

const PROGRAM_KEY: &str = if cfg!(target_os = "macos") {
    "That key belongs to the program. Use ⌘, ⌥, ⌃⇧ or an F key."
} else {
    "That key belongs to the program. Use Ctrl+Shift, Alt or an F key."
};

// Plain keys and bare Ctrl keys reach the program in the terminal, so a shortcut never takes them.
fn for_window(key: &Keystroke) -> bool {
    let modifiers = &key.modifiers;
    let function = key.key.strip_prefix('f').is_some_and(|number| {
        !number.starts_with('0') && matches!(number.parse::<u8>(), Ok(1..=35))
    });
    function || modifiers.platform || modifiers.alt || (modifiers.control && modifiers.shift)
}

fn taken(key: &str, by: &str) -> String {
    format!("{} is already used by {by}.", keys_text(&parse(key)))
}

fn keys_text(strokes: &[Keystroke]) -> String {
    strokes
        .iter()
        .map(Kbd::format)
        .collect::<Vec<_>>()
        .join(" ")
}

impl Kagan {
    // Escape stops without saving. Only this window's keys count: About has its own.
    fn record_shortcut(&mut self, action: String, window: &mut Window, cx: &mut Context<Self>) {
        let kagan = cx.entity().downgrade();
        let here = window.window_handle();
        let intercept = cx.intercept_keystrokes(move |event, window, cx| {
            let key = &event.keystroke;
            if window.window_handle() != here
                || matches!(
                    key.key.as_str(),
                    "shift" | "control" | "alt" | "platform" | "function"
                )
            {
                return;
            }
            let key = key.clone();
            if kagan
                .update(cx, |this, cx| this.recorded_shortcut(&key, cx))
                .unwrap_or(false)
            {
                cx.stop_propagation();
            }
        });
        self.keyboard = Recorder {
            recording: Some((action, intercept)),
            fault: None,
        };
        cx.notify();
    }

    // A key pressed after Settings closed some other way still reaches the window.
    fn recorded_shortcut(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some((action, _)) = self.keyboard.recording.take() else {
            return false;
        };
        cx.notify();
        if !self.in_settings() {
            return false;
        }
        let text = key.unparse(kg_core::host_os());
        if text != "escape" {
            if for_window(key) {
                self.save_shortcut(action, Some(text), cx);
            } else {
                self.keyboard.fault = Some((action, PROGRAM_KEY.to_owned()));
            }
        }
        true
    }

    fn save_shortcut(&mut self, action: String, key: Option<String>, cx: &mut Context<Self>) {
        let op = match key.clone() {
            Some(key) => KeymapOp::Set {
                action: action.clone(),
                key,
                reset: false,
            },
            None => KeymapOp::Remove {
                action: action.clone(),
            },
        };
        self.keyboard.fault = None;
        let saved = Workspace::dispatch(Op::Keymap(op), cx);
        cx.spawn(async move |this, cx| {
            let result = saved.await;
            this.update(cx, |this, cx| {
                let message = match result {
                    Ok(OpResult::Setting(value)) if value["ok"] == false => taken(
                        key.as_deref().unwrap_or_default(),
                        &label(&labels(cx), value["conflict"].as_str().unwrap_or_default()),
                    ),
                    Ok(_) => return,
                    Err(error) => error.to_string(),
                };
                this.keyboard.fault = Some((action, message));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn stop_recording(&mut self) {
        self.keyboard = Recorder::default();
    }
}
