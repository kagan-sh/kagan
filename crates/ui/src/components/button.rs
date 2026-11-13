use std::sync::Arc;

use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _, Size};
use gpui_kit::{
    AnyElement, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, div,
};
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, ComponentArgument, ComponentDescriptor,
    ComponentMaterializer, ComponentPayload, ComponentRegistry, ConstructorDescriptor,
    MaterializeRequest, MethodDescriptor, RegistryError, anyhow,
};

use super::component_error::ComponentError;
use super::{Set, apply, plain_method};

pub(super) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    registry
        .register(
            ComponentDescriptor::new("KgButton", Arc::new(KgButton))
                .with_constructors(vec![ConstructorDescriptor::new(
                    "KgButton",
                    vec![super::string("id")],
                    |arguments| {
                        let [ComponentArgument::String(id)] = arguments else {
                            return Err("new KgButton(id) expects one string".into());
                        };
                        super::nonempty(id)?;
                        Ok(ComponentPayload::new(id.clone()))
                    },
                )])
                .with_methods(vec![
                    text_method("label", "Sets the visible button label.", |props, text| props.label = text),
                    text_method("tooltip", "Sets concise hover help.", |props, text| props.tooltip = Some(text)),
                    text_method(
                        "icon",
                        "Sets the icon by its asset path, such as icons/menu.svg. The button is then icon-only: its label is its accessible name and tooltip.",
                        |props, path| props.icon = Some(path),
                    ),
                    flag_method("loading", "Sets the loading presentation.", |props, flag| props.loading = flag),
                    size_method(),
                    colour_method("primary", Button::primary, "Uses the primary action variant."),
                    colour_method("secondary", Button::secondary, "Uses the secondary variant."),
                    colour_method("danger", Button::danger, "Uses the destructive-action variant."),
                    colour_method("success", Button::success, "Uses the success variant."),
                    colour_method("warning", Button::warning, "Uses the warning variant."),
                    colour_method("ghost", Button::ghost, "Uses the quiet ghost variant."),
                    colour_method("link", Button::link, "Uses the link-like visual variant."),
                    plain_method::<Props>("outline", |props| props.outline = true, "Uses the outline presentation."),
                    plain_method::<Props>("compact", |props| props.compact = true, "Uses compact internal spacing."),
                    // The shell records these itself and reads them through the request.
                    shell_call("disabled", ArgumentSchema::Boolean, "Stops the button responding to clicks."),
                    shell_call("selected", ArgumentSchema::Boolean, "Marks the button as the current choice."),
                    shell_call(
                        "on_click",
                        ArgumentSchema::Callback("(cx: Context) => void"),
                        "Runs when the button is clicked.",
                    ),
                ])
                .with_documentation(
                    "A command button kagan can find, click and check through kg ui. It takes the calls of Button: label, tooltip, loading, size, the colour variants, outline, compact, disabled, selected, on_click, and children.",
                ),
        )
        .map(|_| ())
}

type Prop = Set<Props>;

fn payload(set: impl Fn(&mut Props) + Send + Sync + 'static) -> ComponentPayload {
    let prop: Prop = Arc::new(set);
    ComponentPayload::new(prop)
}

fn text_method(
    name: &'static str,
    documentation: &'static str,
    set: fn(&mut Props, String),
) -> MethodDescriptor {
    MethodDescriptor::new(
        name,
        vec![ArgumentDescriptor::new(name, ArgumentSchema::String)],
        move |arguments| match arguments {
            [ComponentArgument::String(value)] => {
                let value = value.clone();
                Ok(payload(move |props| set(props, value.clone())))
            }
            _ => Err(format!("KgButton.{name} expects one string")),
        },
    )
    .with_documentation(documentation)
}

fn flag_method(
    name: &'static str,
    documentation: &'static str,
    set: fn(&mut Props, bool),
) -> MethodDescriptor {
    MethodDescriptor::new(
        name,
        vec![ArgumentDescriptor::new(name, ArgumentSchema::Boolean)],
        move |arguments| match arguments {
            [ComponentArgument::Boolean(flag)] => {
                let flag = *flag;
                Ok(payload(move |props| set(props, flag)))
            }
            _ => Err(format!("KgButton.{name} expects one boolean")),
        },
    )
    .with_documentation(documentation)
}

