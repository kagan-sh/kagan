//! `KgInput` and `KgTextarea`: text fields a script builds, observed by `kg ui`.
//!
//! The shell's `Input` and `Textarea` are built where this crate cannot wrap them or replace
//! their names. These take an accessible name and state the field's name, value and focus.

use std::sync::Arc;

use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, ElementId, Entity, FocusHandle, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Refineable as _, Role, SharedString, StyleRefinement,
    Styled as _, div,
};
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, ComponentArgument, ComponentDescriptor,
    ComponentMaterializer, ComponentPayload, ComponentRegistry, ConstructorDescriptor,
    MaterializeRequest, MethodDescriptor, RegistryError, anyhow,
};

pub(super) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    registry.register(descriptor(
        "KgInput",
        "InputState",
        "A single-line text field kagan can find, click and type into through kg ui. aria_label is its accessible name. Without one, the field is not a control.",
        Arc::new(KgInput),
    ))?;
    registry.register(descriptor(
        "KgTextarea",
        "TextareaState",
        "A multi-line text field kagan can find, click and type into through kg ui. aria_label is its accessible name. Without one, the field is not a control.",
        Arc::new(KgTextarea),
    ))?;
    Ok(())
}

fn descriptor(
    name: &'static str,
    state: &'static str,
    documentation: &'static str,
    materializer: Arc<dyn ComponentMaterializer>,
) -> ComponentDescriptor {
    ComponentDescriptor::new(name, materializer)
        .with_constructors(vec![ConstructorDescriptor::new(
            name,
            vec![ArgumentDescriptor::new("state", ArgumentSchema::Entity(state))],
            move |arguments| match arguments {
                [argument @ ComponentArgument::Entity { .. }] => {
                    Ok(ComponentPayload::new(argument.clone()))
                }
                _ => Err(format!("{name} expects one {state}")),
            },
        )])
        .with_methods(vec![
            MethodDescriptor::new(
                "aria_label",
                vec![ArgumentDescriptor::new("label", ArgumentSchema::String)],
                move |arguments| match arguments {
                    [ComponentArgument::String(value)] if !value.trim().is_empty() => {
                        Ok(ComponentPayload::new(value.clone()))
                    }
                    _ => Err(format!("{name}.aria_label expects non-empty text")),
                },
            )
            .with_documentation(
                "Sets the accessible name. kg ui finds, clicks and types into the field by this name, and reads its value and focus.",
            ),
            MethodDescriptor::new(
                "disabled",
                vec![ArgumentDescriptor::new("disabled", ArgumentSchema::Boolean)],
                |_| Ok(ComponentPayload::new(())),
            )
            .with_documentation("Stops the field accepting edits."),
        ])
        .with_documentation(documentation)
}

struct KgInput;

impl ComponentMaterializer for KgInput {
    fn materialize(&self, mut request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let argument = argument(&request, "KgInput")?;
        let state = request.with_state::<Entity<InputState>, _>(&argument, Clone::clone)?;
        let label = accessible_name(&request);
        let disabled = request.disabled();
        anyhow::ensure!(
            request.children_len() == 0,
            "KgInput does not accept children"
        );
        let style = request.take_style();
        request.with_window_app(move |_, cx| {
            let current = state.read(cx);
            let masked = current.presentation().is_masked();
            let value = current.value();
            let focus = state.focus_handle(cx);
            let input = Input::new(&state)
                .disabled(disabled)
                .when_some(label.clone(), Input::aria_label);
            Ok(paint_field(
                &style, label, disabled, &value, masked, focus, input,
            ))
        })
    }
}

struct KgTextarea;

impl ComponentMaterializer for KgTextarea {
    fn materialize(&self, mut request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let argument = argument(&request, "KgTextarea")?;
        let state = request.with_state::<Entity<TextareaState>, _>(&argument, Clone::clone)?;
        let label = accessible_name(&request);
        let disabled = request.disabled();
        anyhow::ensure!(
            request.children_len() == 0,
            "KgTextarea does not accept children"
        );
        let style = request.take_style();
        request.with_window_app(move |_, cx| {
            let current = state.read(cx);
            let masked = current.presentation().is_masked();
            let value = current.value();
            let focus = state.focus_handle(cx);
            let input = Textarea::new(&state)
                .disabled(disabled)
                .when_some(label.clone(), Textarea::aria_label);
            Ok(paint_field(
                &style, label, disabled, &value, masked, focus, input,
            ))
        })
    }
}

fn argument(request: &MaterializeRequest<'_>, name: &str) -> anyhow::Result<ComponentArgument> {
    request
        .payload()
        .downcast_ref::<ComponentArgument>()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{name} received an incompatible payload"))
}

fn accessible_name(request: &MaterializeRequest<'_>) -> Option<String> {
    let mut name = None;
    for method in request.methods() {
        if let Some(label) = method.payload().downcast_ref::<String>() {
            name = Some(label.clone());
        }
    }
    name
}

fn paint_field(
    style: &StyleRefinement,
    label: Option<String>,
    disabled: bool,
    value: &SharedString,
    masked: bool,
    focus: FocusHandle,
    child: impl IntoElement,
) -> AnyElement {
    let shown = if masked && !value.is_empty() {
        "••••••••".to_owned()
    } else {
        value.to_string()
    };
    let child = child.into_any_element();
    if let Some(name) = label {
        let facts = Facts {
            role: Role::TextInput,
            name: SharedString::from(name.clone()),
            value: Some(SharedString::from(shown)),
            disabled,
            selected: None,
            focus: Some(focus),
            hint: None,
        };
        let mut row = div()
            .id(ElementId::Name(SharedString::from(name)))
            .size_full()
            .ui_observe_as(facts)
            .child(child);
        row.style().refine(style);
        row.into_any_element()
    } else {
        let mut row = div().size_full().child(child);
        row.style().refine(style);
        row.into_any_element()
    }
}
