use crate::kagan::dispatch;
use gpui_kit::component::Selectable as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, Context, DismissEvent, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, Window, anchored,
    deferred, div, px,
};
use kg_domain::{Op, SettingsOp, WindowOp};

use super::{ActivateRow, EnterRow, Sidebar};
use crate::kagan::controls::{control, menu_item};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl Sidebar {
    fn open_brand_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.brand_menu.is_some() {
            return;
        }
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            menu.item(item(
                "kagan-about",
                "About kagan",
                Op::Window(WindowOp::About),
            ))
            .item(item(
                "kagan-settings",
                "Settings…",
                Op::Settings(SettingsOp::Open),
            ))
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, menu, _: &DismissEvent, window, cx| {
            this.brand_menu = None;
            let kept = menu.read(cx).focus_handle(cx).contains_focused(window, cx);
            if kept || window.focused(cx).is_none() {
                window.focus(&this.brand_focus, cx);
            }
            cx.notify();
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.brand_menu = Some((menu, dismiss));
        cx.notify();
    }
}

fn item(id: &'static str, label: &'static str, op: Op) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| menu_item(id, label)).on_click(move |_, _, cx| {
        dispatch::logged(op.clone(), cx);
    })
}

pub(super) fn button(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let focus = sidebar.brand_focus.clone();
    let menu = sidebar.brand_menu.as_ref().map(|(menu, _)| menu.clone());
    div()
        .id("kagan-menu")
        .relative()
        .flex_shrink_0()
        .key_context("SidebarRow")
        .track_focus(&focus)
        .ui_observe_as(Facts {
            focus: Some(focus.clone()),
            hint: Some("kagan".into()),
            ..Facts::button("kagan")
        })
        .on_action(cx.listener(|this, _: &ActivateRow, window, cx| {
            this.open_brand_menu(window, cx);
        }))
        .on_action(cx.listener(|this, _: &EnterRow, window, cx| {
            this.open_brand_menu(window, cx);
        }))
        .child(
            Button::new("kagan-menu-button")
                .small()
                .rounded(px(crate::theme::radius_xs(cx)))
                .ghost()
                .selected(menu.is_some())
                .icon(crate::brand::mark_icon())
                .accessibility_label("kagan")
                .tooltip("kagan")
                .tab_stop(false)
                .on_click(cx.listener(|this, _, window, cx| this.open_brand_menu(window, cx))),
        )
        .when_some(menu, |button, menu| button.child(popup(menu)))
}

// The menu opens below the button and to its right, because the rail is the window's left edge.
fn popup(menu: Entity<PopupMenu>) -> impl IntoElement {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .size_0()
        .child(deferred(
            anchored().anchor(Anchor::TopLeft).child(
                div()
                    .id("kagan-menu-layer")
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(control(
                        "kagan-menu-popup",
                        Facts {
                            role: Role::Menu,
                            ..Facts::button("kagan")
                        },
                        menu,
                    )),
            ),
        ))
}
