//! The chip that names the agent's mode, and the menu that changes its modes and options.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, Styled as _, WeakEntity, div,
};
use kg_domain::{ModeRisk, ThreadOption, ThreadOptionValue};

use super::ChatView;
use super::composer::Shown;
use crate::kagan::controls::control;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

// The agent's own word for the mode it runs in, first in the bar. The menu holds every option the
// agent offered, each value with the agent's description and a check on the one in force.
pub(super) fn chip(
    view: &ChatView,
    shown: &Shown,
    cx: &mut Context<ChatView>,
) -> Option<AnyElement> {
    if !shown.chip() {
        return None;
    }
    let mode = view
        .transcript
        .options
        .iter()
        .find(|option| option.is_mode())?;
    let current = mode.current()?;
    let risky = ModeRisk::of(&current.value, &current.name);
    let label = SharedString::from(current.name.clone());
    let tooltip = match &current.description {
        Some(description) => format!("Mode: {}. {description}", current.name),
        None => format!("Mode: {}", current.name),
    };
    let mut button = Button::new("chat-mode-button")
        .small()
        .label(label.clone())
        .accessibility_label(tooltip.clone())
        .tooltip(tooltip)
        .when(risky, |button| button.icon(IconName::TriangleAlert));
    button = if risky {
        button.custom(ButtonCustomVariant::new(cx).foreground(cx.theme().warning))
    } else {
        button.ghost()
    };
    let options = view.transcript.options.clone();
    let weak = cx.weak_entity();
    Some(
        control(
            "chat-mode",
            Facts {
                value: Some(label.clone()),
                ..Facts::button(label)
            },
            button.dropdown_menu(move |menu, _, cx| options_menu(menu, &options, &weak, cx)),
        )
        .into_any_element(),
    )
}

fn options_menu(
    menu: PopupMenu,
    options: &[ThreadOption],
    weak: &WeakEntity<ChatView>,
    cx: &App,
) -> PopupMenu {
    // An agent may offer a great many values; the menu scrolls rather than run off the window.
    let mut menu = menu.scrollable(true);
    for (index, option) in options.iter().enumerate() {
        if index > 0 {
            menu = menu.separator();
        }
        menu = menu.item(group_title(&option.name));
        for value in &option.values {
            let chosen = value.value == option.value;
            let id = format!("chat-mode-{}-{}", option.id, value.value);
            let weak = weak.clone();
            let chosen_option = option.id.clone();
            let chosen_value = value.value.clone();
            menu = menu.item(
                value_item(&id, value, chosen, cx).on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| {
                        this.choose(chosen_option.clone(), chosen_value.clone(), cx);
                    })
                    .ok();
                }),
            );
        }
    }
    menu
}

// What a group is called: the agent's own name for the option, as a line above its values.
fn group_title(name: &str) -> PopupMenuItem {
    let name = name.to_owned();
    let id = format!("chat-mode-{name}");
    PopupMenuItem::element(move |_, cx| {
        div()
            .id(ElementId::Name(id.clone().into()))
            .ui_observe_as(Facts::label(name.clone(), name.clone()))
            .type_caption(cx)
            .text_color(cx.theme().muted_foreground)
            .child(name.clone())
    })
    .disabled(true)
}

fn value_item(id: &str, value: &ThreadOptionValue, chosen: bool, cx: &App) -> PopupMenuItem {
    let name = value.name.clone();
    let risky = ModeRisk::of(&value.value, &value.name);
    let color = if risky {
        cx.theme().warning
    } else {
        cx.theme().foreground
    };
    let description = value.description.clone();
    let id = id.to_owned();
    PopupMenuItem::element(move |_, cx| {
        let muted = cx.theme().muted_foreground;
        let description = description.clone();
        div()
            .id(ElementId::Name(id.clone().into()))
            .ui_observe_as(Facts {
                role: Role::MenuItem,
                selected: Some(chosen),
                ..Facts::button(name.clone())
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .when(risky, |row| {
                        row.child(
                            div()
                                .id(ElementId::Name(format!("{id}-risk").into()))
                                .ui_observe_as(Facts::label("Risk", "asks nothing"))
                                .child(
                                    Icon::new(IconName::TriangleAlert)
                                        .xsmall()
                                        .text_color(color),
                                ),
                        )
                    })
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(color)
                            .child(name.clone()),
                    ),
            )
            .when_some(description, |row, description| {
                row.child(div().type_caption(cx).text_color(muted).child(description))
            })
    })
    .checked(chosen)
}
