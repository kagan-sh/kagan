use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Context, DismissEvent, ElementId, Focusable as _, Role, Window};
use kg_core::Workspace;
use kg_domain::{Op, SurfaceOp};

use super::tabs::NewTerminal;
use super::{Fill, ToggleDetails, ToggleFill};
use crate::kagan::Kagan;
use crate::kagan::controls::control;
use crate::ui_registry::Facts;
impl Kagan {
    pub(in crate::kagan) fn thread_visible(&self, cx: &App) -> bool {
        self.shown_thread(cx).is_some() && !self.shows_archived() && self.screen.page().is_none()
    }

    pub(in crate::kagan) fn has_right_content(&self, cx: &App) -> bool {
        self.thread_visible(cx)
            || self.waiting_count(cx) > 0
            || !self.right.slots.is_empty()
            || !self.right.panels.is_empty()
    }

    pub(in crate::kagan) fn open_right_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.right.menu.is_some() {
            return;
        }
        let items: Vec<_> = self
            .closed_right_panels()
            .map(|item| (item.plugin.clone(), item.id.clone(), item.title.clone()))
            .collect();
        let (browser, terminal) = (self.can_open_browser(cx), self.can_open_terminal(cx));
        if items.is_empty() && !browser && !terminal {
            return;
        }
        let kagan = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let open =
                |id: &'static str,
                 title: &'static str,
                 add: fn(&mut Kagan, &mut Window, &mut Context<Kagan>)| {
                    let kagan = kagan.clone();
                    PopupMenuItem::element(move |_, _| {
                        control(
                            id,
                            Facts {
                                role: Role::MenuItem,
                                ..Facts::button(title)
                            },
                            title,
                        )
                    })
                    .on_click(move |_, window, cx| {
                        _ = kagan.update(cx, |this, cx| add(this, window, cx));
                    })
                };
            let menu = menu
                .when(browser, |menu| {
                    menu.item(open("right-add-browser", "Browser", Kagan::open_browser))
                })
                .when(terminal, |menu| {
                    menu.item(open(
                        "right-add-terminal",
                        "Terminal",
                        |this, window, cx| {
                            this.new_terminal(&NewTerminal, window, cx);
                        },
                    ))
                });
            items.into_iter().fold(menu, |menu, (plugin, id, title)| {
                let item_id = format!("right-add-{}-{id}", plugin.0);
                menu.item(
                    PopupMenuItem::element(move |_, _| {
                        control(
                            ElementId::Name(item_id.clone().into()),
                            Facts {
                                role: Role::MenuItem,
                                ..Facts::button(title.clone())
                            },
                            title.clone(),
                        )
                    })
                    .on_click(move |_, _, cx| {
                        Workspace::dispatch(
                            Op::Surface(SurfaceOp::Open {
                                plugin: plugin.clone(),
                                id: id.clone(),
                            }),
                            cx,
                        )
                        .detach();
                    }),
                )
            })
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _, cx| {
            this.right.menu = None;
            cx.notify();
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.right.menu = Some((menu, dismiss));
        cx.notify();
    }
}

impl Kagan {
    pub(in crate::kagan) fn toggle_fill_action(
        &mut self,
        _: &ToggleFill,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.right.fill = match self.right.fill {
            Fill::Column => Fill::Window,
            Fill::Window => Fill::Column,
        };
        cx.notify();
    }

    pub(in crate::kagan) fn toggle_details_action(
        &mut self,
        action: &ToggleDetails,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flip_slot_details(&action.panel);
        cx.notify();
    }
}
