use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::component::input::InputState;
use gpui_kit::{App, Context, Entity, Subscription, WeakEntity, Window};
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpError, OpResult, PluginId, PluginOp};
use kg_plugins::{PluginHost, Setting};
use serde_json::{Value, json};

use super::row::{Group, Row, group, row, switch};
use crate::kagan::Kagan;

mod controls;
mod text;

use controls::{folder_control, project_control, secret_control, secret_field, select_control};
#[cfg(test)]
pub(crate) use text::text_for_test;
use text::{FieldInput, FieldKind, TextField, string_control, sync_one, text_field};

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

pub(super) enum Change {
    Set(Value),
    Clear,
}

// A secret's field starts empty and empties after each save: it never holds a saved value.
pub(super) struct Secret {
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
        let enabled = host.status().into_iter().filter(|status| status.enabled);
        enabled
            .flat_map(|status| {
                let id = status.id.clone();
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
    let scopes = live_scopes(cx);
    sync_text(drafts, &host, &scopes, window, cx);
    drafts.fault.retain(|scope, fault| {
        let Some((life, value)) = scopes.get(scope) else {
            return false;
        };
        fault.life == *life && &fault.saved == value
    });
}

fn text_rows(host: &PluginHost) -> Vec<(PluginId, String, FieldKind, String)> {
    host.status()
        .into_iter()
        .filter(|status| status.enabled)
        .flat_map(|status| {
            let id = status.id.clone();
            host.settings(&id)
                .into_iter()
                .filter_map(|(key, setting, value)| match setting {
                    Setting::String { multiline, .. } => Some((
                        id.clone(),
                        key,
                        if multiline {
                            FieldKind::Multiline
                        } else {
                            FieldKind::Text
                        },
                        value.as_str().unwrap_or_default().to_owned(),
                    )),
                    Setting::Number { .. } => Some((
                        id.clone(),
                        key,
                        FieldKind::Number,
                        value
                            .as_number()
                            .map(ToString::to_string)
                            .unwrap_or_default(),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn sync_text(
    drafts: &mut Drafts,
    host: &Entity<PluginHost>,
    scopes: &BTreeMap<String, (Option<u64>, Value)>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) {
    let rows = {
        let host = host.read(cx);
        text_rows(host)
    };
    let weak = cx.entity().downgrade();
    let mut present = BTreeSet::new();
    for (id, key, kind, text) in rows {
        let scope = format!("{}/{key}", id.0);
        present.insert(scope.clone());
        let life = scopes.get(&scope).and_then(|(life, _)| *life);
        if let Some(kept) = drafts.text.get_mut(&scope) {
            let (field, replaced) = (kept.field.clone(), kept.life != life);
            kept.life = life;
            if replaced {
                forget_queued(drafts, &scope);
                let stale = match &field.read(cx).input {
                    FieldInput::Area(_) => kind != FieldKind::Multiline,
                    FieldInput::Line(_) => kind == FieldKind::Multiline,
                };
                if stale {
                    let fresh = text_field(&weak, &id, &key, kind, text, window, cx);
                    let watch = cx.observe(&fresh, |_, _, cx| cx.notify());
                    drafts.text.insert(
                        scope,
                        Kept {
                            field: fresh,
                            _watch: watch,
                            life,
                        },
                    );
                    continue;
                }
                field.update(cx, |field, cx| {
                    field.kind = kind;
                    field.drop_pending(cx);
                });
            }
            sync_one(&field, &text, window, cx);
        } else {
            let field = text_field(&weak, &id, &key, kind, text, window, cx);
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
}

pub(super) fn groups(kagan: &Kagan, id: &PluginId, cx: &mut Context<Kagan>) -> Vec<Group> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    let settings = host.read(cx).settings(id);
    let name = host
        .read(cx)
        .status()
        .into_iter()
        .find(|status| &status.id == id)
        .map_or_else(|| id.0.clone(), |status| status.name.clone());
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
        Setting::String { title, .. } | Setting::Number { title, .. } => {
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
            (
                title,
                fault,
                folder_control(weak, &scope, id, key, shown, cx),
            )
        }
        Setting::Project { title } => {
            let (hint, control) = project_control(weak, id, key, &scope, value, cx);
            let note = if fault.is_empty() { hint } else { fault };
            (title, note, control)
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
    let mut entry = row(&scope, title, &note, control);
    entry.wide = matches!(
        setting,
        Setting::String {
            multiline: true,
            ..
        }
    );
    Some(entry)
}

pub(super) fn save_value(
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

pub(super) fn forget_queued(drafts: &mut Drafts, scope: &str) {
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
        .filter(|status| status.enabled)
        .flat_map(|status| {
            let id = status.id.clone();
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
