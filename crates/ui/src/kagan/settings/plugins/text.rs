#[cfg(test)]
use std::collections::BTreeMap;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Subscription,
    WeakEntity, Window, div, rems,
};
use kg_domain::{OpError, OpResult, PluginId};
use serde_json::{Value, json};

use super::Change;
use super::forget_queued;
use crate::kagan::Kagan;
use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::Role;

pub(super) fn string_control(
    title: &str,
    scope: &str,
    field: &Entity<TextField>,
    cx: &mut Context<Kagan>,
) -> (String, AnyElement) {
    let (draft, committed, pending, error, kind, input) = {
        let field = field.read(cx);
        let draft = field.input.value(cx);
        let pending = field.sent.as_ref().is_some_and(|sent| sent == &draft);
        (
            draft,
            field.committed.clone(),
            pending,
            field.error.clone(),
            field.kind,
            field.input.clone(),
        )
    };
    #[cfg(test)]
    input.record_for_test(scope, cx);
    let note = if let Some(error) = error.as_ref() {
        match kind {
            FieldKind::Text => format!("{error} Enter saves."),
            FieldKind::Multiline | FieldKind::Number => error.clone(),
        }
    } else if pending {
        "Saving…".to_owned()
    } else if draft != committed {
        match kind {
            FieldKind::Multiline => "Unsaved changes.".to_owned(),
            FieldKind::Text | FieldKind::Number => "Enter saves.".to_owned(),
        }
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
    let element = div()
        .id(ElementId::Name(format!("plugin-text-{scope}").into()))
        .ui_observe_as(input.facts(title, cx));
    let element = match &input {
        FieldInput::Line(state) => element
            .w(rems(13.75))
            .h(rems(1.75))
            .child(
                Input::new(state)
                    .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                    .small(),
            )
            .into_any_element(),
        FieldInput::Area(state) => element
            .w_full()
            .child(
                Textarea::new(state)
                    .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                    .small(),
            )
            .into_any_element(),
    };
    let control = if kind == FieldKind::Multiline {
        let save = {
            let field = field.clone();
            control(
                format!("setting-save-{scope}"),
                Facts::button("Save"),
                Button::new(ElementId::Name(
                    format!("button-setting-save-{scope}").into(),
                ))
                .small()
                .outline()
                .label("Save")
                .on_click(move |_, window, cx| {
                    field.update(cx, |field, cx| field.submit(window, cx));
                }),
            )
        };
        v_flex()
            .gap_2()
            .child(element)
            .child(h_flex().gap_2().children(revert).child(save))
            .into_any_element()
    } else {
        h_flex()
            .gap_2()
            .items_center()
            .children(revert)
            .child(element)
            .into_any_element()
    };
    (note, control)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FieldKind {
    Text,
    Multiline,
    Number,
}

#[derive(Clone)]
pub(super) enum FieldInput {
    Line(Entity<InputState>),
    Area(Entity<TextareaState>),
}

impl FieldInput {
    fn value(&self, cx: &App) -> String {
        match self {
            Self::Line(state) => state.read(cx).value().to_string(),
            Self::Area(state) => state.read(cx).value().to_string(),
        }
    }

    fn set_value(&self, text: String, window: &mut Window, cx: &mut App) {
        match self {
            Self::Line(state) => state.update(cx, |state, cx| state.set_value(text, window, cx)),
            Self::Area(state) => state.update(cx, |state, cx| state.set_value(text, window, cx)),
        }
    }

    fn facts(&self, title: &str, cx: &App) -> Facts {
        match self {
            Self::Line(state) => Facts::input(title, state, cx),
            Self::Area(state) => Facts {
                role: Role::TextInput,
                value: Some(state.read(cx).value().to_string().into()),
                focus: Some(state.focus_handle(cx)),
                ..Facts::button(title)
            },
        }
    }

    #[cfg(test)]
    fn record_for_test(&self, scope: &str, _cx: &App) {
        match self {
            Self::Line(state) => {
                TEXT_INPUTS.with(|inputs| {
                    inputs
                        .borrow_mut()
                        .insert(scope.to_owned(), state.downgrade());
                });
            }
            Self::Area(state) => {
                TEXTAREAS.with(|areas| {
                    areas
                        .borrow_mut()
                        .insert(scope.to_owned(), state.downgrade());
                });
            }
        }
    }
}

pub(super) struct TextField {
    pub(super) input: FieldInput,
    pub(super) kind: FieldKind,
    pub(super) committed: String,
    pub(super) sent: Option<String>,
    pub(super) error: Option<String>,
    pub(super) request: Option<u64>,
    pub(super) plugin: PluginId,
    pub(super) key: String,
    pub(super) kagan: WeakEntity<Kagan>,
    pub(super) _save: Subscription,
}

impl TextField {
    pub(super) fn dirty(&self, cx: &App) -> bool {
        let draft = self.input.value(cx);
        draft != self.committed || self.sent.is_some() || self.error.is_some()
    }

    pub(super) fn drop_pending(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.sent = None;
        self.request = None;
        cx.notify();
    }

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.value(cx);
        if value == self.committed && self.error.is_none() && self.sent.is_none() {
            return;
        }
        let (change, sent) = match self.kind {
            FieldKind::Text | FieldKind::Multiline => (Change::Set(json!(value.clone())), value),
            FieldKind::Number => match parse_number(&value) {
                Ok(number) => {
                    let sent = number.to_string();
                    if sent != value {
                        self.input.set_value(sent.clone(), window, cx);
                    }
                    (Change::Set(Value::Number(number)), sent)
                }
                Err(message) => {
                    self.error = Some(message);
                    self.sent = None;
                    cx.notify();
                    return;
                }
            },
        };
        self.error = None;
        self.sent = Some(sent);
        let plugin = self.plugin.clone();
        let key = self.key.clone();
        let kagan = self.kagan.clone();
        let baseline = self.committed.clone();
        let scope = format!("{}/{key}", plugin.0);
        let request = kagan.update(cx, |kagan, cx| {
            kagan
                .settings_drafts
                .begin(scope, plugin, key, change, Some(baseline), cx)
        });
        self.request = request.ok();
        cx.notify();
    }

    pub(super) fn finish(
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
        let Some(sent) = shown_value(written) else {
            return;
        };
        let draft = self.input.value(cx);
        self.sent = None;
        let baseline = baseline.unwrap_or("");
        if draft != sent {
            if result.is_ok() && self.committed == baseline {
                self.committed = sent;
            }
            cx.notify();
            return;
        }
        match result {
            Ok(_) => {
                if self.committed == baseline || self.committed == sent {
                    self.committed = sent;
                    self.error = None;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
}

fn parse_number(text: &str) -> Result<serde_json::Number, String> {
    let trimmed = text.trim();
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Number(number)) if number.as_f64().is_some_and(f64::is_finite) => Ok(number),
        _ => Err("Type a number.".into()),
    }
}

fn shown_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub(super) fn text_field(
    kagan: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    kind: FieldKind,
    text: String,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Entity<TextField> {
    let (plugin, setting, owner) = (id.clone(), key.to_owned(), kagan.clone());
    cx.new(move |cx| {
        let (input, save) = if kind == FieldKind::Multiline {
            let state = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(2, 6)
                    .default_value(text.clone())
            });
            let save =
                cx.subscribe_in(&state, window, move |_: &mut TextField, _, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                });
            (FieldInput::Area(state), save)
        } else {
            let state = cx.new(|cx| InputState::new(window, cx).default_value(text.clone()));
            let save = cx.subscribe_in(
                &state,
                window,
                move |field: &mut TextField, _, event, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => field.submit(window, cx),
                    InputEvent::Focus | InputEvent::Blur => {}
                },
            );
            (FieldInput::Line(state), save)
        };
        TextField {
            input,
            kind,
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

pub(super) fn sync_one(field: &Entity<TextField>, text: &str, window: &mut Window, cx: &mut App) {
    field.update(cx, |field, cx| {
        if field.committed == text && field.sent.is_none() {
            return;
        }
        let draft = field.input.value(cx);
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
            field.input.set_value(text.to_owned(), window, cx);
        }
    });
}

pub(super) fn revert(field: &Entity<TextField>, window: &mut Window, cx: &mut App) {
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
        field.input.set_value(committed, window, cx);
        cx.notify();
    });
}

#[cfg(test)]
thread_local! {
    static TEXT_INPUTS: std::cell::RefCell<BTreeMap<String, gpui_kit::WeakEntity<InputState>>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
    static TEXTAREAS: std::cell::RefCell<BTreeMap<String, gpui_kit::WeakEntity<TextareaState>>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

#[cfg(test)]
pub(crate) fn text_for_test(plugin: &PluginId, key: &str, cx: &App) -> String {
    let scope = format!("{}/{key}", plugin.0);
    let input = TEXT_INPUTS.with(|inputs| inputs.borrow().get(&scope).cloned());
    if let Some(text) = input
        .and_then(|input| input.upgrade())
        .map(|input| input.read(cx).value().to_string())
    {
        return text;
    }
    TEXTAREAS
        .with(|areas| areas.borrow().get(&scope).cloned())
        .and_then(|area| area.upgrade())
        .map_or_else(String::new, |area| area.read(cx).value().to_string())
}
