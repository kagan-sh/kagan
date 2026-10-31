use crate::theme::TypeScale;
use crate::ui_observe::UiObserveExt;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
};
use gpui_shell::{
    ComponentArgument, ComponentDescriptor, ComponentMaterializer, ComponentPayload,
    ComponentRegistry, ConstructorDescriptor, MaterializeRequest, RegistryError, anyhow,
};
use kg_core::{Thread, Workspace};
use kg_domain::ThreadId;

#[derive(Clone, Copy)]
enum Part {
    Title,
    Status,
}

struct ThreadComponent {
    thread: Entity<Thread>,
    part: Part,
    _changes: Subscription,
}

impl Render for ThreadComponent {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let thread = self.thread.read(cx).record();
        match self.part {
            Part::Title => div()
                .id("thread-title")
                .ui_container()
                .text_color(cx.theme().foreground)
                .text_ellipsis()
                .child(thread.title.clone()),
            Part::Status => {
                let ended = Workspace::global(cx).read(cx).program_ended(&thread.id);
                let tone =
                    crate::theme::Tone::of(thread, self.thread.read(cx).busy(), ended, false);
                div()
                    .id("thread-status")
                    .ui_container()
                    .aria_label(format!("Thread status: {}", tone.word()))
                    .flex()
                    .items_center()
                    .gap_2()
                    .type_body()
                    .child(tone.mark(cx))
                    .child(tone.word())
            }
        }
    }
}

impl ComponentMaterializer for Part {
    fn materialize(
        &self,
        mut request: MaterializeRequest<'_>,
    ) -> anyhow::Result<gpui_kit::AnyElement> {
        let id = request
            .payload()
            .downcast_ref::<ThreadId>()
            .ok_or_else(|| {
                super::fail::ComponentError::new(
                    "Thread component received an incompatible thread id",
                )
            })?
            .clone();
        let key = request.element_id().clone();
        let part = *self;
        let view = request.with_window_app(|window, cx| {
            let thread = Workspace::global(cx)
                .read(cx)
                .thread(&id)
                .cloned()
                .ok_or_else(|| {
                    super::fail::ComponentError::new(format!("Thread {id} does not exist"))
                })?;
            let kind = match part {
                Part::Title => "title",
                Part::Status => "status",
            };
            let view = window
                .use_keyed_state(
                    gpui_kit::SharedString::from(format!("kagan-thread:{key:?}:{kind}:{}", id.0)),
                    cx,
                    |_, cx| ThreadComponent {
                        _changes: cx.observe(&thread, |_, _, cx| cx.notify()),
                        thread,
                        part,
                    },
                )
                .clone();
            Ok(view)
        })?;
        request.finish(div().child(view))
    }
}

pub(super) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    for (name, part, documentation) in [
        (
            "ThreadTitle",
            Part::Title,
            "A thread title, observed from core by thread id.",
        ),
        (
            "ThreadStatus",
            Part::Status,
            "A thread status bar and status word, observed from core in the active Workbench theme.",
        ),
    ] {
        registry.register(
            ComponentDescriptor::new(name, Arc::new(part))
                .with_constructors(vec![ConstructorDescriptor::new(
                    name,
                    vec![super::string("thread_id")],
                    |arguments| {
                        let [ComponentArgument::String(id)] = arguments else {
                            return Err("Thread component expects a thread id".into());
                        };
                        super::nonempty(id)?;
                        Ok(ComponentPayload::new(ThreadId(id.clone())))
                    },
                )])
                .with_documentation(documentation),
        )?;
    }
    Ok(())
}
