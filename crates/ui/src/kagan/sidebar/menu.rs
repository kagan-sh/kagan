use crate::kagan::controls::{control, menu_popup};
use crate::kagan::dispatch;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, DismissEvent, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, Styled as _, Window, div, px,
};
use kg_domain::{Op, PreferenceOp};
use serde_json::{Value, json};

use super::options::ThreadOrder;
use super::{ActivateRow, EnterRow, Sidebar};

impl Sidebar {
    fn open_list_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.list_menu.is_some() {
            return;
        }
        let options = self.options;
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            menu.item(choice(
                "sidebar-order-current",
                "Current order",
                options.order == ThreadOrder::Current,
                set(
                    kg_domain::prefs::SIDEBAR_ORDER,
                    json!(kg_domain::prefs::SIDEBAR_ORDER_CURRENT),
                ),
            ))
            .item(choice(
                "sidebar-order-attention",
                "Attention, newest first",
                options.order == ThreadOrder::Attention,
                set(
                    kg_domain::prefs::SIDEBAR_ORDER,
                    json!(kg_domain::prefs::SIDEBAR_ORDER_ATTENTION),
                ),
            ))
            .separator()
            .item(choice(
                "sidebar-hide-empty-projects",
                "Hide empty projects",
                options.hide_empty_projects,
                set(
                    kg_domain::prefs::SIDEBAR_HIDE_EMPTY_PROJECTS,
                    json!(!options.hide_empty_projects),
                ),
            ))
            .item(choice(
                "sidebar-unread-first",
                "Unread first",
                options.unread_first,
                set(
                    kg_domain::prefs::SIDEBAR_UNREAD_FIRST,
                    json!(!options.unread_first),
                ),
            ))
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, menu, _: &DismissEvent, window, cx| {
            this.list_menu = None;
            let kept = menu.read(cx).focus_handle(cx).contains_focused(window, cx);
            if kept || window.focused(cx).is_none() {
                window.focus(&this.list_focus, cx);
            }
            cx.notify();
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.list_menu = Some((menu, dismiss));
        cx.notify();
    }
}

fn set(key: &'static str, value: Value) -> Op {
    Op::Preference(PreferenceOp::Set {
        key: key.into(),
        value,
    })
}

fn choice(id: &'static str, label: &'static str, on: bool, op: Op) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        control(
            id,
            Facts {
                role: Role::MenuItem,
                selected: Some(on),
                ..Facts::button(label)
            },
            label,
        )
    })
    .checked(on)
    .on_click(move |_, _, cx| {
        dispatch::logged(op.clone(), cx);
    })
}

pub(super) fn button(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let focus = sidebar.list_focus.clone();
    let menu = sidebar.list_menu.as_ref().map(|(menu, _)| menu.clone());
    div()
        .id("sidebar-list-options")
        .relative()
        .flex_shrink_0()
        .key_context("SidebarRow")
        .track_focus(&focus)
        .ui_observe_as(Facts {
            focus: Some(focus.clone()),
            selected: Some(menu.is_some()),
            ..Facts::button("List options")
        })
        .on_action(cx.listener(|this, _: &ActivateRow, window, cx| {
            this.open_list_menu(window, cx);
        }))
        .on_action(cx.listener(|this, _: &EnterRow, window, cx| {
            this.open_list_menu(window, cx);
        }))
        .child(
            Button::new("sidebar-list-options-button")
                .small()
                .rounded(px(crate::theme::radius_xs(cx)))
                .ghost()
                .selected(menu.is_some())
                .icon(gpui_kit::assets::IconName::ListSortAscending)
                .accessibility_label("List options")
                .tooltip("List options")
                .tab_stop(false)
                .on_click(cx.listener(|this, _, window, cx| this.open_list_menu(window, cx))),
        )
        .when_some(menu, |button, menu| {
            button.child(menu_popup(
                &menu,
                "sidebar-list-layer",
                "sidebar-list-menu",
                "List options",
            ))
        })
}
