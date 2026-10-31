#[macro_use]
mod fail;
mod button;
mod sketch;
#[cfg(test)]
mod tests;
mod thread;

use std::sync::Arc;

use gpui_kit::component::text::TextView;
use gpui_kit::{AnyElement, ParentElement as _, div};
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, ComponentArgument, ComponentDescriptor,
    ComponentMaterializer, ComponentPayload, ComponentRegistry, ConstructorDescriptor,
    MaterializeRequest, RegistryError, anyhow,
};

pub(crate) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    button::register(registry)?;
    sketch::register(registry)?;
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
            .ok_or_else(|| fail::ComponentError::new("Markdown received incompatible text data"))?;
        let element = div().child(TextView::markdown(id.clone(), text.clone()));
        request.finish(element)
    }
}
