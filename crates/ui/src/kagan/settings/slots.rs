use gpui_kit::App;
use gpui_kit::component::menu::PopupMenu;
use kg_domain::{Op, PluginId, PluginOp, Slot, SlotChoice};
use kg_plugins::PluginHost;

use super::Kagan;
use super::row::{Row, save};
use crate::kagan::controls::{choice_item, choice_trigger};

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
    let control = choice_trigger(&format!("slot-{}", slot.id()), shown, move |menu, _, _| {
        slot_menu(menu, slot, &chosen, &choices)
    });
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
    super::row::row(&format!("slot-{}", slot.id()), title, note, control)
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
            let item = choice_item(
                format!("slot-{}-{choice}", slot.id()),
                label.clone(),
                choice == chosen,
            );
            let pick = choice.clone();
            menu.item(item.on_click(move |_, _, cx| {
                // Letting a plugin draw kagan's parts is a person's choice, as in decision 105.
                if matches!(pick, SlotChoice::Plugin(_)) && crate::synthetic::refuse_for_person() {
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
            }))
        })
}
