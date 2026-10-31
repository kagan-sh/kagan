//! A plugin's settings page: its declared settings as rows, then its own view when it has one.
//! A save reply is applied only when that save is still the latest and the plugin activation is still the one captured when the save started.

use crate::theme::TypeScale as _;

use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, PathPromptOptions, SharedString, Styled as _, Subscription,
    WeakEntity, Window, div, rems,
};
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpError, OpResult, PluginId, PluginOp};
use kg_plugins::{PluginHost, Setting};
use serde_json::{Value, json};

use super::Kagan;
use super::pages::{Group, Row, group, row, switch};
use crate::kagan::shown::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::Role;

#[derive(Default)]
pub(crate) struct Drafts {
    text: BTreeMap<String, Kept>,
    secret: BTreeMap<String, Secret>,
    fault: BTreeMap<String, Fault>,
    lane: BTreeMap<String, Lane>,
}

struct Fault {
    message: String,
    life: Option<u64>,
    saved: Value,
}

struct Lane {
    next: u64,
    plugin: PluginId,
    key: String,
    flight: Option<Save>,
    queued: Option<Save>,
}

struct Save {
    request: u64,
    life: Option<u64>,
    baseline: Option<String>,
    change: Change,
}

enum Change {
    Set(Value),
    Clear,
}

/// A secret's field starts empty and empties after each save: it never holds a saved value.
struct Secret {
    input: Entity<InputState>,
    _enter: Subscription,
}

struct Kept {
    field: Entity<TextField>,
    _watch: Subscription,
    life: Option<u64>,
}