fn size_method() -> MethodDescriptor {
    MethodDescriptor::new(
        "size",
        vec![ArgumentDescriptor::new(
            "size",
            ArgumentSchema::Enum(&["xsmall", "small", "medium", "large"]),
        )],
        |arguments| match arguments {
            [ComponentArgument::Enum(value)] => {
                let size = match value.as_str() {
                    "xsmall" => Size::XSmall,
                    "small" => Size::Small,
                    "medium" => Size::Medium,
                    "large" => Size::Large,
                    _ => return Err(format!("unsupported size `{value}`")),
                };
                Ok(payload(move |props| props.size = Some(size)))
            }
            _ => Err("KgButton.size expects a size literal".into()),
        },
    )
    .with_documentation("Sets the size.")
}

fn colour_method(
    name: &'static str,
    colour: fn(Button) -> Button,
    documentation: &'static str,
) -> MethodDescriptor {
    plain_method::<Props>(
        name,
        move |props| props.colour = Some(colour),
        documentation,
    )
}

// A call the shell records itself on every component. The declaration lets a script make the
// call; the materializer reads the shell's record, not this payload.
fn shell_call(
    name: &'static str,
    schema: ArgumentSchema,
    documentation: &'static str,
) -> MethodDescriptor {
    MethodDescriptor::new(
        name,
        vec![ArgumentDescriptor::new(name, schema)],
        move |_| Ok(ComponentPayload::new(())),
    )
    .with_documentation(documentation)
}

struct KgButton;

impl ComponentMaterializer for KgButton {
    fn materialize(&self, mut request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let id = request
            .payload()
            .downcast_ref::<String>()
            .ok_or_else(|| ComponentError::new("KgButton received an incompatible id"))?
            .clone();
        let props: Props = apply(&request);
        let disabled = request.disabled();
        let selected = request.selected();
        let click = request.on_click();
        let children = request.take_children()?;
        let key = ElementId::Name(SharedString::from(id.clone()));
        let focus = request.with_window_app(|window, cx| {
            Ok(window
                .use_keyed_state(
                    SharedString::from(format!("kg-button-focus:{id}")),
                    cx,
                    |_, cx| cx.focus_handle(),
                )
                .read(cx)
                .clone())
        })?;
        let mut button = Button::new(SharedString::from(format!("{id}.button")))
            .disabled(disabled)
            .selected(selected)
            .track_focus(&focus);
        button = match &props.icon {
            Some(path) => button
                .icon(Icon::empty().path(path.clone()))
                .accessibility_label(props.label.clone()),
            None => button.label(props.label.clone()),
        };
        if let Some(colour) = props.colour {
            button = colour(button);
        }
        if props.outline {
            button = button.outline();
        }
        if props.compact {
            button = button.compact();
        }
        if let Some(size) = props.size {
            button = button.with_size(size);
        }
        button = button.loading(props.loading);
        match (props.tooltip, props.icon.is_some()) {
            (Some(tooltip), _) => button = button.tooltip(tooltip),
            (None, true) => button = button.tooltip(props.label.clone()),
            (None, false) => {}
        }
        if let Some(click) = click {
            button = button.on_click(move |event, window, cx| click.invoke(event, window, cx));
        }
        button.extend(children);
        let facts = Facts {
            hint: None,
            role: Role::Button,
            name: SharedString::from(props.label),
            value: None,
            disabled,
            selected: Some(selected),
            focus: Some(focus.clone()),
        };
        request.finish(
            div()
                .id(key)
                .ui_observe_as(facts)
                .child(button.into_element()),
        )
    }
}

#[derive(Default)]
struct Props {
    label: String,
    icon: Option<String>,
    tooltip: Option<String>,
    loading: bool,
    size: Option<Size>,
    colour: Option<fn(Button) -> Button>,
    outline: bool,
    compact: bool,
}
