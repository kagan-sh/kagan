mod button;
mod component_error;
mod field;
#[cfg(test)]
mod tests;
mod thread;
mod webview;

use std::sync::Arc;

use gpui_kit::component::text::TextView;
use gpui_kit::{AnyElement, ParentElement as _, div};
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, ComponentArgument, ComponentDescriptor,
    ComponentMaterializer, ComponentPayload, ComponentRegistry, ConstructorDescriptor,
    MaterializeRequest, RegistryError, anyhow,
};

#[cfg(not(desktop))]
pub(crate) use webview::UNAVAILABLE;
#[cfg(test)]
pub(crate) use webview::{REFUSED_HOST, native_shown};
pub(crate) use webview::{drop_stores, fills_panel, sync_pages};

pub(crate) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    button::register(registry)?;
    field::register(registry)?;
    webview::register(registry)?;
    thread::register(registry)?;
    registry.register(
        ComponentDescriptor::new("Markdown", Arc::new(Markdown))
            .with_constructors(vec![ConstructorDescriptor::new(
                "Markdown",
                vec![string("id"), string("text")],
                |arguments| {
                    let [
                        ComponentArgument::String(id),
                        ComponentArgument::String(text),
                    ] = arguments
                    else {
                        return Err("new Markdown(id, text) expects two strings".into());
                    };
                    nonempty(id)?;
                    Ok(ComponentPayload::new((id.clone(), text.clone())))
                },
            )])
            .with_documentation("Rendered Markdown message text in the active Workbench theme."),
    )?;
    Ok(())
}

fn string(name: &'static str) -> ArgumentDescriptor {
    ArgumentDescriptor::new(name, ArgumentSchema::String)
}

fn nonempty(id: &str) -> Result<(), String> {
    if id.is_empty() {
        Err("Component id must not be empty".into())
    } else {
        Ok(())
    }
}

struct Markdown;

impl ComponentMaterializer for Markdown {
    fn materialize(&self, request: MaterializeRequest<'_>) -> anyhow::Result<AnyElement> {
        let (id, text) = request
            .payload()
            .downcast_ref::<(String, String)>()
            .ok_or_else(|| {
                component_error::ComponentError::new("Markdown received incompatible text data")
            })?;
        let element = div().child(TextView::markdown(id.clone(), text.clone()));
        request.finish(element)
    }
}