pub(super) fn sync(drafts: &mut Drafts, window: &mut Window, cx: &mut Context<Kagan>) {
    let Some(host) = PluginHost::try_global(cx) else {
        return;
    };
    let secrets: Vec<(PluginId, String)> = {
        let host = host.read(cx);
        let enabled = host
            .status()
            .into_iter()
            .filter(kg_plugins::PluginStatus::is_enabled);
        enabled
            .flat_map(|status| {
                let id = status.id().clone();
                let settings = host.settings(&id).into_iter();
                settings.filter_map(move |(key, setting, _)| {
                    matches!(setting, Setting::Secret { .. }).then(|| (id.clone(), key))
                })
            })
            .collect()
    };
    let mut old = std::mem::take(&mut drafts.secret);
    for (id, key) in secrets {
        let scope = format!("{}/{key}", id.0);
        let field = old
            .remove(&scope)
            .unwrap_or_else(|| secret_field(&scope, id, key, window, cx));
        drafts.secret.insert(scope, field);
    }
    let rows: Vec<(PluginId, String, String)> = {
        let host = host.read(cx);
        host.status()
            .into_iter()
            .filter(kg_plugins::PluginStatus::is_enabled)
            .flat_map(|status| {
                let id = status.id().clone();
                host.settings(&id)
                    .into_iter()
                    .filter_map(|(key, setting, value)| match setting {
                        Setting::String { .. } => Some((
                            id.clone(),
                            key,
                            value.as_str().unwrap_or_default().to_owned(),
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let weak = cx.entity().downgrade();
    let scopes = live_scopes(cx);
    let mut present = BTreeSet::new();
    for (id, key, text) in rows {
        let scope = format!("{}/{key}", id.0);
        present.insert(scope.clone());
        let life = scopes.get(&scope).and_then(|(life, _)| *life);
        if let Some(kept) = drafts.text.get_mut(&scope) {
            let (field, replaced) = (kept.field.clone(), kept.life != life);
            kept.life = life;
            if replaced {
                forget_queued(drafts, &scope);
                field.update(cx, TextField::drop_pending);
            }
            sync_one(&field, &text, window, cx);
        } else {
            let field = text_field(&weak, &id, &key, text, window, cx);
            let watch = cx.observe(&field, |_, _, cx| cx.notify());
            drafts.text.insert(
                scope,
                Kept {
                    field,
                    _watch: watch,
                    life,
                },
            );
        }
    }
    let mut gone = Vec::new();
    drafts.text.retain(|scope, kept| {
        if present.contains(scope) {
            return true;
        }
        kept.field.update(cx, TextField::drop_pending);
        gone.push(scope.clone());
        kept.field.read(cx).dirty(cx)
    });
    for scope in gone {
        forget_queued(drafts, &scope);
    }
    drafts.fault.retain(|scope, fault| {
        let Some((life, value)) = scopes.get(scope) else {
            return false;
        };
        fault.life == *life && &fault.saved == value
    });
}

/// A plugin's declared settings, as one group named after the plugin.
pub(super) fn plugin_groups(kagan: &Kagan, id: &PluginId, cx: &mut Context<Kagan>) -> Vec<Group> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    let settings = host.read(cx).settings(id);
    let name = host
        .read(cx)
        .status()
        .into_iter()
        .find(|status| status.id() == id)
        .map_or_else(|| id.0.clone(), |status| status.name().to_owned());
    let weak = cx.entity().downgrade();
    let rows: Vec<Row> = settings
        .into_iter()
        .filter_map(|(key, setting, value)| {
            plugin_row(kagan, &weak, id, &key, &setting, &value, cx)
        })
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    vec![group(format!("plugin-{}", id.0), name, rows)]
}

fn plugin_row(
    kagan: &Kagan,
    weak: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    setting: &Setting,
    value: &Value,
    cx: &mut Context<Kagan>,
) -> Option<Row> {
    let scope = format!("{}/{key}", id.0);
    let fault = kagan
        .settings_drafts
        .fault
        .get(&scope)
        .map(|fault| fault.message.clone())
        .unwrap_or_default();
    let (title, note, control) = match setting {
        Setting::Boolean { title, .. } => {
            let on = value.as_bool().unwrap_or(false);
            let (plugin, setting_key, scope) = (id.clone(), key.to_owned(), scope.clone());
            let weak = weak.clone();
            (
                title,
                fault,
                switch(&format!("plugin-{scope}"), title, on, move |on, _, cx| {
                    save_value(
                        &weak,
                        scope.clone(),
                        plugin.clone(),
                        setting_key.clone(),
                        Change::Set(json!(on)),
                        cx,
                    );
                }),
            )
        }
        Setting::String { title, .. } => {
            let field = kagan.settings_drafts.text.get(&scope)?.field.clone();
            let (note, control) = string_control(title, &scope, &field, cx);
            (title, note, control)
        }
        Setting::Select { title, options, .. } => (
            title,
            fault,
            select_control(weak, id, key, &scope, options, value),
        ),
        Setting::Folder { title } => {
            let shown = value.as_str().unwrap_or("Not chosen").to_owned();
            (title, fault, folder_control(weak, &scope, id, key, shown))
        }
        Setting::Secret { title } => {
            let input = kagan.settings_drafts.secret.get(&scope)?.input.clone();
            let warning =
                PluginHost::try_global(cx).and_then(|host| host.read(cx).secret_warning());
            let note = Some(fault).filter(|fault| !fault.is_empty()).or(warning);
            let control = secret_control(weak, id, key, title, &input, !value.is_null(), cx);
            (title, note.unwrap_or_default(), control)
        }
        Setting::Provider { .. } => return None,
    };
    Some(row(&scope, title, &note, control))
}

fn string_control(
    title: &str,
    scope: &str,
    field: &Entity<TextField>,
    cx: &mut Context<Kagan>,
) -> (String, AnyElement) {
    let input = field.read(cx).input.clone();
    #[cfg(test)]
    TEXT_INPUTS.with(|inputs| {
        inputs
            .borrow_mut()
            .insert(scope.to_owned(), input.downgrade());
    });
    let (draft, committed, pending, error) = {
        let field = field.read(cx);
        let draft = field.input.read(cx).value().to_string();
        let pending = field.sent.as_ref().is_some_and(|sent| sent == &draft);
        (draft, field.committed.clone(), pending, field.error.clone())
    };
    let note = if let Some(error) = error.as_ref() {
        format!("{error} Enter saves.")
    } else if pending {
        "Saving…".to_owned()
    } else if draft != committed {
        "Enter saves.".to_owned()
    } else {
        String::new()
    };
    let show_revert = error.is_some() || draft != committed;
    let revert = show_revert.then(|| {
        let field = field.clone();
        control(
            format!("setting-revert-{scope}"),
            Facts::button("Revert"),
            Button::new(ElementId::Name(
                format!("button-setting-revert-{scope}").into(),
            ))
            .small()
            .outline()
            .label("Revert")
            .on_click(move |_, window, cx| revert(&field, window, cx)),
        )
    });
    let control = h_flex()
        .gap_2()
        .items_center()
        .children(revert)
        .child(
            div()
                .id(ElementId::Name(format!("plugin-text-{scope}").into()))
                .ui_observe_as(Facts::input(title, &input, cx))
                .w(rems(13.75))
                .h(rems(1.75))
                .child(
                    Input::new(&input)
                        .text_size(gpui_kit::px(crate::theme::TYPE_BODY))
                        .small(),
                ),
        )
        .into_any_element();
    (note, control)
}

fn secret_field(
    scope: &str,
    plugin: PluginId,
    key: String,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Secret {
    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .masked(true)
            .placeholder("Paste a new value")
    });
    let scope = scope.to_owned();
    let enter = cx.subscribe_in(&input, window, move |kagan, input, event, window, cx| {
        if let InputEvent::PressEnter { .. } = event
            && let Some(text) = take_secret(input, window, cx)
        {
            let (scope, plugin, key) = (scope.clone(), plugin.clone(), key.clone());
            kagan
                .settings_drafts
                .begin(scope, plugin, key, text, None, cx);
        }
    });
    Secret {
        input,
        _enter: enter,
    }
}

fn take_secret(input: &Entity<InputState>, window: &mut Window, cx: &mut App) -> Option<Change> {
    let text = input.read(cx).value().to_string();
    input.update(cx, |state, cx| state.set_value("", window, cx));
    (!text.is_empty()).then(|| Change::Set(json!(text)))
}

fn secret_control(
    weak: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    title: &str,
    input: &Entity<InputState>,
    set: bool,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let scope = &format!("{}/{key}", id.0);
    let label = if set { "Replace" } else { "Set" };
    let save = {
        let (weak, scope, plugin, key, input) = (
            weak.clone(),
            scope.to_owned(),
            id.clone(),
            key.to_owned(),
            input.clone(),
        );
        move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
            if let Some(text) = take_secret(&input, window, cx) {
                save_value(&weak, scope.clone(), plugin.clone(), key.clone(), text, cx);
            }
        }
    };
    let clear = set.then(|| {
        let (weak, scope, plugin, key) =
            (weak.clone(), scope.to_owned(), id.clone(), key.to_owned());
        control(
            format!("setting-clear-{scope}"),
            Facts::button("Clear"),
            Button::new(ElementId::Name(
                format!("button-setting-clear-{scope}").into(),
            ))
            .small()
            .outline()
            .label("Clear")
            .on_click(move |_, _, cx| {
                let (scope, plugin, key) = (scope.clone(), plugin.clone(), key.clone());
                save_value(&weak, scope, plugin, key, Change::Clear, cx);
            }),
        )
    });
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .type_small()
                .child(if set { "Set" } else { "Not set" }),
        )
        .child(
            div()
                .id(ElementId::Name(format!("plugin-secret-{scope}").into()))
                .ui_observe_as(Facts::input(title, input, cx))
                .w(rems(13.75))
                .h(rems(1.75))
                .child(
                    Input::new(input)
                        .text_size(gpui_kit::px(crate::theme::TYPE_BODY))
                        .small(),
                ),
        )
        .child(control(
            format!("setting-secret-{scope}"),
            Facts::button(label),
            Button::new(ElementId::Name(
                format!("button-setting-secret-{scope}").into(),
            ))
            .small()
            .outline()
            .label(label)
            .on_click(save),
        ))
        .children(clear)
        .into_any_element()
}

fn select_control(
    weak: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    scope: &str,
    options: &[String],
    value: &Value,
) -> AnyElement {
    let current = value.as_str().unwrap_or_default().to_owned();
    h_flex()
        .gap_1()
        .flex_wrap()
        .justify_end()
        .children(options.iter().map(|option| {
            let option_id = format!("option-{scope}/{option}");
            let (plugin, setting_key, pick, scope, weak) = (
                id.clone(),
                key.to_owned(),
                option.clone(),
                scope.to_owned(),
                weak.clone(),
            );
            let button = Button::new(ElementId::Name(
                format!("button-option-{scope}/{option}").into(),
            ))
            .small()
            .label(option.clone())
            .on_click(move |_, _, cx| {
                save_value(
                    &weak,
                    scope.clone(),
                    plugin.clone(),
                    setting_key.clone(),
                    Change::Set(json!(pick)),
                    cx,
                );
            });
            let button = if *option == current {
                button.primary()
            } else {
                button.outline()
            };
            control(
                option_id,
                Facts {
                    role: Role::RadioButton,
                    selected: Some(*option == current),
                    ..Facts::button(option.clone())
                },
                button,
            )
        }))
        .into_any_element()
}

struct TextField {
    input: Entity<InputState>,
    committed: String,
    sent: Option<String>,
    error: Option<String>,
    request: Option<u64>,
    plugin: PluginId,
    key: String,
    kagan: WeakEntity<Kagan>,
    _save: Subscription,
}

impl TextField {
    fn dirty(&self, cx: &App) -> bool {
        let draft = self.input.read(cx).value().to_string();
        draft != self.committed || self.sent.is_some() || self.error.is_some()
    }

    fn drop_pending(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.sent = None;
        self.request = None;
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        if value == self.committed && self.error.is_none() && self.sent.is_none() {
            return;
        }
        self.error = None;
        self.sent = Some(value.clone());
        let plugin = self.plugin.clone();
        let key = self.key.clone();
        let kagan = self.kagan.clone();
        let baseline = self.committed.clone();
        let scope = format!("{}/{key}", plugin.0);
        let request = kagan.update(cx, |kagan, cx| {
            kagan.settings_drafts.begin(
                scope,
                plugin,
                key,
                Change::Set(json!(value)),
                Some(baseline),
                cx,
            )
        });
        self.request = request.ok();
        cx.notify();
    }

    fn finish(
        &mut self,
        request: u64,
        baseline: Option<&str>,
        written: &Value,
        result: Result<OpResult, OpError>,
        cx: &mut Context<Self>,
    ) {
        if self.request != Some(request) {
            return;
        }
        let Some(sent) = written.as_str() else {
            return;
        };
        let draft = self.input.read(cx).value().to_string();
        self.sent = None;
        let baseline = baseline.unwrap_or("");
        if draft != sent {
            if result.is_ok() && self.committed == baseline {
                sent.clone_into(&mut self.committed);
            }
            cx.notify();
            return;
        }
        match result {
            Ok(_) => {
                if self.committed == baseline || self.committed == sent {
                    sent.clone_into(&mut self.committed);
                    self.error = None;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
}

fn text_field(
    kagan: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    text: String,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Entity<TextField> {
    let (plugin, setting, owner) = (id.clone(), key.to_owned(), kagan.clone());
    cx.new(move |cx| {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(text.clone()));
        let save = cx.subscribe_in(
            &input,
            window,
            move |field: &mut TextField, _, event, _, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => field.submit(cx),
                InputEvent::Focus | InputEvent::Blur => {}
            },
        );
        TextField {
            input,
            committed: text,
            sent: None,
            error: None,
            request: None,
            plugin,
            key: setting,
            kagan: owner,
            _save: save,
        }
    })
}

fn sync_one(field: &Entity<TextField>, text: &str, window: &mut Window, cx: &mut App) {
    field.update(cx, |field, cx| {
        if field.committed == text && field.sent.is_none() {
            return;
        }
        let draft = field.input.read(cx).value().to_string();
        if field.sent.as_deref() == Some(text) {
            text.clone_into(&mut field.committed);
            return;
        }
        let dirty = draft != field.committed || field.sent.is_some() || field.error.is_some();
        let changed = field.committed != text;
        if dirty && draft != text {
            text.clone_into(&mut field.committed);
            if changed {
                field.error = None;
            }
            return;
        }
        text.clone_into(&mut field.committed);
        field.error = None;
        if draft != text {
            field.input.update(cx, |state, cx| {
                state.set_value(text.to_owned(), window, cx);
            });
        }
    });
}

fn revert(field: &Entity<TextField>, window: &mut Window, cx: &mut App) {
    field.update(cx, |field, cx| {
        field.sent = None;
        field.error = None;
        field.request = None;
        let scope = format!("{}/{}", field.plugin.0, field.key);
        let kagan = field.kagan.clone();
        let _ = kagan.update(cx, |kagan, _| {
            forget_queued(&mut kagan.settings_drafts, &scope);
        });
        let committed = field.committed.clone();
        field.input.update(cx, |state, cx| {
            state.set_value(committed, window, cx);
        });
        cx.notify();
    });
}

fn folder_control(
    weak: &WeakEntity<Kagan>,
    scope: &str,
    id: &PluginId,
    key: &str,
    shown: String,
) -> AnyElement {
    let (plugin, setting, scope) = (id.clone(), key.to_owned(), scope.to_owned());
    let weak = weak.clone();
    h_flex()
        .gap_2()
        .child(div().type_small().child(SharedString::from(shown)))
        .child(control(
            format!("choose-{}/{key}", id.0),
            Facts::button("Choose…"),
            Button::new(ElementId::Name(
                format!("button-choose-{}/{key}", id.0).into(),
            ))
            .small()
            .outline()
            .label("Choose…")
            .on_click(move |_, _, cx| {
                choose_folder(
                    weak.clone(),
                    scope.clone(),
                    plugin.clone(),
                    setting.clone(),
                    cx,
                );
            }),
        ))
        .into_any_element()
}

fn choose_folder(weak: WeakEntity<Kagan>, scope: String, id: PluginId, key: String, cx: &mut App) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Choose".into()),
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = picked.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        cx.update(|cx| {
            save_value(
                &weak,
                scope,
                id,
                key,
                Change::Set(json!(path.to_string_lossy().into_owned())),
                cx,
            );
        });
    })
    .detach();
}

fn save_value(
    kagan: &WeakEntity<Kagan>,
    scope: String,
    plugin: PluginId,
    key: String,
    change: Change,
    cx: &mut App,
) {
    let _ = kagan.update(cx, |kagan, cx| {
        kagan
            .settings_drafts
            .begin(scope, plugin, key, change, None, cx);
    });
}

impl Drafts {
    fn begin(
        &mut self,
        scope: String,
        plugin: PluginId,
        key: String,
        change: Change,
        baseline: Option<String>,
        cx: &mut Context<Kagan>,
    ) -> u64 {
        let life = live_setting(&plugin, &key, cx).and_then(|(life, _)| life);
        let lane = self.lane.entry(scope.clone()).or_insert_with(|| Lane {
            next: 0,
            plugin: plugin.clone(),
            key: key.clone(),
            flight: None,
            queued: None,
        });
        lane.plugin = plugin;
        lane.key = key;
        lane.next += 1;
        let save = Save {
            request: lane.next,
            life,
            baseline,
            change,
        };
        let request = save.request;
        if lane.flight.is_some() {
            lane.queued = Some(Save {
                baseline: None,
                ..save
            });
            cx.notify();
            return request;
        }
        self.launch(scope, save, cx);
        request
    }

    fn launch(&mut self, scope: String, mut save: Save, cx: &mut Context<Kagan>) {
        save.baseline = save.baseline.or_else(|| {
            self.text
                .get(&scope)
                .map(|kept| kept.field.read(cx).committed.clone())
        });
        let Some(lane) = self.lane.get_mut(&scope) else {
            return;
        };
        let request = save.request;
        let key = lane.key.clone();
        let op = match &save.change {
            Change::Set(value) => ConfigOp::Set {
                key,
                value: Some(value.clone()),
            },
            Change::Clear => ConfigOp::Clear { key },
        };
        let op = Op::Plugin(PluginOp::Config {
            id: lane.plugin.clone(),
            op,
            thread_id: None,
        });
        lane.flight = Some(save);
        let task = Workspace::dispatch(op, cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |kagan, cx| {
                kagan.settings_drafts.land(scope, request, result, cx);
            });
        })
        .detach();
    }

    fn land(
        &mut self,
        scope: String,
        request: u64,
        result: Result<OpResult, OpError>,
        cx: &mut Context<Kagan>,
    ) {
        let Some(lane) = self.lane.get_mut(&scope) else {
            return;
        };
        let Some(flight) = lane.flight.take_if(|flight| flight.request == request) else {
            return;
        };
        let queued = lane.queued.take();
        let (plugin, key) = (lane.plugin.clone(), lane.key.clone());
        let now = live_setting(&plugin, &key, cx);
        if same_life(flight.life, now.as_ref()) && queued.is_none() {
            self.apply(
                &scope,
                request,
                flight.baseline.as_deref(),
                match &flight.change {
                    Change::Set(value) => value,
                    Change::Clear => &Value::Null,
                },
                result,
                cx,
            );
        } else if same_life(flight.life, now.as_ref()) {
            if result.is_ok() {
                self.fault.remove(&scope);
            }
        } else {
            self.abandon(&scope, request, cx);
        }
        if let Some(queued) = queued
            && same_life(queued.life, live_setting(&plugin, &key, cx).as_ref())
        {
            self.launch(scope, queued, cx);
        }
        cx.notify();
    }

    fn apply(
        &mut self,
        scope: &str,
        request: u64,
        baseline: Option<&str>,
        value: &Value,
        result: Result<OpResult, OpError>,
        cx: &mut Context<Kagan>,
    ) {
        if let Some(field) = self.text.get(scope).map(|kept| kept.field.clone()) {
            let saved = result.is_ok();
            field.update(cx, |field, cx| {
                field.finish(request, baseline, value, result, cx);
            });
            if saved {
                self.fault.remove(scope);
            }
            return;
        }
        match result {
            Ok(_) => {
                self.fault.remove(scope);
            }
            Err(error) => self.keep_fault(scope, error.to_string(), cx),
        }
    }

    fn keep_fault(&mut self, scope: &str, message: String, cx: &App) {
        let Some(lane) = self.lane.get(scope) else {
            return;
        };
        let plugin = lane.plugin.clone();
        let key = lane.key.clone();
        let Some((life, saved)) = live_setting(&plugin, &key, cx) else {
            return;
        };
        self.fault.insert(
            scope.to_owned(),
            Fault {
                message,
                life,
                saved,
            },
        );
    }

    fn abandon(&mut self, scope: &str, request: u64, cx: &mut Context<Kagan>) {
        self.fault.remove(scope);
        let Some(field) = self.text.get(scope).map(|kept| kept.field.clone()) else {
            return;
        };
        field.update(cx, |field, cx| {
            if field.request == Some(request) {
                field.drop_pending(cx);
            }
        });
    }
}

fn forget_queued(drafts: &mut Drafts, scope: &str) {
    if let Some(lane) = drafts.lane.get_mut(scope) {
        lane.queued = None;
    }
}

fn live_scopes(cx: &App) -> BTreeMap<String, (Option<u64>, Value)> {
    let Some(host) = PluginHost::try_global(cx) else {
        return BTreeMap::new();
    };
    let host = host.read(cx);
    host.status()
        .into_iter()
        .filter(kg_plugins::PluginStatus::is_enabled)
        .flat_map(|status| {
            let id = status.id().clone();
            let life = host.activation(&id);
            host.settings(&id)
                .into_iter()
                .map(move |(key, _, value)| (format!("{}/{key}", id.0), (life, value)))
        })
        .collect()
}

fn live_setting(id: &PluginId, key: &str, cx: &App) -> Option<(Option<u64>, Value)> {
    let host = PluginHost::try_global(cx)?;
    let host = host.read(cx);
    let (_, _, value) = host
        .settings(id)
        .into_iter()
        .find(|(name, _, _)| name == key)?;
    Some((host.activation(id), value))
}

fn same_life(started: Option<u64>, live: Option<&(Option<u64>, Value)>) -> bool {
    matches!(
        (started, live),
        (Some(before), Some((Some(now), _))) if before == *now
    )
}

#[cfg(test)]
thread_local! {
    /// The input of each text setting as the last render drew it, for the tests. Weak, so the
    /// test does not keep the input alive.
    static TEXT_INPUTS: std::cell::RefCell<BTreeMap<String, gpui_kit::WeakEntity<InputState>>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// The text a plugin's text setting shows, as the last render drew it. Used by the tests.
#[cfg(test)]
pub(super) fn text_for_test(plugin: &PluginId, key: &str, cx: &App) -> String {
    let input =
        TEXT_INPUTS.with(|inputs| inputs.borrow().get(&format!("{}/{key}", plugin.0)).cloned());
    input
        .and_then(|input| input.upgrade())
        .map_or_else(String::new, |input| input.read(cx).value().to_string())
}
