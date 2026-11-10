use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Role, div,
};
use kg_domain::{Op, PluginId, PluginOp, Slot, SlotChoice};
use kg_plugins::PluginHost;

use super::Kagan;
use super::row::{Row, save};
use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn row(kagan: &Kagan, slot: Slot, cx: &App) -> Row {
    let chosen = kagan
        .slots
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
    let choices: Vec<(SlotChoice, String)> =
        std::iter::once((SlotChoice::Kagan, "Built-in".to_owned()))
            .chain(offers)
            .collect();
    let shown = choices
        .iter()
        .find(|(choice, _)| choice == &chosen)
        .map_or_else(|| chosen.to_string(), |(_, name)| name.clone());
    let id = format!("slot-{}", slot.id());
    let control = control(
        id.clone(),
        Facts::button(shown.clone()),
        Button::new(ElementId::Name(format!("button-{id}").into()))
            .small()
            .outline()
            .label(shown)
            .dropdown_menu(move |menu, _, _| slot_menu(menu, slot, &chosen, &choices)),
    );
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

fn slot_menu(
    menu: PopupMenu,
    slot: Slot,
    chosen: &SlotChoice,
    choices: &[(SlotChoice, String)],
) -> PopupMenu {
    choices
        .iter()
        .fold(menu.scrollable(true), |menu, (choice, label)| {
            let selected = choice == chosen;
            let item_id = format!("slot-{}-{choice}", slot.id());
            let name = label.clone();
            let pick = choice.clone();
            menu.item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .id(ElementId::Name(item_id.clone().into()))
                        .ui_observe_as(Facts {
                            role: Role::MenuItem,
                            selected: Some(selected),
                            ..Facts::button(name.clone())
                        })
                        .child(name.clone())
                })
                .checked(selected)
                .on_click(move |_, _, cx| {
                    // Letting a plugin draw kagan's parts is a person's choice, as in decision 105.
                    if matches!(pick, SlotChoice::Plugin(_))
                        && crate::synthetic::refuse_for_person()
                    {
                        return;
                    }
                    save(
                        Op::Plugin(PluginOp::Use {
                            slot,
                            id: pick.clone(),
                            thread_id: None,
                        }),
                        cx,
                    );
                }),
            )
        })
}
