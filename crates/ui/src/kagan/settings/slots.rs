use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{App, ElementId, IntoElement, ParentElement as _, Role, Styled as _};
use kg_domain::{Op, PluginId, PluginOp, Slot, SlotChoice};
use kg_plugins::PluginHost;

use super::Kagan;
use super::row::{Row, save};
use crate::kagan::controls::control;
use crate::ui_registry::Facts;

/// Built-in, or each enabled plugin that offers to fill `slot`. Only this choice fills it.
pub(super) fn row(kagan: &Kagan, slot: Slot, cx: &App) -> Row {
    let chosen = kagan
        .replaced
        .chosen(slot)
        .map_or(SlotChoice::Kagan, |id| SlotChoice::Plugin(id.clone()));
    let names: Vec<(PluginId, String)> = PluginHost::try_global(cx)
        .map(|host| {
            host.read(cx)
                .status()
                .into_iter()
                .map(|status| (status.id.clone(), status.name.as_str().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let offers = kg_plugins::contributions(cx)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, ui)| ui.replacement(slot).is_some())
        .map(|(id, _)| {
            let name = names
                .iter()
                .find(|(named, _)| *named == id)
                .map_or_else(|| id.0.clone(), |(_, name)| name.clone());
            (SlotChoice::Plugin(id), name)
        });
    let choices = std::iter::once((SlotChoice::Kagan, "Built-in".to_owned())).chain(offers);
    let control = h_flex()
        .gap_1()
        .flex_wrap()
        .children(choices.map(|(choice, label)| {
            let selected = choice == chosen;
            let key = format!("{}-{choice}", slot.id());
            let button = Button::new(ElementId::Name(format!("button-slot-{key}").into()))
                .small()
                .label(label.clone())
                .on_click(move |_, _, cx| {
                    // Letting a plugin draw kagan's parts is a person's choice, as in decision 105.
                    if matches!(choice, SlotChoice::Plugin(_))
                        && crate::synthetic::refuse_for_person()
                    {
                        return;
                    }
                    save(
                        Op::Plugin(PluginOp::Use {
                            slot,
                            id: choice.clone(),
                            thread_id: None,
                        }),
                        cx,
                    );
                });
            control(
                format!("slot-{key}"),
                Facts {
                    role: Role::RadioButton,
                    selected: Some(selected),
                    ..Facts::button(label)
                },
                if selected {
                    button.primary()
                } else {
                    button.outline()
                },
            )
        }));
    let (title, note) = match slot {
        Slot::Threads => (
            "Sidebar",
            "A plugin can draw the thread list. The rail, questions and counts stay kagan’s.",
        ),
        Slot::Review => (
            "Review",
            "A plugin can draw the changes of a review. Kagan’s footer still sends and discards.",
        ),
    };
    super::row::row(
        &format!("slot-{}", slot.id()),
        title,
        note,
        control.into_any_element(),
    )
}
